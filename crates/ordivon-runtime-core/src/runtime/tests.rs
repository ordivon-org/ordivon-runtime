use super::engine::native_windows_pre_target_evidence_gap;
use super::registry::{set_test_commit_fault, TestCommitFault, TestCommitPoint};
use super::repair::{AdminRepairAudit, AdminRepairOperation};
use super::supervisor::AttemptSupervisorOwner;
use super::*;
use crate::universal::{
    CapturedOutput, RunnerTaskResult, TaskTerminalStatus, UniversalExecutorConfig,
    WorkspaceCloseRequest, WorkspaceFilePatch, WorkspaceMutateRequest, WorkspaceMutation,
    WorkspaceMutationMode, WorkspacePatchRequest, WorkspaceTextEdit, WorkspaceTextPosition,
    WorkspaceTextRange, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

struct Sandbox {
    root: PathBuf,
    registry: Registry,
}

impl Sandbox {
    fn new(label: &str, busy_timeout_ms: u64) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ordivon-{label}-{}-{}",
            std::process::id(),
            Uuid::now_v7()
        ));
        let store = root.join("store");
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let registry = Registry::initialize(RegistryConfig {
            db_path: store.join("registry.sqlite3"),
            store_root: store,
            busy_timeout_ms,
        })
        .unwrap();
        Self { root, registry }
    }

    fn workspace(&self) -> PathBuf {
        self.root.join("workspace")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn file_digest(path: &Path) -> String {
    digest(&fs::read(path).unwrap())
}

fn runtime_config(sandbox: &Sandbox) -> RuntimeConfig {
    RuntimeConfig {
        registry: sandbox.registry.config().clone(),
        executor: UniversalExecutorConfig {
            store_root: sandbox.root.join("runtime"),
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
    }
}

fn doctor_config(sandbox: &Sandbox) -> RuntimeDoctorConfig {
    RuntimeDoctorConfig {
        db_path: sandbox.registry.config().db_path.clone(),
        store_root: sandbox.registry.config().store_root.clone(),
        busy_timeout_ms: 5_000,
    }
}

fn inspection_config(sandbox: &Sandbox) -> RuntimeInspectionConfig {
    RuntimeInspectionConfig {
        db_path: sandbox.registry.config().db_path.clone(),
        busy_timeout_ms: 5_000,
    }
}

fn write_completed_runner_result(attempt: &AttemptRecord, finished_at_ms: u128) {
    let bundle = Path::new(&attempt.bundle_path);
    fs::create_dir_all(bundle).unwrap();
    let stdout = b"DOCTOR_RESULT_OK
";
    let stderr = b"";
    fs::write(bundle.join("stdout.log"), stdout).unwrap();
    fs::write(bundle.join("stderr.log"), stderr).unwrap();
    let result = RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: attempt.attempt_id.clone(),
        job_id: Some(attempt.job_id.clone()),
        attempt_id: Some(attempt.attempt_id.clone()),
        launch_token_digest: Some(attempt.launch_token_digest.clone()),
        payload_uid: None,
        payload_gid: None,
        status: TaskTerminalStatus::Completed,
        exit_code: Some(0),
        timed_out: false,
        infrastructure_error_code: None,
        infrastructure_error: None,
        started_unix_ms: finished_at_ms.saturating_sub(1),
        finished_unix_ms: finished_at_ms,
        steps: Vec::new(),
        failed_step_id: None,
        failed_step_index: None,
        stdout: CapturedOutput {
            artifact_id: format!("{}.stdout", attempt.attempt_id),
            file_name: "stdout.log".to_string(),
            digest: digest(stdout),
            retained_bytes: stdout.len() as u64,
            dropped_bytes: 0,
            truncated: false,
        },
        stderr: CapturedOutput {
            artifact_id: format!("{}.stderr", attempt.attempt_id),
            file_name: "stderr.log".to_string(),
            digest: digest(stderr),
            retained_bytes: 0,
            dropped_bytes: 0,
            truncated: false,
        },
    };
    fs::write(
        bundle.join("result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
}

fn write_test_snapshot(sandbox: &Sandbox, name: &str) -> PathBuf {
    let snapshot = sandbox.root.join(format!("snapshot-{name}"));
    fs::create_dir_all(&snapshot).unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    let target = snapshot.join("registry.sqlite3");
    fs::copy(&sandbox.registry.config().db_path, &target).unwrap();
    let bytes = fs::read(&target).unwrap();
    fs::write(
        snapshot.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "createdAt": "test",
            "files": [{
                "path": "registry.sqlite3",
                "bytes": bytes.len(),
                "digest": digest(&bytes),
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    snapshot
}

fn request(sandbox: &Sandbox, client_request_id: &str, global_limit: u32) -> SubmitRequest {
    let executable = fs::canonicalize("/usr/bin/true").unwrap();
    SubmitRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: client_request_id.to_string(),
        request_identity_digest: None,
        execution_provider: None,
        runtime_release_effect: None,
        plan: RuntimeExecutionPlan {
            schema_version: RUNTIME_SCHEMA_VERSION,
            workspace_id: "workspace:test".to_string(),
            workspace_path: sandbox.workspace().to_string_lossy().into_owned(),
            source_revision: "test-revision".to_string(),
            workspace_source_digest: None,
            workspace_git_common_dir: None,
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: file_digest(&executable),
            args: Vec::new(),
            cwd: sandbox.workspace().to_string_lossy().into_owned(),
            env: Default::default(),
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            steps: Vec::new(),
            budget: crate::ExecutionBudget::default(),
            execution_profile: super::ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            windows_execution_context: None,
            foreign_references: Vec::new(),
            input_set_id: None,
            effective_inputs: Vec::new(),
            principal: "principal:test".to_string(),
        },
        global_limit,
        host_dependencies: Vec::new(),
    }
}

fn created(outcome: AdmissionOutcome) -> CreatedAdmission {
    match outcome {
        AdmissionOutcome::Created(created) => *created,
        AdmissionOutcome::Existing { .. } => panic!("expected newly created admission"),
    }
}

fn running_attempt_for_commit_fault(sandbox: &Sandbox, client_request_id: &str) -> AttemptRecord {
    let created = created(
        sandbox
            .registry
            .submit(&request(sandbox, client_request_id, 1))
            .unwrap(),
    );
    let attempt = sandbox
        .registry
        .mark_bundle_ready(&created.attempt.attempt_id, 0, &digest(b"bundle"), 10)
        .unwrap();
    let attempt = sandbox
        .registry
        .mark_dispatch_issued(&attempt.attempt_id, attempt.row_version, 11)
        .unwrap();
    sandbox
        .registry
        .bind_running(
            &attempt.attempt_id,
            attempt.row_version,
            &RunnerIdentity {
                boot_id: "boot:test".to_string(),
                unit_name: attempt.unit_name.clone(),
                invocation_id: format!("invocation:{client_request_id}"),
                control_group: "/system.slice/ordivon-test.service".to_string(),
                main_pid: 42,
                process_start_identity: format!("start:{client_request_id}"),
                runner_start_digest: digest(b"runner-start"),
                observed_at_ms: 12,
            },
        )
        .unwrap()
}

fn input_bound_submit_for_job(
    sandbox: &Sandbox,
    client_request_id: &str,
    bytes: &[u8],
) -> SubmitRequest {
    let mut submit = request(sandbox, client_request_id, 8);
    submit.plan.execution_profile = ExecutionProfile::ContainedLocal;
    submit.plan.input_set_id = Some("a".repeat(64));
    submit.plan.effective_inputs = vec![EffectiveInputBinding {
        authority: "finance-state".to_string(),
        relative_object: "input.bin".to_string(),
        digest: digest(bytes),
        byte_length: bytes.len() as u64,
        presentation_relative_path: "state/input.bin".to_string(),
        access: InputAccessMode::ReadOnly,
    }];
    submit
}

#[test]
fn committed_prepared_input_is_adopted_by_replacement_runtime_startup() {
    let sandbox = Sandbox::new("input-ownership-restart", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    let ids = runtime.registry().preallocate_admission_ids();
    let bytes = b"FROZEN-S0";
    let submit = input_bound_submit_for_job(&sandbox, "request:input-ownership-restart", bytes);
    let prepared_root = executor.input_materializations_root().join(&ids.job_id);
    fs::create_dir_all(prepared_root.join("state")).unwrap();
    fs::write(prepared_root.join("state/input.bin"), bytes).unwrap();
    let created = created(
        runtime
            .registry()
            .submit_preallocated(&submit, &ids)
            .unwrap(),
    );
    drop(runtime);

    let replacement = Runtime::new(runtime_config(&sandbox)).unwrap();
    let owned_root = executor.job_input_path(&created.job.job_id);
    assert!(!prepared_root.exists());
    assert_eq!(fs::read(owned_root.join("state/input.bin")).unwrap(), bytes);
    let plan = replacement
        .registry()
        .execution_plan(&created.job.job_id)
        .unwrap();
    assert_eq!(
        plan.input_set_id.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(plan.effective_inputs[0].digest, digest(bytes));
}

#[test]
fn concurrent_replacement_runtimes_converge_one_committed_input_adoption() {
    let sandbox = Sandbox::new("input-ownership-race", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config.clone()).unwrap();
    let ids = runtime.registry().preallocate_admission_ids();
    let bytes = b"FROZEN-RACE";
    let submit = input_bound_submit_for_job(&sandbox, "request:input-ownership-race", bytes);
    let prepared_root = executor.input_materializations_root().join(&ids.job_id);
    fs::create_dir_all(prepared_root.join("state")).unwrap();
    fs::write(prepared_root.join("state/input.bin"), bytes).unwrap();
    let created = created(
        runtime
            .registry()
            .submit_preallocated(&submit, &ids)
            .unwrap(),
    );
    drop(runtime);

    let barrier = Arc::new(Barrier::new(3));
    let launch = |config: RuntimeConfig| {
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            Runtime::new(config)
        })
    };
    let a = launch(config.clone());
    let b = launch(config);
    barrier.wait();
    let runtime_a = a.join().unwrap().unwrap();
    let runtime_b = b.join().unwrap().unwrap();
    let owned_root = executor.job_input_path(&created.job.job_id);
    assert!(!prepared_root.exists());
    assert_eq!(fs::read(owned_root.join("state/input.bin")).unwrap(), bytes);
    assert_eq!(
        runtime_a
            .registry()
            .get_job(&created.job.job_id)
            .unwrap()
            .job_id,
        runtime_b
            .registry()
            .get_job(&created.job.job_id)
            .unwrap()
            .job_id
    );
}

#[test]
fn unowned_prepared_input_is_retained_during_grace_then_collectable() {
    let sandbox = Sandbox::new("input-prepared-gc", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    let orphan = executor
        .input_materializations_root()
        .join(format!("job-{}", Uuid::now_v7()));
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("input.bin"), b"NO-OWNER").unwrap();

    runtime.reconcile_prepared_input_sets(u64::MAX).unwrap();
    assert!(orphan.exists());
    runtime.reconcile_prepared_input_sets(0).unwrap();
    assert!(!orphan.exists());
}

#[test]
fn staging_lease_prevents_live_cleanup_and_crash_release_allows_collection() {
    let sandbox = Sandbox::new("input-staging-lease", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    let job_id = format!("job-{}", Uuid::now_v7());
    let root = executor.input_materializations_root();
    let lease_path = root.join(format!(".{job_id}.lease"));
    let staging = root.join(format!(".{job_id}.staging-{}", Uuid::now_v7()));
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&lease_path)
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("partial.bin"), b"PARTIAL").unwrap();

    runtime.reconcile_prepared_input_sets(0).unwrap();
    assert!(lease_path.exists());
    assert!(staging.exists());
    drop(lease);
    runtime.reconcile_prepared_input_sets(0).unwrap();
    assert!(!lease_path.exists());
    assert!(!staging.exists());
}

#[test]
fn corrupt_committed_prepared_input_does_not_block_runtime_startup_and_fails_its_job() {
    let sandbox = Sandbox::new("input-corrupt-prepared-isolation", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    let ids = runtime.registry().preallocate_admission_ids();
    let submit =
        input_bound_submit_for_job(&sandbox, "request:input-corrupt-prepared", b"EXPECTED");
    let prepared_root = executor.input_materializations_root().join(&ids.job_id);
    fs::create_dir_all(prepared_root.join("state")).unwrap();
    fs::write(prepared_root.join("state/input.bin"), b"CORRUPT!").unwrap();
    let created = created(
        runtime
            .registry()
            .submit_preallocated(&submit, &ids)
            .unwrap(),
    );
    drop(runtime);

    let replacement = Runtime::new(runtime_config(&sandbox)).unwrap();
    assert!(prepared_root.exists());
    replacement.reconcile_all().unwrap();
    let attempt = replacement
        .registry()
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Failed);
    assert!(!executor.job_input_path(&created.job.job_id).exists());
}

#[test]
fn committed_job_owned_input_drift_fails_before_dispatch() {
    let sandbox = Sandbox::new("input-owned-drift", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    let ids = runtime.registry().preallocate_admission_ids();
    let submit = input_bound_submit_for_job(&sandbox, "request:input-owned-drift", b"EXPECTED");
    let owned_root = executor.job_input_path(&ids.job_id);
    fs::create_dir_all(owned_root.join("state")).unwrap();
    fs::write(owned_root.join("state/input.bin"), b"CORRUPT!").unwrap();
    let created = created(
        runtime
            .registry()
            .submit_preallocated(&submit, &ids)
            .unwrap(),
    );

    runtime.reconcile_all().unwrap();
    let attempt = runtime
        .registry()
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Failed);
    let observation = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id,
            wait_ms: 0,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(
        observation.execution_reason_code.as_deref(),
        Some("INPUT_PRECONDITION_DRIFT")
    );
}

#[test]
fn committed_input_loss_fails_closed_without_reopening_authority() {
    let sandbox = Sandbox::new("input-loss-terminal", 5_000);
    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    let ids = runtime.registry().preallocate_admission_ids();
    let submit = input_bound_submit_for_job(&sandbox, "request:input-loss-terminal", b"MISSING");
    let created = created(
        runtime
            .registry()
            .submit_preallocated(&submit, &ids)
            .unwrap(),
    );
    assert!(!executor
        .input_materializations_root()
        .join(&created.job.job_id)
        .exists());
    assert!(!executor.job_input_path(&created.job.job_id).exists());

    runtime.reconcile_all().unwrap();
    let attempt = runtime
        .registry()
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Failed);
    let observation = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id,
            wait_ms: 0,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observation.status, "failed");
    assert_eq!(
        observation.execution_reason_code.as_deref(),
        Some("INPUT_PRECONDITION_DRIFT")
    );
    assert!(observation
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == "terminal_evidence"));
}

fn git_output(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn run_git_command(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn durable_patch_fixture(
    label: &str,
    workspace_id: &str,
) -> (Sandbox, Runtime, UniversalExecutorConfig) {
    let sandbox = Sandbox::new(label, 5000);
    let source = sandbox.root.join("patch-source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("README.md"), "alpha\n").unwrap();
    fs::write(source.join("SECOND.md"), "beta\n").unwrap();
    run_git_command(&source, &["init", "-q"]);
    run_git_command(
        &source,
        &["config", "user.email", "runtime-tests@ordivon.local"],
    );
    run_git_command(&source, &["config", "user.name", "Ordivon Runtime Tests"]);
    run_git_command(&source, &["add", "."]);
    run_git_command(&source, &["commit", "-qm", "fixture"]);

    let config = runtime_config(&sandbox);
    let executor = config.executor.clone();
    let runtime = Runtime::new(config).unwrap();
    runtime
        .open_workspace(&crate::GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        })
        .unwrap();
    (sandbox, runtime, executor)
}

fn durable_patch_request(
    executor: &UniversalExecutorConfig,
    workspace_id: &str,
    client_request_id: &str,
    include_second: bool,
) -> DurableWorkspacePatchRequest {
    let workspace = executor.workspace_path(workspace_id);
    let mut files = vec![WorkspaceFilePatch {
        relative_path: "README.md".to_string(),
        expected_digest: Some(file_digest(&workspace.join("README.md"))),
        edits: vec![WorkspaceTextEdit {
            range: WorkspaceTextRange {
                start: WorkspaceTextPosition { line: 1, column: 0 },
                end: WorkspaceTextPosition { line: 1, column: 5 },
            },
            expected_text: "alpha".to_string(),
            replacement: "omega".to_string(),
        }],
    }];
    if include_second {
        files.push(WorkspaceFilePatch {
            relative_path: "SECOND.md".to_string(),
            expected_digest: Some(file_digest(&workspace.join("SECOND.md"))),
            edits: vec![WorkspaceTextEdit {
                range: WorkspaceTextRange {
                    start: WorkspaceTextPosition { line: 1, column: 0 },
                    end: WorkspaceTextPosition { line: 1, column: 4 },
                },
                expected_text: "beta".to_string(),
                replacement: "gamma".to_string(),
            }],
        });
    }
    DurableWorkspacePatchRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        principal: "principal:durable-patch-test".to_string(),
        client_request_id: client_request_id.to_string(),
        patch: WorkspacePatchRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            files,
            max_diff_bytes: 16 * 1024,
        },
    }
}

#[test]
fn durable_workspace_patch_replays_exact_receipt_and_conflicts_on_changed_request() {
    let workspace_id = "workspace-durable-patch-replay";
    let (_sandbox, runtime, executor) = durable_patch_fixture("durable-patch-replay", workspace_id);
    let request = durable_patch_request(
        &executor,
        workspace_id,
        "request:durable-patch:replay",
        false,
    );

    let first = runtime.patch_workspace_durable(&request).unwrap();
    assert!(!first.replayed);
    assert_eq!(
        fs::read_to_string(executor.workspace_path(workspace_id).join("README.md")).unwrap(),
        "omega\n"
    );

    let replay = runtime.patch_workspace_durable(&request).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.operation_id, first.operation_id);
    assert_eq!(replay.request_digest, first.request_digest);
    assert_eq!(replay.patch, first.patch);

    let mut changed = request.clone();
    changed.patch.files[0].edits[0].replacement = "other".to_string();
    let error = runtime.patch_workspace_durable(&changed).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);
    assert_eq!(error.field.as_deref(), Some("clientRequestId"));
}

#[test]
fn durable_workspace_patch_recovers_committed_files_after_receipt_loss() {
    let workspace_id = "workspace-durable-patch-receipt-loss";
    let (sandbox, runtime, executor) =
        durable_patch_fixture("durable-patch-receipt-loss", workspace_id);
    let request = durable_patch_request(
        &executor,
        workspace_id,
        "request:durable-patch:receipt-loss",
        false,
    );
    let first = runtime.patch_workspace_durable(&request).unwrap();

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE workspace_patch_operations SET state='prepared',result_json=NULL WHERE operation_id=?1",
            [&first.operation_id],
        )
        .unwrap();
    drop(connection);

    let recovered = runtime.patch_workspace_durable(&request).unwrap();
    assert!(recovered.replayed);
    assert_eq!(recovered.operation_id, first.operation_id);
    assert_eq!(recovered.patch, first.patch);
    let status = runtime
        .workspace_patch_status(&WorkspacePatchStatusRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            principal: request.principal.clone(),
            client_request_id: request.client_request_id.clone(),
        })
        .unwrap();
    assert_eq!(status.state, WorkspacePatchOperationState::Committed);
    assert_eq!(status.patch, Some(first.patch));
}

#[test]
fn durable_workspace_patch_marks_mixed_physical_state_unknown_without_replay() {
    let workspace_id = "workspace-durable-patch-mixed";
    let (sandbox, runtime, executor) = durable_patch_fixture("durable-patch-mixed", workspace_id);
    let request =
        durable_patch_request(&executor, workspace_id, "request:durable-patch:mixed", true);
    let first = runtime.patch_workspace_durable(&request).unwrap();

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE workspace_patch_operations SET state='prepared',result_json=NULL WHERE operation_id=?1",
            [&first.operation_id],
        )
        .unwrap();
    drop(connection);
    fs::write(
        executor.workspace_path(workspace_id).join("README.md"),
        "alpha\n",
    )
    .unwrap();

    let status = runtime
        .workspace_patch_status(&WorkspacePatchStatusRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            principal: request.principal.clone(),
            client_request_id: request.client_request_id.clone(),
        })
        .unwrap();
    assert_eq!(status.state, WorkspacePatchOperationState::Unknown);
    assert_eq!(status.patch, None);
    assert_eq!(
        fs::read_to_string(executor.workspace_path(workspace_id).join("README.md")).unwrap(),
        "alpha\n"
    );
    assert_eq!(
        fs::read_to_string(executor.workspace_path(workspace_id).join("SECOND.md")).unwrap(),
        "gamma\n"
    );

    let error = runtime.patch_workspace_durable(&request).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
}

#[test]
fn task_observe_reconciliation_is_scoped_to_the_requested_job() {
    let sandbox = Sandbox::new("observe-scope", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();

    let target = created(
        runtime
            .registry()
            .submit(&request(&sandbox, "request:observe-scope:target", 8))
            .unwrap(),
    );
    runtime
        .registry()
        .request_cancel(&target.job.job_id, 100)
        .unwrap();

    let mut unrelated_request = request(&sandbox, "request:observe-scope:other", 8);
    unrelated_request.plan.workspace_id = "workspace:observe-scope:other".to_string();
    let unrelated = created(runtime.registry().submit(&unrelated_request).unwrap());

    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: target.job.job_id.clone(),
            wait_ms: 0,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 0,
            stderr_tail_bytes: 0,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert!(observed.execution_terminal);
    assert_eq!(observed.operation_digest, target.job.operation_digest);
    assert_eq!(
        runtime
            .registry()
            .get_attempt(&unrelated.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Accepted
    );
}

#[test]
fn projection_and_workspace_guards_do_not_dispatch_accepted_jobs() {
    let workspace_id = "workspace-observation-contract";
    let (sandbox, runtime, executor) = durable_patch_fixture("observation-contract", workspace_id);

    let mut target_request = request(&sandbox, "request:observation-contract:target", 8);
    target_request.plan.workspace_id = workspace_id.to_string();
    target_request.plan.workspace_path = executor
        .workspace_path(workspace_id)
        .to_string_lossy()
        .into_owned();
    target_request.plan.cwd = target_request.plan.workspace_path.clone();
    let target = created(runtime.registry().submit(&target_request).unwrap());

    let mut unrelated_request = request(&sandbox, "request:observation-contract:other", 8);
    unrelated_request.plan.workspace_id = "workspace:observation-contract:other".to_string();
    let unrelated = created(runtime.registry().submit(&unrelated_request).unwrap());

    let assert_still_accepted = || {
        assert_eq!(
            runtime
                .registry()
                .get_attempt(&target.attempt.attempt_id)
                .unwrap()
                .state,
            AttemptState::Accepted
        );
        assert_eq!(
            runtime
                .registry()
                .get_attempt(&unrelated.attempt.attempt_id)
                .unwrap()
                .state,
            AttemptState::Accepted
        );
    };

    let workspace = runtime
        .get_workspace(&RuntimeWorkspaceGetRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
        })
        .unwrap();
    assert!(workspace.active_job_ids.contains(&target.job.job_id));
    assert_still_accepted();

    let listed_workspaces = runtime
        .list_workspaces(&RuntimeWorkspaceListRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            limit: 100,
            cursor: None,
            include_source_state_digest: false,
        })
        .unwrap();
    assert!(listed_workspaces
        .workspaces
        .iter()
        .any(|workspace| workspace.workspace_id == workspace_id));
    assert_still_accepted();

    let listed_jobs = runtime
        .list_jobs(&RuntimeJobListRequest {
            limit: 100,
            cursor: None,
            client_request_id: None,
            workspace_id: Some(workspace_id.to_string()),
        })
        .unwrap();
    assert_eq!(listed_jobs.jobs.len(), 1);
    assert_eq!(listed_jobs.jobs[0].job_id, target.job.job_id);
    assert_still_accepted();

    let inspected_job = runtime.inspect_job(&target.job.job_id, 16).unwrap();
    assert_eq!(inspected_job.job.job_id, target.job.job_id);
    assert_eq!(inspected_job.attempts.len(), 1);
    assert_eq!(inspected_job.attempts[0].state, AttemptState::Accepted);
    assert!(inspected_job
        .timeline
        .iter()
        .all(|event| event.detail.is_none()));
    assert_still_accepted();

    let mutate_error = runtime
        .mutate_workspace(&WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            mutations: vec![WorkspaceMutation {
                relative_path: "NEW.md".to_string(),
                mode: WorkspaceMutationMode::Write,
                content: "new\n".to_string(),
                expected_digest: None,
                expected_text: None,
            }],
        })
        .unwrap_err();
    assert_eq!(mutate_error.code, RuntimeErrorCode::WorkspaceBusy);
    assert_still_accepted();

    let patch_request = durable_patch_request(
        &executor,
        workspace_id,
        "request:observation-contract:patch",
        false,
    );
    let patch_error = runtime.patch_workspace(&patch_request.patch).unwrap_err();
    assert_eq!(patch_error.code, RuntimeErrorCode::WorkspaceBusy);
    assert_still_accepted();

    let close_error = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
            expected_source_state_digest: None,
        })
        .unwrap_err();
    assert_eq!(close_error.code, RuntimeErrorCode::WorkspaceBusy);
    assert_still_accepted();
}

