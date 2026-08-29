use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::evidence::prepare_runner_terminal_from_bundle;
use super::patch::{
    durable_patch_request_digest, validate_durable_patch_request, validate_patch_status_request,
};
use super::registry::JobSnapshot;
use super::supervisor::{
    classify_supervisor_recovery, classify_windows_launcher_recovery, AttemptSupervisorOwner,
    SupervisorObservation, SupervisorRecoveryDisposition, SupervisorUnitState, TerminationIntent,
};
use super::systemd::*;
use super::windows::*;
use super::{
    runtime_release_effect_id, runtime_release_request_identity_digest, validate_client_request_id,
    validate_logical_id, AdmissionOutcome, ArtifactDescriptor, ArtifactReadRequest,
    ArtifactReadResult, ArtifactRegistration, AttemptRecord, AttemptState,
    AttemptTerminationIntent, DurableWorkspacePatchRequest, DurableWorkspacePatchResult,
    EffectiveInputBinding, ExecutionProviderContract, ExecutionProviderSnapshot,
    HostDependencyBinding, InputAccessMode, InputAuthority, InputBindingRequest, JobDesiredState,
    JobResolution, Registry, RegistryConfig, RunnerIdentity, RuntimeArtifactRecord,
    RuntimeCapabilities, RuntimeError, RuntimeErrorCode, RuntimeExecutionPlan,
    RuntimeExecutionStep, RuntimeExecutionTargetCapability, RuntimeJobListRequest,
    RuntimeJobListResult, RuntimeReleaseAdmission, RuntimeReleaseContract,
    RuntimeReleaseDisposition, RuntimeReleaseEffectBinding, RuntimeReleaseGetRequest,
    RuntimeReleaseProjection, RuntimeReleaseRequest, RuntimeResult, RuntimeWorkspaceGetRequest,
    RuntimeWorkspaceIssue, RuntimeWorkspaceIssueStage, RuntimeWorkspaceListRequest,
    RuntimeWorkspaceListResult, RuntimeWorkspaceSummary, SubmitRequest, TaskCancelRequest,
    TaskObservation, TaskObserveRequest, TaskObserveWaitUntil, TaskRunRequest, TerminalCommit,
    WorkspacePatchOperationState, WorkspacePatchOperationStatus, WorkspacePatchStatusRequest,
    MAX_ARTIFACT_READ_BYTES, MAX_TASK_TAIL_BYTES, MAX_TASK_WAIT_MS, RUNTIME_SCHEMA_VERSION,
};
use crate::universal::{
    canonical_directory, create_git_workspace_compact, inspect_workspace_patch_plan,
    list_open_workspace_record_inventory, load_workspace_record, mutate_workspace,
    open_regular_file_beneath, patch_workspace, plan_workspace_patch, remove_git_workspace,
    resolve_workspace_cwd, result_from_workspace_patch_plan, sha256_bytes, sha256_file,
    workspace_cleanup_dependents, workspace_git_common_dir_at, workspace_head_and_dirty_at,
    workspace_head_revision, workspace_source_state_digest, write_bytes_atomic, write_json_atomic,
    CompactWorkspaceOpenResult, GitWorkspaceCreateRequest, RunnerExecutionStep,
    RunnerHostDependencyCommitment, RunnerInputCommitment, RunnerPayloadConfig,
    RunnerStartEvidence, RunnerTaskProgress, RunnerTaskRequest, RunnerTaskResult,
    UniversalExecutorConfig, WorkspaceCloseRequest, WorkspaceCloseResult, WorkspaceDiffRequest,
    WorkspaceMutateRequest, WorkspaceMutateResult, WorkspacePatchPlanState, WorkspacePatchRequest,
    WorkspacePatchResult, UNIVERSAL_EXEC_SCHEMA_VERSION,
};

const RUNNER_REQUEST_FILE: &str = "request.json";
const PLAN_FILE: &str = "plan.json";
const BUNDLE_MANIFEST_FILE: &str = "bundle-manifest.json";
const RUNNER_START_FILE: &str = "runner-start.json";
const WINDOWS_LAUNCHER_START_FILE: &str = "windows-launcher-start.json";
const WINDOWS_START_FILE: &str = "windows-start.json";
const RESULT_FILE: &str = "result.json";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";
const PROGRESS_FILE: &str = "progress.json";
const CANCEL_FILE: &str = "cancel-requested.json";
const CONTROL_RESULT_FILE: &str = "control-result.json";
const ORPHAN_REMEDIATION_FILE: &str = "orphan-remediation.json";
const TERMINAL_EVIDENCE_FILE_PREFIX: &str = "terminal-evidence-";
const INTERACTIVE_RECONCILIATION_LIMIT: u32 = 32;
const ADAPTIVE_POLL_DELAYS_MS: [u64; 5] = [2, 5, 10, 20, 50];
const DEFAULT_EXECUTION_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const DEFAULT_EXECUTION_HOME: &str = "/root";
const STALE_PREPARED_INPUT_AGE_MS: u64 = 60_000;
const WINDOWS_NATIVE_OUTER_DEADLINE_GRACE_MS: u64 = 5_000;
const HOST_DEPENDENCY_CONTINUITY_SCOPE: &str = "runtime_host_namespace_path_witness";
const TRUSTED_BUILD_TARGET_PRESENTATION: &str = "/proc/self/fd/198";
const WINDOWS_INPUT_PRESENTATION_COMPONENTS: [&str; 1] = ["OrdivonImmutableInputs"];
const CONTAINED_RUNTIME_ENVIRONMENT: [&str; 15] = [
    "HOME",
    "TMPDIR",
    "XDG_CACHE_HOME",
    "CARGO_TARGET_DIR",
    "UV_CACHE_DIR",
    "PIP_CACHE_DIR",
    "npm_config_cache",
    "PNPM_HOME",
    "COREPACK_HOME",
    "BUN_INSTALL_CACHE_DIR",
    "GOMODCACHE",
    "GOCACHE",
    "GIT_OPTIONAL_LOCKS",
    "ORDIVON_PAYLOAD_UID",
    "ORDIVON_PAYLOAD_GID",
];

fn set_environment_value_case_insensitive(
    environment: &mut BTreeMap<String, String>,
    name: &str,
    value: String,
) {
    if let Some(existing) = environment
        .keys()
        .find(|existing| existing.eq_ignore_ascii_case(name))
        .cloned()
    {
        environment.remove(&existing);
    }
    environment.insert(name.to_string(), value);
}

fn required_environment_value_case_insensitive<'a>(
    environment: &'a BTreeMap<String, String>,
    name: &str,
    field: &str,
) -> RuntimeResult<&'a str> {
    environment
        .iter()
        .find(|(actual, value)| actual.eq_ignore_ascii_case(name) && !value.is_empty())
        .map(|(_, value)| value.as_str())
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("committed Windows environment omitted {name}"),
                Some(field),
                false,
            )
        })
}

fn windows_input_presentation_root(
    plan: &RuntimeExecutionPlan,
    input_set_id: &str,
) -> RuntimeResult<String> {
    let program_data =
        required_environment_value_case_insensitive(&plan.env, "ProgramData", "executionPlan.env")?;
    let mut root = program_data.trim_end_matches(['\\', '/']).to_string();
    for component in WINDOWS_INPUT_PRESENTATION_COMPONENTS {
        root.push('\\');
        root.push_str(component);
    }
    root.push('\\');
    root.push_str(input_set_id);
    Ok(root)
}

pub(crate) fn windows_input_bindings_digest(inputs: &[EffectiveInputBinding]) -> String {
    let mut ordered = inputs.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.presentation_relative_path
            .cmp(&right.presentation_relative_path)
    });
    let mut directories = BTreeSet::<String>::new();
    for input in &ordered {
        let components = input
            .presentation_relative_path
            .split('/')
            .collect::<Vec<_>>();
        for end in 1..components.len() {
            directories.insert(components[..end].join("/"));
        }
    }
    let mut bytes = b"windows-immutable-input-tree-v2\0".to_vec();
    for directory in directories {
        bytes.extend_from_slice(b"D\0");
        bytes.extend_from_slice(directory.as_bytes());
        bytes.push(0);
    }
    for input in ordered {
        bytes.extend_from_slice(b"F\0");
        bytes.extend_from_slice(input.presentation_relative_path.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(input.digest.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(input.byte_length.to_string().as_bytes());
        bytes.push(0);
    }
    sha256_bytes(&bytes)
}

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
    pub windows: Option<WindowsExecutionConfig>,
}

#[derive(Clone, Debug)]
struct OpenedInputAuthority {
    root: Arc<File>,
}

#[derive(Debug)]
struct PreparedInputSet {
    input_set_id: String,
    prepared_root: PathBuf,
    effective_inputs: Vec<EffectiveInputBinding>,
}

#[derive(Clone, Debug)]
pub struct Runtime {
    registry: Registry,
    executor: UniversalExecutorConfig,
    startup_grace_ms: u64,
    execution_path: String,
    execution_home: String,
    windows: Option<WindowsExecutionConfig>,
    input_authorities: BTreeMap<String, OpenedInputAuthority>,
    lifecycle_lock: Arc<Mutex<()>>,
    control_terminal_lock: Arc<Mutex<()>>,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalSupervisorEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    boot_id: Option<String>,
    unit_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    main_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    process_start_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runner_start_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObservedSupervisorEvidence {
    boot_id: String,
    unit_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    main_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    main_process_start_identity: Option<String>,
    recorded_pid_alive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    recorded_pid_start_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_main_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_main_status: Option<i32>,
}

impl From<&SupervisorObservation> for ObservedSupervisorEvidence {
    fn from(observation: &SupervisorObservation) -> Self {
        Self {
            boot_id: observation.boot_id.clone(),
            unit_state: match observation.unit_state {
                SupervisorUnitState::Running => "running",
                SupervisorUnitState::Terminal => "terminal",
                SupervisorUnitState::NotFound => "not_found",
            }
            .to_string(),
            invocation_id: observation.invocation_id.clone(),
            control_group: observation.control_group.clone(),
            main_pid: observation.main_pid,
            main_process_start_identity: observation.main_process_start_identity.clone(),
            recorded_pid_alive: observation.recorded_pid_alive,
            recorded_pid_start_identity: observation.recorded_pid_start_identity.clone(),
            result: observation.result.clone(),
            exec_main_code: observation.exec_main_code,
            exec_main_status: observation.exec_main_status,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalProcessEvidence {
    schema_version: u32,
    job_id: String,
    attempt_id: String,
    operation_digest: String,
    execution_plan_digest: String,
    workspace_id: String,
    source_revision: String,
    execution_profile: super::ExecutionProfile,
    #[serde(default, skip_serializing_if = "super::ExecutionTarget::is_default")]
    execution_target: super::ExecutionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_provider: Option<ExecutionProviderSnapshot>,
    #[serde(default)]
    windows_authority: super::WindowsAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    windows_execution_context: Option<super::WindowsExecutionContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    foreign_references: Vec<super::ForeignReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    host_dependencies: Vec<HostDependencyBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_dependency_continuity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_dependency_continuity_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    effective_inputs: Vec<EffectiveInputBinding>,
    executable: String,
    executable_digest: String,
    args: Vec<String>,
    cwd: String,
    supervisor: TerminalSupervisorEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_supervisor: Option<ObservedSupervisorEvidence>,
    start_disposition: String,
    cancellation_disposition: String,
    execution_disposition: String,
    delivery_disposition: String,
    process_tree_disposition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    process_tree_detail: Option<String>,
    reason_code: String,
    terminal_artifact_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supersedes_artifact_id: Option<String>,
    observed_at_ms: u64,
}

#[derive(Clone, Copy)]
struct ObservationOutputRequest {
    stdout_tail_bytes: u64,
    stderr_tail_bytes: u64,
    stdout_offset: Option<u64>,
    stderr_offset: Option<u64>,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> RuntimeResult<Self> {
        Self::new_with_input_authorities(config, Vec::new())
    }

    /// Construction boundary for operator-owned immutable input authorities.
    /// Authority roots are Runtime-instance configuration, never action input.
    pub fn new_with_input_authorities(
        config: RuntimeConfig,
        input_authorities: Vec<InputAuthority>,
    ) -> RuntimeResult<Self> {
        config.executor.validate().map_err(map_universal_error)?;
        if let Some(windows) = &config.windows {
            windows.validate()?;
        }
        if config.startup_grace_ms == 0 {
            return Err(RuntimeError::invalid(
                "startupGraceMs must be positive",
                "startupGraceMs",
            ));
        }
        if Instant::now()
            .checked_add(Duration::from_millis(config.startup_grace_ms))
            .is_none()
        {
            return Err(RuntimeError::invalid(
                "startupGraceMs exceeds platform monotonic clock range",
                "startupGraceMs",
            ));
        }
        let mut configured_input_authorities = BTreeMap::new();
        for authority in input_authorities {
            validate_input_authority_name(&authority.name, "inputAuthorities.name")?;
            if !authority.root.is_absolute() {
                return Err(RuntimeError::invalid(
                    "input authority root must be absolute",
                    "inputAuthorities.root",
                ));
            }
            let root = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
                .open(&authority.root)
                .map_err(|error| io_error("open input authority root", error))?;
            let metadata = root
                .metadata()
                .map_err(|error| io_error("inspect input authority root", error))?;
            if !metadata.is_dir() {
                return Err(RuntimeError::invalid(
                    "input authority root must be a directory",
                    "inputAuthorities.root",
                ));
            }
            if configured_input_authorities
                .insert(
                    authority.name,
                    OpenedInputAuthority {
                        root: Arc::new(root),
                    },
                )
                .is_some()
            {
                return Err(RuntimeError::invalid(
                    "input authority names must be unique",
                    "inputAuthorities.name",
                ));
            }
        }
        let registry = Registry::initialize(config.registry)?;
        let execution_path = configured_execution_path()?;
        let execution_home = configured_execution_home()?;
        let runtime = Self {
            registry,
            executor: config.executor,
            startup_grace_ms: config.startup_grace_ms,
            execution_path,
            execution_home,
            windows: config.windows,
            input_authorities: configured_input_authorities,
            lifecycle_lock: Arc::new(Mutex::new(())),
            control_terminal_lock: Arc::new(Mutex::new(())),
        };
        runtime.reconcile_recoverable_orphans()?;
        Ok(runtime)
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn inspect_job(
        &self,
        job_id: &str,
        event_limit: u32,
    ) -> RuntimeResult<super::RuntimeJobInspection> {
        let registry = self.registry.config();
        super::inspection::inspect_job(
            &super::RuntimeInspectionConfig {
                db_path: registry.db_path.clone(),
                busy_timeout_ms: registry.busy_timeout_ms,
            },
            job_id,
            event_limit,
            false,
        )
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

    fn lock_control_terminal(&self) -> RuntimeResult<MutexGuard<'_, ()>> {
        self.control_terminal_lock.lock().map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                "Runtime control-terminal lock is poisoned",
                None,
                true,
            )
        })
    }

    pub fn run_task(&self, request: &TaskRunRequest) -> RuntimeResult<TaskObservation> {
        validate_run_request_structure(request)?;
        let request_identity_digest = super::operation_request_identity_digest(request)?;
        self.run_concrete_task(request, request_identity_digest)
    }

    /// Core execution path for exact immutable foreign inputs.
    /// Existing Jobs replay before current authority roots are consulted.
    pub fn run_task_with_inputs(
        &self,
        request: &TaskRunRequest,
        inputs: &[InputBindingRequest],
    ) -> RuntimeResult<TaskObservation> {
        validate_run_request_structure(request)?;
        let inputs = canonical_input_binding_requests(inputs)?;
        let request_identity_digest = super::input_bound_request_identity_digest(request, &inputs)?;
        let (job_id, created) = {
            let _guard = self.lock_lifecycle()?;
            if let Some(existing) = self.registry.find_idempotent_job(
                &request.principal,
                &request.client_request_id,
                &request_identity_digest,
            )? {
                (existing.job_id, false)
            } else {
                let job_id =
                    self.admit_new_task_with_inputs(request, request_identity_digest, &inputs)?;
                (job_id, true)
            }
        };
        if created {
            self.ensure_newly_admitted_job_dispatched(&job_id)?;
        }
        self.observe_admitted_task(
            &job_id,
            request.wait_ms,
            request.stdout_tail_bytes,
            request.stderr_tail_bytes,
        )
    }

    /// Admit an Agent-authored proposal with exact immutable inputs. Proposal identity plus the
    /// canonical input bindings is fixed before current operator policy or authority roots are
    /// consulted, so replay preserves the same semantics as ordinary proposal admission.
    pub fn run_task_proposal_with_inputs(
        &self,
        proposal: &super::TaskRunProposal,
        inputs: &[InputBindingRequest],
    ) -> RuntimeResult<TaskObservation> {
        validate_run_proposal_structure(proposal)?;
        let inputs = canonical_input_binding_requests(inputs)?;
        let request_identity_digest =
            super::input_bound_proposal_request_identity_digest(proposal, &inputs)?;
        let job_id = {
            let _guard = self.lock_lifecycle()?;
            if let Some(existing) = self.registry.find_idempotent_job(
                &proposal.principal,
                &proposal.client_request_id,
                &request_identity_digest,
            )? {
                existing.job_id
            } else {
                let request = self.resolve_proposal(proposal);
                validate_run_request_structure(&request)?;
                self.admit_new_task_with_inputs(&request, request_identity_digest, &inputs)?
            }
        };
        self.observe_admitted_task(
            &job_id,
            proposal.wait_ms,
            proposal.stdout_tail_bytes,
            proposal.stderr_tail_bytes,
        )
    }

    fn admit_new_task_with_inputs(
        &self,
        request: &TaskRunRequest,
        request_identity_digest: String,
        inputs: &[InputBindingRequest],
    ) -> RuntimeResult<String> {
        match request.execution.execution_target {
            super::ExecutionTarget::LocalLinux => {
                if request.execution.execution_profile != super::ExecutionProfile::ContainedLocal {
                    return Err(RuntimeError::invalid(
                        "local_linux immutable input bindings require contained_local execution",
                        "execution.executionProfile",
                    ));
                }
            }
            super::ExecutionTarget::WindowsNative => {
                if request.execution.execution_profile != super::ExecutionProfile::TrustedLocal {
                    return Err(RuntimeError::invalid(
                        "windows_native immutable input bindings require trusted_local execution",
                        "execution.executionProfile",
                    ));
                }
                if request.execution.windows_authority != super::WindowsAuthority::Limited {
                    return Err(RuntimeError::invalid(
                        "windows_native immutable input bindings support limited authority only",
                        "execution.windowsAuthority",
                    ));
                }
                validate_windows_input_relative_paths(
                    inputs
                        .iter()
                        .map(|input| input.presentation_relative_path.as_str()),
                )?;
            }
        }
        validate_new_admission_policy(
            request,
            self.executor.max_runtime_ms,
            self.executor.max_output_bytes,
        )?;
        self.reconcile_recoverable_orphans()?;
        let _ = self.reconcile_workspace(&request.execution.workspace_id)?;
        let mut plan = self.resolve_plan(request)?;
        let admission_ids = self.registry.preallocate_admission_ids();
        let prepared = self.materialize_input_bindings(
            request,
            &request_identity_digest,
            &admission_ids.job_id,
            inputs,
        )?;
        plan.input_set_id = Some(prepared.input_set_id.clone());
        plan.effective_inputs = prepared.effective_inputs.clone();
        let input_root = match plan.execution_target {
            super::ExecutionTarget::LocalLinux => CONTAINED_INPUT_ROOT.to_string(),
            super::ExecutionTarget::WindowsNative => {
                windows_input_presentation_root(&plan, &prepared.input_set_id)?
            }
        };
        set_environment_value_case_insensitive(
            &mut plan.env,
            "ORDIVON_INPUT_ROOT",
            input_root.clone(),
        );
        for step in &mut plan.steps {
            set_environment_value_case_insensitive(
                &mut step.env,
                "ORDIVON_INPUT_ROOT",
                input_root.clone(),
            );
        }
        let submit = SubmitRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: request.client_request_id.clone(),
            request_identity_digest: Some(request_identity_digest),
            execution_provider: Some(
                self.current_execution_provider_snapshot(request.execution.execution_target)?,
            ),
            runtime_release_effect: None,
            host_dependencies: Vec::new(),
            plan,
            global_limit: request.global_limit,
        };
        match self.registry.submit_preallocated(&submit, &admission_ids) {
            Ok(AdmissionOutcome::Created(created)) => {
                let job_id = created.job.job_id.clone();
                self.ensure_job_input_ownership(&job_id)
                    .map_err(|error| error.with_operation_id(job_id.clone()))?;
                Ok(job_id)
            }
            Ok(AdmissionOutcome::Existing { job }) => {
                self.discard_prepared_input_set(&prepared.prepared_root)?;
                Ok(job.job_id)
            }
            Err(error) => {
                self.discard_prepared_input_set(&prepared.prepared_root)?;
                Err(error)
            }
        }
    }

    /// Admit an Agent-authored proposal whose proven mechanical execution limits may be omitted.
    /// Proposal identity is resolved before current operator policy so replay returns historical
    /// Runtime truth instead of re-adjudicating an already committed Job.
    pub fn run_task_proposal(
        &self,
        proposal: &super::TaskRunProposal,
    ) -> RuntimeResult<TaskObservation> {
        validate_run_proposal_structure(proposal)?;
        let request_identity_digest = super::proposal_request_identity_digest(proposal)?;
        let (job_id, created) = {
            let _guard = self.lock_lifecycle()?;
            if let Some(existing) = self.registry.find_idempotent_job(
                &proposal.principal,
                &proposal.client_request_id,
                &request_identity_digest,
            )? {
                (existing.job_id, false)
            } else {
                let request = self.resolve_proposal(proposal);
                validate_run_request_structure(&request)?;
                validate_new_admission_policy(
                    &request,
                    self.executor.max_runtime_ms,
                    self.executor.max_output_bytes,
                )?;
                (
                    self.admit_new_task(&request, request_identity_digest)?,
                    true,
                )
            }
        };
        if created {
            self.ensure_newly_admitted_job_dispatched(&job_id)?;
        }
        self.observe_admitted_task(
            &job_id,
            proposal.wait_ms,
            proposal.stdout_tail_bytes,
            proposal.stderr_tail_bytes,
        )
    }

    fn run_concrete_task(
        &self,
        request: &TaskRunRequest,
        request_identity_digest: String,
    ) -> RuntimeResult<TaskObservation> {
        let (job_id, created) = {
            let _guard = self.lock_lifecycle()?;
            if let Some(existing) = self.registry.find_idempotent_job(
                &request.principal,
                &request.client_request_id,
                &request_identity_digest,
            )? {
                (existing.job_id, false)
            } else {
                validate_new_admission_policy(
                    request,
                    self.executor.max_runtime_ms,
                    self.executor.max_output_bytes,
                )?;
                (self.admit_new_task(request, request_identity_digest)?, true)
            }
        };
        if created {
            self.ensure_newly_admitted_job_dispatched(&job_id)?;
        }
        self.observe_admitted_task(
            &job_id,
            request.wait_ms,
            request.stdout_tail_bytes,
            request.stderr_tail_bytes,
        )
    }

    fn admit_new_task(
        &self,
        request: &TaskRunRequest,
        request_identity_digest: String,
    ) -> RuntimeResult<String> {
        self.reconcile_recoverable_orphans()?;
        let _ = self.reconcile_workspace(&request.execution.workspace_id)?;
        let host_dependencies = self.validate_host_dependencies(request)?;
        let plan = self.resolve_plan(request)?;
        let submit = SubmitRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: request.client_request_id.clone(),
            request_identity_digest: Some(request_identity_digest),
            execution_provider: Some(
                self.current_execution_provider_snapshot(request.execution.execution_target)?,
            ),
            runtime_release_effect: None,
            host_dependencies,
            plan,
            global_limit: request.global_limit,
        };
        match self.registry.submit(&submit)? {
            AdmissionOutcome::Created(created) => Ok(created.job.job_id.clone()),
            AdmissionOutcome::Existing { job } => Ok(job.job_id),
        }
    }

