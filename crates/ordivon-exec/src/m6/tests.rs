use super::*;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

struct Sandbox {
    root: PathBuf,
    registry: M6Registry,
}

impl Sandbox {
    fn new(label: &str, busy_timeout_ms: u64) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ordivon-m6-{label}-{}-{}",
            std::process::id(),
            Uuid::now_v7()
        ));
        let store = root.join("store");
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let registry = M6Registry::initialize(M6RegistryConfig {
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

fn request(sandbox: &Sandbox, client_request_id: &str, global_limit: u32) -> M6SubmitRequest {
    let executable = fs::canonicalize("/usr/bin/true").unwrap();
    M6SubmitRequest {
        schema_version: M6_SCHEMA_VERSION,
        client_request_id: client_request_id.to_string(),
        plan: M6ExecutionPlan {
            schema_version: M6_SCHEMA_VERSION,
            plan_kind: PlanKind::UniversalSandbox,
            workspace_id: "workspace:test".to_string(),
            workspace_path: sandbox.workspace().to_string_lossy().into_owned(),
            source_revision: "test-revision".to_string(),
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: file_digest(&executable),
            args: Vec::new(),
            cwd: sandbox.workspace().to_string_lossy().into_owned(),
            env: Default::default(),
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            policy_id: "policy:test".to_string(),
            policy_version: "1".to_string(),
            policy_digest: digest(b"policy:test:1"),
            profile_id: None,
            principal: "principal:test".to_string(),
            authority_ref: "authority:test".to_string(),
        },
        global_limit,
        profile_limit: None,
    }
}

fn created(outcome: AdmissionOutcomeM6) -> CreatedAdmissionM6 {
    match outcome {
        AdmissionOutcomeM6::Created(created) => *created,
        AdmissionOutcomeM6::Existing { .. } => panic!("expected newly created admission"),
    }
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
fn idempotent_replay_returns_one_job_and_conflict_rejects_change() {
    let sandbox = Sandbox::new("idempotency", 5000);
    let original = request(&sandbox, "request:same", 4);
    let first = created(sandbox.registry.submit(&original).unwrap());
    let replay = sandbox.registry.submit(&original).unwrap();
    let existing = match replay {
        AdmissionOutcomeM6::Existing { job } => job,
        AdmissionOutcomeM6::Created(_) => panic!("replay created a second Job"),
    };
    assert_eq!(first.job.job_id, existing.job_id);
    assert_eq!(sandbox.registry.active_reservation_count().unwrap(), 1);

    let mut changed = original;
    changed.plan.timeout_ms += 1;
    let error = sandbox.registry.submit(&changed).unwrap_err();
    assert_eq!(error.code, M6ErrorCode::IdempotencyConflict);
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
            .filter(|outcome| matches!(outcome, AdmissionOutcomeM6::Created(_)))
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
    assert_eq!(error.code, M6ErrorCode::ConcurrencyLimit);
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
    assert_eq!(error.code, M6ErrorCode::RegistryBusy);
    assert!(error.retryable);
    lock.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn list_is_bounded_and_cursor_stable() {
    let sandbox = Sandbox::new("list", 5000);
    for index in 0..3 {
        let created = created(
            sandbox
                .registry
                .submit(&request(&sandbox, &format!("request:list:{index}"), 8))
                .unwrap(),
        );
        assert!(created.job.job_id.starts_with("job-"));
    }
    let first = sandbox
        .registry
        .list_jobs(&JobListRequestM6 {
            limit: 2,
            cursor: None,
        })
        .unwrap();
    assert_eq!(first.jobs.len(), 2);
    assert!(first.next_cursor.is_some());
    let second = sandbox
        .registry
        .list_jobs(&JobListRequestM6 {
            limit: 2,
            cursor: first.next_cursor,
        })
        .unwrap();
    assert_eq!(second.jobs.len(), 1);
    assert!(second.next_cursor.is_none());
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
            &RunnerIdentityM6 {
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
    let terminal = TerminalCommitM6 {
        attempt_id: attempt.attempt_id.clone(),
        expected_row_version: attempt.row_version,
        state: AttemptState::Succeeded,
        result_digest: digest(b"result"),
        exit_code: Some(0),
        infrastructure_error_digest: None,
        finished_at_ms: 13,
        artifacts: vec![ArtifactRegistrationM6 {
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
    assert_eq!(error.code, M6ErrorCode::ResultIdentityConflict);
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
    let terminal = TerminalCommitM6 {
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
fn newer_schema_and_checksum_drift_fail_closed() {
    let newer = Sandbox::new("newer-schema", 5000);
    let connection = Connection::open(&newer.registry.config().db_path).unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,'future','sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',0)",
            [if cfg!(feature = "runtime-hardening-m7") { 3 } else { 2 }],
        )
        .unwrap();
    drop(connection);
    let error = M6Registry::initialize(newer.registry.config().clone()).unwrap_err();
    assert_eq!(error.code, M6ErrorCode::SchemaVersionUnsupported);

    let drift = Sandbox::new("checksum-drift", 5000);
    let connection = Connection::open(&drift.registry.config().db_path).unwrap();
    connection
        .execute(
            "UPDATE schema_migrations SET checksum='sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE version=1",
            [],
        )
        .unwrap();
    drop(connection);
    let error = M6Registry::initialize(drift.registry.config().clone()).unwrap_err();
    assert_eq!(error.code, M6ErrorCode::MigrationChecksumMismatch);
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
fn corrupt_database_fails_closed() {
    let sandbox = Sandbox::new("corrupt", 5000);
    fs::write(&sandbox.registry.config().db_path, b"not a sqlite database").unwrap();
    let error = M6Registry::initialize(sandbox.registry.config().clone()).unwrap_err();
    assert!(matches!(
        error.code,
        M6ErrorCode::RegistryCorrupt | M6ErrorCode::RegistryUnavailable
    ));
}
