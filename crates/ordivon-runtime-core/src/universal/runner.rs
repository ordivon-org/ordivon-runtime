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
    canonical_directory, now_unix_ms, sha256_file, workspace_source_state_digest_at,
    write_json_atomic, CapturedOutput, RunnerExecutionStep, RunnerStartEvidence, RunnerStepResult,
    RunnerTaskProgress, RunnerTaskRequest, RunnerTaskResult, TaskTerminalStatus,
    UniversalExecError, UniversalExecErrorCode, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

const REQUEST_FILE: &str = "request.json";
const RESULT_FILE: &str = "result.json";
const CANCEL_FILE: &str = "cancel-requested.json";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";
const RUNNER_START_FILE: &str = "runner-start.json";
const PROGRESS_FILE: &str = "progress.json";

pub fn run_task_runner(task_dir: &Path) -> Result<(), UniversalExecError> {
    if !task_dir.is_absolute() {
        return Err(runner_error("task directory must be absolute"));
    }
    let task_dir = canonical_directory(task_dir, "taskDir")?;
    let request = load_request(&task_dir)?;
    let started_unix_ms = now_unix_ms()?;
    let execution = validate_request_identity(&request).and_then(|()| {
        let observed_workspace_source_digest = observe_workspace_source(&request)?;
        write_runner_start(
            &task_dir,
            &request,
            observed_workspace_source_digest.as_deref(),
            started_unix_ms,
        )?;
        validate_workspace_source_commitment(
            &request,
            observed_workspace_source_digest.as_deref(),
        )?;
        execute_request(&task_dir, &request, started_unix_ms)
    });
    let result = execution.unwrap_or_else(|error| {
        let infrastructure_error_code = error.code.as_str().to_string();
        failure_result(
            &task_dir,
            &request,
            started_unix_ms,
            infrastructure_error_code.clone(),
            error.to_string(),
        )
        .unwrap_or_else(|secondary| RunnerTaskResult {
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
            infrastructure_error_code: Some(infrastructure_error_code),
            infrastructure_error: Some(format!(
                "runner failure: {error}; result construction failure: {secondary}"
            )),
            started_unix_ms,
            finished_unix_ms: started_unix_ms,
            steps: Vec::new(),
            failed_step_id: None,
            failed_step_index: None,
            stdout: empty_output(&request.task_id, true),
            stderr: empty_output(&request.task_id, false),
        })
    });
    write_json_atomic(&task_dir.join(RESULT_FILE), &result)
}

fn observe_workspace_source(
    request: &RunnerTaskRequest,
) -> Result<Option<String>, UniversalExecError> {
    if request.workspace_source_digest.is_none() {
        return Ok(None);
    }
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
    workspace_source_state_digest_at(&workspace).map(Some)
}