    fn observe_admitted_task(
        &self,
        job_id: &str,
        wait_ms: u64,
        stdout_tail_bytes: u64,
        stderr_tail_bytes: u64,
    ) -> RuntimeResult<TaskObservation> {
        self.observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: job_id.to_string(),
            wait_ms,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes,
            stderr_tail_bytes,
            stdout_offset: None,
            stderr_offset: None,
        })
        .map_err(|error| error.with_operation_id(job_id.to_string()))
    }

    fn resolve_proposal(&self, proposal: &super::TaskRunProposal) -> TaskRunRequest {
        let timeout_ms = proposal
            .execution
            .timeout_ms
            .unwrap_or(self.executor.max_runtime_ms);
        TaskRunRequest {
            schema_version: proposal.schema_version,
            client_request_id: proposal.client_request_id.clone(),
            principal: proposal.principal.clone(),
            global_limit: proposal.global_limit,
            execution: super::UniversalExecutionRequest {
                workspace_id: proposal.execution.workspace_id.clone(),
                executable: proposal.execution.executable.clone(),
                args: proposal.execution.args.clone(),
                cwd_relative: proposal.execution.cwd_relative.clone(),
                env: proposal.execution.env.clone(),
                timeout_ms,
                stdout_limit_bytes: proposal
                    .execution
                    .stdout_limit_bytes
                    .unwrap_or(self.executor.max_output_bytes),
                stderr_limit_bytes: proposal
                    .execution
                    .stderr_limit_bytes
                    .unwrap_or(self.executor.max_output_bytes),
                steps: proposal
                    .execution
                    .steps
                    .iter()
                    .map(|step| super::UniversalExecutionStep {
                        id: step.id.clone(),
                        executable: step.executable.clone(),
                        args: step.args.clone(),
                        cwd_relative: step.cwd_relative.clone(),
                        env: step.env.clone(),
                        timeout_ms: step.timeout_ms.unwrap_or(timeout_ms),
                        continue_on_error: step.continue_on_error,
                    })
                    .collect(),
                budget: proposal.execution.budget.clone(),
                execution_profile: proposal.execution.execution_profile,
                execution_target: proposal.execution.execution_target,
                windows_authority: proposal.execution.windows_authority,
                foreign_references: proposal.execution.foreign_references.clone(),
                host_dependencies: proposal.execution.host_dependencies.clone(),
            },
            wait_ms: proposal.wait_ms,
            stdout_tail_bytes: proposal.stdout_tail_bytes,
            stderr_tail_bytes: proposal.stderr_tail_bytes,
        }
    }

    pub fn find_runtime_release_for_apply(
        &self,
        request: &RuntimeReleaseRequest,
    ) -> RuntimeResult<Option<RuntimeReleaseAdmission>> {
        validate_runtime_release_request(request)?;
        let request_digest = runtime_release_request_identity_digest(request)?;
        let Some(job) = self.registry.find_idempotent_job(
            &request.principal,
            &request.client_request_id,
            &request_digest,
        )?
        else {
            return Ok(None);
        };
        let binding = self
            .registry
            .runtime_release_effect_for_job(&job.job_id)?
            .ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "Runtime Release request identity is missing its release side truth",
                    Some("runtimeReleaseEffect"),
                    false,
                )
            })?;
        validate_release_binding_matches_request(&binding, request)?;
        let release = self.runtime_release_projection(&job.job_id, &binding)?;
        Ok(Some(RuntimeReleaseAdmission {
            replayed: true,
            release,
        }))
    }

    pub fn admit_runtime_release_effect(
        &self,
        request: &RuntimeReleaseRequest,
        proposal: &super::TaskRunProposal,
        receipt_path: &Path,
    ) -> RuntimeResult<RuntimeReleaseAdmission> {
        validate_runtime_release_request(request)?;
        validate_run_proposal_structure(proposal)?;
        if proposal.client_request_id != request.client_request_id
            || proposal.principal != request.principal
            || proposal.execution.workspace_id != request.workspace_id
        {
            return Err(RuntimeError::invalid(
                "Runtime Release proposal identity does not match the structured release request",
                "clientRequestId",
            ));
        }
        if !receipt_path.is_absolute() {
            return Err(RuntimeError::invalid(
                "Runtime Release receipt path must be absolute",
                "receiptPath",
            ));
        }
        let request_digest = runtime_release_request_identity_digest(request)?;
        let binding = RuntimeReleaseEffectBinding {
            contract: RuntimeReleaseContract::RuntimeReleaseV1,
            effect_id: runtime_release_effect_id(request),
            request_digest: request_digest.clone(),
            workspace_id: request.workspace_id.clone(),
            commit: request.commit.clone(),
            candidate_manifest_digest: request.candidate_manifest_digest.clone(),
            expected_tool_count: request.expected_tool_count,
            receipt_path: receipt_path.to_string_lossy().into_owned(),
        };
        let _guard = self.lock_lifecycle()?;
        if let Some(job) = self.registry.find_idempotent_job(
            &request.principal,
            &request.client_request_id,
            &request_digest,
        )? {
            let committed = self
                .registry
                .runtime_release_effect_for_job(&job.job_id)?
                .ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorCode::RegistryCorrupt,
                        "Runtime Release replay lost its release side truth",
                        Some("runtimeReleaseEffect"),
                        false,
                    )
                })?;
            validate_release_binding_matches_request(&committed, request)?;
            return Ok(RuntimeReleaseAdmission {
                replayed: true,
                release: self.runtime_release_projection(&job.job_id, &committed)?,
            });
        }

        let resolved = self.resolve_proposal(proposal);
        validate_run_request_structure(&resolved)?;
        validate_new_admission_policy(
            &resolved,
            self.executor.max_runtime_ms,
            self.executor.max_output_bytes,
        )?;
        self.reconcile_recoverable_orphans()?;
        let _ = self.reconcile_workspace(&request.workspace_id)?;
        let plan = self.resolve_plan(&resolved)?;
        let submit = SubmitRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: request.client_request_id.clone(),
            request_identity_digest: Some(request_digest),
            execution_provider: Some(
                self.current_execution_provider_snapshot(resolved.execution.execution_target)?,
            ),
            runtime_release_effect: Some(binding.clone()),
            host_dependencies: Vec::new(),
            plan,
            global_limit: resolved.global_limit,
        };
        let job_id = match self.registry.submit(&submit)? {
            AdmissionOutcome::Created(created) => created.job.job_id.clone(),
            AdmissionOutcome::Existing { job } => job.job_id.clone(),
        };
        // Intentionally do not dispatch here. The durable Accepted Job is visible before any
        // self-replacing release can remove the initiating MCP connection. Normal Runtime
        // reconciliation owns the later at-most-once physical dispatch.
        Ok(RuntimeReleaseAdmission {
            replayed: false,
            release: self.runtime_release_projection(&job_id, &binding)?,
        })
    }

    pub fn get_runtime_release_effect(
        &self,
        request: &RuntimeReleaseGetRequest,
    ) -> RuntimeResult<RuntimeReleaseProjection> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(RuntimeError::invalid(
                "unsupported runtime schema version",
                "schemaVersion",
            ));
        }
        let Some((job, binding)) = self
            .registry
            .find_runtime_release_effect(&request.principal, &request.client_request_id)?
        else {
            return Err(RuntimeError::new(
                RuntimeErrorCode::JobNotFound,
                "Runtime Release effect not found",
                Some("clientRequestId"),
                false,
            ));
        };
        self.runtime_release_projection(&job.job_id, &binding)
    }

    fn runtime_release_projection(
        &self,
        job_id: &str,
        binding: &RuntimeReleaseEffectBinding,
    ) -> RuntimeResult<RuntimeReleaseProjection> {
        let snapshot = self.registry.job_snapshot(job_id)?;
        let receipt = inspect_runtime_release_receipt(binding, &snapshot)?;
        Ok(RuntimeReleaseProjection {
            contract: binding.contract,
            effect_id: binding.effect_id.clone(),
            client_request_id: snapshot.job.client_request_id,
            job_id: snapshot.job.job_id,
            workspace_id: binding.workspace_id.clone(),
            commit: binding.commit.clone(),
            candidate_manifest_digest: binding.candidate_manifest_digest.clone(),
            expected_tool_count: binding.expected_tool_count,
            effect_disposition: receipt.disposition,
            effect_terminal: receipt.terminal,
            receipt_available: receipt.available,
            receipt_digest: receipt.digest,
            deployed_tool_count: receipt.deployed_tool_count,
            tool_catalog_digest: receipt.tool_catalog_digest,
            rollback_status: receipt.rollback_status,
            reconciliation_issue: receipt.issue,
            attempt_state: snapshot.attempt.as_ref().map(|attempt| attempt.state),
            execution_terminal: snapshot.projection.execution_terminal,
            execution_disposition: snapshot.projection.execution_disposition,
            delivery_disposition: snapshot.projection.delivery_disposition,
            recovery_required: snapshot.projection.recovery_required,
            semantic_completion_evaluated: false,
        })
    }

    pub fn open_workspace(
        &self,
        request: &GitWorkspaceCreateRequest,
    ) -> RuntimeResult<CompactWorkspaceOpenResult> {
        let _guard = self.lock_lifecycle()?;
        create_git_workspace_compact(&self.executor, request).map_err(map_universal_error)
    }

    pub fn get_workspace(
        &self,
        request: &RuntimeWorkspaceGetRequest,
    ) -> RuntimeResult<RuntimeWorkspaceSummary> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(RuntimeError::invalid(
                "unsupported runtime schema version",
                "schemaVersion",
            ));
        }
        let record = load_workspace_record(&self.executor, &request.workspace_id)
            .map_err(map_universal_error)?;
        self.workspace_summary(&record)
    }

    pub fn list_workspaces(
        &self,
        request: &RuntimeWorkspaceListRequest,
    ) -> RuntimeResult<RuntimeWorkspaceListResult> {
        if request.schema_version != RUNTIME_SCHEMA_VERSION {
            return Err(RuntimeError::invalid(
                "unsupported runtime schema version",
                "schemaVersion",
            ));
        }
        if request.limit == 0 || request.limit > super::MAX_RUNTIME_LIST_LIMIT {
            return Err(RuntimeError::invalid(
                format!("limit must be in 1..={}", super::MAX_RUNTIME_LIST_LIMIT),
                "limit",
            ));
        }
        if let Some(cursor) = &request.cursor {
            crate::universal::validate_id(&cursor.workspace_id, "cursor.workspaceId")
                .map_err(map_universal_error)?;
        }
        let inventory =
            list_open_workspace_record_inventory(&self.executor).map_err(map_universal_error)?;
        let (records, next_cursor) =
            workspace_record_page(inventory.records, request.limit, request.cursor.as_ref());
        let mut workspaces = Vec::with_capacity(records.len());
        let mut issues = inventory
            .issues
            .into_iter()
            .map(|issue| {
                workspace_issue(
                    &issue.workspace_id,
                    RuntimeWorkspaceIssueStage::Inventory,
                    map_universal_error(issue.error),
                )
            })
            .collect::<Vec<_>>();
        for record in records {
            let active_job_ids = match self
                .registry
                .active_job_ids_for_workspace(&record.workspace_id)
            {
                Ok(active_job_ids) => active_job_ids,
                Err(error) if error.is_reconciliation_fatal() => return Err(error),
                Err(error) => {
                    issues.push(workspace_issue(
                        &record.workspace_id,
                        RuntimeWorkspaceIssueStage::ActiveJobs,
                        error,
                    ));
                    continue;
                }
            };
            let (current_head_revision, dirty) =
                match workspace_head_and_dirty_at(Path::new(&record.workspace_path)) {
                    Ok(projection) => projection,
                    Err(error) => {
                        let stage = if error.code
                            == crate::universal::UniversalExecErrorCode::RevisionNotFound
                        {
                            RuntimeWorkspaceIssueStage::HeadRevision
                        } else {
                            RuntimeWorkspaceIssueStage::DirtyProbe
                        };
                        issues.push(workspace_issue(
                            &record.workspace_id,
                            stage,
                            map_universal_error(error),
                        ));
                        continue;
                    }
                };
            let source_state_digest = if request.include_source_state_digest {
                match workspace_source_state_digest(&self.executor, &record.workspace_id) {
                    Ok(digest) => Some(digest),
                    Err(error) => {
                        issues.push(workspace_issue(
                            &record.workspace_id,
                            RuntimeWorkspaceIssueStage::SourceStateDigest,
                            map_universal_error(error),
                        ));
                        continue;
                    }
                }
            } else {
                None
            };
            workspaces.push(Self::workspace_summary_from_parts(
                &record,
                current_head_revision,
                dirty,
                source_state_digest,
                active_job_ids,
            ));
        }
        Ok(RuntimeWorkspaceListResult {
            workspaces,
            next_cursor,
            issues,
        })
    }

    fn workspace_summary(
        &self,
        record: &crate::universal::WorkspaceRecord,
    ) -> RuntimeResult<RuntimeWorkspaceSummary> {
        let active_job_ids = self
            .registry
            .active_job_ids_for_workspace(&record.workspace_id)?;
        let diff = crate::universal::workspace_diff(
            &self.executor,
            &WorkspaceDiffRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: record.workspace_id.clone(),
                max_bytes: 1,
            },
        )
        .map_err(map_universal_error)?;
        Ok(Self::workspace_summary_from_parts(
            record,
            workspace_head_revision(&self.executor, &record.workspace_id)
                .map_err(map_universal_error)?,
            diff.byte_length > 0 || !diff.untracked_paths.is_empty(),
            Some(
                workspace_source_state_digest(&self.executor, &record.workspace_id)
                    .map_err(map_universal_error)?,
            ),
            active_job_ids,
        ))
    }

    fn workspace_summary_from_parts(
        record: &crate::universal::WorkspaceRecord,
        current_head_revision: String,
        dirty: bool,
        source_state_digest: Option<String>,
        active_job_ids: Vec<String>,
    ) -> RuntimeWorkspaceSummary {
        RuntimeWorkspaceSummary {
            workspace_id: record.workspace_id.clone(),
            source_repo: record.source_repo.clone(),
            source_revision: record.source_revision.clone(),
            current_head_revision,
            created_at_ms: u64::try_from(record.created_unix_ms).unwrap_or(u64::MAX),
            head_mode: "detached".to_string(),
            dirty,
            source_state_digest,
            active_job_ids,
        }
    }

    pub fn mutate_workspace(
        &self,
        request: &WorkspaceMutateRequest,
    ) -> RuntimeResult<WorkspaceMutateResult> {
        let _guard = self.lock_lifecycle()?;
        let active = self
            .registry
            .active_job_ids_for_workspace(&request.workspace_id)?;
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

    pub fn patch_workspace(
        &self,
        request: &WorkspacePatchRequest,
    ) -> RuntimeResult<WorkspacePatchResult> {
        let _guard = self.lock_lifecycle()?;
        let active = self
            .registry
            .active_job_ids_for_workspace(&request.workspace_id)?;
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
        patch_workspace(&self.executor, request).map_err(map_universal_error)
    }

    pub fn patch_workspace_durable(
        &self,
        request: &DurableWorkspacePatchRequest,
    ) -> RuntimeResult<DurableWorkspacePatchResult> {
        validate_durable_patch_request(request)?;
        let request_digest = durable_patch_request_digest(request)?;
        let _guard = self.lock_lifecycle()?;
        let active = self
            .registry
            .active_job_ids_for_workspace(&request.patch.workspace_id)?;
        if !active.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceBusy,
                format!(
                    "workspace source state is committed by active or held Jobs: {}",
                    active.join(", ")
                ),
                Some("patch.workspaceId"),
                true,
            ));
        }

        let (operation, replayed) = if let Some(existing) = self
            .registry
            .find_workspace_patch_operation(&request.principal, &request.client_request_id)?
        {
            if existing.request_digest != request_digest {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::IdempotencyConflict,
                    "clientRequestId is already bound to a different Workspace Patch request",
                    Some("clientRequestId"),
                    false,
                ));
            }
            (existing, true)
        } else {
            let plan = plan_workspace_patch(&self.executor, &request.patch)
                .map_err(map_universal_error)?;
            self.registry
                .prepare_workspace_patch_operation(request, &request_digest, &plan)?
        };

        if operation.state == WorkspacePatchOperationState::Unknown {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReconciliationRequired,
                "Workspace Patch files no longer match a wholly uncommitted or committed state",
                Some("clientRequestId"),
                false,
            ));
        }
        if operation.state == WorkspacePatchOperationState::Committed {
            let patch = operation.result.ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "committed Workspace Patch omitted its result",
                    Some("result"),
                    false,
                )
            })?;
            return Ok(DurableWorkspacePatchResult {
                operation_id: operation.operation_id,
                client_request_id: operation.client_request_id,
                request_digest: operation.request_digest,
                replayed: true,
                patch,
            });
        }

        let patch = match inspect_workspace_patch_plan(&self.executor, &operation.plan)
            .map_err(map_universal_error)?
        {
            WorkspacePatchPlanState::Before => {
                match patch_workspace(&self.executor, &request.patch) {
                    Ok(result) => result,
                    Err(error)
                        if error.code
                            == crate::UniversalExecErrorCode::WorkspaceMutationIncomplete =>
                    {
                        self.registry
                            .mark_workspace_patch_unknown(&operation.operation_id)?;
                        return Err(RuntimeError::new(
                            RuntimeErrorCode::ReconciliationRequired,
                            error.message,
                            error.field.as_deref(),
                            false,
                        ));
                    }
                    Err(error) => return Err(map_universal_error(error)),
                }
            }
            WorkspacePatchPlanState::After => result_from_workspace_patch_plan(
                &self.executor,
                &operation.plan,
                operation.max_diff_bytes,
            )
            .map_err(map_universal_error)?,
            WorkspacePatchPlanState::Mixed => {
                self.registry
                    .mark_workspace_patch_unknown(&operation.operation_id)?;
                return Err(RuntimeError::new(
                    RuntimeErrorCode::ReconciliationRequired,
                    "Workspace Patch has a mixed or externally changed file state",
                    Some("clientRequestId"),
                    false,
                ));
            }
        };
        self.registry
            .commit_workspace_patch_operation(&operation.operation_id, &patch)?;
        Ok(DurableWorkspacePatchResult {
            operation_id: operation.operation_id,
            client_request_id: operation.client_request_id,
            request_digest: operation.request_digest,
            replayed,
            patch,
        })
    }

    pub fn workspace_patch_status(
        &self,
        request: &WorkspacePatchStatusRequest,
    ) -> RuntimeResult<WorkspacePatchOperationStatus> {
        validate_patch_status_request(request)?;
        let _guard = self.lock_lifecycle()?;
        let mut operation = self
            .registry
            .find_workspace_patch_operation(&request.principal, &request.client_request_id)?
            .ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::JobNotFound,
                    "Workspace Patch operation not found",
                    Some("clientRequestId"),
                    false,
                )
            })?;
        if operation.state == WorkspacePatchOperationState::Prepared {
            match inspect_workspace_patch_plan(&self.executor, &operation.plan)
                .map_err(map_universal_error)?
            {
                WorkspacePatchPlanState::Before => {}
                WorkspacePatchPlanState::After => {
                    let result = result_from_workspace_patch_plan(
                        &self.executor,
                        &operation.plan,
                        operation.max_diff_bytes,
                    )
                    .map_err(map_universal_error)?;
                    self.registry
                        .commit_workspace_patch_operation(&operation.operation_id, &result)?;
                    operation.state = WorkspacePatchOperationState::Committed;
                    operation.result = Some(result);
                }
                WorkspacePatchPlanState::Mixed => {
                    self.registry
                        .mark_workspace_patch_unknown(&operation.operation_id)?;
                    operation.state = WorkspacePatchOperationState::Unknown;
                }
            }
        }
        Ok(WorkspacePatchOperationStatus {
            operation_id: operation.operation_id,
            client_request_id: operation.client_request_id,
            request_digest: operation.request_digest,
            workspace_id: operation.workspace_id,
            state: operation.state,
            patch: operation.result,
        })
    }

    pub fn close_workspace(
        &self,
        request: &WorkspaceCloseRequest,
    ) -> RuntimeResult<WorkspaceCloseResult> {
        let _guard = self.lock_lifecycle()?;
        let active = self
            .registry
            .active_job_ids_for_workspace(&request.workspace_id)?;
        if !active.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceBusy,
                format!("workspace has active or held Jobs: {}", active.join(", ")),
                Some("workspaceId"),
                true,
            ));
        }
        let dependents = workspace_cleanup_dependents(&self.executor, &request.workspace_id)
            .map_err(map_universal_error)?;
        if !dependents.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceBusy,
                format!(
                    "workspace owns paths required as Git authority by open Workspaces: {}",
                    dependents.join(", ")
                ),
                Some("workspaceId"),
                true,
            ));
        }
        remove_git_workspace(&self.executor, request).map_err(map_universal_error)
    }

    fn current_execution_provider_snapshot(
        &self,
        target: super::ExecutionTarget,
    ) -> RuntimeResult<ExecutionProviderSnapshot> {
        match target {
            super::ExecutionTarget::LocalLinux => {
                let runner = validate_runner(&self.executor.runner_path)?;
                Ok(ExecutionProviderSnapshot {
                    contract: ExecutionProviderContract::LocalLinuxRunnerV1,
                    executable_digest: sha256_file(&runner).map_err(map_universal_error)?,
                    wsl_distribution: None,
                })
            }
            super::ExecutionTarget::WindowsNative => {
                let windows = self.windows.as_ref().ok_or_else(|| {
                    RuntimeError::invalid(
                        "windows_native target is not configured on this Runtime",
                        "execution.executionTarget",
                    )
                })?;
                windows.validate()?;
                let launcher = fs::canonicalize(&windows.launcher_path).map_err(|error| {
                    io_error("canonicalize Windows execution provider launcher", error)
                })?;
                Ok(ExecutionProviderSnapshot {
                    contract: ExecutionProviderContract::WindowsNativeLauncherV1,
                    executable_digest: sha256_file(&launcher).map_err(map_universal_error)?,
                    wsl_distribution: windows.wsl_distribution.clone(),
                })
            }
        }
    }

    pub fn capabilities(&self) -> RuntimeCapabilities {
        let mut allowed_executable_roots = self
            .executor
            .allowed_executable_roots
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        allowed_executable_roots.sort();
        allowed_executable_roots.dedup();
        let input_authorities = self.input_authorities.keys().cloned().collect::<Vec<_>>();

        let linux_provider = self
            .current_execution_provider_snapshot(super::ExecutionTarget::LocalLinux)
            .ok();
        let linux = RuntimeExecutionTargetCapability {
            target: super::ExecutionTarget::LocalLinux,
            configured: true,
            available: linux_provider.is_some(),
            execution_profiles: vec![
                super::ExecutionProfile::TrustedLocal,
                super::ExecutionProfile::ContainedLocal,
            ],
            windows_authorities: Vec::new(),
            windows_immutable_input_authorities: Vec::new(),
            structured_plan: true,
            immutable_inputs: true,
            host_dependency_commitments: true,
            host_dependency_continuity_scope: Some(HOST_DEPENDENCY_CONTINUITY_SCOPE.to_string()),
            availability_issue: linux_provider
                .is_none()
                .then(|| "EXECUTION_PROVIDER_UNAVAILABLE".to_string()),
            execution_provider: linux_provider,
        };

        let windows_configured = self.windows.is_some();
        let (windows_provider, windows_authorities, windows_issue) = if let Some(windows) =
            &self.windows
        {
            match self.current_execution_provider_snapshot(super::ExecutionTarget::WindowsNative) {
                Ok(provider) => {
                    let mut authorities = Vec::new();
                    for authority in [
                        super::WindowsAuthority::Limited,
                        super::WindowsAuthority::Elevated,
                    ] {
                        if snapshot_windows_runtime_context(windows, authority).is_ok() {
                            authorities.push(authority);
                        }
                    }
                    let issue = authorities
                        .is_empty()
                        .then(|| "WINDOWS_AUTHORITY_UNAVAILABLE".to_string());
                    (Some(provider), authorities, issue)
                }
                Err(_) => (
                    None,
                    Vec::new(),
                    Some("EXECUTION_PROVIDER_UNAVAILABLE".to_string()),
                ),
            }
        } else {
            (None, Vec::new(), None)
        };
        let windows_immutable_input_authorities = windows_authorities
            .contains(&super::WindowsAuthority::Limited)
            .then_some(vec![super::WindowsAuthority::Limited])
            .unwrap_or_default();
        let windows = RuntimeExecutionTargetCapability {
            target: super::ExecutionTarget::WindowsNative,
            configured: windows_configured,
            available: windows_provider.is_some() && !windows_authorities.is_empty(),
            execution_profiles: vec![super::ExecutionProfile::TrustedLocal],
            windows_authorities,
            windows_immutable_input_authorities: windows_immutable_input_authorities.clone(),
            structured_plan: false,
            immutable_inputs: !windows_immutable_input_authorities.is_empty(),
            host_dependency_commitments: false,
            host_dependency_continuity_scope: None,
            execution_provider: windows_provider,
            availability_issue: windows_issue,
        };

        RuntimeCapabilities {
            schema_version: RUNTIME_SCHEMA_VERSION,
            max_runtime_ms: self.executor.max_runtime_ms,
            max_output_bytes: self.executor.max_output_bytes,
            allowed_executable_roots,
            input_authorities,
            targets: vec![linux, windows],
        }
    }

    fn verify_committed_execution_provider(&self, job_id: &str) -> RuntimeResult<()> {
        let plan = self.registry.execution_plan(job_id)?;
        let Some(expected) = self.registry.execution_provider(job_id)? else {
            // Historical Jobs predate provider commitment and retain their original semantics.
            return Ok(());
        };
        let observed = self.current_execution_provider_snapshot(plan.execution_target)?;
        if observed != expected {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ProviderStateMismatch,
                format!(
                    "execution provider changed after operation admission: expected {:?}, observed {:?}",
                    expected, observed
                ),
                Some("executionProvider"),
                false,
            ));
        }
        Ok(())
    }

    fn validate_host_dependencies(
        &self,
        request: &TaskRunRequest,
    ) -> RuntimeResult<Vec<HostDependencyBinding>> {
        if request.execution.host_dependencies.is_empty() {
            return Ok(Vec::new());
        }
        if request.execution.execution_target != super::ExecutionTarget::LocalLinux
            || request.execution.execution_profile != super::ExecutionProfile::TrustedLocal
        {
            return Err(RuntimeError::invalid(
                "Host Dependencies require trusted_local local_linux execution",
                "execution.hostDependencies",
            ));
        }
        let mut bindings = request.execution.host_dependencies.clone();
        bindings.sort_by(|left, right| left.path.cmp(&right.path));
        let mut previous: Option<&str> = None;
        for (index, binding) in bindings.iter().enumerate() {
            let path = Path::new(&binding.path);
            if !path.is_absolute() || binding.path.as_bytes().contains(&0) {
                return Err(RuntimeError::invalid(
                    "Host Dependency path must be absolute and NUL-free",
                    &format!("execution.hostDependencies[{index}].path"),
                ));
            }
            if previous == Some(binding.path.as_str()) {
                return Err(RuntimeError::invalid(
                    "Host Dependency paths must be unique",
                    "execution.hostDependencies",
                ));
            }
            previous = Some(&binding.path);
            let expected = binding
                .expected_digest
                .strip_prefix("sha256:")
                .ok_or_else(|| {
                    RuntimeError::invalid(
                        "Host Dependency expectedDigest must use sha256",
                        &format!("execution.hostDependencies[{index}].expectedDigest"),
                    )
                })?;
            if expected.len() != 64
                || !expected
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(RuntimeError::invalid(
                    "Host Dependency expectedDigest must contain 64 lowercase hexadecimal characters",
                    &format!("execution.hostDependencies[{index}].expectedDigest"),
                ));
            }
            let metadata = fs::symlink_metadata(path).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::InvalidRequest,
                    format!("Host Dependency {} is unavailable: {error}", binding.path),
                    Some("execution.hostDependencies"),
                    false,
                )
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(RuntimeError::invalid(
                    "Host Dependency must be a regular non-symlink file",
                    &format!("execution.hostDependencies[{index}].path"),
                ));
            }
            let observed = sha256_file(path).map_err(map_universal_error)?;
            if observed != binding.expected_digest {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::InvalidRequest,
                    format!(
                        "Host Dependency {} does not match expected digest: expected {}, observed {}",
                        binding.path, binding.expected_digest, observed
                    ),
                    Some("execution.hostDependencies"),
                    false,
                ));
            }
        }
        Ok(bindings)
    }

    fn verify_committed_host_dependencies(&self, job_id: &str) -> RuntimeResult<()> {
        for binding in self.registry.host_dependencies(job_id)?.iter() {
            let path = Path::new(&binding.path);
            let metadata = fs::symlink_metadata(path).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::WorkspaceStateMismatch,
                    format!(
                        "committed Host Dependency {} is unavailable before dispatch: {error}",
                        binding.path
                    ),
                    Some("hostDependencies"),
                    false,
                )
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::WorkspaceStateMismatch,
                    format!(
                        "committed Host Dependency {} is not a regular non-symlink file",
                        binding.path
                    ),
                    Some("hostDependencies"),
                    false,
                ));
            }
            let observed = sha256_file(path).map_err(map_universal_error)?;
            if observed != binding.expected_digest {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::WorkspaceStateMismatch,
                    format!(
                        "committed Host Dependency {} changed after admission: expected {}, observed {}",
                        binding.path, binding.expected_digest, observed
                    ),
                    Some("hostDependencies"),
                    false,
                ));
            }
        }
        Ok(())
    }

    fn resolve_plan(&self, request: &TaskRunRequest) -> RuntimeResult<RuntimeExecutionPlan> {
        let record = load_workspace_record(&self.executor, &request.execution.workspace_id)
            .map_err(map_universal_error)?;
        let workspace_path =
            canonical_directory(Path::new(&record.workspace_path), "workspacePath")
                .map_err(map_universal_error)?;
        let cwd = resolve_workspace_cwd(
            &record,
            &request.execution.cwd_relative,
            "execution.cwdRelative",
        )
        .map_err(map_universal_error)?;
        let executable = validate_executable(
            &self.executor,
            &request.execution.executable,
            "execution.executable",
        )?;
        let (base_environment, windows_execution_context) = match request.execution.execution_target
        {
            super::ExecutionTarget::LocalLinux => (
                self.execution_environment(&record, request.execution.execution_profile)?,
                None,
            ),
            super::ExecutionTarget::WindowsNative => {
                let windows = self.windows.as_ref().ok_or_else(|| {
                    RuntimeError::invalid(
                        "windows_native target is not configured on this Runtime",
                        "execution.executionTarget",
                    )
                })?;
                if mounted_windows_path(&executable).is_none() {
                    return Err(RuntimeError::invalid(
                        "windows_native executable must reside on a WSL-mounted Windows drive",
                        "execution.executable",
                    ));
                }
                let snapshot =
                    snapshot_windows_runtime_context(windows, request.execution.windows_authority)?;
                let token_class = match request.execution.windows_authority {
                    super::WindowsAuthority::Limited => super::WindowsTokenClass::Limited,
                    super::WindowsAuthority::Elevated => super::WindowsTokenClass::Elevated,
                };
                (
                    snapshot.environment,
                    Some(super::WindowsExecutionContext {
                        token_class,
                        token_user_sid: snapshot.token_user_sid,
                        environment_source: "windows_user_machine_profile_allowlist_v1".to_string(),
                    }),
                )
            }
        };
        let mut steps = Vec::with_capacity(request.execution.steps.len());
        for (step_index, step) in request.execution.steps.iter().enumerate() {
            let step_cwd = resolve_workspace_cwd(
                &record,
                &step.cwd_relative,
                &format!("execution.steps[{step_index}].cwdRelative"),
            )
            .map_err(map_universal_error)?;
            let step_executable = validate_executable(
                &self.executor,
                &step.executable,
                &format!("execution.steps[{step_index}].executable"),
            )?;
            steps.push(RuntimeExecutionStep {
                id: step.id.clone(),
                executable: step_executable.to_string_lossy().into_owned(),
                executable_digest: sha256_file(&step_executable).map_err(map_universal_error)?,
                args: step.args.clone(),
                cwd: step_cwd.to_string_lossy().into_owned(),
                env: match request.execution.execution_target {
                    super::ExecutionTarget::LocalLinux => {
                        merge_environment(&base_environment, &step.env)
                    }
                    super::ExecutionTarget::WindowsNative => {
                        merge_windows_environment(&base_environment, &step.env)?
                    }
                },
                timeout_ms: step.timeout_ms,
                continue_on_error: step.continue_on_error,
            });
        }
        Ok(RuntimeExecutionPlan {
            schema_version: RUNTIME_SCHEMA_VERSION,
            workspace_id: request.execution.workspace_id.clone(),
            workspace_path: workspace_path.to_string_lossy().into_owned(),
            source_revision: record.source_revision,
            workspace_source_digest: Some(
                workspace_source_state_digest(&self.executor, &request.execution.workspace_id)
                    .map_err(map_universal_error)?,
            ),
            workspace_git_common_dir: Some(
                workspace_git_common_dir_at(&workspace_path)
                    .map_err(map_universal_error)?
                    .to_string_lossy()
                    .into_owned(),
            ),
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: sha256_file(&executable).map_err(map_universal_error)?,
            args: request.execution.args.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            env: match request.execution.execution_target {
                super::ExecutionTarget::LocalLinux => {
                    merge_environment(&base_environment, &request.execution.env)
                }
                super::ExecutionTarget::WindowsNative => {
                    merge_windows_environment(&base_environment, &request.execution.env)?
                }
            },
            timeout_ms: request.execution.timeout_ms,
            stdout_limit_bytes: request.execution.stdout_limit_bytes,
            stderr_limit_bytes: request.execution.stderr_limit_bytes,
            steps,
            budget: request.execution.budget.clone(),
            execution_profile: request.execution.execution_profile,
            execution_target: request.execution.execution_target,
            windows_authority: request.execution.windows_authority,
            windows_execution_context,
            foreign_references: request.execution.foreign_references.clone(),
            input_set_id: None,
            effective_inputs: Vec::new(),
            principal: request.principal.clone(),
        })
    }

    fn materialize_input_bindings(
        &self,
        request: &TaskRunRequest,
        request_identity_digest: &str,
        job_id: &str,
        inputs: &[InputBindingRequest],
    ) -> RuntimeResult<PreparedInputSet> {
        let set_digest = sha256_bytes(
            format!(
                "{}\0{}\0{}",
                request.principal, request.client_request_id, request_identity_digest
            )
            .as_bytes(),
        );
        let input_set_id = set_digest
            .strip_prefix("sha256:")
            .unwrap_or(&set_digest)
            .to_string();
        let materialization_root = self.executor.input_materializations_root();
        fs::create_dir_all(&materialization_root)
            .map_err(|error| io_error("create input materialization root", error))?;
        let prepared_root = materialization_root.join(job_id);
        let owned_root = self.executor.job_input_path(job_id);
        if prepared_root.exists() || owned_root.exists() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "preallocated immutable input identity already has physical state",
                Some("jobId"),
                false,
            ));
        }

        let lease_path = materialization_root.join(format!(".{job_id}.lease"));
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&lease_path)
            .map_err(|error| io_error("create input staging lease", error))?;
        if unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            let _ = fs::remove_file(&lease_path);
            return Err(io_error("lock input staging lease", error));
        }
        let staging = materialization_root.join(format!(".{job_id}.staging-{}", Uuid::now_v7()));
        let result = (|| {
            fs::create_dir(&staging)
                .map_err(|error| io_error("create input staging directory", error))?;
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
                .map_err(|error| io_error("protect input staging directory", error))?;
            for (index, input) in inputs.iter().enumerate() {
                let authority = self
                    .input_authorities
                    .get(&input.authority)
                    .ok_or_else(|| {
                        RuntimeError::invalid(
                            format!("unknown input authority {}", input.authority),
                            &format!("inputs[{index}].authority"),
                        )
                    })?;
                let source =
                    open_authority_file(authority.root.as_ref(), &input.relative_object, index)?;
                let target = staging.join(&input.presentation_relative_path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| io_error("create input presentation tree", error))?;
                }
                let observed_digest = copy_input_and_digest(source, &target)?;
                if observed_digest != input.expected_digest {
                    return Err(RuntimeError::invalid(
                        format!(
                            "materialized input digest mismatch: expected {}, observed {observed_digest}",
                            input.expected_digest
                        ),
                        &format!("inputs[{index}].expectedDigest"),
                    ));
                }
                fs::set_permissions(&target, fs::Permissions::from_mode(0o444))
                    .map_err(|error| io_error("protect materialized input", error))?;
            }
            sync_directory(&staging)?;
            fs::rename(&staging, &prepared_root)
                .map_err(|error| io_error("publish prepared immutable input set", error))?;
            sync_directory(&materialization_root)?;
            let effective_inputs = verify_effective_input_set(&prepared_root, inputs)?;
            Ok(PreparedInputSet {
                input_set_id,
                prepared_root: prepared_root.clone(),
                effective_inputs,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir_all(&prepared_root);
        }
        drop(lease);
        let _ = fs::remove_file(&lease_path);
        let _ = sync_directory(&materialization_root);
        result
    }

    fn discard_prepared_input_set(&self, prepared_root: &Path) -> RuntimeResult<()> {
        if !prepared_root.exists() {
            return Ok(());
        }
        fs::remove_dir_all(prepared_root)
            .map_err(|error| io_error("remove unowned prepared input set", error))?;
        if let Some(parent) = prepared_root.parent() {
            sync_directory(parent)?;
        }
        Ok(())
    }

    fn ensure_job_input_ownership(&self, job_id: &str) -> RuntimeResult<()> {
        let plan = self.registry.execution_plan(job_id)?;
        if plan.effective_inputs.is_empty() {
            if plan.input_set_id.is_some() {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "inputSetId exists without effective immutable inputs",
                    Some("executionPlan"),
                    false,
                ));
            }
            return Ok(());
        }
        if plan.input_set_id.is_none() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "effective immutable inputs have no committed inputSetId",
                Some("executionPlan"),
                false,
            ));
        }
        let requests = effective_input_requests_from_plan(&plan)?;
        let owned_root = self.executor.job_input_path(job_id);
        let prepared_root = self.executor.input_materializations_root().join(job_id);
        fs::create_dir_all(self.executor.job_inputs_root())
            .map_err(|error| io_error("create Job input ownership root", error))?;
        if owned_root.exists() {
            verify_effective_input_set(&owned_root, &requests)?;
            if prepared_root.exists() {
                self.discard_prepared_input_set(&prepared_root)?;
            }
            return Ok(());
        }
        if !prepared_root.exists() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReconciliationRequired,
                "committed Job immutable inputs are missing both prepared and Job-owned bytes",
                Some("executionPlan.inputSetId"),
                true,
            ));
        }
        verify_effective_input_set(&prepared_root, &requests)?;
        match fs::rename(&prepared_root, &owned_root) {
            Ok(()) => sync_directory(&self.executor.job_inputs_root())?,
            Err(error) if owned_root.exists() => {
                let _ = error;
            }
            Err(error) => return Err(io_error("adopt prepared immutable inputs for Job", error)),
        }
        verify_effective_input_set(&owned_root, &requests)?;
        Ok(())
    }

    fn ensure_newly_admitted_job_dispatched(&self, job_id: &str) -> RuntimeResult<()> {
        let attempt = self.registry.get_latest_attempt(job_id)?.ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "newly admitted Job has no Attempt",
                Some("jobId"),
                false,
            )
        })?;
        if attempt.state == AttemptState::Accepted {
            self.ensure_attempt_dispatched(&attempt)
                .map_err(|error| error.with_operation_id(job_id.to_string()))?;
        }
        Ok(())
    }

    fn ensure_attempt_dispatched(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        if let Err(error) = self.verify_committed_execution_provider(&attempt.job_id) {
            if error.code == RuntimeErrorCode::ProviderStateMismatch {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Failed,
                    "EXECUTION_PROVIDER_PRECONDITION_DRIFT",
                    Some(error.to_string()),
                )?;
                return Ok(());
            }
            return Err(error);
        }
        if let Err(error) = self.verify_committed_host_dependencies(&attempt.job_id) {
            if matches!(
                error.code,
                RuntimeErrorCode::WorkspaceStateMismatch | RuntimeErrorCode::InvalidRequest
            ) {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Failed,
                    "HOST_DEPENDENCY_PRECONDITION_DRIFT",
                    Some(error.to_string()),
                )?;
                return Ok(());
            }
            return Err(error);
        }
        if let Err(error) = self.ensure_job_input_ownership(&attempt.job_id) {
            if matches!(
                error.code,
                RuntimeErrorCode::ReconciliationRequired | RuntimeErrorCode::WorkspaceStateMismatch
            ) {
                self.commit_control_terminal(
                    attempt,
                    AttemptState::Failed,
                    "INPUT_PRECONDITION_DRIFT",
                    Some(error.to_string()),
                )?;
                return Ok(());
            }
            return Err(error);
        }
        let mut attempt = attempt.clone();
        if attempt.bundle_digest.is_none() {
            attempt = match self.materialize_bundle(&attempt) {
                Ok(current) => current,
                Err(error) if error.code == RuntimeErrorCode::AttemptStateConflict => {
                    let current = self.registry.get_attempt(&attempt.attempt_id)?;
                    if current.bundle_digest.is_none() && current.state == AttemptState::Accepted {
                        self.materialize_bundle(&current)?
                    } else {
                        current
                    }
                }
                Err(error) => return Err(error),
            };
        }
        match attempt.state {
            AttemptState::Accepted => match self.dispatch_attempt(&attempt) {
                Ok(()) => Ok(()),
                Err(error) if error.code == RuntimeErrorCode::AttemptStateConflict => {
                    let current = self.registry.get_attempt(&attempt.attempt_id)?;
                    match current.state {
                        AttemptState::Starting
                        | AttemptState::Running
                        | AttemptState::Stopping
                        | AttemptState::Recovering => self.reconcile_attempt(&current.attempt_id),
                        state if state.is_terminal() => Ok(()),
                        _ => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
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
        false
    }

    fn ensure_trusted_workspace_tmp_presentation(
        &self,
        workspace_id: &str,
        backing: &Path,
    ) -> RuntimeResult<PathBuf> {
        let presentation = self.executor.workspace_tmp_presentation_path(workspace_id);
        let parent = presentation.parent().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::IoError,
                "trusted temporary presentation has no parent directory",
                Some("workspaceId"),
                false,
            )
        })?;
        for _ in 0..2 {
            match fs::symlink_metadata(parent) {
                Ok(metadata) => {
                    let mode = metadata.mode() & 0o777;
                    let owner = metadata.uid();
                    let effective_uid = unsafe { libc::geteuid() };
                    if metadata.file_type().is_symlink()
                        || !metadata.is_dir()
                        || owner != effective_uid
                        || mode != 0o700
                    {
                        return Err(RuntimeError::new(
                            RuntimeErrorCode::WorkspaceStateMismatch,
                            format!(
                                "trusted temporary presentation root {} must be a non-symlink directory owned by uid {} with mode 0700; observed uid {} mode {:04o}",
                                parent.display(), effective_uid, owner, mode
                            ),
                            Some("workspaceId"),
                            false,
                        ));
                    }
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut builder = fs::DirBuilder::new();
                    builder.mode(0o700);
                    match builder.create(parent) {
                        Ok(()) => continue,
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(error) => {
                            return Err(io_error(
                                &format!("create temporary presentation root {}", parent.display()),
                                error,
                            ));
                        }
                    }
                }
                Err(error) => {
                    return Err(io_error(
                        &format!("inspect temporary presentation root {}", parent.display()),
                        error,
                    ));
                }
            }
        }

        for _ in 0..2 {
            match fs::symlink_metadata(&presentation) {
                Ok(metadata) => {
                    if !metadata.file_type().is_symlink() {
                        return Err(RuntimeError::new(
                            RuntimeErrorCode::WorkspaceStateMismatch,
                            format!(
                                "trusted temporary presentation {} is not a symlink",
                                presentation.display()
                            ),
                            Some("workspaceId"),
                            false,
                        ));
                    }
                    let target = fs::read_link(&presentation).map_err(|error| {
                        io_error(
                            &format!("read temporary presentation {}", presentation.display()),
                            error,
                        )
                    })?;
                    if target != backing {
                        return Err(RuntimeError::new(
                            RuntimeErrorCode::WorkspaceStateMismatch,
                            format!(
                                "trusted temporary presentation {} points at {}, expected {}",
                                presentation.display(),
                                target.display(),
                                backing.display()
                            ),
                            Some("workspaceId"),
                            false,
                        ));
                    }
                    return Ok(presentation);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match std::os::unix::fs::symlink(backing, &presentation) {
                        Ok(()) => return Ok(presentation),
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            continue;
                        }
                        Err(error) => {
                            return Err(io_error(
                                &format!(
                                    "create temporary presentation {}",
                                    presentation.display()
                                ),
                                error,
                            ));
                        }
                    }
                }
                Err(error) => {
                    return Err(io_error(
                        &format!("inspect temporary presentation {}", presentation.display()),
                        error,
                    ));
                }
            }
        }
        Err(RuntimeError::new(
            RuntimeErrorCode::WorkspaceStateMismatch,
            "trusted temporary presentation changed concurrently during creation",
            Some("workspaceId"),
            true,
        ))
    }

    fn execution_environment(
        &self,
        record: &crate::universal::WorkspaceRecord,
        execution_profile: super::ExecutionProfile,
    ) -> RuntimeResult<BTreeMap<String, String>> {
        let workspace_cache = self.executor.workspace_cache_path(&record.workspace_id);
        let workspace_tmp = self.executor.workspace_tmp_path(&record.workspace_id);
        let build_cache = self
            .executor
            .workspace_build_cache_path(&record.workspace_id);
        let package_cache = match execution_profile {
            super::ExecutionProfile::TrustedLocal => self.executor.shared_caches_root(),
            super::ExecutionProfile::ContainedLocal => workspace_cache.join("tooling"),
        };
        let cargo_target_backing = build_cache.join("cargo");
        for path in [
            &workspace_cache,
            &build_cache,
            &cargo_target_backing,
            &workspace_tmp,
            &package_cache,
        ] {
            fs::create_dir_all(path).map_err(|error| {
                io_error(&format!("create execution cache {}", path.display()), error)
            })?;
        }
        let mut environment = BTreeMap::new();
        environment.insert("PATH".to_string(), self.execution_path.clone());
        let execution_home = match execution_profile {
            crate::runtime::ExecutionProfile::TrustedLocal => self.execution_home.clone(),
            super::ExecutionProfile::ContainedLocal => {
                let contained_home = workspace_cache.join("home");
                fs::create_dir_all(&contained_home).map_err(|error| {
                    io_error(
                        &format!(
                            "create contained execution home {}",
                            contained_home.display()
                        ),
                        error,
                    )
                })?;
                contained_home.to_string_lossy().into_owned()
            }
        };
        environment.insert("HOME".to_string(), execution_home);
        environment.insert("LANG".to_string(), "C.UTF-8".to_string());
        environment.insert("LC_ALL".to_string(), "C.UTF-8".to_string());
        let tmp_presentation = match execution_profile {
            super::ExecutionProfile::TrustedLocal => self
                .ensure_trusted_workspace_tmp_presentation(&record.workspace_id, &workspace_tmp)?,
            super::ExecutionProfile::ContainedLocal => workspace_tmp.clone(),
        };
        environment.insert(
            "TMPDIR".to_string(),
            tmp_presentation.to_string_lossy().into_owned(),
        );
        environment.insert(
            "XDG_CACHE_HOME".to_string(),
            workspace_cache.to_string_lossy().into_owned(),
        );
        let cargo_target = match execution_profile {
            super::ExecutionProfile::TrustedLocal => {
                PathBuf::from(TRUSTED_BUILD_TARGET_PRESENTATION)
            }
            super::ExecutionProfile::ContainedLocal => cargo_target_backing,
        };
        for (name, path) in [
            ("CARGO_TARGET_DIR", cargo_target),
            ("UV_CACHE_DIR", package_cache.join("uv")),
            ("PIP_CACHE_DIR", package_cache.join("pip")),
            ("npm_config_cache", package_cache.join("npm")),
            ("PNPM_HOME", package_cache.join("pnpm")),
            ("COREPACK_HOME", package_cache.join("corepack")),
            (
                "BUN_INSTALL_CACHE_DIR",
                package_cache.join("bun/install-cache"),
            ),
            ("GOMODCACHE", package_cache.join("go/mod")),
            ("GOCACHE", package_cache.join("go/build")),
        ] {
            environment.insert(name.to_string(), path.to_string_lossy().into_owned());
        }
        for name in [
            "HOME",
            "TMPDIR",
            "XDG_CACHE_HOME",
            "CARGO_TARGET_DIR",
            "UV_CACHE_DIR",
            "PIP_CACHE_DIR",
            "npm_config_cache",
            "PNPM_HOME",
            "COREPACK_HOME",
            "BUN_INSTALL_CACHE_DIR",
            "GOMODCACHE",
            "GOCACHE",
        ] {
            if name == "CARGO_TARGET_DIR"
                && environment.get(name).map(String::as_str)
                    == Some(TRUSTED_BUILD_TARGET_PRESENTATION)
            {
                continue;
            }
            let path = Path::new(environment.get(name).expect("execution path is present"));
            fs::create_dir_all(path).map_err(|error| {
                io_error(
                    &format!("create execution environment path {}", path.display()),
                    error,
                )
            })?;
        }
        Ok(environment)
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
            build_target_backing: if plan.execution_target == super::ExecutionTarget::LocalLinux
                && plan.execution_profile == super::ExecutionProfile::TrustedLocal
                && (plan.env.get("CARGO_TARGET_DIR").map(String::as_str)
                    == Some(TRUSTED_BUILD_TARGET_PRESENTATION)
                    || plan.steps.iter().any(|step| {
                        step.env.get("CARGO_TARGET_DIR").map(String::as_str)
                            == Some(TRUSTED_BUILD_TARGET_PRESENTATION)
                    })) {
                Some(
                    self.executor
                        .workspace_build_cache_path(&plan.workspace_id)
                        .join("cargo")
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                None
            },
            input_presentation_root: if plan.execution_target == super::ExecutionTarget::LocalLinux
                && !plan.effective_inputs.is_empty()
            {
                Some(CONTAINED_INPUT_ROOT.to_string())
            } else {
                None
            },
            input_commitments: if plan.execution_target == super::ExecutionTarget::LocalLinux {
                plan.effective_inputs
                    .iter()
                    .map(|input| RunnerInputCommitment {
                        presentation_path: Path::new(CONTAINED_INPUT_ROOT)
                            .join(&input.presentation_relative_path)
                            .to_string_lossy()
                            .into_owned(),
                        digest: input.digest.clone(),
                        byte_length: input.byte_length,
                    })
                    .collect()
            } else {
                Vec::new()
            },
            host_dependencies: self
                .registry
                .host_dependencies(&attempt.job_id)?
                .into_iter()
                .map(|dependency| RunnerHostDependencyCommitment {
                    path: dependency.path,
                    digest: dependency.expected_digest,
                })
                .collect(),
            executable: plan.executable.clone(),
            executable_digest: plan.executable_digest.clone(),
            args: plan.args.clone(),
            cwd: plan.cwd.clone(),
            env: plan.env.clone(),
            steps: plan
                .steps
                .iter()
                .map(|step| RunnerExecutionStep {
                    id: step.id.clone(),
                    executable: step.executable.clone(),
                    executable_digest: step.executable_digest.clone(),
                    args: step.args.clone(),
                    cwd: step.cwd.clone(),
                    env: step.env.clone(),
                    timeout_ms: step.timeout_ms,
                    continue_on_error: step.continue_on_error,
                })
                .collect(),
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
        if final_path.exists() {
            verify_published_bundle(&final_path, &request_bytes, &plan_bytes, &manifest_bytes)?;
        } else {
            let staging = parent.join(format!(
                ".{}.staging-{}",
                attempt.attempt_id,
                Uuid::now_v7()
            ));
            fs::create_dir(&staging).map_err(|error| io_error("create staging bundle", error))?;
            let publish = (|| {
                fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
                    .map_err(|error| io_error("protect staging bundle", error))?;
                write_bytes_synced(&staging.join(RUNNER_REQUEST_FILE), &request_bytes)?;
                write_bytes_synced(&staging.join(PLAN_FILE), &plan_bytes)?;
                write_bytes_synced(&staging.join(BUNDLE_MANIFEST_FILE), &manifest_bytes)?;
                sync_directory(&staging)?;
                match fs::rename(&staging, &final_path) {
                    Ok(()) => sync_directory(parent)?,
                    Err(error) if final_path.is_dir() => {
                        let _ = error;
                        fs::remove_dir_all(&staging).map_err(|cleanup_error| {
                            io_error("remove losing bundle staging directory", cleanup_error)
                        })?;
                    }
                    Err(error) => return Err(io_error("commit Attempt bundle", error)),
                }
                verify_published_bundle(&final_path, &request_bytes, &plan_bytes, &manifest_bytes)
            })();
            if publish.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            publish?;
        }
        match self.registry.mark_bundle_ready(
            &attempt.attempt_id,
            attempt.row_version,
            &bundle_digest,
            now_ms()?,
        ) {
            Ok(current) => Ok(current),
            Err(error) if error.code == RuntimeErrorCode::AttemptStateConflict => {
                let current = self.registry.get_attempt(&attempt.attempt_id)?;
                if current.bundle_digest.as_deref() == Some(bundle_digest.as_str()) {
                    Ok(current)
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
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
        let runtime_ceiling = plan.timeout_ms.saturating_add(5_000);
        let output = match plan.execution_target {
            super::ExecutionTarget::LocalLinux => {
                let runner = validate_runner(&self.executor.runner_path)?;
                let input_set_path = if plan.input_set_id.is_some() {
                    self.ensure_job_input_ownership(&starting.job_id)?;
                    let path = self.executor.job_input_path(&starting.job_id);
                    let requests = effective_input_requests_from_plan(&plan)?;
                    verify_effective_input_set(&path, &requests)?;
                    Some(path)
                } else {
                    None
                };
                systemd_run(&SystemdRunSpec {
                    unit_name: &starting.unit_name,
                    runner: &runner,
                    bundle_path: &bundle_path,
                    workspace_path: Path::new(&plan.workspace_path),
                    workspace_git_common_dir: plan
                        .workspace_git_common_dir
                        .as_deref()
                        .map(Path::new),
                    input_set_path: input_set_path.as_deref(),
                    runtime_ceiling_ms: runtime_ceiling,
                    budget: &plan.budget,
                    execution_profile: plan.execution_profile,
                    environment: &plan.env,
                })?
            }
            super::ExecutionTarget::WindowsNative => {
                let windows = self.windows.as_ref().ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorCode::RegistryCorrupt,
                        "committed windows_native Job has no configured Windows provider",
                        Some("executionTarget"),
                        true,
                    )
                })?;
                let input_source_root = if plan.input_set_id.is_some() {
                    self.ensure_job_input_ownership(&starting.job_id)?;
                    let path = self.executor.job_input_path(&starting.job_id);
                    let requests = effective_input_requests_from_plan(&plan)?;
                    verify_effective_input_set(&path, &requests)?;
                    Some(path)
                } else {
                    None
                };
                let input_bindings_digest = if plan.effective_inputs.is_empty() {
                    None
                } else {
                    Some(windows_input_bindings_digest(&plan.effective_inputs))
                };
                let input_presentation_root = if plan.effective_inputs.is_empty() {
                    None
                } else {
                    Some(required_environment_value_case_insensitive(
                        &plan.env,
                        "ORDIVON_INPUT_ROOT",
                        "executionPlan.env",
                    )?)
                };
                if windows.wsl_distribution.is_none() {
                    let dispatch = spawn_windows_native(&WindowsNativeRunSpec {
                        config: windows,
                        bundle_path: &bundle_path,
                        job_id: &starting.job_id,
                        attempt_id: &starting.attempt_id,
                        launch_token_digest: &starting.launch_token_digest,
                        authority: plan.windows_authority,
                        executable: Path::new(&plan.executable),
                        args: &plan.args,
                        cwd: Path::new(&plan.cwd),
                        environment: &plan.env,
                        input_source_root: input_source_root.as_deref(),
                        input_set_id: plan.input_set_id.as_deref(),
                        input_presentation_root,
                        input_bindings_digest: input_bindings_digest.as_deref(),
                        budget: &plan.budget,
                        timeout_ms: plan.timeout_ms,
                        stdout_limit_bytes: plan.stdout_limit_bytes,
                        stderr_limit_bytes: plan.stderr_limit_bytes,
                    });
                    if let Err(error) = dispatch {
                        self.commit_control_terminal(
                            &starting,
                            AttemptState::Failed,
                            "RUNNER_START_FAILED",
                            Some(format!(
                                "native Windows launcher spawn failed: {}",
                                error.message
                            )),
                        )?;
                        return Ok(());
                    }
                    return self.await_launch_evidence(&starting);
                }
                windows_systemd_run(&WindowsSystemdRunSpec {
                    config: windows,
                    unit_name: &starting.unit_name,
                    bundle_path: &bundle_path,
                    job_id: &starting.job_id,
                    attempt_id: &starting.attempt_id,
                    launch_token_digest: &starting.launch_token_digest,
                    authority: plan.windows_authority,
                    executable: Path::new(&plan.executable),
                    args: &plan.args,
                    cwd: Path::new(&plan.cwd),
                    environment: &plan.env,
                    input_source_root: input_source_root.as_deref(),
                    input_set_id: plan.input_set_id.as_deref(),
                    input_presentation_root,
                    input_bindings_digest: input_bindings_digest.as_deref(),
                    budget: &plan.budget,
                    runtime_ceiling_ms: runtime_ceiling,
                    timeout_ms: plan.timeout_ms,
                    stdout_limit_bytes: plan.stdout_limit_bytes,
                    stderr_limit_bytes: plan.stderr_limit_bytes,
                })?
            }
        };
        if !output.status.success() {
            let detail = format!(
                "{} launch failed: {}",
                plan.execution_target.as_str(),
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
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(self.startup_grace_ms))
            .ok_or_else(|| {
                RuntimeError::invalid(
                    "startupGraceMs exceeds platform monotonic clock range",
                    "startupGraceMs",
                )
            })?;
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        let start_path = Path::new(&attempt.bundle_path).join(match plan.execution_target {
            super::ExecutionTarget::LocalLinux => RUNNER_START_FILE,
            super::ExecutionTarget::WindowsNative => WINDOWS_START_FILE,
        });
        let mut poll_index = 0;
        loop {
            if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
                return self.reconcile_runner_result(attempt);
            }
            if start_path.exists() {
                match self.bind_attempt_start(attempt, plan.execution_target) {
                    Ok(_) => return Ok(()),
                    Err(error) if error.code == RuntimeErrorCode::LaunchIdentityMismatch => {
                        // A very short-lived unit can write valid start evidence, finish, and be
                        // unloaded between the filesystem check and systemctl_show. A complete
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

    fn bind_attempt_start(
        &self,
        attempt: &AttemptRecord,
        target: super::ExecutionTarget,
    ) -> RuntimeResult<AttemptRecord> {
        match target {
            super::ExecutionTarget::LocalLinux => self.bind_runner_start(attempt),
            super::ExecutionTarget::WindowsNative => self.bind_windows_start(attempt),
        }
    }

    fn validate_windows_launcher_start_evidence(
        &self,
        attempt: &AttemptRecord,
    ) -> RuntimeResult<(WindowsLauncherStartEvidence, String)> {
        let path = Path::new(&attempt.bundle_path).join(WINDOWS_LAUNCHER_START_FILE);
        let bytes = fs::read(&path)
            .map_err(|error| io_error("read Windows launcher start evidence", error))?;
        let evidence: WindowsLauncherStartEvidence =
            serde_json::from_slice(&bytes).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::LaunchIdentityMismatch,
                    format!("invalid Windows launcher start evidence: {error}"),
                    Some("windowsLauncherStart"),
                    false,
                )
            })?;
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        if plan.execution_target != super::ExecutionTarget::WindowsNative {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "Windows launcher start evidence belongs to a non-Windows execution plan",
                Some("windowsLauncherStart"),
                false,
            ));
        }
        let windows = self.windows.as_ref().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "committed windows_native Job has no configured Windows provider",
                Some("executionTarget"),
                true,
            )
        })?;
        if windows.wsl_distribution.is_some() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "early Windows launcher evidence is reserved for native control-plane dispatch",
                Some("windowsLauncherStart"),
                false,
            ));
        }
        let provider = self
            .registry
            .execution_provider(&attempt.job_id)?
            .ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "native Windows Attempt has no committed execution provider",
                    Some("executionProvider"),
                    false,
                )
            })?;
        let expected_job_name = format!("Ordivon.{}", attempt.attempt_id);
        if evidence.schema_version != RUNTIME_SCHEMA_VERSION
            || evidence.job_id != attempt.job_id
            || evidence.attempt_id != attempt.attempt_id
            || evidence.launch_token_digest != attempt.launch_token_digest
            || evidence.job_name != expected_job_name
            || evidence.launcher_process_id == 0
            || evidence.launcher_process_creation_time_file_time == 0
            || evidence.observed_unix_ms == 0
            || provider.contract != ExecutionProviderContract::WindowsNativeLauncherV1
            || provider.wsl_distribution.is_some()
            || evidence.launcher_image_digest != provider.executable_digest
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "Windows launcher start identity does not match committed native Attempt",
                Some("windowsLauncherStart"),
                false,
            ));
        }
        let digest = sha256_bytes(&bytes);
        if digest != sha256_file(&path).map_err(map_universal_error)? {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "Windows launcher start evidence digest changed while reading",
                Some("windowsLauncherStart"),
                false,
            ));
        }
        Ok((evidence, digest))
    }

    fn validate_windows_start_evidence(
        &self,
        attempt: &AttemptRecord,
    ) -> RuntimeResult<(WindowsStartEvidence, String)> {
        let path = Path::new(&attempt.bundle_path).join(WINDOWS_START_FILE);
        let bytes =
            fs::read(&path).map_err(|error| io_error("read Windows start evidence", error))?;
        let evidence: WindowsStartEvidence = serde_json::from_slice(&bytes).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                format!("invalid Windows start evidence: {error}"),
                Some("windowsStart"),
                false,
            )
        })?;
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        if plan.execution_target != super::ExecutionTarget::WindowsNative {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "Windows start evidence belongs to a non-Windows execution plan",
                Some("windowsStart"),
                false,
            ));
        }
        let windows = self.windows.as_ref().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "committed windows_native Job has no configured Windows provider",
                Some("executionTarget"),
                true,
            )
        })?;
        let context = plan.windows_execution_context.as_ref().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "committed windows_native Job has no frozen Windows execution context",
                Some("windowsExecutionContext"),
                false,
            )
        })?;
        let expected_token_class = match plan.windows_authority {
            super::WindowsAuthority::Limited => super::WindowsTokenClass::Limited,
            super::WindowsAuthority::Elevated => super::WindowsTokenClass::Elevated,
        };
        if context.token_class != expected_token_class
            || context.environment_source != "windows_user_machine_profile_allowlist_v1"
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "committed Windows requested/effective authority is inconsistent",
                Some("windowsExecutionContext"),
                false,
            ));
        }
        let expected_job_name = format!("Ordivon.{}", attempt.attempt_id);
        let expected_image =
            windows_visible_path(windows, Path::new(&plan.executable), "execution.executable")?;
        let observed_image = evidence
            .image_path
            .strip_prefix("\\\\?\\")
            .unwrap_or(&evidence.image_path);
        let expected_image_normalized = expected_image
            .strip_prefix("\\\\?\\")
            .unwrap_or(&expected_image);
        let expected_input_evidence = if plan.effective_inputs.is_empty() {
            (None, None, None)
        } else {
            let input_set_id = plan.input_set_id.as_deref().ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "Windows immutable inputs have no committed inputSetId",
                    Some("executionPlan.inputSetId"),
                    false,
                )
            })?;
            let presentation_root = required_environment_value_case_insensitive(
                &plan.env,
                "ORDIVON_INPUT_ROOT",
                "executionPlan.env",
            )?;
            (
                Some(input_set_id),
                Some(presentation_root),
                Some(windows_input_bindings_digest(&plan.effective_inputs)),
            )
        };
        let token_authority_matches = match plan.windows_authority {
            super::WindowsAuthority::Limited => {
                !evidence.token_is_elevated
                    && evidence.token_integrity_level_rid <= 8192
                    && (evidence.administrators_group_attributes == u32::MAX
                        || (evidence.administrators_group_attributes & 0x4) == 0
                        || (evidence.administrators_group_attributes & 0x10) != 0)
                    && matches!(
                        evidence.token_selection.as_str(),
                        "lua_medium_filtered" | "current_limited"
                    )
                    && (evidence.token_selection != "lua_medium_filtered"
                        || evidence.administrators_group_attributes == u32::MAX
                        || (evidence.administrators_group_attributes & 0x10) != 0)
            }
            super::WindowsAuthority::Elevated => {
                evidence.token_is_elevated
                    && evidence.token_integrity_level_rid >= 12288
                    && evidence.administrators_group_attributes != u32::MAX
                    && (evidence.administrators_group_attributes & 0x4) != 0
                    && (evidence.administrators_group_attributes & 0x10) == 0
                    && evidence.token_selection == "current_elevated"
            }
        };
        if evidence.schema_version != RUNTIME_SCHEMA_VERSION
            || evidence.job_id != attempt.job_id
            || evidence.attempt_id != attempt.attempt_id
            || evidence.launch_token_digest != attempt.launch_token_digest
            || evidence.job_name != expected_job_name
            || evidence.launcher_process_id == 0
            || evidence.process_id == 0
            || evidence.process_creation_time_file_time == 0
            || evidence.image_digest != plan.executable_digest
            || evidence.token_user_sid != context.token_user_sid
            || evidence.token_type != 1
            || !token_authority_matches
            || evidence.power_request_type != "system_required"
            || !evidence.power_request_acquired
            || evidence.input_set_id.as_deref() != expected_input_evidence.0
            || evidence.input_presentation_root.as_deref() != expected_input_evidence.1
            || evidence.input_bindings_digest.as_deref() != expected_input_evidence.2.as_deref()
            || !observed_image.eq_ignore_ascii_case(expected_image_normalized)
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "Windows start identity does not match committed Attempt",
                Some("windowsStart"),
                false,
            ));
        }
        let start_digest = sha256_bytes(&bytes);
        if start_digest != sha256_file(&path).map_err(map_universal_error)? {
            return Err(RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "Windows start evidence digest changed while reading",
                Some("windowsStart"),
                false,
            ));
        }
        if windows.wsl_distribution.is_none() {
            let provider = self
                .registry
                .execution_provider(&attempt.job_id)?
                .ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorCode::RegistryCorrupt,
                        "native Windows Attempt has no committed execution provider",
                        Some("executionProvider"),
                        false,
                    )
                })?;
            if provider.contract != ExecutionProviderContract::WindowsNativeLauncherV1
                || provider.wsl_distribution.is_some()
                || evidence
                    .launcher_process_creation_time_file_time
                    .is_none_or(|identity| identity == 0)
                || evidence.launcher_image_digest.as_deref()
                    != Some(provider.executable_digest.as_str())
            {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::LaunchIdentityMismatch,
                    "native Windows launcher owner identity does not match the committed provider",
                    Some("windowsStart"),
                    false,
                ));
            }
        }
        Ok((evidence, start_digest))
    }

    fn bind_windows_start(&self, attempt: &AttemptRecord) -> RuntimeResult<AttemptRecord> {
        let (evidence, start_digest) = self.validate_windows_start_evidence(attempt)?;
        let windows = self.windows.as_ref().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "committed windows_native Job has no configured Windows provider",
                Some("executionTarget"),
                true,
            )
        })?;
        if windows.wsl_distribution.is_none() {
            let launcher_process_creation_time_file_time = evidence
                .launcher_process_creation_time_file_time
                .filter(|identity| *identity != 0)
                .ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorCode::LaunchIdentityMismatch,
                        "native Windows start evidence omitted launcher process creation identity",
                        Some("windowsStart.launcherProcessCreationTimeFileTime"),
                        false,
                    )
                })?;
            let launcher_image_digest =
                evidence.launcher_image_digest.clone().ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorCode::LaunchIdentityMismatch,
                        "native Windows start evidence omitted launcher image digest",
                        Some("windowsStart.launcherImageDigest"),
                        false,
                    )
                })?;
            return self.registry.bind_supervisor_owner(
                &attempt.attempt_id,
                attempt.row_version,
                &AttemptSupervisorOwner::WindowsLauncherV1 {
                    launcher_process_id: evidence.launcher_process_id,
                    launcher_process_creation_time_file_time,
                    launcher_image_digest,
                    job_name: evidence.job_name.clone(),
                    start_evidence_digest: start_digest,
                },
                evidence.observed_unix_ms,
            );
        }
        let properties = systemctl_show(&attempt.unit_name)?;
        let invocation_id = nonempty_property(&properties, "InvocationID")
            .ok_or_else(|| missing_systemd_property("InvocationID"))?;
        let control_group = nonempty_property(&properties, "ControlGroup")
            .ok_or_else(|| missing_systemd_property("ControlGroup"))?;
        let main_pid: u32 = properties
            .get("MainPID")
            .ok_or_else(|| missing_systemd_property("MainPID"))?
            .parse()
            .map_err(|_| missing_systemd_property("MainPID"))?;
        if main_pid == 0 {
            return Err(missing_systemd_property("MainPID"));
        }
        let process_start_identity = process_identity(main_pid).ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::LaunchIdentityMismatch,
                "Windows launcher systemd MainPID has no observable host process identity",
                Some("mainPid"),
                false,
            )
        })?;
        let boot_id = read_trimmed("/proc/sys/kernel/random/boot_id")?;
        self.registry.bind_running(
            &attempt.attempt_id,
            attempt.row_version,
            &RunnerIdentity {
                boot_id,
                unit_name: attempt.unit_name.clone(),
                invocation_id,
                control_group,
                main_pid,
                process_start_identity,
                runner_start_digest: start_digest,
                observed_at_ms: evidence.observed_unix_ms,
            },
        )
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
        if let Some(provider) = self.registry.execution_provider(&attempt.job_id)? {
            if provider.contract == super::ExecutionProviderContract::LocalLinuxRunnerV1 {
                let observed = evidence.runner_executable_digest.as_deref().ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorCode::LaunchIdentityMismatch,
                        "provider-bound Runner-start evidence omitted the actual Runner image digest",
                        Some("runnerStart.runnerExecutableDigest"),
                        false,
                    )
                })?;
                if observed != provider.executable_digest {
                    return Err(RuntimeError::new(
                        RuntimeErrorCode::LaunchIdentityMismatch,
                        format!(
                            "actual Runner image differs from the committed execution provider: expected {}, observed {}",
                            provider.executable_digest, observed
                        ),
                        Some("runnerStart.runnerExecutableDigest"),
                        false,
                    ));
                }
            }
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
        if let Some(provider) = self.registry.execution_provider(&attempt.job_id)? {
            if provider.contract == super::ExecutionProviderContract::LocalLinuxRunnerV1 {
                let proc_exe = PathBuf::from(format!("/proc/{main_pid}/exe"));
                match sha256_file(&proc_exe) {
                    Ok(os_observed) => {
                        if evidence.runner_executable_digest.as_deref()
                            != Some(os_observed.as_str())
                            || os_observed != provider.executable_digest
                        {
                            return Err(RuntimeError::new(
                                RuntimeErrorCode::LaunchIdentityMismatch,
                                format!(
                                    "systemd MainPID image differs from committed/self-reported Runner provider: committed {}, runner-start {:?}, OS observed {}",
                                    provider.executable_digest,
                                    evidence.runner_executable_digest,
                                    os_observed
                                ),
                                Some("runnerStart.runnerExecutableDigest"),
                                false,
                            ));
                        }
                    }
                    Err(error) => {
                        if process_identity(main_pid).as_deref()
                            == Some(process_start_identity.as_str())
                        {
                            return Err(RuntimeError::new(
                                RuntimeErrorCode::LaunchIdentityMismatch,
                                format!(
                                    "cannot independently inspect the live systemd MainPID Runner image: {error}"
                                ),
                                Some("mainPid"),
                                false,
                            ));
                        }
                    }
                }
            }
        }
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
                        | RuntimeErrorCode::LaunchIdentityMismatch
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
                if current.state != AttemptState::Orphaned {
                    self.release_attempt_supervisor(&current)?;
                }
                return self.observation_from_registry(&current.job_id, 0, 0);
            }
            let mut terminal = self.prepare_runner_terminal(&current)?;
            self.append_terminal_evidence(&current, &mut terminal)?;
            match self.registry.commit_terminal(&terminal) {
                Ok(_) => {
                    self.release_attempt_supervisor(&current)?;
                    self.cleanup_payload_view(&current.attempt_id)?;
                    return self.observation_from_registry(&current.job_id, 4096, 4096);
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
        let mut terminal = prepare_runner_terminal_from_bundle(current)?;
        let plan = self.registry.execution_plan(&current.job_id)?;
        if plan.execution_target == super::ExecutionTarget::WindowsNative {
            let (_, digest) = self.validate_windows_start_evidence(current)?;
            let path = Path::new(&current.bundle_path).join(WINDOWS_START_FILE);
            terminal.artifacts.push(ArtifactRegistration {
                artifact_id: format!("{}.windows-start", current.attempt_id),
                kind: "windows_start".to_string(),
                relative_path: WINDOWS_START_FILE.to_string(),
                digest,
                media_type: "application/json".to_string(),
                byte_length: fs::metadata(&path)
                    .map_err(|error| io_error("inspect Windows start evidence", error))?
                    .len(),
                truncated: false,
            });
        }
        Ok(terminal)
    }

    pub fn observe_task(&self, request: &TaskObserveRequest) -> RuntimeResult<TaskObservation> {
        validate_observe_request(request)?;
        let deadline = Instant::now() + Duration::from_millis(request.wait_ms);
        let mut poll_index = 0;
        let mut initial_signature = None;
        loop {
            self.reconcile_job(&request.job_id)?;
            let snapshot = self.registry.job_snapshot(&request.job_id)?;
            let signature =
                task_activity_signature(snapshot.attempt.as_ref(), &snapshot.projection)?;
            let changed = initial_signature
                .as_ref()
                .is_some_and(|initial| initial != &signature);
            if initial_signature.is_none() {
                initial_signature = Some(signature);
            }
            if snapshot.projection.result_available
                || request.wait_ms == 0
                || Instant::now() >= deadline
                || (request.wait_until == TaskObserveWaitUntil::ChangeOrTerminal && changed)
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
        let effective_limits = effective_limits_from_plan_json(&snapshot.job.execution_plan_json)?;
        self.observation_from_parts(
            snapshot.projection,
            snapshot.attempt,
            effective_limits,
            ObservationOutputRequest {
                stdout_tail_bytes,
                stderr_tail_bytes,
                stdout_offset: None,
                stderr_offset: None,
            },
        )
    }

    fn observation_from_snapshot(
        &self,
        snapshot: JobSnapshot,
        request: &TaskObserveRequest,
    ) -> RuntimeResult<TaskObservation> {
        let effective_limits = effective_limits_from_plan_json(&snapshot.job.execution_plan_json)?;
        self.observation_from_parts(
            snapshot.projection,
            snapshot.attempt,
            effective_limits,
            ObservationOutputRequest {
                stdout_tail_bytes: request.stdout_tail_bytes,
                stderr_tail_bytes: request.stderr_tail_bytes,
                stdout_offset: request.stdout_offset,
                stderr_offset: request.stderr_offset,
            },
        )
    }

    fn observation_from_parts(
        &self,
        projection: super::JobProjection,
        attempt: Option<AttemptRecord>,
        effective_limits: super::EffectiveExecutionLimits,
        output_request: ObservationOutputRequest,
    ) -> RuntimeResult<TaskObservation> {
        let job_id = projection.job_id.clone();
        let terminal = projection.result_available;
        let now = now_ms()?;
        let progress = attempt
            .as_ref()
            .map(load_runner_progress_if_present)
            .transpose()?
            .flatten();
        // Orphaned is the quarantine state for invalid or identity-conflicting Runner
        // evidence. The corrupt file remains available for diagnosis, but observation must
        // project the committed control result instead of reparsing quarantined evidence.
        let result = if projection.status == "orphaned" {
            None
        } else {
            attempt
                .as_ref()
                .map(load_runner_result_if_present)
                .transpose()?
                .flatten()
        };
        let control_error_summary = if result.is_none() {
            attempt
                .as_ref()
                .map(load_control_error_summary_if_present)
                .transpose()?
                .flatten()
        } else {
            None
        };
        let (
            stdout_view,
            stderr_view,
            stdout_truncated,
            stderr_truncated,
            artifacts,
            error_summary,
            last_output_at_ms,
        ) = if let Some(attempt) = &attempt {
            let stdout_view = read_output_text(
                &Path::new(&attempt.bundle_path).join(STDOUT_FILE),
                output_request.stdout_offset,
                output_request.stdout_tail_bytes,
                terminal,
                "stdoutOffset",
                "stdoutTailBytes",
            )?;
            let stderr_view = read_output_text(
                &Path::new(&attempt.bundle_path).join(STDERR_FILE),
                output_request.stderr_offset,
                output_request.stderr_tail_bytes,
                terminal,
                "stderrOffset",
                "stderrTailBytes",
            )?;
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
                .or(control_error_summary);
            let last_output_at_ms = latest_output_modified_ms(attempt)?;
            (
                stdout_view,
                stderr_view,
                stdout_truncated,
                stderr_truncated,
                artifacts,
                error_summary,
                last_output_at_ms,
            )
        } else {
            (
                OutputView::empty(output_request.stdout_offset, terminal),
                OutputView::empty(output_request.stderr_offset, terminal),
                false,
                false,
                Vec::new(),
                None,
                None,
            )
        };
        Ok(TaskObservation {
            job_id,
            operation_digest: projection.operation_digest,
            status: projection.status,
            desired_state: projection.desired_state,
            attempt_id: attempt.as_ref().map(|attempt| attempt.attempt_id.clone()),
            attempt_state: projection.attempt_state,
            termination_intent: projection.termination_intent,
            exit_code: projection.exit_code,
            execution_terminal: projection.execution_terminal,
            execution_disposition: projection.execution_disposition,
            execution_reason_code: projection.execution_reason_code,
            delivery_disposition: projection.delivery_disposition,
            effective_limits,
            recovery_required: projection.recovery_required,
            semantic_completion_evaluated: projection.semantic_completion_evaluated,
            result_available: projection.result_available,
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
            elapsed_ms: attempt.as_ref().map(|attempt| {
                let started_at_ms = attempt.started_at_ms.unwrap_or(attempt.created_at_ms);
                attempt
                    .finished_at_ms
                    .unwrap_or(now)
                    .saturating_sub(started_at_ms)
            }),
            last_output_at_ms,
            progress_revision: progress.as_ref().map(|progress| progress.revision),
            completed_steps: progress.as_ref().map(|progress| progress.completed_steps),
            total_steps: progress.as_ref().map(|progress| progress.total_steps),
            current_step_id: progress
                .as_ref()
                .and_then(|progress| progress.current_step_id.clone()),
            current_step_index: progress
                .as_ref()
                .and_then(|progress| progress.current_step_index),
            current_step_elapsed_ms: progress.as_ref().and_then(|progress| {
                progress
                    .current_step_started_unix_ms
                    .and_then(|started| u64::try_from(started).ok())
                    .map(|started| now.saturating_sub(started))
            }),
            failed_step_id: result
                .as_ref()
                .and_then(|result| result.failed_step_id.clone())
                .or_else(|| {
                    progress
                        .as_ref()
                        .and_then(|progress| progress.failed_step_id.clone())
                }),
            failed_step_index: result
                .as_ref()
                .and_then(|result| result.failed_step_index)
                .or_else(|| {
                    progress
                        .as_ref()
                        .and_then(|progress| progress.failed_step_index)
                }),
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
        self.reconcile_prepared_input_sets(STALE_PREPARED_INPUT_AGE_MS)?;
        let attempts = self.registry.list_held_orphaned_attempts()?;
        self.reconcile_candidates_into(attempts, report)
    }

    pub(super) fn reconcile_prepared_input_sets(&self, stale_age_ms: u64) -> RuntimeResult<()> {
        let root = self.executor.input_materializations_root();
        fs::create_dir_all(&root)
            .map_err(|error| io_error("create input materialization root", error))?;
        let now = SystemTime::now();
        for entry in
            fs::read_dir(&root).map_err(|error| io_error("enumerate prepared input sets", error))?
        {
            let entry = entry.map_err(|error| io_error("read prepared input set entry", error))?;
            let metadata = entry
                .metadata()
                .map_err(|error| io_error("inspect prepared input set", error))?;
            if !metadata.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(job_id) = name.to_str() else {
                continue;
            };
            if !job_id.starts_with("job-") {
                continue;
            }
            match self.registry.get_job(job_id) {
                Ok(job) => {
                    let plan: RuntimeExecutionPlan = serde_json::from_str(&job.execution_plan_json)
                        .map_err(|error| {
                            RuntimeError::new(
                                RuntimeErrorCode::RegistryCorrupt,
                                format!("stored execution plan is invalid: {error}"),
                                Some("executionPlan"),
                                false,
                            )
                        })?;
                    if plan.effective_inputs.is_empty() || plan.input_set_id.is_none() {
                        continue;
                    }
                    if self.ensure_job_input_ownership(job_id).is_err() {
                        // One corrupt Job must not block Runtime startup. Its Attempt owns
                        // deterministic failure/quarantine; never reopen current source authority.
                        continue;
                    }
                }
                Err(error) if error.code == RuntimeErrorCode::JobNotFound => {
                    let old_enough = metadata
                        .modified()
                        .ok()
                        .and_then(|modified| now.duration_since(modified).ok())
                        .is_some_and(|age| age.as_millis() >= u128::from(stale_age_ms));
                    if old_enough {
                        self.discard_prepared_input_set(&entry.path())?;
                    }
                }
                Err(error) => return Err(error),
            }
        }

        // Copy-in-progress state is guarded by a process-held flock. A hard crash releases
        // the lock automatically, allowing stale staging state to be collected without
        // guessing how long a valid large copy may take.
        for entry in fs::read_dir(&root)
            .map_err(|error| io_error("enumerate input staging leases", error))?
        {
            let entry = entry.map_err(|error| io_error("read input staging lease entry", error))?;
            let name = entry.file_name();
            let Some(job_id) = name
                .to_str()
                .and_then(|name| name.strip_prefix('.'))
                .and_then(|name| name.strip_suffix(".lease"))
                .filter(|job_id| job_id.starts_with("job-"))
            else {
                continue;
            };
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| io_error("inspect input staging lease", error))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                continue;
            }
            let old_enough = metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age.as_millis() >= u128::from(stale_age_ms));
            if !old_enough {
                continue;
            }
            let lease = match OpenOptions::new().read(true).write(true).open(entry.path()) {
                Ok(lease) => lease,
                Err(_) => continue,
            };
            if unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                continue;
            }
            let prefix = format!(".{job_id}.staging-");
            for staging in fs::read_dir(&root)
                .map_err(|error| io_error("enumerate abandoned input staging directories", error))?
            {
                let staging = staging
                    .map_err(|error| io_error("read abandoned input staging entry", error))?;
                let staging_name = staging.file_name();
                if staging_name
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix))
                    && staging
                        .file_type()
                        .map(|kind| kind.is_dir())
                        .unwrap_or(false)
                {
                    fs::remove_dir_all(staging.path()).map_err(|error| {
                        io_error("remove abandoned input staging directory", error)
                    })?;
                }
            }
            fs::remove_file(entry.path())
                .map_err(|error| io_error("remove abandoned input staging lease", error))?;
            drop(lease);
            sync_directory(&root)?;
        }
        Ok(())
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
        let mut terminal = TerminalCommit {
            attempt_id: attempt.attempt_id.clone(),
            expected_row_version: attempt.row_version,
            state,
            result_digest,
            exit_code: None,
            infrastructure_error_digest: Some(sha256_bytes(reason_code.as_bytes())),
            finished_at_ms: observed_at_ms,
            artifacts,
            reason_code: reason_code.to_string(),
        };
        self.append_terminal_evidence(attempt, &mut terminal)?;
        self.registry.recover_orphaned_terminal(&terminal)?;
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
        self.append_terminal_evidence(&current, &mut terminal)?;
        self.registry.recover_orphaned_terminal(&terminal)?;
        self.release_attempt_supervisor(&current)?;
        self.cleanup_payload_view(&current.attempt_id)?;
        Ok(true)
    }

    fn orphan_process_tree_alive(&self, attempt: &AttemptRecord) -> RuntimeResult<bool> {
        if let Some(owner) = self
            .registry
            .attempt_supervisor_owner(&attempt.attempt_id)?
        {
            let windows = self.windows.as_ref().ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "Attempt Supervisor Owner exists without a configured Windows provider",
                    Some("attemptSupervisorOwner"),
                    false,
                )
            })?;
            let AttemptSupervisorOwner::WindowsLauncherV1 {
                launcher_process_id,
                launcher_process_creation_time_file_time,
                ..
            } = owner;
            let observed = observe_windows_launcher_owner(windows, launcher_process_id)?;
            return Ok(observed.process_alive
                && observed.process_creation_time_file_time
                    == Some(launcher_process_creation_time_file_time));
        }
        attempt_process_tree_alive(attempt)
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
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        let start_path = Path::new(&attempt.bundle_path).join(match plan.execution_target {
            super::ExecutionTarget::LocalLinux => RUNNER_START_FILE,
            super::ExecutionTarget::WindowsNative => WINDOWS_START_FILE,
        });
        let native_unbound_stopping = attempt.state == AttemptState::Stopping
            && plan.execution_target == super::ExecutionTarget::WindowsNative
            && self
                .windows
                .as_ref()
                .is_some_and(|windows| windows.wsl_distribution.is_none())
            && self
                .registry
                .attempt_supervisor_owner(&attempt.attempt_id)?
                .is_none();
        if (attempt.state == AttemptState::Starting || native_unbound_stopping)
            && start_path.exists()
        {
            let bound = self.bind_attempt_start(&attempt, plan.execution_target)?;
            if Path::new(&bound.bundle_path).join(RESULT_FILE).exists() {
                return self.reconcile_runner_result(&bound);
            }
            if native_unbound_stopping {
                return self.reconcile_bound_attempt(&bound);
            }
            return Ok(());
        }
        if attempt.state == AttemptState::Starting || native_unbound_stopping {
            return self.reconcile_starting_without_token(&attempt);
        }
        self.reconcile_bound_attempt(&attempt)
    }

    fn reconcile_starting_without_token(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        if plan.execution_target == super::ExecutionTarget::WindowsNative
            && self
                .windows
                .as_ref()
                .is_some_and(|windows| windows.wsl_distribution.is_none())
        {
            return self.reconcile_native_starting_without_target_evidence(attempt);
        }
        let properties = systemctl_show(&attempt.unit_name)?;
        let active = unit_is_active(&properties);
        let age_ms = now_ms()?.saturating_sub(attempt.created_at_ms);
        let wsl_distribution_configured = self
            .windows
            .as_ref()
            .is_some_and(|windows| windows.wsl_distribution.is_some());
        if (active && age_ms < self.startup_grace_ms)
            || wsl_backed_windows_live_unit_must_wait(
                plan.execution_target,
                wsl_distribution_configured,
                active,
            )
        {
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

    fn enforce_native_windows_outer_deadline(
        &self,
        attempt: &AttemptRecord,
        plan: &RuntimeExecutionPlan,
        deadline_started_at_ms: u64,
        launcher_process_id: u32,
        launcher_process_creation_time_file_time: u64,
        pre_target_start: bool,
    ) -> RuntimeResult<bool> {
        let observed_at_ms = now_ms()?;
        let current = match attempt.termination_intent {
            AttemptTerminationIntent::Natural
                if native_windows_outer_deadline_due(
                    deadline_started_at_ms,
                    plan.timeout_ms,
                    observed_at_ms,
                ) =>
            {
                self.registry
                    .request_deadline_termination(&attempt.attempt_id, observed_at_ms)?
            }
            AttemptTerminationIntent::DeadlineExceeded => {
                self.registry.get_attempt(&attempt.attempt_id)?
            }
            AttemptTerminationIntent::Natural | AttemptTerminationIntent::StopRequested => {
                return Ok(false);
            }
        };
        if current.state.is_terminal() {
            return Ok(true);
        }
        if current.termination_intent != AttemptTerminationIntent::DeadlineExceeded {
            return Ok(false);
        }
        if Path::new(&current.bundle_path).join(RESULT_FILE).is_file() {
            self.reconcile_runner_result(&current)?;
            return Ok(true);
        }
        let windows = self.windows.as_ref().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "native Windows deadline enforcement has no configured provider",
                Some("executionTarget"),
                true,
            )
        })?;
        if windows.wsl_distribution.is_some() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "direct native Windows deadline enforcement cannot use a WSL provider",
                Some("executionTarget"),
                false,
            ));
        }
        let disposition = terminate_windows_launcher_owner_for_deadline(
            windows,
            launcher_process_id,
            launcher_process_creation_time_file_time,
        )?;
        let mut current = self.registry.get_attempt(&current.attempt_id)?;
        if Path::new(&current.bundle_path).join(RESULT_FILE).is_file() {
            self.reconcile_runner_result(&current)?;
            return Ok(true);
        }
        if pre_target_start
            && Path::new(&current.bundle_path)
                .join(WINDOWS_START_FILE)
                .is_file()
        {
            current = self.bind_attempt_start(&current, super::ExecutionTarget::WindowsNative)?;
            if Path::new(&current.bundle_path).join(RESULT_FILE).is_file() {
                self.reconcile_runner_result(&current)?;
                return Ok(true);
            }
        }
        let disposition = match disposition {
            WindowsDeadlineOwnerTerminationDisposition::Terminated => "terminated",
            WindowsDeadlineOwnerTerminationDisposition::AlreadyAbsent => "already_absent",
            WindowsDeadlineOwnerTerminationDisposition::IdentityMismatch => return Ok(false),
        };
        self.commit_control_terminal(
            &current,
            AttemptState::TimedOut,
            "NATIVE_WINDOWS_OUTER_DEADLINE_CONTROL_TERMINAL",
            Some(format!(
                "outer deadline intent was durably committed before exact launcher-owner termination; disposition={disposition}; no runner result was available. TimedOut is derived from the persisted outer deadline and proven loss of the exact owner, not from owner death alone"
            )),
        )?;
        Ok(true)
    }

    fn reconcile_native_starting_without_target_evidence(
        &self,
        attempt: &AttemptRecord,
    ) -> RuntimeResult<()> {
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        let launcher_start_path = Path::new(&attempt.bundle_path).join(WINDOWS_LAUNCHER_START_FILE);
        if launcher_start_path.is_file() {
            let (evidence, _) = self.validate_windows_launcher_start_evidence(attempt)?;
            let windows = self.windows.as_ref().ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "native Windows Starting Attempt has no configured provider",
                    Some("executionTarget"),
                    true,
                )
            })?;
            let observation =
                observe_windows_launcher_owner(windows, evidence.launcher_process_id)?;
            if Path::new(&attempt.bundle_path).join(RESULT_FILE).is_file() {
                return self.reconcile_runner_result(attempt);
            }
            if Path::new(&attempt.bundle_path)
                .join(WINDOWS_START_FILE)
                .is_file()
            {
                let running =
                    self.bind_attempt_start(attempt, super::ExecutionTarget::WindowsNative)?;
                if Path::new(&running.bundle_path).join(RESULT_FILE).is_file() {
                    return self.reconcile_runner_result(&running);
                }
                return Ok(());
            }
            if observation.process_alive
                && observation.process_creation_time_file_time
                    == Some(evidence.launcher_process_creation_time_file_time)
            {
                if self.enforce_native_windows_outer_deadline(
                    attempt,
                    &plan,
                    evidence.observed_unix_ms,
                    evidence.launcher_process_id,
                    evidence.launcher_process_creation_time_file_time,
                    true,
                )? {
                    return Ok(());
                }
                return Ok(());
            }
            // The actual target is created suspended and windows-start.json is published before
            // ResumeThread. Once the original provisional launcher owner is gone, no future target
            // effect can emerge from this dispatch if target-start evidence is still absent.
            let (terminal_state, reason_code) = match attempt.termination_intent {
                AttemptTerminationIntent::StopRequested => (
                    AttemptState::Cancelled,
                    "STOP_REQUESTED_WINDOWS_LAUNCHER_PRESTART_GONE",
                ),
                AttemptTerminationIntent::DeadlineExceeded => (
                    AttemptState::TimedOut,
                    "DEADLINE_WINDOWS_LAUNCHER_PRESTART_GONE",
                ),
                AttemptTerminationIntent::Natural => {
                    (AttemptState::Failed, "WINDOWS_LAUNCHER_PRESTART_GONE")
                }
            };
            self.commit_control_terminal(
                attempt,
                terminal_state,
                reason_code,
                Some(
                    "native launcher owner disappeared before durable target-start evidence; the target could not have been resumed"
                        .to_string(),
                ),
            )?;
            return Ok(());
        }
        let age_ms = now_ms()?.saturating_sub(attempt.created_at_ms);
        if age_ms < self.startup_grace_ms {
            return Ok(());
        }
        Err(RuntimeError::new(
            RuntimeErrorCode::LaunchIdentityMismatch,
            "native dispatch intent has no launcher-start, target-start, or result evidence; retain the Starting Attempt and capacity until new evidence appears or an operator reconciles it",
            Some("windowsLauncherStart"),
            true,
        ))
    }

    fn reconcile_provider_owned_attempt(
        &self,
        attempt: &AttemptRecord,
        plan: &RuntimeExecutionPlan,
        owner: &AttemptSupervisorOwner,
    ) -> RuntimeResult<()> {
        if plan.execution_target != super::ExecutionTarget::WindowsNative {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "Attempt Supervisor Owner is bound to a non-Windows execution target",
                Some("attemptSupervisorOwner"),
                false,
            ));
        }
        if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
            return self.reconcile_runner_result(attempt);
        }
        let windows = self.windows.as_ref().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "provider-owned Windows Attempt has no configured Windows provider",
                Some("executionTarget"),
                true,
            )
        })?;
        if windows.wsl_distribution.is_some() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "native Attempt Supervisor Owner cannot be reconciled through a WSL provider",
                Some("attemptSupervisorOwner"),
                false,
            ));
        }
        let AttemptSupervisorOwner::WindowsLauncherV1 {
            launcher_process_id,
            launcher_process_creation_time_file_time,
            ..
        } = owner;
        let deadline_started_at_ms = attempt.started_at_ms.ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "native Attempt Supervisor Owner has no durable execution start time",
                Some("attemptSupervisorOwner"),
                false,
            )
        })?;
        if self.enforce_native_windows_outer_deadline(
            attempt,
            plan,
            deadline_started_at_ms,
            *launcher_process_id,
            *launcher_process_creation_time_file_time,
            false,
        )? {
            return Ok(());
        }
        let observation = observe_windows_launcher_owner(windows, *launcher_process_id)?;
        if Path::new(&attempt.bundle_path).join(RESULT_FILE).exists() {
            return self.reconcile_runner_result(attempt);
        }
        let intent = match attempt.termination_intent {
            AttemptTerminationIntent::Natural => TerminationIntent::Natural,
            AttemptTerminationIntent::StopRequested => TerminationIntent::StopRequested,
            AttemptTerminationIntent::DeadlineExceeded => TerminationIntent::DeadlineExceeded,
        };
        match classify_windows_launcher_recovery(owner, &observation, intent)? {
            SupervisorRecoveryDisposition::Running => Ok(()),
            SupervisorRecoveryDisposition::Terminal(state) => {
                let reason_code = match intent {
                    TerminationIntent::StopRequested => "STOP_REQUESTED_PROCESS_TREE_GONE",
                    TerminationIntent::DeadlineExceeded => "DEADLINE_EXCEEDED",
                    TerminationIntent::Natural => "WINDOWS_LAUNCHER_LINEAGE_GONE",
                };
                self.commit_control_terminal(
                    attempt,
                    state,
                    reason_code,
                    (intent == TerminationIntent::Natural).then(|| {
                        "native Windows launcher owner identity is gone; JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE makes the Attempt process tree definitively non-running and no result evidence exists".to_string()
                    }),
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
                Ok(())
            }
            SupervisorRecoveryDisposition::Lost => Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "native Windows launcher classifier returned unsupported lost disposition",
                Some("attemptSupervisorOwner"),
                false,
            )),
        }
    }

    fn reconcile_bound_attempt(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        if let Some(owner) = self
            .registry
            .attempt_supervisor_owner(&attempt.attempt_id)?
        {
            return self.reconcile_provider_owned_attempt(attempt, &plan, &owner);
        }
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
        if windows_native_launcher_lineage_is_definite_failure(
            plan.execution_target,
            attempt.termination_intent,
            observation.unit_state,
            observation.recorded_pid_alive,
        ) {
            self.commit_observed_control_terminal(
                attempt,
                AttemptState::Failed,
                "WINDOWS_LAUNCHER_LINEAGE_GONE",
                Some(
                    "windows_native launcher unit and persisted launcher process identity are absent; the launcher is the sole owner of the JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE handle, so the native process tree cannot still be running and no result evidence exists"
                        .to_string(),
                ),
                Some(&observation),
            )?;
            return Ok(());
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
                let reason_code = match (observation.unit_state, intent) {
                    (SupervisorUnitState::NotFound, TerminationIntent::StopRequested) => {
                        "STOP_REQUESTED_PROCESS_TREE_GONE"
                    }
                    (SupervisorUnitState::NotFound, TerminationIntent::DeadlineExceeded) => {
                        "DEADLINE_EXCEEDED"
                    }
                    _ => "SUPERVISOR_TERMINAL_FALLBACK",
                };
                self.commit_observed_control_terminal(
                    attempt,
                    state,
                    reason_code,
                    None,
                    Some(&observation),
                )?;
                Ok(())
            }
            SupervisorRecoveryDisposition::Lost => {
                self.commit_observed_control_terminal(
                    attempt,
                    AttemptState::Lost,
                    "SUPERVISOR_EVIDENCE_LOST",
                    None,
                    Some(&observation),
                )?;
                Ok(())
            }
            SupervisorRecoveryDisposition::Orphaned(reason) => {
                self.commit_observed_control_terminal(
                    attempt,
                    AttemptState::Orphaned,
                    "SUPERVISOR_IDENTITY_ORPHANED",
                    Some(reason),
                    Some(&observation),
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
        self.commit_observed_control_terminal(attempt, state, reason_code, detail, None)
    }

    fn commit_observed_control_terminal(
        &self,
        attempt: &AttemptRecord,
        state: AttemptState,
        reason_code: &str,
        detail: Option<String>,
        observed_supervisor: Option<&SupervisorObservation>,
    ) -> RuntimeResult<TaskObservation> {
        let control_terminal_guard = self.lock_control_terminal()?;
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
            detail: detail.clone(),
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
        let mut terminal = TerminalCommit {
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
        };
        append_terminal_evidence_for_commit_with_observation(
            &self.registry,
            &current,
            &mut terminal,
            observed_supervisor,
        )?;
        let _ = self.registry.commit_terminal(&terminal)?;
        drop(control_terminal_guard);
        if state != AttemptState::Orphaned {
            self.release_attempt_supervisor(&current)?;
            self.cleanup_payload_view(&current.attempt_id)?;
        }
        self.observation_from_registry(&current.job_id, 4096, 4096)
    }

    pub(crate) fn append_terminal_evidence(
        &self,
        attempt: &AttemptRecord,
        terminal: &mut TerminalCommit,
    ) -> RuntimeResult<()> {
        append_terminal_evidence_for_commit(&self.registry, attempt, terminal)
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
        let cancel_plan = self.registry.execution_plan(&request.job_id)?;
        let native_direct = cancel_plan.execution_target == super::ExecutionTarget::WindowsNative
            && self
                .windows
                .as_ref()
                .is_some_and(|windows| windows.wsl_distribution.is_none());
        match self.reconcile_job(&request.job_id) {
            Ok(()) => {}
            Err(error)
                if native_direct && error.code == RuntimeErrorCode::LaunchIdentityMismatch => {}
            Err(error) => return Err(error),
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
        if native_direct {
            return self.cancel_native_windows_attempt(&request.job_id, &attempt);
        }
        let output = Command::new("systemctl")
            .args(["--no-block", "stop", &attempt.unit_name])
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

    fn cancel_native_windows_attempt(
        &self,
        job_id: &str,
        attempt: &AttemptRecord,
    ) -> RuntimeResult<TaskObservation> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut poll_index = 0;
        loop {
            let current = self.registry.get_attempt(&attempt.attempt_id)?;
            if Path::new(&current.bundle_path).join(RESULT_FILE).exists() {
                return self.commit_runner_result(&current);
            }
            if self
                .registry
                .attempt_supervisor_owner(&current.attempt_id)?
                .is_some()
            {
                if !self.orphan_process_tree_alive(&current)? {
                    if Path::new(&current.bundle_path).join(RESULT_FILE).exists() {
                        return self.commit_runner_result(&current);
                    }
                    return self.commit_control_terminal(
                        &current,
                        AttemptState::Cancelled,
                        "STOP_REQUESTED_PROCESS_TREE_GONE",
                        Some(
                            "native Windows launcher owner identity is gone after committed cancel intent; JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE makes the Attempt process tree definitively non-running"
                                .to_string(),
                        ),
                    );
                }
            } else {
                let target_start = Path::new(&current.bundle_path).join(WINDOWS_START_FILE);
                if target_start.is_file() {
                    let _ =
                        self.bind_attempt_start(&current, super::ExecutionTarget::WindowsNative)?;
                    continue;
                }
                let launcher_start =
                    Path::new(&current.bundle_path).join(WINDOWS_LAUNCHER_START_FILE);
                if launcher_start.is_file() {
                    let (evidence, _) = self.validate_windows_launcher_start_evidence(&current)?;
                    let windows = self.windows.as_ref().ok_or_else(|| {
                        RuntimeError::new(
                            RuntimeErrorCode::RegistryCorrupt,
                            "native Windows cancel has no configured provider",
                            Some("executionTarget"),
                            true,
                        )
                    })?;
                    let observed =
                        observe_windows_launcher_owner(windows, evidence.launcher_process_id)?;
                    if target_start.is_file() {
                        continue;
                    }
                    let original_launcher_alive = observed.process_alive
                        && observed.process_creation_time_file_time
                            == Some(evidence.launcher_process_creation_time_file_time);
                    if !original_launcher_alive {
                        if target_start.is_file() {
                            continue;
                        }
                        return self.commit_control_terminal(
                            &current,
                            AttemptState::Cancelled,
                            "STOP_REQUESTED_WINDOWS_LAUNCHER_PRESTART_GONE",
                            Some(
                                "committed cancel intent outlived the provisional native launcher before target-start evidence; the suspended target could not have executed"
                                    .to_string(),
                            ),
                        );
                    }
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep_until_poll(deadline, &mut poll_index);
        }
        match self.reconcile_attempt(&attempt.attempt_id) {
            Ok(()) => {}
            Err(error) if error.code == RuntimeErrorCode::LaunchIdentityMismatch => {}
            Err(error) => return Err(error),
        }
        self.observation_from_registry(job_id, 4096, 4096)
    }

    pub fn list_jobs(
        &self,
        request: &RuntimeJobListRequest,
    ) -> RuntimeResult<RuntimeJobListResult> {
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

    fn release_attempt_supervisor(&self, attempt: &AttemptRecord) -> RuntimeResult<()> {
        let plan = self.registry.execution_plan(&attempt.job_id)?;
        if plan.execution_target == super::ExecutionTarget::WindowsNative
            && self
                .windows
                .as_ref()
                .is_some_and(|windows| windows.wsl_distribution.is_none())
        {
            return Ok(());
        }
        if self
            .registry
            .attempt_supervisor_owner(&attempt.attempt_id)?
            .is_none()
        {
            release_terminal_unit(&attempt.unit_name);
        }
        Ok(())
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

fn configured_execution_path() -> RuntimeResult<String> {
    let value =
        std::env::var("ORDIVON_EXEC_PATH").unwrap_or_else(|_| DEFAULT_EXECUTION_PATH.to_string());
    if value.is_empty()
        || value.as_bytes().contains(&0)
        || crate::universal::validate_env(&BTreeMap::from([("PATH".to_string(), value.clone())]))
            .is_err()
        || value
            .split(':')
            .any(|entry| entry.is_empty() || !Path::new(entry).is_absolute())
    {
        return Err(RuntimeError::invalid(
            "ORDIVON_EXEC_PATH must fit the Linux execve per-string boundary and contain only absolute paths",
            "ORDIVON_EXEC_PATH",
        ));
    }
    Ok(value)
}

fn configured_execution_home() -> RuntimeResult<String> {
    let value = std::env::var("ORDIVON_EXEC_HOME")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| DEFAULT_EXECUTION_HOME.to_string());
    if value.is_empty()
        || value.as_bytes().contains(&0)
        || crate::universal::validate_env(&BTreeMap::from([("HOME".to_string(), value.clone())]))
            .is_err()
        || !Path::new(&value).is_absolute()
    {
        return Err(RuntimeError::invalid(
            "ORDIVON_EXEC_HOME must fit the Linux execve per-string boundary and be an absolute path",
            "ORDIVON_EXEC_HOME",
        ));
    }
    Ok(value)
}

fn merge_environment(
    base: &BTreeMap<String, String>,
    explicit: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = base.clone();
    merged.extend(
        explicit
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    merged
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

pub(crate) fn append_terminal_evidence_for_commit(
    registry: &Registry,
    attempt: &AttemptRecord,
    terminal: &mut TerminalCommit,
) -> RuntimeResult<()> {
    append_terminal_evidence_for_commit_with_observation(registry, attempt, terminal, None)
}

fn append_terminal_evidence_for_commit_with_observation(
    registry: &Registry,
    attempt: &AttemptRecord,
    terminal: &mut TerminalCommit,
    observed_supervisor: Option<&SupervisorObservation>,
) -> RuntimeResult<()> {
    let job = registry.get_job(&attempt.job_id)?;
    let plan: RuntimeExecutionPlan =
        serde_json::from_str(&job.execution_plan_json).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("stored execution plan is invalid: {error}"),
                Some("executionPlan"),
                false,
            )
        })?;
    let previous_terminal_evidence = registry
        .list_artifacts(&attempt.job_id)?
        .into_iter()
        .rfind(|artifact| artifact.kind == "terminal_evidence")
        .map(|artifact| artifact.artifact_id);
    let (process_tree_disposition, process_tree_detail) = observe_terminal_process_tree(attempt);
    let cancellation_disposition = match attempt.termination_intent {
        AttemptTerminationIntent::Natural if job.desired_state == JobDesiredState::Cancelled => {
            "requested"
        }
        AttemptTerminationIntent::Natural => "not_requested",
        AttemptTerminationIntent::StopRequested => "requested",
        AttemptTerminationIntent::DeadlineExceeded => "deadline_exceeded",
    };
    let delivery_disposition = match terminal.state {
        AttemptState::Orphaned => "reconciliation_required",
        AttemptState::Lost => "unknown",
        _ => "committed",
    };
    let host_dependencies = registry.host_dependencies(&attempt.job_id)?;
    let host_dependency_continuity = if host_dependencies.is_empty() {
        None
    } else {
        match terminal.reason_code.as_str() {
            "HOST_DEPENDENCY_RUNTIME_DRIFT" => Some("runtime_path_drift_detected".to_string()),
            "PROCESS_EXIT_ZERO"
            | "PROCESS_EXIT_NONZERO"
            | "PROCESS_COMPLETED_BEFORE_STOP_EFFECTIVE"
            | "DEADLINE_EXCEEDED" => Some("no_runtime_path_drift_observed".to_string()),
            _ => None,
        }
    };
    let host_dependency_continuity_scope = host_dependency_continuity
        .as_ref()
        .map(|_| HOST_DEPENDENCY_CONTINUITY_SCOPE.to_string());
    let evidence = TerminalProcessEvidence {
        schema_version: RUNTIME_SCHEMA_VERSION,
        job_id: attempt.job_id.clone(),
        attempt_id: attempt.attempt_id.clone(),
        operation_digest: job.operation_digest,
        execution_plan_digest: job.execution_plan_digest,
        workspace_id: plan.workspace_id,
        source_revision: plan.source_revision,
        execution_profile: plan.execution_profile,
        execution_target: plan.execution_target,
        execution_provider: registry.execution_provider(&attempt.job_id)?,
        windows_authority: plan.windows_authority,
        windows_execution_context: plan.windows_execution_context,
        foreign_references: plan.foreign_references,
        host_dependencies,
        host_dependency_continuity,
        host_dependency_continuity_scope,
        input_set_id: plan.input_set_id,
        effective_inputs: plan.effective_inputs,
        executable: plan.executable,
        executable_digest: plan.executable_digest,
        args: plan.args,
        cwd: plan.cwd,
        supervisor: TerminalSupervisorEvidence {
            boot_id: attempt.boot_id.clone(),
            unit_name: attempt.unit_name.clone(),
            invocation_id: attempt.invocation_id.clone(),
            control_group: attempt.control_group.clone(),
            main_pid: attempt.main_pid,
            process_start_identity: attempt.process_start_identity.clone(),
            runner_start_digest: attempt.runner_start_digest.clone(),
        },
        observed_supervisor: observed_supervisor.map(ObservedSupervisorEvidence::from),
        start_disposition: if attempt.runner_start_digest.is_some() {
            "identity_bound".to_string()
        } else {
            "not_bound".to_string()
        },
        cancellation_disposition: cancellation_disposition.to_string(),
        execution_disposition: terminal.state.as_db().to_string(),
        delivery_disposition: delivery_disposition.to_string(),
        process_tree_disposition,
        process_tree_detail,
        reason_code: terminal.reason_code.clone(),
        terminal_artifact_ids: terminal
            .artifacts
            .iter()
            .map(|artifact| artifact.artifact_id.clone())
            .collect(),
        supersedes_artifact_id: previous_terminal_evidence,
        observed_at_ms: terminal.finished_at_ms,
    };
    let bytes = serde_json::to_vec_pretty(&evidence).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("cannot serialize terminal evidence: {error}"),
            Some("terminalEvidence"),
            false,
        )
    })?;
    let digest = sha256_bytes(&bytes);
    let digest_hex = digest.strip_prefix("sha256:").ok_or_else(|| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "terminal evidence digest has an invalid prefix",
            Some("terminalEvidence"),
            false,
        )
    })?;
    let file_name = format!("{TERMINAL_EVIDENCE_FILE_PREFIX}{digest_hex}.json");
    let path = Path::new(&attempt.bundle_path).join(&file_name);
    if path.is_file() {
        let observed = sha256_file(&path).map_err(map_universal_error)?;
        if observed != digest {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "content-addressed terminal evidence has conflicting bytes",
                Some("terminalEvidence"),
                false,
            ));
        }
    } else {
        write_bytes_atomic(&path, &bytes).map_err(map_universal_error)?;
    }
    terminal.artifacts.push(ArtifactRegistration {
        artifact_id: format!("{}.terminal-evidence.{digest_hex}", attempt.attempt_id),
        kind: "terminal_evidence".to_string(),
        relative_path: file_name,
        digest,
        media_type: "application/json".to_string(),
        byte_length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        truncated: false,
    });
    Ok(())
}

