use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::registry::{load_attempt, load_job, load_reservation, MAX_MIGRATION_VERSION};
use super::{
    AttemptState, AttemptTerminationIntent, JobDesiredState, JobResolution, ReservationState,
    RuntimeError, RuntimeErrorCode, RuntimeResult,
};

pub const RUNTIME_INSPECTION_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_INSPECTION_EVENT_LIMIT: u32 = 200;
pub const MAX_INSPECTION_EVENT_LIMIT: u32 = 1_000;
const MAX_INSPECTION_ATTEMPTS: u32 = 32;

#[derive(Clone, Debug)]
pub struct RuntimeInspectionConfig {
    pub db_path: PathBuf,
    pub busy_timeout_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeJobInspection {
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub migration_version: i64,
    pub job: RuntimeInspectionJob,
    pub attempts: Vec<RuntimeInspectionAttempt>,
    pub attempts_truncated: bool,
    pub artifacts: RuntimeInspectionArtifactSummary,
    pub episodes: RuntimeInspectionEpisodes,
    pub timeline: Vec<RuntimeInspectionEvent>,
    pub events_truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInspectionJob {
    pub job_id: String,
    pub client_request_id: String,
    pub operation_digest: String,
    pub workspace_id: String,
    pub created_at_ms: u64,
    pub desired_state: JobDesiredState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<JobResolution>,
    pub mechanically_converged: bool,
    pub semantic_completion_evaluated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInspectionAttempt {
    pub attempt_id: String,
    pub attempt_number: u32,
    pub state: AttemptState,
    pub termination_intent: AttemptTerminationIntent,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub result_available: bool,
    pub reservation_state: ReservationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservation_release_reason: Option<String>,
    pub conditions: Vec<RuntimeInspectionCondition>,
    pub artifact_count: u64,
    pub artifact_bytes: u64,
    pub truncated_artifacts: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInspectionCondition {
    pub condition_type: String,
    pub status: String,
    pub reason_code: String,
    pub observed_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInspectionArtifactSummary {
    pub count: u64,
    pub bytes: u64,
    pub truncated: u64,
    pub by_kind: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInspectionEpisodes {
    pub dispatches: u64,
    pub duplicate_dispatches: u64,
    pub stop_requests: u64,
    pub reconciliation_failures: u64,
    pub reconciliation_convergences: u64,
    pub runner_result_recoveries: u64,
    pub resolution_corrections: u64,
    pub administrative_repairs: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInspectionEvent {
    pub sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    pub event_type: String,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_state: Option<String>,
    pub reason_code: String,
    pub observed_at_ms: u64,
    pub elapsed_ms: u64,
    pub delta_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceSummary {
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub migration_version: i64,
    pub since_ms: u64,
    pub jobs: RuntimeExperienceJobSummary,
    pub resolutions: BTreeMap<String, u64>,
    pub attempts: BTreeMap<String, u64>,
    pub reservations: BTreeMap<String, u64>,
    pub recovery: RuntimeExperienceRecoverySummary,
    pub dispatch: RuntimeExperienceDispatchSummary,
    pub cancellation: RuntimeExperienceCancellationSummary,
    pub mechanical_latency_ms: RuntimeExperienceMechanicalLatencySummary,
    pub duration_ms: RuntimeExperienceDurationSummary,
    pub artifacts: RuntimeExperienceArtifactSummary,
    pub event_types: BTreeMap<String, u64>,
    pub terminal_reasons: BTreeMap<String, u64>,
    pub semantic_completion_evaluated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceJobSummary {
    pub total: u64,
    pub converged: u64,
    pub unresolved: u64,
    pub recovery_required: u64,
    pub capacity_held: u64,
    pub convergence_rate_basis_points: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceRecoverySummary {
    pub jobs_with_reconciliation_failure: u64,
    pub jobs_with_automatic_recovery: u64,
    pub jobs_with_administrative_repair: u64,
    pub automatic_recovery_rate_basis_points: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceDispatchSummary {
    pub dispatches: u64,
    pub jobs_with_duplicate_dispatch: u64,
    pub duplicate_dispatches: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceCancellationSummary {
    pub requested: u64,
    pub resolved_cancelled: u64,
    pub resolved_other: u64,
    pub unresolved: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceDurationSummary {
    pub samples: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceMechanicalLatencySummary {
    pub admission_to_dispatch: RuntimeExperienceDurationSummary,
    pub dispatch_to_runner_bound: RuntimeExperienceDurationSummary,
    pub runner_bound_to_terminal: RuntimeExperienceDurationSummary,
    pub cancellation_to_terminal: RuntimeExperienceDurationSummary,
    pub reconciliation_to_convergence: RuntimeExperienceDurationSummary,
}

#[derive(Default)]
struct RuntimeMechanicalLatencySamples {
    admission_to_dispatch: Vec<u64>,
    dispatch_to_runner_bound: Vec<u64>,
    runner_bound_to_terminal: Vec<u64>,
    cancellation_to_terminal: Vec<u64>,
    reconciliation_to_convergence: Vec<u64>,
}

#[derive(Default)]
struct RuntimeJobLatencyState {
    created_at_ms: u64,
    dispatch_at_ms: Option<u64>,
    runner_bound_at_ms: Option<u64>,
    cancellation_at_ms: Option<u64>,
    reconciliation_failed_at_ms: Option<u64>,
    terminal_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExperienceArtifactSummary {
    pub count: u64,
    pub bytes: u64,
    pub truncated: u64,
}

pub fn inspect_job(
    config: &RuntimeInspectionConfig,
    job_id: &str,
    event_limit: u32,
    include_detail: bool,
) -> RuntimeResult<RuntimeJobInspection> {
    if job_id.is_empty() {
        return Err(RuntimeError::invalid("jobId must not be empty", "jobId"));
    }
    if event_limit == 0 || event_limit > MAX_INSPECTION_EVENT_LIMIT {
        return Err(RuntimeError::invalid(
            format!("eventLimit must be in 1..={MAX_INSPECTION_EVENT_LIMIT}"),
            "eventLimit",
        ));
    }
    let (connection, migration_version) = open_read_only(config)?;
    let job = load_job(&connection, job_id)?;

    let mut attempt_ids: Vec<String> = Vec::new();
    let mut statement = connection
        .prepare("SELECT attempt_id FROM attempts WHERE job_id=?1 ORDER BY attempt_number LIMIT ?2")
        .map_err(|error| RuntimeError::from_sql(error, "prepare Job Attempt inspection"))?;
    let rows = statement
        .query_map(params![job_id, MAX_INSPECTION_ATTEMPTS + 1], |row| {
            row.get(0)
        })
        .map_err(|error| RuntimeError::from_sql(error, "query Job Attempts"))?;
    for row in rows {
        attempt_ids.push(
            row.map_err(|error| RuntimeError::from_sql(error, "decode Job Attempt identity"))?,
        );
    }
    let attempts_truncated = attempt_ids.len() > MAX_INSPECTION_ATTEMPTS as usize;
    attempt_ids.truncate(MAX_INSPECTION_ATTEMPTS as usize);

    let mut attempts = Vec::with_capacity(attempt_ids.len());
    for attempt_id in attempt_ids {
        let attempt = load_attempt(&connection, &attempt_id)?;
        let reservation = load_reservation(&connection, &attempt_id)?;
        let conditions = load_conditions(&connection, &attempt_id)?;
        let (artifact_count, artifact_bytes, truncated_artifacts) = connection
            .query_row(
                "SELECT COUNT(*),COALESCE(SUM(byte_length),0),COALESCE(SUM(truncated),0) FROM artifacts WHERE attempt_id=?1",
                [&attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| RuntimeError::from_sql(error, "summarize Attempt Artifacts"))?;
        attempts.push(RuntimeInspectionAttempt {
            attempt_id: attempt.attempt_id,
            attempt_number: attempt.attempt_number,
            state: attempt.state,
            termination_intent: attempt.termination_intent,
            created_at_ms: attempt.created_at_ms,
            started_at_ms: attempt.started_at_ms,
            finished_at_ms: attempt.finished_at_ms,
            duration_ms: attempt
                .finished_at_ms
                .map(|finished| finished.saturating_sub(attempt.created_at_ms)),
            exit_code: attempt.exit_code,
            result_available: attempt.result_digest.is_some(),
            reservation_state: reservation.state,
            reservation_release_reason: reservation.release_reason,
            conditions,
            artifact_count,
            artifact_bytes,
            truncated_artifacts,
        });
    }

    let artifacts = summarize_job_artifacts(&connection, job_id)?;
    let episodes = summarize_job_episodes(&connection, job_id)?;
    let total_events: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_events WHERE job_id=?1",
            [job_id],
            |row| row.get(0),
        )
        .map_err(|error| RuntimeError::from_sql(error, "count Job events"))?;
    let timeline = load_timeline(
        &connection,
        job_id,
        job.created_at_ms,
        event_limit,
        include_detail,
    )?;
    let recovery_required: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM attempt_conditions c JOIN attempts a ON a.attempt_id=c.attempt_id WHERE a.job_id=?1 AND c.condition_type='recovery_required' AND c.status='true'",
            [job_id],
            |row| row.get(0),
        )
        .map_err(|error| RuntimeError::from_sql(error, "count active Job recovery conditions"))?;
    let mechanically_converged = job.resolution.is_some()
        && !attempts_truncated
        && recovery_required == 0
        && attempts.iter().all(|attempt| attempt.state.is_terminal())
        && attempts
            .iter()
            .all(|attempt| attempt.reservation_state == ReservationState::Released);

    Ok(RuntimeJobInspection {
        schema_version: RUNTIME_INSPECTION_SCHEMA_VERSION,
        generated_at_ms: now_ms()?,
        migration_version,
        job: RuntimeInspectionJob {
            job_id: job.job_id,
            client_request_id: job.client_request_id,
            operation_digest: job.operation_digest,
            workspace_id: job.workspace_id,
            created_at_ms: job.created_at_ms,
            desired_state: job.desired_state,
            resolution: job.resolution,
            mechanically_converged,
            semantic_completion_evaluated: false,
        },
        attempts,
        attempts_truncated,
        artifacts,
        episodes,
        timeline,
        events_truncated: total_events > u64::from(event_limit),
    })
}

pub fn summarize_experience(
    config: &RuntimeInspectionConfig,
    since_ms: u64,
) -> RuntimeResult<RuntimeExperienceSummary> {
    let (connection, migration_version) = open_read_only(config)?;
    let jobs_total = count(
        &connection,
        "SELECT COUNT(*) FROM jobs WHERE created_at_ms>=?1",
        since_ms,
        "count summary Jobs",
    )?;
    let jobs_unresolved = count(
        &connection,
        "SELECT COUNT(*) FROM jobs WHERE created_at_ms>=?1 AND resolution IS NULL",
        since_ms,
        "count unresolved summary Jobs",
    )?;
    let jobs_recovery_required = count(
        &connection,
        "SELECT COUNT(DISTINCT j.job_id) FROM jobs j JOIN attempts a ON a.job_id=j.job_id JOIN attempt_conditions c ON c.attempt_id=a.attempt_id WHERE j.created_at_ms>=?1 AND c.condition_type='recovery_required' AND c.status='true'",
        since_ms,
        "count recovery-required summary Jobs",
    )?;
    let jobs_capacity_held = count(
        &connection,
        "SELECT COUNT(DISTINCT j.job_id) FROM jobs j JOIN attempts a ON a.job_id=j.job_id JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE j.created_at_ms>=?1 AND r.state!='released'",
        since_ms,
        "count capacity-held summary Jobs",
    )?;
    let jobs_converged = count(
        &connection,
        "SELECT COUNT(*) FROM jobs j WHERE j.created_at_ms>=?1 AND j.resolution IS NOT NULL AND NOT EXISTS(SELECT 1 FROM attempts a WHERE a.job_id=j.job_id AND a.state NOT IN ('succeeded','failed','timed_out','cancelled','lost','orphaned')) AND NOT EXISTS(SELECT 1 FROM attempts a JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE a.job_id=j.job_id AND r.state!='released') AND NOT EXISTS(SELECT 1 FROM attempts a JOIN attempt_conditions c ON c.attempt_id=a.attempt_id WHERE a.job_id=j.job_id AND c.condition_type='recovery_required' AND c.status='true')",
        since_ms,
        "count converged summary Jobs",
    )?;

    let resolutions = grouped_counts(
        &connection,
        "SELECT COALESCE(resolution,'unresolved'),COUNT(*) FROM jobs WHERE created_at_ms>=?1 GROUP BY COALESCE(resolution,'unresolved') ORDER BY 1",
        since_ms,
        "group Job resolutions",
    )?;
    let attempts = grouped_counts(
        &connection,
        "SELECT a.state,COUNT(*) FROM attempts a JOIN jobs j ON j.job_id=a.job_id WHERE j.created_at_ms>=?1 GROUP BY a.state ORDER BY a.state",
        since_ms,
        "group Attempt states",
    )?;
    let reservations = grouped_counts(
        &connection,
        "SELECT r.state,COUNT(*) FROM concurrency_reservations r JOIN attempts a ON a.attempt_id=r.attempt_id JOIN jobs j ON j.job_id=a.job_id WHERE j.created_at_ms>=?1 GROUP BY r.state ORDER BY r.state",
        since_ms,
        "group reservation states",
    )?;

    let recovery_failures = count(
        &connection,
        "SELECT COUNT(DISTINCT e.job_id) FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 AND e.event_type='RECONCILIATION_FAILED'",
        since_ms,
        "count Jobs with reconciliation failure",
    )?;
    let automatic_recoveries = count(
        &connection,
        "SELECT COUNT(*) FROM jobs j WHERE j.created_at_ms>=?1 AND EXISTS(SELECT 1 FROM job_events failure WHERE failure.job_id=j.job_id AND failure.event_type='RECONCILIATION_FAILED') AND EXISTS(SELECT 1 FROM job_events recovery WHERE recovery.job_id=j.job_id AND recovery.event_type IN ('RECONCILIATION_CONVERGED','RUNNER_RESULT_RECOVERED','JOB_RESOLUTION_CORRECTED') AND recovery.event_sequence>(SELECT MIN(failure.event_sequence) FROM job_events failure WHERE failure.job_id=j.job_id AND failure.event_type='RECONCILIATION_FAILED')) AND NOT EXISTS(SELECT 1 FROM job_events repair WHERE repair.job_id=j.job_id AND repair.event_type='ADMIN_TERMINAL_REPAIR')",
        since_ms,
        "count automatically recovered Jobs",
    )?;
    let admin_repairs = count(
        &connection,
        "SELECT COUNT(DISTINCT e.job_id) FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 AND e.event_type='ADMIN_TERMINAL_REPAIR'",
        since_ms,
        "count administratively repaired Jobs",
    )?;

    let dispatches = count(
        &connection,
        "SELECT COUNT(*) FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 AND e.event_type='DISPATCH_ISSUED'",
        since_ms,
        "count dispatch events",
    )?;
    let (jobs_with_duplicate_dispatch, duplicate_dispatches): (u64, u64) = connection
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(extra),0) FROM (SELECT e.job_id,e.attempt_id,COUNT(*)-1 extra FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 AND e.event_type='DISPATCH_ISSUED' GROUP BY e.job_id,e.attempt_id HAVING COUNT(*)>1)",
            [since_ms],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| RuntimeError::from_sql(error, "summarize duplicate dispatches"))?;

    let cancellation_requested = count(
        &connection,
        "SELECT COUNT(*) FROM jobs WHERE created_at_ms>=?1 AND desired_state='cancelled'",
        since_ms,
        "count cancellation requests",
    )?;
    let cancellation_cancelled = count(
        &connection,
        "SELECT COUNT(*) FROM jobs WHERE created_at_ms>=?1 AND desired_state='cancelled' AND resolution='cancelled'",
        since_ms,
        "count cancelled resolutions",
    )?;
    let cancellation_other = count(
        &connection,
        "SELECT COUNT(*) FROM jobs WHERE created_at_ms>=?1 AND desired_state='cancelled' AND resolution IS NOT NULL AND resolution!='cancelled'",
        since_ms,
        "count cancellation races resolved otherwise",
    )?;
    let cancellation_unresolved = cancellation_requested
        .saturating_sub(cancellation_cancelled)
        .saturating_sub(cancellation_other);

    let mechanical_latency = collect_mechanical_latency_samples(&connection, since_ms)?;

    let mut durations: Vec<u64> = Vec::new();
    let mut statement = connection
        .prepare(
            "SELECT a.finished_at_ms-j.created_at_ms FROM jobs j JOIN attempts a ON a.job_id=j.job_id WHERE j.created_at_ms>=?1 AND a.attempt_number=(SELECT MAX(latest.attempt_number) FROM attempts latest WHERE latest.job_id=j.job_id) AND a.finished_at_ms IS NOT NULL ORDER BY 1",
        )
        .map_err(|error| RuntimeError::from_sql(error, "prepare Job duration summary"))?;
    let rows = statement
        .query_map([since_ms], |row| row.get(0))
        .map_err(|error| RuntimeError::from_sql(error, "query Job durations"))?;
    for row in rows {
        durations.push(row.map_err(|error| RuntimeError::from_sql(error, "decode Job duration"))?);
    }

    let (artifact_count, artifact_bytes, truncated_artifacts): (u64, u64, u64) = connection
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(ar.byte_length),0),COALESCE(SUM(ar.truncated),0) FROM artifacts ar JOIN jobs j ON j.job_id=ar.job_id WHERE j.created_at_ms>=?1",
            [since_ms],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| RuntimeError::from_sql(error, "summarize Runtime Artifacts"))?;

    let event_types = grouped_counts(
        &connection,
        "SELECT e.event_type,COUNT(*) FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 GROUP BY e.event_type ORDER BY e.event_type",
        since_ms,
        "group Runtime event types",
    )?;
    let terminal_reasons = grouped_counts(
        &connection,
        "SELECT e.reason_code,COUNT(*) FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 AND e.event_type='JOB_TERMINAL' GROUP BY e.reason_code ORDER BY e.reason_code",
        since_ms,
        "group terminal reasons",
    )?;

    Ok(RuntimeExperienceSummary {
        schema_version: RUNTIME_INSPECTION_SCHEMA_VERSION,
        generated_at_ms: now_ms()?,
        migration_version,
        since_ms,
        jobs: RuntimeExperienceJobSummary {
            total: jobs_total,
            converged: jobs_converged,
            unresolved: jobs_unresolved,
            recovery_required: jobs_recovery_required,
            capacity_held: jobs_capacity_held,
            convergence_rate_basis_points: rate_basis_points(jobs_converged, jobs_total),
        },
        resolutions,
        attempts,
        reservations,
        recovery: RuntimeExperienceRecoverySummary {
            jobs_with_reconciliation_failure: recovery_failures,
            jobs_with_automatic_recovery: automatic_recoveries,
            jobs_with_administrative_repair: admin_repairs,
            automatic_recovery_rate_basis_points: rate_basis_points(
                automatic_recoveries,
                recovery_failures,
            ),
        },
        dispatch: RuntimeExperienceDispatchSummary {
            dispatches,
            jobs_with_duplicate_dispatch,
            duplicate_dispatches,
        },
        cancellation: RuntimeExperienceCancellationSummary {
            requested: cancellation_requested,
            resolved_cancelled: cancellation_cancelled,
            resolved_other: cancellation_other,
            unresolved: cancellation_unresolved,
        },
        mechanical_latency_ms: RuntimeExperienceMechanicalLatencySummary {
            admission_to_dispatch: duration_summary(&mechanical_latency.admission_to_dispatch),
            dispatch_to_runner_bound: duration_summary(
                &mechanical_latency.dispatch_to_runner_bound,
            ),
            runner_bound_to_terminal: duration_summary(
                &mechanical_latency.runner_bound_to_terminal,
            ),
            cancellation_to_terminal: duration_summary(
                &mechanical_latency.cancellation_to_terminal,
            ),
            reconciliation_to_convergence: duration_summary(
                &mechanical_latency.reconciliation_to_convergence,
            ),
        },
        duration_ms: duration_summary(&durations),
        artifacts: RuntimeExperienceArtifactSummary {
            count: artifact_count,
            bytes: artifact_bytes,
            truncated: truncated_artifacts,
        },
        event_types,
        terminal_reasons,
        semantic_completion_evaluated: false,
    })
}

fn collect_mechanical_latency_samples(
    connection: &Connection,
    since_ms: u64,
) -> RuntimeResult<RuntimeMechanicalLatencySamples> {
    let mut statement = connection
        .prepare(
            "SELECT e.job_id,j.created_at_ms,e.event_type,e.observed_at_ms FROM job_events e JOIN jobs j ON j.job_id=e.job_id WHERE j.created_at_ms>=?1 ORDER BY e.job_id,e.event_sequence",
        )
        .map_err(|error| RuntimeError::from_sql(error, "prepare mechanical latency events"))?;
    let rows = statement
        .query_map([since_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })
        .map_err(|error| RuntimeError::from_sql(error, "query mechanical latency events"))?;

    let mut samples = RuntimeMechanicalLatencySamples::default();
    let mut current_job: Option<String> = None;
    let mut state = RuntimeJobLatencyState::default();
    for row in rows {
        let (job_id, created_at_ms, event_type, observed_at_ms) =
            row.map_err(|error| RuntimeError::from_sql(error, "decode mechanical latency event"))?;
        if current_job.as_deref() != Some(job_id.as_str()) {
            if current_job.is_some() {
                append_job_latency_samples(&state, &mut samples);
            }
            current_job = Some(job_id);
            state = RuntimeJobLatencyState {
                created_at_ms,
                ..RuntimeJobLatencyState::default()
            };
        }
        match event_type.as_str() {
            "DISPATCH_ISSUED" => {
                state.dispatch_at_ms.get_or_insert(observed_at_ms);
            }
            "RUNNER_BOUND" => {
                state.runner_bound_at_ms.get_or_insert(observed_at_ms);
            }
            "STOP_REQUESTED" => {
                state.cancellation_at_ms.get_or_insert(observed_at_ms);
            }
            "RECONCILIATION_FAILED" => {
                state
                    .reconciliation_failed_at_ms
                    .get_or_insert(observed_at_ms);
            }
            "JOB_TERMINAL" => {
                state.terminal_at_ms.get_or_insert(observed_at_ms);
            }
            "RECONCILIATION_CONVERGED" | "RUNNER_RESULT_RECOVERED" | "JOB_RESOLUTION_CORRECTED" => {
                if let Some(failed_at_ms) = state.reconciliation_failed_at_ms.take() {
                    append_interval(
                        failed_at_ms,
                        Some(observed_at_ms),
                        &mut samples.reconciliation_to_convergence,
                    );
                }
            }
            _ => {}
        }
    }
    if current_job.is_some() {
        append_job_latency_samples(&state, &mut samples);
    }
    for values in [
        &mut samples.admission_to_dispatch,
        &mut samples.dispatch_to_runner_bound,
        &mut samples.runner_bound_to_terminal,
        &mut samples.cancellation_to_terminal,
        &mut samples.reconciliation_to_convergence,
    ] {
        values.sort_unstable();
    }
    Ok(samples)
}

fn append_job_latency_samples(
    state: &RuntimeJobLatencyState,
    samples: &mut RuntimeMechanicalLatencySamples,
) {
    append_interval(
        state.created_at_ms,
        state.dispatch_at_ms,
        &mut samples.admission_to_dispatch,
    );
    if let Some(dispatch_at_ms) = state.dispatch_at_ms {
        append_interval(
            dispatch_at_ms,
            state.runner_bound_at_ms,
            &mut samples.dispatch_to_runner_bound,
        );
    }
    if let Some(runner_bound_at_ms) = state.runner_bound_at_ms {
        append_interval(
            runner_bound_at_ms,
            state.terminal_at_ms,
            &mut samples.runner_bound_to_terminal,
        );
    }
    if let Some(cancellation_at_ms) = state.cancellation_at_ms {
        append_interval(
            cancellation_at_ms,
            state.terminal_at_ms,
            &mut samples.cancellation_to_terminal,
        );
    }
}

fn append_interval(start_ms: u64, end_ms: Option<u64>, values: &mut Vec<u64>) {
    if let Some(end_ms) = end_ms.filter(|end_ms| *end_ms >= start_ms) {
        values.push(end_ms - start_ms);
    }
}

fn duration_summary(values: &[u64]) -> RuntimeExperienceDurationSummary {
    RuntimeExperienceDurationSummary {
        samples: values.len() as u64,
        p50: percentile(values, 50),
        p95: percentile(values, 95),
        max: values.last().copied(),
    }
}

fn open_read_only(config: &RuntimeInspectionConfig) -> RuntimeResult<(Connection, i64)> {
    if !config.db_path.is_absolute() {
        return Err(RuntimeError::invalid(
            "database path must be absolute",
            "database",
        ));
    }
    if config.busy_timeout_ms == 0 {
        return Err(RuntimeError::invalid(
            "busyTimeoutMs must be positive",
            "busyTimeoutMs",
        ));
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(&config.db_path, flags)
        .map_err(|error| RuntimeError::from_sql(error, "cannot open Runtime Registry read-only"))?;
    connection
        .busy_timeout(Duration::from_millis(config.busy_timeout_ms))
        .map_err(|error| RuntimeError::from_sql(error, "cannot set inspection busy timeout"))?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|error| {
            RuntimeError::from_sql(error, "cannot enable inspection query-only mode")
        })?;
    connection
        .pragma_update(None, "trusted_schema", false)
        .map_err(|error| RuntimeError::from_sql(error, "cannot disable trusted schema"))?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| RuntimeError::from_sql(error, "cannot inspect Registry integrity"))?;
    if integrity != "ok" {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("Registry integrity check returned {integrity}"),
            None,
            false,
        ));
    }
    let migration_version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
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
    Ok((connection, migration_version))
}

fn load_conditions(
    connection: &Connection,
    attempt_id: &str,
) -> RuntimeResult<Vec<RuntimeInspectionCondition>> {
    let mut statement = connection
        .prepare(
            "SELECT condition_type,status,reason_code,observed_at_ms FROM attempt_conditions WHERE attempt_id=?1 ORDER BY condition_type",
        )
        .map_err(|error| RuntimeError::from_sql(error, "prepare Attempt condition inspection"))?;
    let rows = statement
        .query_map([attempt_id], |row| {
            Ok(RuntimeInspectionCondition {
                condition_type: row.get(0)?,
                status: row.get(1)?,
                reason_code: row.get(2)?,
                observed_at_ms: row.get(3)?,
            })
        })
        .map_err(|error| RuntimeError::from_sql(error, "query Attempt conditions"))?;
    rows.map(|row| row.map_err(|error| RuntimeError::from_sql(error, "decode Attempt condition")))
        .collect()
}

fn summarize_job_artifacts(
    connection: &Connection,
    job_id: &str,
) -> RuntimeResult<RuntimeInspectionArtifactSummary> {
    let (count, bytes, truncated): (u64, u64, u64) = connection
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(byte_length),0),COALESCE(SUM(truncated),0) FROM artifacts WHERE job_id=?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| RuntimeError::from_sql(error, "summarize Job Artifacts"))?;
    let mut by_kind = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT kind,COUNT(*) FROM artifacts WHERE job_id=?1 GROUP BY kind ORDER BY kind")
        .map_err(|error| RuntimeError::from_sql(error, "prepare Job Artifact kinds"))?;
    let rows = statement
        .query_map([job_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| RuntimeError::from_sql(error, "query Job Artifact kinds"))?;
    for row in rows {
        let (kind, value) =
            row.map_err(|error| RuntimeError::from_sql(error, "decode Job Artifact kind"))?;
        by_kind.insert(kind, value);
    }
    Ok(RuntimeInspectionArtifactSummary {
        count,
        bytes,
        truncated,
        by_kind,
    })
}

fn summarize_job_episodes(
    connection: &Connection,
    job_id: &str,
) -> RuntimeResult<RuntimeInspectionEpisodes> {
    let counts = grouped_job_event_counts(connection, job_id)?;
    let duplicate_dispatches: u64 = connection
        .query_row(
            "SELECT COALESCE(SUM(extra),0) FROM (SELECT COUNT(*)-1 extra FROM job_events WHERE job_id=?1 AND event_type='DISPATCH_ISSUED' GROUP BY attempt_id HAVING COUNT(*)>1)",
            [job_id],
            |row| row.get(0),
        )
        .map_err(|error| RuntimeError::from_sql(error, "summarize Job duplicate dispatches"))?;
    Ok(RuntimeInspectionEpisodes {
        dispatches: count_key(&counts, "DISPATCH_ISSUED"),
        duplicate_dispatches,
        stop_requests: count_key(&counts, "STOP_REQUESTED"),
        reconciliation_failures: count_key(&counts, "RECONCILIATION_FAILED"),
        reconciliation_convergences: count_key(&counts, "RECONCILIATION_CONVERGED"),
        runner_result_recoveries: count_key(&counts, "RUNNER_RESULT_RECOVERED"),
        resolution_corrections: count_key(&counts, "JOB_RESOLUTION_CORRECTED"),
        administrative_repairs: count_key(&counts, "ADMIN_TERMINAL_REPAIR"),
    })
}

fn load_timeline(
    connection: &Connection,
    job_id: &str,
    created_at_ms: u64,
    event_limit: u32,
    include_detail: bool,
) -> RuntimeResult<Vec<RuntimeInspectionEvent>> {
    let mut statement = connection
        .prepare(
            "SELECT event_sequence,attempt_id,event_type,origin,previous_state,new_state,reason_code,detail_json,observed_at_ms FROM job_events WHERE job_id=?1 ORDER BY event_sequence LIMIT ?2",
        )
        .map_err(|error| RuntimeError::from_sql(error, "prepare Job timeline"))?;
    let rows = statement
        .query_map(params![job_id, event_limit], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, u64>(8)?,
            ))
        })
        .map_err(|error| RuntimeError::from_sql(error, "query Job timeline"))?;
    let mut result = Vec::new();
    let mut previous_at = created_at_ms;
    for row in rows {
        let (
            sequence,
            attempt_id,
            event_type,
            origin,
            previous_state,
            new_state,
            reason_code,
            detail_json,
            observed_at_ms,
        ) = row.map_err(|error| RuntimeError::from_sql(error, "decode Job timeline"))?;
        let detail = if include_detail {
            Some(serde_json::from_str(&detail_json).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    format!("Job event detail is invalid JSON: {error}"),
                    Some("detailJson"),
                    false,
                )
            })?)
        } else {
            None
        };
        result.push(RuntimeInspectionEvent {
            sequence,
            attempt_id,
            event_type,
            origin,
            previous_state,
            new_state,
            reason_code,
            observed_at_ms,
            elapsed_ms: observed_at_ms.saturating_sub(created_at_ms),
            delta_ms: observed_at_ms.saturating_sub(previous_at),
            detail,
        });
        previous_at = observed_at_ms;
    }
    Ok(result)
}

