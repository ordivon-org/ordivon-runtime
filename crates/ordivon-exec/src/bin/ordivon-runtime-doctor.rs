use ordivon_exec::{inspect_runtime, RuntimeDoctorConfig};
use std::env;
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, String> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("inspect") {
        return Err(usage());
    }
    let mut database = None;
    let mut store_root = None;
    let mut busy_timeout_ms = 5_000_u64;
    let mut pretty = false;
    let mut fail_on_violation = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--database" => database = Some(PathBuf::from(require_value(&mut args, "--database")?)),
            "--store-root" => {
                store_root = Some(PathBuf::from(require_value(&mut args, "--store-root")?))
            }
            "--busy-timeout-ms" => {
                busy_timeout_ms = require_value(&mut args, "--busy-timeout-ms")?
                    .parse()
                    .map_err(|_| "--busy-timeout-ms must be an integer".to_string())?;
            }
            "--pretty" => pretty = true,
            "--fail-on-violation" => fail_on_violation = true,
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("unknown argument: {argument}\n{}", usage())),
        }
    }
    let config = RuntimeDoctorConfig {
        db_path: database.ok_or_else(usage)?,
        store_root: store_root.ok_or_else(usage)?,
        busy_timeout_ms,
    };
    let report = inspect_runtime(&config).map_err(|error| error.to_string())?;
    let output = if pretty {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
    }
    .map_err(|error| format!("cannot serialize Doctor report: {error}"))?;
    println!("{output}");
    Ok(if fail_on_violation && report.violation_count > 0 {
        2
    } else {
        0
    })
}

fn require_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn usage() -> String {
    "usage: ordivon-runtime-doctor inspect --database ABSOLUTE_PATH --store-root ABSOLUTE_PATH [--busy-timeout-ms N] [--pretty] [--fail-on-violation]".to_string()
}