fn observe_terminal_process_tree(attempt: &AttemptRecord) -> (String, Option<String>) {
    if attempt.control_group.is_none()
        && attempt.main_pid.is_none()
        && attempt.invocation_id.is_none()
    {
        return (
            "unknown".to_string(),
            Some("the Attempt never bound a supervisor process identity".to_string()),
        );
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut poll_index = 0;
    loop {
        match attempt_process_tree_alive(attempt) {
            Ok(false) => return ("terminal_clean".to_string(), None),
            Ok(true) if Instant::now() < deadline => {
                sleep_until_poll(deadline, &mut poll_index);
            }
            Ok(true) => {
                return (
                    "unexpected_residual".to_string(),
                    Some(
                        "the identity-bound unit, PID, or cgroup remained populated after the terminal result"
                            .to_string(),
                    ),
                )
            }
            Err(error) => {
                return (
                    "unknown".to_string(),
                    Some(format!(
                        "post-terminal process-tree observation failed: {}",
                        error.message
                    )),
                )
            }
        }
    }
}

fn attempt_process_tree_alive(attempt: &AttemptRecord) -> RuntimeResult<bool> {
    let properties = systemctl_show(&attempt.unit_name)?;
    let matching_unit_active = unit_is_active(&properties)
        && attempt
            .invocation_id
            .as_deref()
            .zip(properties.get("InvocationID").map(String::as_str))
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

fn workspace_record_page(
    records: Vec<crate::universal::WorkspaceRecord>,
    limit: u32,
    cursor: Option<&super::RuntimeWorkspaceListCursor>,
) -> (
    Vec<crate::universal::WorkspaceRecord>,
    Option<super::RuntimeWorkspaceListCursor>,
) {
    let mut page = records
        .into_iter()
        .filter(|record| {
            let Some(cursor) = cursor else {
                return true;
            };
            record.created_unix_ms < u128::from(cursor.created_at_ms)
                || (record.created_unix_ms == u128::from(cursor.created_at_ms)
                    && record.workspace_id > cursor.workspace_id)
        })
        .take(limit as usize + 1)
        .collect::<Vec<_>>();
    if page.len() <= limit as usize {
        return (page, None);
    }
    page.truncate(limit as usize);
    let last = page.last().expect("non-empty page after limit validation");
    let next_cursor = super::RuntimeWorkspaceListCursor {
        created_at_ms: u64::try_from(last.created_unix_ms).unwrap_or(u64::MAX),
        workspace_id: last.workspace_id.clone(),
    };
    (page, Some(next_cursor))
}

fn verify_published_bundle(
    final_path: &Path,
    request_bytes: &[u8],
    plan_bytes: &[u8],
    manifest_bytes: &[u8],
) -> RuntimeResult<()> {
    let metadata = fs::symlink_metadata(final_path)
        .map_err(|error| io_error("inspect published Attempt bundle", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "published Attempt bundle is not a non-symlink directory",
            Some("bundlePath"),
            false,
        ));
    }
    for (name, expected) in [
        (RUNNER_REQUEST_FILE, request_bytes),
        (PLAN_FILE, plan_bytes),
        (BUNDLE_MANIFEST_FILE, manifest_bytes),
    ] {
        let path = final_path.join(name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error("inspect published Attempt bundle file", error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "published Attempt bundle contains a non-regular file",
                Some("bundlePath"),
                false,
            ));
        }
        let observed = fs::read(&path)
            .map_err(|error| io_error("read published Attempt bundle file", error))?;
        if observed != expected {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "published Attempt bundle bytes do not match deterministic Attempt identity",
                Some("bundlePath"),
                false,
            ));
        }
    }
    Ok(())
}