fn validate_workspace_source_commitment(
    request: &RunnerTaskRequest,
    observed: Option<&str>,
) -> Result<(), UniversalExecError> {
    match (request.workspace_source_digest.as_deref(), observed) {
        (None, None) => Ok(()),
        (Some(expected), Some(observed)) if expected == observed => Ok(()),
        (Some(_), Some(_)) => Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceStateMismatch,
            "Workspace source state changed after operation admission",
            Some("workspaceSourceDigest"),
            false,
        )),
        _ => Err(runner_error(
            "Workspace source observation is inconsistent with the Runner request",
        )),
    }
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
    initialize_output_file(&task_dir.join(STDOUT_FILE))?;
    initialize_output_file(&task_dir.join(STDERR_FILE))?;

    let steps = if request.steps.is_empty() {
        vec![RunnerExecutionStep {
            id: "command".to_string(),
            executable: request.executable.clone(),
            executable_digest: request.executable_digest.clone(),
            args: request.args.clone(),
            cwd: request.cwd.clone(),
            env: request.env.clone(),
            timeout_ms: request.timeout_ms,
            continue_on_error: false,
        }]
    } else {
        request.steps.clone()
    };
    let total_steps = u32::try_from(steps.len()).unwrap_or(u32::MAX);
    let overall_deadline = Instant::now()
        .checked_add(Duration::from_millis(request.timeout_ms))
        .ok_or_else(|| runner_error("task timeout exceeds platform monotonic clock range"))?;
    let mut revision = 0_u64;
    let mut completed_steps = 0_u32;
    let mut step_results = Vec::with_capacity(steps.len());
    let mut failed_step_id = None;
    let mut failed_step_index = None;
    let mut first_failure_exit = None;
    let mut any_timed_out = false;
    let mut stdout_retained = 0_u64;
    let mut stderr_retained = 0_u64;
    let mut stdout_dropped = 0_u64;
    let mut stderr_dropped = 0_u64;
    let mut cancelled = false;

    write_progress(
        task_dir,
        request,
        &mut revision,
        "working",
        completed_steps,
        total_steps,
        None,
        None,
        None,
        None,
        None,
    )?;

    for (index, step) in steps.iter().enumerate() {
        if task_dir.join(CANCEL_FILE).exists() {
            cancelled = true;
            break;
        }
        let index_u32 = u32::try_from(index).unwrap_or(u32::MAX);
        let step_started = now_unix_ms()?;
        write_progress(
            task_dir,
            request,
            &mut revision,
            "working",
            completed_steps,
            total_steps,
            Some(&step.id),
            Some(index_u32),
            Some(step_started),
            failed_step_id.as_deref(),
            failed_step_index,
        )?;
        let outcome = execute_step(
            task_dir,
            request,
            &workspace,
            step,
            overall_deadline,
            stdout_retained,
            stderr_retained,
        )?;
        stdout_retained = stdout_retained.saturating_add(outcome.stdout_retained);
        stderr_retained = stderr_retained.saturating_add(outcome.stderr_retained);
        stdout_dropped = stdout_dropped.saturating_add(outcome.stdout_dropped);
        stderr_dropped = stderr_dropped.saturating_add(outcome.stderr_dropped);
        let step_finished = now_unix_ms()?;
        let was_cancelled = task_dir.join(CANCEL_FILE).exists();
        let succeeded = outcome.status.success() && !outcome.timed_out && !was_cancelled;
        let status = if was_cancelled {
            "cancelled"
        } else if outcome.timed_out {
            "timed_out"
        } else if outcome.status.success() {
            "succeeded"
        } else {
            "failed"
        };
        if succeeded {
            completed_steps = completed_steps.saturating_add(1);
        } else if failed_step_id.is_none() {
            failed_step_id = Some(step.id.clone());
            failed_step_index = Some(index_u32);
            first_failure_exit = outcome.status.code();
        }
        any_timed_out |= outcome.timed_out;
        let continued =
            !succeeded && !was_cancelled && !outcome.timed_out && step.continue_on_error;
        step_results.push(RunnerStepResult {
            id: step.id.clone(),
            index: index_u32,
            status: status.to_string(),
            exit_code: outcome.status.code(),
            timed_out: outcome.timed_out,
            continued,
            started_unix_ms: step_started,
            finished_unix_ms: step_finished,
        });
        write_progress(
            task_dir,
            request,
            &mut revision,
            if succeeded || continued {
                "working"
            } else {
                status
            },
            completed_steps,
            total_steps,
            None,
            None,
            None,
            failed_step_id.as_deref(),
            failed_step_index,
        )?;
        if was_cancelled {
            cancelled = true;
            break;
        }
        if !succeeded && (!continued || outcome.timed_out) {
            break;
        }
        if Instant::now() >= overall_deadline {
            any_timed_out = true;
            if failed_step_id.is_none() {
                failed_step_id = Some(step.id.clone());
                failed_step_index = Some(index_u32);
            }
            break;
        }
    }

    let terminal_status = if cancelled {
        TaskTerminalStatus::Cancelled
    } else if failed_step_id.is_some() || any_timed_out {
        TaskTerminalStatus::Failed
    } else {
        TaskTerminalStatus::Completed
    };
    let terminal_progress = match terminal_status {
        TaskTerminalStatus::Completed => "succeeded",
        TaskTerminalStatus::Cancelled => "cancelled",
        TaskTerminalStatus::Failed if any_timed_out => "timed_out",
        TaskTerminalStatus::Failed => "failed",
    };
    write_progress(
        task_dir,
        request,
        &mut revision,
        terminal_progress,
        completed_steps,
        total_steps,
        None,
        None,
        None,
        failed_step_id.as_deref(),
        failed_step_index,
    )?;
    let stdout =
        captured_output_from_file(task_dir, request, true, stdout_retained, stdout_dropped)?;
    let stderr =
        captured_output_from_file(task_dir, request, false, stderr_retained, stderr_dropped)?;
    Ok(RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: request.task_id.clone(),
        job_id: request.job_id.clone(),
        attempt_id: request.attempt_id.clone(),
        launch_token_digest: request.launch_token.as_deref().map(sha256_text),
        payload_uid: request.payload.as_ref().map(|payload| payload.uid),
        payload_gid: request.payload.as_ref().map(|payload| payload.gid),
        status: terminal_status,
        exit_code: first_failure_exit
            .or_else(|| step_results.last().and_then(|step| step.exit_code)),
        timed_out: any_timed_out,
        infrastructure_error_code: None,
        infrastructure_error: None,
        started_unix_ms,
        finished_unix_ms: now_unix_ms()?,
        steps: step_results,
        failed_step_id,
        failed_step_index,
        stdout,
        stderr,
    })
}

