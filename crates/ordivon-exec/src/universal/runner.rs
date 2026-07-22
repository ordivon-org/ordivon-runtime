use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    canonical_directory, now_unix_ms, sha256_file, write_json_atomic, CapturedOutput,
    RunnerStartEvidence, RunnerTaskRequest, RunnerTaskResult, TaskTerminalStatus,
    UniversalExecError, UniversalExecErrorCode, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

const REQUEST_FILE: &str = "request.json";
const RESULT_FILE: &str = "result.json";
const CANCEL_FILE: &str = "cancel-requested.json";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";
const RUNNER_START_FILE: &str = "runner-start.json";

pub fn run_task_runner(task_dir: &Path) -> Result<(), UniversalExecError> {
    if !task_dir.is_absolute() {
        return Err(runner_error("task directory must be absolute"));
    }
    let task_dir = canonical_directory(task_dir, "taskDir")?;
    let request = load_request(&task_dir)?;
    let started_unix_ms = now_unix_ms()?;
    let execution = validate_request_identity(&request)
        .and_then(|()| write_runner_start(&task_dir, &request, started_unix_ms))
        .and_then(|()| execute_request(&task_dir, &request, started_unix_ms));
    let result = execution.unwrap_or_else(|error| {
        failure_result(&task_dir, &request, started_unix_ms, error.to_string()).unwrap_or_else(
            |secondary| RunnerTaskResult {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                task_id: request.task_id.clone(),
                job_id: request.job_id.clone(),
                attempt_id: request.attempt_id.clone(),
                launch_token_digest: request.launch_token.as_deref().map(sha256_text),
                payload_uid: request.payload.as_ref().map(|payload| payload.uid),
                payload_gid: request.payload.as_ref().map(|payload| payload.gid),
                status: TaskTerminalStatus::Failed,
                exit_code: None,
                timed_out: false,
                infrastructure_error: Some(format!(
                    "runner failure: {error}; result construction failure: {secondary}"
                )),
                started_unix_ms,
                finished_unix_ms: started_unix_ms,
                stdout: empty_output(&request.task_id, true),
                stderr: empty_output(&request.task_id, false),
            },
        )
    });
    write_json_atomic(&task_dir.join(RESULT_FILE), &result)
}

fn execute_request(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    started_unix_ms: u128,
) -> Result<RunnerTaskResult, UniversalExecError> {
    validate_request_identity(request)?;
    let workspace = canonical_directory(
        Path::new(
            request
                .payload
                .as_ref()
                .map(|payload| payload.workspace_view.as_str())
                .unwrap_or(request.workspace_path.as_str()),
        ),
        "workspacePath",
    )?;
    let cwd = canonical_directory(
        Path::new(
            request
                .payload
                .as_ref()
                .map(|payload| payload.cwd_view.as_str())
                .unwrap_or(request.cwd.as_str()),
        ),
        "cwd",
    )?;
    if !cwd.starts_with(&workspace) {
        return Err(runner_error("runner cwd escaped workspace"));
    }
    let executable = validate_executable(request)?;
    let mut command = Command::new(&executable);
    command.args(&request.args).env_clear().envs(&request.env);
    if let Some(payload) = &request.payload {
        command
            .env("HOME", &payload.runtime_view)
            .env("XDG_CACHE_HOME", &payload.cache_view)
            .env("TMPDIR", &payload.runtime_view)
            .env("ORDIVON_PAYLOAD_UID", payload.uid.to_string())
            .env("ORDIVON_PAYLOAD_GID", payload.gid.to_string());
        configure_payload_drop(&mut command, payload, &cwd)?;
    } else {
        command.current_dir(&cwd);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("cannot start target executable: {error}"),
            Some("executable"),
            false,
        )
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| runner_error("target stdout pipe is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| runner_error("target stderr pipe is unavailable"))?;
    let stdout_path = task_dir.join(STDOUT_FILE);
    let stderr_path = task_dir.join(STDERR_FILE);
    let stdout_limit = request.stdout_limit_bytes;
    let stderr_limit = request.stderr_limit_bytes;
    let task_id_stdout = request.task_id.clone();
    let task_id_stderr = request.task_id.clone();
    let stdout_thread = thread::spawn(move || {
        capture_stream(
            stdout,
            &stdout_path,
            stdout_limit,
            format!("{task_id_stdout}.stdout"),
            STDOUT_FILE,
        )
    });
    let stderr_thread = thread::spawn(move || {
        capture_stream(
            stderr,
            &stderr_path,
            stderr_limit,
            format!("{task_id_stderr}.stderr"),
            STDERR_FILE,
        )
    });

    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("cannot observe target process: {error}"),
                None,
                false,
            )
        })? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child.kill().map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    format!("cannot terminate timed-out target: {error}"),
                    None,
                    false,
                )
            })?;
            break child.wait().map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    format!("cannot reap timed-out target: {error}"),
                    None,
                    false,
                )
            })?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout_thread
        .join()
        .map_err(|_| runner_error("stdout capture thread panicked"))??;
    let stderr = stderr_thread
        .join()
        .map_err(|_| runner_error("stderr capture thread panicked"))??;
    let cancelled = task_dir.join(CANCEL_FILE).exists();
    let terminal_status = if cancelled {
        TaskTerminalStatus::Cancelled
    } else if timed_out || !status.success() {
        TaskTerminalStatus::Failed
    } else {
        TaskTerminalStatus::Completed
    };
    Ok(RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        job_id: request.job_id.clone(),
        attempt_id: request.attempt_id.clone(),
        launch_token_digest: request.launch_token.as_deref().map(sha256_text),
        payload_uid: request.payload.as_ref().map(|payload| payload.uid),
        payload_gid: request.payload.as_ref().map(|payload| payload.gid),
        status: terminal_status,
        exit_code: status.code(),
        timed_out,
        infrastructure_error: None,
        started_unix_ms,
        finished_unix_ms: now_unix_ms()?,
        stdout,
        stderr,
    })
}