fn canonical_input_binding_requests(
    inputs: &[InputBindingRequest],
) -> RuntimeResult<Vec<InputBindingRequest>> {
    if inputs.is_empty() {
        return Err(RuntimeError::invalid(
            "at least one immutable input is required",
            "inputs",
        ));
    }
    let mut canonical = Vec::with_capacity(inputs.len());
    let mut presentation_paths = BTreeSet::<PathBuf>::new();
    for (index, input) in inputs.iter().enumerate() {
        validate_input_authority_name(&input.authority, &format!("inputs[{index}].authority"))?;
        let object = validate_normal_relative_path(
            &input.relative_object,
            &format!("inputs[{index}].relativeObject"),
        )?;
        let presentation = validate_normal_relative_path(
            &input.presentation_relative_path,
            &format!("inputs[{index}].presentationRelativePath"),
        )?;
        validate_sha256_digest(
            &input.expected_digest,
            &format!("inputs[{index}].expectedDigest"),
        )?;
        for existing in &presentation_paths {
            if presentation == *existing
                || presentation.starts_with(existing)
                || existing.starts_with(&presentation)
            {
                return Err(RuntimeError::invalid(
                    "input presentation paths must not overlap as file/ancestor paths",
                    &format!("inputs[{index}].presentationRelativePath"),
                ));
            }
        }
        presentation_paths.insert(presentation.clone());
        canonical.push(InputBindingRequest {
            authority: input.authority.clone(),
            relative_object: object.to_string_lossy().into_owned(),
            expected_digest: input.expected_digest.to_ascii_lowercase(),
            presentation_relative_path: presentation.to_string_lossy().into_owned(),
        });
    }
    canonical.sort_by(|left, right| {
        (
            &left.presentation_relative_path,
            &left.authority,
            &left.relative_object,
            &left.expected_digest,
        )
            .cmp(&(
                &right.presentation_relative_path,
                &right.authority,
                &right.relative_object,
                &right.expected_digest,
            ))
    });
    Ok(canonical)
}

