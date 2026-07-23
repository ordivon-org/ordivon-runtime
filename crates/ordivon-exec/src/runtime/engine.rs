use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::registry::JobSnapshot;
use super::supervisor::{
    classify_supervisor_recovery, SupervisorIdentity, SupervisorObservation,
    SupervisorRecoveryDisposition, SupervisorUnitState, TerminationIntent,
};
use super::{
    AdmissionOutcome, ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult,
    ArtifactRegistration, AttemptRecord, AttemptState, JobResolution, Registry, RegistryConfig,
    RunnerIdentity, RuntimeArtifactRecord, RuntimeError, RuntimeErrorCode, RuntimeExecutionPlan,
    RuntimeJobListRequest, RuntimeJobListResult, RuntimeResult, SubmitRequest, TaskCancelRequest,
    TaskObservation, TaskObserveRequest, TaskRunRequest, TerminalCommit, RUNTIME_SCHEMA_VERSION,
};
use crate::universal::{
    canonical_directory, load_workspace_record, resolve_workspace_cwd, sha256_bytes, sha256_file,
    write_json_atomic, CapturedOutput, RunnerPayloadConfig, RunnerStartEvidence, RunnerTaskRequest,
    RunnerTaskResult, TaskTerminalStatus, UniversalExecutorConfig, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

const RUNNER_REQUEST_FILE: &str = "request.json";
const PLAN_FILE: &str = "plan.json";
const BUNDLE_MANIFEST_FILE: &str = "bundle-manifest.json";
const RUNNER_START_FILE: &str = "runner-start.json";
const RESULT_FILE: &str = "result.json";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";
const CANCEL_FILE: &str = "cancel-requested.json";
const CONTROL_RESULT_FILE: &str = "control-result.json";
const MAX_TASK_WAIT_MS: u64 = 30_000;
const MAX_TASK_TAIL_BYTES: u64 = 64 * 1024;
const MAX_ARTIFACT_READ_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub registry: RegistryConfig,
    pub executor: UniversalExecutorConfig,
    pub startup_grace_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Runtime {
    registry: Registry,
    executor: UniversalExecutorConfig,
    startup_grace_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BundleManifest {
    schema_version: u32,
    job_id: String,
    attempt_id: String,
    request_digest: String,
    plan_digest: String,
    launch_token_digest: String,
    created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ControlTerminalEvidence {
    schema_version: u32,
    job_id: String,
    attempt_id: String,
    status: String,
    reason_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    observed_at_ms: u64,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> RuntimeResult<Self> {
        config.executor.validate().map_err(map_universal_error)?;
        if config.startup_grace_ms == 0 || config.startup_grace_ms > 30_000 {
            return Err(RuntimeError::invalid(
                "startupGraceMs must be in 1..=30000",
                "startupGraceMs",
            ));
        }
        let registry = Registry::initialize(config.registry)?;
        let runtime = Self {
            registry,
            executor: config.executor,
            startup_grace_ms: config.startup_grace_ms,
        };
        runtime.reconcile_recoverable_orphans()?;
        Ok(runtime)
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn run_task(&self, request: &TaskRunRequest) -> RuntimeResult<TaskObservation> {
        validate_run_request(request)?;
        self.reconcile_recoverable_orphans()?;
        let plan = self.resolve_plan(request)?;
        let submit = SubmitRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: request.client_request_id.clone(),
            plan,
            global_limit: request.global_limit,
        };
        let job_id = match self.registry.submit(&submit)? {
            AdmissionOutcome::Created(created) => {
                let job_id = created.job.job_id.clone();
                self.ensure_attempt_dispatched(&created.attempt)?;
                job_id
            }
            AdmissionOutcome::Existing { job } => job.job_id.clone(),
        };
        self.observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id,
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
        })
    }

    fn resolve_plan(&self, request: &TaskRunRequest) -> RuntimeResult<RuntimeExecutionPlan> {
        let record = load_workspace_record(&self.executor, &request.execution.workspace_id)
            .map_err(map_universal_error)?;
        let workspace_path =
            canonical_directory(Path::new(&record.workspace_path), "workspacePath")
                .map_err(map_universal_error)?;
        let cwd = resolve_workspace_cwd(&record, &request.execution.cwd_relative)
            .map_err(map_universal_error)?;
        let executable = validate_executable(&self.executor, &request.execution.executable)?;
        Ok(RuntimeExecutionPlan {
            schema_version: RUNTIME_SCHEMA_VERSION,
            workspace_id: request.execution.workspace_id.clone(),
            workspace_path: workspace_path.to_string_lossy().into_owned(),
            source_revision: record.source_revision,
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: sha256_file(&executable).map_err(map_universal_error)?,
            args: request.execution.args.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            env: request.execution.env.clone(),
            timeout_ms: request.execution.timeout_ms,
            stdout_limit_bytes: request.execution.stdout_limit_bytes,
            stderr_limit_bytes: request.execution.stderr_limit_bytes,
            principal: request.principal.clone(),
        })
    }

    fn ensure_attempt_dispatched(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let attempt = if attempt.bundle_digest.is_none() {
            self.materialize_bundle(attempt)?
        } else {
            attempt.clone()
        };
        match attempt.state {
            AttemptState::Accepted => self.dispatch_attempt(&attempt),
            AttemptState::Starting
            | AttemptState::Running
            | AttemptState::Stopping
            | AttemptState::Recovering => {
                self.reconcile_attempt(&attempt.attempt_id)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn inherit_host_environment(&self) -> bool {
        true
    }

    fn materialize_bundle(&self, attempt: &AttemptRecord) -> RuntimeResult<AttemptRecord> {
        if attempt.state != AttemptState::Accepted {
            return Err(RuntimeError::new(
                RuntimeErrorCode::AttemptStateConflict,
                "only accepted Attempts may materialize a bundle",
                Some("attemptId"),
                false,
            ));
        }
        let snapshot = self.registry.job_snapshot(&attempt.job_id)?;
        let job = snapshot.job;
        let stored_attempt = snapshot.attempt.ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "Job has no Attempt while materializing bundle",
                Some("attemptId"),
                false,
            )
        })?;
        if stored_attempt.attempt_id != attempt.attempt_id
            || stored_attempt.row_version != attempt.row_version
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::AttemptStateConflict,
                "Attempt changed before bundle materialization",
                Some("attemptId"),
                false,
            ));
        }
        let plan: RuntimeExecutionPlan =
            serde_json::from_str(&job.execution_plan_json).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    format!("stored execution plan is invalid: {error}"),
                    Some("executionPlan"),
                    false,
                )
            })?;
        let launch_token = sha256_bytes(
            format!(
                "runtime-launch-v1\0{}\0{}",
                attempt.attempt_id, job.operation_digest
            )
            .as_bytes(),
        );
        if sha256_bytes(launch_token.as_bytes()) != attempt.launch_token_digest {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "stored launch-token digest is inconsistent",
                Some("launchTokenDigest"),
                false,
            ));
        }
        let request = RunnerTaskRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            job_id: Some(job.job_id.clone()),
            attempt_id: Some(attempt.attempt_id.clone()),
            launch_token: Some(launch_token.clone()),
            unit_name: Some(attempt.unit_name.clone()),
            payload: self.payload_config(&attempt.attempt_id, &plan)?,
            inherit_host_environment: self.inherit_host_environment(),
            task_id: attempt.attempt_id.clone(),
            workspace_id: plan.workspace_id.clone(),
            workspace_path: plan.workspace_path.clone(),
            executable: plan.executable.clone(),
            executable_digest: plan.executable_digest.clone(),
            args: plan.args.clone(),
            cwd: plan.cwd.clone(),
            env: plan.env.clone(),
            timeout_ms: plan.timeout_ms,
            stdout_limit_bytes: plan.stdout_limit_bytes,
            stderr_limit_bytes: plan.stderr_limit_bytes,
        };
        let request_bytes = serde_json::to_vec(&request).map_err(serialization_error)?;
        let plan_bytes = serde_json::to_vec(&plan).map_err(serialization_error)?;
        let manifest = BundleManifest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: job.job_id.clone(),
            attempt_id: attempt.attempt_id.clone(),
            request_digest: sha256_bytes(&request_bytes),
            plan_digest: sha256_bytes(&plan_bytes),
            launch_token_digest: sha256_bytes(launch_token.as_bytes()),
            created_at_ms: attempt.created_at_ms,
        };
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(serialization_error)?;
        let bundle_digest = sha256_bytes(&manifest_bytes);
        if manifest.launch_token_digest != attempt.launch_token_digest
            || manifest.plan_digest != job.execution_plan_digest
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "reconstructed bundle identity does not match Registry",
                Some("attemptId"),
                false,
            ));
        }

        let final_path = PathBuf::from(&attempt.bundle_path);
        let parent = final_path.parent().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::IoError,
                "Attempt bundle has no parent directory",
                Some("bundlePath"),
                false,
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| io_error("create attempts root", error))?;
        let staging_prefix = format!(".{}.staging-", attempt.attempt_id);
        for entry in
            fs::read_dir(parent).map_err(|error| io_error("scan Attempt staging bundles", error))?
        {
            let entry = entry.map_err(|error| io_error("read Attempt staging entry", error))?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&staging_prefix))
            {
                fs::remove_dir_all(entry.path())
                    .map_err(|error| io_error("remove stale staging bundle", error))?;
            }
        }
        let staging = parent.join(format!("{staging_prefix}{}", std::process::id()));
        if final_path.exists() {
            fs::remove_dir_all(&final_path)
                .map_err(|error| io_error("remove uncommitted bundle", error))?;
        }
        fs::create_dir(&staging).map_err(|error| io_error("create staging bundle", error))?;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("protect staging bundle", error))?;
        write_bytes_synced(&staging.join(RUNNER_REQUEST_FILE), &request_bytes)?;
        write_bytes_synced(&staging.join(PLAN_FILE), &plan_bytes)?;
        write_bytes_synced(&staging.join(BUNDLE_MANIFEST_FILE), &manifest_bytes)?;
        sync_directory(&staging)?;
        fs::rename(&staging, &final_path)
            .map_err(|error| io_error("commit Attempt bundle", error))?;
        sync_directory(parent)?;
        self.registry.mark_bundle_ready(
            &attempt.attempt_id,
            attempt.row_version,
            &bundle_digest,
            now_ms()?,
        )
    }

    fn dispatch_attempt(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let starting = self.registry.mark_dispatch_issued(
            &attempt.attempt_id,
            attempt.row_version,
            now_ms()?,
        )?;
        let plan = self.registry.execution_plan(&starting.job_id)?;
        let bundle_path = canonical_directory(Path::new(&starting.bundle_path), "bundlePath")
            .map_err(map_universal_error)?;
        let runner = validate_runner(&self.executor.runner_path)?;
        let runtime_ceiling = plan.timeout_ms.saturating_add(5_000);
        let output = systemd_run(
            &starting.unit_name,
            &runner,
            &bundle_path,
            Path::new(&plan.workspace_path),
            runtime_ceiling,
        )?;
        if !output.status.success() {
            let detail = format!(
                "systemd-run failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            self.commit_control_terminal(
                &starting,
                AttemptState::Failed,
                "RUNNER_START_FAILED",
                Some(detail),
            )?;
            return Ok(());
        }
        self.await_launch_evidence(&starting)
    }

    fn await_launch_evidence(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let deadline = Instant::now() + Duration::from_millis(self.startup_grace_ms);
        loop {
            if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
                return self.reconcile_runner_result(attempt);
            }
            if Path::new(&attempt.bundle_path)
                .join(RUNNER_START_FILE)
                .exists()
            {
                match self.bind_runner_start(attempt) {
                    Ok(_) => return Ok(()),
                    Err(error) if error.code == RuntimeErrorCode::LaunchIdentityMismatch => {
                        // A very short-lived unit can write valid start evidence, finish, and be
                        // collected between the filesystem check and systemctl_show. A complete
                        // identity-bound Runner result is stronger terminal evidence than the
                        // already-disappeared transient unit.
                        if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
                            return self.reconcile_runner_result(attempt);
                        }
                        thread::sleep(Duration::from_millis(20));
                        if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
                            return self.reconcile_runner_result(attempt);
                        }
                        return Err(error);
                    }
                    Err(error) => return Err(error),
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.reconcile_attempt(&attempt.attempt_id)
    }

    fn bind_runner_start(&self, attempt: &AttemptRecord) -> RuntimeResult<AttemptRecord> {
        let path = Path::new(&attempt.bundle_path).join(RUNNER_START_FILE);
        let bytes =
            fs::read(&path).map_err(|error| io_error("read runner-start evidence", error))?;
        let evidence: RunnerStartEvidence = serde_json::from_slice(&bytes).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                format!("invalid runner-start evidence: {error}"),
                Some("runnerStart"),
                false,
            )
        })?;
        if evidence.job_id != attempt.job_id
            || evidence.attempt_id != attempt.attempt_id
            || evidence.unit_name != attempt.unit_name
            || evidence.launch_token_digest != attempt.launch_token_digest
            || !self.payload_evidence_matches(evidence.payload_uid, evidence.payload_gid)
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "runner-start identity does not match committed Attempt",
                Some("runnerStart"),
                false,
            ));
        }
        let properties = systemctl_show(&attempt.unit_name)?;
        require_property(&properties, "InvocationID", &evidence.invocation_id)?;
        require_property(&properties, "ControlGroup", &evidence.control_group)?;
        let main_pid: u32 = properties
            .get("MainPID")
            .ok_or_else(|| missing_systemd_property("MainPID"))?
            .parse()
            .map_err(|_| missing_systemd_property("MainPID"))?;
        if evidence.namespace_pid == 0 || evidence.namespace_process_start_identity.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "runner-start omitted PID namespace identity",
                Some("namespacePid"),
                false,
            ));
        }
        let process_start_identity = process_identity(main_pid).ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "systemd MainPID has no observable host process identity",
                Some("mainPid"),
                false,
            )
        })?;
        let runner_start_digest = sha256_bytes(&bytes);
        if runner_start_digest != sha256_file(&path).map_err(map_universal_error)? {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "runner-start evidence digest changed while reading",
                Some("runnerStart"),
                false,
            ));
        }
        let boot_id = read_trimmed("/proc/sys/kernel/random/boot_id")?;
        self.registry.bind_running(
            &attempt.attempt_id,
            attempt.row_version,
            &RunnerIdentity {
                boot_id,
                unit_name: evidence.unit_name,
                invocation_id: evidence.invocation_id,
                control_group: evidence.control_group,
                main_pid,
                process_start_identity,
                runner_start_digest,
                observed_at_ms: u64::try_from(evidence.observed_unix_ms).unwrap_or(u64::MAX),
            },
        )
    }

    fn reconcile_runner_result(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        match self.commit_runner_result(attempt) {
            Ok(_) => Ok(()),
            Err(error)
                if matches!(
                    error.code,
                    RuntimeErrorCode::RegistryCorrupt
                        | RuntimeErrorCode::ResultIdentityConflict
                        | RuntimeErrorCode::ArtifactIdentityConflict
                ) =>
            {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Orphaned,
                    "RUNNER_RESULT_QUARANTINED",
                    Some(error.to_string()),
                )?;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn commit_runner_result(&self, attempt: &AttemptRecord) -> RuntimeResult<TaskObservation> {
        let current = self.registry.get_attempt(&attempt.attempt_id)?;
        if current.state == AttemptState::Orphaned
            && self.recover_orphaned_runner_result(&current)?
        {
            return self.observation_from_registry(&current.job_id, 0, 0);
        }
        if current.state.is_terminal() {
            return self.observation_from_registry(&current.job_id, 0, 0);
        }
        let terminal = self.prepare_runner_terminal(&current)?;
        let projection = self.registry.commit_terminal(&terminal)?;
        self.cleanup_payload_view(&current.attempt_id)?;
        self.observation_from_parts(projection, Some(current), 4096, 4096)
    }

    fn prepare_runner_terminal(&self, current: &AttemptRecord) -> RuntimeResult<TerminalCommit> {
        let result_path = Path::new(&current.bundle_path).join(RESULT_FILE);
        let bytes =
            fs::read(&result_path).map_err(|error| io_error("read Runner result", error))?;
        let result: RunnerTaskResult = serde_json::from_slice(&bytes).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("invalid Runner result: {error}"),
                Some("result"),
                false,
            )
        })?;
        if result.task_id != current.attempt_id
            || result.job_id.as_deref() != Some(current.job_id.as_str())
            || result.attempt_id.as_deref() != Some(current.attempt_id.as_str())
            || result.launch_token_digest.as_deref() != Some(current.launch_token_digest.as_str())
            || !self.payload_evidence_matches(result.payload_uid, result.payload_gid)
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ResultIdentityConflict,
                "Runner result identity does not match committed Attempt",
                Some("result"),
                false,
            ));
        }
        let result_digest = sha256_bytes(&bytes);
        let stdout = self.validate_captured_output(current, &result.stdout, true)?;
        let stderr = self.validate_captured_output(current, &result.stderr, false)?;
        let (state, reason_code) = match result.status {
            TaskTerminalStatus::Completed => (AttemptState::Succeeded, "PROCESS_EXIT_ZERO"),
            TaskTerminalStatus::Failed if result.timed_out => {
                (AttemptState::TimedOut, "DEADLINE_EXCEEDED")
            }
            TaskTerminalStatus::Failed => (AttemptState::Failed, "PROCESS_EXIT_NONZERO"),
            TaskTerminalStatus::Cancelled => (AttemptState::Cancelled, "STOP_REQUESTED"),
        };
        let infrastructure_error_digest = result
            .infrastructure_error
            .as_deref()
            .map(|message| sha256_bytes(message.as_bytes()));
        let mut artifacts = vec![stdout, stderr];
        artifacts.push(ArtifactRegistration {
            artifact_id: format!("{}.result", current.attempt_id),
            kind: "execution_result".to_string(),
            relative_path: RESULT_FILE.to_string(),
            digest: result_digest.clone(),
            media_type: "application/json".to_string(),
            byte_length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            truncated: false,
        });
        Ok(TerminalCommit {
            attempt_id: current.attempt_id.clone(),
            expected_row_version: current.row_version,
            state,
            result_digest,
            exit_code: result.exit_code,
            infrastructure_error_digest,
            finished_at_ms: u64::try_from(result.finished_unix_ms).unwrap_or(u64::MAX),
            artifacts,
            reason_code: reason_code.to_string(),
        })
    }

    fn validate_captured_output(
        &self,
        attempt: &AttemptRecord,
        output: &CapturedOutput,
        stdout: bool,
    ) -> RuntimeResult<ArtifactRegistration> {
        let expected_file = if stdout { STDOUT_FILE } else { STDERR_FILE };
        let expected_kind = if stdout { "stdout" } else { "stderr" };
        let expected_id = format!("{}.{}", attempt.attempt_id, expected_kind);
        if output.file_name != expected_file || output.artifact_id != expected_id {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "Runner output identity does not match Attempt",
                Some("artifact"),
                false,
            ));
        }
        let path = Path::new(&attempt.bundle_path).join(expected_file);
        let metadata = fs::metadata(&path).map_err(|error| io_error("inspect output", error))?;
        let digest = sha256_file(&path).map_err(map_universal_error)?;
        if digest != output.digest || metadata.len() != output.retained_bytes {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "Runner output digest or byte length changed",
                Some("artifact"),
                false,
            ));
        }
        Ok(ArtifactRegistration {
            artifact_id: expected_id,
            kind: expected_kind.to_string(),
            relative_path: expected_file.to_string(),
            digest,
            media_type: "text/plain; charset=utf-8".to_string(),
            byte_length: metadata.len(),
            truncated: output.truncated,
        })
    }

    pub fn observe_task(&self, request: &TaskObserveRequest) -> RuntimeResult<TaskObservation> {
        validate_observe_request(request)?;
        let deadline = Instant::now() + Duration::from_millis(request.wait_ms);
        loop {
            self.reconcile_job(&request.job_id)?;
            let snapshot = self.registry.job_snapshot(&request.job_id)?;
            if snapshot.projection.result_available
                || request.wait_ms == 0
                || Instant::now() >= deadline
            {
                return self.observation_from_snapshot(
                    snapshot,
                    request.stdout_tail_bytes,
                    request.stderr_tail_bytes,
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn observation_from_registry(
        &self,
        job_id: &str,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> RuntimeResult<TaskObservation> {
        let snapshot = self.registry.job_snapshot(job_id)?;
        self.observation_from_snapshot(snapshot, stdout_tail_bytes, stderr_tail_bytes)
    }

    fn observation_from_snapshot(
        &self,
        snapshot: JobSnapshot,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> RuntimeResult<TaskObservation> {
        self.observation_from_parts(
            snapshot.projection,
            snapshot.attempt,
            stdout_tail_bytes,
            stderr_tail_bytes,
        )
    }

    fn observation_from_parts(
        &self,
        projection: super::JobProjection,
        attempt: Option<AttemptRecord>,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> RuntimeResult<TaskObservation> {
        let job_id = projection.job_id.clone();
        let (
            stdout_tail,
            stderr_tail,
            stdout_truncated,
            stderr_truncated,
            artifacts,
            error_summary,
        ) = if let Some(attempt) = &attempt {
            let stdout_tail = read_tail_text(
                &Path::new(&attempt.bundle_path).join(STDOUT_FILE),
                stdout_tail_bytes,
            )?;
            let stderr_tail = read_tail_text(
                &Path::new(&attempt.bundle_path).join(STDERR_FILE),
                stderr_tail_bytes,
            )?;
            let (result, result_error) = match load_runner_result_if_present(attempt) {
                Ok(result) => (result, None),
                Err(error) => (None, Some(error.to_string())),
            };
            let stdout_truncated = result
                .as_ref()
                .is_some_and(|result| result.stdout.truncated);
            let stderr_truncated = result
                .as_ref()
                .is_some_and(|result| result.stderr.truncated);
            let artifacts = self
                .registry
                .list_artifacts(&job_id)?
                .into_iter()
                .map(|artifact| artifact_descriptor(artifact, result.as_ref()))
                .collect();
            let error_summary = result
                .as_ref()
                .and_then(|result| result.infrastructure_error.clone())
                .or(result_error);
            (
                stdout_tail,
                stderr_tail,
                stdout_truncated,
                stderr_truncated,
                artifacts,
                error_summary,
            )
        } else {
            (String::new(), String::new(), false, false, Vec::new(), None)
        };
        Ok(TaskObservation {
            job_id,
            status: projection.status,
            attempt_id: attempt.map(|attempt| attempt.attempt_id),
            exit_code: projection.exit_code,
            stdout_tail,
            stderr_tail,
            stdout_truncated,
            stderr_truncated,
            artifacts_available: projection.artifacts_available,
            artifacts,
            poll_after_ms: projection.poll_after_ms,
            error_summary,
        })
    }

    fn reconcile_job(&self, job_id: &str) -> RuntimeResult<()> {
        let snapshot = self.registry.job_snapshot(job_id)?;
        if snapshot.job.resolution.is_some() {
            if snapshot.job.resolution == Some(JobResolution::Orphaned) {
                if let Some(attempt) = snapshot.attempt {
                    let _ = self.recover_orphaned_runner_result(&attempt)?;
                }
            }
            return Ok(());
        }
        let attempt = snapshot.attempt.ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "unresolved Job has no Attempt",
                Some("jobId"),
                false,
            )
        })?;
        if attempt.state == AttemptState::Accepted {
            return self.ensure_attempt_dispatched(&attempt);
        }
        self.reconcile_attempt(&attempt.attempt_id)
    }

    pub fn reconcile_all(&self) -> RuntimeResult<Vec<TaskObservation>> {
        self.reconcile_recoverable_orphans()?;
        let attempts = self.registry.list_nonterminal_attempts()?;
        let mut observations = Vec::with_capacity(attempts.len());
        for attempt in attempts {
            if attempt.state == AttemptState::Accepted {
                self.ensure_attempt_dispatched(&attempt)?;
            } else {
                self.reconcile_attempt(&attempt.attempt_id)?;
            }
            observations.push(self.observation_from_registry(&attempt.job_id, 0, 0)?);
        }
        Ok(observations)
    }

    pub fn reconcile_recoverable_orphans(&self) -> RuntimeResult<Vec<String>> {
        let mut recovered = Vec::new();
        for attempt in self.registry.list_held_orphaned_attempts()? {
            if !Path::new(&attempt.bundle_path).join(RESULT_FILE).is_file() {
                continue;
            }
            match self.recover_orphaned_runner_result(&attempt) {
                Ok(true) => recovered.push(attempt.attempt_id),
                Ok(false) => {}
                Err(error)
                    if matches!(
                        error.code,
                        RuntimeErrorCode::ResultIdentityConflict
                            | RuntimeErrorCode::ArtifactIdentityConflict
                    ) || (error.code == RuntimeErrorCode::RegistryCorrupt
                        && error.field.as_deref() == Some("result")) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(recovered)
    }

    fn recover_orphaned_runner_result(&self, attempt: &AttemptRecord) -> RuntimeResult<bool> {
        let current = self.registry.get_attempt(&attempt.attempt_id)?;
        if current.state != AttemptState::Orphaned
            || !Path::new(&current.bundle_path).join(RESULT_FILE).is_file()
            || self.orphan_process_tree_alive(&current)?
        {
            return Ok(false);
        }
        let mut terminal = self.prepare_runner_terminal(&current)?;
        terminal.reason_code = "LATE_IDENTITY_BOUND_RUNNER_RESULT".to_string();
        self.registry.recover_orphaned_terminal(&terminal)?;
        self.cleanup_payload_view(&current.attempt_id)?;
        Ok(true)
    }

    fn orphan_process_tree_alive(&self, attempt: &AttemptRecord) -> RuntimeResult<bool> {
        let properties = systemctl_show(&attempt.unit_name)?;
        let matching_unit_active = unit_is_active(&properties)
            && attempt
                .invocation_id
                .as_deref()
                .zip(nonempty_property(&properties, "InvocationID").as_deref())
                .is_some_and(|(expected, observed)| expected == observed);
        let recorded_pid_alive = attempt.main_pid.is_some_and(|pid| {
            process_identity(pid)
                .as_deref()
                .zip(attempt.process_start_identity.as_deref())
                .is_some_and(|(observed, expected)| observed == expected)
        });
        let cgroup_alive = attempt
            .control_group
            .as_deref()
            .map(cgroup_has_processes)
            .transpose()?
            .unwrap_or(false);
        Ok(matching_unit_active || recorded_pid_alive || cgroup_alive)
    }

    pub fn reconcile_attempt(&self, attempt_id: &str) -> RuntimeResult<()> {
        let attempt = self.registry.get_attempt(attempt_id)?;
        if attempt.state.is_terminal() {
            return Ok(());
        }
        let result_path = Path::new(&attempt.bundle_path).join(RESULT_FILE);
        if result_path.exists() {
            return self.reconcile_runner_result(&attempt);
        }
        let runner_start_path = Path::new(&attempt.bundle_path).join(RUNNER_START_FILE);
        if attempt.state == AttemptState::Starting && runner_start_path.exists() {
            let running = self.bind_runner_start(&attempt)?;
            if Path::new(&running.bundle_path).join(RESULT_FILE).exists() {
                return self.reconcile_runner_result(&running);
            }
            return Ok(());
        }
        if attempt.state == AttemptState::Starting {
            return self.reconcile_starting_without_token(&attempt);
        }
        self.reconcile_bound_attempt(&attempt)
    }

    fn reconcile_starting_without_token(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let properties = systemctl_show(&attempt.unit_name)?;
        let active = unit_is_active(&properties);
        let age_ms = now_ms()?.saturating_sub(attempt.created_at_ms);
        if active && age_ms < self.startup_grace_ms {
            return Ok(());
        }
        if active {
            self.commit_control_terminal(
                attempt,
                AttemptState::Orphaned,
                "LIVE_UNIT_WITHOUT_LAUNCH_TOKEN_EVIDENCE",
                Some("systemd unit is live but runner-start identity is unavailable".to_string()),
            )?;
            return Ok(());
        }
        if age_ms < self.startup_grace_ms {
            return Ok(());
        }
        self.commit_control_terminal(
            attempt,
            AttemptState::Lost,
            "DISPATCH_OUTCOME_UNKNOWN",
            Some(
                "dispatch intent exists without matching unit, runner-start, or result evidence"
                    .to_string(),
            ),
        )?;
        Ok(())
    }

    fn reconcile_bound_attempt(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let expected = supervisor_identity(attempt)?;
        let properties = systemctl_show(&attempt.unit_name)?;
        let current_boot_id = read_trimmed("/proc/sys/kernel/random/boot_id")?;
        let unit_state = if unit_is_active(&properties) {
            SupervisorUnitState::Running
        } else if properties
            .get("LoadState")
            .is_some_and(|state| state == "not-found")
        {
            SupervisorUnitState::NotFound
        } else {
            SupervisorUnitState::Terminal
        };
        let recorded_pid_alive = process_identity(attempt.main_pid.unwrap_or_default())
            .is_some_and(|identity| {
                attempt.process_start_identity.as_deref() == Some(identity.as_str())
            });
        let observation = SupervisorObservation {
            boot_id: current_boot_id,
            unit_state,
            invocation_id: nonempty_property(&properties, "InvocationID"),
            control_group: nonempty_property(&properties, "ControlGroup"),
            main_pid: properties
                .get("MainPID")
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|pid| *pid > 0),
            main_process_start_identity: properties
                .get("MainPID")
                .and_then(|value| value.parse::<u32>().ok())
                .and_then(process_identity),
            recorded_pid_alive,
            recorded_pid_start_identity: attempt.main_pid.and_then(process_identity),
            result: nonempty_property(&properties, "Result"),
            exec_main_code: properties
                .get("ExecMainCode")
                .and_then(|value| value.parse().ok()),
            exec_main_status: properties
                .get("ExecMainStatus")
                .and_then(|value| value.parse().ok()),
        };
        let intent = match attempt.termination_intent {
            super::AttemptTerminationIntent::Natural => TerminationIntent::Natural,
            super::AttemptTerminationIntent::StopRequested => TerminationIntent::StopRequested,
            super::AttemptTerminationIntent::DeadlineExceeded => {
                TerminationIntent::DeadlineExceeded
            }
        };
        if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
            return self.reconcile_runner_result(attempt);
        }
        let disposition =
            classify_supervisor_recovery(&expected, &observation, intent).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    format!("supervisor recovery classification failed: {error}"),
                    Some("attemptId"),
                    false,
                )
            })?;
        match disposition {
            SupervisorRecoveryDisposition::Running => Ok(()),
            SupervisorRecoveryDisposition::Terminal(state) => {
                self.commit_control_terminal(attempt, state, "SUPERVISOR_TERMINAL_FALLBACK", None)?;
                Ok(())
            }
            SupervisorRecoveryDisposition::Lost => {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Lost,
                    "SUPERVISOR_EVIDENCE_LOST",
                    None,
                )?;
                Ok(())
            }
            SupervisorRecoveryDisposition::Orphaned(reason) => {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Orphaned,
                    "SUPERVISOR_IDENTITY_ORPHANED",
                    Some(reason),
                )?;
                let current = self.registry.get_attempt(&attempt.attempt_id)?;
                if Path::new(&current.bundle_path).join(RESULT_FILE).exists() {
                    let _ = self.recover_orphaned_runner_result(&current)?;
                }
                Ok(())
            }
        }
    }

    fn commit_control_terminal(
        &self,
        attempt: &AttemptRecord,
        state: AttemptState,
        reason_code: &str,
        detail: Option<String>,
    ) -> RuntimeResult<TaskObservation> {
        let current = self.registry.get_attempt(&attempt.attempt_id)?;
        if current.state.is_terminal() {
            return self.observation_from_registry(&current.job_id, 0, 0);
        }
        let observed_at_ms = now_ms()?;
        let evidence = ControlTerminalEvidence {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: current.job_id.clone(),
            attempt_id: current.attempt_id.clone(),
            status: state.as_db().to_string(),
            reason_code: reason_code.to_string(),
            detail: detail
                .as_ref()
                .map(|value| value.chars().take(4096).collect()),
            observed_at_ms,
        };
        let evidence_path = Path::new(&current.bundle_path).join(CONTROL_RESULT_FILE);
        if let Some(parent) = evidence_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create control evidence directory", error))?;
        }
        write_json_atomic(&evidence_path, &evidence).map_err(map_universal_error)?;
        let result_digest = sha256_file(&evidence_path).map_err(map_universal_error)?;
        let mut artifacts = vec![ArtifactRegistration {
            artifact_id: format!("{}.control-result", current.attempt_id),
            kind: "control_result".to_string(),
            relative_path: CONTROL_RESULT_FILE.to_string(),
            digest: result_digest.clone(),
            media_type: "application/json".to_string(),
            byte_length: fs::metadata(&evidence_path)
                .map_err(|error| io_error("inspect control evidence", error))?
                .len(),
            truncated: false,
        }];
        if state != AttemptState::Orphaned {
            for (file_name, kind) in [(STDOUT_FILE, "stdout"), (STDERR_FILE, "stderr")] {
                let path = Path::new(&current.bundle_path).join(file_name);
                if path.is_file() {
                    artifacts.push(ArtifactRegistration {
                        artifact_id: format!("{}.{}", current.attempt_id, kind),
                        kind: kind.to_string(),
                        relative_path: file_name.to_string(),
                        digest: sha256_file(&path).map_err(map_universal_error)?,
                        media_type: "text/plain; charset=utf-8".to_string(),
                        byte_length: fs::metadata(&path)
                            .map_err(|error| io_error("inspect control output", error))?
                            .len(),
                        truncated: false,
                    });
                }
            }
        }
        let projection = self.registry.commit_terminal(&TerminalCommit {
            attempt_id: current.attempt_id.clone(),
            expected_row_version: current.row_version,
            state,
            result_digest,
            exit_code: None,
            infrastructure_error_digest: detail
                .as_deref()
                .map(|value| sha256_bytes(value.as_bytes())),
            finished_at_ms: observed_at_ms,
            artifacts,
            reason_code: reason_code.to_string(),
        })?;
        if state != AttemptState::Orphaned {
            self.cleanup_payload_view(&current.attempt_id)?;
        }
        self.observation_from_parts(projection, Some(current), 4096, 4096)
    }

    pub fn cancel_task(&self, request: &TaskCancelRequest) -> RuntimeResult<TaskObservation> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(RuntimeError::invalid(
                "unsupported runtime schema version",
                "schemaVersion",
            ));
        }
        let projection = self.registry.request_cancel(&request.job_id, now_ms()?)?;
        if projection.result_available {
            return self.observation_from_registry(&request.job_id, 4096, 4096);
        }
        let attempt = self
            .registry
            .get_latest_attempt(&request.job_id)?
            .ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "cancelled Job has no Attempt",
                    Some("jobId"),
                    false,
                )
            })?;
        write_json_atomic(
            &Path::new(&attempt.bundle_path).join(CANCEL_FILE),
            &serde_json::json!({
                "schemaVersion": RUNTIME_SCHEMA_VERSION,
                "jobId": request.job_id,
                "attemptId": attempt.attempt_id,
                "requestedAtMs": now_ms()?,
            }),
        )
        .map_err(map_universal_error)?;
        let output = Command::new("systemctl")
            .args(["stop", &attempt.unit_name])
            .output()
            .map_err(|error| tool_error("cannot execute systemctl stop", error))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
                return self.commit_runner_result(&attempt);
            }
            let properties = systemctl_show(&attempt.unit_name)?;
            let recorded_alive =
                attempt
                    .main_pid
                    .and_then(process_identity)
                    .is_some_and(|identity| {
                        attempt.process_start_identity.as_deref() == Some(identity.as_str())
                    });
            if !unit_is_active(&properties) && !recorded_alive {
                return self.commit_control_terminal(
                    &attempt,
                    AttemptState::Cancelled,
                    "STOP_REQUESTED_PROCESS_TREE_GONE",
                    (!output.status.success())
                        .then(|| String::from_utf8_lossy(&output.stderr).trim().to_string()),
                );
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        self.reconcile_attempt(&attempt.attempt_id)?;
        self.observation_from_registry(&request.job_id, 4096, 4096)
    }

    pub fn list_jobs(
        &self,
        request: &RuntimeJobListRequest,
    ) -> RuntimeResult<RuntimeJobListResult> {
        self.reconcile_recoverable_orphans()?;
        let mut result = self.registry.list_jobs(request)?;
        for job in &mut result.jobs {
            job.artifacts = self
                .registry
                .list_artifacts(&job.job_id)?
                .into_iter()
                .map(|artifact| artifact_descriptor(artifact, None))
                .collect();
        }
        Ok(result)
    }

    pub fn read_artifact(
        &self,
        request: &ArtifactReadRequest,
    ) -> RuntimeResult<ArtifactReadResult> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(RuntimeError::invalid(
                "unsupported runtime schema version",
                "schemaVersion",
            ));
        }
        if request.max_bytes == 0 || request.max_bytes > MAX_ARTIFACT_READ_BYTES {
            return Err(RuntimeError::invalid(
                format!("maxBytes must be in 1..={MAX_ARTIFACT_READ_BYTES}"),
                "maxBytes",
            ));
        }
        let artifact = self
            .registry
            .get_artifact(&request.job_id, &request.artifact_id)?;
        let attempt = self.registry.get_attempt(&artifact.attempt_id)?;
        let bundle = canonical_directory(Path::new(&attempt.bundle_path), "bundlePath")
            .map_err(map_universal_error)?;
        let path = bundle.join(&artifact.relative_path);
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| io_error("inspect Artifact", error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "Artifact path is not a regular non-symlink file",
                Some("artifactId"),
                false,
            ));
        }
        let canonical =
            fs::canonicalize(&path).map_err(|error| io_error("canonicalize Artifact", error))?;
        if !canonical.starts_with(&bundle) {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "Artifact escaped Attempt bundle",
                Some("artifactId"),
                false,
            ));
        }
        if sha256_file(&canonical).map_err(map_universal_error)? != artifact.digest
            || metadata.len() != artifact.byte_length
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "Artifact digest or byte length changed",
                Some("artifactId"),
                false,
            ));
        }
        let mut file = File::open(&canonical).map_err(|error| io_error("open Artifact", error))?;
        file.seek(SeekFrom::Start(request.offset))
            .map_err(|error| io_error("seek Artifact", error))?;
        let mut bytes = vec![0_u8; usize::try_from(request.max_bytes).unwrap_or(usize::MAX)];
        let read = file
            .read(&mut bytes)
            .map_err(|error| io_error("read Artifact", error))?;
        bytes.truncate(read);
        let next_offset = request.offset.saturating_add(read as u64);
        Ok(ArtifactReadResult {
            job_id: request.job_id.clone(),
            artifact_id: request.artifact_id.clone(),
            content: String::from_utf8_lossy(&bytes).into_owned(),
            offset: request.offset,
            next_offset,
            eof: next_offset >= artifact.byte_length,
            digest: artifact.digest,
        })
    }

    fn cleanup_payload_view(&self, _attempt_id: &str) -> RuntimeResult<()> {
        Ok(())
    }

    fn payload_config(
        &self,
        _attempt_id: &str,
        _plan: &RuntimeExecutionPlan,
    ) -> RuntimeResult<Option<RunnerPayloadConfig>> {
        Ok(None)
    }

    fn payload_evidence_matches(&self, uid: Option<u32>, gid: Option<u32>) -> bool {
        uid.is_none() && gid.is_none()
    }
}

