use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::evidence::{prepare_runner_terminal_from_bundle, RESULT_FILE};
use super::registry::{
    inspect_runtime_invariants_connection, load_attempt, load_job, load_reservation,
    MAX_MIGRATION_VERSION,
};
use super::{
    AttemptRecord, AttemptState, AttemptTerminationIntent, JobResolution, RegistryConfig,
    ReservationRecord, ReservationState, RuntimeError, RuntimeErrorCode, RuntimeInvariantViolation,
    RuntimeJobRecord, RuntimeResult, TerminalCommit,
};
use crate::universal::sha256_bytes;

pub const RUNTIME_DOCTOR_SCHEMA_VERSION: u32 = 2;
const MAX_DOCTOR_CAPACITY_HOLDERS: usize = 50;

#[derive(Clone, Debug)]
pub struct RuntimeDoctorConfig {
    pub db_path: PathBuf,
    pub store_root: PathBuf,
    pub busy_timeout_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorReport {
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub database_path: String,
    pub store_root: String,
    pub migration_version: i64,
    pub integrity_check: String,
    pub fingerprint: String,
    pub summary: RuntimeDoctorSummary,
    pub violation_count: usize,
    pub violations: Vec<RuntimeInvariantViolation>,
    pub cases: Vec<RuntimeDoctorCase>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorSummary {
    pub status: String,
    pub jobs_total: u64,
    pub unresolved_jobs: u64,
    pub attempts_by_state: BTreeMap<String, u64>,
    pub reservations_by_state: BTreeMap<String, u64>,
    pub recovery_required_attempts: u64,
    pub artifacts_total: u64,
    pub artifact_bytes: u64,
    pub capacity_holders: Vec<RuntimeDoctorCapacityHolder>,
    pub capacity_holders_truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorCapacityHolder {
    pub job_id: String,
    pub workspace_id: String,
    pub attempt_id: String,
    pub attempt_state: AttemptState,
    pub reservation_state: ReservationState,
    pub recovery_required: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorCase {
    pub job: RuntimeDoctorJobState,
    pub attempt: RuntimeDoctorAttemptState,
    pub reservation: RuntimeDoctorReservationState,
    pub violation_codes: Vec<String>,
    pub runner_result_present: bool,
    pub control_result_present: bool,
    pub fingerprint: String,
    pub proposal: RuntimeDoctorProposal,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorJobState {
    pub job_id: String,
    pub workspace_id: String,
    pub resolution: Option<JobResolution>,
    pub current_attempt_id: Option<String>,
    pub row_version: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorAttemptState {
    pub attempt_id: String,
    pub state: AttemptState,
    pub termination_intent: AttemptTerminationIntent,
    pub result_digest: Option<String>,
    pub exit_code: Option<i32>,
    pub finished_at_ms: Option<u64>,
    pub row_version: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDoctorReservationState {
    pub reservation_id: String,
    pub state: ReservationState,
    pub released_at_ms: Option<u64>,
    pub release_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RuntimeDoctorProposal {
    RecoverRunnerResult { terminal: TerminalCommit },
    ReleaseTerminalReservation,
    NoRepairNeeded,
    ManualReview { reasons: Vec<String> },
}

pub fn inspect_runtime(config: &RuntimeDoctorConfig) -> RuntimeResult<RuntimeDoctorReport> {
    validate_config(config)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(&config.db_path, flags)
        .map_err(|error| RuntimeError::from_sql(error, "cannot open Runtime Registry read-only"))?;
    connection
        .busy_timeout(Duration::from_millis(config.busy_timeout_ms))
        .map_err(|error| RuntimeError::from_sql(error, "cannot set Doctor busy timeout"))?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|error| RuntimeError::from_sql(error, "cannot enable Doctor query-only mode"))?;
    connection
        .pragma_update(None, "trusted_schema", false)
        .map_err(|error| RuntimeError::from_sql(error, "cannot disable trusted schema"))?;

    let integrity_check: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| RuntimeError::from_sql(error, "cannot run Registry integrity check"))?;
    if integrity_check != "ok" {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("Registry integrity check returned {integrity_check}"),
            None,
            false,
        ));
    }
    let migration_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|error| RuntimeError::from_sql(error, "cannot read Registry migration version"))?;
    if migration_version > MAX_MIGRATION_VERSION {
        return Err(RuntimeError::new(
            RuntimeErrorCode::SchemaVersionUnsupported,
            format!(
                "Registry schema {migration_version} is newer than supported {MAX_MIGRATION_VERSION}"
            ),
            None,
            false,
        ));
    }

    let mut violations = inspect_runtime_invariants_connection(&connection)?;
    violations.sort_by(|left, right| {
        (
            left.job_id.as_deref(),
            left.attempt_id.as_deref(),
            left.code.as_str(),
        )
            .cmp(&(
                right.job_id.as_deref(),
                right.attempt_id.as_deref(),
                right.code.as_str(),
            ))
    });

    let mut by_attempt: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for violation in &violations {
        if let Some(attempt_id) = &violation.attempt_id {
            by_attempt
                .entry(attempt_id.clone())
                .or_default()
                .insert(violation.code.clone());
        }
    }

    let mut cases = Vec::with_capacity(by_attempt.len());
    for (attempt_id, codes) in by_attempt {
        let attempt = load_attempt(&connection, &attempt_id)?;
        let job = load_job(&connection, &attempt.job_id)?;
        let reservation = load_reservation(&connection, &attempt_id)?;
        let expected_bundle = config.store_root.join("attempts").join(&attempt.attempt_id);
        let bundle_path_trusted = Path::new(&attempt.bundle_path) == expected_bundle;
        let result_path = expected_bundle.join(RESULT_FILE);
        let control_path = expected_bundle.join("control-result.json");
        let runner_result_present = bundle_path_trusted && result_path.is_file();
        let control_result_present = bundle_path_trusted && control_path.is_file();
        let proposal = if bundle_path_trusted {
            propose_repair(&job, &attempt, &reservation, runner_result_present)
        } else {
            RuntimeDoctorProposal::ManualReview {
                reasons: vec![
                    "Attempt bundle path is outside the canonical Registry store".to_string(),
                ],
            }
        };
        let violation_codes: Vec<String> = codes.into_iter().collect();
        let job_state = RuntimeDoctorJobState {
            job_id: job.job_id.clone(),
            workspace_id: job.workspace_id.clone(),
            resolution: job.resolution,
            current_attempt_id: job.current_attempt_id.clone(),
            row_version: job.row_version,
        };
        let attempt_state = RuntimeDoctorAttemptState {
            attempt_id: attempt.attempt_id.clone(),
            state: attempt.state,
            termination_intent: attempt.termination_intent,
            result_digest: attempt.result_digest.clone(),
            exit_code: attempt.exit_code,
            finished_at_ms: attempt.finished_at_ms,
            row_version: attempt.row_version,
        };
        let reservation_state = RuntimeDoctorReservationState {
            reservation_id: reservation.reservation_id.clone(),
            state: reservation.state,
            released_at_ms: reservation.released_at_ms,
            release_reason: reservation.release_reason.clone(),
        };
        let fingerprint = case_fingerprint(
            &job_state,
            &attempt_state,
            &reservation_state,
            &violation_codes,
            runner_result_present,
            control_result_present,
            &proposal,
        )?;
        cases.push(RuntimeDoctorCase {
            job: job_state,
            attempt: attempt_state,
            reservation: reservation_state,
            violation_codes,
            runner_result_present,
            control_result_present,
            fingerprint,
            proposal,
        });
    }

    let mut summary = inspect_summary(&connection)?;
    summary.status = if violations.is_empty()
        && summary.recovery_required_attempts == 0
        && summary
            .reservations_by_state
            .get("held_orphaned")
            .copied()
            .unwrap_or(0)
            == 0
    {
        "healthy".to_string()
    } else {
        "attention".to_string()
    };

    let report_fingerprint = sha256_bytes(
        &serde_json::to_vec(&serde_json::json!({
            "schemaVersion": RUNTIME_DOCTOR_SCHEMA_VERSION,
            "databasePath": config.db_path,
            "storeRoot": config.store_root,
            "migrationVersion": migration_version,
            "integrityCheck": integrity_check,
            "summary": summary,
            "violations": violations,
            "cases": cases.iter().map(|case| &case.fingerprint).collect::<Vec<_>>(),
        }))
        .map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("cannot serialize Doctor fingerprint material: {error}"),
                None,
                false,
            )
        })?,
    );