#[test]
fn workspace_source_drift_is_persisted_as_precondition_failure() {
    let sandbox = Sandbox::new("source-drift-evidence", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:source-drift-evidence", 4))
            .unwrap(),
    );
    let bundle = Path::new(&created.attempt.bundle_path);
    fs::create_dir_all(bundle).unwrap();
    let stdout = b"";
    let stderr = b"";
    fs::write(bundle.join("stdout.log"), stdout).unwrap();
    fs::write(bundle.join("stderr.log"), stderr).unwrap();
    let result = RunnerTaskResult {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        task_id: created.attempt.attempt_id.clone(),
        job_id: Some(created.job.job_id.clone()),
        attempt_id: Some(created.attempt.attempt_id.clone()),
        launch_token_digest: Some(created.attempt.launch_token_digest.clone()),
        payload_uid: None,
        payload_gid: None,
        status: TaskTerminalStatus::Failed,
        exit_code: None,
        timed_out: false,
        infrastructure_error_code: Some("WORKSPACE_STATE_MISMATCH".to_string()),
        infrastructure_error: Some(
            "WorkspaceStateMismatch: Workspace source state changed after operation admission"
                .to_string(),
        ),
        started_unix_ms: u128::from(created.attempt.created_at_ms),
        finished_unix_ms: u128::from(created.attempt.created_at_ms + 1),
        steps: Vec::new(),
        failed_step_id: None,
        failed_step_index: None,
        stdout: CapturedOutput {
            artifact_id: format!("{}.stdout", created.attempt.attempt_id),
            file_name: "stdout.log".to_string(),
            digest: digest(stdout),
            retained_bytes: 0,
            dropped_bytes: 0,
            truncated: false,
        },
        stderr: CapturedOutput {
            artifact_id: format!("{}.stderr", created.attempt.attempt_id),
            file_name: "stderr.log".to_string(),
            digest: digest(stderr),
            retained_bytes: 0,
            dropped_bytes: 0,
            truncated: false,
        },
    };
    fs::write(
        bundle.join("result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();

    let terminal = super::evidence::prepare_runner_terminal_from_bundle(&created.attempt).unwrap();
    assert_eq!(terminal.state, AttemptState::Failed);
    assert_eq!(terminal.exit_code, None);
    assert_eq!(terminal.reason_code, "WORKSPACE_SOURCE_PRECONDITION_DRIFT");
    sandbox.registry.commit_terminal(&terminal).unwrap();

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let reason: String = connection
        .query_row(
            "SELECT reason_code FROM job_events WHERE job_id=?1 AND event_type='JOB_TERMINAL'",
            [&created.job.job_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(reason, "WORKSPACE_SOURCE_PRECONDITION_DRIFT");
}

#[test]
fn registry_initializes_with_private_permissions_and_valid_schema() {
    let sandbox = Sandbox::new("schema", 5000);
    let metadata = fs::metadata(&sandbox.registry.config().db_path).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    let store = fs::metadata(&sandbox.registry.config().store_root).unwrap();
    assert_eq!(store.permissions().mode() & 0o777, 0o700);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
}

#[test]
fn empty_execution_budget_preserves_legacy_plan_identity() {
    let sandbox = Sandbox::new("budget-legacy-identity", 5000);
    let submit = request(&sandbox, "request:budget-legacy", 4);
    let value = serde_json::to_value(&submit.plan).unwrap();
    assert!(value.get("budget").is_none());
    let decoded: RuntimeExecutionPlan = serde_json::from_value(value).unwrap();
    assert!(decoded.budget.is_empty());
}

#[test]
fn maintenance_scan_is_bounded_and_oldest_first() {
    let sandbox = Sandbox::new("maintenance-order", 5000);
    let mut created_attempts = Vec::new();
    for index in 0..3_u32 {
        let mut submit = request(&sandbox, &format!("request:maintenance:{index}"), 8);
        submit.plan.workspace_id = format!("workspace:maintenance:{index}");
        let admission = created(sandbox.registry.submit(&submit).unwrap());
        created_attempts.push(admission.attempt);
    }
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    for (index, attempt) in created_attempts.iter().enumerate() {
        connection
            .execute(
                "UPDATE attempts SET created_at_ms=?1 WHERE attempt_id=?2",
                rusqlite::params![100_u64 + index as u64, attempt.attempt_id],
            )
            .unwrap();
    }
    drop(connection);

    let attempts = sandbox
        .registry
        .list_maintenance_attempts_bounded(2)
        .unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].attempt_id, created_attempts[0].attempt_id);
    assert_eq!(attempts[1].attempt_id, created_attempts[1].attempt_id);
}

#[test]
fn maintenance_scan_prioritizes_recovery_over_older_running_work() {
    let sandbox = Sandbox::new("maintenance-priority", 5000);
    let mut old_request = request(&sandbox, "request:maintenance-priority-old", 4);
    old_request.plan.workspace_id = "workspace:maintenance-priority-old".to_string();
    let old = created(sandbox.registry.submit(&old_request).unwrap());

    let mut recovery_request = request(&sandbox, "request:maintenance-priority-recovery", 4);
    recovery_request.plan.workspace_id = "workspace:maintenance-priority-recovery".to_string();
    let recovery = created(sandbox.registry.submit(&recovery_request).unwrap());
    let failure = RuntimeError::new(
        RuntimeErrorCode::AttemptStateConflict,
        "prioritized recovery",
        Some("attemptId"),
        false,
    );
    sandbox
        .registry
        .record_reconciliation_failure(&recovery.attempt, &failure, 30)
        .unwrap();

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET created_at_ms=1 WHERE attempt_id=?1",
            [&old.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE attempts SET created_at_ms=2 WHERE attempt_id=?1",
            [&recovery.attempt.attempt_id],
        )
        .unwrap();
    drop(connection);

    let attempts = sandbox
        .registry
        .list_maintenance_attempts_bounded(1)
        .unwrap();
    assert_eq!(attempts[0].attempt_id, recovery.attempt.attempt_id);
}

#[test]
fn maintenance_batch_clears_stale_terminal_recovery_condition() {
    let sandbox = Sandbox::new("maintenance-stale-recovery", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:maintenance-stale-recovery", 4))
            .unwrap(),
    );
    let failure = RuntimeError::new(
        RuntimeErrorCode::AttemptStateConflict,
        "simulated stale observation",
        Some("attemptId"),
        false,
    );
    sandbox
        .registry
        .record_reconciliation_failure(&created.attempt, &failure, 20)
        .unwrap();
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Failed,
            result_digest: digest(b"maintenance-terminal"),
            exit_code: Some(1),
            infrastructure_error_digest: None,
            finished_at_ms: 21,
            artifacts: Vec::new(),
            reason_code: "CONTROL_FAILURE".to_string(),
        })
        .unwrap();
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(before.summary.status, "attention");
    assert_eq!(before.summary.recovery_required_attempts, 1);

    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let report = runtime.reconcile_maintenance_batch(8).unwrap();
    assert_eq!(report.inspected, 1);
    let after = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(after.summary.status, "healthy");
    assert_eq!(after.summary.recovery_required_attempts, 0);
    assert_eq!(
        runtime
            .registry()
            .get_attempt(&created.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Failed
    );
}

#[test]
fn execution_budget_is_validated_and_part_of_idempotent_identity() {
    let sandbox = Sandbox::new("execution-budget", 5000);
    let mut invalid = request(&sandbox, "request:budget-invalid", 4);
    invalid.plan.budget.memory_max_bytes = Some(0);
    let error = sandbox.registry.submit(&invalid).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("plan.budget.memoryMaxBytes"));

    let mut original = request(&sandbox, "request:budget-identity", 4);
    original.plan.budget.tasks_max = Some(64);
    created(sandbox.registry.submit(&original).unwrap());
    let mut changed = original;
    changed.plan.budget.tasks_max = Some(65);
    let error = sandbox.registry.submit(&changed).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);
}

#[test]
fn deployment_fence_blocks_only_new_admission_and_preserves_exact_replay() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    let sandbox = Sandbox::new("deployment-admission-fence", 5_000);
    let existing_request = request(&sandbox, "request:deployment-fence-existing", 4);
    let created = created(sandbox.registry.submit(&existing_request).unwrap());
    let fence = OpenOptions::new()
        .read(true)
        .write(true)
        .open(sandbox.registry.config().admission_fence_path())
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(fence.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );

    let replay = sandbox.registry.submit(&existing_request).unwrap();
    let replay_job = match replay {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => {
            panic!("exact replay created a second Job under deployment fence")
        }
    };
    assert_eq!(replay_job.job_id, created.job.job_id);

    let mut new_request = request(&sandbox, "request:deployment-fence-new", 4);
    new_request.plan.workspace_id = "workspace:deployment-fence-new".to_string();
    let blocked = sandbox.registry.submit(&new_request).unwrap_err();
    assert_eq!(blocked.code, RuntimeErrorCode::DeploymentInProgress);
    assert!(blocked.retryable);
    assert_eq!(blocked.retry_after_ms, Some(1_000));
    drop(fence);
    assert!(matches!(
        sandbox.registry.submit(&new_request).unwrap(),
        AdmissionOutcome::Created(_)
    ));
}

#[test]
fn client_request_id_law_is_unicode_scalar_bounded_trimmed_and_control_free() {
    let ascii_max = "a".repeat(CLIENT_REQUEST_ID_MAX_LENGTH);
    validate_client_request_id(&ascii_max, "clientRequestId").unwrap();

    let unicode_over_old_byte_bound = "🙂".repeat(100);
    assert!(unicode_over_old_byte_bound.len() > 256);
    assert_eq!(unicode_over_old_byte_bound.chars().count(), 100);
    validate_client_request_id(&unicode_over_old_byte_bound, "clientRequestId").unwrap();

    let too_many_scalars = "🙂".repeat(CLIENT_REQUEST_ID_MAX_LENGTH + 1);
    let error = validate_client_request_id(&too_many_scalars, "clientRequestId").unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("clientRequestId"));

    for value in [
        " leading",
        "trailing ",
        "\u{00a0}nonbreaking",
        "newline\ninside",
        "",
    ] {
        let error = validate_client_request_id(value, "clientRequestId").unwrap_err();
        assert_eq!(
            error.code,
            RuntimeErrorCode::InvalidRequest,
            "value={value:?}"
        );
    }
    validate_client_request_id("interior whitespace is fine", "clientRequestId").unwrap();
    validate_client_request_id("请求-🙂-e\u{301}", "clientRequestId").unwrap();
}

#[test]
fn admission_fence_io_failure_is_safe_same_request() {
    let sandbox = Sandbox::new("admission-fence-io", 5_000);
    let fence = sandbox.registry.config().admission_fence_path();
    fs::remove_file(&fence).unwrap();
    fs::create_dir(&fence).unwrap();
    let error = sandbox
        .registry
        .submit(&request(&sandbox, "request:admission-fence-io", 1))
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::RegistryUnavailable);
    assert!(error.retryable);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
}

#[test]
fn representation_cardinality_is_not_runtime_admission_policy() {
    let sandbox = Sandbox::new("representation-cardinality", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let steps = (0..33)
        .map(|index| UniversalExecutionStep {
            id: format!("step-{index}"),
            executable: "/usr/bin/true".to_string(),
            args: vec![format!("arg-{index}")],
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 1_000,
            continue_on_error: false,
        })
        .collect::<Vec<_>>();
    let foreign_references = (0..17)
        .map(|index| ForeignReference {
            namespace: "ordivon.test".to_string(),
            reference_type: "fixture".to_string(),
            id: format!("reference-{index}"),
            generation: None,
            digest: None,
        })
        .collect::<Vec<_>>();
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:large-representation".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace-does-not-exist".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: (0..129).map(|index| format!("arg-{index}")).collect(),
            cwd_relative: ".".to_string(),
            env: (0..65)
                .map(|index| (format!("KEY_{index}"), format!("value-{index}")))
                .collect(),
            timeout_ms: 33_000,
            stdout_limit_bytes: 1_024,
            stderr_limit_bytes: 1_024,
            steps,
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references,
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let error = runtime.run_task(&request).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::WorkspaceNotFound);
}

#[test]
fn operator_runtime_and_output_ceilings_are_enforced_before_admission() {
    let sandbox = Sandbox::new("operator-ceilings", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let base = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:operator-ceiling".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace-does-not-exist".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 60_000,
            stdout_limit_bytes: 1_048_576,
            stderr_limit_bytes: 1_048_576,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };

    let mut timeout = base.clone();
    timeout.execution.timeout_ms = 60_001;
    let error = runtime.run_task(&timeout).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("execution.timeoutMs"));

    let mut output = base;
    output.execution.stdout_limit_bytes = 1_048_577;
    let error = runtime.run_task(&output).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("execution.stdoutLimitBytes"));
}

#[test]
fn oversized_exec_string_is_rejected_before_admission() {
    let sandbox = Sandbox::new("exec-string-boundary", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:oversized-arg".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace-does-not-exist".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: vec!["x".repeat(128 * 1024)],
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 1_000,
            stdout_limit_bytes: 1_024,
            stderr_limit_bytes: 1_024,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let error = runtime.run_task(&request).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("args"));
}

#[test]
fn windows_execution_context_is_durable_plan_evidence_not_request_identity_input() {
    let context = super::WindowsExecutionContext {
        token_class: super::WindowsTokenClass::Limited,
        token_user_sid: "S-1-5-21-test-1001".to_string(),
        environment_source: "windows_user_machine_profile_allowlist_v1".to_string(),
    };
    let value = serde_json::to_value(&context).unwrap();
    assert_eq!(value["tokenClass"], "limited");
    assert_eq!(value["tokenUserSid"], "S-1-5-21-test-1001");
    assert_eq!(
        value["environmentSource"],
        "windows_user_machine_profile_allowlist_v1"
    );
    let decoded: super::WindowsExecutionContext = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, context);
}

#[test]
fn request_identity_excludes_observation_preferences_and_capacity_policy() {
    let base = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:identity-boundary".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 4,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace:test".to_string(),
            executable: "/usr/bin/../bin/true".to_string(),
            args: vec!["argument".to_string()],
            cwd_relative: "subdir/../subdir".to_string(),
            env: BTreeMap::from([("KEY".to_string(), "VALUE".to_string())]),
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: super::ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let digest = operation_request_identity_digest(&base).unwrap();
    assert_eq!(
        digest,
        "runtime-request-v1:sha256:588131daa80c66808139c86fada4fd1b07ed3b67b276b5da7b2ff0a0462bbc22"
    );

    let mut windows_native = base.clone();
    windows_native.execution.execution_target = super::ExecutionTarget::WindowsNative;
    assert_ne!(
        operation_request_identity_digest(&windows_native).unwrap(),
        digest,
        "execution target must be part of operation identity"
    );

    let windows_limited_digest = operation_request_identity_digest(&windows_native).unwrap();
    assert_eq!(
        windows_limited_digest,
        "runtime-request-v1:sha256:3c177dbeb4756a4db09a939de2f923415f6ca5a1ea03d50b7bf4f69c6dfe1078"
    );
    let mut windows_elevated = windows_native.clone();
    windows_elevated.execution.windows_authority = super::WindowsAuthority::Elevated;
    assert_ne!(
        operation_request_identity_digest(&windows_elevated).unwrap(),
        windows_limited_digest,
        "requested Windows authority must be part of operation identity"
    );

    let mut observation_only = base.clone();
    observation_only.global_limit = 99;
    observation_only.wait_ms = 30_000;
    observation_only.stdout_tail_bytes = 8_192;
    observation_only.stderr_tail_bytes = 8_192;
    assert_eq!(
        operation_request_identity_digest(&observation_only).unwrap(),
        digest
    );

    let mut normalized_paths = base.clone();
    normalized_paths.execution.executable = "/usr/bin/true".to_string();
    normalized_paths.execution.cwd_relative = "subdir".to_string();
    assert_eq!(
        operation_request_identity_digest(&normalized_paths).unwrap(),
        digest
    );

    let mut host_bound = base.clone();
    host_bound.execution.host_dependencies = vec![
        HostDependencyBinding {
            path: "/opt/runtime/b.so".to_string(),
            expected_digest: self::digest(b"b-v1"),
        },
        HostDependencyBinding {
            path: "/opt/runtime/a.so".to_string(),
            expected_digest: self::digest(b"a-v1"),
        },
    ];
    let host_bound_digest = operation_request_identity_digest(&host_bound).unwrap();
    assert_ne!(host_bound_digest, digest);
    host_bound.execution.host_dependencies.reverse();
    assert_eq!(
        operation_request_identity_digest(&host_bound).unwrap(),
        host_bound_digest
    );
    host_bound.execution.host_dependencies[0].expected_digest = self::digest(b"changed");
    assert_ne!(
        operation_request_identity_digest(&host_bound).unwrap(),
        host_bound_digest
    );

    let mut changed = base;
    changed.execution.args.push("different".to_string());
    assert_ne!(operation_request_identity_digest(&changed).unwrap(), digest);
}