fn artifact_descriptor(
    artifact: RuntimeArtifactRecord,
    result: Option<&RunnerTaskResult>,
) -> ArtifactDescriptor {
    let dropped_bytes = match artifact.kind.as_str() {
        "stdout" => result.map(|result| result.stdout.dropped_bytes),
        "stderr" => result.map(|result| result.stderr.dropped_bytes),
        _ => None,
    };
    ArtifactDescriptor {
        artifact_id: artifact.artifact_id,
        kind: artifact.kind,
        digest: artifact.digest,
        retained_bytes: artifact.byte_length,
        dropped_bytes,
        truncated: artifact.truncated,
    }
}

fn validate_run_request(request: &TaskRunRequest) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err(RuntimeError::invalid(
            "unsupported runtime schema version",
            "schemaVersion",
        ));
    }
    for (value, field) in [
        (&request.client_request_id, "clientRequestId"),
        (&request.principal, "principal"),
        (&request.execution.workspace_id, "execution.workspaceId"),
    ] {
        validate_text_id(value, field)?;
    }
    if request.global_limit == 0 {
        return Err(RuntimeError::invalid(
            "concurrency limits must be positive",
            "globalLimit",
        ));
    }
    if request.wait_ms > MAX_TASK_WAIT_MS
        || request.stdout_tail_bytes > MAX_TASK_TAIL_BYTES
        || request.stderr_tail_bytes > MAX_TASK_TAIL_BYTES
    {
        return Err(RuntimeError::invalid(
            "wait or tail bounds exceed the runtime compact limit",
            "waitMs",
        ));
    }
    if request.execution.executable.is_empty()
        || !Path::new(&request.execution.executable).is_absolute()
        || request.execution.cwd_relative.is_empty()
        || Path::new(&request.execution.cwd_relative).is_absolute()
    {
        return Err(RuntimeError::invalid(
            "executable must be absolute and cwdRelative must be relative",
            "execution",
        ));
    }
    if request
        .execution
        .cwd_relative
        .split('/')
        .any(|part| part == "..")
    {
        return Err(RuntimeError::invalid(
            "cwdRelative cannot contain parent traversal",
            "execution.cwdRelative",
        ));
    }
    if request.execution.timeout_ms == 0
        || request.execution.stdout_limit_bytes == 0
        || request.execution.stderr_limit_bytes == 0
    {
        return Err(RuntimeError::invalid(
            "runtime and output limits must be positive",
            "execution",
        ));
    }
    if request.execution.args.len() > 128 || request.execution.env.len() > 64 {
        return Err(RuntimeError::invalid(
            "args or environment exceed runtime bounds",
            "execution",
        ));
    }
    Ok(())
}