    Ok(RuntimeDoctorReport {
        schema_version: RUNTIME_DOCTOR_SCHEMA_VERSION,
        generated_at_ms: now_ms()?,
        database_path: config.db_path.to_string_lossy().into_owned(),
        store_root: config.store_root.to_string_lossy().into_owned(),
        migration_version,
        integrity_check,
        fingerprint: report_fingerprint,
        summary,
        violation_count: violations.len(),
        violations,
        cases,
    })
}

fn inspect_summary(connection: &Connection) -> RuntimeResult<RuntimeDoctorSummary> {
    let jobs_total = count_query(connection, "SELECT COUNT(*) FROM jobs", "count Jobs")?;
    let unresolved_jobs = count_query(
        connection,
        "SELECT COUNT(*) FROM jobs WHERE resolution IS NULL",
        "count unresolved Jobs",
    )?;
    let recovery_required_attempts = count_query(
        connection,
        "SELECT COUNT(*) FROM attempt_conditions WHERE condition_type='recovery_required' AND status='true'",
        "count recovery-required Attempts",
    )?;
    let artifacts_total = count_query(
        connection,
        "SELECT COUNT(*) FROM artifacts",
        "count Artifacts",
    )?;
    let artifact_bytes = count_query(
        connection,
        "SELECT COALESCE(SUM(byte_length),0) FROM artifacts",
        "sum Artifact bytes",
    )?;
    let attempts_by_state = grouped_counts(
        connection,
        "SELECT state,COUNT(*) FROM attempts GROUP BY state ORDER BY state",
        "count Attempts by state",
    )?;
    let reservations_by_state = grouped_counts(
        connection,
        "SELECT state,COUNT(*) FROM concurrency_reservations GROUP BY state ORDER BY state",
        "count reservations by state",
    )?;

    let mut statement = connection
        .prepare(
            "SELECT j.job_id,j.workspace_id,a.attempt_id,a.state,r.state,EXISTS(SELECT 1 FROM attempt_conditions c WHERE c.attempt_id=a.attempt_id AND c.condition_type='recovery_required' AND c.status='true') FROM concurrency_reservations r JOIN attempts a ON a.attempt_id=r.attempt_id JOIN jobs j ON j.job_id=a.job_id WHERE r.state IN ('active','held_orphaned') ORDER BY r.acquired_at_ms,j.job_id LIMIT ?1",
        )
        .map_err(|error| RuntimeError::from_sql(error, "prepare capacity-holder summary"))?;
    let rows = statement
        .query_map([(MAX_DOCTOR_CAPACITY_HOLDERS + 1) as u64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, bool>(5)?,
            ))
        })
        .map_err(|error| RuntimeError::from_sql(error, "query capacity-holder summary"))?;
    let mut capacity_holders = Vec::new();
    for row in rows {
        let (job_id, workspace_id, attempt_id, attempt_state, reservation_state, recovery_required) =
            row.map_err(|error| RuntimeError::from_sql(error, "decode capacity-holder summary"))?;
        capacity_holders.push(RuntimeDoctorCapacityHolder {
            job_id,
            workspace_id,
            attempt_id,
            attempt_state: AttemptState::parse(&attempt_state)?,
            reservation_state: ReservationState::parse(&reservation_state)?,
            recovery_required,
        });
    }

    let capacity_holders_truncated = capacity_holders.len() > MAX_DOCTOR_CAPACITY_HOLDERS;
    capacity_holders.truncate(MAX_DOCTOR_CAPACITY_HOLDERS);

    Ok(RuntimeDoctorSummary {
        status: String::new(),
        jobs_total,
        unresolved_jobs,
        attempts_by_state,
        reservations_by_state,
        recovery_required_attempts,
        artifacts_total,
        artifact_bytes,
        capacity_holders,
        capacity_holders_truncated,
    })
}