fn validate_input_authority_name(value: &str, field: &str) -> RuntimeResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(RuntimeError::invalid(
            "input authority name must use 1-128 ASCII alphanumeric/._- characters",
            field,
        ));
    }
    Ok(())
}

fn validate_normal_relative_path(value: &str, field: &str) -> RuntimeResult<PathBuf> {
    let path = Path::new(value);
    if value.is_empty() || value.as_bytes().contains(&0) || path.is_absolute() {
        return Err(RuntimeError::invalid(
            "path must be non-empty, relative, and NUL-free",
            field,
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => normalized.push(value),
            _ => {
                return Err(RuntimeError::invalid(
                    "path must contain only normal relative components",
                    field,
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(RuntimeError::invalid("path must not be empty", field));
    }
    Ok(normalized)
}

fn validate_sha256_digest(value: &str, field: &str) -> RuntimeResult<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(RuntimeError::invalid("digest must use sha256:<hex>", field));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RuntimeError::invalid(
            "digest must be 32-byte SHA-256 hex",
            field,
        ));
    }
    Ok(())
}

fn open_authority_file(root_file: &File, relative: &str, index: usize) -> RuntimeResult<File> {
    let relative_path =
        validate_normal_relative_path(relative, &format!("inputs[{index}].relativeObject"))?;
    open_regular_file_beneath(root_file, &relative_path, true).map_err(|error| {
        RuntimeError::invalid(
            format!("cannot resolve input object inside authority: {error}"),
            &format!("inputs[{index}].relativeObject"),
        )
    })
}

fn copy_input_and_digest(mut source: File, target: &Path) -> RuntimeResult<String> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)
        .map_err(|error| io_error("create materialized input", error))?;
    std::io::copy(&mut source, &mut output)
        .map_err(|error| io_error("copy input object", error))?;
    output
        .sync_all()
        .map_err(|error| io_error("sync materialized input", error))?;
    sha256_file(target).map_err(map_universal_error)
}

