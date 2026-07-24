use super::*;
use proptest::prelude::*;
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

fn request(sandbox: &Sandbox, client_request_id: &str, global_limit: u32) -> SubmitRequest {
    let executable = fs::canonicalize("/usr/bin/true").unwrap();
    SubmitRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: client_request_id.to_string(),
        plan: RuntimeExecutionPlan {
            schema_version: RUNTIME_SCHEMA_VERSION,
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
            principal: "principal:test".to_string(),
        },
        global_limit,
    }
}

fn created(outcome: AdmissionOutcome) -> CreatedAdmission {
    match outcome {
        AdmissionOutcome::Created(created) => *created,
        AdmissionOutcome::Existing { .. } => panic!("expected newly created admission"),
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
fn active_workspace_job_lookup_tracks_reservations_and_resolution() {
    let sandbox = Sandbox::new("active-workspace", 5000);
    let mut active_request = request(&sandbox, "request:active-workspace", 4);
    active_request.plan.workspace_id = "workspace:active-workspace".to_string();
    let created = created(sandbox.registry.submit(&active_request).unwrap());
    assert_eq!(
        sandbox
            .registry
            .active_job_ids_for_workspace("workspace:active-workspace", 20)
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
        .active_job_ids_for_workspace("workspace:active-workspace", 20)
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
        })
        .unwrap();
    assert_eq!(first.jobs.len(), 2);
    assert!(first.next_cursor.is_some());
    let second = sandbox
        .registry
        .list_jobs(&RuntimeJobListRequest {
            limit: 2,
            cursor: first.next_cursor,
        })
        .unwrap();
    assert_eq!(second.jobs.len(), 1);
    assert!(second.next_cursor.is_none());
}

proptest! {
    #[test]
    fn newest_first_cursor_pagination_is_complete_and_unique(
        job_count in 1usize..30,
        page_size in 1u32..10,
    ) {
        let sandbox = Sandbox::new("property-list", 5000);
        for index in 0..job_count {
            let mut list_request = request(&sandbox, &format!("request:property:{index}"), 64);
            list_request.plan.workspace_id = format!("workspace:property:{index}");
            sandbox.registry.submit(&list_request).unwrap();
        }
        let mut cursor = None;
        let mut observed = Vec::new();
        loop {
            let page = sandbox.registry.list_jobs(&RuntimeJobListRequest {
                limit: page_size,
                cursor,
            }).unwrap();
            observed.extend(page.jobs.iter().map(|job| (
                job.created_at_ms,
                job.job_id.clone(),
                job.client_request_id.clone(),
            )));
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        prop_assert_eq!(observed.len(), job_count);
        let unique: std::collections::BTreeSet<_> = observed.iter().map(|(_, id, _)| id).collect();
        prop_assert_eq!(unique.len(), job_count);
        let newest_first = observed.windows(2).all(|pair| {
            (pair[0].0, pair[0].1.as_str()) >= (pair[1].0, pair[1].1.as_str())
        });
        prop_assert!(newest_first);
        let requests: std::collections::BTreeSet<_> = observed.iter().map(|(_, _, request)| request).collect();
        prop_assert_eq!(requests.len(), job_count);
    }
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
            [3],
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
fn existing_v1_registry_upgrades_to_orphan_recovery_schema() {
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
    assert_eq!(max_version, 2);
    let checksum: String = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(checksum, RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM);
    drop(connection);
    fs::remove_dir_all(root).unwrap();
}