#[test]
fn elevated_windows_authority_is_rejected_for_local_linux_before_admission() {
    let sandbox = Sandbox::new("windows-authority-local-linux", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:windows-authority-local-linux".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace-does-not-exist".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 1_000,
            stdout_limit_bytes: 1_024,
            stderr_limit_bytes: 1_024,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Elevated,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let error = runtime.run_task(&request).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("execution.windowsAuthority"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

fn input_bound_task_request(workspace_id: &str, client_request_id: &str) -> TaskRunRequest {
    TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: client_request_id.to_string(),
        principal: "principal:input-test".to_string(),
        global_limit: 4,
        execution: UniversalExecutionRequest {
            workspace_id: workspace_id.to_string(),
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
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    }
}

#[test]
fn input_bound_identity_is_order_independent_but_binding_sensitive() {
    let request = input_bound_task_request("workspace:test", "request:input-identity");
    let digest_a = digest(b"input-a");
    let digest_b = digest(b"input-b");
    let inputs = vec![
        InputBindingRequest {
            authority: "finance".to_string(),
            relative_object: "fragments/a.parquet".to_string(),
            expected_digest: digest_a.clone(),
            presentation_relative_path: "data/a.parquet".to_string(),
        },
        InputBindingRequest {
            authority: "finance".to_string(),
            relative_object: "fragments/b.parquet".to_string(),
            expected_digest: digest_b.clone(),
            presentation_relative_path: "data/b.parquet".to_string(),
        },
    ];
    let first = input_bound_request_identity_digest(&request, &inputs).unwrap();
    assert!(first.starts_with(INPUT_BOUND_IDENTITY_PREFIX));

    let reversed = vec![inputs[1].clone(), inputs[0].clone()];
    assert_eq!(
        input_bound_request_identity_digest(&request, &reversed).unwrap(),
        first
    );

    let mut changed_digest = inputs.clone();
    changed_digest[0].expected_digest = digest(b"different");
    assert_ne!(
        input_bound_request_identity_digest(&request, &changed_digest).unwrap(),
        first
    );

    let mut changed_presentation = inputs;
    changed_presentation[0].presentation_relative_path = "other/a.parquet".to_string();
    assert_ne!(
        input_bound_request_identity_digest(&request, &changed_presentation).unwrap(),
        first
    );
}

#[test]
fn input_bound_proposal_identity_preserves_proposal_and_binding_semantics() {
    let request = input_bound_task_request("workspace:test", "request:input-proposal-identity");
    let proposal = TaskRunProposal {
        schema_version: request.schema_version,
        client_request_id: request.client_request_id.clone(),
        principal: request.principal.clone(),
        global_limit: request.global_limit,
        execution: ExecutionProposal {
            workspace_id: request.execution.workspace_id.clone(),
            executable: request.execution.executable.clone(),
            args: request.execution.args.clone(),
            cwd_relative: request.execution.cwd_relative.clone(),
            env: request.execution.env.clone(),
            timeout_ms: None,
            stdout_limit_bytes: None,
            stderr_limit_bytes: None,
            steps: Vec::new(),
            budget: request.execution.budget.clone(),
            execution_profile: ExecutionProfile::ContainedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let inputs = vec![
        InputBindingRequest {
            authority: "finance-prepared".to_string(),
            relative_object: "bundle/manifest.json".to_string(),
            expected_digest: digest(b"manifest"),
            presentation_relative_path: "finance-lab/bundle/manifest.json".to_string(),
        },
        InputBindingRequest {
            authority: "finance-prepared".to_string(),
            relative_object: "bundle/cuts/rates.parquet".to_string(),
            expected_digest: digest(b"rates"),
            presentation_relative_path: "finance-lab/bundle/cuts/rates.parquet".to_string(),
        },
    ];
    let first = input_bound_proposal_request_identity_digest(&proposal, &inputs).unwrap();
    assert!(first.starts_with("runtime-request-input-v2:"));
    assert_eq!(
        input_bound_proposal_request_identity_digest(
            &proposal,
            &[inputs[1].clone(), inputs[0].clone()]
        )
        .unwrap(),
        first
    );

    let mut changed_input = inputs.clone();
    changed_input[0].expected_digest = digest(b"different");
    assert_ne!(
        input_bound_proposal_request_identity_digest(&proposal, &changed_input).unwrap(),
        first
    );

    let mut explicit_limit = proposal.clone();
    explicit_limit.execution.timeout_ms = Some(60_000);
    assert_ne!(
        input_bound_proposal_request_identity_digest(&explicit_limit, &inputs).unwrap(),
        first
    );
}

#[test]
fn local_linux_immutable_inputs_accept_trusted_local_and_continue_to_workspace_resolution() {
    let sandbox = Sandbox::new("input-trusted-admit", 5000);
    let authority = sandbox.root.join("authority");
    fs::create_dir_all(&authority).unwrap();
    fs::write(authority.join("input.bin"), b"payload").unwrap();
    let runtime = Runtime::new_with_input_authorities(
        runtime_config(&sandbox),
        vec![InputAuthority {
            name: "finance".to_string(),
            root: authority,
        }],
    )
    .unwrap();
    let mut request = input_bound_task_request("workspace-does-not-exist", "request:input-trusted");
    request.execution.execution_profile = ExecutionProfile::TrustedLocal;
    let error = runtime
        .run_task_with_inputs(
            &request,
            &[InputBindingRequest {
                authority: "finance".to_string(),
                relative_object: "input.bin".to_string(),
                expected_digest: digest(b"payload"),
                presentation_relative_path: "input.bin".to_string(),
            }],
        )
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::WorkspaceNotFound);
    assert_ne!(error.field.as_deref(), Some("execution.executionProfile"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
fn windows_immutable_inputs_reject_elevated_authority_before_workspace_resolution() {
    let sandbox = Sandbox::new("input-windows-elevated-reject", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let mut request =
        input_bound_task_request("workspace-does-not-exist", "request:input-windows-elevated");
    request.execution.execution_profile = ExecutionProfile::TrustedLocal;
    request.execution.execution_target = ExecutionTarget::WindowsNative;
    request.execution.windows_authority = WindowsAuthority::Elevated;
    let error = runtime
        .run_task_with_inputs(
            &request,
            &[InputBindingRequest {
                authority: "finance".to_string(),
                relative_object: "input.bin".to_string(),
                expected_digest: digest(b"payload"),
                presentation_relative_path: "input.bin".to_string(),
            }],
        )
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("execution.windowsAuthority"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
fn windows_immutable_input_paths_reject_native_aliases_before_workspace_resolution() {
    for invalid in [
        "data\\alias.bin",
        "data/CON.txt",
        "data/trailing. ",
        "data/colon:name.bin",
    ] {
        let error = super::windows::validate_windows_input_relative_path(invalid, 0).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest, "{invalid}");
        assert_eq!(
            error.field.as_deref(),
            Some("inputs[0].presentationRelativePath"),
            "{invalid}"
        );
    }
    super::windows::validate_windows_input_relative_path("data/fragment.parquet", 0).unwrap();
    for paths in [
        ["Data/input.bin", "data/INPUT.bin"],
        ["Bundle", "bundle/child.bin"],
    ] {
        let error = super::windows::validate_windows_input_relative_paths(paths).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidRequest, "{paths:?}");
        assert_eq!(
            error.field.as_deref(),
            Some("inputs[1].presentationRelativePath"),
            "{paths:?}"
        );
    }
}

#[test]
fn windows_immutable_input_tree_digest_matches_native_provider_contract() {
    let binding = EffectiveInputBinding {
        authority: "finance".to_string(),
        relative_object: "input.txt".to_string(),
        digest: "sha256:9e839d11caba648e8f04df5db7bbdb8c3a12fa0721635b841d13bc2ba00e5f7b"
            .to_string(),
        byte_length: 22,
        presentation_relative_path: "input.txt".to_string(),
        access: InputAccessMode::ReadOnly,
    };
    assert_eq!(
        super::engine::windows_input_bindings_digest(std::slice::from_ref(&binding)),
        "sha256:1ef2624c5421da462d132c75b4a7e38b2d92e9e34bf9844cc55a9515b3eaf469"
    );

    let nested = EffectiveInputBinding {
        presentation_relative_path: "payload/input.txt".to_string(),
        ..binding
    };
    assert_eq!(
        super::engine::windows_input_bindings_digest(&[nested]),
        "sha256:fbf44f54dec2b9e07f462fdf80bcc5af4660a6d9f055a349f3e7a1ac60f9ce6c"
    );
}

#[test]
fn input_digest_mismatch_fails_before_job_admission() {
    let (sandbox, original_runtime, _executor) =
        durable_patch_fixture("input-digest-mismatch", "workspace-input-digest-mismatch");
    drop(original_runtime);
    let authority = sandbox.root.join("authority");
    fs::create_dir_all(&authority).unwrap();
    fs::write(authority.join("fragment.parquet"), b"actual-bytes").unwrap();
    let runtime = Runtime::new_with_input_authorities(
        runtime_config(&sandbox),
        vec![InputAuthority {
            name: "finance".to_string(),
            root: authority,
        }],
    )
    .unwrap();
    let request = input_bound_task_request(
        "workspace-input-digest-mismatch",
        "request:input-digest-mismatch",
    );
    let inputs = vec![InputBindingRequest {
        authority: "finance".to_string(),
        relative_object: "fragment.parquet".to_string(),
        expected_digest: digest(b"different-bytes"),
        presentation_relative_path: "data/fragment.parquet".to_string(),
    }];
    let error = runtime.run_task_with_inputs(&request, &inputs).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert!(error.message.contains("materialized input digest mismatch"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    let identity = input_bound_request_identity_digest(&request, &inputs).unwrap();
    assert!(runtime
        .registry()
        .find_idempotent_job(&request.principal, &request.client_request_id, &identity)
        .unwrap()
        .is_none());
}

#[test]
fn input_authority_rejects_dotdot_and_symlink_escape_before_admission() {
    let (sandbox, original_runtime, _executor) =
        durable_patch_fixture("input-authority-escape", "workspace-input-authority-escape");
    drop(original_runtime);
    let authority = sandbox.root.join("authority");
    let outside = sandbox.root.join("outside");
    fs::create_dir_all(&authority).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.bin"), b"secret").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.bin"), authority.join("link.bin")).unwrap();
    let request = input_bound_task_request(
        "workspace-input-authority-escape",
        "request:input-authority-escape",
    );
    let runtime = Runtime::new_with_input_authorities(
        runtime_config(&sandbox),
        vec![InputAuthority {
            name: "finance".to_string(),
            root: authority,
        }],
    )
    .unwrap();

    let dotdot = runtime
        .run_task_with_inputs(
            &request,
            &[InputBindingRequest {
                authority: "finance".to_string(),
                relative_object: "../outside/secret.bin".to_string(),
                expected_digest: digest(b"secret"),
                presentation_relative_path: "secret.bin".to_string(),
            }],
        )
        .unwrap_err();
    assert_eq!(dotdot.code, RuntimeErrorCode::InvalidRequest);
    assert!(dotdot.field.as_deref().unwrap().contains("relativeObject"));

    let symlink = runtime
        .run_task_with_inputs(
            &request,
            &[InputBindingRequest {
                authority: "finance".to_string(),
                relative_object: "link.bin".to_string(),
                expected_digest: digest(b"secret"),
                presentation_relative_path: "secret.bin".to_string(),
            }],
        )
        .unwrap_err();
    assert_eq!(symlink.code, RuntimeErrorCode::InvalidRequest);
    assert!(symlink
        .message
        .contains("cannot resolve input object inside authority"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
fn execution_profile_and_foreign_references_are_part_of_request_identity() {
    let base = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:profile-reference-identity".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 4,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace-identity".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 1_000,
            stdout_limit_bytes: 1_024,
            stderr_limit_bytes: 1_024,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let trusted = operation_request_identity_digest(&base).unwrap();
    let serialized_local = serde_json::to_value(&base.execution).unwrap();
    assert_eq!(serialized_local["executionTarget"], "local_linux");

    let mut contained = base.clone();
    contained.execution.execution_profile = ExecutionProfile::ContainedLocal;
    assert_ne!(
        operation_request_identity_digest(&contained).unwrap(),
        trusted
    );

    let mut windows = base.clone();
    windows.execution.execution_target = super::ExecutionTarget::WindowsNative;
    assert_eq!(
        serde_json::to_value(&windows.execution).unwrap()["executionTarget"],
        "windows_native"
    );
    assert_ne!(
        operation_request_identity_digest(&windows).unwrap(),
        trusted
    );
    assert_eq!(
        windows.execution.execution_profile,
        ExecutionProfile::TrustedLocal
    );

    let mut referenced = base;
    referenced
        .execution
        .foreign_references
        .push(ForeignReference {
            namespace: "ordivon.edge".to_string(),
            reference_type: "supervisor_generation".to_string(),
            id: "edge-supervisor-1".to_string(),
            generation: Some("7".to_string()),
            digest: Some(digest(b"edge-generation")),
        });
    assert_ne!(
        operation_request_identity_digest(&referenced).unwrap(),
        trusted
    );
}

#[test]
fn duplicate_foreign_references_are_rejected_before_admission() {
    let sandbox = Sandbox::new("duplicate-foreign-reference", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let reference = ForeignReference {
        namespace: "ordivon.security".to_string(),
        reference_type: "operation".to_string(),
        id: "security-operation-1".to_string(),
        generation: None,
        digest: None,
    };
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:duplicate-reference".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: "workspace-test".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 1_000,
            stdout_limit_bytes: 1_024,
            stderr_limit_bytes: 1_024,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: vec![reference.clone(), reference],
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let error = runtime.run_task(&request).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert!(error
        .field
        .as_deref()
        .unwrap()
        .contains("foreignReferences"));
}

#[test]
fn terminal_evidence_is_a_durable_artifact_with_native_binding() {
    let sandbox = Sandbox::new("terminal-native-evidence", 5000);
    let mut submit = request(&sandbox, "request:terminal-native-evidence", 4);
    submit.plan.execution_profile = ExecutionProfile::TrustedLocal;
    submit.plan.execution_target = super::ExecutionTarget::WindowsNative;
    submit.plan.windows_execution_context = Some(super::WindowsExecutionContext {
        token_class: super::WindowsTokenClass::Limited,
        token_user_sid: "S-1-5-21-test-1001".to_string(),
        environment_source: "windows_user_machine_profile_allowlist_v1".to_string(),
    });
    submit.plan.foreign_references.push(ForeignReference {
        namespace: "ordivon.edge".to_string(),
        reference_type: "supervisor_generation".to_string(),
        id: "edge-supervisor-9".to_string(),
        generation: Some("9".to_string()),
        digest: None,
    });
    let created = created(sandbox.registry.submit(&submit).unwrap());
    let bundle_ready = sandbox
        .registry
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"terminal-evidence-bundle"),
            created.attempt.created_at_ms + 1,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(
            &bundle_ready.attempt_id,
            bundle_ready.row_version,
            bundle_ready.created_at_ms + 2,
        )
        .unwrap();
    write_completed_runner_result(&starting, 42);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let mut terminal = super::evidence::prepare_runner_terminal_from_bundle(&starting).unwrap();
    runtime
        .append_terminal_evidence(&starting, &mut terminal)
        .unwrap();
    sandbox.registry.commit_terminal(&terminal).unwrap();

    let artifact = sandbox
        .registry
        .list_artifacts(&created.job.job_id)
        .unwrap()
        .into_iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    assert!(artifact
        .artifact_id
        .starts_with(&format!("{}.terminal-evidence.", starting.attempt_id)));
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&starting.bundle_path).join(&artifact.relative_path)).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["operationDigest"], created.job.operation_digest);
    assert_eq!(
        evidence["executionPlanDigest"],
        created.job.execution_plan_digest
    );
    assert_eq!(evidence["executableDigest"], submit.plan.executable_digest);
    assert_eq!(evidence["executionProfile"], "trusted_local");
    assert_eq!(evidence["executionTarget"], "windows_native");
    assert_eq!(evidence["windowsExecutionContext"]["tokenClass"], "limited");
    assert_eq!(
        evidence["windowsExecutionContext"]["tokenUserSid"],
        "S-1-5-21-test-1001"
    );
    assert_eq!(evidence["foreignReferences"][0]["id"], "edge-supervisor-9");
    assert_eq!(evidence["executionDisposition"], "succeeded");
    assert_eq!(evidence["deliveryDisposition"], "committed");
    assert_eq!(evidence["processTreeDisposition"], "unknown");
    assert_eq!(evidence["terminalArtifactIds"].as_array().unwrap().len(), 3);
}

#[test]
fn host_boundary_references_replay_conflict_and_survive_terminal_evidence() {
    let sandbox = Sandbox::new("host-boundary-references", 5000);
    let mut submit = request(&sandbox, "request:harness-fixture:g1:step1", 4);
    submit.plan.source_revision = "fixture-revision".to_string();
    submit.plan.foreign_references = vec![
        ForeignReference {
            namespace: "ordivon.host".to_string(),
            reference_type: "assignment".to_string(),
            id: "assignment:fixture:1:g1".to_string(),
            generation: Some("1".to_string()),
            digest: Some(digest(b"assignment-generation-1")),
        },
        ForeignReference {
            namespace: "ordivon.host".to_string(),
            reference_type: "harness_run".to_string(),
            id: "harness-run:codex:1".to_string(),
            generation: None,
            digest: Some(digest(b"harness-run-codex-1")),
        },
        ForeignReference {
            namespace: "ordivon.host".to_string(),
            reference_type: "task".to_string(),
            id: "task:fixture".to_string(),
            generation: Some("7".to_string()),
            digest: Some(digest(b"task-fixture-revision-7")),
        },
        ForeignReference {
            namespace: "ordivon.host".to_string(),
            reference_type: "task_attempt".to_string(),
            id: "task-attempt:fixture:1".to_string(),
            generation: None,
            digest: Some(digest(b"task-attempt-fixture-1")),
        },
    ];

    let first = created(sandbox.registry.submit(&submit).unwrap());
    let replay = sandbox.registry.submit(&submit).unwrap();
    let replayed_job = match replay {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => panic!("exact Host replay created a second Job"),
    };
    assert_eq!(replayed_job.job_id, first.job.job_id);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let attempt_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM attempts WHERE job_id=?1",
            [&first.job.job_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempt_count, 1);
    drop(connection);

    let mut changed_generation = submit.clone();
    changed_generation.plan.foreign_references[0].generation = Some("2".to_string());
    let error = sandbox.registry.submit(&changed_generation).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);

    let mut changed_digest = submit.clone();
    changed_digest.plan.foreign_references[0].digest =
        Some(digest(b"assignment-generation-1-drift"));
    let error = sandbox.registry.submit(&changed_digest).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);

    let bundle_ready = sandbox
        .registry
        .mark_bundle_ready(
            &first.attempt.attempt_id,
            first.attempt.row_version,
            &digest(b"host-boundary-bundle"),
            first.attempt.created_at_ms + 1,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(
            &bundle_ready.attempt_id,
            bundle_ready.row_version,
            bundle_ready.created_at_ms + 2,
        )
        .unwrap();
    write_completed_runner_result(&starting, 42);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let mut terminal = super::evidence::prepare_runner_terminal_from_bundle(&starting).unwrap();
    runtime
        .append_terminal_evidence(&starting, &mut terminal)
        .unwrap();
    sandbox.registry.commit_terminal(&terminal).unwrap();

    let terminal_artifact = sandbox
        .registry
        .list_artifacts(&first.job.job_id)
        .unwrap()
        .into_iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&starting.bundle_path).join(&terminal_artifact.relative_path)).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["jobId"], first.job.job_id);
    assert_eq!(evidence["attemptId"], starting.attempt_id);
    assert_eq!(evidence["operationDigest"], first.job.operation_digest);
    assert_eq!(
        evidence["executionPlanDigest"],
        first.job.execution_plan_digest
    );
    assert_eq!(evidence["workspaceId"], submit.plan.workspace_id);
    assert_eq!(evidence["sourceRevision"], "fixture-revision");
    assert_eq!(evidence["executable"], submit.plan.executable);
    assert_eq!(evidence["executableDigest"], submit.plan.executable_digest);
    assert_eq!(
        serde_json::from_value::<Vec<ForeignReference>>(evidence["foreignReferences"].clone(),)
            .unwrap(),
        submit.plan.foreign_references,
    );
    assert_eq!(evidence["executionDisposition"], "succeeded");
    assert!(evidence.get("semanticCompletion").is_none());
    assert!(evidence.get("taskOutcome").is_none());

    let fresh_registry = Registry::initialize(sandbox.registry.config().clone()).unwrap();
    let located = fresh_registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 10,
            cursor: None,
            client_request_id: Some(submit.client_request_id.clone()),
            workspace_id: None,
        })
        .unwrap();
    assert_eq!(located.jobs.len(), 1);
    assert_eq!(located.jobs[0].job_id, first.job.job_id);
    assert_eq!(located.jobs[0].operation_digest, first.job.operation_digest);
    let recovered_artifact = fresh_registry
        .list_artifacts(&first.job.job_id)
        .unwrap()
        .into_iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    assert_eq!(recovered_artifact.digest, terminal_artifact.digest);
}

#[test]
fn recovered_terminal_evidence_supersedes_without_overwriting_history() {
    let sandbox = Sandbox::new("terminal-evidence-supersession", 5000);
    let submit = request(&sandbox, "request:terminal-evidence-supersession", 4);
    let created = created(sandbox.registry.submit(&submit).unwrap());
    let ready = sandbox
        .registry
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"supersession-bundle"),
            created.attempt.created_at_ms + 1,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(
            &ready.attempt_id,
            ready.row_version,
            ready.created_at_ms + 2,
        )
        .unwrap();
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();

    let mut orphaned = TerminalCommit {
        attempt_id: starting.attempt_id.clone(),
        expected_row_version: starting.row_version,
        state: AttemptState::Orphaned,
        result_digest: digest(b"orphaned-control-result"),
        exit_code: None,
        infrastructure_error_digest: Some(digest(b"identity-conflict")),
        finished_at_ms: starting.created_at_ms + 3,
        artifacts: Vec::new(),
        reason_code: "SUPERVISOR_IDENTITY_ORPHANED".to_string(),
    };
    runtime
        .append_terminal_evidence(&starting, &mut orphaned)
        .unwrap();
    sandbox.registry.commit_terminal(&orphaned).unwrap();

    let current = sandbox.registry.get_attempt(&starting.attempt_id).unwrap();
    let mut recovered = TerminalCommit {
        attempt_id: current.attempt_id.clone(),
        expected_row_version: current.row_version,
        state: AttemptState::Succeeded,
        result_digest: digest(b"late-runner-result"),
        exit_code: Some(0),
        infrastructure_error_digest: None,
        finished_at_ms: current.finished_at_ms.unwrap() + 1,
        artifacts: Vec::new(),
        reason_code: "LATE_IDENTITY_BOUND_RUNNER_RESULT".to_string(),
    };
    runtime
        .append_terminal_evidence(&current, &mut recovered)
        .unwrap();
    sandbox
        .registry
        .recover_orphaned_terminal(&recovered)
        .unwrap();

    let evidence_artifacts = sandbox
        .registry
        .list_artifacts(&created.job.job_id)
        .unwrap()
        .into_iter()
        .filter(|artifact| artifact.kind == "terminal_evidence")
        .collect::<Vec<_>>();
    assert_eq!(evidence_artifacts.len(), 2);
    assert_ne!(
        evidence_artifacts[0].relative_path,
        evidence_artifacts[1].relative_path
    );
    let first: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&starting.bundle_path).join(&evidence_artifacts[0].relative_path))
            .unwrap(),
    )
    .unwrap();
    let second: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&starting.bundle_path).join(&evidence_artifacts[1].relative_path))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["executionDisposition"], "orphaned");
    assert_eq!(second["executionDisposition"], "succeeded");
    assert_eq!(
        second["supersedesArtifactId"],
        evidence_artifacts[0].artifact_id
    );
}

#[test]
fn request_identity_replays_across_world_change_but_operation_identity_binds_world() {
    let sandbox = Sandbox::new("request-world-identity", 5000);
    let proposal = format!("{}{}", REQUEST_IDENTITY_PREFIX, digest(b"same proposal"));
    let mut first = request(&sandbox, "request:request-world-identity", 4);
    first.request_identity_digest = Some(proposal.clone());
    first.plan.workspace_source_digest = Some(digest(b"source-state-one"));
    let created = created(sandbox.registry.submit(&first).unwrap());
    let first_operation = created.job.operation_digest.clone();

    let mut same_request_new_world = first.clone();
    same_request_new_world.plan.workspace_source_digest = Some(digest(b"source-state-two"));
    let replay = sandbox.registry.submit(&same_request_new_world).unwrap();
    let replay_job = match replay {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => panic!("same request identity must replay"),
    };
    assert_eq!(replay_job.job_id, created.job.job_id);
    assert_eq!(replay_job.operation_digest, first_operation);

    let mut changed_request = same_request_new_world;
    changed_request.request_identity_digest = Some(format!(
        "{}{}",
        REQUEST_IDENTITY_PREFIX,
        digest(b"different proposal")
    ));
    let error = sandbox.registry.submit(&changed_request).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);
}

#[test]
fn registry_accepts_input_bound_proposal_identity_without_changing_legacy_prefixes() {
    for (index, prefix) in [
        REQUEST_IDENTITY_PREFIX,
        PROPOSAL_IDENTITY_PREFIX,
        INPUT_BOUND_IDENTITY_PREFIX,
        INPUT_BOUND_PROPOSAL_IDENTITY_PREFIX,
    ]
    .into_iter()
    .enumerate()
    {
        let sandbox = Sandbox::new(&format!("request-identity-prefix-{index}"), 5000);
        let mut submission = request(&sandbox, "request:identity-prefix", 4);
        submission.request_identity_digest = Some(format!("{}sha256:{}", prefix, "a".repeat(64)));
        let admitted = sandbox.registry.submit(&submission).unwrap();
        assert!(matches!(admitted, AdmissionOutcome::Created(_)), "{prefix}");
    }
}