fn count_query(connection: &Connection, sql: &str, context: &str) -> RuntimeResult<u64> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(|error| RuntimeError::from_sql(error, context))
}

fn grouped_counts(
    connection: &Connection,
    sql: &str,
    context: &str,
) -> RuntimeResult<BTreeMap<String, u64>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| RuntimeError::from_sql(error, context))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| RuntimeError::from_sql(error, context))?;
    let mut result = BTreeMap::new();
    for row in rows {
        let (state, count) = row.map_err(|error| RuntimeError::from_sql(error, context))?;
        result.insert(state, count);
    }
    Ok(result)
}

fn validate_config(config: &RuntimeDoctorConfig) -> RuntimeResult<()> {
    RegistryConfig {
        db_path: config.db_path.clone(),
        store_root: config.store_root.clone(),
        busy_timeout_ms: config.busy_timeout_ms,
    }
    .validate()?;
    if !config.db_path.is_file() {
        return Err(RuntimeError::invalid(
            "database must be an existing regular file",
            "database",
        ));
    }
    Ok(())
}

fn propose_repair(
    job: &RuntimeJobRecord,
    attempt: &AttemptRecord,
    reservation: &ReservationRecord,
    result_present: bool,
) -> RuntimeDoctorProposal {
    if result_present {
        return match prepare_runner_terminal_from_bundle(attempt) {
            Ok(terminal) => {
                let expected_resolution = resolution_for_state(terminal.state);
                let target_reservation = reservation_target(terminal.state);
                let terminal_matches = attempt.state == terminal.state
                    && attempt.result_digest.as_deref() == Some(terminal.result_digest.as_str())
                    && attempt.finished_at_ms == Some(terminal.finished_at_ms)
                    && attempt.exit_code == terminal.exit_code
                    && job.resolution == expected_resolution
                    && job.current_attempt_id.is_none();
                if terminal_matches && reservation.state == target_reservation {
                    RuntimeDoctorProposal::NoRepairNeeded
                } else if terminal_matches {
                    RuntimeDoctorProposal::ReleaseTerminalReservation
                } else {
                    RuntimeDoctorProposal::RecoverRunnerResult { terminal }
                }
            }
            Err(error) => RuntimeDoctorProposal::ManualReview {
                reasons: vec![format!("{}: {}", error.code.as_str(), error.message)],
            },
        };
    }

    let expected_resolution = resolution_for_state(attempt.state);
    let evidence_complete = attempt.state.is_terminal()
        && attempt.result_digest.is_some()
        && attempt.finished_at_ms.is_some()
        && job.resolution == expected_resolution
        && job.current_attempt_id.is_none();
    if evidence_complete {
        let target = reservation_target(attempt.state);
        if reservation.state == target {
            RuntimeDoctorProposal::NoRepairNeeded
        } else {
            RuntimeDoctorProposal::ReleaseTerminalReservation
        }
    } else {
        let mut reasons = Vec::new();
        if !attempt.state.is_terminal() {
            reasons.push("Attempt is not terminal".to_string());
        }
        if attempt.result_digest.is_none() {
            reasons.push("terminal result digest is missing".to_string());
        }
        if attempt.finished_at_ms.is_none() {
            reasons.push("terminal finish time is missing".to_string());
        }
        if job.resolution != expected_resolution {
            reasons.push("Job resolution does not match Attempt state".to_string());
        }
        if job.current_attempt_id.is_some() {
            reasons.push("resolved Job still retains current_attempt_id".to_string());
        }
        reasons.push("Runner result bundle is absent".to_string());
        RuntimeDoctorProposal::ManualReview { reasons }
    }
}

