use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
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
        validate_input_commitments(&request)?;
        let mut host_dependency_watch =
            PathDriftWatch::new_host_dependencies(&request.host_dependencies)?;
        validate_host_dependency_commitments(&request)?;
        if let Some(watch) = host_dependency_watch.as_mut() {
            watch.check()?;
        }
        execute_request(
            &task_dir,
            &request,
            started_unix_ms,
            host_dependency_watch.as_mut(),
        )
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

fn validate_input_tree_exact(
    root: &Path,
    commitments: &[super::RunnerInputCommitment],
) -> Result<(), UniversalExecError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::InputStateMismatch,
            format!("immutable input presentation root is unavailable: {error}"),
            Some("inputPresentationRoot"),
            false,
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::InputStateMismatch,
            "immutable input presentation root is not a non-symlink directory",
            Some("inputPresentationRoot"),
            false,
        ));
    }

    let expected_files = commitments
        .iter()
        .map(|input| {
            Path::new(&input.presentation_path)
                .strip_prefix(root)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    UniversalExecError::new(
                        UniversalExecErrorCode::InputStateMismatch,
                        format!(
                            "immutable input {} is outside presentation root {}",
                            input.presentation_path,
                            root.display()
                        ),
                        Some("inputPresentationRoot"),
                        false,
                    )
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut expected_directories = BTreeSet::new();
    for file in &expected_files {
        let mut parent = file.parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            expected_directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let mut observed_files = BTreeSet::new();
    let mut observed_directories = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "cannot enumerate immutable input presentation tree {}: {error}",
                    directory.display()
                ),
                Some("inputPresentationRoot"),
                false,
            )
        })? {
            let entry = entry.map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::InputStateMismatch,
                    format!("cannot read immutable input tree entry: {error}"),
                    Some("inputPresentationRoot"),
                    false,
                )
            })?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::InputStateMismatch,
                    format!("cannot inspect immutable input tree entry: {error}"),
                    Some("inputPresentationRoot"),
                    false,
                )
            })?;
            if metadata.file_type().is_symlink() {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::InputStateMismatch,
                    format!("immutable input tree contains symlink {}", path.display()),
                    Some("inputPresentationRoot"),
                    false,
                ));
            }
            if metadata.is_dir() {
                observed_directories.insert(path.strip_prefix(root).unwrap().to_path_buf());
                pending.push(path);
            } else if metadata.is_file() {
                observed_files.insert(path.strip_prefix(root).unwrap().to_path_buf());
            } else {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::InputStateMismatch,
                    format!(
                        "immutable input tree contains unsupported filesystem object {}",
                        path.display()
                    ),
                    Some("inputPresentationRoot"),
                    false,
                ));
            }
        }
    }
    if observed_files != expected_files || observed_directories != expected_directories {
        let unexpected_files = observed_files
            .difference(&expected_files)
            .cloned()
            .collect::<Vec<_>>();
        let missing_files = expected_files
            .difference(&observed_files)
            .cloned()
            .collect::<Vec<_>>();
        let unexpected_directories = observed_directories
            .difference(&expected_directories)
            .cloned()
            .collect::<Vec<_>>();
        let missing_directories = expected_directories
            .difference(&observed_directories)
            .cloned()
            .collect::<Vec<_>>();
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::InputStateMismatch,
            format!(
                "immutable input presentation tree differs from committed closure: unexpectedFiles={unexpected_files:?}, missingFiles={missing_files:?}, unexpectedDirectories={unexpected_directories:?}, missingDirectories={missing_directories:?}"
            ),
            Some("inputPresentationRoot"),
            false,
        ));
    }
    Ok(())
}