fn grouped_job_event_counts(
    connection: &Connection,
    job_id: &str,
) -> RuntimeResult<BTreeMap<String, u64>> {
    let mut statement = connection
        .prepare(
            "SELECT event_type,COUNT(*) FROM job_events WHERE job_id=?1 GROUP BY event_type ORDER BY event_type",
        )
        .map_err(|error| RuntimeError::from_sql(error, "prepare Job event counts"))?;
    let rows = statement
        .query_map([job_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| RuntimeError::from_sql(error, "query Job event counts"))?;
    let mut counts = BTreeMap::new();
    for row in rows {
        let (event_type, value) =
            row.map_err(|error| RuntimeError::from_sql(error, "decode Job event count"))?;
        counts.insert(event_type, value);
    }
    Ok(counts)
}

fn grouped_counts(
    connection: &Connection,
    sql: &str,
    since_ms: u64,
    context: &str,
) -> RuntimeResult<BTreeMap<String, u64>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| RuntimeError::from_sql(error, context))?;
    let rows = statement
        .query_map([since_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| RuntimeError::from_sql(error, context))?;
    let mut counts = BTreeMap::new();
    for row in rows {
        let (key, value) = row.map_err(|error| RuntimeError::from_sql(error, context))?;
        counts.insert(key, value);
    }
    Ok(counts)
}

fn count(connection: &Connection, sql: &str, since_ms: u64, context: &str) -> RuntimeResult<u64> {
    connection
        .query_row(sql, [since_ms], |row| row.get(0))
        .map_err(|error| RuntimeError::from_sql(error, context))
}

fn count_key(counts: &BTreeMap<String, u64>, key: &str) -> u64 {
    counts.get(key).copied().unwrap_or(0)
}

fn rate_basis_points(numerator: u64, denominator: u64) -> u64 {
    numerator
        .saturating_mul(10_000)
        .checked_div(denominator)
        .unwrap_or(0)
}

fn percentile(sorted: &[u64], percentile: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = percentile.saturating_mul(sorted.len()).div_ceil(100);
    sorted.get(rank.saturating_sub(1)).copied()
}

fn now_ms() -> RuntimeResult<u64> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                format!("system clock is before the Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis();
    u64::try_from(value).map_err(|_| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryUnavailable,
            "current time exceeds the Runtime timestamp range",
            None,
            false,
        )
    })
}