struct StepOutcome {
    status: std::process::ExitStatus,
    timed_out: bool,
    stdout_retained: u64,
    stderr_retained: u64,
    stdout_dropped: u64,
    stderr_dropped: u64,
}

fn execute_step(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    workspace: &Path,
    step: &RunnerExecutionStep,
    overall_deadline: Instant,
    stdout_retained_before: u64,
    stderr_retained_before: u64,
) -> Result<StepOutcome, UniversalExecError> {
    let cwd_text = if let Some(payload) = &request.payload {
        let relative = Path::new(&step.cwd)
            .strip_prefix(&request.workspace_path)
            .unwrap_or(Path::new("."));
        Path::new(&payload.workspace_view)
            .join(relative)
            .to_string_lossy()
            .into_owned()
    } else {
        step.cwd.clone()
    };
    let cwd = canonical_directory(Path::new(&cwd_text), "cwd")?;
    if !cwd.starts_with(workspace) {
        return Err(runner_error("runner cwd escaped workspace"));
    }
    super::validate_exec_payload(&step.args, &step.env, "steps")?;
    let executable =
        validate_executable_identity(&step.executable, &step.executable_digest, "executable")?;
    let mut command = Command::new(&executable);
    command.args(&step.args);
    if !request.inherit_host_environment {
        command.env_clear();
    }
    command.envs(&step.env);
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
    // Give every step its own process group. A timed-out shell may leave
    // descendants holding stdout/stderr pipes after the direct child exits;
    // killing only the child would then prevent the Runner from committing
    // its durable result before the outer systemd RuntimeMaxSec ceiling.
    command.process_group(0);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("cannot start step {}: {error}", step.id),
            Some("steps.executable"),
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
    let stdout_thread = thread::spawn(move || {
        capture_stream_append(stdout, &stdout_path, stdout_limit, stdout_retained_before)
    });
    let stderr_thread = thread::spawn(move || {
        capture_stream_append(stderr, &stderr_path, stderr_limit, stderr_retained_before)
    });
    let step_deadline = Instant::now()
        .checked_add(Duration::from_millis(step.timeout_ms))
        .ok_or_else(|| runner_error("step timeout exceeds platform monotonic clock range"))?;
    let deadline = step_deadline.min(overall_deadline);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::ToolFailed,
                format!("cannot observe step {}: {error}", step.id),
                None,
                false,
            )
        })? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            terminate_process_group(child.id(), &step.id)?;
            break child.wait().map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    format!("cannot reap timed-out step {}: {error}", step.id),
                    None,
                    false,
                )
            })?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let (stdout_retained, stdout_dropped) = stdout_thread
        .join()
        .map_err(|_| runner_error("stdout capture thread panicked"))??;
    let (stderr_retained, stderr_dropped) = stderr_thread
        .join()
        .map_err(|_| runner_error("stderr capture thread panicked"))??;
    Ok(StepOutcome {
        status,
        timed_out,
        stdout_retained,
        stderr_retained,
        stdout_dropped,
        stderr_dropped,
    })
}