fn validate_observe_request(request: &TaskObserveRequest) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err(RuntimeError::invalid(
            "unsupported runtime schema version",
            "schemaVersion",
        ));
    }
    validate_text_id(&request.job_id, "jobId")?;
    if request.wait_ms > MAX_TASK_WAIT_MS
        || request.stdout_tail_bytes > MAX_TASK_TAIL_BYTES
        || request.stderr_tail_bytes > MAX_TASK_TAIL_BYTES
    {
        return Err(RuntimeError::invalid(
            "observe bounds exceed runtime limits",
            "waitMs",
        ));
    }
    Ok(())
}

fn validate_executable(config: &UniversalExecutorConfig, value: &str) -> RuntimeResult<PathBuf> {
    let path = Path::new(value);
    let canonical =
        fs::canonicalize(path).map_err(|error| io_error("canonicalize executable", error))?;
    let metadata =
        fs::metadata(&canonical).map_err(|error| io_error("inspect executable", error))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(RuntimeError::invalid(
            "executable must resolve to an executable file",
            "execution.executable",
        ));
    }
    let allowed = config.allowed_executable_roots.iter().any(|root| {
        fs::canonicalize(root)
            .map(|root| canonical.starts_with(root))
            .unwrap_or(false)
    });
    if !allowed {
        return Err(RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            "executable is outside configured roots",
            Some("execution.executable"),
            false,
        ));
    }
    Ok(canonical)
}