fn resolution_for_state(state: AttemptState) -> Option<JobResolution> {
    match state {
        AttemptState::Succeeded => Some(JobResolution::Succeeded),
        AttemptState::Failed => Some(JobResolution::Failed),
        AttemptState::TimedOut => Some(JobResolution::TimedOut),
        AttemptState::Cancelled => Some(JobResolution::Cancelled),
        AttemptState::Lost => Some(JobResolution::Lost),
        AttemptState::Orphaned => Some(JobResolution::Orphaned),
        _ => None,
    }
}

fn reservation_target(state: AttemptState) -> ReservationState {
    if state == AttemptState::Orphaned {
        ReservationState::HeldOrphaned
    } else {
        ReservationState::Released
    }
}

#[allow(clippy::too_many_arguments)]
fn case_fingerprint(
    job: &RuntimeDoctorJobState,
    attempt: &RuntimeDoctorAttemptState,
    reservation: &RuntimeDoctorReservationState,
    violation_codes: &[String],
    runner_result_present: bool,
    control_result_present: bool,
    proposal: &RuntimeDoctorProposal,
) -> RuntimeResult<String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "job": job,
        "attempt": attempt,
        "reservation": reservation,
        "violationCodes": violation_codes,
        "runnerResultPresent": runner_result_present,
        "controlResultPresent": control_result_present,
        "proposal": proposal,
    }))
    .map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("cannot serialize Doctor case fingerprint: {error}"),
            None,
            false,
        )
    })?;
    Ok(sha256_bytes(&bytes))
}

fn now_ms() -> RuntimeResult<u64> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::IoError,
                format!("system clock precedes Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis();
    u64::try_from(value).map_err(|_| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            "system time exceeds Runtime range",
            None,
            false,
        )
    })
}