fn terminate_process_group(pid: u32, step_id: &str) -> Result<(), UniversalExecError> {
    let pgid = i32::try_from(pid).map_err(|_| {
        UniversalExecError::new(
            UniversalExecErrorCode::ToolFailed,
            format!("timed-out step {step_id} has an invalid process identity"),
            None,
            false,
        )
    })?;
    let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(UniversalExecError::new(
        UniversalExecErrorCode::ToolFailed,
        format!("cannot terminate process group for timed-out step {step_id}: {error}"),
        None,
        false,
    ))
}

fn initialize_output_file(path: &Path) -> Result<(), UniversalExecError> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot create {}: {error}", path.display()),
                None,
                false,
            )
        })
}

fn capture_stream_append(
    mut reader: impl Read,
    path: &Path,
    limit: u64,
    retained_before: u64,
) -> Result<(u64, u64), UniversalExecError> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot open {} for append: {error}", path.display()),
                None,
                false,
            )
        })?;
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
        let remaining = limit.saturating_sub(retained_before.saturating_add(retained)) as usize;
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
            retained = retained.saturating_add(write_len as u64);
        }
        dropped = dropped.saturating_add((read - write_len) as u64);
    }
    file.sync_all().map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot sync target output: {error}"),
            None,
            false,
        )
    })?;
    Ok((retained, dropped))
}

fn captured_output_from_file(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    stdout: bool,
    retained: u64,
    dropped: u64,
) -> Result<CapturedOutput, UniversalExecError> {
    let (suffix, file_name) = if stdout {
        ("stdout", STDOUT_FILE)
    } else {
        ("stderr", STDERR_FILE)
    };
    let path = task_dir.join(file_name);
    let actual = fs::metadata(&path)
        .map_err(|error| runner_error(format!("cannot inspect {file_name}: {error}")))?
        .len();
    if actual != retained {
        return Err(runner_error(format!(
            "captured {file_name} length {actual} does not match retained count {retained}"
        )));
    }
    Ok(CapturedOutput {
        artifact_id: format!("{}.{}", request.task_id, suffix),
        file_name: file_name.to_string(),
        digest: sha256_file(&path)?,
        retained_bytes: retained,
        dropped_bytes: dropped,
        truncated: dropped > 0,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_progress(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    revision: &mut u64,
    status: &str,
    completed_steps: u32,
    total_steps: u32,
    current_step_id: Option<&str>,
    current_step_index: Option<u32>,
    current_step_started_unix_ms: Option<u128>,
    failed_step_id: Option<&str>,
    failed_step_index: Option<u32>,
) -> Result<(), UniversalExecError> {
    *revision = revision.saturating_add(1);
    write_json_atomic(
        &task_dir.join(PROGRESS_FILE),
        &RunnerTaskProgress {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            task_id: request.task_id.clone(),
            revision: *revision,
            status: status.to_string(),
            completed_steps,
            total_steps,
            current_step_id: current_step_id.map(ToString::to_string),
            current_step_index,
            current_step_started_unix_ms,
            failed_step_id: failed_step_id.map(ToString::to_string),
            failed_step_index,
            updated_unix_ms: now_unix_ms()?,
        },
    )
}

fn validate_executable_identity(
    executable: &str,
    expected_digest: &str,
    field: &str,
) -> Result<PathBuf, UniversalExecError> {
    let path = Path::new(executable);
    let canonical = fs::canonicalize(path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot canonicalize target executable: {error}"),
            Some(field),
            false,
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            format!("cannot inspect target executable: {error}"),
            Some(field),
            false,
        )
    })?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(runner_error("target must resolve to an executable file"));
    }
    let digest = sha256_file(&canonical)?;
    if digest != expected_digest {
        return Err(runner_error(
            "target executable digest changed before launch",
        ));
    }
    Ok(canonical)
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
    super::validate_exec_payload(&request.args, &request.env, "execution")?;
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

fn failure_result(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    started_unix_ms: u128,
    infrastructure_error_code: String,
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
        infrastructure_error_code: Some(infrastructure_error_code),
        infrastructure_error: Some(message),
        started_unix_ms,
        finished_unix_ms: now_unix_ms()?,
        steps: Vec::new(),
        failed_step_id: None,
        failed_step_index: None,
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
    observed_workspace_source_digest: Option<&str>,
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
                observed_workspace_source_digest: observed_workspace_source_digest
                    .map(ToString::to_string),
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