fn validate_host_dependency_commitments(
    request: &RunnerTaskRequest,
) -> Result<(), UniversalExecError> {
    for (index, dependency) in request.host_dependencies.iter().enumerate() {
        let field = format!("hostDependencies[{index}]");
        let path = Path::new(&dependency.path);
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "declared Host Dependency {} is unavailable at target-spawn boundary: {error}",
                    dependency.path
                ),
                Some(&format!("{field}.path")),
                false,
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "declared Host Dependency {} is not a regular non-symlink file",
                    dependency.path
                ),
                Some(&format!("{field}.path")),
                false,
            ));
        }
        let observed = sha256_file(path)?;
        if observed != dependency.digest {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "declared Host Dependency {} changed after admission: expected {}, observed {}",
                    dependency.path, dependency.digest, observed
                ),
                Some(&format!("{field}.digest")),
                false,
            ));
        }
    }
    Ok(())
}

fn validate_input_commitments(request: &RunnerTaskRequest) -> Result<(), UniversalExecError> {
    if let Some(root) = request.input_presentation_root.as_deref() {
        validate_input_tree_exact(Path::new(root), &request.input_commitments)?;
    }
    for (index, input) in request.input_commitments.iter().enumerate() {
        let path = Path::new(&input.presentation_path);
        let presentation_field = format!("inputCommitments[{index}].presentationPath");
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "immutable input {} is unavailable at target-spawn boundary: {error}",
                    input.presentation_path
                ),
                Some(&presentation_field),
                false,
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "immutable input {} is not a regular non-symlink file",
                    input.presentation_path
                ),
                Some(&presentation_field),
                false,
            ));
        }
        if metadata.len() != input.byte_length {
            let length_field = format!("inputCommitments[{index}].byteLength");
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "immutable input {} byte length changed after admission: expected {}, observed {}",
                    input.presentation_path, input.byte_length, metadata.len()
                ),
                Some(&length_field),
                false,
            ));
        }
        let observed = sha256_file(path)?;
        if observed != input.digest {
            let digest_field = format!("inputCommitments[{index}].digest");
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::InputStateMismatch,
                format!(
                    "immutable input {} digest changed after admission: expected {}, observed {}",
                    input.presentation_path, input.digest, observed
                ),
                Some(&digest_field),
                false,
            ));
        }
    }
    Ok(())
}