fn validate_runner(path: &Path) -> RuntimeResult<PathBuf> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error("inspect Runner", error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(RuntimeError::invalid(
            "Runner must be a non-symlink executable file",
            "runnerPath",
        ));
    }
    fs::canonicalize(path).map_err(|error| io_error("canonicalize Runner", error))
}

fn build_systemd_run_command(
    unit_name: &str,
    runner: &Path,
    bundle_path: &Path,
    _workspace_path: &Path,
    runtime_ceiling_ms: u64,
) -> RuntimeResult<Command> {
    let mut command = Command::new("systemd-run");
    command
        .arg(format!("--unit={unit_name}"))
        .arg("--collect")
        .args([
            "--property=Type=exec",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=2s",
            "--property=SendSIGKILL=yes",
            "--property=StandardOutput=journal",
            "--property=StandardError=journal",
        ])
        .arg(format!("--property=RuntimeMaxSec={runtime_ceiling_ms}ms"));

    append_trusted_environment(&mut command);

    command.arg(runner).arg("--task-dir").arg(bundle_path);
    Ok(command)
}

fn append_trusted_environment(command: &mut Command) {
    for (name, value) in std::env::vars_os() {
        let Some(name) = name.to_str() else {
            continue;
        };
        if !valid_environment_name(name)
            || name.starts_with("ORDIVON_")
            || matches!(
                name,
                "INVOCATION_ID"
                    | "JOURNAL_STREAM"
                    | "LISTEN_FDS"
                    | "LISTEN_FDNAMES"
                    | "LISTEN_PID"
                    | "NOTIFY_SOCKET"
                    | "WATCHDOG_PID"
                    | "WATCHDOG_USEC"
            )
        {
            continue;
        }
        command.arg(format!("--setenv={name}={}", value.to_string_lossy()));
    }
}

fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first == b'_' || first.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

fn systemd_run(
    unit_name: &str,
    runner: &Path,
    bundle_path: &Path,
    workspace_path: &Path,
    runtime_ceiling_ms: u64,
) -> RuntimeResult<std::process::Output> {
    build_systemd_run_command(
        unit_name,
        runner,
        bundle_path,
        workspace_path,
        runtime_ceiling_ms,
    )?
    .output()
    .map_err(|error| tool_error("cannot execute systemd-run", error))
}

fn systemctl_show(unit_name: &str) -> RuntimeResult<BTreeMap<String, String>> {
    let output = Command::new("systemctl")
        .args([
            "show",
            unit_name,
            "--property=LoadState,ActiveState,SubState,InvocationID,ControlGroup,MainPID,Result,ExecMainCode,ExecMainStatus",
        ])
        .output()
        .map_err(|error| tool_error("cannot execute systemctl show", error))?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ToolFailed,
            format!(
                "systemctl show failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            None,
            true,
        ));
    }
    let mut properties = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            properties.insert(key.to_string(), value.to_string());
        }
    }
    properties
        .entry("LoadState".to_string())
        .or_insert_with(|| "not-found".to_string());
    properties
        .entry("ActiveState".to_string())
        .or_insert_with(|| "inactive".to_string());
    Ok(properties)
}