fn collect_materialized_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> RuntimeResult<()> {
    for entry in
        fs::read_dir(current).map_err(|error| io_error("scan materialized input set", error))?
    {
        let entry = entry.map_err(|error| io_error("read materialized input entry", error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error("inspect materialized input entry", error))?;
        if metadata.file_type().is_symlink() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceStateMismatch,
                "materialized input set contains a symlink",
                Some("effectiveInputs"),
                false,
            ));
        }
        if metadata.is_dir() {
            collect_materialized_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorCode::WorkspaceStateMismatch,
                    "materialized input escaped its set root",
                    Some("effectiveInputs"),
                    false,
                )
            })?;
            files.insert(relative.to_path_buf());
        } else {
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceStateMismatch,
                "materialized input set contains a non-file entry",
                Some("effectiveInputs"),
                false,
            ));
        }
    }
    Ok(())
}

fn verify_effective_input_set(
    root: &Path,
    inputs: &[InputBindingRequest],
) -> RuntimeResult<Vec<EffectiveInputBinding>> {
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| io_error("inspect materialized input set", error))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::WorkspaceStateMismatch,
            "materialized input set root is not a non-symlink directory",
            Some("effectiveInputs"),
            false,
        ));
    }
    let expected = inputs
        .iter()
        .map(|input| PathBuf::from(&input.presentation_relative_path))
        .collect::<BTreeSet<_>>();
    let mut observed = BTreeSet::new();
    collect_materialized_files(root, root, &mut observed)?;
    if observed != expected {
        return Err(RuntimeError::new(
            RuntimeErrorCode::WorkspaceStateMismatch,
            "materialized input set file inventory differs from the committed binding set",
            Some("effectiveInputs"),
            false,
        ));
    }
    let mut effective = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let path = root.join(&input.presentation_relative_path);
        let metadata =
            fs::metadata(&path).map_err(|error| io_error("inspect materialized input", error))?;
        let digest = sha256_file(&path).map_err(map_universal_error)?;
        if digest != input.expected_digest {
            let field = format!("effectiveInputs[{index}].digest");
            return Err(RuntimeError::new(
                RuntimeErrorCode::WorkspaceStateMismatch,
                format!(
                    "materialized input digest mismatch: expected {}, observed {digest}",
                    input.expected_digest
                ),
                Some(&field),
                false,
            ));
        }
        effective.push(EffectiveInputBinding {
            authority: input.authority.clone(),
            relative_object: input.relative_object.clone(),
            digest,
            byte_length: metadata.len(),
            presentation_relative_path: input.presentation_relative_path.clone(),
            access: InputAccessMode::ReadOnly,
        });
    }
    Ok(effective)
}

fn effective_input_requests_from_plan(
    plan: &RuntimeExecutionPlan,
) -> RuntimeResult<Vec<InputBindingRequest>> {
    canonical_input_binding_requests(
        &plan
            .effective_inputs
            .iter()
            .map(|input| InputBindingRequest {
                authority: input.authority.clone(),
                relative_object: input.relative_object.clone(),
                expected_digest: input.digest.clone(),
                presentation_relative_path: input.presentation_relative_path.clone(),
            })
            .collect::<Vec<_>>(),
    )
}

const MAX_RUNTIME_RELEASE_RECEIPT_BYTES: u64 = 1_048_576;

struct RuntimeReleaseReceiptProjection {
    disposition: RuntimeReleaseDisposition,
    terminal: bool,
    available: bool,
    digest: Option<String>,
    deployed_tool_count: Option<u32>,
    tool_catalog_digest: Option<String>,
    rollback_status: Option<String>,
    issue: Option<String>,
}

fn validate_runtime_release_request(request: &RuntimeReleaseRequest) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err(RuntimeError::invalid(
            "unsupported runtime schema version",
            "schemaVersion",
        ));
    }
    validate_client_request_id(&request.client_request_id, "clientRequestId")?;
    if request.expected_tool_count == 0 {
        return Err(RuntimeError::invalid(
            "Runtime Release expectedToolCount must be positive",
            "expectedToolCount",
        ));
    }
    if request.commit.len() != 40
        || !request
            .commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(RuntimeError::invalid(
            "Runtime Release commit must be exactly 40 lowercase hexadecimal characters",
            "commit",
        ));
    }
    let manifest = request
        .candidate_manifest_digest
        .strip_prefix("sha256:")
        .ok_or_else(|| {
            RuntimeError::invalid(
                "candidate manifest digest must use sha256",
                "candidateManifestDigest",
            )
        })?;
    if manifest.len() != 64
        || !manifest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(RuntimeError::invalid(
            "candidate manifest digest must contain 64 lowercase hexadecimal characters",
            "candidateManifestDigest",
        ));
    }
    Ok(())
}

fn validate_release_binding_matches_request(
    binding: &RuntimeReleaseEffectBinding,
    request: &RuntimeReleaseRequest,
) -> RuntimeResult<()> {
    let expected_request_digest = runtime_release_request_identity_digest(request)?;
    let expected_effect_id = runtime_release_effect_id(request);
    if binding.contract != RuntimeReleaseContract::RuntimeReleaseV1
        || binding.effect_id != expected_effect_id
        || binding.request_digest != expected_request_digest
        || binding.workspace_id != request.workspace_id
        || binding.commit != request.commit
        || binding.candidate_manifest_digest != request.candidate_manifest_digest
        || binding.expected_tool_count != request.expected_tool_count
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "stored Runtime Release side truth does not match the request identity",
            Some("runtimeReleaseEffect"),
            false,
        ));
    }
    Ok(())
}

fn release_receipt_json(path: &Path) -> Result<Option<(serde_json::Value, String)>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("RELEASE_RECEIPT_METADATA_UNAVAILABLE".to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("RELEASE_RECEIPT_FILE_UNSAFE".to_string());
    }
    if metadata.len() > MAX_RUNTIME_RELEASE_RECEIPT_BYTES {
        return Err("RELEASE_RECEIPT_FILE_TOO_LARGE".to_string());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| "RELEASE_RECEIPT_OPEN_FAILED".to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_RUNTIME_RELEASE_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "RELEASE_RECEIPT_READ_FAILED".to_string())?;
    if bytes.len() as u64 > MAX_RUNTIME_RELEASE_RECEIPT_BYTES {
        return Err("RELEASE_RECEIPT_FILE_TOO_LARGE".to_string());
    }
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|_| "RELEASE_RECEIPT_JSON_INVALID".to_string())?;
    if !value.is_object() {
        return Err("RELEASE_RECEIPT_JSON_INVALID".to_string());
    }
    Ok(Some((value, sha256_bytes(&bytes))))
}

fn release_effect_json_matches(
    value: &serde_json::Value,
    binding: &RuntimeReleaseEffectBinding,
) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("contract").and_then(|value| value.as_str()) == Some("runtime_release_v1")
        && object.get("effectId").and_then(|value| value.as_str())
            == Some(binding.effect_id.as_str())
        && object.get("requestDigest").and_then(|value| value.as_str())
            == Some(binding.request_digest.as_str())
        && object.get("commit").and_then(|value| value.as_str()) == Some(binding.commit.as_str())
        && object
            .get("candidateManifestDigest")
            .and_then(|value| value.as_str())
            == Some(binding.candidate_manifest_digest.as_str())
        && object
            .get("expectedToolCount")
            .and_then(|value| value.as_u64())
            == Some(u64::from(binding.expected_tool_count))
}

fn unresolved_release_projection(snapshot: &JobSnapshot) -> RuntimeReleaseReceiptProjection {
    let admitted = snapshot
        .attempt
        .as_ref()
        .is_some_and(|attempt| attempt.state == AttemptState::Accepted);
    RuntimeReleaseReceiptProjection {
        disposition: if admitted {
            RuntimeReleaseDisposition::Admitted
        } else {
            RuntimeReleaseDisposition::InProgress
        },
        terminal: false,
        available: false,
        digest: None,
        deployed_tool_count: None,
        tool_catalog_digest: None,
        rollback_status: None,
        issue: None,
    }
}

fn release_reconciliation_projection(issue: &str) -> RuntimeReleaseReceiptProjection {
    RuntimeReleaseReceiptProjection {
        disposition: RuntimeReleaseDisposition::ReconciliationRequired,
        terminal: false,
        available: false,
        digest: None,
        deployed_tool_count: None,
        tool_catalog_digest: None,
        rollback_status: None,
        issue: Some(issue.to_string()),
    }
}

