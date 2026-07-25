use super::repair::{AdminRepairAudit, AdminRepairOperation};
use super::*;
use crate::universal::{
    CapturedOutput, RunnerTaskResult, TaskTerminalStatus, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use proptest::prelude::*;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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

fn doctor_config(sandbox: &Sandbox) -> RuntimeDoctorConfig {
    RuntimeDoctorConfig {
        db_path: sandbox.registry.config().db_path.clone(),
        store_root: sandbox.registry.config().store_root.clone(),
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
        infrastructure_error: None,
        started_unix_ms: finished_at_ms.saturating_sub(1),
        finished_unix_ms: finished_at_ms,
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
fn runtime_doctor_is_read_only_and_fingerprint_is_stable() {
    let sandbox = Sandbox::new("doctor-read-only", 5000);
    let before = fs::read(&sandbox.registry.config().db_path).unwrap();

    let first = inspect_runtime(&doctor_config(&sandbox)).unwrap();
    let second = inspect_runtime(&doctor_config(&sandbox)).unwrap();

    assert_eq!(first.violation_count, 0);
    assert!(first.cases.is_empty());
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(
        before,
        fs::read(&sandbox.registry.config().db_path).unwrap()
    );
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
    assert_eq!(repair_events, 2);
    assert_eq!(receipts, 2);
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
            [4],
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
fn existing_v1_registry_upgrades_to_terminal_repair_schema() {
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
    assert_eq!(max_version, 3);
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
    drop(connection);
    fs::remove_dir_all(root).unwrap();
}