fn execute_request(
    task_dir: &Path,
    request: &RunnerTaskRequest,
    started_unix_ms: u128,
    mut host_dependency_watch: Option<&mut PathDriftWatch>,
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
            StepExecutionContext {
                overall_deadline,
                stdout_retained_before: stdout_retained,
                stderr_retained_before: stderr_retained,
                host_dependency_watch: host_dependency_watch.as_deref_mut(),
            },
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

    if let Some(watch) = host_dependency_watch.as_mut() {
        watch.check()?;
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

struct StepExecutionContext<'a> {
    overall_deadline: Instant,
    stdout_retained_before: u64,
    stderr_retained_before: u64,
    host_dependency_watch: Option<&'a mut PathDriftWatch>,
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
    mut context: StepExecutionContext<'_>,
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
    if let Some(watch) = context.host_dependency_watch.as_mut() {
        watch.check()?;
    }
    let mut executable =
        prepare_executable_realization(&step.executable, &step.executable_digest, "executable")?;
    let mut command = Command::new(executable.exec_path());
    command.arg0(&step.executable).args(&step.args);
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
        capture_stream_append(
            stdout,
            &stdout_path,
            stdout_limit,
            context.stdout_retained_before,
        )
    });
    let stderr_thread = thread::spawn(move || {
        capture_stream_append(
            stderr,
            &stderr_path,
            stderr_limit,
            context.stderr_retained_before,
        )
    });
    let step_deadline = Instant::now()
        .checked_add(Duration::from_millis(step.timeout_ms))
        .ok_or_else(|| runner_error("step timeout exceeds platform monotonic clock range"))?;
    let deadline = step_deadline.min(context.overall_deadline);
    let mut timed_out = false;
    let mut runtime_drift = None;
    let status = loop {
        if let Err(error) = executable.check() {
            let _ = terminate_process_group(child.id(), &step.id);
            let status = child.wait().map_err(|wait_error| {
                UniversalExecError::new(
                    UniversalExecErrorCode::ToolFailed,
                    format!(
                        "cannot reap executable-drifted step {}: {wait_error}",
                        step.id
                    ),
                    None,
                    false,
                )
            })?;
            runtime_drift = Some(error);
            break status;
        }
        if let Some(watch) = context.host_dependency_watch.as_mut() {
            if let Err(error) = watch.check() {
                let _ = terminate_process_group(child.id(), &step.id);
                let status = child.wait().map_err(|wait_error| {
                    UniversalExecError::new(
                        UniversalExecErrorCode::ToolFailed,
                        format!("cannot reap drifted step {}: {wait_error}", step.id),
                        None,
                        false,
                    )
                })?;
                runtime_drift = Some(error);
                break status;
            }
        }
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
    if runtime_drift.is_none() {
        if let Err(error) = executable.check() {
            runtime_drift = Some(error);
        }
    }
    if runtime_drift.is_none() {
        if let Some(watch) = context.host_dependency_watch.as_mut() {
            if let Err(error) = watch.check() {
                runtime_drift = Some(error);
            }
        }
    }
    if let Some(error) = runtime_drift {
        return Err(error);
    }
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

struct PreparedExecutable {
    path: PathBuf,
    watch: PathDriftWatch,
}

impl PreparedExecutable {
    fn exec_path(&self) -> PathBuf {
        self.path.clone()
    }

    fn check(&mut self) -> Result<(), UniversalExecError> {
        self.watch.check()
    }
}

fn prepare_executable_realization(
    executable: &str,
    expected_digest: &str,
    field: &str,
) -> Result<PreparedExecutable, UniversalExecError> {
    // Preserve target pathname semantics. The witness is established before the final
    // canonicalize/hash so any later byte or topology change turns the Attempt into an
    // explicit physical-realization failure rather than a false success.
    let mut watch = PathDriftWatch::new_executable(Path::new(executable))?;
    let canonical = fs::canonicalize(executable).map_err(|error| {
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
    let observed_digest = sha256_file(&canonical)?;
    watch.check()?;
    if observed_digest != expected_digest {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::InputStateMismatch,
            format!(
                "target executable bytes changed before realization: expected {expected_digest}, observed {observed_digest}"
            ),
            Some(field),
            false,
        ));
    }
    Ok(PreparedExecutable {
        path: canonical,
        watch,
    })
}

#[derive(Clone, Copy)]
enum PathDriftKind {
    HostDependency,
    Executable,
}

#[derive(Default)]
struct PathDriftWatchPlan {
    direct: bool,
    children: BTreeSet<Vec<u8>>,
}

struct PathDriftWatchSpec {
    path: PathBuf,
    direct: bool,
    children: BTreeSet<Vec<u8>>,
}

struct PathDriftWatch {
    fd: OwnedFd,
    specs: BTreeMap<i32, PathDriftWatchSpec>,
    kind: PathDriftKind,
}

impl PathDriftWatch {
    fn new_host_dependencies(
        dependencies: &[super::RunnerHostDependencyCommitment],
    ) -> Result<Option<Self>, UniversalExecError> {
        if dependencies.is_empty() {
            return Ok(None);
        }
        let paths = dependencies
            .iter()
            .map(|dependency| PathBuf::from(&dependency.path))
            .collect::<Vec<_>>();
        Self::new(&paths, PathDriftKind::HostDependency).map(Some)
    }

    fn new_executable(path: &Path) -> Result<Self, UniversalExecError> {
        Self::new(&[path.to_path_buf()], PathDriftKind::Executable)
    }

    fn new(paths: &[PathBuf], kind: PathDriftKind) -> Result<Self, UniversalExecError> {
        let raw_fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw_fd < 0 {
            return Err(path_drift_infrastructure_error(
                kind,
                format!(
                    "cannot create path drift witness: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
        let mut plans = BTreeMap::<PathBuf, PathDriftWatchPlan>::new();
        for path in paths {
            plans.entry(path.clone()).or_default().direct = true;
            let mut current = path.as_path();
            while let (Some(parent), Some(name)) = (current.parent(), current.file_name()) {
                plans
                    .entry(parent.to_path_buf())
                    .or_default()
                    .children
                    .insert(name.as_bytes().to_vec());
                if parent == Path::new("/") {
                    break;
                }
                current = parent;
            }
        }
        let mut specs = BTreeMap::new();
        for (path, plan) in plans {
            let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                path_drift_infrastructure_error(kind, "path drift watch contains NUL")
            })?;
            let mut mask = libc::IN_ATTRIB
                | libc::IN_CLOSE_WRITE
                | libc::IN_DELETE_SELF
                | libc::IN_MOVE_SELF
                | libc::IN_UNMOUNT;
            if plan.direct {
                mask |= libc::IN_MODIFY;
            }
            if !plan.children.is_empty() {
                mask |= libc::IN_CREATE
                    | libc::IN_DELETE
                    | libc::IN_MOVED_FROM
                    | libc::IN_MOVED_TO
                    | libc::IN_MODIFY;
            }
            let wd = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), c_path.as_ptr(), mask) };
            if wd < 0 {
                return Err(path_drift_infrastructure_error(
                    kind,
                    format!(
                        "cannot watch path {}: {}",
                        path.display(),
                        std::io::Error::last_os_error()
                    ),
                ));
            }
            specs.insert(
                wd,
                PathDriftWatchSpec {
                    path,
                    direct: plan.direct,
                    children: plan.children,
                },
            );
        }
        Ok(Self { fd, specs, kind })
    }

    fn check(&mut self) -> Result<(), UniversalExecError> {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                )
            };
            if read < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(());
                }
                return Err(path_drift_infrastructure_error(
                    self.kind,
                    format!("cannot read path drift witness: {error}"),
                ));
            }
            if read == 0 {
                return Ok(());
            }
            let mut offset = 0_usize;
            let read = usize::try_from(read).unwrap_or(buffer.len());
            while offset + std::mem::size_of::<libc::inotify_event>() <= read {
                let event = unsafe {
                    std::ptr::read_unaligned(
                        buffer.as_ptr().add(offset).cast::<libc::inotify_event>(),
                    )
                };
                let event_size =
                    std::mem::size_of::<libc::inotify_event>().saturating_add(event.len as usize);
                if event_size == 0 || offset.saturating_add(event_size) > read {
                    return Err(path_drift_infrastructure_error(
                        self.kind,
                        "path drift witness emitted a malformed event",
                    ));
                }
                if event.mask & libc::IN_Q_OVERFLOW != 0 {
                    return Err(path_drift_infrastructure_error(
                        self.kind,
                        "path drift witness queue overflowed",
                    ));
                }
                if let Some(spec) = self.specs.get(&event.wd) {
                    let self_mask = libc::IN_ATTRIB
                        | libc::IN_CLOSE_WRITE
                        | libc::IN_DELETE_SELF
                        | libc::IN_MOVE_SELF
                        | libc::IN_UNMOUNT
                        | libc::IN_IGNORED;
                    if event.mask & self_mask != 0 && (spec.direct || event.len == 0) {
                        return Err(path_runtime_drift(self.kind, &spec.path, event.mask));
                    }
                    if event.len > 0 {
                        let name_start = offset + std::mem::size_of::<libc::inotify_event>();
                        let name_bytes = &buffer[name_start..offset + event_size];
                        let name_end = name_bytes
                            .iter()
                            .position(|byte| *byte == 0)
                            .unwrap_or(name_bytes.len());
                        let name = &name_bytes[..name_end];
                        let child_mask = libc::IN_ATTRIB
                            | libc::IN_CLOSE_WRITE
                            | libc::IN_CREATE
                            | libc::IN_DELETE
                            | libc::IN_MOVED_FROM
                            | libc::IN_MOVED_TO
                            | libc::IN_MODIFY;
                        if event.mask & child_mask != 0 && spec.children.contains(name) {
                            return Err(path_runtime_drift(self.kind, &spec.path, event.mask));
                        }
                    }
                }
                offset += event_size;
            }
        }
    }
}

