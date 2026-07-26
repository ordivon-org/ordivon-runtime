use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::evidence::prepare_runner_terminal_from_bundle;
use super::registry::JobSnapshot;
use super::supervisor::{
    classify_supervisor_recovery, SupervisorIdentity, SupervisorObservation,
    SupervisorRecoveryDisposition, SupervisorUnitState, TerminationIntent,
};
use super::{
    AdmissionOutcome, ArtifactDescriptor, ArtifactReadRequest, ArtifactReadResult,
    ArtifactRegistration, AttemptRecord, AttemptState, AttemptTerminationIntent, JobDesiredState,
    JobResolution, Registry, RegistryConfig, RunnerIdentity, RuntimeArtifactRecord, RuntimeError,
    RuntimeErrorCode, RuntimeExecutionPlan, RuntimeJobListRequest, RuntimeJobListResult,
    RuntimeResult, SubmitRequest, TaskCancelRequest, TaskObservation, TaskObserveRequest,
    TaskRunRequest, TerminalCommit, MAX_ARTIFACT_READ_BYTES, MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS,
    RUNTIME_SCHEMA_VERSION,
};
use crate::universal::{
    canonical_directory, create_git_workspace_compact, load_workspace_record, mutate_workspace,
    remove_git_workspace, resolve_workspace_cwd, sha256_bytes, sha256_file,
    workspace_source_state_digest, write_json_atomic, CompactWorkspaceOpenResult,
    GitWorkspaceCreateRequest, RunnerPayloadConfig, RunnerStartEvidence, RunnerTaskRequest,
    RunnerTaskResult, UniversalExecutorConfig, WorkspaceCloseRequest, WorkspaceCloseResult,
    WorkspaceMutateRequest, WorkspaceMutateResult, UNIVERSAL_EXEC_SCHEMA_VERSION,
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
const ORPHAN_REMEDIATION_FILE: &str = "orphan-remediation.json";
const INTERACTIVE_RECONCILIATION_LIMIT: u32 = 32;
const ADAPTIVE_POLL_DELAYS_MS: [u64; 5] = [2, 5, 10, 20, 50];

fn adaptive_poll_delay(poll_index: usize) -> Duration {
    Duration::from_millis(
        ADAPTIVE_POLL_DELAYS_MS[poll_index.min(ADAPTIVE_POLL_DELAYS_MS.len() - 1)],
    )
}

fn sleep_until_poll(deadline: Instant, poll_index: &mut usize) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return;
    }
    thread::sleep(adaptive_poll_delay(*poll_index).min(remaining));
    *poll_index = poll_index.saturating_add(1);
}

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
    lifecycle_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationFailure {
    pub attempt_id: String,
    pub job_id: String,
    pub code: RuntimeErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconciliationReport {
    pub inspected: usize,
    pub reconciled: usize,
    pub recovered_orphans: usize,
    pub quarantined: usize,
    pub unchanged: usize,
    pub failed: usize,
    pub failures: Vec<ReconciliationFailure>,
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
            lifecycle_lock: Arc::new(Mutex::new(())),
        };
        runtime.reconcile_recoverable_orphans()?;
        Ok(runtime)
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    fn lock_lifecycle(&self) -> RuntimeResult<MutexGuard<'_, ()>> {
        self.lifecycle_lock.lock().map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                "Workspace lifecycle lock is poisoned",
                None,
                true,
            )
        })
    }

    pub fn run_task(&self, request: &TaskRunRequest) -> RuntimeResult<TaskObservation> {
        validate_run_request(request, self.executor.max_output_bytes)?;
        let job_id = {
            let _guard = self.lifecycle_lock.lock().map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryUnavailable,
                    "Workspace lifecycle lock is poisoned",
                    None,
                    true,
                )
            })?;
            let request_identity_digest = super::operation_request_identity_digest(request)?;
            if let Some(existing) = self.registry.find_idempotent_job(
                &request.principal,
                &request.client_request_id,
                &request_identity_digest,
            )? {
                existing.job_id
            } else {
                self.reconcile_recoverable_orphans()?;
                let _ = self.reconcile_workspace(&request.execution.workspace_id)?;
                let plan = self.resolve_plan(request)?;
                let submit = SubmitRequest {
                    schema_version: RUNTIME_SCHEMA_VERSION,
                    client_request_id: request.client_request_id.clone(),
                    request_identity_digest: Some(request_identity_digest),
                    plan,
                    global_limit: request.global_limit,
                };
                match self.registry.submit(&submit)? {
                    AdmissionOutcome::Created(created) => {
                        let job_id = created.job.job_id.clone();
                        self.ensure_attempt_dispatched(&created.attempt)?;
                        job_id
                    }
                    AdmissionOutcome::Existing { job } => job.job_id.clone(),
                }
            }
        };
        self.observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id,
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
            stdout_offset: None,
            stderr_offset: None,
        })
    }

    pub fn open_workspace(
        &self,
        request: &GitWorkspaceCreateRequest,
    ) -> RuntimeResult<CompactWorkspaceOpenResult> {
        let _guard = self.lock_lifecycle()?;
        create_git_workspace_compact(&self.executor, request).map_err(map_universal_error)
    }

    pub fn mutate_workspace(
        &self,
        request: &WorkspaceMutateRequest,
    ) -> RuntimeResult<WorkspaceMutateResult> {
        let _guard = self.lock_lifecycle()?;
        let _ = self.reconcile_workspace(&request.workspace_id)?;
        let active = self
            .registry
            .active_job_ids_for_workspace(&request.workspace_id, 20)?;
        if !active.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceBusy,
                format!(
                    "workspace source state is committed by active or held Jobs: {}",
                    active.join(", ")
                ),
                Some("workspaceId"),
                true,
            ));
        }
        mutate_workspace(&self.executor, request).map_err(map_universal_error)
    }

    pub fn close_workspace(
        &self,
        request: &WorkspaceCloseRequest,
    ) -> RuntimeResult<WorkspaceCloseResult> {
        let _guard = self.lock_lifecycle()?;
        let _ = self.reconcile_workspace(&request.workspace_id)?;
        let active = self
            .registry
            .active_job_ids_for_workspace(&request.workspace_id, 20)?;
        if !active.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceBusy,
                format!("workspace has active or held Jobs: {}", active.join(", ")),
                Some("workspaceId"),
                true,
            ));
        }
        remove_git_workspace(&self.executor, request).map_err(map_universal_error)
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
            workspace_source_digest: Some(
                workspace_source_state_digest(&self.executor, &request.execution.workspace_id)
                    .map_err(map_universal_error)?,
            ),
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: sha256_file(&executable).map_err(map_universal_error)?,
            args: request.execution.args.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            env: request.execution.env.clone(),
            timeout_ms: request.execution.timeout_ms,
            stdout_limit_bytes: request.execution.stdout_limit_bytes,
            stderr_limit_bytes: request.execution.stderr_limit_bytes,
            budget: request.execution.budget.clone(),
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
            workspace_source_digest: plan.workspace_source_digest.clone(),
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
            &plan.budget,
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
        let mut poll_index = 0;
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
            sleep_until_poll(deadline, &mut poll_index);
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
        let mut current = self.registry.get_attempt(&attempt.attempt_id)?;
        for retry in 0..=1 {
            if current.state == AttemptState::Orphaned
                && self.recover_orphaned_runner_result(&current)?
            {
                return self.observation_from_registry(&current.job_id, 0, 0);
            }
            if current.state.is_terminal() {
                return self.observation_from_registry(&current.job_id, 0, 0);
            }
            let terminal = self.prepare_runner_terminal(&current)?;
            match self.registry.commit_terminal(&terminal) {
                Ok(projection) => {
                    self.cleanup_payload_view(&current.attempt_id)?;
                    return self.observation_from_parts(
                        projection,
                        Some(current),
                        4096,
                        4096,
                        None,
                        None,
                    );
                }
                Err(error)
                    if retry == 0 && error.code == RuntimeErrorCode::AttemptStateConflict =>
                {
                    current = self.registry.get_attempt(&attempt.attempt_id)?;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("terminal commit retry loop always returns")
    }

    fn prepare_runner_terminal(&self, current: &AttemptRecord) -> RuntimeResult<TerminalCommit> {
        prepare_runner_terminal_from_bundle(current)
    }

    pub fn observe_task(&self, request: &TaskObserveRequest) -> RuntimeResult<TaskObservation> {
        validate_observe_request(request)?;
        let deadline = Instant::now() + Duration::from_millis(request.wait_ms);
        let mut poll_index = 0;
        loop {
            self.reconcile_job(&request.job_id)?;
            let snapshot = self.registry.job_snapshot(&request.job_id)?;
            if snapshot.projection.result_available
                || request.wait_ms == 0
                || Instant::now() >= deadline
            {
                return self.observation_from_snapshot(snapshot, request);
            }
            sleep_until_poll(deadline, &mut poll_index);
        }
    }

    fn observation_from_registry(
        &self,
        job_id: &str,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> RuntimeResult<TaskObservation> {
        let snapshot = self.registry.job_snapshot(job_id)?;
        self.observation_from_parts(
            snapshot.projection,
            snapshot.attempt,
            stdout_tail_bytes,
            stderr_tail_bytes,
            None,
            None,
        )
    }

    fn observation_from_snapshot(
        &self,
        snapshot: JobSnapshot,
        request: &TaskObserveRequest,
    ) -> RuntimeResult<TaskObservation> {
        self.observation_from_parts(
            snapshot.projection,
            snapshot.attempt,
            request.stdout_tail_bytes,
            request.stderr_tail_bytes,
            request.stdout_offset,
            request.stderr_offset,
        )
    }

    fn observation_from_parts(
        &self,
        projection: super::JobProjection,
        attempt: Option<AttemptRecord>,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
        stdout_offset: Option<u64>,
        stderr_offset: Option<u64>,
    ) -> RuntimeResult<TaskObservation> {
        let job_id = projection.job_id.clone();
        let terminal = projection.result_available;
        let (
            stdout_view,
            stderr_view,
            stdout_truncated,
            stderr_truncated,
            artifacts,
            error_summary,
        ) = if let Some(attempt) = &attempt {
            let stdout_view = read_output_text(
                &Path::new(&attempt.bundle_path).join(STDOUT_FILE),
                stdout_offset,
                stdout_tail_bytes,
                terminal,
                "stdoutOffset",
                "stdoutTailBytes",
            )?;
            let stderr_view = read_output_text(
                &Path::new(&attempt.bundle_path).join(STDERR_FILE),
                stderr_offset,
                stderr_tail_bytes,
                terminal,
                "stderrOffset",
                "stderrTailBytes",
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
                stdout_view,
                stderr_view,
                stdout_truncated,
                stderr_truncated,
                artifacts,
                error_summary,
            )
        } else {
            (
                OutputView::empty(stdout_offset, terminal),
                OutputView::empty(stderr_offset, terminal),
                false,
                false,
                Vec::new(),
                None,
            )
        };
        Ok(TaskObservation {
            job_id,
            status: projection.status,
            attempt_id: attempt.map(|attempt| attempt.attempt_id),
            exit_code: projection.exit_code,
            stdout_tail: stdout_view.content,
            stderr_tail: stderr_view.content,
            stdout_offset: stdout_view.offset,
            stdout_next_offset: stdout_view.next_offset,
            stdout_available_bytes: stdout_view.available_bytes,
            stdout_eof: stdout_view.eof,
            stderr_offset: stderr_view.offset,
            stderr_next_offset: stderr_view.next_offset,
            stderr_available_bytes: stderr_view.available_bytes,
            stderr_eof: stderr_view.eof,
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
                    let _ = self.reconcile_orphaned_attempt(&attempt)?;
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

    pub fn reconcile_all(&self) -> RuntimeResult<ReconciliationReport> {
        let mut report = ReconciliationReport::default();
        self.reconcile_recoverable_orphans_into(&mut report)?;
        let attempts = self.registry.list_nonterminal_attempts()?;
        self.reconcile_candidates_into(attempts, &mut report)?;
        Ok(report)
    }

    pub fn reconcile_workspace(&self, workspace_id: &str) -> RuntimeResult<ReconciliationReport> {
        let attempts = self.registry.list_workspace_reconciliation_attempts(
            workspace_id,
            INTERACTIVE_RECONCILIATION_LIMIT,
        )?;
        let mut report = ReconciliationReport::default();
        self.reconcile_candidates_into(attempts, &mut report)?;
        Ok(report)
    }

    fn reconcile_nonterminal_batch(&self, limit: u32) -> RuntimeResult<ReconciliationReport> {
        let attempts = self.registry.list_nonterminal_attempts_bounded(limit)?;
        let mut report = ReconciliationReport::default();
        self.reconcile_candidates_into(attempts, &mut report)?;
        Ok(report)
    }

    pub fn reconcile_maintenance_batch(&self, limit: u32) -> RuntimeResult<ReconciliationReport> {
        let attempts = self.registry.list_maintenance_attempts_bounded(limit)?;
        let mut report = ReconciliationReport::default();
        self.reconcile_candidates_into(attempts, &mut report)?;
        Ok(report)
    }

    pub fn reconcile_recoverable_orphans(&self) -> RuntimeResult<ReconciliationReport> {
        let mut report = ReconciliationReport::default();
        self.reconcile_recoverable_orphans_into(&mut report)?;
        Ok(report)
    }

    fn reconcile_recoverable_orphans_into(
        &self,
        report: &mut ReconciliationReport,
    ) -> RuntimeResult<()> {
        let attempts = self.registry.list_held_orphaned_attempts()?;
        self.reconcile_candidates_into(attempts, report)
    }

    fn reconcile_candidates_into(
        &self,
        attempts: Vec<AttemptRecord>,
        report: &mut ReconciliationReport,
    ) -> RuntimeResult<()> {
        for attempt in attempts {
            self.reconcile_candidate_into(&attempt, report)?;
        }
        Ok(())
    }

    fn reconcile_candidate_into(
        &self,
        attempt: &AttemptRecord,
        report: &mut ReconciliationReport,
    ) -> RuntimeResult<()> {
        report.inspected += 1;
        let before = attempt.state;
        let reconciling_orphan = attempt.state == AttemptState::Orphaned;
        let result = if reconciling_orphan {
            self.reconcile_orphaned_attempt(attempt)
        } else if attempt.state.is_terminal() {
            self.registry
                .converge_terminal_reservation(&attempt.attempt_id, now_ms()?)
        } else if attempt.state == AttemptState::Accepted {
            self.ensure_attempt_dispatched(attempt).map(|()| false)
        } else {
            self.reconcile_attempt(&attempt.attempt_id).map(|()| false)
        };
        match result {
            Ok(changed) => {
                let current = self.registry.get_attempt(&attempt.attempt_id)?;
                let orphan_converged =
                    reconciling_orphan && (changed || current.state != AttemptState::Orphaned);
                if orphan_converged {
                    report.recovered_orphans += 1;
                } else if current.state == AttemptState::Orphaned
                    && before != AttemptState::Orphaned
                {
                    report.quarantined += 1;
                } else if changed || current.state != before {
                    report.reconciled += 1;
                } else {
                    report.unchanged += 1;
                }
                if !reconciling_orphan || orphan_converged {
                    self.registry
                        .clear_reconciliation_failure(&attempt.attempt_id, now_ms()?)?;
                }
            }
            Err(error) => {
                self.record_isolated_reconciliation_failure(attempt, error, report)?;
            }
        }
        Ok(())
    }

    fn record_isolated_reconciliation_failure(
        &self,
        attempt: &AttemptRecord,
        error: RuntimeError,
        report: &mut ReconciliationReport,
    ) -> RuntimeResult<()> {
        if error.is_reconciliation_fatal() {
            return Err(error);
        }
        self.registry
            .record_reconciliation_failure(attempt, &error, now_ms()?)?;
        report.failed += 1;
        report.failures.push(ReconciliationFailure {
            attempt_id: attempt.attempt_id.clone(),
            job_id: attempt.job_id.clone(),
            code: error.code,
            message: error.message,
        });
        Ok(())
    }

    fn reconcile_orphaned_attempt(&self, attempt: &AttemptRecord) -> RuntimeResult<bool> {
        let current = self.registry.get_attempt(&attempt.attempt_id)?;
        if current.state != AttemptState::Orphaned {
            return Ok(false);
        }
        if Path::new(&current.bundle_path).join(RESULT_FILE).is_file() {
            return self.recover_orphaned_runner_result(&current);
        }
        if self.orphan_process_tree_alive(&current)? {
            return Ok(false);
        }
        if Path::new(&current.bundle_path).join(RESULT_FILE).is_file() {
            return self.recover_orphaned_runner_result(&current);
        }
        let job = self.registry.get_job(&current.job_id)?;
        let (state, reason) = match current.termination_intent {
            AttemptTerminationIntent::StopRequested => (
                AttemptState::Cancelled,
                "ORPHAN_CANCELLED_PROCESS_TREE_GONE",
            ),
            AttemptTerminationIntent::DeadlineExceeded => {
                (AttemptState::TimedOut, "ORPHAN_DEADLINE_PROCESS_TREE_GONE")
            }
            AttemptTerminationIntent::Natural
                if job.desired_state == JobDesiredState::Cancelled =>
            {
                (
                    AttemptState::Cancelled,
                    "ORPHAN_CANCELLED_PROCESS_TREE_GONE",
                )
            }
            AttemptTerminationIntent::Natural => (AttemptState::Lost, "ORPHANED_PROCESS_TREE_GONE"),
        };
        self.resolve_absent_orphan(&current, state, reason)?;
        Ok(true)
    }

    fn resolve_absent_orphan(
        &self,
        attempt: &AttemptRecord,
        state: AttemptState,
        reason_code: &str,
    ) -> RuntimeResult<()> {
        // Interactive reconciliation paths may inspect the same orphan concurrently. Derive
        // one stable logical observation time from the orphan terminal record so every writer
        // produces identical remediation evidence bytes and Digest.
        let observed_at_ms = attempt
            .finished_at_ms
            .unwrap_or(attempt.created_at_ms)
            .saturating_add(1);
        let evidence = ControlTerminalEvidence {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: attempt.job_id.clone(),
            attempt_id: attempt.attempt_id.clone(),
            status: state.as_db().to_string(),
            reason_code: reason_code.to_string(),
            detail: Some(
                "the persisted unit, process identity, and cgroup no longer own a live process tree"
                    .to_string(),
            ),
            observed_at_ms,
        };
        let evidence_path = Path::new(&attempt.bundle_path).join(ORPHAN_REMEDIATION_FILE);
        write_json_atomic(&evidence_path, &evidence).map_err(map_universal_error)?;
        let result_digest = sha256_file(&evidence_path).map_err(map_universal_error)?;
        let existing_artifacts = self
            .registry
            .job_snapshot(&attempt.job_id)?
            .projection
            .artifacts;
        let mut artifacts = vec![ArtifactRegistration {
            artifact_id: format!("{}.orphan-remediation", attempt.attempt_id),
            kind: "control_result".to_string(),
            relative_path: ORPHAN_REMEDIATION_FILE.to_string(),
            digest: result_digest.clone(),
            media_type: "application/json".to_string(),
            byte_length: fs::metadata(&evidence_path)
                .map_err(|error| io_error("inspect orphan remediation evidence", error))?
                .len(),
            truncated: false,
        }];
        for (file_name, kind) in [(STDOUT_FILE, "stdout"), (STDERR_FILE, "stderr")] {
            let artifact_id = format!("{}.{}", attempt.attempt_id, kind);
            let path = Path::new(&attempt.bundle_path).join(file_name);
            if path.is_file()
                && !existing_artifacts
                    .iter()
                    .any(|artifact| artifact.artifact_id == artifact_id)
            {
                artifacts.push(ArtifactRegistration {
                    artifact_id,
                    kind: kind.to_string(),
                    relative_path: file_name.to_string(),
                    digest: sha256_file(&path).map_err(map_universal_error)?,
                    media_type: "text/plain; charset=utf-8".to_string(),
                    byte_length: fs::metadata(&path)
                        .map_err(|error| io_error("inspect orphan output", error))?
                        .len(),
                    truncated: false,
                });
            }
        }
        self.registry.recover_orphaned_terminal(&TerminalCommit {
            attempt_id: attempt.attempt_id.clone(),
            expected_row_version: attempt.row_version,
            state,
            result_digest,
            exit_code: None,
            infrastructure_error_digest: Some(sha256_bytes(reason_code.as_bytes())),
            finished_at_ms: observed_at_ms,
            artifacts,
            reason_code: reason_code.to_string(),
        })?;
        self.cleanup_payload_view(&attempt.attempt_id)
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
        self.observation_from_parts(projection, Some(current), 4096, 4096, None, None)
    }

    pub fn cancel_task(&self, request: &TaskCancelRequest) -> RuntimeResult<TaskObservation> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(RuntimeError::invalid(
                "unsupported runtime schema version",
                "schemaVersion",
            ));
        }
        let snapshot = self.registry.job_snapshot(&request.job_id)?;
        if snapshot.job.resolution == Some(JobResolution::Orphaned) {
            if let Some(attempt) = snapshot.attempt {
                if Path::new(&attempt.bundle_path).join(RESULT_FILE).is_file()
                    && self.recover_orphaned_runner_result(&attempt)?
                {
                    return self.observation_from_registry(&request.job_id, 4096, 4096);
                }
                let _ = self.registry.request_cancel(&request.job_id, now_ms()?)?;
                let current = self.registry.get_attempt(&attempt.attempt_id)?;
                if !self.orphan_process_tree_alive(&current)? {
                    if Path::new(&current.bundle_path).join(RESULT_FILE).is_file() {
                        let _ = self.recover_orphaned_runner_result(&current)?;
                    } else {
                        self.resolve_absent_orphan(
                            &current,
                            AttemptState::Cancelled,
                            "ORPHAN_CANCELLED_PROCESS_TREE_GONE",
                        )?;
                    }
                }
                return self.observation_from_registry(&request.job_id, 4096, 4096);
            }
        }
        self.reconcile_job(&request.job_id)?;
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
        let mut poll_index = 0;
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
            sleep_until_poll(deadline, &mut poll_index);
        }
        self.reconcile_attempt(&attempt.attempt_id)?;
        self.observation_from_registry(&request.job_id, 4096, 4096)
    }

    pub fn list_jobs(
        &self,
        request: &RuntimeJobListRequest,
    ) -> RuntimeResult<RuntimeJobListResult> {
        self.reconcile_recoverable_orphans()?;
        let _ = self.reconcile_nonterminal_batch(INTERACTIVE_RECONCILIATION_LIMIT)?;
        self.registry.list_jobs(request)
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
        let range = read_utf8_range(
            &canonical,
            request.offset,
            request.max_bytes,
            artifact.byte_length,
            true,
            RangeFields {
                offset: "offset",
                max_bytes: "maxBytes",
            },
            "Artifact",
        )?;
        Ok(ArtifactReadResult {
            job_id: request.job_id.clone(),
            artifact_id: request.artifact_id.clone(),
            content: range.content,
            offset: request.offset,
            next_offset: range.next_offset,
            eof: range.next_offset >= artifact.byte_length,
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

fn validate_run_request(request: &TaskRunRequest, max_output_bytes: u64) -> RuntimeResult<()> {
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
    if request.execution.stdout_limit_bytes > max_output_bytes {
        return Err(RuntimeError::invalid(
            format!("stdoutLimitBytes exceeds configured maximum {max_output_bytes}"),
            "execution.stdoutLimitBytes",
        ));
    }
    if request.execution.stderr_limit_bytes > max_output_bytes {
        return Err(RuntimeError::invalid(
            format!("stderrLimitBytes exceeds configured maximum {max_output_bytes}"),
            "execution.stderrLimitBytes",
        ));
    }
    validate_execution_budget(&request.execution.budget, "execution.budget")?;
    if request.execution.args.len() > 128 || request.execution.env.len() > 64 {
        return Err(RuntimeError::invalid(
            "args or environment exceed runtime bounds",
            "execution",
        ));
    }
    Ok(())
}

fn validate_execution_budget(
    budget: &super::ExecutionBudget,
    field_prefix: &str,
) -> RuntimeResult<()> {
    if budget.memory_max_bytes.is_some_and(|value| {
        !(super::MIN_MEMORY_MAX_BYTES..=super::MAX_MEMORY_MAX_BYTES).contains(&value)
    }) {
        return Err(RuntimeError::invalid(
            format!(
                "memoryMaxBytes must be in {}..={}",
                super::MIN_MEMORY_MAX_BYTES,
                super::MAX_MEMORY_MAX_BYTES
            ),
            &format!("{field_prefix}.memoryMaxBytes"),
        ));
    }
    if budget
        .tasks_max
        .is_some_and(|value| value == 0 || value > super::MAX_TASKS_MAX)
    {
        return Err(RuntimeError::invalid(
            format!("tasksMax must be in 1..={}", super::MAX_TASKS_MAX),
            &format!("{field_prefix}.tasksMax"),
        ));
    }
    if budget
        .cpu_quota_percent
        .is_some_and(|value| value == 0 || value > super::MAX_CPU_QUOTA_PERCENT)
    {
        return Err(RuntimeError::invalid(
            format!(
                "cpuQuotaPercent must be in 1..={}",
                super::MAX_CPU_QUOTA_PERCENT
            ),
            &format!("{field_prefix}.cpuQuotaPercent"),
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
    budget: &super::ExecutionBudget,
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
    if let Some(memory_max_bytes) = budget.memory_max_bytes {
        command.arg(format!("--property=MemoryMax={memory_max_bytes}"));
    }
    if let Some(tasks_max) = budget.tasks_max {
        command.arg(format!("--property=TasksMax={tasks_max}"));
    }
    if let Some(cpu_quota_percent) = budget.cpu_quota_percent {
        command.arg(format!("--property=CPUQuota={cpu_quota_percent}%"));
    }

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
    budget: &super::ExecutionBudget,
) -> RuntimeResult<std::process::Output> {
    build_systemd_run_command(
        unit_name,
        runner,
        bundle_path,
        workspace_path,
        runtime_ceiling_ms,
        budget,
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

#[derive(Debug)]
struct OutputView {
    content: String,
    offset: Option<u64>,
    next_offset: Option<u64>,
    available_bytes: Option<u64>,
    eof: Option<bool>,
}

impl OutputView {
    fn empty(offset: Option<u64>, terminal: bool) -> Self {
        Self {
            content: String::new(),
            offset,
            next_offset: offset,
            available_bytes: offset.map(|_| 0),
            eof: offset.map(|value| terminal && value == 0),
        }
    }
}

#[derive(Debug)]
struct TextRange {
    content: String,
    next_offset: u64,
}

#[derive(Clone, Copy)]
struct RangeFields<'a> {
    offset: &'a str,
    max_bytes: &'a str,
}

fn read_utf8_range(
    path: &Path,
    offset: u64,
    max_bytes: u64,
    available: u64,
    terminal: bool,
    fields: RangeFields<'_>,
    context: &str,
) -> RuntimeResult<TextRange> {
    if offset > available {
        return Err(RuntimeError::invalid(
            format!("{} exceeds retained byte length {available}", fields.offset),
            fields.offset,
        ));
    }
    if max_bytes == 0 || offset == available {
        return Ok(TextRange {
            content: String::new(),
            next_offset: offset,
        });
    }
    let read_limit = max_bytes.min(available.saturating_sub(offset));
    let mut file =
        File::open(path).map_err(|error| io_error(&format!("open {context} range"), error))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| io_error(&format!("seek {context} range"), error))?;
    let mut bytes = vec![0_u8; usize::try_from(read_limit).unwrap_or(usize::MAX)];
    let read = file
        .read(&mut bytes)
        .map_err(|error| io_error(&format!("read {context} range"), error))?;
    bytes.truncate(read);
    if offset > 0 && bytes.first().is_some_and(|byte| byte & 0xc0 == 0x80) {
        return Err(RuntimeError::invalid(
            format!("{} must point to a UTF-8 character boundary", fields.offset),
            fields.offset,
        ));
    }
    let safe_len = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => bytes.len(),
    };
    if safe_len == 0 && !bytes.is_empty() {
        if !terminal && offset.saturating_add(bytes.len() as u64) >= available {
            return Ok(TextRange {
                content: String::new(),
                next_offset: offset,
            });
        }
        return Err(RuntimeError::invalid(
            format!(
                "{} is too small for the next UTF-8 character; use at least 4 bytes",
                fields.max_bytes
            ),
            fields.max_bytes,
        ));
    }
    bytes.truncate(safe_len);
    Ok(TextRange {
        content: String::from_utf8_lossy(&bytes).into_owned(),
        next_offset: offset.saturating_add(safe_len as u64),
    })
}

fn read_output_text(
    path: &Path,
    offset: Option<u64>,
    max_bytes: u64,
    terminal: bool,
    offset_field: &str,
    max_bytes_field: &str,
) -> RuntimeResult<OutputView> {
    let Some(offset) = offset else {
        return Ok(OutputView {
            content: read_tail_text(path, max_bytes)?,
            offset: None,
            next_offset: None,
            available_bytes: None,
            eof: None,
        });
    };
    let available = if path.exists() {
        fs::metadata(path)
            .map_err(|error| io_error("inspect output range", error))?
            .len()
    } else {
        0
    };
    if offset > available {
        return Err(RuntimeError::invalid(
            format!("{offset_field} exceeds retained output length {available}"),
            offset_field,
        ));
    }
    if max_bytes == 0 || !path.exists() {
        return Ok(OutputView {
            content: String::new(),
            offset: Some(offset),
            next_offset: Some(offset),
            available_bytes: Some(available),
            eof: Some(terminal && offset >= available),
        });
    }
    let range = read_utf8_range(
        path,
        offset,
        max_bytes,
        available,
        terminal,
        RangeFields {
            offset: offset_field,
            max_bytes: max_bytes_field,
        },
        "output",
    )?;
    Ok(OutputView {
        content: range.content,
        offset: Some(offset),
        next_offset: Some(range.next_offset),
        available_bytes: Some(available),
        eof: Some(terminal && range.next_offset >= available),
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
    let code = match error.code {
        crate::UniversalExecErrorCode::WorkspaceDirty => RuntimeErrorCode::WorkspaceDirty,
        crate::UniversalExecErrorCode::WorkspaceCapacityExceeded => {
            RuntimeErrorCode::WorkspaceCapacityExceeded
        }
        _ => RuntimeErrorCode::InvalidRequest,
    };
    RuntimeError::new(code, error.message, error.field.as_deref(), error.retryable)
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
    use crate::UniversalExecutionRequest;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn incremental_output_ranges_reconstruct_retained_bytes(
            chunks in prop::collection::vec(
                prop::collection::vec(any::<char>(), 0..16)
                    .prop_map(|chars| chars.into_iter().collect::<String>()),
                1..30,
            ),
            chunk_size in 4u64..64,
        ) {
            let root = std::env::temp_dir().join(format!(
                "ordivon-output-range-property-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
            ));
            fs::create_dir_all(&root).unwrap();
            let path = root.join("stdout.log");
            let expected = chunks.concat();
            fs::write(&path, expected.as_bytes()).unwrap();
            let mut offset = 0u64;
            let mut reconstructed = String::new();
            loop {
                let view = read_output_text(
                    &path,
                    Some(offset),
                    chunk_size,
                    true,
                    "stdoutOffset",
                    "stdoutTailBytes",
                ).unwrap();
                reconstructed.push_str(&view.content);
                offset = view.next_offset.unwrap();
                if view.eof == Some(true) {
                    break;
                }
            }
            prop_assert_eq!(reconstructed, expected);
            prop_assert_eq!(offset, fs::metadata(&path).unwrap().len());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn utf8_ranges_respect_hard_byte_bounds() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-utf8-hard-bound-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("stdout.log");
        fs::write(&path, "🙂x".as_bytes()).unwrap();
        let error = read_utf8_range(
            &path,
            0,
            3,
            5,
            true,
            RangeFields {
                offset: "stdoutOffset",
                max_bytes: "stdoutTailBytes",
            },
            "output",
        )
        .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
        assert_eq!(error.field.as_deref(), Some("stdoutTailBytes"));
        let first = read_utf8_range(
            &path,
            0,
            4,
            5,
            true,
            RangeFields {
                offset: "stdoutOffset",
                max_bytes: "stdoutTailBytes",
            },
            "output",
        )
        .unwrap();
        assert_eq!(first.content, "🙂");
        assert_eq!(first.next_offset, 4);
        let second = read_utf8_range(
            &path,
            4,
            1,
            5,
            true,
            RangeFields {
                offset: "stdoutOffset",
                max_bytes: "stdoutTailBytes",
            },
            "output",
        )
        .unwrap();
        assert_eq!(second.content, "x");
        assert_eq!(second.next_offset, 5);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_output_limit_is_enforced_before_admission() {
        let request = TaskRunRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: "request:output-limit".to_string(),
            principal: "principal:test".to_string(),
            global_limit: 1,
            execution: UniversalExecutionRequest {
                workspace_id: "workspace-output-limit".to_string(),
                executable: "/usr/bin/true".to_string(),
                args: Vec::new(),
                cwd_relative: ".".to_string(),
                env: BTreeMap::new(),
                timeout_ms: 1_000,
                stdout_limit_bytes: 1_025,
                stderr_limit_bytes: 1_024,
                budget: crate::ExecutionBudget::default(),
            },
            wait_ms: 0,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
        };
        let error = validate_run_request(&request, 1_024).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
        assert_eq!(error.field.as_deref(), Some("execution.stdoutLimitBytes"));
    }

    #[test]
    fn adaptive_polling_starts_fast_and_caps_at_fifty_milliseconds() {
        let observed = (0..8)
            .map(|index| adaptive_poll_delay(index).as_millis())
            .collect::<Vec<_>>();
        assert_eq!(observed, vec![2, 5, 10, 20, 50, 50, 50, 50]);
    }

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
            Path::new("/root/projects/ordivon-runtime"),
            10_000,
            &crate::ExecutionBudget::default(),
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
            "CPUQuota",
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
        assert!(!valid_environment_name(
            "CARGO_BIN_EXE_ordivon-runtime-job-fixture"
        ));
    }

    #[test]
    fn execution_budget_maps_to_systemd_resource_properties() {
        let command = build_systemd_run_command(
            "ordivon-budget.service",
            Path::new("/usr/bin/true"),
            Path::new("/var/lib/ordivon/attempts/attempt-budget"),
            Path::new("/root/projects/ordivon-runtime"),
            10_000,
            &crate::ExecutionBudget {
                memory_max_bytes: Some(512 * 1024 * 1024),
                tasks_max: Some(64),
                cpu_quota_percent: Some(250),
            },
        )
        .unwrap();
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(args.contains("MemoryMax=536870912"));
        assert!(args.contains("TasksMax=64"));
        assert!(args.contains("CPUQuota=250%"));
    }
}