fn inspect_runtime_release_receipt(
    binding: &RuntimeReleaseEffectBinding,
    snapshot: &JobSnapshot,
) -> RuntimeResult<RuntimeReleaseReceiptProjection> {
    let receipt = Path::new(&binding.receipt_path);
    let directory = match fs::symlink_metadata(receipt) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Ok(release_reconciliation_projection(
                "RELEASE_RECEIPT_DIRECTORY_UNAVAILABLE",
            ));
        }
    };
    let Some(directory) = directory else {
        return if snapshot.job.resolution.is_none() {
            Ok(unresolved_release_projection(snapshot))
        } else {
            Ok(release_reconciliation_projection(
                "RELEASE_RECEIPT_MISSING_AFTER_JOB_TERMINAL",
            ))
        };
    };
    if directory.file_type().is_symlink() || !directory.is_dir() {
        return Ok(release_reconciliation_projection(
            "RELEASE_RECEIPT_DIRECTORY_UNSAFE",
        ));
    }

    let effect_request = match release_receipt_json(&receipt.join("effect-request.json")) {
        Ok(Some((value, _))) => value,
        Ok(None) if snapshot.job.resolution.is_none() => {
            return Ok(unresolved_release_projection(snapshot))
        }
        Ok(None) => {
            return Ok(release_reconciliation_projection(
                "RELEASE_EFFECT_REQUEST_MISSING_AFTER_JOB_TERMINAL",
            ));
        }
        Err(issue) => return Ok(release_reconciliation_projection(&issue)),
    };
    if !release_effect_json_matches(&effect_request, binding) {
        return Ok(release_reconciliation_projection(
            "RELEASE_EFFECT_REQUEST_MISMATCH",
        ));
    }

    let result = match release_receipt_json(&receipt.join("result.json")) {
        Ok(Some(value)) => value,
        Ok(None) if snapshot.job.resolution.is_none() => {
            return Ok(unresolved_release_projection(snapshot))
        }
        Ok(None) => {
            return Ok(release_reconciliation_projection(
                "RELEASE_RESULT_MISSING_AFTER_JOB_TERMINAL",
            ));
        }
        Err(issue) => return Ok(release_reconciliation_projection(&issue)),
    };
    let (result, result_digest) = result;
    if result.get("commit").and_then(|value| value.as_str()) != Some(binding.commit.as_str())
        || !result
            .get("releaseEffect")
            .is_some_and(|value| release_effect_json_matches(value, binding))
    {
        return Ok(release_reconciliation_projection("RELEASE_RESULT_MISMATCH"));
    }

    let rollback = match release_receipt_json(&receipt.join("rollback-result.json")) {
        Ok(Some((value, _))) => value
            .get("status")
            .and_then(|status| status.as_str())
            .map(str::to_string),
        Ok(None) => None,
        Err(issue) => return Ok(release_reconciliation_projection(&issue)),
    };
    if rollback.as_deref() == Some("restored_previous") {
        return Ok(RuntimeReleaseReceiptProjection {
            disposition: RuntimeReleaseDisposition::RolledBack,
            terminal: true,
            available: true,
            digest: Some(result_digest),
            deployed_tool_count: result
                .pointer("/probe/toolCount")
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok()),
            tool_catalog_digest: result
                .pointer("/probe/toolCatalogDigest")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            rollback_status: rollback,
            issue: None,
        });
    }

    let status = result.get("status").and_then(|value| value.as_str());
    let deployed_tool_count = result
        .pointer("/probe/toolCount")
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok());
    let tool_catalog_digest = result
        .pointer("/probe/toolCatalogDigest")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    if status == Some("deployed") && deployed_tool_count != Some(binding.expected_tool_count) {
        return Ok(release_reconciliation_projection(
            "RELEASE_RESULT_TOOL_COUNT_MISMATCH",
        ));
    }
    let (disposition, terminal, issue) = match status {
        Some("deployed") => (RuntimeReleaseDisposition::Deployed, true, None),
        Some("not_committed") => (RuntimeReleaseDisposition::NotCommitted, true, None),
        Some("rolled_back") => (RuntimeReleaseDisposition::RolledBack, true, None),
        Some("rollback_failed") => (
            RuntimeReleaseDisposition::ReconciliationRequired,
            false,
            Some("RELEASE_ROLLBACK_FAILED".to_string()),
        ),
        Some("recovery_failed") => (
            RuntimeReleaseDisposition::ReconciliationRequired,
            false,
            Some("RELEASE_RECOVERY_FAILED".to_string()),
        ),
        _ => (
            RuntimeReleaseDisposition::ReconciliationRequired,
            false,
            Some("RELEASE_RESULT_STATUS_UNKNOWN".to_string()),
        ),
    };
    Ok(RuntimeReleaseReceiptProjection {
        disposition,
        terminal,
        available: true,
        digest: Some(result_digest),
        deployed_tool_count,
        tool_catalog_digest,
        rollback_status: rollback,
        issue,
    })
}

fn validate_run_request_structure(request: &TaskRunRequest) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err(RuntimeError::invalid(
            "unsupported runtime schema version",
            "schemaVersion",
        ));
    }
    validate_client_request_id(&request.client_request_id, "clientRequestId")?;
    for (value, field) in [
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
    validate_execution_budget(&request.execution.budget, "execution.budget")?;
    if request.execution.execution_target == super::ExecutionTarget::LocalLinux
        && request.execution.windows_authority != super::WindowsAuthority::Limited
    {
        return Err(RuntimeError::invalid(
            "windowsAuthority=elevated requires executionTarget=windows_native",
            "execution.windowsAuthority",
        ));
    }
    if request.execution.execution_target == super::ExecutionTarget::WindowsNative {
        if request.execution.execution_profile != super::ExecutionProfile::TrustedLocal {
            return Err(RuntimeError::invalid(
                "windows_native currently supports trusted_local only",
                "execution.executionProfile",
            ));
        }
        if !request.execution.steps.is_empty() {
            return Err(RuntimeError::invalid(
                "windows_native currently supports one command only",
                "execution.steps",
            ));
        }
    }
    crate::universal::validate_exec_payload(
        &request.execution.args,
        &request.execution.env,
        "execution",
    )
    .map_err(map_universal_error)?;
    let mut foreign_reference_keys = std::collections::BTreeSet::new();
    for (index, reference) in request.execution.foreign_references.iter().enumerate() {
        for (value, suffix) in [
            (&reference.namespace, "namespace"),
            (&reference.reference_type, "type"),
            (&reference.id, "id"),
        ] {
            validate_logical_id(
                value,
                &format!("execution.foreignReferences[{index}].{suffix}"),
            )?;
        }
        if let Some(generation) = &reference.generation {
            validate_logical_id(
                generation,
                &format!("execution.foreignReferences[{index}].generation"),
            )?;
        }
        if let Some(digest) = &reference.digest {
            validate_logical_id(
                digest,
                &format!("execution.foreignReferences[{index}].digest"),
            )?;
        }
        let key = (
            reference.namespace.as_str(),
            reference.reference_type.as_str(),
            reference.id.as_str(),
        );
        if !foreign_reference_keys.insert(key) {
            return Err(RuntimeError::invalid(
                "foreignReferences must be unique by namespace, type, and id",
                &format!("execution.foreignReferences[{index}]"),
            ));
        }
    }
    if request.execution.execution_profile == super::ExecutionProfile::ContainedLocal {
        validate_contained_environment(&request.execution.env, "execution.env")?;
    }
    let mut step_ids = std::collections::BTreeSet::new();
    for (index, step) in request.execution.steps.iter().enumerate() {
        validate_logical_id(&step.id, &format!("execution.steps[{index}].id"))?;
        if !step_ids.insert(&step.id) {
            return Err(RuntimeError::invalid(
                "step ids must be unique",
                &format!("execution.steps[{index}].id"),
            ));
        }
        if step.executable.is_empty()
            || !Path::new(&step.executable).is_absolute()
            || step.cwd_relative.is_empty()
            || Path::new(&step.cwd_relative).is_absolute()
            || step.cwd_relative.split('/').any(|part| part == "..")
        {
            return Err(RuntimeError::invalid(
                "step executable must be absolute and cwdRelative must be relative",
                &format!("execution.steps[{index}]"),
            ));
        }
        if step.timeout_ms == 0 {
            return Err(RuntimeError::invalid(
                "step timeoutMs must be positive",
                &format!("execution.steps[{index}].timeoutMs"),
            ));
        }
        crate::universal::validate_exec_payload(
            &step.args,
            &step.env,
            &format!("execution.steps[{index}]"),
        )
        .map_err(map_universal_error)?;
        if request.execution.execution_profile == super::ExecutionProfile::ContainedLocal {
            validate_contained_environment(&step.env, &format!("execution.steps[{index}].env"))?;
        }
    }
    Ok(())
}

fn validate_run_proposal_structure(proposal: &super::TaskRunProposal) -> RuntimeResult<()> {
    let validation_request = TaskRunRequest {
        schema_version: proposal.schema_version,
        client_request_id: proposal.client_request_id.clone(),
        principal: proposal.principal.clone(),
        global_limit: proposal.global_limit,
        execution: super::UniversalExecutionRequest {
            workspace_id: proposal.execution.workspace_id.clone(),
            executable: proposal.execution.executable.clone(),
            args: proposal.execution.args.clone(),
            cwd_relative: proposal.execution.cwd_relative.clone(),
            env: proposal.execution.env.clone(),
            timeout_ms: proposal.execution.timeout_ms.unwrap_or(1),
            stdout_limit_bytes: proposal.execution.stdout_limit_bytes.unwrap_or(1),
            stderr_limit_bytes: proposal.execution.stderr_limit_bytes.unwrap_or(1),
            steps: proposal
                .execution
                .steps
                .iter()
                .map(|step| super::UniversalExecutionStep {
                    id: step.id.clone(),
                    executable: step.executable.clone(),
                    args: step.args.clone(),
                    cwd_relative: step.cwd_relative.clone(),
                    env: step.env.clone(),
                    timeout_ms: step.timeout_ms.unwrap_or(1),
                    continue_on_error: step.continue_on_error,
                })
                .collect(),
            budget: proposal.execution.budget.clone(),
            execution_profile: proposal.execution.execution_profile,
            execution_target: proposal.execution.execution_target,
            windows_authority: proposal.execution.windows_authority,
            foreign_references: proposal.execution.foreign_references.clone(),
            host_dependencies: Vec::new(),
        },
        wait_ms: proposal.wait_ms,
        stdout_tail_bytes: proposal.stdout_tail_bytes,
        stderr_tail_bytes: proposal.stderr_tail_bytes,
    };
    validate_run_request_structure(&validation_request)
}

fn validate_new_admission_policy(
    request: &TaskRunRequest,
    max_runtime_ms: u64,
    max_output_bytes: u64,
) -> RuntimeResult<()> {
    if request.execution.timeout_ms > max_runtime_ms {
        return Err(RuntimeError::invalid(
            format!("timeoutMs exceeds configured maximum {max_runtime_ms}"),
            "execution.timeoutMs",
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
    Ok(())
}

fn effective_limits_from_plan_json(
    execution_plan_json: &str,
) -> RuntimeResult<super::EffectiveExecutionLimits> {
    let plan: RuntimeExecutionPlan =
        serde_json::from_str(execution_plan_json).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("stored execution plan is invalid: {error}"),
                Some("executionPlan"),
                false,
            )
        })?;
    Ok(super::EffectiveExecutionLimits {
        timeout_ms: plan.timeout_ms,
        stdout_limit_bytes: plan.stdout_limit_bytes,
        stderr_limit_bytes: plan.stderr_limit_bytes,
        step_timeouts: plan
            .steps
            .into_iter()
            .map(|step| super::EffectiveStepTimeout {
                id: step.id,
                timeout_ms: step.timeout_ms,
            })
            .collect(),
    })
}

fn validate_execution_budget(
    budget: &super::ExecutionBudget,
    field_prefix: &str,
) -> RuntimeResult<()> {
    if budget.memory_max_bytes == Some(0) {
        return Err(RuntimeError::invalid(
            "memoryMaxBytes must be positive",
            &format!("{field_prefix}.memoryMaxBytes"),
        ));
    }
    if budget.tasks_max == Some(0) {
        return Err(RuntimeError::invalid(
            "tasksMax must be positive",
            &format!("{field_prefix}.tasksMax"),
        ));
    }
    if budget.cpu_quota_percent == Some(0) {
        return Err(RuntimeError::invalid(
            "cpuQuotaPercent must be positive",
            &format!("{field_prefix}.cpuQuotaPercent"),
        ));
    }
    Ok(())
}

fn validate_contained_environment(
    environment: &BTreeMap<String, String>,
    field: &str,
) -> RuntimeResult<()> {
    if let Some(name) = CONTAINED_RUNTIME_ENVIRONMENT
        .iter()
        .find(|name| environment.contains_key(**name))
    {
        let environment_field = format!("{field}.{name}");
        return Err(RuntimeError::invalid(
            format!("{name} is owned by the contained-local Runtime profile"),
            &environment_field,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct TaskActivitySignature {
    status: String,
    stdout_bytes: u64,
    stderr_bytes: u64,
    progress_revision: u64,
}

fn task_activity_signature(
    attempt: Option<&AttemptRecord>,
    projection: &super::JobProjection,
) -> RuntimeResult<TaskActivitySignature> {
    let Some(attempt) = attempt else {
        return Ok(TaskActivitySignature {
            status: projection.status.clone(),
            stdout_bytes: 0,
            stderr_bytes: 0,
            progress_revision: 0,
        });
    };
    let bundle = Path::new(&attempt.bundle_path);
    let stdout_bytes = file_length_if_present(&bundle.join(STDOUT_FILE))?;
    let stderr_bytes = file_length_if_present(&bundle.join(STDERR_FILE))?;
    let progress_revision = load_runner_progress_if_present(attempt)?
        .map(|progress| progress.revision)
        .unwrap_or(0);
    Ok(TaskActivitySignature {
        status: projection.status.clone(),
        stdout_bytes,
        stderr_bytes,
        progress_revision,
    })
}

fn file_length_if_present(path: &Path) -> RuntimeResult<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(io_error(&format!("inspect {}", path.display()), error)),
    }
}

pub(crate) fn load_runner_progress_if_present(
    attempt: &AttemptRecord,
) -> RuntimeResult<Option<RunnerTaskProgress>> {
    let path = Path::new(&attempt.bundle_path).join(PROGRESS_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| io_error("read Runner progress", error))?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("invalid Runner progress: {error}"),
            Some("progress"),
            false,
        )
    })
}

pub(crate) fn latest_output_modified_ms(attempt: &AttemptRecord) -> RuntimeResult<Option<u64>> {
    let bundle = Path::new(&attempt.bundle_path);
    let mut latest = None;
    for path in [bundle.join(STDOUT_FILE), bundle.join(STDERR_FILE)] {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_error(&format!("inspect {}", path.display()), error)),
        };
        if metadata.len() == 0 {
            continue;
        }
        let modified = metadata
            .modified()
            .map_err(|error| io_error(&format!("read {} mtime", path.display()), error))?;
        let millis = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let millis = u64::try_from(millis).unwrap_or(u64::MAX);
        latest = Some(latest.map_or(millis, |value: u64| value.max(millis)));
    }
    Ok(latest)
}

fn load_control_error_summary_if_present(attempt: &AttemptRecord) -> RuntimeResult<Option<String>> {
    let path = Path::new(&attempt.bundle_path).join(CONTROL_RESULT_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| io_error("read control result", error))?;
    let evidence: ControlTerminalEvidence = serde_json::from_slice(&bytes).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("invalid control result: {error}"),
            Some("controlResult"),
            false,
        )
    })?;
    if evidence.job_id != attempt.job_id || evidence.attempt_id != attempt.attempt_id {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ResultIdentityConflict,
            "control result identity does not match Attempt",
            Some("controlResult"),
            false,
        ));
    }
    Ok(evidence.detail)
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
    let mut bytes = Vec::with_capacity(usize::try_from(max_bytes.min(length)).unwrap_or(0));
    file.take(max_bytes)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read output tail", error))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn native_windows_outer_deadline_due(
    execution_started_at_ms: u64,
    timeout_ms: u64,
    observed_at_ms: u64,
) -> bool {
    observed_at_ms
        >= execution_started_at_ms
            .saturating_add(timeout_ms)
            .saturating_add(WINDOWS_NATIVE_OUTER_DEADLINE_GRACE_MS)
}

fn windows_native_launcher_lineage_is_definite_failure(
    execution_target: super::ExecutionTarget,
    termination_intent: super::AttemptTerminationIntent,
    unit_state: SupervisorUnitState,
    recorded_pid_alive: bool,
) -> bool {
    execution_target == super::ExecutionTarget::WindowsNative
        && termination_intent == super::AttemptTerminationIntent::Natural
        && unit_state == SupervisorUnitState::NotFound
        && !recorded_pid_alive
}

fn wsl_backed_windows_live_unit_must_wait(
    execution_target: super::ExecutionTarget,
    wsl_distribution_configured: bool,
    unit_active: bool,
) -> bool {
    execution_target == super::ExecutionTarget::WindowsNative
        && wsl_distribution_configured
        && unit_active
}

#[cfg(test)]
mod windows_lineage_tests {
    use super::*;

    #[test]
    fn native_windows_outer_deadline_uses_durable_start_time_and_outer_grace() {
        assert!(!native_windows_outer_deadline_due(200, 1_000, 6_199));
        assert!(native_windows_outer_deadline_due(200, 1_000, 6_200));
        assert!(!native_windows_outer_deadline_due(
            100,
            u64::MAX,
            u64::MAX - 1
        ));
    }

    #[test]
    fn wsl_backed_windows_live_unit_remains_starting_without_target_evidence() {
        assert!(wsl_backed_windows_live_unit_must_wait(
            crate::runtime::ExecutionTarget::WindowsNative,
            true,
            true,
        ));
        assert!(!wsl_backed_windows_live_unit_must_wait(
            crate::runtime::ExecutionTarget::WindowsNative,
            true,
            false,
        ));
        assert!(!wsl_backed_windows_live_unit_must_wait(
            crate::runtime::ExecutionTarget::WindowsNative,
            false,
            true,
        ));
        assert!(!wsl_backed_windows_live_unit_must_wait(
            crate::runtime::ExecutionTarget::LocalLinux,
            true,
            true,
        ));
    }

    #[test]
    fn missing_windows_launcher_lineage_is_failed_only_for_natural_windows_execution() {
        assert!(windows_native_launcher_lineage_is_definite_failure(
            crate::runtime::ExecutionTarget::WindowsNative,
            crate::runtime::AttemptTerminationIntent::Natural,
            SupervisorUnitState::NotFound,
            false,
        ));
        assert!(!windows_native_launcher_lineage_is_definite_failure(
            crate::runtime::ExecutionTarget::LocalLinux,
            crate::runtime::AttemptTerminationIntent::Natural,
            SupervisorUnitState::NotFound,
            false,
        ));
        assert!(!windows_native_launcher_lineage_is_definite_failure(
            crate::runtime::ExecutionTarget::WindowsNative,
            crate::runtime::AttemptTerminationIntent::StopRequested,
            SupervisorUnitState::NotFound,
            false,
        ));
        assert!(!windows_native_launcher_lineage_is_definite_failure(
            crate::runtime::ExecutionTarget::WindowsNative,
            crate::runtime::AttemptTerminationIntent::DeadlineExceeded,
            SupervisorUnitState::NotFound,
            false,
        ));
        assert!(!windows_native_launcher_lineage_is_definite_failure(
            crate::runtime::ExecutionTarget::WindowsNative,
            crate::runtime::AttemptTerminationIntent::Natural,
            SupervisorUnitState::Running,
            false,
        ));
        assert!(!windows_native_launcher_lineage_is_definite_failure(
            crate::runtime::ExecutionTarget::WindowsNative,
            crate::runtime::AttemptTerminationIntent::Natural,
            SupervisorUnitState::NotFound,
            true,
        ));
    }
}

#[cfg(test)]
mod output_tail_tests {
    use super::*;

    #[test]
    fn tail_lossy_decode_preserves_valid_text_around_invalid_bytes() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-tail-test-{}-{}",
            std::process::id(),
            now_ms().unwrap()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("output.log");
        fs::write(&path, b"0123456789alpha\xffomega").unwrap();
        let observed = read_tail_text(&path, 11).unwrap();
        assert!(
            observed.contains("alpha"),
            "valid prefix was discarded: {observed:?}"
        );
        assert!(
            observed.contains("omega"),
            "valid suffix was discarded: {observed:?}"
        );
        fs::remove_dir_all(root).unwrap();
    }
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

fn workspace_issue(
    workspace_id: &str,
    stage: RuntimeWorkspaceIssueStage,
    error: RuntimeError,
) -> RuntimeWorkspaceIssue {
    RuntimeWorkspaceIssue {
        workspace_id: workspace_id.to_string(),
        stage,
        code: error.code.as_str().to_string(),
        message: error.message,
        retryable: error.retryable,
    }
}