fn unit_is_active(properties: &BTreeMap<String, String>) -> bool {
    properties
        .get("ActiveState")
        .is_some_and(|state| matches!(state.as_str(), "active" | "activating" | "reloading"))
}

fn nonempty_property(properties: &BTreeMap<String, String>, key: &str) -> Option<String> {
    properties
        .get(key)
        .filter(|value| !value.is_empty())
        .cloned()
}

fn require_property(
    properties: &BTreeMap<String, String>,
    key: &str,
    expected: &str,
) -> RuntimeResult<()> {
    if properties.get(key).map(String::as_str) != Some(expected) {
        return Err(RuntimeError::new(
            RuntimeErrorCode::LaunchIdentityMismatch,
            format!("systemd {key} does not match runner-start evidence"),
            Some(key),
            false,
        ));
    }
    Ok(())
}

fn missing_systemd_property(key: &str) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::LaunchIdentityMismatch,
        format!("systemd omitted {key}"),
        Some(key),
        false,
    )
}

fn supervisor_identity(attempt: &AttemptRecord) -> RuntimeResult<SupervisorIdentity> {
    Ok(SupervisorIdentity {
        boot_id: attempt.boot_id.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no bootId",
                Some("bootId"),
                false,
            )
        })?,
        unit_name: attempt.unit_name.clone(),
        invocation_id: attempt.invocation_id.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no invocationId",
                Some("invocationId"),
                false,
            )
        })?,
        control_group: attempt.control_group.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no controlGroup",
                Some("controlGroup"),
                false,
            )
        })?,
        main_pid: attempt.main_pid.ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no mainPid",
                Some("mainPid"),
                false,
            )
        })?,
        main_process_start_identity: attempt.process_start_identity.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "bound Attempt has no process start identity",
                Some("processStartIdentity"),
                false,
            )
        })?,
    })
}