#[test]
fn proposal_identity_preserves_omission_and_normalizes_equivalent_paths() {
    let base = TaskRunProposal {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:proposal-identity".to_string(),
        principal: "principal:test".to_string(),
        global_limit: 4,
        execution: ExecutionProposal {
            workspace_id: "workspace:test".to_string(),
            executable: "/usr/bin/../bin/true".to_string(),
            args: vec!["argument".to_string()],
            cwd_relative: "subdir/../subdir".to_string(),
            env: BTreeMap::from([("KEY".to_string(), "VALUE".to_string())]),
            timeout_ms: None,
            stdout_limit_bytes: None,
            stderr_limit_bytes: None,
            steps: vec![ExecutionStepProposal {
                id: "step".to_string(),
                executable: "/usr/bin/../bin/true".to_string(),
                args: Vec::new(),
                cwd_relative: "subdir/../subdir".to_string(),
                env: BTreeMap::new(),
                timeout_ms: None,
                continue_on_error: false,
            }],
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: super::ExecutionTarget::LocalLinux,
            windows_authority: super::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let digest = proposal_request_identity_digest(&base).unwrap();
    assert!(digest.starts_with(PROPOSAL_IDENTITY_PREFIX));

    let mut equivalent = base.clone();
    equivalent.execution.executable = "/usr/bin/true".to_string();
    equivalent.execution.cwd_relative = "subdir".to_string();
    equivalent.execution.steps[0].executable = "/usr/bin/true".to_string();
    equivalent.execution.steps[0].cwd_relative = "subdir".to_string();
    assert_eq!(
        proposal_request_identity_digest(&equivalent).unwrap(),
        digest
    );

    let mut explicit = equivalent;
    explicit.execution.timeout_ms = Some(10_000);
    assert_ne!(proposal_request_identity_digest(&explicit).unwrap(), digest);
}

#[test]
fn proposal_identity_replays_original_effective_plan() {
    let sandbox = Sandbox::new("proposal-replay", 5000);
    let mut first = request(&sandbox, "request:proposal-replay", 4);
    let proposal = format!("{}sha256:{}", PROPOSAL_IDENTITY_PREFIX, "a".repeat(64));
    first.request_identity_digest = Some(proposal.clone());
    first.plan.timeout_ms = 60_000;
    first.plan.stdout_limit_bytes = 1_048_576;
    first.plan.stderr_limit_bytes = 1_048_576;
    let created = created(sandbox.registry.submit(&first).unwrap());
    let original_plan = created.job.execution_plan_json.clone();

    let mut same_proposal_different_effective_plan = first.clone();
    same_proposal_different_effective_plan.plan.timeout_ms = 3_000;
    same_proposal_different_effective_plan
        .plan
        .stdout_limit_bytes = 65_536;
    same_proposal_different_effective_plan
        .plan
        .stderr_limit_bytes = 65_536;
    let replay = sandbox
        .registry
        .submit(&same_proposal_different_effective_plan)
        .unwrap();
    let existing = match replay {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => panic!("same proposal identity must replay existing Job"),
    };
    assert_eq!(existing.job_id, created.job.job_id);
    assert_eq!(existing.request_digest, proposal);
    assert_eq!(existing.execution_plan_json, original_plan);
}

#[test]
fn legacy_job_request_identity_is_derived_from_stored_plan() {
    let sandbox = Sandbox::new("legacy-request-identity", 5000);
    let legacy = request(&sandbox, "request:legacy-request-identity", 4);
    let created = created(sandbox.registry.submit(&legacy).unwrap());
    assert!(!created
        .job
        .request_digest
        .starts_with(REQUEST_IDENTITY_PREFIX));
    let derived = operation_request_identity_digest_from_plan(&legacy.plan).unwrap();
    let found = sandbox
        .registry
        .find_idempotent_job(&legacy.plan.principal, &legacy.client_request_id, &derived)
        .unwrap()
        .unwrap();
    assert_eq!(found.job_id, created.job.job_id);
}

#[test]
fn idempotent_replay_returns_one_job_and_conflict_rejects_change() {
    let sandbox = Sandbox::new("idempotency", 5000);
    let original = request(&sandbox, "request:same", 4);
    let first = created(sandbox.registry.submit(&original).unwrap());
    let replay = sandbox.registry.submit(&original).unwrap();
    let existing = match replay {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => panic!("replay created a second Job"),
    };
    assert_eq!(first.job.job_id, existing.job_id);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);

    let mut changed = original;
    changed.plan.timeout_ms += 1;
    let error = sandbox.registry.submit(&changed).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
}

#[test]
fn simultaneous_same_key_creates_one_job_and_one_attempt() {
    let sandbox = Sandbox::new("same-key-race", 5000);
    let registry = sandbox.registry.clone();
    let request = request(&sandbox, "request:race", 4);
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let registry = registry.clone();
        let request = request.clone();
        let barrier = barrier.clone();
        joins.push(thread::spawn(move || {
            barrier.wait();
            registry.submit(&request)
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = joins
        .into_iter()
        .map(|join| join.join().unwrap().unwrap())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, AdmissionOutcome::Created(_)))
            .count(),
        1
    );
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
}

#[test]
fn simultaneous_admissions_cannot_overbook_last_global_slot() {
    let sandbox = Sandbox::new("capacity-race", 5000);
    let registry = sandbox.registry.clone();
    let first = request(&sandbox, "request:capacity:a", 1);
    let second = request(&sandbox, "request:capacity:b", 1);
    let barrier = Arc::new(Barrier::new(3));
    let joins = [first, second].map(|request| {
        let registry = registry.clone();
        let barrier = barrier.clone();
        thread::spawn(move || {
            barrier.wait();
            registry.submit(&request)
        })
    });
    barrier.wait();
    let results: Vec<_> = joins.into_iter().map(|join| join.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let error = results.into_iter().find_map(Result::err).unwrap();
    assert_eq!(error.code, RuntimeErrorCode::ConcurrencyLimit);
    assert!(error.retryable);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
}

#[test]
fn busy_writer_fails_with_retryable_registry_busy() {
    let sandbox = Sandbox::new("busy", 40);
    let lock = Connection::open(&sandbox.registry.config().db_path).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let error = sandbox
        .registry
        .submit(&request(&sandbox, "request:busy", 4))
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::RegistryBusy);
    assert!(error.retryable);
    lock.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn admission_commit_fault_classifies_durable_and_uncommitted_outcomes() {
    let committed = Sandbox::new("admission-commit-then-error", 5_000);
    let committed_request = request(&committed, "request:admission-commit-then-error", 1);
    set_test_commit_fault(TestCommitPoint::Admission, TestCommitFault::CommitThenError);
    let existing = match committed.registry.submit(&committed_request).unwrap() {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => panic!("commit-then-error was not reconciled as durable"),
    };
    assert_eq!(committed.registry.active_reservation_count().unwrap(), 1);
    let replay = match committed.registry.submit(&committed_request).unwrap() {
        AdmissionOutcome::Existing { job } => job,
        AdmissionOutcome::Created(_) => panic!("durable admission replay created a second Job"),
    };
    assert_eq!(existing.job_id, replay.job_id);

    for (label, fault) in [
        ("rollback-then-error", TestCommitFault::RollbackThenError),
        ("deferred-constraint", TestCommitFault::DeferredConstraint),
    ] {
        let sandbox = Sandbox::new(&format!("admission-{label}"), 5_000);
        let submit = request(&sandbox, &format!("request:admission-{label}"), 1);
        set_test_commit_fault(TestCommitPoint::Admission, fault);
        let error = sandbox.registry.submit(&submit).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::RegistryUnavailable);
        assert!(error.retryable);
        assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
        let created = created(sandbox.registry.submit(&submit).unwrap());
        assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
        let replay = match sandbox.registry.submit(&submit).unwrap() {
            AdmissionOutcome::Existing { job } => job,
            AdmissionOutcome::Created(_) => panic!("replay created a second Job after retry"),
        };
        assert_eq!(created.job.job_id, replay.job_id);
    }
}

#[test]
fn cancel_commit_fault_classifies_durable_and_uncommitted_outcomes() {
    let committed = Sandbox::new("cancel-commit-then-error", 5_000);
    let committed_admission = created(
        committed
            .registry
            .submit(&request(&committed, "request:cancel-commit-then-error", 1))
            .unwrap(),
    );
    set_test_commit_fault(TestCommitPoint::Cancel, TestCommitFault::CommitThenError);
    let projection = committed
        .registry
        .request_cancel(&committed_admission.job.job_id, 10)
        .unwrap();
    assert_eq!(projection.status, "cancelled");
    assert_eq!(committed.registry.active_reservation_count().unwrap(), 0);
    let replay = committed
        .registry
        .request_cancel(&committed_admission.job.job_id, 11)
        .unwrap();
    assert_eq!(replay.status, "cancelled");

    for (label, fault) in [
        ("rollback-then-error", TestCommitFault::RollbackThenError),
        ("deferred-constraint", TestCommitFault::DeferredConstraint),
    ] {
        let sandbox = Sandbox::new(&format!("cancel-{label}"), 5_000);
        let created = created(
            sandbox
                .registry
                .submit(&request(&sandbox, &format!("request:cancel-{label}"), 1))
                .unwrap(),
        );
        set_test_commit_fault(TestCommitPoint::Cancel, fault);
        let error = sandbox
            .registry
            .request_cancel(&created.job.job_id, 10)
            .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::RegistryUnavailable);
        assert!(error.retryable);
        let current = sandbox.registry.get_job(&created.job.job_id).unwrap();
        assert_eq!(current.desired_state, JobDesiredState::Run);
        assert_eq!(current.resolution, None);
        assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
        let projection = sandbox
            .registry
            .request_cancel(&created.job.job_id, 11)
            .unwrap();
        assert_eq!(projection.status, "cancelled");
        assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    }
}

#[test]
fn terminal_commit_fault_classifies_durable_and_uncommitted_outcomes() {
    let committed = Sandbox::new("terminal-commit-then-error", 5_000);
    let attempt =
        running_attempt_for_commit_fault(&committed, "request:terminal-commit-then-error");
    let terminal = TerminalCommit {
        attempt_id: attempt.attempt_id.clone(),
        expected_row_version: attempt.row_version,
        state: AttemptState::Succeeded,
        result_digest: digest(b"commit-fault-result"),
        exit_code: Some(0),
        infrastructure_error_digest: None,
        finished_at_ms: 13,
        artifacts: Vec::new(),
        reason_code: "PROCESS_EXIT_ZERO".to_string(),
    };
    set_test_commit_fault(TestCommitPoint::Terminal, TestCommitFault::CommitThenError);
    let projection = committed.registry.commit_terminal(&terminal).unwrap();
    assert_eq!(projection.status, "succeeded");
    assert_eq!(committed.registry.active_reservation_count().unwrap(), 0);
    assert_eq!(
        committed.registry.commit_terminal(&terminal).unwrap(),
        projection
    );

    for (label, fault) in [
        ("rollback-then-error", TestCommitFault::RollbackThenError),
        ("deferred-constraint", TestCommitFault::DeferredConstraint),
    ] {
        let sandbox = Sandbox::new(&format!("terminal-{label}"), 5_000);
        let attempt =
            running_attempt_for_commit_fault(&sandbox, &format!("request:terminal-{label}"));
        let terminal = TerminalCommit {
            attempt_id: attempt.attempt_id.clone(),
            expected_row_version: attempt.row_version,
            state: AttemptState::Succeeded,
            result_digest: digest(format!("terminal-{label}").as_bytes()),
            exit_code: Some(0),
            infrastructure_error_digest: None,
            finished_at_ms: 13,
            artifacts: Vec::new(),
            reason_code: "PROCESS_EXIT_ZERO".to_string(),
        };
        set_test_commit_fault(TestCommitPoint::Terminal, fault);
        let error = sandbox.registry.commit_terminal(&terminal).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::RegistryUnavailable);
        assert!(error.retryable);
        let current = sandbox.registry.get_attempt(&attempt.attempt_id).unwrap();
        assert_eq!(current.state, AttemptState::Running);
        assert_eq!(current.row_version, terminal.expected_row_version);
        assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
        let projection = sandbox.registry.commit_terminal(&terminal).unwrap();
        assert_eq!(projection.status, "succeeded");
        assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    }
}

#[test]
fn active_workspace_job_lookup_tracks_reservations_and_resolution() {
    let sandbox = Sandbox::new("active-workspace", 5000);
    let mut active_request = request(&sandbox, "request:active-workspace", 4);
    active_request.plan.workspace_id = "workspace:active-workspace".to_string();
    let created = created(sandbox.registry.submit(&active_request).unwrap());
    assert_eq!(
        sandbox
            .registry
            .active_job_ids_for_workspace("workspace:active-workspace")
            .unwrap(),
        vec![created.job.job_id.clone()]
    );
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id,
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Cancelled,
            result_digest: digest(b"cancelled"),
            exit_code: None,
            infrastructure_error_digest: None,
            finished_at_ms: created.job.created_at_ms + 1,
            artifacts: Vec::new(),
            reason_code: "TEST_CANCELLED".to_string(),
        })
        .unwrap();
    assert!(sandbox
        .registry
        .active_job_ids_for_workspace("workspace:active-workspace")
        .unwrap()
        .is_empty());
}

#[test]
fn list_is_bounded_and_cursor_stable() {
    let sandbox = Sandbox::new("list", 5000);
    for index in 0..3 {
        let mut list_request = request(&sandbox, &format!("request:list:{index}"), 8);
        list_request.plan.workspace_id = format!("workspace:list:{index}");
        let created = created(sandbox.registry.submit(&list_request).unwrap());
        assert!(created.job.job_id.starts_with("job-"));
    }
    let first = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 2,
            cursor: None,
            client_request_id: None,
            workspace_id: None,
        })
        .unwrap();
    assert_eq!(first.jobs.len(), 2);
    assert!(first.next_cursor.is_some());
    let second = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 2,
            cursor: first.next_cursor,
            client_request_id: None,
            workspace_id: None,
        })
        .unwrap();
    assert_eq!(second.jobs.len(), 1);
    assert!(second.next_cursor.is_none());
}

#[test]
fn workspace_close_preserves_git_authority_owned_by_an_open_child() {
    let (sandbox, runtime, executor) = durable_patch_fixture(
        "workspace-dependent-git-authority",
        "workspace-dependent-parent",
    );
    let parent_id = "workspace-dependent-parent";
    let child_id = "workspace-dependent-child";
    let child_source = executor.workspace_tmp_path(parent_id).join("child-source");
    fs::create_dir_all(&child_source).unwrap();
    fs::write(child_source.join("child.txt"), "child\n").unwrap();
    run_git_command(&child_source, &["init", "-q"]);
    run_git_command(
        &child_source,
        &["config", "user.email", "runtime-tests@ordivon.local"],
    );
    run_git_command(
        &child_source,
        &["config", "user.name", "Ordivon Runtime Tests"],
    );
    run_git_command(&child_source, &["add", "."]);
    run_git_command(&child_source, &["commit", "-qm", "child source"]);
    runtime
        .open_workspace(&crate::GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: child_id.to_string(),
            source_repo: child_source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        })
        .unwrap();
    let child_workspace = executor.workspace_path(child_id);
    let child_head = git_output(&child_workspace, &["rev-parse", "HEAD"]);

    let blocked = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: parent_id.to_string(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap_err();
    assert_eq!(blocked.code, RuntimeErrorCode::WorkspaceBusy);
    assert!(blocked.message.contains(child_id));
    assert!(child_source.exists());
    assert_eq!(
        git_output(&child_workspace, &["rev-parse", "HEAD"]),
        child_head
    );

    runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: child_id.to_string(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: parent_id.to_string(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    assert!(!executor.workspace_tmp_path(parent_id).exists());
    drop(sandbox);
}

#[test]
fn workspace_close_tracks_git_authority_not_source_path_text() {
    let (_sandbox, runtime, executor) = durable_patch_fixture(
        "workspace-dependent-authority-not-path",
        "workspace-authority-parent",
    );
    let parent_id = "workspace-authority-parent";
    let child_id = "workspace-authority-child";
    let parent_workspace = executor.workspace_path(parent_id);
    runtime
        .open_workspace(&crate::GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: child_id.to_string(),
            source_repo: parent_workspace.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        })
        .unwrap();
    let child_workspace = executor.workspace_path(child_id);
    let child_head = git_output(&child_workspace, &["rev-parse", "HEAD"]);
    let parent_close = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: parent_id.to_string(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    assert!(parent_close.removed);
    assert_eq!(
        git_output(&child_workspace, &["rev-parse", "HEAD"]),
        child_head
    );
    runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: child_id.to_string(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
}

#[test]
fn concurrent_workspace_open_and_parent_close_never_create_a_broken_child() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let (sandbox, runtime, executor) =
        durable_patch_fixture("workspace-dependent-race", "workspace-race-bootstrap");
    runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-race-bootstrap".to_string(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    let stable_source = sandbox.root.join("patch-source");

    for index in 0..12 {
        let parent_id = format!("workspace-race-parent-{index}");
        let child_id = format!("workspace-race-child-{index}");
        runtime
            .open_workspace(&crate::GitWorkspaceCreateRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: parent_id.clone(),
                source_repo: stable_source.to_string_lossy().into_owned(),
                source_revision: "HEAD".to_string(),
            })
            .unwrap();
        let child_source = executor.workspace_tmp_path(&parent_id).join("child-source");
        fs::create_dir_all(&child_source).unwrap();
        fs::write(child_source.join("child.txt"), format!("child {index}\n")).unwrap();
        run_git_command(&child_source, &["init", "-q"]);
        run_git_command(
            &child_source,
            &["config", "user.email", "runtime-tests@ordivon.local"],
        );
        run_git_command(
            &child_source,
            &["config", "user.name", "Ordivon Runtime Tests"],
        );
        run_git_command(&child_source, &["add", "."]);
        run_git_command(&child_source, &["commit", "-qm", "child source"]);

        let barrier = Arc::new(Barrier::new(3));
        let close_runtime = runtime.clone();
        let close_barrier = barrier.clone();
        let close_parent_id = parent_id.clone();
        let close_thread = thread::spawn(move || {
            close_barrier.wait();
            close_runtime.close_workspace(&WorkspaceCloseRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: close_parent_id,
                force: true,
                expected_source_state_digest: None,
            })
        });
        let open_runtime = runtime.clone();
        let open_barrier = barrier.clone();
        let open_child_id = child_id.clone();
        let open_source = child_source.clone();
        let open_thread = thread::spawn(move || {
            open_barrier.wait();
            open_runtime.open_workspace(&crate::GitWorkspaceCreateRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: open_child_id,
                source_repo: open_source.to_string_lossy().into_owned(),
                source_revision: "HEAD".to_string(),
            })
        });
        barrier.wait();
        let close_result = close_thread.join().unwrap();
        let open_result = open_thread.join().unwrap();

        match (close_result, open_result) {
            (Err(close_error), Ok(_)) => {
                assert_eq!(close_error.code, RuntimeErrorCode::WorkspaceBusy);
                assert!(close_error.message.contains(&child_id));
                assert!(git_output(&executor.workspace_path(&child_id), &["rev-parse", "HEAD"])
                    .len()
                    >= 40);
                runtime
                    .close_workspace(&WorkspaceCloseRequest {
                        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                        workspace_id: child_id.clone(),
                        force: true,
                        expected_source_state_digest: None,
                    })
                    .unwrap();
                runtime
                    .close_workspace(&WorkspaceCloseRequest {
                        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                        workspace_id: parent_id.clone(),
                        force: true,
                        expected_source_state_digest: None,
                    })
                    .unwrap();
            }
            (Ok(_), Err(_)) => {
                assert!(!executor.workspace_path(&parent_id).exists());
                assert!(!executor.workspace_path(&child_id).exists());
            }
            (Ok(_), Ok(_)) => panic!(
                "parent close and dependent child open both succeeded; lifecycle lock failed"
            ),
            (Err(close_error), Err(open_error)) => panic!(
                "both serialized lifecycle operations failed: close={close_error}; open={open_error}"
            ),
        }
    }
}

#[test]
fn workspace_get_distinguishes_opening_revision_from_current_head() {
    let (sandbox, runtime, executor) =
        durable_patch_fixture("workspace-get-source-repo", "workspace-get-source-repo");
    let initial = runtime
        .get_workspace(&RuntimeWorkspaceGetRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            workspace_id: "workspace-get-source-repo".to_string(),
        })
        .unwrap();
    assert_eq!(
        Path::new(&initial.source_repo),
        fs::canonicalize(sandbox.root.join("patch-source")).unwrap()
    );
    assert_eq!(initial.source_revision, initial.current_head_revision);

    let workspace = executor.workspace_path("workspace-get-source-repo");
    fs::write(workspace.join("README.md"), "advanced\n").unwrap();
    run_git_command(&workspace, &["add", "README.md"]);
    run_git_command(&workspace, &["commit", "-qm", "advance workspace head"]);
    let current_head = git_output(&workspace, &["rev-parse", "HEAD"]);

    let advanced = runtime
        .get_workspace(&RuntimeWorkspaceGetRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            workspace_id: "workspace-get-source-repo".to_string(),
        })
        .unwrap();
    assert_eq!(advanced.source_revision, initial.source_revision);
    assert_ne!(advanced.source_revision, current_head);
    assert_eq!(advanced.current_head_revision, current_head);

    let listed = runtime
        .list_workspaces(&RuntimeWorkspaceListRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            limit: 20,
            cursor: None,
            include_source_state_digest: false,
        })
        .unwrap();
    let listed = listed
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == "workspace-get-source-repo")
        .unwrap();
    assert_eq!(listed.source_revision, initial.source_revision);
    assert_eq!(listed.current_head_revision, current_head);
}

#[test]
fn workspace_inspection_is_projection_only_and_keeps_terminal_attempt_truth() {
    let (sandbox, _runtime, executor) =
        durable_patch_fixture("workspace-inspection", "workspace-inspection");
    let workspace = executor.workspace_path("workspace-inspection");
    fs::write(workspace.join("UNTRACKED.txt"), "observer\n").unwrap();

    let mut submit = request(&sandbox, "request:workspace-inspection", 4);
    submit.plan.workspace_id = "workspace-inspection".to_string();
    submit.plan.workspace_path = workspace.to_string_lossy().into_owned();
    submit.plan.cwd = workspace.to_string_lossy().into_owned();
    submit.plan.source_revision = git_output(&workspace, &["rev-parse", "HEAD"]);
    let created = created(sandbox.registry.submit(&submit).unwrap());

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    let before_active = fs::read(&sandbox.registry.config().db_path).unwrap();
    let git_index = PathBuf::from(git_output(
        &workspace,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    ));
    let before_index = fs::read(&git_index).unwrap();
    let config = RuntimeWorkspaceInspectionConfig {
        db_path: sandbox.registry.config().db_path.clone(),
        store_root: executor.store_root.clone(),
        busy_timeout_ms: 5_000,
    };
    let active = inspect_workspace(&config, "workspace-inspection", 20).unwrap();
    assert_eq!(
        before_active,
        fs::read(&sandbox.registry.config().db_path).unwrap()
    );
    assert_eq!(before_index, fs::read(&git_index).unwrap());
    assert!(active.dirty);
    assert!(active
        .untracked_paths
        .contains(&"UNTRACKED.txt".to_string()));
    assert_eq!(active.active_job_ids, vec![created.job.job_id.clone()]);
    assert_eq!(active.recent_jobs.len(), 1);
    assert_eq!(active.recent_jobs[0].job_id, created.job.job_id);
    assert_eq!(
        active.recent_jobs[0].attempt_state,
        Some(AttemptState::Accepted)
    );
    assert_eq!(active.recent_jobs[0].execution_disposition, None);
    assert!(!active.recent_jobs_truncated);

    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Cancelled,
            result_digest: digest(b"workspace-inspection-terminal"),
            exit_code: None,
            infrastructure_error_digest: None,
            finished_at_ms: created.job.created_at_ms + 1,
            artifacts: Vec::new(),
            reason_code: "TEST_CANCELLED".to_string(),
        })
        .unwrap();
    assert!(sandbox
        .registry
        .get_job(&created.job.job_id)
        .unwrap()
        .current_attempt_id
        .is_none());

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    let before_terminal = fs::read(&sandbox.registry.config().db_path).unwrap();
    let terminal = inspect_workspace(&config, "workspace-inspection", 20).unwrap();
    assert_eq!(
        before_terminal,
        fs::read(&sandbox.registry.config().db_path).unwrap()
    );
    assert!(terminal.active_job_ids.is_empty());
    assert_eq!(terminal.recent_jobs.len(), 1);
    assert_eq!(
        terminal.recent_jobs[0].attempt_id.as_deref(),
        Some(created.attempt.attempt_id.as_str())
    );
    assert_eq!(
        terminal.recent_jobs[0].attempt_state,
        Some(AttemptState::Cancelled)
    );
    assert_eq!(
        terminal.recent_jobs[0].execution_disposition,
        Some(JobResolution::Cancelled)
    );
}

#[test]
fn workspace_list_cursor_pagination_is_complete_and_unique() {
    let (sandbox, runtime, _executor) =
        durable_patch_fixture("workspace-list-cursor", "workspace-list-cursor-0");
    let source = sandbox.root.join("patch-source");
    for index in 1..4 {
        runtime
            .open_workspace(&crate::GitWorkspaceCreateRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: format!("workspace-list-cursor-{index}"),
                source_repo: source.to_string_lossy().into_owned(),
                source_revision: "HEAD".to_string(),
            })
            .unwrap();
    }

    let mut cursor = None;
    let mut observed = Vec::new();
    loop {
        let page = runtime
            .list_workspaces(&RuntimeWorkspaceListRequest {
                schema_version: RUNTIME_SCHEMA_VERSION,
                limit: 1,
                cursor,
                include_source_state_digest: false,
            })
            .unwrap();
        assert!(page.issues.is_empty());
        observed.extend(
            page.workspaces
                .iter()
                .map(|workspace| workspace.workspace_id.clone()),
        );
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(observed.len(), 4);
    let unique = observed.iter().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), 4);
    for index in 0..4 {
        assert!(observed.contains(&format!("workspace-list-cursor-{index}")));
    }
}

