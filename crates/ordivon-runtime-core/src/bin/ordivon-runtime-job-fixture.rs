use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Root,
    Child,
    Grandchild,
}

impl Role {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "root" => Ok(Self::Root),
            "child" => Ok(Self::Child),
            "grandchild" => Ok(Self::Grandchild),
            _ => Err(format!("invalid role: {value}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Child => "child",
            Self::Grandchild => "grandchild",
        }
    }
}

#[derive(Debug)]
struct Config {
    role: Role,
    marker_dir: PathBuf,
    hold_ms: u64,
    ignore_term: bool,
    detach_session: bool,
    spawn_descendant: bool,
    exit_code: u8,
    stdout_lines: u32,
    stderr_lines: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    role: String,
    pid: u32,
    parent_pid: i32,
    process_group_id: i32,
    session_id: i32,
    process_start_ticks: u64,
    cgroup: String,
    written_at_unix_ms: u128,
    ignored_sigterm: bool,
    detached_session: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("fixture error: {error}");
            ExitCode::from(64)
        }
    }
}

fn run() -> Result<u8, String> {
    let config = parse_args(env::args().skip(1))?;
    fs::create_dir_all(&config.marker_dir).map_err(|error| error.to_string())?;

    if config.detach_session {
        let result = unsafe { libc::setsid() };
        if result < 0 {
            return Err(io::Error::last_os_error().to_string());
        }
    }
    if config.ignore_term {
        let previous = unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
        if previous == libc::SIG_ERR {
            return Err(io::Error::last_os_error().to_string());
        }
    }

    write_marker(&config)?;
    emit_lines("stdout", config.role, config.stdout_lines, false)?;
    emit_lines("stderr", config.role, config.stderr_lines, true)?;

    let mut descendant = if config.spawn_descendant {
        Some(spawn_descendant(&config)?)
    } else {
        None
    };

    println!(
        "fixture_ready role={} pid={} markerDir={}",
        config.role.as_str(),
        std::process::id(),
        config.marker_dir.display()
    );
    io::stdout().flush().map_err(|error| error.to_string())?;

    thread::sleep(Duration::from_millis(config.hold_ms));
    if let Some(child) = descendant.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(config.exit_code)
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut role = Role::Root;
    let mut marker_dir = None;
    let mut hold_ms = 300_000;
    let mut ignore_term = false;
    let mut detach_session = false;
    let mut spawn_descendant = false;
    let mut exit_code = 0;
    let mut stdout_lines = 0;
    let mut stderr_lines = 0;
    let values: Vec<String> = args.collect();
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--role" => role = Role::parse(next_value(&values, &mut index, "--role")?)?,
            "--marker-dir" => {
                marker_dir = Some(PathBuf::from(next_value(
                    &values,
                    &mut index,
                    "--marker-dir",
                )?))
            }
            "--hold-ms" => hold_ms = parse_number(next_value(&values, &mut index, "--hold-ms")?)?,
            "--exit-code" => {
                exit_code = parse_number(next_value(&values, &mut index, "--exit-code")?)?
            }
            "--stdout-lines" => {
                stdout_lines = parse_number(next_value(&values, &mut index, "--stdout-lines")?)?
            }
            "--stderr-lines" => {
                stderr_lines = parse_number(next_value(&values, &mut index, "--stderr-lines")?)?
            }
            "--ignore-term" => ignore_term = true,
            "--detach-session" => detach_session = true,
            "--spawn-descendant" => spawn_descendant = true,
            unknown => return Err(format!("unknown argument: {unknown}")),
        }
        index += 1;
    }
    let marker_dir = marker_dir.ok_or_else(|| "--marker-dir is required".to_string())?;
    if !marker_dir.is_absolute() {
        return Err("--marker-dir must be absolute".to_string());
    }
    Ok(Config {
        role,
        marker_dir,
        hold_ms,
        ignore_term,
        detach_session,
        spawn_descendant,
        exit_code,
        stdout_lines,
        stderr_lines,
    })
}

fn next_value<'a>(values: &'a [String], index: &mut usize, name: &str) -> Result<&'a str, String> {
    *index += 1;
    values
        .get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} requires a value"))
}

fn parse_number<T>(value: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|error| error.to_string())
}

fn spawn_descendant(config: &Config) -> Result<Child, String> {
    let next_role = match config.role {
        Role::Root => Role::Child,
        Role::Child => Role::Grandchild,
        Role::Grandchild => return Err("grandchild cannot spawn another descendant".to_string()),
    };
    let mut command = Command::new(env::current_exe().map_err(|error| error.to_string())?);
    command
        .arg("--role")
        .arg(next_role.as_str())
        .arg("--marker-dir")
        .arg(&config.marker_dir)
        .arg("--hold-ms")
        .arg(config.hold_ms.to_string())
        .arg("--ignore-term");
    if matches!(next_role, Role::Child) {
        command.arg("--detach-session").arg("--spawn-descendant");
    }
    command.spawn().map_err(|error| error.to_string())
}

fn write_marker(config: &Config) -> Result<(), String> {
    let marker = Marker {
        role: config.role.as_str().to_string(),
        pid: std::process::id(),
        parent_pid: unsafe { libc::getppid() },
        process_group_id: unsafe { libc::getpgid(0) },
        session_id: unsafe { libc::getsid(0) },
        process_start_ticks: process_start_ticks()?,
        cgroup: current_cgroup()?,
        written_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
        ignored_sigterm: config.ignore_term,
        detached_session: config.detach_session,
    };
    let bytes = serde_json::to_vec_pretty(&marker).map_err(|error| error.to_string())?;
    let final_path = config
        .marker_dir
        .join(format!("{}.json", config.role.as_str()));
    let temp_path = config.marker_dir.join(format!(
        ".{}.json.tmp-{}",
        config.role.as_str(),
        std::process::id()
    ));
    fs::write(&temp_path, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temp_path, &final_path).map_err(|error| error.to_string())
}

fn process_start_ticks() -> Result<u64, String> {
    let stat = fs::read_to_string("/proc/self/stat").map_err(|error| error.to_string())?;
    let close = stat
        .rfind(')')
        .ok_or_else(|| "invalid /proc/self/stat".to_string())?;
    stat[close + 1..]
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| "missing process starttime".to_string())?
        .parse::<u64>()
        .map_err(|error| error.to_string())
}

fn current_cgroup() -> Result<String, String> {
    fs::read_to_string("/proc/self/cgroup")
        .map_err(|error| error.to_string())?
        .lines()
        .find_map(|line| line.strip_prefix("0::").map(ToString::to_string))
        .ok_or_else(|| "unified cgroup path not found".to_string())
}

fn emit_lines(prefix: &str, role: Role, count: u32, stderr: bool) -> Result<(), String> {
    for index in 0..count {
        if stderr {
            eprintln!("{prefix} role={} index={index}", role.as_str());
        } else {
            println!("{prefix} role={} index={index}", role.as_str());
        }
    }
    if stderr {
        io::stderr().flush().map_err(|error| error.to_string())
    } else {
        io::stdout().flush().map_err(|error| error.to_string())
    }
}
