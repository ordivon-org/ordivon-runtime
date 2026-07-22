use super::*;
use crate::{
    AdmissionOutcomeM6, ArtifactRegistrationM6, AttemptState, M6ErrorCode, M6ExecutionPlan,
    M6Registry, M6RegistryConfig, M6SubmitRequest, PlanKind, TerminalCommitM6, M6_SCHEMA_VERSION,
};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct Sandbox {
    root: PathBuf,
    registry: M6Registry,
    hardening: M7RuntimeHardeningConfig,
    policy: M7LifecyclePolicy,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ordivon-m7-{label}-{}-{}",
            std::process::id(),
            Uuid::now_v7()
        ));
        let control = root.join("control");
        let worker = root.join("worker");
        let cache = root.join("cache");
        let views = root.join("views");
        fs::create_dir_all(root.join("workspace")).unwrap();
        let policy = M7LifecyclePolicy {
            schema_version: M7_SCHEMA_VERSION,
            retention_ms: 10,
            max_retained_artifact_bytes: 1_048_576,
            max_single_job_artifact_bytes: 262_144,
            max_gc_items: 100,
        };
        let hardening = M7RuntimeHardeningConfig {
            worker: M7WorkerIdentity {
                user: "ordivon-worker".to_string(),
                group: "ordivon-worker".to_string(),
                uid: 65_534,
                gid: 65_534,
            },
            control_root: control.clone(),
            worker_root: worker,
            cache_root: cache,
            runtime_view_root: views,
            lifecycle_policy: policy.clone(),
        };
        let registry = M6Registry::initialize(M6RegistryConfig {
            db_path: control.join("registry/registry.sqlite3"),
            store_root: control.join("registry"),
            busy_timeout_ms: 5000,
        })
        .unwrap();
        Self {
            root,
            registry,
            hardening,
            policy,
        }
    }

    fn workspace(&self) -> PathBuf {
        self.root.join("workspace")
    }

    fn manager(&self) -> M7LifecycleManager {
        M7LifecycleManager::new(
            self.registry.clone(),
            self.hardening.clone(),
            self.policy.clone(),
        )
        .unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn digest(bytes: &[u8]) -> String {
    crate::universal::sha256_bytes(bytes)
}

fn request(sandbox: &Sandbox, id: &str, quota: Option<M7AdmissionQuota>) -> M6SubmitRequest {
    let executable = fs::canonicalize("/usr/bin/true").unwrap();
    M6SubmitRequest {
        schema_version: M6_SCHEMA_VERSION,
        client_request_id: id.to_string(),
        plan: M6ExecutionPlan {
            schema_version: M6_SCHEMA_VERSION,
            plan_kind: PlanKind::UniversalSandbox,
            workspace_id: "workspace:m7".to_string(),
            workspace_path: sandbox.workspace().to_string_lossy().into_owned(),
            source_revision: "revision:m7".to_string(),
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: crate::universal::sha256_file(&executable).unwrap(),
            args: Vec::new(),
            cwd: sandbox.workspace().to_string_lossy().into_owned(),
            env: BTreeMap::new(),
            timeout_ms: 1000,
            stdout_limit_bytes: 1024,
            stderr_limit_bytes: 1024,
            policy_id: "policy:m7-test".to_string(),
            policy_version: "1".to_string(),
            policy_digest: digest(b"policy:m7-test:1"),
            profile_id: None,
            principal: "principal:m7-test".to_string(),
            authority_ref: "authority:m7-test".to_string(),
        },
        global_limit: 4,
        profile_limit: None,
        lifecycle_quota: quota,
    }
}

fn created(outcome: AdmissionOutcomeM6) -> crate::CreatedAdmissionM6 {
    match outcome {
        AdmissionOutcomeM6::Created(value) => *value,
        AdmissionOutcomeM6::Existing { .. } => panic!("expected created admission"),
    }
}

fn terminal_job(sandbox: &Sandbox, id: &str, finished_at_ms: u64) -> (String, String, PathBuf) {
    let created = created(
        sandbox
            .registry
            .submit(&request(sandbox, id, None))
            .unwrap(),
    );
    let bundle = PathBuf::from(&created.attempt.bundle_path);
    fs::create_dir_all(&bundle).unwrap();
    let stdout = bundle.join("stdout.log");
    let result = bundle.join("result.json");
    fs::write(&stdout, "M7_LIFECYCLE\n").unwrap();
    fs::write(&result, "{}\n").unwrap();
    let bundle_digest = digest(b"bundle:m7-test");
    let attempt = sandbox
        .registry
        .mark_bundle_ready(&created.attempt.attempt_id, 0, &bundle_digest, 5)
        .unwrap();
    sandbox
        .registry
        .commit_terminal(&TerminalCommitM6 {
            attempt_id: attempt.attempt_id.clone(),
            expected_row_version: attempt.row_version,
            state: AttemptState::Failed,
            result_digest: crate::universal::sha256_file(&result).unwrap(),
            exit_code: Some(1),
            infrastructure_error_digest: None,
            finished_at_ms,
            artifacts: vec![
                ArtifactRegistrationM6 {
                    artifact_id: format!("{}.stdout", attempt.attempt_id),
                    kind: "stdout".to_string(),
                    relative_path: "stdout.log".to_string(),
                    digest: crate::universal::sha256_file(&stdout).unwrap(),
                    media_type: "text/plain".to_string(),
                    byte_length: fs::metadata(&stdout).unwrap().len(),
                    truncated: false,
                },
                ArtifactRegistrationM6 {
                    artifact_id: format!("{}.result", attempt.attempt_id),
                    kind: "result".to_string(),
                    relative_path: "result.json".to_string(),
                    digest: crate::universal::sha256_file(&result).unwrap(),
                    media_type: "application/json".to_string(),
                    byte_length: fs::metadata(&result).unwrap().len(),
                    truncated: false,
                },
            ],
            reason_code: "M7_TEST_TERMINAL".to_string(),
        })
        .unwrap();
    (created.job.job_id, attempt.attempt_id, bundle)
}

#[test]
fn quota_rejection_is_atomic_and_auditable() {
    let sandbox = Sandbox::new("quota");
    let quota = M7AdmissionQuota {
        policy_digest: sandbox.policy.digest().unwrap(),
        estimated_artifact_bytes: 10_000,
        max_retained_artifact_bytes: 100,
        max_single_job_artifact_bytes: 100,
    };
    let error = sandbox
        .registry
        .submit(&request(&sandbox, "quota:denied", Some(quota)))
        .unwrap_err();
    assert_eq!(error.code, M6ErrorCode::LifecycleQuotaExceeded);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 0);
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let jobs: u64 = connection
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .unwrap();
    let events: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM m7_lifecycle_events WHERE event_type='ADMISSION_QUOTA_REJECTED'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(jobs, 0);
    assert_eq!(events, 1);
}