#[test]
fn workspace_list_isolates_workspace_local_projection_failure_with_stage() {
    let (_sandbox, runtime, executor) =
        durable_patch_fixture("workspace-list-local-issue", "workspace-list-local-issue");
    let record_path = executor.workspace_record_path("workspace-list-local-issue");
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    record["workspacePath"] = serde_json::Value::String(
        executor
            .workspace_path("different-workspace")
            .to_string_lossy()
            .into_owned(),
    );
    fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();

    let result = runtime
        .list_workspaces(&RuntimeWorkspaceListRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            limit: 20,
            cursor: None,
            include_source_state_digest: false,
        })
        .unwrap();

    assert!(result.workspaces.is_empty());
    assert_eq!(result.issues.len(), 1);
    let issue = &result.issues[0];
    assert_eq!(issue.workspace_id, "workspace-list-local-issue");
    assert_eq!(issue.stage, RuntimeWorkspaceIssueStage::Inventory);
    assert_eq!(issue.code, "METADATA_CORRUPT");
}

#[test]
fn workspace_list_surfaces_invalid_current_physical_candidate() {
    let (_sandbox, runtime, executor) = durable_patch_fixture(
        "workspace-list-invalid-current",
        "workspace-list-current-valid",
    );
    fs::create_dir(executor.workspaces_root().join("invalid current id")).unwrap();

    let result = runtime
        .list_workspaces(&RuntimeWorkspaceListRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            limit: 20,
            cursor: None,
            include_source_state_digest: false,
        })
        .unwrap();

    assert_eq!(result.workspaces.len(), 1);
    assert_eq!(
        result.workspaces[0].workspace_id,
        "workspace-list-current-valid"
    );
    assert!(result.issues.iter().any(|issue| {
        issue.workspace_id == "invalid current id"
            && issue.stage == RuntimeWorkspaceIssueStage::Inventory
            && issue.code == "INVALID_REQUEST"
    }));
}

#[test]
fn workspace_list_ignores_corrupt_history_without_a_physical_open_workspace() {
    let (_sandbox, runtime, executor) =
        durable_patch_fixture("workspace-list-history-poison", "workspace-list-current");
    fs::write(
        executor
            .workspace_records_root()
            .join("historical-poison.json"),
        b"{ definitely-not-json",
    )
    .unwrap();

    let result = runtime
        .list_workspaces(&RuntimeWorkspaceListRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            limit: 20,
            cursor: None,
            include_source_state_digest: false,
        })
        .unwrap();

    assert_eq!(result.workspaces.len(), 1);
    assert_eq!(result.workspaces[0].workspace_id, "workspace-list-current");
    assert!(result.issues.is_empty());
}

#[test]
fn newest_first_cursor_pagination_is_complete_and_unique() {
    let sandbox = Sandbox::new("pagination-maximal", 5000);
    let job_count = 29usize;
    for index in 0..job_count {
        let mut list_request = request(&sandbox, &format!("request:pagination:{index}"), 64);
        list_request.plan.workspace_id = format!("workspace:pagination:{index}");
        sandbox.registry.submit(&list_request).unwrap();
    }

    for page_size in 1u32..10 {
        let mut cursor = None;
        let mut observed = Vec::new();
        loop {
            let page = sandbox
                .registry
                .list_jobs(&RuntimeJobListRequest {
                    limit: page_size,
                    cursor,
                    client_request_id: None,
                    workspace_id: None,
                })
                .unwrap();
            observed.extend(page.jobs.iter().map(|job| {
                (
                    job.created_at_ms,
                    job.job_id.clone(),
                    job.client_request_id.clone(),
                )
            }));
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(observed.len(), job_count, "page_size={page_size}");
        let unique: std::collections::BTreeSet<_> = observed.iter().map(|(_, id, _)| id).collect();
        assert_eq!(unique.len(), job_count, "page_size={page_size}");
        assert!(
            observed
                .windows(2)
                .all(|pair| { (pair[0].0, pair[0].1.as_str()) >= (pair[1].0, pair[1].1.as_str()) }),
            "page_size={page_size}"
        );
        let requests: std::collections::BTreeSet<_> =
            observed.iter().map(|(_, _, request)| request).collect();
        assert_eq!(requests.len(), job_count, "page_size={page_size}");
    }
}

#[test]
fn list_filters_by_exact_client_request_id() {
    let sandbox = Sandbox::new("list-client-request", 5000);
    let target_id = "request:list-client-request:target";
    for index in 0..3 {
        let request_id = if index == 1 {
            target_id.to_string()
        } else {
            format!("request:list-client-request:{index}")
        };
        let mut list_request = request(&sandbox, &request_id, 8);
        list_request.plan.workspace_id = format!("workspace:list-client-request:{index}");
        sandbox.registry.submit(&list_request).unwrap();
    }

    let filtered = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 100,
            cursor: None,
            client_request_id: Some(target_id.to_string()),
            workspace_id: None,
        })
        .unwrap();
    assert_eq!(filtered.jobs.len(), 1);
    assert_eq!(filtered.jobs[0].client_request_id, target_id);
    assert_eq!(filtered.jobs[0].desired_state, JobDesiredState::Run);
    assert_eq!(filtered.jobs[0].attempt_state, Some(AttemptState::Accepted));
    assert_eq!(
        filtered.jobs[0].termination_intent,
        Some(AttemptTerminationIntent::Natural)
    );
    assert!(!filtered.jobs[0].execution_terminal);
    assert_eq!(filtered.jobs[0].execution_disposition, None);
    assert_eq!(
        filtered.jobs[0].delivery_disposition,
        RuntimeDeliveryDisposition::InProgress
    );
    assert!(!filtered.jobs[0].recovery_required);
    assert!(!filtered.jobs[0].semantic_completion_evaluated);
    assert!(filtered.next_cursor.is_none());

    let absent = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 100,
            cursor: None,
            client_request_id: Some("request:list-client-request:absent".to_string()),
            workspace_id: None,
        })
        .unwrap();
    assert!(absent.jobs.is_empty());
    assert!(absent.next_cursor.is_none());
}

#[test]
fn list_filters_and_paginates_by_exact_workspace_id() {
    let sandbox = Sandbox::new("list-workspace", 5000);
    let target_workspace = "workspace:list-workspace:target";
    for index in 0..5 {
        let mut list_request = request(&sandbox, &format!("request:list-workspace:{index}"), 8);
        list_request.plan.workspace_id = if index < 3 {
            target_workspace.to_string()
        } else {
            format!("workspace:list-workspace:other:{index}")
        };
        let admitted = created(sandbox.registry.submit(&list_request).unwrap());
        sandbox
            .registry
            .request_cancel(&admitted.job.job_id, 100 + index)
            .unwrap();
    }

    let mut cursor = None;
    let mut observed = Vec::new();
    loop {
        let page = sandbox
            .registry
            .list_jobs(&RuntimeJobListRequest {
                limit: 1,
                cursor,
                client_request_id: None,
                workspace_id: Some(target_workspace.to_string()),
            })
            .unwrap();
        observed.extend(page.jobs);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(observed.len(), 3);
    assert!(observed
        .iter()
        .all(|job| job.workspace_id == target_workspace));
    let unique: BTreeSet<_> = observed.iter().map(|job| &job.job_id).collect();
    assert_eq!(unique.len(), 3);
}

#[test]
fn list_intersects_workspace_and_client_request_identity() {
    let sandbox = Sandbox::new("list-workspace-request-intersection", 5000);
    let target_request = "request:list-workspace-request:shared";
    let target_workspace = "workspace:list-workspace-request:target";
    for index in 0..3 {
        let mut list_request = request(&sandbox, target_request, 8);
        list_request.plan.principal = format!("principal:list-workspace-request:{index}");
        list_request.plan.workspace_id = if index == 1 {
            target_workspace.to_string()
        } else {
            format!("workspace:list-workspace-request:other:{index}")
        };
        sandbox.registry.submit(&list_request).unwrap();
    }

    let result = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 100,
            cursor: None,
            client_request_id: Some(target_request.to_string()),
            workspace_id: Some(target_workspace.to_string()),
        })
        .unwrap();
    assert_eq!(result.jobs.len(), 1);
    assert_eq!(result.jobs[0].client_request_id, target_request);
    assert_eq!(result.jobs[0].workspace_id, target_workspace);
}

#[test]
fn filtered_list_paginates_same_request_across_principals() {
    let sandbox = Sandbox::new("list-client-request-pagination", 5000);
    let target_id = "request:list-client-request:shared";
    for index in 0..3 {
        let mut list_request = request(&sandbox, target_id, 8);
        list_request.plan.principal = format!("principal:list-client-request:{index}");
        list_request.plan.workspace_id =
            format!("workspace:list-client-request-pagination:{index}");
        sandbox.registry.submit(&list_request).unwrap();
    }

    let mut cursor = None;
    let mut observed = Vec::new();
    loop {
        let page = sandbox
            .registry
            .list_jobs(&RuntimeJobListRequest {
                limit: 1,
                cursor,
                client_request_id: Some(target_id.to_string()),
                workspace_id: None,
            })
            .unwrap();
        observed.extend(page.jobs);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(observed.len(), 3);
    assert!(observed
        .iter()
        .all(|job| job.client_request_id == target_id));
    let unique: BTreeSet<_> = observed.iter().map(|job| &job.job_id).collect();
    assert_eq!(unique.len(), 3);
}

#[test]
fn list_rejects_invalid_client_request_filter() {
    let sandbox = Sandbox::new("list-client-request-invalid", 5000);
    let error = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 100,
            cursor: None,
            client_request_id: Some(String::new()),
            workspace_id: None,
        })
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("clientRequestId"));
}

#[test]
fn list_rejects_invalid_workspace_filter() {
    let sandbox = Sandbox::new("list-workspace-invalid", 5000);
    let error = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 100,
            cursor: None,
            client_request_id: None,
            workspace_id: Some(String::new()),
        })
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("workspaceId"));
}

#[test]
fn runner_binding_converges_exact_duplicate_identity_without_duplicate_event() {
    let sandbox = Sandbox::new("runner-bind-idempotent", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:runner-bind-idempotent", 1))
            .unwrap(),
    );
    let ready = sandbox
        .registry
        .mark_bundle_ready(&created.attempt.attempt_id, 0, &digest(b"bundle"), 10)
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(&ready.attempt_id, ready.row_version, 11)
        .unwrap();
    let stale_row_version = starting.row_version;
    let identity = RunnerIdentity {
        boot_id: "boot:test".to_string(),
        unit_name: starting.unit_name.clone(),
        invocation_id: "invocation:test".to_string(),
        control_group: "/system.slice/ordivon-test.service".to_string(),
        main_pid: 42,
        process_start_identity: "start:42".to_string(),
        runner_start_digest: digest(b"runner-start"),
        observed_at_ms: 12,
    };
    let first = sandbox
        .registry
        .bind_running(&starting.attempt_id, stale_row_version, &identity)
        .unwrap();
    assert_eq!(first.state, AttemptState::Running);

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let bound_events_before: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE job_id=?1 AND event_type='RUNNER_BOUND'",
            [&created.job.job_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    assert_eq!(bound_events_before, 1);

    let duplicate = sandbox
        .registry
        .bind_running(&starting.attempt_id, stale_row_version, &identity)
        .unwrap();
    assert_eq!(duplicate, first);

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let bound_events_after: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE job_id=?1 AND event_type='RUNNER_BOUND'",
            [&created.job.job_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    assert_eq!(bound_events_after, 1);

    let mut conflicting_identity = identity;
    conflicting_identity.invocation_id = "invocation:different".to_string();
    let error = sandbox
        .registry
        .bind_running(
            &starting.attempt_id,
            stale_row_version,
            &conflicting_identity,
        )
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::AttemptStateConflict);
}

#[test]
fn terminal_commit_is_atomic_idempotent_and_releases_capacity() {
    let sandbox = Sandbox::new("terminal", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:terminal", 1))
            .unwrap(),
    );
    let attempt = sandbox
        .registry
        .mark_bundle_ready(&created.attempt.attempt_id, 0, &digest(b"bundle"), 10)
        .unwrap();
    let attempt = sandbox
        .registry
        .mark_dispatch_issued(&attempt.attempt_id, attempt.row_version, 11)
        .unwrap();
    let attempt = sandbox
        .registry
        .bind_running(
            &attempt.attempt_id,
            attempt.row_version,
            &RunnerIdentity {
                boot_id: "boot:test".to_string(),
                unit_name: attempt.unit_name.clone(),
                invocation_id: "invocation:test".to_string(),
                control_group: "/system.slice/ordivon-test.service".to_string(),
                main_pid: 42,
                process_start_identity: "start:42".to_string(),
                runner_start_digest: digest(b"runner-start"),
                observed_at_ms: 12,
            },
        )
        .unwrap();
    let terminal = TerminalCommit {
        attempt_id: attempt.attempt_id.clone(),
        expected_row_version: attempt.row_version,
        state: AttemptState::Succeeded,
        result_digest: digest(b"result"),
        exit_code: Some(0),
        infrastructure_error_digest: None,
        finished_at_ms: 13,
        artifacts: vec![ArtifactRegistration {
            artifact_id: "artifact:stdout".to_string(),
            kind: "stdout".to_string(),
            relative_path: "stdout.log".to_string(),
            digest: digest(b"output"),
            media_type: "text/plain".to_string(),
            byte_length: 6,
            truncated: false,
        }],
        reason_code: "PROCESS_EXIT_ZERO".to_string(),
    };
    let projection = sandbox.registry.commit_terminal(&terminal).unwrap();
    assert_eq!(projection.status, "succeeded");
    assert_eq!(projection.attempt_state, Some(AttemptState::Succeeded));
    assert!(projection.execution_terminal);
    assert_eq!(
        projection.execution_disposition,
        Some(JobResolution::Succeeded)
    );
    assert_eq!(
        projection.execution_reason_code.as_deref(),
        Some("PROCESS_EXIT_ZERO")
    );
    assert_eq!(
        projection.delivery_disposition,
        RuntimeDeliveryDisposition::Committed
    );
    assert!(!projection.recovery_required);
    assert!(!projection.semantic_completion_evaluated);
    assert!(projection.result_available);
    assert!(projection.artifacts_available);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert_eq!(
        sandbox
            .registry
            .list_artifacts(&created.job.job_id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        sandbox.registry.commit_terminal(&terminal).unwrap(),
        projection
    );

    let mut conflict = terminal;
    conflict.result_digest = digest(b"different-result");
    let error = sandbox.registry.commit_terminal(&conflict).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ResultIdentityConflict);
}

#[test]
fn stopping_attempt_accepts_verified_success_and_releases_capacity() {
    let sandbox = Sandbox::new("stopping-success", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:stopping-success", 1))
            .unwrap(),
    );
    let attempt = sandbox
        .registry
        .mark_bundle_ready(&created.attempt.attempt_id, 0, &digest(b"bundle"), 10)
        .unwrap();
    let attempt = sandbox
        .registry
        .mark_dispatch_issued(&attempt.attempt_id, attempt.row_version, 11)
        .unwrap();
    let attempt = sandbox
        .registry
        .bind_running(
            &attempt.attempt_id,
            attempt.row_version,
            &RunnerIdentity {
                boot_id: "boot:test".to_string(),
                unit_name: attempt.unit_name.clone(),
                invocation_id: "invocation:test".to_string(),
                control_group: "/system.slice/ordivon-test.service".to_string(),
                main_pid: 42,
                process_start_identity: "start:42".to_string(),
                runner_start_digest: digest(b"runner-start"),
                observed_at_ms: 12,
            },
        )
        .unwrap();
    sandbox
        .registry
        .request_cancel(&created.job.job_id, 13)
        .unwrap();
    let stopping = sandbox.registry.get_attempt(&attempt.attempt_id).unwrap();
    assert_eq!(stopping.state, AttemptState::Stopping);

    let projection = sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: stopping.attempt_id.clone(),
            expected_row_version: stopping.row_version,
            state: AttemptState::Succeeded,
            result_digest: digest(b"completed-before-stop"),
            exit_code: Some(0),
            infrastructure_error_digest: None,
            finished_at_ms: 14,
            artifacts: Vec::new(),
            reason_code: "PROCESS_COMPLETED_BEFORE_STOP_EFFECTIVE".to_string(),
        })
        .unwrap();

    assert_eq!(projection.status, "succeeded");
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert_eq!(
        sandbox
            .registry
            .get_reservation(&stopping.attempt_id)
            .unwrap()
            .state,
        ReservationState::Released
    );
}

#[test]
fn runtime_job_inspection_projects_bounded_read_only_timeline() {
    let sandbox = Sandbox::new("inspection-job", 5000);
    let expected_source_digest = digest(b"inspection-source-state");
    let mut inspection_request = request(&sandbox, "request:inspection-job", 4);
    inspection_request.plan.workspace_source_digest = Some(expected_source_digest.clone());
    let created = created(sandbox.registry.submit(&inspection_request).unwrap());
    let base = created.job.created_at_ms;
    let ready = sandbox
        .registry
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"inspection-bundle"),
            base + 10,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(&ready.attempt_id, ready.row_version, base + 11)
        .unwrap();
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: starting.attempt_id.clone(),
            expected_row_version: starting.row_version,
            state: AttemptState::Failed,
            result_digest: digest(b"inspection-result"),
            exit_code: Some(7),
            infrastructure_error_digest: None,
            finished_at_ms: base + 20,
            artifacts: Vec::new(),
            reason_code: "PROCESS_EXIT_NONZERO".to_string(),
        })
        .unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    let before = fs::read(&sandbox.registry.config().db_path).unwrap();

    let bounded = inspect_job(&inspection_config(&sandbox), &created.job.job_id, 3, false).unwrap();
    assert_eq!(bounded.schema_version, RUNTIME_INSPECTION_SCHEMA_VERSION);
    assert_eq!(bounded.timeline.len(), 3);
    assert!(bounded.events_truncated);
    assert!(bounded.timeline.iter().all(|event| event.detail.is_none()));

    let full = inspect_job(
        &inspection_config(&sandbox),
        &created.job.job_id,
        MAX_INSPECTION_EVENT_LIMIT,
        true,
    )
    .unwrap();
    assert_eq!(full.job.resolution, Some(JobResolution::Failed));
    assert_eq!(full.job.source_revision, "test-revision");
    assert_eq!(
        full.job.workspace_source_digest.as_deref(),
        Some(expected_source_digest.as_str())
    );
    assert!(full.job.mechanically_converged);
    assert!(!full.job.semantic_completion_evaluated);
    assert_eq!(full.attempts.len(), 1);
    assert_eq!(full.attempts[0].state, AttemptState::Failed);
    assert_eq!(
        full.attempts[0].reservation_state,
        ReservationState::Released
    );
    assert_eq!(full.episodes.dispatches, 1);
    assert_eq!(full.episodes.duplicate_dispatches, 0);
    assert!(!full.events_truncated);
    assert!(full.timeline.iter().all(|event| event.detail.is_some()));
    assert_eq!(
        before,
        fs::read(&sandbox.registry.config().db_path).unwrap()
    );

    let terminal = sandbox.registry.get_attempt(&starting.attempt_id).unwrap();
    let failure = RuntimeError::new(
        RuntimeErrorCode::AttemptStateConflict,
        "terminal evidence still needs review",
        Some("attemptId"),
        false,
    );
    sandbox
        .registry
        .record_reconciliation_failure(&terminal, &failure, base + 21)
        .unwrap();
    let recovery_projection = sandbox.registry.project_job(&created.job.job_id).unwrap();
    assert!(recovery_projection.execution_terminal);
    assert_eq!(
        recovery_projection.attempt_state,
        Some(AttemptState::Failed)
    );
    assert!(recovery_projection.recovery_required);
    assert_eq!(
        recovery_projection.execution_disposition,
        Some(JobResolution::Failed)
    );
    assert_eq!(
        recovery_projection.delivery_disposition,
        RuntimeDeliveryDisposition::ReconciliationRequired
    );
    assert_eq!(recovery_projection.poll_after_ms, Some(250));
    assert!(!recovery_projection.semantic_completion_evaluated);
    let attention = inspect_job(
        &inspection_config(&sandbox),
        &created.job.job_id,
        MAX_INSPECTION_EVENT_LIMIT,
        false,
    )
    .unwrap();
    assert!(!attention.job.mechanically_converged);
    let summary = summarize_experience(&inspection_config(&sandbox), 0).unwrap();
    assert_eq!(summary.jobs.recovery_required, 1);
    assert_eq!(summary.jobs.capacity_held, 0);
    assert_eq!(summary.jobs.converged, 0);
}

#[test]
fn runtime_experience_summary_reports_only_mechanical_facts() {
    let sandbox = Sandbox::new("inspection-summary", 5000);
    let first = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:inspection-summary-first", 4))
            .unwrap(),
    );
    let first_base = first.job.created_at_ms;
    let ready = sandbox
        .registry
        .mark_bundle_ready(
            &first.attempt.attempt_id,
            first.attempt.row_version,
            &digest(b"summary-bundle"),
            first_base + 10,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(&ready.attempt_id, ready.row_version, first_base + 11)
        .unwrap();
    let failure = RuntimeError::new(
        RuntimeErrorCode::AttemptStateConflict,
        "summary recovery episode",
        Some("attemptId"),
        false,
    );
    sandbox
        .registry
        .record_reconciliation_failure(&starting, &failure, first_base + 12)
        .unwrap();
    sandbox
        .registry
        .clear_reconciliation_failure(&starting.attempt_id, first_base + 13)
        .unwrap();
    let current = sandbox.registry.get_attempt(&starting.attempt_id).unwrap();
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: current.attempt_id.clone(),
            expected_row_version: current.row_version,
            state: AttemptState::Failed,
            result_digest: digest(b"summary-result"),
            exit_code: Some(1),
            infrastructure_error_digest: None,
            finished_at_ms: first_base + 20,
            artifacts: Vec::new(),
            reason_code: "PROCESS_EXIT_NONZERO".to_string(),
        })
        .unwrap();

    let mut second_request = request(&sandbox, "request:inspection-summary-second", 4);
    second_request.plan.workspace_id = "workspace:inspection-summary-second".to_string();
    let second = created(sandbox.registry.submit(&second_request).unwrap());
    sandbox
        .registry
        .request_cancel(&second.job.job_id, second.job.created_at_ms + 30)
        .unwrap();

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let next_sequence: u64 = connection
        .query_row(
            "SELECT MAX(event_sequence)+1 FROM job_events WHERE job_id=?1",
            [&first.job.job_id],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO job_events(event_id,job_id,attempt_id,event_sequence,event_type,origin,previous_state,new_state,reason_code,detail_json,detail_digest,observed_at_ms) VALUES(?1,?2,?3,?4,'DISPATCH_ISSUED','SYSTEM_DERIVED','starting','starting','AT_MOST_ONCE_BOUNDARY_COMMITTED','{}',?5,?6)",
            rusqlite::params![
                format!("event-{}", Uuid::now_v7()),
                first.job.job_id,
                first.attempt.attempt_id,
                next_sequence,
                digest(b"{}"),
                first_base + 14
            ],
        )
        .unwrap();
    drop(connection);

    let summary = summarize_experience(&inspection_config(&sandbox), 0).unwrap();
    assert_eq!(summary.jobs.total, 2);
    assert_eq!(summary.jobs.converged, 2);
    assert_eq!(summary.jobs.unresolved, 0);
    assert_eq!(summary.jobs.recovery_required, 0);
    assert_eq!(summary.jobs.capacity_held, 0);
    assert_eq!(summary.jobs.convergence_rate_basis_points, 10_000);
    assert_eq!(summary.recovery.jobs_with_reconciliation_failure, 1);
    assert_eq!(summary.recovery.jobs_with_automatic_recovery, 1);
    assert_eq!(
        summary.recovery.automatic_recovery_rate_basis_points,
        10_000
    );
    assert_eq!(summary.dispatch.dispatches, 2);
    assert_eq!(summary.dispatch.jobs_with_duplicate_dispatch, 1);
    assert_eq!(summary.dispatch.duplicate_dispatches, 1);
    assert_eq!(summary.cancellation.requested, 1);
    assert_eq!(summary.cancellation.resolved_cancelled, 1);
    assert_eq!(summary.cancellation.unresolved, 0);
    assert_eq!(
        summary.mechanical_latency_ms.admission_to_dispatch.samples,
        1
    );
    assert_eq!(
        summary.mechanical_latency_ms.admission_to_dispatch.p50,
        Some(11)
    );
    assert_eq!(
        summary
            .mechanical_latency_ms
            .reconciliation_to_convergence
            .samples,
        1
    );
    assert_eq!(
        summary
            .mechanical_latency_ms
            .reconciliation_to_convergence
            .p50,
        Some(1)
    );
    assert_eq!(
        summary
            .mechanical_latency_ms
            .dispatch_to_runner_bound
            .samples,
        0
    );
    assert_eq!(summary.resolutions.get("failed"), Some(&1));
    assert_eq!(summary.resolutions.get("cancelled"), Some(&1));
    assert!(!summary.semantic_completion_evaluated);
}