fn cgroup_has_processes(control_group: &str) -> RuntimeResult<bool> {
    if !control_group.starts_with('/')
        || control_group
            .split('/')
            .any(|part| part == ".." || part.contains('\0'))
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "recorded cgroup path is invalid",
            Some("controlGroup"),
            false,
        ));
    }
    let path = Path::new("/sys/fs/cgroup")
        .join(control_group.trim_start_matches('/'))
        .join("cgroup.procs");
    if !path.is_file() {
        return Ok(false);
    }
    let content = fs::read_to_string(path)
        .map_err(|error| io_error("read cgroup process membership", error))?;
    Ok(content
        .lines()
        .any(|line| line.trim().parse::<u32>().is_ok()))
}

fn process_identity(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = stat.rfind(')')?;
    stat[close + 1..]
        .split_whitespace()
        .nth(19)
        .map(ToString::to_string)
}

fn read_trimmed(path: &str) -> RuntimeResult<String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| io_error(&format!("read {path}"), error))
}

fn load_runner_result_if_present(
    attempt: &AttemptRecord,
) -> RuntimeResult<Option<RunnerTaskResult>> {
    let path = Path::new(&attempt.bundle_path).join(RESULT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| io_error("read Runner result", error))?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("invalid Runner result: {error}"),
            Some("result"),
            false,
        )
    })
}