fn path_drift_infrastructure_error(
    kind: PathDriftKind,
    message: impl Into<String>,
) -> UniversalExecError {
    UniversalExecError::new(
        match kind {
            PathDriftKind::HostDependency => UniversalExecErrorCode::HostDependencyRuntimeDrift,
            PathDriftKind::Executable => UniversalExecErrorCode::ExecutableRuntimeDrift,
        },
        message,
        Some(match kind {
            PathDriftKind::HostDependency => "hostDependencies",
            PathDriftKind::Executable => "executable",
        }),
        false,
    )
}

fn path_runtime_drift(kind: PathDriftKind, path: &Path, mask: u32) -> UniversalExecError {
    let subject = match kind {
        PathDriftKind::HostDependency => "declared Host Dependency",
        PathDriftKind::Executable => "target executable",
    };
    path_drift_infrastructure_error(
        kind,
        format!(
            "{subject} path topology or bytes changed during execution near {} (inotify mask 0x{mask:x})",
            path.display()
        ),
    )
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
    match (
        request.input_presentation_root.as_deref(),
        request.input_commitments.is_empty(),
    ) {
        (None, true) => {}
        (Some(root), false) if Path::new(root).is_absolute() && !root.as_bytes().contains(&0) => {}
        _ => {
            return Err(runner_error(
                "inputPresentationRoot must be an absolute NUL-free path exactly when input commitments exist",
            ));
        }
    }
    for (index, input) in request.input_commitments.iter().enumerate() {
        if !Path::new(&input.presentation_path).is_absolute()
            || input.presentation_path.as_bytes().contains(&0)
        {
            return Err(runner_error(format!(
                "inputCommitments[{index}].presentationPath must be an absolute NUL-free path"
            )));
        }
        if let Some(root) = request.input_presentation_root.as_deref() {
            if !Path::new(&input.presentation_path).starts_with(root) {
                return Err(runner_error(format!(
                    "inputCommitments[{index}].presentationPath must remain inside inputPresentationRoot"
                )));
            }
        }
        let Some(hex) = input.digest.strip_prefix("sha256:") else {
            return Err(runner_error(format!(
                "inputCommitments[{index}].digest must use sha256:<hex>"
            )));
        };
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(runner_error(format!(
                "inputCommitments[{index}].digest must be 32-byte SHA-256 hex"
            )));
        }
    }
    let mut previous_host_dependency_path: Option<&str> = None;
    for (index, dependency) in request.host_dependencies.iter().enumerate() {
        if !Path::new(&dependency.path).is_absolute() || dependency.path.as_bytes().contains(&0) {
            return Err(runner_error(format!(
                "hostDependencies[{index}].path must be an absolute NUL-free path"
            )));
        }
        let Some(hex) = dependency.digest.strip_prefix("sha256:") else {
            return Err(runner_error(format!(
                "hostDependencies[{index}].digest must use sha256:<hex>"
            )));
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(runner_error(format!(
                "hostDependencies[{index}].digest must be lowercase 32-byte SHA-256 hex"
            )));
        }
        if previous_host_dependency_path
            .is_some_and(|previous| previous >= dependency.path.as_str())
        {
            return Err(runner_error(
                "hostDependencies must be sorted by unique path",
            ));
        }
        previous_host_dependency_path = Some(&dependency.path);
    }
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
                runner_executable_digest: Some(sha256_file(Path::new("/proc/self/exe"))?),
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