#[test]
fn runtime_doctor_is_read_only_and_fingerprint_is_stable() {
    let sandbox = Sandbox::new("doctor-read-only", 5000);
    let before = fs::read(&sandbox.registry.config().db_path).unwrap();

    let first = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    let second = inspect_runtime(&doctor_config(&sandbox)).unwrap();

    assert_eq!(first.schema_version, 2);
    assert_eq!(first.summary.status, "healthy");
    assert_eq!(first.summary.jobs_total, 0);
    assert_eq!(first.summary.unresolved_jobs, 0);
    assert!(first.summary.capacity_holders.is_empty());
    assert_eq!(first.violation_count, 0);
    assert!(first.cases.is_empty());
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(
        before,
        fs::read(&sandbox.registry.config().db_path).unwrap()
    );
}

#[test]
fn runtime_doctor_summarizes_capacity_holders() {
    let sandbox = Sandbox::new("doctor-summary", 5000);
    let admission = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:doctor-summary", 4))
            .unwrap(),
    );
    let report = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(report.summary.status, "healthy");
    assert_eq!(report.summary.jobs_total, 1);
    assert_eq!(report.summary.unresolved_jobs, 1);
    assert_eq!(report.summary.reservations_by_state.get("active"), Some(&1));
    assert_eq!(report.summary.capacity_holders.len(), 1);
    assert!(!report.summary.capacity_holders_truncated);
    assert_eq!(
        report.summary.capacity_holders[0].job_id,
        admission.job.job_id
    );
    assert_eq!(
        report.summary.capacity_holders[0].reservation_state,
        ReservationState::Active
    );
}

#[test]
fn runtime_doctor_marks_capacity_holder_projection_incomplete() {
    let sandbox = Sandbox::new("doctor-capacity-truncation", 5000);
    for index in 0..51 {
        let mut request = request(
            &sandbox,
            &format!("request:doctor-capacity-truncation:{index}"),
            64,
        );
        request.plan.workspace_id = format!("workspace:doctor-capacity:{index}");
        created(sandbox.registry.submit(&request).unwrap());
    }
    let report = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(report.summary.capacity_holders.len(), 50);
    assert!(report.summary.capacity_holders_truncated);
}

#[test]
fn runtime_doctor_proposes_verified_runner_result_recovery() {
    let sandbox = Sandbox::new("doctor-runner-result", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:doctor-runner-result", 1))
            .unwrap(),
    );
    write_completed_runner_result(&created.attempt, 42);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);

    let report = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(report.cases.len(), 1);
    assert!(report.violation_count >= 3);
    match &report.cases[0].proposal {
        RuntimeDoctorProposal::RecoverRunnerResult { terminal } => {
            assert_eq!(terminal.state, AttemptState::Succeeded);
            assert_eq!(terminal.exit_code, Some(0));
            assert_eq!(terminal.finished_at_ms, 42);
            assert_eq!(terminal.artifacts.len(), 3);
        }
        proposal => panic!("unexpected Doctor proposal: {proposal:?}"),
    }
}

#[test]
fn runtime_doctor_does_not_guess_when_runner_result_is_missing() {
    let sandbox = Sandbox::new("doctor-manual", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:doctor-manual", 1))
            .unwrap(),
    );
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);

    let report = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    match &report.cases[0].proposal {
        RuntimeDoctorProposal::ManualReview { reasons } => {
            assert!(reasons
                .iter()
                .any(|reason| reason.contains("result digest")));
            assert!(reasons
                .iter()
                .any(|reason| reason.contains("Runner result")));
        }
        proposal => panic!("unexpected Doctor proposal: {proposal:?}"),
    }
}

#[test]
fn runtime_doctor_does_not_follow_noncanonical_bundle_paths() {
    let sandbox = Sandbox::new("doctor-bundle-boundary", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:doctor-bundle-boundary", 1))
            .unwrap(),
    );
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost',bundle_path='/tmp/not-an-ordivon-bundle' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);

    let report = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    match &report.cases[0].proposal {
        RuntimeDoctorProposal::ManualReview { reasons } => assert!(reasons
            .iter()
            .any(|reason| reason.contains("outside the canonical Registry store"))),
        proposal => panic!("unexpected Doctor proposal: {proposal:?}"),
    }
}

#[test]
fn runtime_doctor_proposes_release_for_complete_terminal_evidence() {
    let sandbox = Sandbox::new("doctor-release", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:doctor-release", 1))
            .unwrap(),
    );
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Cancelled,
            result_digest: digest(b"doctor-control"),
            exit_code: None,
            infrastructure_error_digest: None,
            finished_at_ms: 50,
            artifacts: Vec::new(),
            reason_code: "TEST_CANCELLED".to_string(),
        })
        .unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE concurrency_reservations SET state='active',released_at_ms=NULL,release_reason=NULL WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    drop(connection);

    let report = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert!(matches!(
        report.cases[0].proposal,
        RuntimeDoctorProposal::ReleaseTerminalReservation
    ));
}

#[test]
fn runtime_repair_recovers_runner_truth_and_explicitly_finalizes_lost() {
    let sandbox = Sandbox::new("repair-complete", 5000);
    let recover = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-recover", 2))
            .unwrap(),
    );
    let mut manual_request = request(&sandbox, "request:repair-manual", 2);
    manual_request.plan.workspace_id = "workspace:repair-manual".to_string();
    let manual = created(sandbox.registry.submit(&manual_request).unwrap());
    write_completed_runner_result(&recover.attempt, 70);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    for created in [&recover, &manual] {
        connection
            .execute(
                "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
                [&created.attempt.attempt_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
                [&created.job.job_id],
            )
            .unwrap();
    }
    drop(connection);
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(before.cases.len(), 2);
    let snapshot = write_test_snapshot(&sandbox, "complete");
    let report = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::from([manual.attempt.attempt_id.clone()]),
        },
    )
    .unwrap();

    assert_eq!(report.actions.len(), 2);
    assert_eq!(report.after.violation_count, 0);
    assert_eq!(
        sandbox
            .registry
            .get_attempt(&recover.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Succeeded
    );
    let manual_attempt = sandbox
        .registry
        .get_attempt(&manual.attempt.attempt_id)
        .unwrap();
    assert_eq!(manual_attempt.state, AttemptState::Lost);
    assert!(manual_attempt.result_digest.is_some());
    assert!(manual_attempt.finished_at_ms.is_some());
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert!(sandbox
        .registry
        .get_job(&recover.job.job_id)
        .unwrap()
        .current_attempt_id
        .is_none());
    assert!(sandbox
        .registry
        .get_job(&manual.job.job_id)
        .unwrap()
        .current_attempt_id
        .is_none());

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let repair_events: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE event_type='ADMIN_TERMINAL_REPAIR'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let receipts: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE kind='admin_repair'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let terminal_evidence: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE kind='terminal_evidence'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(repair_events, 2);
    assert_eq!(receipts, 2);
    assert_eq!(terminal_evidence, 2);
}

#[test]
fn runtime_repair_batch_rolls_back_every_case_on_late_conflict() {
    let sandbox = Sandbox::new("repair-atomic", 5000);
    let first = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-atomic-first", 2))
            .unwrap(),
    );
    let mut second_request = request(&sandbox, "request:repair-atomic-second", 2);
    second_request.plan.workspace_id = "workspace:repair-atomic-second".to_string();
    let second = created(sandbox.registry.submit(&second_request).unwrap());
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    for created in [&first, &second] {
        connection
            .execute(
                "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
                [&created.attempt.attempt_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
                [&created.job.job_id],
            )
            .unwrap();
    }
    drop(connection);

    let audit = |created: &CreatedAdmission, expected_job_row_version: u64| AdminRepairAudit {
        report_fingerprint: digest(b"atomic-report"),
        case_fingerprint: digest(created.attempt.attempt_id.as_bytes()),
        snapshot_path: "/tmp/atomic-snapshot".to_string(),
        snapshot_digest: digest(b"atomic-snapshot"),
        principal: "runtime-admin:test".to_string(),
        action: "recover_runner_result".to_string(),
        observed_at_ms: 90,
        expected_job_row_version,
        expected_current_attempt_id: Some(created.attempt.attempt_id.clone()),
        expected_reservation_state: ReservationState::Active,
    };
    let terminal = |created: &CreatedAdmission| TerminalCommit {
        attempt_id: created.attempt.attempt_id.clone(),
        expected_row_version: created.attempt.row_version,
        state: AttemptState::Succeeded,
        result_digest: digest(created.job.job_id.as_bytes()),
        exit_code: Some(0),
        infrastructure_error_digest: None,
        finished_at_ms: 90,
        artifacts: Vec::new(),
        reason_code: "PROCESS_EXIT_ZERO".to_string(),
    };
    let operations = vec![
        AdminRepairOperation::Terminal {
            terminal: terminal(&first),
            audit: audit(&first, first.job.row_version),
        },
        AdminRepairOperation::Terminal {
            terminal: terminal(&second),
            audit: audit(&second, second.job.row_version + 1),
        },
    ];

    let error = sandbox
        .registry
        .repair_admin_batch(&operations)
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    for created in [&first, &second] {
        let attempt = sandbox
            .registry
            .get_attempt(&created.attempt.attempt_id)
            .unwrap();
        assert_eq!(attempt.state, AttemptState::Lost);
        assert!(attempt.result_digest.is_none());
        assert_eq!(
            sandbox
                .registry
                .get_reservation(&created.attempt.attempt_id)
                .unwrap()
                .state,
            ReservationState::Active
        );
    }
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let event_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE event_type='ADMIN_TERMINAL_REPAIR'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let artifact_count: u32 = connection
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(event_count, 0);
    assert_eq!(artifact_count, 0);
}

#[test]
fn runtime_repair_batch_rolls_back_when_any_invariant_remains() {
    let sandbox = Sandbox::new("repair-precommit-invariant", 5000);
    let repairable = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-precommit-repairable", 2))
            .unwrap(),
    );
    let mut unrelated_request = request(&sandbox, "request:repair-precommit-unrelated", 2);
    unrelated_request.plan.workspace_id = "workspace:repair-precommit-unrelated".to_string();
    let unrelated = created(sandbox.registry.submit(&unrelated_request).unwrap());
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&repairable.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&repairable.job.job_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET current_attempt_id=NULL WHERE job_id=?1",
            [&unrelated.job.job_id],
        )
        .unwrap();
    drop(connection);

    let operation = AdminRepairOperation::Terminal {
        terminal: TerminalCommit {
            attempt_id: repairable.attempt.attempt_id.clone(),
            expected_row_version: repairable.attempt.row_version,
            state: AttemptState::Succeeded,
            result_digest: digest(b"precommit-result"),
            exit_code: Some(0),
            infrastructure_error_digest: None,
            finished_at_ms: 95,
            artifacts: Vec::new(),
            reason_code: "PROCESS_EXIT_ZERO".to_string(),
        },
        audit: AdminRepairAudit {
            report_fingerprint: digest(b"precommit-report"),
            case_fingerprint: digest(b"precommit-case"),
            snapshot_path: "/tmp/precommit-snapshot".to_string(),
            snapshot_digest: digest(b"precommit-snapshot"),
            principal: "runtime-admin:test".to_string(),
            action: "recover_runner_result".to_string(),
            observed_at_ms: 95,
            expected_job_row_version: repairable.job.row_version,
            expected_current_attempt_id: Some(repairable.attempt.attempt_id.clone()),
            expected_reservation_state: ReservationState::Active,
        },
    };

    let error = sandbox
        .registry
        .repair_admin_batch(&[operation])
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    let attempt = sandbox
        .registry
        .get_attempt(&repairable.attempt.attempt_id)
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Lost);
    assert!(attempt.result_digest.is_none());
    assert_eq!(
        sandbox
            .registry
            .get_reservation(&repairable.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::Active
    );
}

#[test]
fn runtime_repair_requires_every_manual_case_to_be_explicit() {
    let sandbox = Sandbox::new("repair-explicit", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-explicit", 1))
            .unwrap(),
    );
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    let snapshot = write_test_snapshot(&sandbox, "explicit");
    let error = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
}

#[test]
fn runtime_repair_rejects_stale_fingerprint_before_writes() {
    let sandbox = Sandbox::new("repair-stale", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-stale", 1))
            .unwrap(),
    );
    write_completed_runner_result(&created.attempt, 80);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    let snapshot = write_test_snapshot(&sandbox, "stale");
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET row_version=row_version+1 WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    drop(connection);

    let error = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    assert!(sandbox
        .registry
        .get_attempt(&created.attempt.attempt_id)
        .unwrap()
        .result_digest
        .is_none());
}

#[test]
fn runtime_repair_rejects_unscoped_invariants_before_writes() {
    let sandbox = Sandbox::new("repair-unscoped", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-unscoped", 1))
            .unwrap(),
    );
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE jobs SET current_attempt_id=NULL WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    assert_eq!(before.violation_count, 1);
    assert!(before.cases.is_empty());
    let snapshot = write_test_snapshot(&sandbox, "unscoped");

    let error = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    assert!(sandbox
        .registry
        .get_job(&created.job.job_id)
        .unwrap()
        .current_attempt_id
        .is_none());
}

#[test]
fn runtime_repair_rejects_snapshot_that_does_not_match_doctor_state() {
    let sandbox = Sandbox::new("repair-snapshot-state", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:repair-snapshot-state", 1))
            .unwrap(),
    );
    let snapshot = write_test_snapshot(&sandbox, "state-mismatch");
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();

    let error = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::from([created.attempt.attempt_id.clone()]),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    assert!(sandbox
        .registry
        .get_attempt(&created.attempt.attempt_id)
        .unwrap()
        .result_digest
        .is_none());
}

#[test]
fn runtime_repair_does_not_apply_schema_migrations() {
    let root = std::env::temp_dir().join(format!(
        "ordivon-repair-v2-{}-{}",
        std::process::id(),
        Uuid::now_v7()
    ));
    let store = root.join("store");
    fs::create_dir_all(store.join("attempts")).unwrap();
    let db_path = store.join("registry.sqlite3");
    let connection = Connection::open(&db_path).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/runtime/0001_runtime.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(1,'0001_runtime',?1,0)",
            [RUNTIME_MIGRATION_CHECKSUM],
        )
        .unwrap();
    connection
        .execute_batch(include_str!(
            "../../migrations/runtime/0002_orphan_recovery.sql"
        ))
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(2,'0002_orphan_recovery',?1,0)",
            [RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM],
        )
        .unwrap();
    drop(connection);
    let doctor = RuntimeDoctorConfig {
        db_path: db_path.clone(),
        store_root: store,
        busy_timeout_ms: 5_000,
    };
    let before = inspect_runtime(&doctor).unwrap();
    assert_eq!(before.migration_version, 2);

    let error = apply_runtime_repair(
        &RuntimeRepairConfig { doctor },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: root.join("unused-snapshot"),
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::SchemaVersionUnsupported);
    let connection = Connection::open(&db_path).unwrap();
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 2);
    drop(connection);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn runtime_repair_rejects_incomplete_control_snapshot() {
    let sandbox = Sandbox::new("repair-incomplete-snapshot", 5000);
    let snapshot = write_test_snapshot(&sandbox, "incomplete-control");
    let manifest_path = snapshot.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "path": "control/attempts/missing/result.json",
            "bytes": 1,
            "digest": digest(b"x"),
        }));
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();

    let error = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt);
}

#[test]
fn runtime_repair_rejects_corrupt_snapshot() {
    let sandbox = Sandbox::new("repair-snapshot", 5000);
    let snapshot = write_test_snapshot(&sandbox, "corrupt");
    fs::write(snapshot.join("registry.sqlite3"), b"corrupt").unwrap();
    let before = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    let error = apply_runtime_repair(
        &RuntimeRepairConfig {
            doctor: doctor_config(&sandbox),
        },
        &RuntimeRepairRequest {
            expected_fingerprint: before.fingerprint,
            snapshot_path: snapshot,
            principal: "runtime-admin:test".to_string(),
            finalize_lost_attempt_ids: BTreeSet::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt);
}

#[test]
fn reconciliation_receipts_change_only_when_the_condition_changes() {
    let sandbox = Sandbox::new("reconciliation-receipts", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:reconciliation-receipts", 1))
            .unwrap(),
    );
    let error = RuntimeError::new(
        RuntimeErrorCode::ReconciliationRequired,
        "synthetic recovery requirement",
        Some("attemptId"),
        false,
    );

    sandbox
        .registry
        .record_reconciliation_failure(&created.attempt, &error, 30)
        .unwrap();
    sandbox
        .registry
        .record_reconciliation_failure(&created.attempt, &error, 31)
        .unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let failed_events: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE attempt_id=?1 AND event_type='RECONCILIATION_FAILED'",
            [&created.attempt.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(failed_events, 1);
    drop(connection);

    sandbox
        .registry
        .clear_reconciliation_failure(&created.attempt.attempt_id, 32)
        .unwrap();
    sandbox
        .registry
        .clear_reconciliation_failure(&created.attempt.attempt_id, 33)
        .unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let converged_events: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE attempt_id=?1 AND event_type='RECONCILIATION_CONVERGED'",
            [&created.attempt.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    let status: String = connection
        .query_row(
            "SELECT status FROM attempt_conditions WHERE attempt_id=?1 AND condition_type='recovery_required'",
            [&created.attempt.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(converged_events, 1);
    assert_eq!(status, "false");
}

#[test]
fn evidence_complete_terminal_reservation_converges_and_clears_invariants() {
    let sandbox = Sandbox::new("terminal-reservation-convergence", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(
                &sandbox,
                "request:terminal-reservation-convergence",
                1,
            ))
            .unwrap(),
    );
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Cancelled,
            result_digest: digest(b"terminal-result"),
            exit_code: None,
            infrastructure_error_digest: None,
            finished_at_ms: 20,
            artifacts: Vec::new(),
            reason_code: "TEST_CANCELLED".to_string(),
        })
        .unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE concurrency_reservations SET state='active',released_at_ms=NULL,release_reason=NULL WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    drop(connection);

    let before = sandbox.registry.inspect_runtime_invariants().unwrap();
    assert!(before
        .iter()
        .any(|violation| violation.code == "TERMINAL_ATTEMPT_HOLDS_RESERVATION"));
    assert!(sandbox
        .registry
        .converge_terminal_reservation(&created.attempt.attempt_id, 21)
        .unwrap());
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert!(sandbox
        .registry
        .inspect_runtime_invariants()
        .unwrap()
        .is_empty());
}

#[test]
fn incomplete_terminal_evidence_is_not_guessed_or_released() {
    let sandbox = Sandbox::new("incomplete-terminal", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:incomplete-terminal", 1))
            .unwrap(),
    );
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET state='lost' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&created.job.job_id],
        )
        .unwrap();
    drop(connection);

    let error = sandbox
        .registry
        .converge_terminal_reservation(&created.attempt.attempt_id, 22)
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ReconciliationRequired);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
    let violations = sandbox.registry.inspect_runtime_invariants().unwrap();
    assert!(violations
        .iter()
        .any(|violation| violation.code == "TERMINAL_ATTEMPT_MISSING_EVIDENCE"));
    assert!(violations
        .iter()
        .any(|violation| violation.code == "RESOLVED_JOB_WITH_CURRENT_ATTEMPT"));
}

#[test]
fn cancel_before_dispatch_resolves_without_launch() {
    let sandbox = Sandbox::new("cancel-accepted", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:cancel", 1))
            .unwrap(),
    );
    let projection = sandbox
        .registry
        .request_cancel(&created.job.job_id, 20)
        .unwrap();
    assert_eq!(projection.status, "cancelled");
    assert_eq!(projection.desired_state, JobDesiredState::Cancelled);
    assert_eq!(
        projection.execution_disposition,
        Some(JobResolution::Cancelled)
    );
    assert_eq!(
        projection.execution_reason_code.as_deref(),
        Some("CANCELLED_BEFORE_DISPATCH")
    );
    assert_eq!(
        projection.delivery_disposition,
        RuntimeDeliveryDisposition::Committed
    );
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert_eq!(
        sandbox
            .registry
            .get_attempt(&created.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Cancelled
    );
}

#[test]
fn orphaned_terminal_keeps_capacity_reserved() {
    let sandbox = Sandbox::new("orphaned", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:orphaned", 1))
            .unwrap(),
    );
    let terminal = TerminalCommit {
        attempt_id: created.attempt.attempt_id.clone(),
        expected_row_version: 0,
        state: AttemptState::Orphaned,
        result_digest: digest(b"identity-mismatch"),
        exit_code: None,
        infrastructure_error_digest: Some(digest(b"identity-mismatch")),
        finished_at_ms: 21,
        artifacts: Vec::new(),
        reason_code: "LAUNCH_IDENTITY_MISMATCH".to_string(),
    };
    let projection = sandbox.registry.commit_terminal(&terminal).unwrap();
    assert_eq!(projection.status, "orphaned");
    assert_eq!(projection.attempt_state, Some(AttemptState::Orphaned));
    assert!(projection.execution_terminal);
    assert_eq!(
        projection.execution_disposition,
        Some(JobResolution::Orphaned)
    );
    assert_eq!(
        projection.delivery_disposition,
        RuntimeDeliveryDisposition::ReconciliationRequired
    );
    assert!(projection.recovery_required);
    assert!(!projection.artifacts_available);
    assert_eq!(projection.poll_after_ms, Some(250));
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
    assert_eq!(
        sandbox
            .registry
            .get_reservation(&created.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::HeldOrphaned
    );
}

#[test]
fn orphaned_cancel_intent_is_persisted_without_unsafe_release() {
    let sandbox = Sandbox::new("orphan-cancel-intent", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:orphan-cancel-intent", 1))
            .unwrap(),
    );
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Orphaned,
            result_digest: digest(b"orphan-control"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"identity-uncertain")),
            finished_at_ms: 20,
            artifacts: Vec::new(),
            reason_code: "SUPERVISOR_IDENTITY_ORPHANED".to_string(),
        })
        .unwrap();

    let projection = sandbox
        .registry
        .request_cancel(&created.job.job_id, 21)
        .unwrap();
    assert_eq!(projection.status, "orphaned");
    assert_eq!(projection.desired_state, JobDesiredState::Cancelled);
    assert_eq!(projection.attempt_state, Some(AttemptState::Orphaned));
    assert_eq!(
        projection.termination_intent,
        Some(AttemptTerminationIntent::StopRequested)
    );
    assert!(projection.recovery_required);
    assert_eq!(
        projection.delivery_disposition,
        RuntimeDeliveryDisposition::ReconciliationRequired
    );
    assert_eq!(
        sandbox
            .registry
            .get_job(&created.job.job_id)
            .unwrap()
            .desired_state,
        JobDesiredState::Cancelled
    );
    assert_eq!(
        sandbox
            .registry
            .get_attempt(&created.attempt.attempt_id)
            .unwrap()
            .termination_intent,
        AttemptTerminationIntent::StopRequested
    );
    assert_eq!(
        sandbox
            .registry
            .get_reservation(&created.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::HeldOrphaned
    );
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
}