#[test]
fn investigation_hold_blocks_gc_then_tombstones_artifacts() {
    let sandbox = Sandbox::new("gc");
    let manager = sandbox.manager();
    let (job_id, attempt_id, bundle) = terminal_job(&sandbox, "gc:terminal", 20);
    let hold = manager
        .place_hold(&job_id, "operator:m7-test", "investigation", 30)
        .unwrap();
    let blocked = manager.plan_gc(100).unwrap();
    assert_eq!(blocked.item_count, 0);
    manager
        .release_hold(&hold, "operator:m7-test", 110)
        .unwrap();
    let plan = manager.plan_gc(120).unwrap();
    assert_eq!(plan.attempt_ids, vec![attempt_id.clone()]);
    let executed = manager
        .execute_gc(&plan.plan_id, "operator:m7-test", 130)
        .unwrap();
    assert_eq!(executed.item_count, 1);
    assert!(!bundle.exists());
    assert!(sandbox.registry.list_artifacts(&job_id).unwrap().is_empty());
    let connection = Connection::open(&sandbox.registry.config().db_path).unwrap();
    let tombstones: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM m7_artifact_tombstones WHERE attempt_id=?1",
            [attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tombstones, 2);
}

#[test]
fn online_backup_and_empty_root_restore_preserve_registry_and_bundles() {
    let sandbox = Sandbox::new("backup");
    let manager = sandbox.manager();
    let (job_id, attempt_id, bundle) = terminal_job(&sandbox, "backup:terminal", 20);
    let parent = sandbox.root.parent().unwrap();
    let backup = parent.join(format!("ordivon-m7-backup-{}", Uuid::now_v7()));
    let restored = parent.join(format!("ordivon-m7-restore-{}", Uuid::now_v7()));
    let result = manager
        .create_backup(&backup, "operator:m7-test", 40)
        .unwrap();
    assert!(result.file_count >= 2);
    let restore = manager
        .restore_backup(&backup, &restored, "operator:m7-test", 50)
        .unwrap();
    assert_eq!(restore.backup_id, result.backup_id);
    let restored_db = restored.join("registry/registry.sqlite3");
    assert!(restored_db.is_file());
    let connection = Connection::open(&restored_db).unwrap();
    let restored_path: String = connection
        .query_row(
            "SELECT bundle_path FROM attempts WHERE attempt_id=?1",
            [attempt_id.clone()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(Path::new(&restored_path).starts_with(&restored));
    assert!(Path::new(&restored_path).join("stdout.log").is_file());
    let restored_jobs: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE job_id=?1",
            [job_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_jobs, 1);
    assert!(bundle.exists());
    fs::remove_dir_all(backup).unwrap();
    fs::remove_dir_all(restored).unwrap();
}

#[test]
fn backup_is_blocked_while_capacity_is_active() {
    let sandbox = Sandbox::new("backup-busy");
    let manager = sandbox.manager();
    let _created = created(
        sandbox
            .registry
            .submit(&request(&sandbox, "backup:busy", None))
            .unwrap(),
    );
    let backup = sandbox
        .root
        .parent()
        .unwrap()
        .join(format!("ordivon-m7-busy-backup-{}", Uuid::now_v7()));
    let error = manager
        .create_backup(&backup, "operator:m7-test", 40)
        .unwrap_err();
    assert_eq!(error.code, M6ErrorCode::BackupBusy);
    assert!(!backup.exists());
}
