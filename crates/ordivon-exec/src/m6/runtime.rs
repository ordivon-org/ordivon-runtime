use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::universal::{
    canonical_directory, load_workspace_record, resolve_workspace_cwd, sha256_bytes, sha256_file,
    write_json_atomic, CapturedOutput, RunnerPayloadConfig, RunnerStartEvidence, RunnerTaskRequest,
    RunnerTaskResult, TaskTerminalStatus, UniversalExecutorConfig, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use crate::{
    classify_supervisor_recovery, JobInternalState, RunnerResultObservation, SupervisorIdentity,
    SupervisorObservation, SupervisorRecoveryDisposition, SupervisorUnitState, TerminationIntent,
};

use super::registry::JobSnapshotM6;
use super::{
    AdmissionOutcomeM6, ArtifactRegistrationM6, AttemptRecordM6, AttemptState, JobListRequestM6,
    JobListResultM6, M6ArtifactReadRequest, M6ArtifactReadResult, M6Error, M6ErrorCode,
    M6ExecutionPlan, M6Registry, M6RegistryConfig, M6Result, M6SubmitRequest, M6TaskCancelRequest,
    M6TaskObservation, M6TaskObserveRequest, M6TaskRunRequest, PlanKind, RunnerIdentityM6,
    TerminalCommitM6, M6_SCHEMA_VERSION,
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
const MAX_M6_WAIT_MS: u64 = 30_000;
const MAX_M6_TAIL_BYTES: u64 = 64 * 1024;
const MAX_M6_ARTIFACT_READ_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct M6RuntimeConfig {
    pub registry: M6RegistryConfig,
    pub executor: UniversalExecutorConfig,
    pub startup_grace_ms: u64,
    #[cfg(feature = "runtime-hardening-m7")]
    pub hardening: Option<crate::M7RuntimeHardeningConfig>,
}

#[derive(Clone, Debug)]
pub struct M6Runtime {
    registry: M6Registry,
    executor: UniversalExecutorConfig,
    startup_grace_ms: u64,
    #[cfg(feature = "runtime-hardening-m7")]
    hardening: Option<crate::M7RuntimeHardeningConfig>,
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

impl M6Runtime {
    pub fn new(config: M6RuntimeConfig) -> M6Result<Self> {
        config.executor.validate().map_err(map_universal_error)?;
        if config.startup_grace_ms == 0 || config.startup_grace_ms > 30_000 {
            return Err(M6Error::invalid(
                "startupGraceMs must be in 1..=30000",
                "startupGraceMs",
            ));
        }
        ensure_systemd_visible(&config.registry.store_root, "registry.storeRoot")?;
        ensure_systemd_visible(&config.executor.store_root, "executor.storeRoot")?;
        #[cfg(feature = "runtime-hardening-m7")]
        if let Some(hardening) = &config.hardening {
            hardening.validate()?;
            validate_hardening_roots(&config, hardening)?;
            crate::m7::ensure_traversal_directory(
                &hardening.workspaces_root(),
                hardening.worker.gid,
                0o710,
            )?;
            crate::m7::ensure_traversal_directory(
                &hardening.attempts_root(),
                hardening.worker.gid,
                0o710,
            )?;
            crate::m7::ensure_traversal_directory(
                &hardening.runtime_view_root,
                hardening.worker.gid,
                0o750,
            )?;
            crate::m7::ensure_owned_directory(
                &hardening.cache_root,
                hardening.worker.uid,
                hardening.worker.gid,
                0o750,
            )?;
        }
        let registry = M6Registry::initialize(config.registry)?;
        Ok(Self {
            registry,
            executor: config.executor,
            startup_grace_ms: config.startup_grace_ms,
            #[cfg(feature = "runtime-hardening-m7")]
            hardening: config.hardening,
        })
    }

    pub fn registry(&self) -> &M6Registry {
        &self.registry
    }

    pub fn run_task(&self, request: &M6TaskRunRequest) -> M6Result<M6TaskObservation> {
        validate_run_request(request)?;
        let plan = self.resolve_plan(request)?;
        let submit = M6SubmitRequest {
            schema_version: M6_SCHEMA_VERSION,
            client_request_id: request.client_request_id.clone(),
            plan,
            global_limit: request.global_limit,
            profile_limit: request.profile_limit,
        };
        let job_id = match self.registry.submit(&submit)? {
            AdmissionOutcomeM6::Created(created) => {
                let job_id = created.job.job_id.clone();
                self.ensure_attempt_dispatched(&created.attempt)?;
                job_id
            }
            AdmissionOutcomeM6::Existing { job } => job.job_id.clone(),
        };
        self.observe_task(&M6TaskObserveRequest {
            schema_version: M6_SCHEMA_VERSION,
            job_id,
            wait_ms: request.wait_ms,
            stdout_tail_bytes: request.stdout_tail_bytes,
            stderr_tail_bytes: request.stderr_tail_bytes,
        })
    }

    fn resolve_plan(&self, request: &M6TaskRunRequest) -> M6Result<M6ExecutionPlan> {
        let record = load_workspace_record(&self.executor, &request.execution.workspace_id)
            .map_err(map_universal_error)?;
        let workspace_path =
            canonical_directory(Path::new(&record.workspace_path), "workspacePath")
                .map_err(map_universal_error)?;
        let cwd = resolve_workspace_cwd(&record, &request.execution.cwd_relative)
            .map_err(map_universal_error)?;
        ensure_systemd_visible(&workspace_path, "workspacePath")?;
        ensure_systemd_visible(&cwd, "cwd")?;
        let executable = validate_executable(&self.executor, &request.execution.executable)?;
        Ok(M6ExecutionPlan {
            schema_version: M6_SCHEMA_VERSION,
            plan_kind: PlanKind::UniversalSandbox,
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
            policy_id: request.policy_id.clone(),
            policy_version: request.policy_version.clone(),
            policy_digest: request.policy_digest.clone(),
            profile_id: request.profile_id.clone(),
            principal: request.principal.clone(),
            authority_ref: request.authority_ref.clone(),
        })
    }

    fn ensure_attempt_dispatched(&self, attempt: &AttemptRecordM6) -> M6Result<()> {
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

    fn materialize_bundle(&self, attempt: &AttemptRecordM6) -> M6Result<AttemptRecordM6> {
        if attempt.state != AttemptState::Accepted {
            return Err(M6Error::new(
                M6ErrorCode::AttemptStateConflict,
                "only accepted Attempts may materialize a bundle",
                Some("attemptId"),
                false,
            ));
        }
        let snapshot = self.registry.job_snapshot(&attempt.job_id)?;
        let job = snapshot.job;
        let stored_attempt = snapshot.attempt.ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "Job has no Attempt while materializing bundle",
                Some("attemptId"),
                false,
            )
        })?;
        if stored_attempt.attempt_id != attempt.attempt_id
            || stored_attempt.row_version != attempt.row_version
        {
            return Err(M6Error::new(
                M6ErrorCode::AttemptStateConflict,
                "Attempt changed before bundle materialization",
                Some("attemptId"),
                false,
            ));
        }
        let plan: M6ExecutionPlan =
            serde_json::from_str(&job.execution_plan_json).map_err(|error| {
                M6Error::new(
                    M6ErrorCode::RegistryCorrupt,
                    format!("stored execution plan is invalid: {error}"),
                    Some("executionPlan"),
                    false,
                )
            })?;
        let launch_token = sha256_bytes(
            format!(
                "m6-launch-v1\0{}\0{}",
                attempt.attempt_id, job.operation_digest
            )
            .as_bytes(),
        );
        if sha256_bytes(launch_token.as_bytes()) != attempt.launch_token_digest {
            return Err(M6Error::new(
                M6ErrorCode::RegistryCorrupt,
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
            schema_version: M6_SCHEMA_VERSION,
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
            return Err(M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "reconstructed bundle identity does not match Registry",
                Some("attemptId"),
                false,
            ));
        }

        let final_path = PathBuf::from(&attempt.bundle_path);
        let parent = final_path.parent().ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::IoError,
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

    fn dispatch_attempt(&self, attempt: &AttemptRecordM6) -> M6Result<()> {
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
            #[cfg(feature = "runtime-hardening-m7")]
            self.hardening.as_ref(),
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

    fn await_launch_evidence(&self, attempt: &AttemptRecordM6) -> M6Result<()> {
        let deadline = Instant::now() + Duration::from_millis(self.startup_grace_ms);
        loop {
            if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
                return self.reconcile_runner_result(attempt);
            }
            if Path::new(&attempt.bundle_path)
                .join(RUNNER_START_FILE)
                .exists()
            {
                self.bind_runner_start(attempt)?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.reconcile_attempt(&attempt.attempt_id)
    }

    fn bind_runner_start(&self, attempt: &AttemptRecordM6) -> M6Result<AttemptRecordM6> {
        let path = Path::new(&attempt.bundle_path).join(RUNNER_START_FILE);
        let bytes =
            fs::read(&path).map_err(|error| io_error("read runner-start evidence", error))?;
        let evidence: RunnerStartEvidence = serde_json::from_slice(&bytes).map_err(|error| {
            M6Error::new(
                M6ErrorCode::LaunchIdentityMismatch,
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
            return Err(M6Error::new(
                M6ErrorCode::LaunchIdentityMismatch,
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
            return Err(M6Error::new(
                M6ErrorCode::LaunchIdentityMismatch,
                "runner-start omitted PID namespace identity",
                Some("namespacePid"),
                false,
            ));
        }
        let process_start_identity = process_identity(main_pid).ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::LaunchIdentityMismatch,
                "systemd MainPID has no observable host process identity",
                Some("mainPid"),
                false,
            )
        })?;
        let runner_start_digest = sha256_bytes(&bytes);
        if runner_start_digest != sha256_file(&path).map_err(map_universal_error)? {
            return Err(M6Error::new(
                M6ErrorCode::LaunchIdentityMismatch,
                "runner-start evidence digest changed while reading",
                Some("runnerStart"),
                false,
            ));
        }
        let boot_id = read_trimmed("/proc/sys/kernel/random/boot_id")?;
        self.registry.bind_running(
            &attempt.attempt_id,
            attempt.row_version,
            &RunnerIdentityM6 {
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

    fn reconcile_runner_result(&self, attempt: &AttemptRecordM6) -> M6Result<()> {
        match self.commit_runner_result(attempt) {
            Ok(_) => Ok(()),
            Err(error)
                if matches!(
                    error.code,
                    M6ErrorCode::RegistryCorrupt
                        | M6ErrorCode::ResultIdentityConflict
                        | M6ErrorCode::ArtifactIdentityConflict
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

    fn commit_runner_result(&self, attempt: &AttemptRecordM6) -> M6Result<M6TaskObservation> {
        let current = self.registry.get_attempt(&attempt.attempt_id)?;
        if current.state.is_terminal() {
            return self.observation_from_registry(&current.job_id, 0, 0);
        }
        let result_path = Path::new(&current.bundle_path).join(RESULT_FILE);
        let bytes =
            fs::read(&result_path).map_err(|error| io_error("read Runner result", error))?;
        let result: RunnerTaskResult = serde_json::from_slice(&bytes).map_err(|error| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
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
            return Err(M6Error::new(
                M6ErrorCode::ResultIdentityConflict,
                "Runner result identity does not match committed Attempt",
                Some("result"),
                false,
            ));
        }
        let result_digest = sha256_bytes(&bytes);
        let stdout = self.validate_captured_output(&current, &result.stdout, true)?;
        let stderr = self.validate_captured_output(&current, &result.stderr, false)?;
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
        artifacts.push(ArtifactRegistrationM6 {
            artifact_id: format!("{}.result", current.attempt_id),
            kind: "execution_result".to_string(),
            relative_path: RESULT_FILE.to_string(),
            digest: result_digest.clone(),
            media_type: "application/json".to_string(),
            byte_length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            truncated: false,
        });
        let projection = self.registry.commit_terminal(&TerminalCommitM6 {
            attempt_id: current.attempt_id.clone(),
            expected_row_version: current.row_version,
            state,
            result_digest,
            exit_code: result.exit_code,
            infrastructure_error_digest,
            finished_at_ms: u64::try_from(result.finished_unix_ms).unwrap_or(u64::MAX),
            artifacts,
            reason_code: reason_code.to_string(),
        })?;
        self.cleanup_payload_view(&current.attempt_id)?;
        self.observation_from_parts(projection, Some(current), 4096, 4096)
    }

    fn validate_captured_output(
        &self,
        attempt: &AttemptRecordM6,
        output: &CapturedOutput,
        stdout: bool,
    ) -> M6Result<ArtifactRegistrationM6> {
        let expected_file = if stdout { STDOUT_FILE } else { STDERR_FILE };
        let expected_kind = if stdout { "stdout" } else { "stderr" };
        let expected_id = format!("{}.{}", attempt.attempt_id, expected_kind);
        if output.file_name != expected_file || output.artifact_id != expected_id {
            return Err(M6Error::new(
                M6ErrorCode::ArtifactIdentityConflict,
                "Runner output identity does not match Attempt",
                Some("artifact"),
                false,
            ));
        }
        let path = Path::new(&attempt.bundle_path).join(expected_file);
        let metadata = fs::metadata(&path).map_err(|error| io_error("inspect output", error))?;
        let digest = sha256_file(&path).map_err(map_universal_error)?;
        if digest != output.digest || metadata.len() != output.retained_bytes {
            return Err(M6Error::new(
                M6ErrorCode::ArtifactIdentityConflict,
                "Runner output digest or byte length changed",
                Some("artifact"),
                false,
            ));
        }
        Ok(ArtifactRegistrationM6 {
            artifact_id: expected_id,
            kind: expected_kind.to_string(),
            relative_path: expected_file.to_string(),
            digest,
            media_type: "text/plain; charset=utf-8".to_string(),
            byte_length: metadata.len(),
            truncated: output.truncated,
        })
    }

    pub fn observe_task(&self, request: &M6TaskObserveRequest) -> M6Result<M6TaskObservation> {
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
    ) -> M6Result<M6TaskObservation> {
        let snapshot = self.registry.job_snapshot(job_id)?;
        self.observation_from_snapshot(snapshot, stdout_tail_bytes, stderr_tail_bytes)
    }

    fn observation_from_snapshot(
        &self,
        snapshot: JobSnapshotM6,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> M6Result<M6TaskObservation> {
        self.observation_from_parts(
            snapshot.projection,
            snapshot.attempt,
            stdout_tail_bytes,
            stderr_tail_bytes,
        )
    }

    fn observation_from_parts(
        &self,
        projection: super::JobProjectionM6,
        attempt: Option<AttemptRecordM6>,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> M6Result<M6TaskObservation> {
        let (stdout_tail, stderr_tail, stdout_truncated, stderr_truncated, error_summary) =
            if let Some(attempt) = &attempt {
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
                let error_summary = result
                    .and_then(|result| result.infrastructure_error)
                    .or(result_error);
                (
                    stdout_tail,
                    stderr_tail,
                    stdout_truncated,
                    stderr_truncated,
                    error_summary,
                )
            } else {
                (String::new(), String::new(), false, false, None)
            };
        Ok(M6TaskObservation {
            job_id: projection.job_id,
            status: projection.status,
            attempt_id: attempt.map(|attempt| attempt.attempt_id),
            exit_code: projection.exit_code,
            stdout_tail,
            stderr_tail,
            stdout_truncated,
            stderr_truncated,
            artifacts_available: projection.artifacts_available,
            poll_after_ms: projection.poll_after_ms,
            error_summary,
        })
    }

    fn reconcile_job(&self, job_id: &str) -> M6Result<()> {
        let snapshot = self.registry.job_snapshot(job_id)?;
        if snapshot.job.resolution.is_some() {
            return Ok(());
        }
        let attempt = snapshot.attempt.ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
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

    pub fn reconcile_all(&self) -> M6Result<Vec<M6TaskObservation>> {
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

    pub fn reconcile_attempt(&self, attempt_id: &str) -> M6Result<()> {
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

    fn reconcile_starting_without_token(&self, attempt: &AttemptRecordM6) -> M6Result<()> {
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

    fn reconcile_bound_attempt(&self, attempt: &AttemptRecordM6) -> M6Result<()> {
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
            super::M6TerminationIntent::Natural => TerminationIntent::Natural,
            super::M6TerminationIntent::StopRequested => TerminationIntent::StopRequested,
            super::M6TerminationIntent::DeadlineExceeded => TerminationIntent::DeadlineExceeded,
        };
        let disposition = classify_supervisor_recovery(
            &expected,
            &observation,
            RunnerResultObservation::Missing,
            intent,
        )
        .map_err(|error| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                format!("supervisor recovery classification failed: {error}"),
                Some("attemptId"),
                false,
            )
        })?;
        match disposition {
            SupervisorRecoveryDisposition::Running => Ok(()),
            SupervisorRecoveryDisposition::Terminal { state, .. } => {
                self.commit_control_terminal(
                    attempt,
                    map_job_state(state)?,
                    "SUPERVISOR_TERMINAL_FALLBACK",
                    None,
                )?;
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
            SupervisorRecoveryDisposition::Orphaned { reason } => {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Orphaned,
                    "SUPERVISOR_IDENTITY_ORPHANED",
                    Some(reason),
                )?;
                Ok(())
            }
        }
    }

    fn commit_control_terminal(
        &self,
        attempt: &AttemptRecordM6,
        state: AttemptState,
        reason_code: &str,
        detail: Option<String>,
    ) -> M6Result<M6TaskObservation> {
        let current = self.registry.get_attempt(&attempt.attempt_id)?;
        if current.state.is_terminal() {
            return self.observation_from_registry(&current.job_id, 0, 0);
        }
        let observed_at_ms = now_ms()?;
        let evidence = ControlTerminalEvidence {
            schema_version: M6_SCHEMA_VERSION,
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
        let mut artifacts = vec![ArtifactRegistrationM6 {
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
                    artifacts.push(ArtifactRegistrationM6 {
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
        let projection = self.registry.commit_terminal(&TerminalCommitM6 {
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

    pub fn cancel_task(&self, request: &M6TaskCancelRequest) -> M6Result<M6TaskObservation> {
        if request.schema_version != M6_SCHEMA_VERSION {
            return Err(M6Error::invalid(
                "unsupported M6 schema version",
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
                M6Error::new(
                    M6ErrorCode::RegistryCorrupt,
                    "cancelled Job has no Attempt",
                    Some("jobId"),
                    false,
                )
            })?;
        write_json_atomic(
            &Path::new(&attempt.bundle_path).join(CANCEL_FILE),
            &serde_json::json!({
                "schemaVersion": M6_SCHEMA_VERSION,
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

    pub fn list_jobs(&self, request: &JobListRequestM6) -> M6Result<JobListResultM6> {
        self.registry.list_jobs(request)
    }

    pub fn read_artifact(&self, request: &M6ArtifactReadRequest) -> M6Result<M6ArtifactReadResult> {
        if request.schema_version != M6_SCHEMA_VERSION {
            return Err(M6Error::invalid(
                "unsupported M6 schema version",
                "schemaVersion",
            ));
        }
        if request.max_bytes == 0 || request.max_bytes > MAX_M6_ARTIFACT_READ_BYTES {
            return Err(M6Error::invalid(
                format!("maxBytes must be in 1..={MAX_M6_ARTIFACT_READ_BYTES}"),
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
            return Err(M6Error::new(
                M6ErrorCode::ArtifactIdentityConflict,
                "Artifact path is not a regular non-symlink file",
                Some("artifactId"),
                false,
            ));
        }
        let canonical =
            fs::canonicalize(&path).map_err(|error| io_error("canonicalize Artifact", error))?;
        if !canonical.starts_with(&bundle) {
            return Err(M6Error::new(
                M6ErrorCode::ArtifactIdentityConflict,
                "Artifact escaped Attempt bundle",
                Some("artifactId"),
                false,
            ));
        }
        if sha256_file(&canonical).map_err(map_universal_error)? != artifact.digest
            || metadata.len() != artifact.byte_length
        {
            return Err(M6Error::new(
                M6ErrorCode::ArtifactIdentityConflict,
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
        Ok(M6ArtifactReadResult {
            job_id: request.job_id.clone(),
            artifact_id: request.artifact_id.clone(),
            content: String::from_utf8_lossy(&bytes).into_owned(),
            offset: request.offset,
            next_offset,
            eof: next_offset >= artifact.byte_length,
            digest: artifact.digest,
        })
    }

    #[cfg(feature = "runtime-hardening-m7")]
    fn cleanup_payload_view(&self, attempt_id: &str) -> M6Result<()> {
        let Some(hardening) = &self.hardening else {
            return Ok(());
        };
        let view_root = hardening.payload_view_root(attempt_id);
        if view_root.exists() {
            fs::remove_dir_all(&view_root)
                .map_err(|error| io_error("remove M7 payload view", error))?;
        }
        Ok(())
    }

    #[cfg(not(feature = "runtime-hardening-m7"))]
    fn cleanup_payload_view(&self, _attempt_id: &str) -> M6Result<()> {
        Ok(())
    }

    #[cfg(feature = "runtime-hardening-m7")]
    fn payload_config(
        &self,
        attempt_id: &str,
        plan: &M6ExecutionPlan,
    ) -> M6Result<Option<RunnerPayloadConfig>> {
        let Some(hardening) = &self.hardening else {
            return Ok(None);
        };
        let runtime_dir = hardening.payload_runtime_dir(attempt_id);
        crate::m7::ensure_owned_directory(
            &runtime_dir,
            hardening.worker.uid,
            hardening.worker.gid,
            0o700,
        )?;
        let view_root = hardening.payload_view_root(attempt_id);
        for path in [
            view_root.clone(),
            view_root.join("workspace"),
            view_root.join("runtime"),
            view_root.join("cache"),
        ] {
            crate::m7::ensure_traversal_directory(&path, hardening.worker.gid, 0o750)?;
        }
        let workspace = Path::new(&plan.workspace_path);
        let cwd = Path::new(&plan.cwd);
        let relative_cwd = cwd
            .strip_prefix(workspace)
            .map_err(|_| M6Error::invalid("M7 cwd escaped workspace", "cwd"))?;
        let workspace_view = view_root.join("workspace");
        Ok(Some(RunnerPayloadConfig {
            uid: hardening.worker.uid,
            gid: hardening.worker.gid,
            workspace_view: workspace_view.to_string_lossy().into_owned(),
            cwd_view: workspace_view
                .join(relative_cwd)
                .to_string_lossy()
                .into_owned(),
            runtime_view: view_root.join("runtime").to_string_lossy().into_owned(),
            cache_view: view_root.join("cache").to_string_lossy().into_owned(),
        }))
    }

    #[cfg(not(feature = "runtime-hardening-m7"))]
    fn payload_config(
        &self,
        _attempt_id: &str,
        _plan: &M6ExecutionPlan,
    ) -> M6Result<Option<RunnerPayloadConfig>> {
        Ok(None)
    }

    fn payload_evidence_matches(&self, uid: Option<u32>, gid: Option<u32>) -> bool {
        #[cfg(feature = "runtime-hardening-m7")]
        {
            match &self.hardening {
                Some(hardening) => {
                    uid == Some(hardening.worker.uid) && gid == Some(hardening.worker.gid)
                }
                None => uid.is_none() && gid.is_none(),
            }
        }
        #[cfg(not(feature = "runtime-hardening-m7"))]
        {
            uid.is_none() && gid.is_none()
        }
    }
}

#[cfg(feature = "runtime-hardening-m7")]
fn validate_hardening_roots(
    config: &M6RuntimeConfig,
    hardening: &crate::M7RuntimeHardeningConfig,
) -> M6Result<()> {
    if !config
        .registry
        .store_root
        .starts_with(&hardening.control_root)
        || !config.registry.db_path.starts_with(&hardening.control_root)
        || !config
            .executor
            .store_root
            .starts_with(&hardening.control_root)
    {
        return Err(M6Error::invalid(
            "Registry and executor metadata must remain under M7 controlRoot",
            "controlRoot",
        ));
    }
    if config.executor.workspace_root.as_ref() != Some(&hardening.workspaces_root())
        || config.executor.workspace_uid != Some(hardening.worker.uid)
        || config.executor.workspace_gid != Some(hardening.worker.gid)
    {
        return Err(M6Error::invalid(
            "executor workspace root and owner must match M7 worker configuration",
            "workspaceRoot",
        ));
    }
    Ok(())
}

fn validate_run_request(request: &M6TaskRunRequest) -> M6Result<()> {
    if request.schema_version != M6_SCHEMA_VERSION {
        return Err(M6Error::invalid(
            "unsupported M6 schema version",
            "schemaVersion",
        ));
    }
    for (value, field) in [
        (&request.client_request_id, "clientRequestId"),
        (&request.principal, "principal"),
        (&request.authority_ref, "authorityRef"),
        (&request.policy_id, "policyId"),
        (&request.policy_version, "policyVersion"),
        (&request.execution.workspace_id, "execution.workspaceId"),
    ] {
        validate_text_id(value, field)?;
    }
    validate_sha256(&request.policy_digest, "policyDigest")?;
    if request.profile_id.is_some() != request.profile_limit.is_some() {
        return Err(M6Error::invalid(
            "profileId and profileLimit must appear together",
            "profileLimit",
        ));
    }
    if request.global_limit == 0 || request.profile_limit == Some(0) {
        return Err(M6Error::invalid(
            "concurrency limits must be positive",
            "globalLimit",
        ));
    }
    if request.wait_ms > MAX_M6_WAIT_MS
        || request.stdout_tail_bytes > MAX_M6_TAIL_BYTES
        || request.stderr_tail_bytes > MAX_M6_TAIL_BYTES
    {
        return Err(M6Error::invalid(
            "wait or tail bounds exceed the M6 compact limit",
            "waitMs",
        ));
    }
    if request.execution.executable.is_empty()
        || !Path::new(&request.execution.executable).is_absolute()
        || request.execution.cwd_relative.is_empty()
        || Path::new(&request.execution.cwd_relative).is_absolute()
    {
        return Err(M6Error::invalid(
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
        return Err(M6Error::invalid(
            "cwdRelative cannot contain parent traversal",
            "execution.cwdRelative",
        ));
    }
    if request.execution.timeout_ms == 0
        || request.execution.stdout_limit_bytes == 0
        || request.execution.stderr_limit_bytes == 0
    {
        return Err(M6Error::invalid(
            "runtime and output limits must be positive",
            "execution",
        ));
    }
    if request.execution.args.len() > 128 || request.execution.env.len() > 64 {
        return Err(M6Error::invalid(
            "args or environment exceed M6 bounds",
            "execution",
        ));
    }
    Ok(())
}

fn validate_observe_request(request: &M6TaskObserveRequest) -> M6Result<()> {
    if request.schema_version != M6_SCHEMA_VERSION {
        return Err(M6Error::invalid(
            "unsupported M6 schema version",
            "schemaVersion",
        ));
    }
    validate_text_id(&request.job_id, "jobId")?;
    if request.wait_ms > MAX_M6_WAIT_MS
        || request.stdout_tail_bytes > MAX_M6_TAIL_BYTES
        || request.stderr_tail_bytes > MAX_M6_TAIL_BYTES
    {
        return Err(M6Error::invalid(
            "observe bounds exceed M6 limits",
            "waitMs",
        ));
    }
    Ok(())
}

fn validate_executable(config: &UniversalExecutorConfig, value: &str) -> M6Result<PathBuf> {
    let path = Path::new(value);
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect executable", error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(M6Error::invalid(
            "executable must be a non-symlink executable file",
            "execution.executable",
        ));
    }
    let canonical =
        fs::canonicalize(path).map_err(|error| io_error("canonicalize executable", error))?;
    let allowed = config.allowed_executable_roots.iter().any(|root| {
        fs::canonicalize(root)
            .map(|root| canonical.starts_with(root))
            .unwrap_or(false)
    });
    if !allowed {
        return Err(M6Error::new(
            M6ErrorCode::InvalidRequest,
            "executable is outside configured roots",
            Some("execution.executable"),
            false,
        ));
    }
    Ok(canonical)
}

fn validate_runner(path: &Path) -> M6Result<PathBuf> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error("inspect Runner", error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(M6Error::invalid(
            "Runner must be a non-symlink executable file",
            "runnerPath",
        ));
    }
    fs::canonicalize(path).map_err(|error| io_error("canonicalize Runner", error))
}

fn systemd_run(
    unit_name: &str,
    runner: &Path,
    bundle_path: &Path,
    _workspace_path: &Path,
    runtime_ceiling_ms: u64,
    #[cfg(feature = "runtime-hardening-m7")] hardening: Option<&crate::M7RuntimeHardeningConfig>,
) -> M6Result<std::process::Output> {
    let mut command = Command::new("systemd-run");
    command
        .arg(format!("--unit={unit_name}"))
        .arg("--collect")
        .args([
            "--property=Type=exec",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=2s",
            "--property=SendSIGKILL=yes",
            "--property=NoNewPrivileges=yes",
            "--property=AmbientCapabilities=",
            "--property=ProtectSystem=strict",
            "--property=PrivateTmp=yes",
            "--property=PrivateNetwork=yes",
            "--property=PrivateDevices=yes",
            "--property=PrivateIPC=yes",
            "--property=PrivatePIDs=yes",
            "--property=ProtectProc=invisible",
            "--property=ProcSubset=pid",
            "--property=RestrictNamespaces=yes",
            "--property=RestrictAddressFamilies=AF_UNIX",
            "--property=ProtectKernelTunables=yes",
            "--property=ProtectKernelModules=yes",
            "--property=ProtectControlGroups=yes",
            "--property=ProtectHostname=yes",
            "--property=ProtectClock=yes",
            "--property=RestrictSUIDSGID=yes",
            "--property=LockPersonality=yes",
            "--property=SystemCallArchitectures=native",
            "--property=InaccessiblePaths=-/run/systemd/private -/run/dbus/system_bus_socket -/run/docker.sock -/var/run/docker.sock -/run/credentials -/root/.ssh -/root/.cloudflared -/root/.config -/root/.aws -/root/.kube -/root/.docker -/root/.git-credentials -/root/.netrc",
            "--property=UMask=0077",
            "--property=TasksMax=128",
            "--property=MemoryMax=1073741824",
            "--property=StandardOutput=journal",
            "--property=StandardError=journal",
        ])
        .arg(format!(
            "--property=RuntimeMaxSec={runtime_ceiling_ms}ms"
        ))
        .arg(format!("--property=ReadWritePaths={}", bundle_path.display()));
    #[cfg(feature = "runtime-hardening-m7")]
    if let Some(hardening) = hardening {
        let attempt_id = bundle_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                M6Error::invalid("bundle path omitted Attempt identity", "bundlePath")
            })?;
        let runtime_dir = hardening.payload_runtime_dir(attempt_id);
        let view_root = hardening.payload_view_root(attempt_id);
        command
            .arg("--property=CapabilityBoundingSet=CAP_SETUID CAP_SETGID")
            .arg("--property=ProtectHome=yes")
            .arg(format!(
                "--property=InaccessiblePaths={}",
                hardening.worker_root.display()
            ))
            .arg(format!(
                "--property=InaccessiblePaths={}",
                hardening.cache_root.display()
            ))
            .arg(format!(
                "--property=BindPaths={}:{}",
                _workspace_path.display(),
                view_root.join("workspace").display()
            ))
            .arg(format!(
                "--property=BindPaths={}:{}",
                runtime_dir.display(),
                view_root.join("runtime").display()
            ))
            .arg(format!(
                "--property=BindPaths={}:{}",
                hardening.cache_root.display(),
                view_root.join("cache").display()
            ))
            .arg(format!(
                "--property=InaccessiblePaths={}/workspace/.git",
                view_root.display()
            ));
    } else {
        command.arg("--property=CapabilityBoundingSet=");
    }
    #[cfg(not(feature = "runtime-hardening-m7"))]
    command.arg("--property=CapabilityBoundingSet=");
    command
        .arg(runner)
        .arg("--task-dir")
        .arg(bundle_path)
        .output()
        .map_err(|error| tool_error("cannot execute systemd-run", error))
}

fn systemctl_show(unit_name: &str) -> M6Result<BTreeMap<String, String>> {
    let output = Command::new("systemctl")
        .args([
            "show",
            unit_name,
            "--property=LoadState,ActiveState,SubState,InvocationID,ControlGroup,MainPID,Result,ExecMainCode,ExecMainStatus",
        ])
        .output()
        .map_err(|error| tool_error("cannot execute systemctl show", error))?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(M6Error::new(
            M6ErrorCode::ToolFailed,
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
) -> M6Result<()> {
    if properties.get(key).map(String::as_str) != Some(expected) {
        return Err(M6Error::new(
            M6ErrorCode::LaunchIdentityMismatch,
            format!("systemd {key} does not match runner-start evidence"),
            Some(key),
            false,
        ));
    }
    Ok(())
}

fn missing_systemd_property(key: &str) -> M6Error {
    M6Error::new(
        M6ErrorCode::LaunchIdentityMismatch,
        format!("systemd omitted {key}"),
        Some(key),
        false,
    )
}

fn supervisor_identity(attempt: &AttemptRecordM6) -> M6Result<SupervisorIdentity> {
    Ok(SupervisorIdentity {
        boot_id: attempt.boot_id.clone().ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "bound Attempt has no bootId",
                Some("bootId"),
                false,
            )
        })?,
        unit_name: attempt.unit_name.clone(),
        invocation_id: attempt.invocation_id.clone().ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "bound Attempt has no invocationId",
                Some("invocationId"),
                false,
            )
        })?,
        control_group: attempt.control_group.clone().ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "bound Attempt has no controlGroup",
                Some("controlGroup"),
                false,
            )
        })?,
        main_pid: attempt.main_pid.ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "bound Attempt has no mainPid",
                Some("mainPid"),
                false,
            )
        })?,
        main_process_start_identity: attempt.process_start_identity.clone().ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "bound Attempt has no process start identity",
                Some("processStartIdentity"),
                false,
            )
        })?,
    })
}

fn map_job_state(state: JobInternalState) -> M6Result<AttemptState> {
    match state {
        JobInternalState::Succeeded => Ok(AttemptState::Succeeded),
        JobInternalState::Failed => Ok(AttemptState::Failed),
        JobInternalState::TimedOut => Ok(AttemptState::TimedOut),
        JobInternalState::Cancelled => Ok(AttemptState::Cancelled),
        JobInternalState::Lost => Ok(AttemptState::Lost),
        JobInternalState::Orphaned => Ok(AttemptState::Orphaned),
        _ => Err(M6Error::new(
            M6ErrorCode::RegistryCorrupt,
            "supervisor terminal classification returned a non-terminal state",
            Some("state"),
            false,
        )),
    }
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

fn read_trimmed(path: &str) -> M6Result<String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| io_error(&format!("read {path}"), error))
}

fn load_runner_result_if_present(attempt: &AttemptRecordM6) -> M6Result<Option<RunnerTaskResult>> {
    let path = Path::new(&attempt.bundle_path).join(RESULT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| io_error("read Runner result", error))?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        M6Error::new(
            M6ErrorCode::RegistryCorrupt,
            format!("invalid Runner result: {error}"),
            Some("result"),
            false,
        )
    })
}

fn read_tail_text(path: &Path, max_bytes: u64) -> M6Result<String> {
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

fn write_bytes_synced(path: &Path, bytes: &[u8]) -> M6Result<()> {
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

fn sync_directory(path: &Path) -> M6Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| io_error("sync directory", error))
}

fn now_ms() -> M6Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            M6Error::new(
                M6ErrorCode::RegistryUnavailable,
                format!("system clock precedes Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis()
        .try_into()
        .map_err(|_| {
            M6Error::new(
                M6ErrorCode::RegistryUnavailable,
                "current time does not fit u64 milliseconds",
                None,
                false,
            )
        })
}

fn validate_text_id(value: &str, field: &str) -> M6Result<()> {
    if value.trim().is_empty()
        || value.len() > 256
        || value.as_bytes().contains(&0)
        || value.chars().any(char::is_control)
    {
        return Err(M6Error::invalid(
            format!("{field} must be non-empty, bounded, and control-free"),
            field,
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> M6Result<()> {
    let valid = value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid {
        return Err(M6Error::invalid(
            format!("{field} must be a SHA-256 digest"),
            field,
        ));
    }
    Ok(())
}

fn serialization_error(error: serde_json::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::RegistryUnavailable,
        format!("cannot serialize M6 bundle: {error}"),
        None,
        false,
    )
}

fn map_universal_error(error: crate::UniversalExecError) -> M6Error {
    M6Error::new(
        M6ErrorCode::InvalidRequest,
        error.message,
        error.field.as_deref(),
        error.retryable,
    )
}

fn io_error(context: &str, error: std::io::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::IoError,
        format!("{context}: {error}"),
        None,
        false,
    )
}

fn tool_error(context: &str, error: std::io::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::ToolUnavailable,
        format!("{context}: {error}"),
        None,
        true,
    )
}

fn ensure_systemd_visible(path: &Path, field: &str) -> M6Result<()> {
    for private_root in ["/tmp", "/var/tmp", "/dev/shm"] {
        if path.starts_with(private_root) {
            return Err(M6Error::invalid(
                format!("{field} is hidden by the Runner PrivateTmp boundary"),
                field,
            ));
        }
    }
    Ok(())
}