fn read_tail_text(path: &Path, max_bytes: u64) -> RuntimeResult<String> {
    if max_bytes == 0 || !path.exists() {
        return Ok(String::new());
    }
    let mut file = File::open(path).map_err(|error| io_error("open output tail", error))?;
    let length = file
        .metadata()
        .map_err(|error| io_error("inspect output tail", error))?
        .len();
    let offset = length.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| io_error("seek output tail", error))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| io_error("read output tail", error))?;
    while offset > 0 && !bytes.is_empty() && std::str::from_utf8(&bytes).is_err() {
        bytes.remove(0);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn write_bytes_synced(path: &Path, bytes: &[u8]) -> RuntimeResult<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| io_error("create bundle file", error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write bundle file", error))?;
    file.sync_all()
        .map_err(|error| io_error("sync bundle file", error))
}

fn sync_directory(path: &Path) -> RuntimeResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| io_error("sync directory", error))
}

fn now_ms() -> RuntimeResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                format!("system clock precedes Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis()
        .try_into()
        .map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                "current time does not fit u64 milliseconds",
                None,
                false,
            )
        })
}

fn validate_text_id(value: &str, field: &str) -> RuntimeResult<()> {
    if value.trim().is_empty()
        || value.len() > 256
        || value.as_bytes().contains(&0)
        || value.chars().any(char::is_control)
    {
        return Err(RuntimeError::invalid(
            format!("{field} must be non-empty, bounded, and control-free"),
            field,
        ));
    }
    Ok(())
}

fn serialization_error(error: serde_json::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::RegistryUnavailable,
        format!("cannot serialize runtime bundle: {error}"),
        None,
        false,
    )
}

fn map_universal_error(error: crate::UniversalExecError) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::InvalidRequest,
        error.message,
        error.field.as_deref(),
        error.retryable,
    )
}

fn io_error(context: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IoError,
        format!("{context}: {error}"),
        None,
        false,
    )
}

fn tool_error(context: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::ToolUnavailable,
        format!("{context}: {error}"),
        None,
        true,
    )
}

#[cfg(test)]
mod trusted_systemd_command_tests {
    use super::*;

    #[test]
    fn trusted_runtime_accepts_temporary_storage_roots() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-runtime-temp-root-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let runtime = Runtime::new(RuntimeConfig {
            registry: RegistryConfig {
                db_path: root.join("registry/registry.sqlite3"),
                store_root: root.join("registry"),
                busy_timeout_ms: 5_000,
            },
            executor: UniversalExecutorConfig {
                store_root: root.join("runtime"),
                workspace_root: None,
                workspace_uid: None,
                workspace_gid: None,
                runner_path: PathBuf::from("/usr/bin/true"),
                allowed_executable_roots: vec![PathBuf::from("/")],
                max_runtime_ms: 60_000,
                max_output_bytes: 1_048_576,
            },
            startup_grace_ms: 2_000,
        })
        .unwrap();
        drop(runtime);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trusted_command_keeps_only_process_ownership_and_lifecycle_properties() {
        let command = build_systemd_run_command(
            "ordivon-test.service",
            Path::new("/usr/bin/true"),
            Path::new("/var/lib/ordivon/attempts/attempt-test"),
            Path::new("/root/projects/Ordivon"),
            10_000,
        )
        .unwrap();
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        for forbidden in [
            "PrivateNetwork",
            "ProtectSystem",
            "InaccessiblePaths",
            "CapabilityBoundingSet",
            "NoNewPrivileges",
            "ReadWritePaths",
            "MemoryMax",
            "TasksMax",
            "UMask",
        ] {
            assert!(
                !args.contains(forbidden),
                "trusted command contains {forbidden}"
            );
        }
        assert!(args.contains("KillMode=control-group"));
        assert!(args.contains("RuntimeMaxSec=10000ms"));
        assert!(valid_environment_name("GITHUB_TOKEN"));
        assert!(valid_environment_name("CARGO_BIN_EXE_ordivon_job_fixture"));
        assert!(!valid_environment_name("CARGO_BIN_EXE_ordivon-job-fixture"));
    }
}