#[test]
fn absent_orphan_can_converge_to_lost_and_release_capacity() {
    let sandbox = Sandbox::new("orphan-lost-convergence", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:orphan-lost-convergence", 1))
            .unwrap(),
    );
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Orphaned,
            result_digest: digest(b"orphan-control"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"identity-uncertain")),
            finished_at_ms: 20,
            artifacts: Vec::new(),
            reason_code: "SUPERVISOR_IDENTITY_ORPHANED".to_string(),
        })
        .unwrap();
    let orphaned = sandbox
        .registry
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();

    let projection = sandbox
        .registry
        .recover_orphaned_terminal(&TerminalCommit {
            attempt_id: orphaned.attempt_id.clone(),
            expected_row_version: orphaned.row_version,
            state: AttemptState::Lost,
            result_digest: digest(b"orphan-process-tree-gone"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"process-tree-gone")),
            finished_at_ms: 21,
            artifacts: Vec::new(),
            reason_code: "ORPHANED_PROCESS_TREE_GONE".to_string(),
        })
        .unwrap();

    assert_eq!(projection.status, "lost");
    assert_eq!(projection.attempt_state, Some(AttemptState::Lost));
    assert!(projection.execution_terminal);
    assert_eq!(projection.execution_disposition, Some(JobResolution::Lost));
    assert_eq!(
        projection.execution_reason_code.as_deref(),
        Some("ORPHANED_PROCESS_TREE_GONE")
    );
    assert_eq!(
        projection.delivery_disposition,
        RuntimeDeliveryDisposition::Unknown
    );
    assert!(!projection.recovery_required);
    assert_eq!(projection.poll_after_ms, None);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert_eq!(
        sandbox
            .registry
            .get_reservation(&created.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::Released
    );
    assert_eq!(
        sandbox
            .registry
            .get_attempt(&created.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Lost
    );
    assert_eq!(
        sandbox
            .registry
            .get_job(&created.job.job_id)
            .unwrap()
            .resolution,
        Some(JobResolution::Lost)
    );
}

#[test]
fn runtime_startup_reclaims_absent_orphan_and_reopens_workspace_slot() {
    let sandbox = Sandbox::new("runtime-orphan-startup", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:runtime-orphan-startup", 1))
            .unwrap(),
    );
    fs::create_dir_all(&created.attempt.bundle_path).unwrap();
    fs::write(
        Path::new(&created.attempt.bundle_path).join("stdout.log"),
        b"partial\n",
    )
    .unwrap();
    fs::write(
        Path::new(&created.attempt.bundle_path).join("stderr.log"),
        b"",
    )
    .unwrap();
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Orphaned,
            result_digest: digest(b"runtime-orphan-control"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"identity-uncertain")),
            finished_at_ms: 20,
            artifacts: Vec::new(),
            reason_code: "SUPERVISOR_IDENTITY_ORPHANED".to_string(),
        })
        .unwrap();
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);

    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let attempt = runtime
        .registry()
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Lost);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    assert_eq!(
        runtime
            .registry()
            .get_reservation(&created.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::Released
    );
    assert!(Path::new(&attempt.bundle_path)
        .join("orphan-remediation.json")
        .is_file());
    runtime
        .registry()
        .get_artifact(
            &created.job.job_id,
            &format!("{}.orphan-remediation", created.attempt.attempt_id),
        )
        .unwrap();

    let admitted = runtime
        .registry()
        .submit(&request(&sandbox, "request:after-orphan-reclaim", 1))
        .unwrap();
    assert!(matches!(admitted, AdmissionOutcome::Created(_)));
}

#[test]
fn task_cancel_reclaims_absent_orphan_as_cancelled() {
    let sandbox = Sandbox::new("runtime-orphan-cancel", 5000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let created = created(
        runtime
            .registry()
            .submit(&request(&sandbox, "request:runtime-orphan-cancel", 1))
            .unwrap(),
    );
    fs::create_dir_all(&created.attempt.bundle_path).unwrap();
    fs::write(
        Path::new(&created.attempt.bundle_path).join("stdout.log"),
        b"partial\n",
    )
    .unwrap();
    fs::write(
        Path::new(&created.attempt.bundle_path).join("stderr.log"),
        b"",
    )
    .unwrap();
    runtime
        .registry()
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Orphaned,
            result_digest: digest(b"runtime-orphan-cancel-control"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"identity-uncertain")),
            finished_at_ms: 20,
            artifacts: Vec::new(),
            reason_code: "SUPERVISOR_IDENTITY_ORPHANED".to_string(),
        })
        .unwrap();

    let observation = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id.clone(),
        })
        .unwrap();
    assert_eq!(observation.status, "cancelled");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    assert_eq!(
        runtime
            .registry()
            .get_attempt(&created.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Cancelled
    );
    assert_eq!(
        runtime
            .registry()
            .get_job(&created.job.job_id)
            .unwrap()
            .desired_state,
        JobDesiredState::Cancelled
    );
    assert_eq!(
        runtime
            .registry()
            .get_reservation(&created.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::Released
    );

    let replay = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id.clone(),
        })
        .unwrap();
    assert_eq!(replay.status, "cancelled");
}

#[test]
fn newer_schema_and_checksum_drift_fail_closed() {
    let newer = Sandbox::new("newer-schema", 5000);
    let connection = Connection::open(&newer.registry.config().db_path).unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,'future','sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',0)",
            [5],
        )
        .unwrap();
    drop(connection);
    let error = Registry::initialize(newer.registry.config().clone()).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::SchemaVersionUnsupported);

    let drift = Sandbox::new("checksum-drift", 5000);
    let connection = Connection::open(&drift.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE schema_migrations SET checksum='sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE version=1",
            [],
        )
        .unwrap();
    drop(connection);
    let error = Registry::initialize(drift.registry.config().clone()).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::MigrationChecksumMismatch);

    let terminal_drift = Sandbox::new("terminal-checksum-drift", 5000);
    let connection = Connection::open(&terminal_drift.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE schema_migrations SET checksum='sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc' WHERE version=3",
            [],
        )
        .unwrap();
    drop(connection);
    let error = Registry::initialize(terminal_drift.registry.config().clone()).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::MigrationChecksumMismatch);

    let reclaim_drift = Sandbox::new("orphan-reclaim-checksum-drift", 5000);
    let connection = Connection::open(&reclaim_drift.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE schema_migrations SET checksum='sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd' WHERE version=4",
            [],
        )
        .unwrap();
    drop(connection);
    let error = Registry::initialize(reclaim_drift.registry.config().clone()).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::MigrationChecksumMismatch);
}

#[test]
fn query_indexes_are_recreated_without_advancing_schema_version() {
    let sandbox = Sandbox::new("query-index-recreate", 5000);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    for index in [
        "idx_jobs_client_request_id_created",
        "idx_jobs_workspace_created",
        "idx_artifacts_job",
    ] {
        connection
            .execute(&format!("DROP INDEX {index}"), [])
            .unwrap();
    }
    connection
        .execute(
            "CREATE INDEX idx_events_job_sequence ON job_events(job_id,event_sequence)",
            [],
        )
        .unwrap();
    drop(connection);

    Registry::initialize(sandbox.registry.config().clone()).unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 4);
    for index in [
        "idx_jobs_client_request_id_created",
        "idx_jobs_workspace_created",
        "idx_artifacts_job",
    ] {
        let actual: String = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='index' AND name=?1",
                [index],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(actual, index);
    }
    let redundant_event_index_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_events_job_sequence')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!redundant_event_index_exists);
}

#[test]
fn workspace_patch_storage_is_recreated_without_advancing_schema_version() {
    let sandbox = Sandbox::new("workspace-patch-storage-recreate", 5000);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute("DROP TABLE workspace_patch_operations", [])
        .unwrap();
    drop(connection);

    Registry::initialize(sandbox.registry.config().clone()).unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    let table: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='workspace_patch_operations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let index: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_workspace_patch_operations_workspace'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(max_version, 4);
    assert_eq!(table, "workspace_patch_operations");
    assert_eq!(index, "idx_workspace_patch_operations_workspace");
}

#[test]
fn event_log_is_append_only_and_terminal_trigger_blocks_reopen() {
    let sandbox = Sandbox::new("triggers", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:triggers", 1))
            .unwrap(),
    );
    sandbox
        .registry
        .request_cancel(&created.job.job_id, 30)
        .unwrap();
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let event_id: String = connection
        .query_row("SELECT event_id FROM job_events LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(connection
        .execute(
            "UPDATE job_events SET reason_code='tampered' WHERE event_id=?1",
            [&event_id],
        )
        .is_err());
    assert!(connection
        .execute(
            "UPDATE attempts SET state='running' WHERE attempt_id=?1",
            [&created.attempt.attempt_id],
        )
        .is_err());
}

#[test]
fn reconciliation_only_treats_registry_wide_failures_as_fatal() {
    let database_corrupt = RuntimeError::new(
        RuntimeErrorCode::RegistryCorrupt,
        "database image is malformed",
        None,
        false,
    );
    assert!(database_corrupt.is_reconciliation_fatal());

    let result_corrupt = RuntimeError::new(
        RuntimeErrorCode::RegistryCorrupt,
        "invalid Runner result",
        Some("result"),
        false,
    );
    assert!(!result_corrupt.is_reconciliation_fatal());

    let job_conflict = RuntimeError::new(
        RuntimeErrorCode::JobAlreadyResolved,
        "Job is already resolved",
        Some("jobId"),
        false,
    );
    assert!(!job_conflict.is_reconciliation_fatal());

    let unavailable = RuntimeError::new(
        RuntimeErrorCode::RegistryUnavailable,
        "cannot open Registry",
        None,
        false,
    );
    assert!(unavailable.is_reconciliation_fatal());
}

#[test]
fn corrupt_database_fails_closed() {
    let sandbox = Sandbox::new("corrupt", 5000);
    fs::write(&sandbox.registry.config().db_path, b"not a sqlite database").unwrap();
    let error = Registry::initialize(sandbox.registry.config().clone()).unwrap_err();
    assert!(matches!(
        error.code,
        RuntimeErrorCode::RegistryCorrupt | RuntimeErrorCode::RegistryUnavailable
    ));
}

#[test]
fn workspace_execution_is_serialized_with_retry_guidance() {
    let sandbox = Sandbox::new("workspace-capacity", 5000);
    let first = request(&sandbox, "request:workspace:first", 4);
    sandbox.registry.submit(&first).unwrap();

    let error = sandbox
        .registry
        .submit(&request(&sandbox, "request:workspace:second", 4))
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ConcurrencyLimit);
    assert_eq!(error.field.as_deref(), Some("workspaceId"));
    assert_eq!(error.retry_after_ms, Some(1_000));
    let capacity = error.capacity.unwrap();
    assert_eq!(capacity.scope, "workspace");
    assert_eq!(capacity.active, 1);
    assert_eq!(capacity.limit, 1);
    assert_eq!(capacity.workspace_id.as_deref(), Some("workspace:test"));
    assert_eq!(capacity.holder_job_ids.len(), 1);
    assert_eq!(capacity.holder_workspace_ids, vec!["workspace:test"]);
    assert!(!capacity.holders_truncated);

    let mut other = request(&sandbox, "request:workspace:other", 4);
    other.plan.workspace_id = "workspace:other".to_string();
    assert!(matches!(
        sandbox.registry.submit(&other).unwrap(),
        AdmissionOutcome::Created(_)
    ));
}

#[test]
fn global_execution_capacity_reports_cross_workspace_usage() {
    let sandbox = Sandbox::new("global-capacity", 5000);
    let first = request(&sandbox, "request:global:first", 1);
    sandbox.registry.submit(&first).unwrap();

    let mut second = request(&sandbox, "request:global:second", 1);
    second.plan.workspace_id = "workspace:second".to_string();
    let error = sandbox.registry.submit(&second).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ConcurrencyLimit);
    assert_eq!(error.field.as_deref(), Some("globalLimit"));
    assert_eq!(error.retry_after_ms, Some(1_000));
    let capacity = error.capacity.unwrap();
    assert_eq!(capacity.scope, "global");
    assert_eq!(capacity.active, 1);
    assert_eq!(capacity.limit, 1);
    assert_eq!(capacity.workspace_id, None);
    assert_eq!(capacity.holder_job_ids.len(), 1);
    assert_eq!(capacity.holder_workspace_ids, vec!["workspace:test"]);
    assert!(!capacity.holders_truncated);
}

#[test]
fn global_capacity_marks_bounded_holder_projection_incomplete() {
    let sandbox = Sandbox::new("global-capacity-truncated", 5000);
    for index in 0..17 {
        let mut admitted = request(&sandbox, &format!("request:global:holder:{index}"), 17);
        admitted.plan.workspace_id = format!("workspace:holder:{index}");
        assert!(matches!(
            sandbox.registry.submit(&admitted).unwrap(),
            AdmissionOutcome::Created(_)
        ));
    }

    let mut rejected = request(&sandbox, "request:global:rejected", 17);
    rejected.plan.workspace_id = "workspace:rejected".to_string();
    let error = sandbox.registry.submit(&rejected).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::ConcurrencyLimit);
    assert_eq!(error.field.as_deref(), Some("globalLimit"));
    let capacity = error.capacity.unwrap();
    assert_eq!(capacity.scope, "global");
    assert_eq!(capacity.active, 17);
    assert_eq!(capacity.limit, 17);
    assert_eq!(capacity.holder_job_ids.len(), 16);
    assert_eq!(capacity.holder_workspace_ids.len(), 16);
    assert!(capacity.holders_truncated);
}

#[test]
fn late_identity_bound_result_corrects_orphan_and_releases_capacity() {
    let sandbox = Sandbox::new("orphan-recovery", 5000);
    let created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:orphan-recovery", 1))
            .unwrap(),
    );
    sandbox
        .registry
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Orphaned,
            result_digest: digest(b"control-orphan"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"identity-uncertain")),
            finished_at_ms: 20,
            artifacts: Vec::new(),
            reason_code: "SUPERVISOR_IDENTITY_ORPHANED".to_string(),
        })
        .unwrap();
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);

    let orphaned = sandbox
        .registry
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();
    let recovered = sandbox
        .registry
        .recover_orphaned_terminal(&TerminalCommit {
            attempt_id: orphaned.attempt_id.clone(),
            expected_row_version: orphaned.row_version,
            state: AttemptState::Succeeded,
            result_digest: digest(b"late-runner-result"),
            exit_code: Some(0),
            infrastructure_error_digest: None,
            finished_at_ms: 21,
            artifacts: Vec::new(),
            reason_code: "LATE_IDENTITY_BOUND_RUNNER_RESULT".to_string(),
        })
        .unwrap();

    assert_eq!(recovered.status, "succeeded");
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    assert_eq!(
        sandbox
            .registry
            .get_reservation(&created.attempt.attempt_id)
            .unwrap()
            .state,
        ReservationState::Released
    );
    assert_eq!(
        sandbox
            .registry
            .get_attempt(&created.attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Succeeded
    );
    assert_eq!(
        sandbox
            .registry
            .get_job(&created.job.job_id)
            .unwrap()
            .resolution,
        Some(JobResolution::Succeeded)
    );
}

#[test]
fn existing_v1_registry_upgrades_and_ensures_lookup_index() {
    let root = std::env::temp_dir().join(format!(
        "ordivon-v1-upgrade-{}-{}",
        std::process::id(),
        Uuid::now_v7()
    ));
    let store = root.join("store");
    fs::create_dir_all(&store).unwrap();
    let db_path = store.join("registry.sqlite3");
    let connection = Connection::open(&db_path).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/runtime/0001_runtime.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(1,'0001_runtime',?1,0)",
            [RUNTIME_MIGRATION_CHECKSUM],
        )
        .unwrap();
    drop(connection);

    let registry = Registry::initialize(RegistryConfig {
        db_path: db_path.clone(),
        store_root: store,
        busy_timeout_ms: 5000,
    })
    .unwrap();
    let connection = Connection::open(registry.config().db_path.clone()).unwrap();
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 4);
    let checksum: String = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(checksum, RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM);
    let repair_checksum: String = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(repair_checksum, RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM);
    let reclaim_checksum: String = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=4",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(reclaim_checksum, RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM);
    let artifact_job_index: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_artifacts_job'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(artifact_job_index, "idx_artifacts_job");
    let patch_table: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='workspace_patch_operations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(patch_table, "workspace_patch_operations");
    let patch_index: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_workspace_patch_operations_workspace'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(patch_index, "idx_workspace_patch_operations_workspace");
    let lookup_index: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_jobs_client_request_id_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lookup_index, "idx_jobs_client_request_id_created");
    let workspace_lookup_index: String = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_jobs_workspace_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(workspace_lookup_index, "idx_jobs_workspace_created");
    drop(connection);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn execution_provider_commitment_binds_operation_without_changing_plan_shape() {
    let sandbox = Sandbox::new("execution-provider-commitment", 5_000);
    let provider = ExecutionProviderSnapshot {
        contract: ExecutionProviderContract::LocalLinuxRunnerV1,
        executable_digest: digest(b"runner-v1"),
        wsl_distribution: None,
    };
    let mut submission = request(&sandbox, "request:execution-provider", 4);
    submission.request_identity_digest = Some(format!(
        "{}{}",
        REQUEST_IDENTITY_PREFIX,
        digest(b"provider-bound-proposal")
    ));
    submission.execution_provider = Some(provider.clone());

    let created = created(sandbox.registry.submit(&submission).unwrap());
    let plan: serde_json::Value = serde_json::from_str(&created.job.execution_plan_json).unwrap();
    assert!(plan.get("executionProvider").is_none());
    assert_eq!(
        sandbox
            .registry
            .execution_provider(&created.job.job_id)
            .unwrap(),
        Some(provider.clone())
    );

    let provider_json = serde_json::to_string(&provider).unwrap();
    let provider_digest = digest(provider_json.as_bytes());
    let workspace_snapshot: serde_json::Value =
        serde_json::from_str(&created.job.workspace_snapshot_json).unwrap();
    assert_eq!(
        workspace_snapshot["executionProviderDigest"],
        provider_digest
    );
    let expected_operation = digest(
        format!(
            "runtime-operation-v4\0{}\0{}\0{}",
            created.job.request_digest, created.job.execution_plan_digest, provider_digest
        )
        .as_bytes(),
    );
    assert_eq!(created.job.operation_digest, expected_operation);
}

#[test]
fn committed_execution_provider_missing_or_tampered_side_truth_fails_closed() {
    for mode in ["missing", "tampered"] {
        let sandbox = Sandbox::new(&format!("execution-provider-{mode}"), 5_000);
        let provider = ExecutionProviderSnapshot {
            contract: ExecutionProviderContract::LocalLinuxRunnerV1,
            executable_digest: digest(b"runner-v1"),
            wsl_distribution: None,
        };
        let mut submission = request(&sandbox, &format!("request:provider-{mode}"), 4);
        submission.execution_provider = Some(provider);
        let created = created(sandbox.registry.submit(&submission).unwrap());
        let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
        match mode {
            "missing" => {
                connection
                    .execute(
                        "DELETE FROM job_execution_providers WHERE job_id=?1",
                        [&created.job.job_id],
                    )
                    .unwrap();
            }
            "tampered" => {
                connection
                    .execute(
                        "UPDATE job_execution_providers SET snapshot_json='{}' WHERE job_id=?1",
                        [&created.job.job_id],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let error = sandbox
            .registry
            .execution_provider(&created.job.job_id)
            .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt, "{mode}");
    }
}

#[test]
fn host_dependency_commitment_binds_operation_v6_without_changing_plan_shape() {
    let sandbox = Sandbox::new("host-dependency-commitment", 5_000);
    let provider = ExecutionProviderSnapshot {
        contract: ExecutionProviderContract::LocalLinuxRunnerV1,
        executable_digest: digest(b"runner-v1"),
        wsl_distribution: None,
    };
    let dependency = HostDependencyBinding {
        path: "/opt/ordivon/runtime-dependency.so".to_string(),
        expected_digest: digest(b"dependency-v1"),
    };
    let mut submission = request(&sandbox, "request:host-dependency", 4);
    submission.request_identity_digest = Some(format!(
        "{}{}",
        REQUEST_IDENTITY_PREFIX,
        digest(b"host-dependency-proposal")
    ));
    submission.execution_provider = Some(provider.clone());
    submission.host_dependencies = vec![dependency.clone()];
    let created = created(sandbox.registry.submit(&submission).unwrap());
    let plan: serde_json::Value = serde_json::from_str(&created.job.execution_plan_json).unwrap();
    assert!(plan.get("hostDependencies").is_none());
    assert_eq!(
        sandbox
            .registry
            .host_dependencies(&created.job.job_id)
            .unwrap(),
        vec![dependency.clone()]
    );
    let provider_digest = digest(serde_json::to_string(&provider).unwrap().as_bytes());
    let host_digest = digest(serde_json::to_string(&vec![dependency]).unwrap().as_bytes());
    let snapshot: serde_json::Value =
        serde_json::from_str(&created.job.workspace_snapshot_json).unwrap();
    assert_eq!(snapshot["hostDependenciesDigest"], host_digest);
    assert_eq!(
        created.job.operation_digest,
        digest(
            format!(
                "runtime-operation-v6\0{}\0{}\0{}\0{}",
                created.job.request_digest,
                created.job.execution_plan_digest,
                provider_digest,
                host_digest
            )
            .as_bytes()
        )
    );
}

#[test]
fn committed_host_dependency_missing_or_tampered_side_truth_fails_closed() {
    for mode in ["missing", "tampered"] {
        let sandbox = Sandbox::new(&format!("host-dependency-{mode}"), 5_000);
        let mut submission = request(&sandbox, &format!("request:host-dependency-{mode}"), 4);
        submission.execution_provider = Some(ExecutionProviderSnapshot {
            contract: ExecutionProviderContract::LocalLinuxRunnerV1,
            executable_digest: digest(b"runner-v1"),
            wsl_distribution: None,
        });
        submission.host_dependencies = vec![HostDependencyBinding {
            path: "/opt/ordivon/runtime-dependency.so".to_string(),
            expected_digest: digest(b"dependency-v1"),
        }];
        let created = created(sandbox.registry.submit(&submission).unwrap());
        let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
        match mode {
            "missing" => {
                connection
                    .execute(
                        "DELETE FROM job_host_dependencies WHERE job_id=?1",
                        [&created.job.job_id],
                    )
                    .unwrap();
            }
            "tampered" => {
                connection
                    .execute(
                        "UPDATE job_host_dependencies SET bindings_json='[]' WHERE job_id=?1",
                        [&created.job.job_id],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let error = sandbox
            .registry
            .host_dependencies(&created.job.job_id)
            .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt, "{mode}");
        if mode == "missing" {
            let error = sandbox
                .registry
                .execution_provider(&created.job.job_id)
                .unwrap_err();
            assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt, "{mode}");
        } else {
            assert!(sandbox
                .registry
                .execution_provider(&created.job.job_id)
                .unwrap()
                .is_some());
        }
    }
}

#[test]
fn host_dependency_storage_is_recreated_without_advancing_schema_version() {
    let sandbox = Sandbox::new("host-dependency-storage", 5_000);
    let config = sandbox.registry.config().clone();
    let connection = Connection::open(&config.db_path).unwrap();
    connection
        .execute("DROP TABLE job_host_dependencies", [])
        .unwrap();
    drop(connection);
    let registry = Registry::initialize(config).unwrap();
    let connection = Connection::open(&registry.config().db_path).unwrap();
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='job_host_dependencies')",
        [], |row| row.get(0)).unwrap();
    assert!(exists);
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 4);
}