pub(crate) fn map_universal_error(error: crate::UniversalExecError) -> RuntimeError {
    use crate::UniversalExecErrorCode as UniversalCode;

    let code = match error.code {
        UniversalCode::InvalidRequest => RuntimeErrorCode::InvalidRequest,
        UniversalCode::WorkspaceExists => RuntimeErrorCode::WorkspaceExists,
        UniversalCode::WorkspaceNotFound => RuntimeErrorCode::WorkspaceNotFound,
        UniversalCode::WorkspacePathNotFound => RuntimeErrorCode::WorkspacePathNotFound,
        UniversalCode::WorkspaceDirty => RuntimeErrorCode::WorkspaceDirty,
        UniversalCode::WorkspacePathDenied => RuntimeErrorCode::WorkspacePathDenied,
        UniversalCode::RevisionNotFound => RuntimeErrorCode::RevisionNotFound,
        UniversalCode::RevisionMismatch => RuntimeErrorCode::RevisionMismatch,
        UniversalCode::WorkspaceStateMismatch
        | UniversalCode::InputStateMismatch
        | UniversalCode::HostDependencyRuntimeDrift
        | UniversalCode::ExecutableRuntimeDrift => RuntimeErrorCode::WorkspaceStateMismatch,
        UniversalCode::WorkspaceMutationIncomplete => RuntimeErrorCode::ReconciliationRequired,
        UniversalCode::TaskExists => RuntimeErrorCode::IdempotencyConflict,
        UniversalCode::TaskNotFound => RuntimeErrorCode::JobNotFound,
        UniversalCode::TaskStartFailed => RuntimeErrorCode::ToolFailed,
        UniversalCode::TaskStateUnavailable => RuntimeErrorCode::ReconciliationRequired,
        UniversalCode::ArtifactNotFound => RuntimeErrorCode::ArtifactNotFound,
        UniversalCode::ArtifactNotUtf8 => RuntimeErrorCode::ArtifactNotUtf8,
        UniversalCode::OutputLimitExceeded => RuntimeErrorCode::OutputLimitExceeded,
        UniversalCode::ToolUnavailable => RuntimeErrorCode::ToolUnavailable,
        UniversalCode::ToolFailed => RuntimeErrorCode::ToolFailed,
        UniversalCode::IoError => RuntimeErrorCode::IoError,
        UniversalCode::WorkspaceCapacityExceeded => RuntimeErrorCode::WorkspaceCapacityExceeded,
        UniversalCode::MetadataCorrupt => RuntimeErrorCode::MetadataCorrupt,
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
    use crate::{
        ExecutionBudget, ExecutionProfile, ExecutionProposal, ExecutionStepProposal,
        TaskRunProposal, UniversalExecutionRequest,
    };
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
    fn concurrent_materialization_keeps_physical_prepared_state_job_scoped() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-input-publish-race-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let authority_root = root.join("authority");
        fs::create_dir_all(&authority_root).unwrap();
        let bytes = vec![b'F'; 2 * 1024 * 1024];
        fs::write(authority_root.join("fragment.bin"), &bytes).unwrap();
        let expected_digest = sha256_bytes(&bytes);
        let request = TaskRunRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: "request:input-publish-race".to_string(),
            principal: "principal:test".to_string(),
            global_limit: 4,
            execution: UniversalExecutionRequest {
                workspace_id: "workspace-input-publish-race".to_string(),
                executable: "/usr/bin/true".to_string(),
                args: Vec::new(),
                cwd_relative: ".".to_string(),
                env: BTreeMap::new(),
                timeout_ms: 5_000,
                stdout_limit_bytes: 4_096,
                stderr_limit_bytes: 4_096,
                steps: Vec::new(),
                budget: ExecutionBudget::default(),
                execution_profile: ExecutionProfile::ContainedLocal,
                execution_target: crate::runtime::ExecutionTarget::LocalLinux,
                windows_authority: crate::runtime::WindowsAuthority::Limited,
                foreign_references: Vec::new(),
                host_dependencies: Vec::new(),
            },
            wait_ms: 0,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
        };
        let inputs = canonical_input_binding_requests(&[InputBindingRequest {
            authority: "finance".to_string(),
            relative_object: "fragment.bin".to_string(),
            expected_digest: expected_digest.clone(),
            presentation_relative_path: "data/fragment.bin".to_string(),
        }])
        .unwrap();
        let identity =
            super::super::input_bound_request_identity_digest(&request, &inputs).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for index in 0..2 {
            let root = root.clone();
            let authority_root = authority_root.clone();
            let request = request.clone();
            let inputs = inputs.clone();
            let identity = identity.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                let runtime = Runtime::new_with_input_authorities(
                    RuntimeConfig {
                        registry: RegistryConfig {
                            db_path: root.join(format!("registry-{index}/registry.sqlite3")),
                            store_root: root.join(format!("registry-{index}")),
                            busy_timeout_ms: 5_000,
                        },
                        executor: UniversalExecutorConfig {
                            store_root: root.join("runtime"),
                            workspace_root: None,
                            workspace_uid: None,
                            workspace_gid: None,
                            runner_path: PathBuf::from("/usr/bin/true"),
                            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
                            max_runtime_ms: 60_000,
                            max_output_bytes: 1_048_576,
                        },
                        startup_grace_ms: 2_000,
                        windows: None,
                    },
                    vec![InputAuthority {
                        name: "finance".to_string(),
                        root: authority_root,
                    }],
                )
                .unwrap();
                barrier.wait();
                let job_id = format!("job-{}", Uuid::now_v7());
                runtime
                    .materialize_input_bindings(&request, &identity, &job_id, &inputs)
                    .unwrap()
            }));
        }
        let left = handles.remove(0).join().unwrap();
        let right = handles.remove(0).join().unwrap();
        assert_eq!(left.input_set_id, right.input_set_id);
        assert_eq!(left.effective_inputs, right.effective_inputs);
        assert_ne!(left.prepared_root, right.prepared_root);
        assert!(left.prepared_root.is_dir());
        assert!(right.prepared_root.is_dir());
        assert_eq!(left.effective_inputs[0].digest, expected_digest);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn materialized_input_verification_fails_closed_after_tamper() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-input-tamper-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/input.bin"), b"S0").unwrap();
        let request = InputBindingRequest {
            authority: "finance".to_string(),
            relative_object: "fragment.bin".to_string(),
            expected_digest: sha256_bytes(b"S0"),
            presentation_relative_path: "data/input.bin".to_string(),
        };
        let first = verify_effective_input_set(&root, std::slice::from_ref(&request)).unwrap();
        assert_eq!(first[0].byte_length, 2);
        assert_eq!(first[0].digest, sha256_bytes(b"S0"));

        fs::write(root.join("data/input.bin"), b"S1").unwrap();
        let error = verify_effective_input_set(&root, std::slice::from_ref(&request)).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::WorkspaceStateMismatch);
        assert!(error.message.contains("digest mismatch"));

        fs::write(root.join("data/input.bin"), b"S0").unwrap();
        fs::write(root.join("unexpected.bin"), b"extra").unwrap();
        let error = verify_effective_input_set(&root, &[request]).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::WorkspaceStateMismatch);
        assert!(error.message.contains("file inventory differs"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn contained_input_set_is_read_only_bound_at_fixed_runtime_path() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-contained-input-command-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = root.join("workspace");
        let bundle = root.join("bundle");
        let inputs = root.join("inputs");
        let cache = root.join("cache");
        for path in [&workspace, &bundle, &inputs, &cache] {
            fs::create_dir_all(path).unwrap();
        }
        let environment = BTreeMap::from([
            (
                "HOME".to_string(),
                cache.join("home").to_string_lossy().into_owned(),
            ),
            (
                "TMPDIR".to_string(),
                cache.join("tmp").to_string_lossy().into_owned(),
            ),
        ]);
        for value in environment.values() {
            fs::create_dir_all(value).unwrap();
        }
        let budget = ExecutionBudget::default();
        let contained = build_systemd_run_command(&SystemdRunSpec {
            unit_name: "ordivon-input-contained.service",
            runner: Path::new("/usr/bin/true"),
            bundle_path: &bundle,
            workspace_path: &workspace,
            workspace_git_common_dir: None,
            input_set_path: Some(&inputs),
            runtime_ceiling_ms: 10_000,
            budget: &budget,
            execution_profile: ExecutionProfile::ContainedLocal,
            environment: &environment,
        })
        .unwrap();
        let contained_args = contained
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(contained_args.contains(&format!(
            "BindReadOnlyPaths={}:{CONTAINED_INPUT_ROOT}",
            inputs.display()
        )));

        let trusted = build_systemd_run_command(&SystemdRunSpec {
            unit_name: "ordivon-input-trusted.service",
            runner: Path::new("/usr/bin/true"),
            bundle_path: &bundle,
            workspace_path: &workspace,
            workspace_git_common_dir: None,
            input_set_path: Some(&inputs),
            runtime_ceiling_ms: 10_000,
            budget: &budget,
            execution_profile: ExecutionProfile::TrustedLocal,
            environment: &BTreeMap::new(),
        })
        .unwrap();
        let trusted_args = trusted
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!trusted_args.contains(CONTAINED_INPUT_ROOT));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn observed_supervisor_evidence_preserves_reconciliation_facts() {
        let missing = SupervisorObservation {
            boot_id: "boot-a".to_string(),
            unit_state: SupervisorUnitState::NotFound,
            invocation_id: None,
            control_group: None,
            main_pid: None,
            main_process_start_identity: None,
            recorded_pid_alive: false,
            recorded_pid_start_identity: None,
            result: Some("success".to_string()),
            exec_main_code: Some(0),
            exec_main_status: Some(0),
        };
        let mut rebooted = missing.clone();
        rebooted.boot_id = "boot-b".to_string();

        let missing = serde_json::to_value(ObservedSupervisorEvidence::from(&missing)).unwrap();
        let rebooted = serde_json::to_value(ObservedSupervisorEvidence::from(&rebooted)).unwrap();
        assert_eq!(missing["unitState"], "not_found");
        assert_eq!(missing["recordedPidAlive"], false);
        assert_eq!(missing["result"], "success");
        assert_eq!(missing["execMainCode"], 0);
        assert_eq!(missing["execMainStatus"], 0);
        assert_eq!(missing["bootId"], "boot-a");
        assert_eq!(rebooted["bootId"], "boot-b");
        assert_ne!(missing, rebooted);
    }

    fn proposal_runtime(label: &str, max_runtime_ms: u64, max_output_bytes: u64) -> Runtime {
        let root = std::env::temp_dir().join(format!(
            "ordivon-proposal-resolution-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = root.join("store");
        let registry = super::RegistryConfig {
            db_path: store.join("registry.sqlite3"),
            store_root: store.clone(),
            busy_timeout_ms: 5_000,
        };
        Runtime::new(super::RuntimeConfig {
            registry,
            executor: UniversalExecutorConfig {
                store_root: root.join("runtime"),
                workspace_root: None,
                workspace_uid: None,
                workspace_gid: None,
                runner_path: PathBuf::from("/usr/bin/true"),
                allowed_executable_roots: vec![PathBuf::from("/")],
                max_runtime_ms,
                max_output_bytes,
            },
            startup_grace_ms: 2_000,
            windows: None,
        })
        .unwrap()
    }

    fn proposal(step_timeouts: &[Option<u64>]) -> TaskRunProposal {
        TaskRunProposal {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: "request:proposal-resolution".to_string(),
            principal: "principal:test".to_string(),
            global_limit: 4,
            execution: ExecutionProposal {
                workspace_id: "workspace:test".to_string(),
                executable: "/usr/bin/true".to_string(),
                args: Vec::new(),
                cwd_relative: ".".to_string(),
                env: BTreeMap::new(),
                timeout_ms: None,
                stdout_limit_bytes: None,
                stderr_limit_bytes: None,
                steps: step_timeouts
                    .iter()
                    .enumerate()
                    .map(|(index, timeout_ms)| ExecutionStepProposal {
                        id: format!("step-{index}"),
                        executable: "/usr/bin/true".to_string(),
                        args: Vec::new(),
                        cwd_relative: ".".to_string(),
                        env: BTreeMap::new(),
                        timeout_ms: *timeout_ms,
                        continue_on_error: false,
                    })
                    .collect(),
                budget: ExecutionBudget::default(),
                execution_profile: ExecutionProfile::TrustedLocal,
                execution_target: crate::runtime::ExecutionTarget::LocalLinux,
                windows_authority: crate::runtime::WindowsAuthority::Limited,
                foreign_references: Vec::new(),
                host_dependencies: Vec::new(),
            },
            wait_ms: 0,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
        }
    }

    #[test]
    fn proposal_resolution_only_fills_omitted_limits_and_preserves_explicit_constraints() {
        let runtime = proposal_runtime("limits", 10_000, 1_048_576);
        let omitted = proposal(&[]);
        let resolved = runtime.resolve_proposal(&omitted);
        assert_eq!(resolved.execution.timeout_ms, 10_000);
        assert_eq!(resolved.execution.stdout_limit_bytes, 1_048_576);
        assert_eq!(resolved.execution.stderr_limit_bytes, 1_048_576);

        let mut explicit = omitted;
        explicit.execution.timeout_ms = Some(2_000);
        explicit.execution.stdout_limit_bytes = Some(4_096);
        explicit.execution.stderr_limit_bytes = Some(8_192);
        let resolved = runtime.resolve_proposal(&explicit);
        assert_eq!(resolved.execution.timeout_ms, 2_000);
        assert_eq!(resolved.execution.stdout_limit_bytes, 4_096);
        assert_eq!(resolved.execution.stderr_limit_bytes, 8_192);

        explicit.execution.timeout_ms = Some(99_000);
        let resolved = runtime.resolve_proposal(&explicit);
        assert_eq!(resolved.execution.timeout_ms, 99_000);
        let error = validate_new_admission_policy(&resolved, 10_000, 1_048_576).unwrap_err();
        assert_eq!(error.field.as_deref(), Some("execution.timeoutMs"));
    }

    #[test]
    fn proposal_plan_uses_shared_overall_deadline_without_rewriting_explicit_step_limits() {
        let runtime = proposal_runtime("plan", 10_000, 1_048_576);
        let fully_explicit = proposal(&[Some(2_000), Some(3_000)]);
        let resolved = runtime.resolve_proposal(&fully_explicit);
        assert_eq!(resolved.execution.timeout_ms, 10_000);
        assert_eq!(
            resolved
                .execution
                .steps
                .iter()
                .map(|step| step.timeout_ms)
                .collect::<Vec<_>>(),
            vec![2_000, 3_000]
        );

        let mixed = proposal(&[Some(2_000), None]);
        let resolved = runtime.resolve_proposal(&mixed);
        assert_eq!(resolved.execution.timeout_ms, 10_000);
        assert_eq!(
            resolved
                .execution
                .steps
                .iter()
                .map(|step| step.timeout_ms)
                .collect::<Vec<_>>(),
            vec![2_000, 10_000]
        );

        let over_sum = proposal(&[Some(8_000), Some(8_000)]);
        let resolved = runtime.resolve_proposal(&over_sum);
        assert_eq!(resolved.execution.timeout_ms, 10_000);
        assert_eq!(
            resolved
                .execution
                .steps
                .iter()
                .map(|step| step.timeout_ms)
                .collect::<Vec<_>>(),
            vec![8_000, 8_000]
        );
        validate_run_request_structure(&resolved).unwrap();
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
                steps: Vec::new(),
                budget: crate::ExecutionBudget::default(),
                execution_profile: crate::runtime::ExecutionProfile::TrustedLocal,
                execution_target: crate::runtime::ExecutionTarget::LocalLinux,
                windows_authority: crate::runtime::WindowsAuthority::Limited,
                foreign_references: Vec::new(),
                host_dependencies: Vec::new(),
            },
            wait_ms: 0,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
        };
        validate_run_request_structure(&request).unwrap();
        let error = validate_new_admission_policy(&request, 60_000, 1_024).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
        assert_eq!(error.field.as_deref(), Some("execution.stdoutLimitBytes"));
    }

    #[test]
    fn contained_runtime_paths_cannot_be_overridden_by_request_or_step_environment() {
        let mut request = TaskRunRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: "request:contained-env".to_string(),
            principal: "principal:test".to_string(),
            global_limit: 1,
            execution: UniversalExecutionRequest {
                workspace_id: "workspace-contained-env".to_string(),
                executable: "/usr/bin/true".to_string(),
                args: Vec::new(),
                cwd_relative: ".".to_string(),
                env: BTreeMap::from([("CARGO_TARGET_DIR".to_string(), "/etc".to_string())]),
                timeout_ms: 1_000,
                stdout_limit_bytes: 1_024,
                stderr_limit_bytes: 1_024,
                steps: Vec::new(),
                budget: crate::ExecutionBudget::default(),
                execution_profile: crate::runtime::ExecutionProfile::ContainedLocal,
                execution_target: crate::runtime::ExecutionTarget::LocalLinux,
                windows_authority: crate::runtime::WindowsAuthority::Limited,
                foreign_references: Vec::new(),
                host_dependencies: Vec::new(),
            },
            wait_ms: 0,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
        };
        let error = validate_run_request_structure(&request).unwrap_err();
        assert_eq!(
            error.field.as_deref(),
            Some("execution.env.CARGO_TARGET_DIR")
        );

        request.execution.env.clear();
        request.execution.steps.push(crate::UniversalExecutionStep {
            id: "step".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: BTreeMap::from([("HOME".to_string(), "/etc".to_string())]),
            timeout_ms: 1_000,
            continue_on_error: false,
        });
        let error = validate_run_request_structure(&request).unwrap_err();
        assert_eq!(error.field.as_deref(), Some("execution.steps[0].env.HOME"));
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
            windows: None,
        })
        .unwrap();
        drop(runtime);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_builds_explicit_minimal_environment_and_external_cache_paths() {
        let root = std::env::temp_dir().join(format!(
            "ordivon-runtime-environment-{}-{}",
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
            windows: None,
        })
        .unwrap();
        let record = crate::universal::WorkspaceRecord {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-env".to_string(),
            source_repo: root.join("source-a").to_string_lossy().into_owned(),
            source_revision: "a".repeat(40),
            workspace_path: root
                .join("runtime/workspaces/workspace-env")
                .to_string_lossy()
                .into_owned(),
            created_unix_ms: 1,
        };
        let peer = crate::universal::WorkspaceRecord {
            workspace_id: "workspace-peer".to_string(),
            workspace_path: root
                .join("runtime/workspaces/workspace-peer")
                .to_string_lossy()
                .into_owned(),
            ..record.clone()
        };
        let other = crate::universal::WorkspaceRecord {
            workspace_id: "workspace-other".to_string(),
            source_repo: root.join("source-b").to_string_lossy().into_owned(),
            workspace_path: root
                .join("runtime/workspaces/workspace-other")
                .to_string_lossy()
                .into_owned(),
            ..record.clone()
        };
        let environment = runtime
            .execution_environment(&record, crate::runtime::ExecutionProfile::TrustedLocal)
            .unwrap();
        let peer_environment = runtime
            .execution_environment(&peer, crate::runtime::ExecutionProfile::TrustedLocal)
            .unwrap();
        let other_environment = runtime
            .execution_environment(&other, crate::runtime::ExecutionProfile::TrustedLocal)
            .unwrap();
        assert!(!runtime.inherit_host_environment());
        assert_eq!(
            environment.get("PATH").map(String::as_str),
            Some(runtime.execution_path.as_str())
        );
        assert_eq!(
            environment.get("HOME").map(String::as_str),
            Some(runtime.execution_home.as_str())
        );
        assert_eq!(
            environment.get("CARGO_TARGET_DIR").map(String::as_str),
            Some(TRUSTED_BUILD_TARGET_PRESENTATION)
        );
        assert_eq!(
            peer_environment.get("CARGO_TARGET_DIR"),
            environment.get("CARGO_TARGET_DIR")
        );
        assert_eq!(
            other_environment.get("CARGO_TARGET_DIR"),
            environment.get("CARGO_TARGET_DIR")
        );
        for workspace_id in ["workspace-env", "workspace-peer", "workspace-other"] {
            let backing = runtime
                .executor
                .workspace_build_cache_path(workspace_id)
                .join("cargo");
            assert!(backing.is_dir());
            assert_ne!(
                backing.to_string_lossy().as_ref(),
                TRUSTED_BUILD_TARGET_PRESENTATION
            );
        }
        for name in [
            "UV_CACHE_DIR",
            "PIP_CACHE_DIR",
            "npm_config_cache",
            "PNPM_HOME",
            "COREPACK_HOME",
            "BUN_INSTALL_CACHE_DIR",
            "GOMODCACHE",
            "GOCACHE",
        ] {
            assert_eq!(environment.get(name), peer_environment.get(name));
            assert!(Path::new(environment.get(name).unwrap())
                .starts_with(root.join("runtime/cache/shared")));
        }
        let workspace_root = root.join("runtime/workspaces/workspace-env");
        let cache_path = Path::new(environment.get("XDG_CACHE_HOME").unwrap());
        assert!(cache_path.starts_with(root.join("runtime/cache")));
        assert!(!cache_path.starts_with(&workspace_root));
        assert!(cache_path.is_dir());

        let trusted_tmp = Path::new(environment.get("TMPDIR").unwrap());
        let peer_tmp = Path::new(peer_environment.get("TMPDIR").unwrap());
        let other_tmp = Path::new(other_environment.get("TMPDIR").unwrap());
        assert!(trusted_tmp.to_string_lossy().starts_with("/tmp/ordivon-t/"));
        assert!(peer_tmp.to_string_lossy().starts_with("/tmp/ordivon-t/"));
        assert!(other_tmp.to_string_lossy().starts_with("/tmp/ordivon-t/"));
        assert_eq!(trusted_tmp.as_os_str().len(), 35);
        assert_eq!(peer_tmp.as_os_str().len(), 35);
        assert_eq!(other_tmp.as_os_str().len(), 35);
        assert_ne!(trusted_tmp, peer_tmp);
        assert_ne!(trusted_tmp, other_tmp);
        assert_eq!(
            fs::read_link(trusted_tmp).unwrap(),
            runtime.executor.workspace_tmp_path("workspace-env")
        );
        assert_eq!(
            fs::read_link(peer_tmp).unwrap(),
            runtime.executor.workspace_tmp_path("workspace-peer")
        );
        assert_eq!(
            fs::read_link(other_tmp).unwrap(),
            runtime.executor.workspace_tmp_path("workspace-other")
        );
        assert!(trusted_tmp.is_dir());
        let deep_store = UniversalExecutorConfig {
            store_root: PathBuf::from(format!("/{}", "deep/".repeat(80))),
            ..runtime.executor.clone()
        };
        let deep_tmp = deep_store.workspace_tmp_presentation_path("workspace-env");
        assert_eq!(deep_tmp.as_os_str().len(), 35);
        assert_ne!(deep_tmp, trusted_tmp);
        let sibling_store = UniversalExecutorConfig {
            store_root: root.join("runtime-sibling"),
            ..runtime.executor.clone()
        };
        assert_ne!(
            sibling_store.workspace_tmp_presentation_path("workspace-env"),
            trusted_tmp
        );

        let contained = runtime
            .execution_environment(&record, crate::runtime::ExecutionProfile::ContainedLocal)
            .unwrap();
        assert!(Path::new(contained.get("CARGO_TARGET_DIR").unwrap())
            .starts_with(root.join("runtime/cache/build/workspace-env")));
        assert!(Path::new(contained.get("UV_CACHE_DIR").unwrap())
            .starts_with(root.join("runtime/cache/workspaces/workspace-env/tooling")));
        assert_eq!(
            Path::new(contained.get("TMPDIR").unwrap()),
            runtime.executor.workspace_tmp_path("workspace-env")
        );

        fs::remove_file(other_tmp).unwrap();
        std::os::unix::fs::symlink(
            runtime.executor.workspace_tmp_path("workspace-env"),
            other_tmp,
        )
        .unwrap();
        let mismatch = runtime
            .execution_environment(&other, crate::runtime::ExecutionProfile::TrustedLocal)
            .unwrap_err();
        assert_eq!(mismatch.code, RuntimeErrorCode::WorkspaceStateMismatch);
        assert!(mismatch.message.contains("points at"));
        for path in [trusted_tmp, peer_tmp, other_tmp] {
            let _ = fs::remove_file(path);
        }
        drop(runtime);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trusted_command_keeps_only_process_ownership_and_lifecycle_properties() {
        let budget = crate::ExecutionBudget::default();
        let environment = BTreeMap::new();
        let command = build_systemd_run_command(&SystemdRunSpec {
            unit_name: "ordivon-test.service",
            runner: Path::new("/usr/bin/true"),
            bundle_path: Path::new("/var/lib/ordivon/attempts/attempt-test"),
            workspace_path: Path::new("/root/projects/ordivon-runtime"),
            workspace_git_common_dir: None,
            input_set_path: None,
            runtime_ceiling_ms: 10_000,
            budget: &budget,
            execution_profile: crate::runtime::ExecutionProfile::TrustedLocal,
            environment: &environment,
        })
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
        assert!(args.contains("CollectMode=inactive"));
        assert!(!args.split_whitespace().any(|value| value == "--collect"));
        assert!(args.contains("RuntimeMaxSec=10000ms"));
        assert!(valid_environment_name("GITHUB_TOKEN"));
        assert!(valid_environment_name("CARGO_BIN_EXE_ordivon_job_fixture"));
        assert!(!valid_environment_name(
            "CARGO_BIN_EXE_ordivon-runtime-job-fixture"
        ));
    }

    #[test]
    fn execution_budget_maps_to_systemd_resource_properties() {
        let budget = crate::ExecutionBudget {
            memory_max_bytes: Some(512 * 1024 * 1024),
            tasks_max: Some(64),
            cpu_quota_percent: Some(250),
        };
        let environment = BTreeMap::new();
        let command = build_systemd_run_command(&SystemdRunSpec {
            unit_name: "ordivon-budget.service",
            runner: Path::new("/usr/bin/true"),
            bundle_path: Path::new("/var/lib/ordivon/attempts/attempt-budget"),
            workspace_path: Path::new("/root/projects/ordivon-runtime"),
            workspace_git_common_dir: None,
            input_set_path: None,
            runtime_ceiling_ms: 10_000,
            budget: &budget,
            execution_profile: crate::runtime::ExecutionProfile::TrustedLocal,
            environment: &environment,
        })
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
    #[test]
    fn contained_command_is_explicitly_isolated_without_trusted_environment() {
        let root =
            std::env::temp_dir().join(format!("ordivon-contained-command-{}", std::process::id()));
        let workspace = root.join("workspace");
        let bundle = root.join("bundle");
        let cache = root.join("cache");
        for path in [&workspace, &bundle, &cache] {
            fs::create_dir_all(path).unwrap();
        }
        let environment = BTreeMap::from([
            (
                "HOME".to_string(),
                cache.join("home").to_string_lossy().into_owned(),
            ),
            (
                "TMPDIR".to_string(),
                cache.join("tmp").to_string_lossy().into_owned(),
            ),
        ]);
        for value in environment.values() {
            fs::create_dir_all(value).unwrap();
        }
        let budget = crate::ExecutionBudget::default();
        let command = build_systemd_run_command(&SystemdRunSpec {
            unit_name: "ordivon-contained-test.service",
            runner: Path::new("/usr/bin/true"),
            bundle_path: &bundle,
            workspace_path: &workspace,
            workspace_git_common_dir: None,
            input_set_path: None,
            runtime_ceiling_ms: 10_000,
            budget: &budget,
            execution_profile: crate::runtime::ExecutionProfile::ContainedLocal,
            environment: &environment,
        })
        .unwrap();
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let joined = args.join(" ");
        for required in [
            "ProtectSystem=strict",
            "ProtectHome=tmpfs",
            "PrivateNetwork=yes",
            "NoNewPrivileges=yes",
            "CapabilityBoundingSet=",
            "ProtectControlGroups=yes",
            "RestrictAddressFamilies=AF_UNIX",
            "TemporaryFileSystem=/run:ro",
            "TemporaryFileSystem=/var:ro",
        ] {
            assert!(joined.contains(required), "missing {required}");
        }
        assert!(joined.contains(&format!(
            "BindPaths={}:{}",
            workspace.display(),
            workspace.display()
        )));
        assert!(joined.contains("BindReadOnlyPaths=/usr/bin/true:/usr/bin/true"));
        assert!(!joined.contains("GITHUB_TOKEN"));
        assert!(!args
            .iter()
            .any(|arg| arg.starts_with("--setenv=GITHUB_TOKEN=")));
        assert!(joined.contains("--setenv=GIT_OPTIONAL_LOCKS=0"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn universal_error_mapping_preserves_agent_control_semantics() {
        let cases = [
            (
                crate::UniversalExecErrorCode::WorkspaceNotFound,
                RuntimeErrorCode::WorkspaceNotFound,
            ),
            (
                crate::UniversalExecErrorCode::RevisionMismatch,
                RuntimeErrorCode::RevisionMismatch,
            ),
            (
                crate::UniversalExecErrorCode::MetadataCorrupt,
                RuntimeErrorCode::MetadataCorrupt,
            ),
            (
                crate::UniversalExecErrorCode::WorkspaceMutationIncomplete,
                RuntimeErrorCode::ReconciliationRequired,
            ),
            (
                crate::UniversalExecErrorCode::TaskStateUnavailable,
                RuntimeErrorCode::ReconciliationRequired,
            ),
        ];
        for (source, expected) in cases {
            let mapped = map_universal_error(crate::UniversalExecError::new(
                source,
                "test",
                Some("field"),
                false,
            ));
            assert_eq!(mapped.code, expected);
        }
    }

    #[test]
    fn cgroup_events_population_is_recursive_and_fail_closed() {
        assert!(parse_cgroup_populated("populated 1\nfrozen 0\n").unwrap());
        assert!(!parse_cgroup_populated("populated 0\nfrozen 0\n").unwrap());
        for invalid in ["frozen 0\n", "populated 2\n", "populated 1 extra\n"] {
            let error = parse_cgroup_populated(invalid).unwrap_err();
            assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt);
        }
    }
}