fn capture_stream(
    mut reader: impl Read,
    path: &Path,
    limit: u64,
    artifact_id: String,
    file_name: &str,
) -> Result<CapturedOutput, UniversalExecError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot create {}: {error}", path.display()),
                None,
                false,
            )
        })?;
    let mut hasher = Sha256::new();
    let mut retained = 0_u64;
    let mut dropped = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot read target output: {error}"),
                None,
                false,
            )
        })?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained) as usize;
        let write_len = read.min(remaining);
        if write_len > 0 {
            file.write_all(&buffer[..write_len]).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::IoError,
                    format!("cannot persist target output: {error}"),
                    None,
                    false,
                )
            })?;
            hasher.update(&buffer[..write_len]);
            retained += write_len as u64;
        }
        dropped += (read - write_len) as u64;
    }
    file.sync_all().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot sync target output: {error}"),
            None,
            false,
        )
    })?;
    Ok(CapturedOutput {
        artifact_id,
        file_name: file_name.to_string(),
        digest: format!("sha256:{}", hex::encode(hasher.finalize())),
        retained_bytes: retained,
        dropped_bytes: dropped,
        truncated: dropped > 0,
    })
}

fn load_request(task_dir: &Path) -> Result<RunnerTaskRequest, UniversalExecError> {
    let path = task_dir.join(REQUEST_FILE);
    let bytes = fs::read(&path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("cannot read runner request: {error}"),
            None,
            false,
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::MetadataCorrupt,
            format!("invalid runner request: {error}"),
            None,
            false,
        )
    })
}

fn validate_request_identity(request: &RunnerTaskRequest) -> Result<(), UniversalExecError> {
    if request.schema_version != UNIVERSAL_EXEC_SCHEMA_VERSION {
        return Err(runner_error("unsupported runner request schema"));
    }
    super::validate_id(&request.task_id, "taskId")?;
    super::validate_id(&request.workspace_id, "workspaceId")?;
    super::validate_args(&request.args)?;
    super::validate_env(&request.env)?;
    if let Some(payload) = &request.payload {
        if payload.uid == 0 || payload.gid == 0 {
            return Err(runner_error("payload identity must be non-root"));
        }
        for (field, value) in [
            ("payload.workspaceView", &payload.workspace_view),
            ("payload.cwdView", &payload.cwd_view),
            ("payload.runtimeView", &payload.runtime_view),
            ("payload.cacheView", &payload.cache_view),
        ] {
            if !Path::new(value).is_absolute() || value.as_bytes().contains(&0) {
                return Err(runner_error(format!(
                    "{field} must be an absolute NUL-free path"
                )));
            }
        }
    }
    Ok(())
}