#[test]
fn execution_provider_contract_must_match_execution_target() {
    let sandbox = Sandbox::new("execution-provider-target", 5_000);
    let mut submission = request(&sandbox, "request:provider-target", 4);
    submission.execution_provider = Some(ExecutionProviderSnapshot {
        contract: ExecutionProviderContract::WindowsNativeLauncherV1,
        executable_digest: digest(b"launcher"),
        wsl_distribution: Some("archlinux".to_string()),
    });
    let error = sandbox.registry.submit(&submission).unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::InvalidRequest);
    assert_eq!(error.field.as_deref(), Some("executionProvider.contract"));
}

#[test]
fn execution_provider_storage_is_recreated_without_advancing_schema_version() {
    let sandbox = Sandbox::new("execution-provider-storage", 5_000);
    let config = sandbox.registry.config().clone();
    let connection = Connection::open(&config.db_path).unwrap();
    connection
        .execute("DROP TABLE job_execution_providers", [])
        .unwrap();
    drop(connection);

    let registry = Registry::initialize(config).unwrap();
    let connection = Connection::open(&registry.config().db_path).unwrap();
    let table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='job_execution_providers')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(table_exists);
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 4);
}

#[test]
fn attempt_supervisor_owner_binding_is_atomic_idempotent_and_tamper_evident() {
    let sandbox = Sandbox::new("attempt-supervisor-owner-binding", 5_000);
    let created = created(
        sandbox
            .registry
            .submit(&request(
                &sandbox,
                "request:attempt-supervisor-owner-binding",
                1,
            ))
            .unwrap(),
    );
    let ready = sandbox
        .registry
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"bundle"),
            10,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(&ready.attempt_id, ready.row_version, 11)
        .unwrap();
    let start_evidence_digest = digest(b"windows-start");
    let owner = AttemptSupervisorOwner::WindowsLauncherV1 {
        launcher_process_id: 4242,
        launcher_process_creation_time_file_time: 123_456_789,
        launcher_image_digest: digest(b"launcher-image"),
        job_name: format!("Ordivon.{}", starting.attempt_id),
        start_evidence_digest: start_evidence_digest.clone(),
    };
    let running = sandbox
        .registry
        .bind_supervisor_owner(&starting.attempt_id, starting.row_version, &owner, 12)
        .unwrap();
    assert_eq!(running.state, AttemptState::Running);
    assert_eq!(
        running.runner_start_digest.as_deref(),
        Some(start_evidence_digest.as_str())
    );
    assert!(running.boot_id.is_none());
    assert!(running.invocation_id.is_none());
    assert!(running.control_group.is_none());
    assert!(running.main_pid.is_none());
    assert!(running.process_start_identity.is_none());
    assert_eq!(
        sandbox
            .registry
            .attempt_supervisor_owner(&starting.attempt_id)
            .unwrap(),
        Some(owner.clone())
    );

    let replay = sandbox
        .registry
        .bind_supervisor_owner(&starting.attempt_id, starting.row_version, &owner, 99)
        .unwrap();
    assert_eq!(replay.row_version, running.row_version);

    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE attempt_supervisor_owners SET owner_json='{}' WHERE attempt_id=?1",
            [&starting.attempt_id],
        )
        .unwrap();
    let error = sandbox
        .registry
        .attempt_supervisor_owner(&starting.attempt_id)
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::RegistryCorrupt);
}

#[test]
fn native_windows_pre_target_evidence_gap_preserves_unknown_no_redrive_semantics() {
    let error = native_windows_pre_target_evidence_gap();
    assert_eq!(error.code, RuntimeErrorCode::LaunchIdentityMismatch);
    assert_eq!(error.field.as_deref(), Some("windowsStart"));
    assert!(error.retryable);
    assert!(error.message.contains("target execution is unknown"));
    assert!(error.message.contains("do not redrive automatically"));
    assert!(!error.message.contains("could not have"));
}

#[test]
fn outer_deadline_intent_is_durable_replay_safe_and_does_not_override_cancel() {
    let sandbox = Sandbox::new("outer-deadline-intent", 5_000);
    let admission = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "request:outer-deadline-intent", 1))
            .unwrap(),
    );
    let ready = sandbox
        .registry
        .mark_bundle_ready(
            &admission.attempt.attempt_id,
            admission.attempt.row_version,
            &digest(b"bundle"),
            10,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(&ready.attempt_id, ready.row_version, 11)
        .unwrap();

    let deadline = sandbox
        .registry
        .request_deadline_termination(&starting.attempt_id, 12)
        .unwrap();
    assert_eq!(deadline.state, AttemptState::Stopping);
    assert_eq!(
        deadline.termination_intent,
        AttemptTerminationIntent::DeadlineExceeded
    );
    assert_eq!(
        sandbox
            .registry
            .get_job(&admission.job.job_id)
            .unwrap()
            .desired_state,
        JobDesiredState::Run
    );
    let replay = sandbox
        .registry
        .request_deadline_termination(&deadline.attempt_id, 99)
        .unwrap();
    assert_eq!(replay.row_version, deadline.row_version);
    assert_eq!(
        replay.termination_intent,
        AttemptTerminationIntent::DeadlineExceeded
    );

    let cancel_sandbox = Sandbox::new("cancel-before-outer-deadline", 5_000);
    let cancelled = created(
        cancel_sandbox
            .registry
            .submit(&request(
                &cancel_sandbox,
                "request:cancel-wins-before-deadline",
                2,
            ))
            .unwrap(),
    );
    let cancel_ready = cancel_sandbox
        .registry
        .mark_bundle_ready(
            &cancelled.attempt.attempt_id,
            cancelled.attempt.row_version,
            &digest(b"cancel-bundle"),
            20,
        )
        .unwrap();
    let cancel_starting = cancel_sandbox
        .registry
        .mark_dispatch_issued(&cancel_ready.attempt_id, cancel_ready.row_version, 21)
        .unwrap();
    cancel_sandbox
        .registry
        .request_cancel(&cancelled.job.job_id, 22)
        .unwrap();
    let after_deadline = cancel_sandbox
        .registry
        .request_deadline_termination(&cancel_starting.attempt_id, 23)
        .unwrap();
    assert_eq!(after_deadline.state, AttemptState::Stopping);
    assert_eq!(
        after_deadline.termination_intent,
        AttemptTerminationIntent::StopRequested
    );
}

#[test]
fn stopping_attempt_can_bind_native_supervisor_owner_without_reentering_running() {
    let sandbox = Sandbox::new("stopping-attempt-supervisor-owner", 5_000);
    let created = created(
        sandbox
            .registry
            .submit(&request(
                &sandbox,
                "request:stopping-attempt-supervisor-owner",
                1,
            ))
            .unwrap(),
    );
    let ready = sandbox
        .registry
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"bundle"),
            10,
        )
        .unwrap();
    let starting = sandbox
        .registry
        .mark_dispatch_issued(&ready.attempt_id, ready.row_version, 11)
        .unwrap();
    sandbox
        .registry
        .request_cancel(&created.job.job_id, 12)
        .unwrap();
    let stopping = sandbox.registry.get_attempt(&starting.attempt_id).unwrap();
    assert_eq!(stopping.state, AttemptState::Stopping);
    assert_eq!(
        stopping.termination_intent,
        AttemptTerminationIntent::StopRequested
    );
    let start_evidence_digest = digest(b"windows-start-after-cancel");
    let owner = AttemptSupervisorOwner::WindowsLauncherV1 {
        launcher_process_id: 4243,
        launcher_process_creation_time_file_time: 223_456_789,
        launcher_image_digest: digest(b"launcher-image"),
        job_name: format!("Ordivon.{}", stopping.attempt_id),
        start_evidence_digest: start_evidence_digest.clone(),
    };
    let bound = sandbox
        .registry
        .bind_supervisor_owner(&stopping.attempt_id, stopping.row_version, &owner, 13)
        .unwrap();
    assert_eq!(bound.state, AttemptState::Stopping);
    assert_eq!(
        bound.termination_intent,
        AttemptTerminationIntent::StopRequested
    );
    assert_eq!(
        bound.runner_start_digest.as_deref(),
        Some(start_evidence_digest.as_str())
    );
    assert_eq!(
        sandbox
            .registry
            .attempt_supervisor_owner(&bound.attempt_id)
            .unwrap(),
        Some(owner.clone())
    );
    let replay = sandbox
        .registry
        .bind_supervisor_owner(&bound.attempt_id, stopping.row_version, &owner, 99)
        .unwrap();
    assert_eq!(replay.state, AttemptState::Stopping);
    assert_eq!(replay.row_version, bound.row_version);
}

#[test]
fn attempt_supervisor_owner_storage_is_recreated_without_advancing_schema_version() {
    let sandbox = Sandbox::new("attempt-supervisor-owner-storage", 5_000);
    let config = sandbox.registry.config().clone();
    let connection = Connection::open(&config.db_path).unwrap();
    connection
        .execute("DROP TABLE attempt_supervisor_owners", [])
        .unwrap();
    drop(connection);

    let registry = Registry::initialize(config).unwrap();
    let connection = Connection::open(&registry.config().db_path).unwrap();
    let table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='attempt_supervisor_owners')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(table_exists);
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 4);
}

#[test]
fn runtime_release_effect_binds_operation_and_receipt_truth_overrides_job_progress() {
    let sandbox = Sandbox::new("runtime-release-effect", 5_000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let release_request = RuntimeReleaseRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: "request:runtime-release-effect".to_string(),
        principal: "principal:test".to_string(),
        workspace_id: "workspace:test".to_string(),
        commit: "a".repeat(40),
        candidate_manifest_digest: digest(b"candidate-manifest"),
        expected_tool_count: 22,
    };
    let request_digest = runtime_release_request_identity_digest(&release_request).unwrap();
    let effect_id = runtime_release_effect_id(&release_request);
    let receipt = sandbox.root.join(format!("effect-{effect_id}"));
    let binding = RuntimeReleaseEffectBinding {
        contract: RuntimeReleaseContract::RuntimeReleaseV1,
        effect_id: effect_id.clone(),
        request_digest: request_digest.clone(),
        workspace_id: release_request.workspace_id.clone(),
        commit: release_request.commit.clone(),
        candidate_manifest_digest: release_request.candidate_manifest_digest.clone(),
        expected_tool_count: release_request.expected_tool_count,
        receipt_path: receipt.to_string_lossy().into_owned(),
    };
    let provider = ExecutionProviderSnapshot {
        contract: ExecutionProviderContract::LocalLinuxRunnerV1,
        executable_digest: file_digest(Path::new("/usr/bin/true")),
        wsl_distribution: None,
    };
    let mut submission = request(&sandbox, &release_request.client_request_id, 4);
    submission.request_identity_digest = Some(request_digest.clone());
    submission.execution_provider = Some(provider.clone());
    submission.runtime_release_effect = Some(binding.clone());
    let created = created(sandbox.registry.submit(&submission).unwrap());

    assert_eq!(
        sandbox
            .registry
            .runtime_release_effect_for_job(&created.job.job_id)
            .unwrap(),
        Some(binding.clone())
    );
    let provider_digest = digest(serde_json::to_string(&provider).unwrap().as_bytes());
    let release_digest = digest(serde_json::to_string(&binding).unwrap().as_bytes());
    assert_eq!(
        created.job.operation_digest,
        digest(
            format!(
                "runtime-operation-v5\0{}\0{}\0{}\0{}",
                request_digest, created.job.execution_plan_digest, provider_digest, release_digest
            )
            .as_bytes()
        )
    );

    fs::create_dir_all(&receipt).unwrap();
    let release_effect_json = serde_json::json!({
        "contract": "runtime_release_v1",
        "effectId": effect_id,
        "requestDigest": request_digest,
        "commit": release_request.commit,
        "candidateManifestDigest": release_request.candidate_manifest_digest,
        "expectedToolCount": release_request.expected_tool_count,
        "operatorOwnedExtra": "ignored-by-projection",
    });
    fs::write(
        receipt.join("effect-request.json"),
        serde_json::to_vec_pretty(&release_effect_json).unwrap(),
    )
    .unwrap();
    fs::write(
        receipt.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 2,
            "status": "deployed",
            "commit": release_request.commit,
            "releaseEffect": release_effect_json,
            "probe": {
                "toolCount": release_request.expected_tool_count,
                "toolCatalogDigest": digest(b"catalog"),
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let projection = runtime
        .get_runtime_release_effect(&RuntimeReleaseGetRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            principal: release_request.principal.clone(),
            client_request_id: release_request.client_request_id.clone(),
        })
        .unwrap();
    assert_eq!(
        projection.effect_disposition,
        RuntimeReleaseDisposition::Deployed
    );
    assert!(projection.effect_terminal);
    assert!(projection.receipt_available);
    assert!(projection.receipt_digest.is_some());
    assert_eq!(projection.deployed_tool_count, Some(22));
    assert_eq!(projection.attempt_state, Some(AttemptState::Accepted));
    assert!(!projection.execution_terminal);
    assert!(!projection.semantic_completion_evaluated);

    let replay = runtime
        .find_runtime_release_for_apply(&release_request)
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.release.job_id, created.job.job_id);
    assert_eq!(
        replay.release.effect_disposition,
        RuntimeReleaseDisposition::Deployed
    );

    let mut conflict = release_request.clone();
    conflict.candidate_manifest_digest = digest(b"different-manifest");
    let error = runtime
        .find_runtime_release_for_apply(&conflict)
        .unwrap_err();
    assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);
}

#[test]
fn runtime_release_storage_is_recreated_without_advancing_schema_version() {
    let sandbox = Sandbox::new("runtime-release-storage", 5_000);
    let config = sandbox.registry.config().clone();
    let connection = Connection::open(&config.db_path).unwrap();
    connection
        .execute("DROP TABLE job_runtime_release_effects", [])
        .unwrap();
    drop(connection);

    let registry = Registry::initialize(config).unwrap();
    let connection = Connection::open(&registry.config().db_path).unwrap();
    let table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='job_runtime_release_effects')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(table_exists);
    let index_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_runtime_release_effect_request')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(index_exists);
    let max_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(max_version, 4);
}

#[test]
fn runtime_capabilities_project_current_affordances_without_input_authority_paths() {
    let sandbox = Sandbox::new("runtime-capabilities", 5_000);
    let input_root = sandbox.root.join("private-input-authority");
    fs::create_dir_all(&input_root).unwrap();
    let runtime = Runtime::new_with_input_authorities(
        runtime_config(&sandbox),
        vec![InputAuthority {
            name: "finance-state".to_string(),
            root: input_root.clone(),
        }],
    )
    .unwrap();

    let capabilities = runtime.capabilities();
    assert_eq!(capabilities.schema_version, RUNTIME_SCHEMA_VERSION);
    assert_eq!(capabilities.max_runtime_ms, 60_000);
    assert_eq!(capabilities.max_output_bytes, 1_048_576);
    assert_eq!(capabilities.allowed_executable_roots, vec!["/".to_string()]);
    assert_eq!(capabilities.input_authorities, vec!["finance-state"]);
    assert_eq!(capabilities.targets.len(), 2);

    let linux = capabilities
        .targets
        .iter()
        .find(|target| target.target == ExecutionTarget::LocalLinux)
        .unwrap();
    assert!(linux.configured);
    assert!(linux.available);
    assert_eq!(
        linux.execution_profiles,
        vec![
            ExecutionProfile::TrustedLocal,
            ExecutionProfile::ContainedLocal
        ]
    );
    assert!(linux.structured_plan);
    assert!(linux.immutable_inputs);
    assert!(linux.windows_authorities.is_empty());
    assert!(linux.windows_immutable_input_authorities.is_empty());
    assert_eq!(
        linux.execution_provider.as_ref().unwrap().contract,
        ExecutionProviderContract::LocalLinuxRunnerV1
    );
    assert_eq!(
        linux.execution_provider.as_ref().unwrap().executable_digest,
        file_digest(Path::new("/usr/bin/true"))
    );

    let windows = capabilities
        .targets
        .iter()
        .find(|target| target.target == ExecutionTarget::WindowsNative)
        .unwrap();
    assert!(!windows.configured);
    assert!(!windows.available);
    assert_eq!(
        windows.execution_profiles,
        vec![ExecutionProfile::TrustedLocal]
    );
    assert!(windows.windows_authorities.is_empty());
    assert!(windows.windows_immutable_input_authorities.is_empty());
    assert!(!windows.structured_plan);
    assert!(!windows.immutable_inputs);
    assert!(windows.execution_provider.is_none());
    assert!(windows.availability_issue.is_none());

    let serialized = serde_json::to_string(&capabilities).unwrap();
    assert!(serialized.contains("finance-state"));
    assert!(!serialized.contains(input_root.to_string_lossy().as_ref()));
}

#[test]
fn terminal_task_observation_elapsed_ms_freezes_at_finished_time() {
    let sandbox = Sandbox::new("terminal-observation-elapsed", 5_000);
    let runtime = Runtime::new(runtime_config(&sandbox)).unwrap();
    let created = created(
        runtime
            .registry()
            .submit(&request(
                &sandbox,
                "request:terminal-observation-elapsed",
                4,
            ))
            .unwrap(),
    );
    let finished_at_ms = created.attempt.created_at_ms + 1;
    runtime
        .registry()
        .commit_terminal(&TerminalCommit {
            attempt_id: created.attempt.attempt_id.clone(),
            expected_row_version: created.attempt.row_version,
            state: AttemptState::Cancelled,
            result_digest: digest(b"terminal-observation-elapsed"),
            exit_code: None,
            infrastructure_error_digest: None,
            finished_at_ms,
            artifacts: Vec::new(),
            reason_code: "TEST_CANCELLED".to_string(),
        })
        .unwrap();

    thread::sleep(std::time::Duration::from_millis(20));
    let observe = || {
        runtime
            .observe_task(&TaskObserveRequest {
                schema_version: RUNTIME_SCHEMA_VERSION,
                job_id: created.job.job_id.clone(),
                wait_ms: 0,
                wait_until: TaskObserveWaitUntil::Terminal,
                stdout_tail_bytes: 0,
                stderr_tail_bytes: 0,
                stdout_offset: None,
                stderr_offset: None,
            })
            .unwrap()
    };
    let first = observe();
    thread::sleep(std::time::Duration::from_millis(20));
    let second = observe();

    assert_eq!(first.elapsed_ms, Some(1));
    assert_eq!(second.elapsed_ms, Some(1));
}

#[cfg(test)]
mod registry_reference_model_properties {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum ModelOp {
        ReplaySame,
        ReplayChanged,
        Cancel,
        CommitSucceeded,
        CommitFailed,
    }

    fn op_strategy() -> impl Strategy<Value = ModelOp> {
        prop_oneof![
            Just(ModelOp::ReplaySame),
            Just(ModelOp::ReplayChanged),
            Just(ModelOp::Cancel),
            Just(ModelOp::CommitSucceeded),
            Just(ModelOp::CommitFailed),
        ]
    }

    fn terminal_identity(
        kind: &ModelOp,
    ) -> Option<(AttemptState, String, Option<i32>, &'static str)> {
        match kind {
            ModelOp::CommitSucceeded => Some((
                AttemptState::Succeeded,
                digest(b"model-terminal-succeeded"),
                Some(0),
                "MODEL_SUCCEEDED",
            )),
            ModelOp::CommitFailed => Some((
                AttemptState::Failed,
                digest(b"model-terminal-failed"),
                Some(1),
                "MODEL_FAILED",
            )),
            _ => None,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 512,
            max_shrink_iters: 16_384,
            .. ProptestConfig::default()
        })]

        #[test]
        fn request_identity_and_terminal_winner_match_reference_model(
            operations in prop::collection::vec(op_strategy(), 1..48),
        ) {
            let sandbox = Sandbox::new("registry-reference-model", 5_000);
            let client_request_id = "request:registry-reference-model";
            let attempt = running_attempt_for_commit_fault(&sandbox, client_request_id);
            let job_id = attempt.job_id.clone();
            let original = request(&sandbox, client_request_id, 1);
            let mut changed = original.clone();
            changed.plan.timeout_ms += 1;
            let mut winner: Option<(AttemptState, String)> = None;
            let mut stop_requested = false;

            for (index, operation) in operations.iter().enumerate() {
                match operation {
                    ModelOp::ReplaySame => {
                        let replay = sandbox.registry.submit(&original).unwrap();
                        let replay_job_id = match replay {
                            AdmissionOutcome::Existing { job } => job.job_id,
                            AdmissionOutcome::Created(_) => {
                                return Err(TestCaseError::fail("exact replay created a second Job"));
                            }
                        };
                        prop_assert_eq!(replay_job_id, job_id.clone());
                    }
                    ModelOp::ReplayChanged => {
                        let error = sandbox.registry.submit(&changed).unwrap_err();
                        prop_assert_eq!(error.code, RuntimeErrorCode::IdempotencyConflict);
                    }
                    ModelOp::Cancel => {
                        let projection = sandbox
                            .registry
                            .request_cancel(&job_id, 20 + index as u64)
                            .unwrap();
                        prop_assert_eq!(projection.job_id, job_id.clone());
                        if winner.is_none() {
                            stop_requested = true;
                        }
                    }
                    ModelOp::CommitSucceeded | ModelOp::CommitFailed => {
                        let (state, result_digest, exit_code, reason_code) =
                            terminal_identity(operation).unwrap();
                        let current = sandbox.registry.get_attempt(&attempt.attempt_id).unwrap();
                        let terminal = TerminalCommit {
                            attempt_id: attempt.attempt_id.clone(),
                            expected_row_version: current.row_version,
                            state,
                            result_digest: result_digest.clone(),
                            exit_code,
                            infrastructure_error_digest: None,
                            finished_at_ms: 100 + index as u64,
                            artifacts: Vec::new(),
                            reason_code: reason_code.to_string(),
                        };
                        match &winner {
                            None => {
                                let projection = sandbox.registry.commit_terminal(&terminal).unwrap();
                                prop_assert_eq!(projection.job_id, job_id.clone());
                                winner = Some((state, result_digest));
                            }
                            Some((winner_state, winner_digest))
                                if *winner_state == state && *winner_digest == result_digest =>
                            {
                                let projection = sandbox.registry.commit_terminal(&terminal).unwrap();
                                prop_assert_eq!(projection.job_id, job_id.clone());
                            }
                            Some(_) => {
                                let error = sandbox.registry.commit_terminal(&terminal).unwrap_err();
                                prop_assert_eq!(error.code, RuntimeErrorCode::ResultIdentityConflict);
                            }
                        }
                    }
                }

                let current = sandbox.registry.get_attempt(&attempt.attempt_id).unwrap();
                let current_job = sandbox.registry.get_job(&job_id).unwrap();
                prop_assert_eq!(current.job_id, job_id.clone());
                prop_assert_eq!(current_job.job_id, job_id.clone());
                match &winner {
                    Some((state, result_digest)) => {
                        prop_assert_eq!(current.state, *state);
                        prop_assert_eq!(current.result_digest.as_deref(), Some(result_digest.as_str()));
                        prop_assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
                    }
                    None if stop_requested => {
                        prop_assert_eq!(current.state, AttemptState::Stopping);
                        prop_assert_eq!(current.termination_intent, AttemptTerminationIntent::StopRequested);
                        prop_assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
                    }
                    None => {
                        prop_assert_eq!(current.state, AttemptState::Running);
                        prop_assert_eq!(current.termination_intent, AttemptTerminationIntent::Natural);
                        prop_assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);
                    }
                }
            }

            let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
            let job_count: u32 = connection
                .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
                .unwrap();
            let dispatch_count: u32 = connection
                .query_row(
                    "SELECT COUNT(*) FROM job_events WHERE job_id=?1 AND event_type='DISPATCH_ISSUED'",
                    [&job_id],
                    |row| row.get(0),
                )
                .unwrap();
            prop_assert_eq!(job_count, 1);
            prop_assert_eq!(dispatch_count, 1);
        }
    }
}