fn configure_payload_drop(
    command: &mut Command,
    payload: &super::RunnerPayloadConfig,
    cwd: &Path,
) -> Result<(), UniversalExecError> {
    let cwd = std::ffi::CString::new(cwd.as_os_str().as_encoded_bytes())
        .map_err(|_| runner_error("payload cwd contains NUL"))?;
    let uid = payload.uid;
    let gid = payload.gid;
    unsafe {
        command.pre_exec(move || {
            if libc::setgroups(0, std::ptr::null()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::setgid(gid) != 0 || libc::setuid(uid) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            libc::umask(0o077);
            if libc::chdir(cwd.as_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

fn validate_executable(request: &RunnerTaskRequest) -> Result<PathBuf, UniversalExecError> {
    let path = Path::new(&request.executable);
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot inspect target executable: {error}"),
            Some("executable"),
            false,
        )
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(runner_error(
            "target executable must remain a non-symlink executable file",
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot canonicalize target executable: {error}"),
            Some("executable"),
            false,
        )
    })?;
    let digest = sha256_file(&canonical)?;
    if digest != request.executable_digest {
        return Err(runner_error(
            "target executable digest changed before launch",
        ));
    }
    Ok(canonical)
}

fn failure_result(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    started_unix_ms: u128,
    message: String,
) -> Result<RunnerTaskResult, UniversalExecError> {
    Ok(RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        job_id: request.job_id.clone(),
        attempt_id: request.attempt_id.clone(),
        launch_token_digest: request.launch_token.as_deref().map(sha256_text),
        payload_uid: request.payload.as_ref().map(|payload| payload.uid),
        payload_gid: request.payload.as_ref().map(|payload| payload.gid),
        status: TaskTerminalStatus::Failed,
        exit_code: None,
        timed_out: false,
        infrastructure_error: Some(message),
        started_unix_ms,
        finished_unix_ms: now_unix_ms()?,
        stdout: capture_empty_if_missing(task_dir, &request.task_id, true)?,
        stderr: capture_empty_if_missing(task_dir, &request.task_id, false)?,
    })
}

fn capture_empty_if_missing(
    task_dir: &Path,
    task_id: &str,
    stdout: bool,
) -> Result<CapturedOutput, UniversalExecError> {
    let (suffix, file_name) = if stdout {
        ("stdout", STDOUT_FILE)
    } else {
        ("stderr", STDERR_FILE)
    };
    let path = task_dir.join(file_name);
    if !path.exists() {
        File::create(&path)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::IoError,
                    format!("cannot create empty output: {error}"),
                    None,
                    false,
                )
            })?;
    }
    let retained = fs::metadata(&path)
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot inspect output: {error}"),
                None,
                false,
            )
        })?
        .len();
    Ok(CapturedOutput {
        artifact_id: format!("{task_id}.{suffix}"),
        file_name: file_name.to_string(),
        digest: sha256_file(&path)?,
        retained_bytes: retained,
        dropped_bytes: 0,
        truncated: false,
    })
}

fn empty_output(task_id: &str, stdout: bool) -> CapturedOutput {
    let (suffix, file_name) = if stdout {
        ("stdout", STDOUT_FILE)
    } else {
        ("stderr", STDERR_FILE)
    };
    CapturedOutput {
        artifact_id: format!("{task_id}.{suffix}"),
        file_name: file_name.to_string(),
        digest: format!("sha256:{}", hex::encode(Sha256::digest([]))),
        retained_bytes: 0,
        dropped_bytes: 0,
        truncated: false,
    }
}

fn runner_error(message: impl Into<String>) -> UniversalExecError {
    UniversalExecError::new(
        UniversalExecErrorCode::MetadataCorrupt,
        message,
        None,
        false,
    )
}

fn sha256_text(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}

fn write_runner_start(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    observed_unix_ms: u128,
) -> Result<(), UniversalExecError> {
    let identity = match (
        request.job_id.as_deref(),
        request.attempt_id.as_deref(),
        request.launch_token.as_deref(),
        request.unit_name.as_deref(),
    ) {
        (None, None, None, None) => return Ok(()),
        (Some(job_id), Some(attempt_id), Some(launch_token), Some(unit_name)) => {
            if request.task_id != attempt_id {
                return Err(runner_error("runtime taskId must equal attemptId"));
            }
            super::validate_id(job_id, "jobId")?;
            super::validate_id(attempt_id, "attemptId")?;
            if !unit_name.ends_with(".service") {
                return Err(runner_error("runtime unitName must identify a service"));
            }
            let invocation_id = std::env::var("INVOCATION_ID")
                .map_err(|_| runner_error("systemd INVOCATION_ID is unavailable"))?;
            RunnerStartEvidence {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                job_id: job_id.to_string(),
                attempt_id: attempt_id.to_string(),
                launch_token_digest: sha256_text(launch_token),
                unit_name: unit_name.to_string(),
                invocation_id,
                control_group: read_self_cgroup()?,
                namespace_pid: std::process::id(),
                namespace_process_start_identity: read_process_start_identity(std::process::id())?,
                payload_uid: request.payload.as_ref().map(|payload| payload.uid),
                payload_gid: request.payload.as_ref().map(|payload| payload.gid),
                observed_unix_ms,
            }
        }
        _ => return Err(runner_error("Runner identity fields must appear together")),
    };
    write_json_atomic(&task_dir.join(RUNNER_START_FILE), &identity)
}

fn read_trimmed(path: &str) -> Result<String, UniversalExecError> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| runner_error(format!("cannot read {path}: {error}")))
}

fn read_self_cgroup() -> Result<String, UniversalExecError> {
    let text = read_trimmed("/proc/self/cgroup")?;
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(ToString::to_string)
        .filter(|path| path.starts_with('/'))
        .ok_or_else(|| runner_error("cannot identify cgroup v2 path"))
}

fn read_process_start_identity(pid: u32) -> Result<String, UniversalExecError> {
    let stat = read_trimmed(&format!("/proc/{pid}/stat"))?;
    let close = stat
        .rfind(')')
        .ok_or_else(|| runner_error("invalid proc stat format"))?;
    stat[close + 1..]
        .split_whitespace()
        .nth(19)
        .map(ToString::to_string)
        .ok_or_else(|| runner_error("proc stat omitted process starttime"))
}
