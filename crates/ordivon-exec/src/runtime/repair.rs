use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::registry::{load_attempt, load_job, load_reservation, MAX_MIGRATION_VERSION};
use super::{
    inspect_runtime, ArtifactRegistration, AttemptState, Registry, RegistryConfig,
    ReservationState, RuntimeDoctorCase, RuntimeDoctorConfig, RuntimeDoctorProposal,
    RuntimeDoctorReport, RuntimeError, RuntimeErrorCode, RuntimeResult, TerminalCommit,
};
use crate::universal::{sha256_bytes, sha256_file, write_json_atomic};

pub const RUNTIME_REPAIR_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct RuntimeRepairConfig {
    pub doctor: RuntimeDoctorConfig,
}

#[derive(Clone, Debug)]
pub struct RuntimeRepairRequest {
    pub expected_fingerprint: String,
    pub snapshot_path: PathBuf,
    pub principal: String,
    pub finalize_lost_attempt_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRepairReport {
    pub schema_version: u32,
    pub applied_at_ms: u64,
    pub expected_fingerprint: String,
    pub snapshot_path: String,
    pub snapshot_digest: String,
    pub principal: String,
    pub actions: Vec<RuntimeRepairAction>,
    pub before: RuntimeDoctorReport,
    pub after: RuntimeDoctorReport,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRepairAction {
    pub job_id: String,
    pub attempt_id: String,
    pub case_fingerprint: String,
    pub kind: RuntimeRepairActionKind,
    pub previous_state: AttemptState,
    pub new_state: AttemptState,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeRepairActionKind {
    RecoverRunnerResult,
    ReleaseTerminalReservation,
    FinalizeLost,
}

#[derive(Clone, Debug)]
pub(crate) enum AdminRepairOperation {
    Terminal {
        terminal: TerminalCommit,
        audit: AdminRepairAudit,
    },
    Reservation {
        attempt_id: String,
        expected_attempt_row_version: u64,
        audit: AdminRepairAudit,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct AdminRepairAudit {
    pub report_fingerprint: String,
    pub case_fingerprint: String,
    pub snapshot_path: String,
    pub snapshot_digest: String,
    pub principal: String,
    pub action: String,
    pub observed_at_ms: u64,
    pub expected_job_row_version: u64,
    pub expected_current_attempt_id: Option<String>,
    pub expected_reservation_state: ReservationState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminRepairReceipt {
    schema_version: u32,
    report_fingerprint: String,
    case_fingerprint: String,
    snapshot_path: String,
    snapshot_digest: String,
    principal: String,
    action: String,
    job_id: String,
    attempt_id: String,
    observed_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct BackupManifest {
    files: Vec<BackupFile>,
}

#[derive(Debug, Deserialize)]
struct BackupFile {
    path: String,
    bytes: u64,
    digest: String,
}

pub fn apply_runtime_repair(
    config: &RuntimeRepairConfig,
    request: &RuntimeRepairRequest,
) -> RuntimeResult<RuntimeRepairReport> {
    validate_request(request)?;
    let before = inspect_runtime(&config.doctor)?;
    if before.migration_version != MAX_MIGRATION_VERSION {
        return Err(RuntimeError::new(
            RuntimeErrorCode::SchemaVersionUnsupported,
            format!(
                "Runtime repair requires schema {MAX_MIGRATION_VERSION}, observed {}",
                before.migration_version
            ),
            Some("migrationVersion"),
            false,
        ));
    }
    if before.fingerprint != request.expected_fingerprint {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            format!(
                "Doctor fingerprint changed: expected {}, observed {}",
                request.expected_fingerprint, before.fingerprint
            ),
            Some("expectedFingerprint"),
            false,
        ));
    }

    let scoped_violation_count: usize = before
        .cases
        .iter()
        .map(|case| case.violation_codes.len())
        .sum();
    if scoped_violation_count != before.violation_count {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "Doctor report contains invariant violations without an actionable Attempt Case",
            Some("violations"),
            false,
        ));
    }
    if before.cases.iter().any(|case| {
        !case.violation_codes.is_empty()
            && matches!(case.proposal, RuntimeDoctorProposal::NoRepairNeeded)
    }) {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "Doctor report contains a violating Case without a repair proposal",
            Some("cases"),
            false,
        ));
    }
    let snapshot_digest = verify_snapshot(&request.snapshot_path)?;
    verify_snapshot_cases(&request.snapshot_path, &before.cases)?;

    let manual_attempts: BTreeSet<String> = before
        .cases
        .iter()
        .filter(|case| matches!(case.proposal, RuntimeDoctorProposal::ManualReview { .. }))
        .map(|case| case.attempt.attempt_id.clone())
        .collect();
    for attempt_id in &request.finalize_lost_attempt_ids {
        if !manual_attempts.contains(attempt_id) {
            return Err(RuntimeError::invalid(
                format!("{attempt_id} is not a current manual-review Attempt"),
                "finalizeLostAttemptIds",
            ));
        }
    }
    let unselected: Vec<String> = manual_attempts
        .difference(&request.finalize_lost_attempt_ids)
        .cloned()
        .collect();
    if !unselected.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            format!(
                "manual-review Attempts require explicit --finalize-lost selection: {}",
                unselected.join(",")
            ),
            Some("finalizeLostAttemptIds"),
            false,
        ));
    }

    let registry = Registry::initialize(RegistryConfig {
        db_path: config.doctor.db_path.clone(),
        store_root: config.doctor.store_root.clone(),
        busy_timeout_ms: config.doctor.busy_timeout_ms,
    })?;
    let applied_at_ms = now_ms()?;
    let mut actions = Vec::new();
    let mut operations = Vec::new();
    for case in &before.cases {
        let audit = AdminRepairAudit {
            report_fingerprint: before.fingerprint.clone(),
            case_fingerprint: case.fingerprint.clone(),
            snapshot_path: request.snapshot_path.to_string_lossy().into_owned(),
            snapshot_digest: snapshot_digest.clone(),
            principal: request.principal.clone(),
            action: action_name(case, &request.finalize_lost_attempt_ids)?.to_string(),
            observed_at_ms: applied_at_ms,
            expected_job_row_version: case.job.row_version,
            expected_current_attempt_id: case.job.current_attempt_id.clone(),
            expected_reservation_state: case.reservation.state,
        };
        match &case.proposal {
            RuntimeDoctorProposal::RecoverRunnerResult { terminal } => {
                let mut terminal = terminal.clone();
                terminal.artifacts.push(write_admin_receipt(
                    &config.doctor.store_root,
                    case,
                    &audit,
                )?);
                operations.push(AdminRepairOperation::Terminal {
                    terminal: terminal.clone(),
                    audit,
                });
                actions.push(action(
                    case,
                    RuntimeRepairActionKind::RecoverRunnerResult,
                    terminal.state,
                ));
            }
            RuntimeDoctorProposal::ReleaseTerminalReservation => {
                operations.push(AdminRepairOperation::Reservation {
                    attempt_id: case.attempt.attempt_id.clone(),
                    expected_attempt_row_version: case.attempt.row_version,
                    audit,
                });
                actions.push(action(
                    case,
                    RuntimeRepairActionKind::ReleaseTerminalReservation,
                    case.attempt.state,
                ));
            }
            RuntimeDoctorProposal::ManualReview { .. } => {
                let terminal =
                    prepare_final_lost_terminal(&config.doctor.store_root, case, &audit)?;
                operations.push(AdminRepairOperation::Terminal { terminal, audit });
                actions.push(action(
                    case,
                    RuntimeRepairActionKind::FinalizeLost,
                    AttemptState::Lost,
                ));
            }
            RuntimeDoctorProposal::NoRepairNeeded => {}
        }
    }
    registry.repair_admin_batch(&operations)?;
    let after = inspect_runtime(&config.doctor)?;
    if after.violation_count != 0 {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            format!(
                "Runtime repair committed but {} invariant violations remain",
                after.violation_count
            ),
            None,
            false,
        ));
    }
    Ok(RuntimeRepairReport {
        schema_version: RUNTIME_REPAIR_SCHEMA_VERSION,
        applied_at_ms,
        expected_fingerprint: request.expected_fingerprint.clone(),
        snapshot_path: request.snapshot_path.to_string_lossy().into_owned(),
        snapshot_digest,
        principal: request.principal.clone(),
        actions,
        before,
        after,
    })
}

fn validate_request(request: &RuntimeRepairRequest) -> RuntimeResult<()> {
    if !is_sha256_digest(&request.expected_fingerprint) {
        return Err(RuntimeError::invalid(
            "expected fingerprint must be a canonical sha256 Digest",
            "expectedFingerprint",
        ));
    }
    if !request.snapshot_path.is_absolute() {
        return Err(RuntimeError::invalid(
            "snapshot path must be absolute",
            "snapshotPath",
        ));
    }
    if request.principal.trim().is_empty()
        || request.principal.len() > 256
        || request.principal.chars().any(char::is_control)
    {
        return Err(RuntimeError::invalid(
            "principal must contain 1..=256 non-control characters",
            "principal",
        ));
    }
    Ok(())
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn verify_snapshot(snapshot_path: &Path) -> RuntimeResult<String> {
    let manifest_path = snapshot_path.join("manifest.json");
    let manifest: BackupManifest = serde_json::from_slice(
        &fs::read(&manifest_path).map_err(|error| io_error("read backup manifest", error))?,
    )
    .map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("invalid backup manifest: {error}"),
            Some("snapshotPath"),
            false,
        )
    })?;
    let mut database_digest = None;
    for file in &manifest.files {
        let relative = Path::new(&file.path);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("backup manifest contains unsafe path {}", file.path),
                Some("snapshotPath"),
                false,
            ));
        }
        let path = snapshot_path.join(relative);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("cannot inspect backup file {}: {error}", file.path),
                Some("snapshotPath"),
                false,
            )
        })?;
        if !metadata.file_type().is_file() || metadata.len() != file.bytes {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!(
                    "backup file {} is missing, non-regular, or has the wrong size",
                    file.path
                ),
                Some("snapshotPath"),
                false,
            ));
        }
        let observed = sha256_file(&path).map_err(map_universal_error)?;
        if observed != file.digest {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("backup file {} Digest does not match manifest", file.path),
                Some("snapshotPath"),
                false,
            ));
        }
        if file.path == "registry.sqlite3" {
            database_digest = Some(observed);
        }
    }
    let observed = database_digest.ok_or_else(|| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "backup manifest does not contain registry.sqlite3",
            Some("snapshotPath"),
            false,
        )
    })?;
    let database_path = snapshot_path.join("registry.sqlite3");
    let connection = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| RuntimeError::from_sql(error, "cannot open backup Registry"))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| RuntimeError::from_sql(error, "cannot set backup busy timeout"))?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| RuntimeError::from_sql(error, "cannot verify backup Registry"))?;
    if integrity != "ok" {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("backup Registry integrity check returned {integrity}"),
            Some("snapshotPath"),
            false,
        ));
    }
    Ok(observed)
}

fn verify_snapshot_cases(snapshot_path: &Path, cases: &[RuntimeDoctorCase]) -> RuntimeResult<()> {
    let database_path = snapshot_path.join("registry.sqlite3");
    let connection = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        RuntimeError::from_sql(error, "cannot open backup Registry for plan validation")
    })?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| RuntimeError::from_sql(error, "cannot set backup plan busy timeout"))?;
    for case in cases {
        let job = load_job(&connection, &case.job.job_id)?;
        let attempt = load_attempt(&connection, &case.attempt.attempt_id)?;
        let reservation = load_reservation(&connection, &case.attempt.attempt_id)?;
        let matches = job.job_id == case.job.job_id
            && job.workspace_id == case.job.workspace_id
            && job.resolution == case.job.resolution
            && job.current_attempt_id == case.job.current_attempt_id
            && job.row_version == case.job.row_version
            && attempt.attempt_id == case.attempt.attempt_id
            && attempt.state == case.attempt.state
            && attempt.termination_intent == case.attempt.termination_intent
            && attempt.result_digest == case.attempt.result_digest
            && attempt.exit_code == case.attempt.exit_code
            && attempt.finished_at_ms == case.attempt.finished_at_ms
            && attempt.row_version == case.attempt.row_version
            && reservation.reservation_id == case.reservation.reservation_id
            && reservation.state == case.reservation.state
            && reservation.released_at_ms == case.reservation.released_at_ms
            && reservation.release_reason == case.reservation.release_reason;
        if !matches {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReconciliationRequired,
                format!(
                    "backup does not contain the exact Doctor state for Attempt {}",
                    case.attempt.attempt_id
                ),
                Some("snapshotPath"),
                false,
            ));
        }
    }
    Ok(())
}

fn action_name<'a>(
    case: &'a RuntimeDoctorCase,
    finalize_lost: &BTreeSet<String>,
) -> RuntimeResult<&'a str> {
    match case.proposal {
        RuntimeDoctorProposal::RecoverRunnerResult { .. } => Ok("recover_runner_result"),
        RuntimeDoctorProposal::ReleaseTerminalReservation => Ok("release_terminal_reservation"),
        RuntimeDoctorProposal::ManualReview { .. }
            if finalize_lost.contains(&case.attempt.attempt_id) =>
        {
            Ok("finalize_lost")
        }
        RuntimeDoctorProposal::ManualReview { .. } => Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "manual-review Attempt was not explicitly selected",
            Some("finalizeLostAttemptIds"),
            false,
        )),
        RuntimeDoctorProposal::NoRepairNeeded => Ok("no_repair"),
    }
}

fn write_admin_receipt(
    store_root: &Path,
    case: &RuntimeDoctorCase,
    audit: &AdminRepairAudit,
) -> RuntimeResult<ArtifactRegistration> {
    let bundle = store_root.join("attempts").join(&case.attempt.attempt_id);
    fs::create_dir_all(&bundle).map_err(|error| io_error("create repair Bundle", error))?;
    let relative_path = "admin-repair.json";
    let path = bundle.join(relative_path);
    let receipt = AdminRepairReceipt {
        schema_version: RUNTIME_REPAIR_SCHEMA_VERSION,
        report_fingerprint: audit.report_fingerprint.clone(),
        case_fingerprint: audit.case_fingerprint.clone(),
        snapshot_path: audit.snapshot_path.clone(),
        snapshot_digest: audit.snapshot_digest.clone(),
        principal: audit.principal.clone(),
        action: audit.action.clone(),
        job_id: case.job.job_id.clone(),
        attempt_id: case.attempt.attempt_id.clone(),
        observed_at_ms: audit.observed_at_ms,
    };
    write_json_atomic(&path, &receipt).map_err(map_universal_error)?;
    let metadata =
        fs::metadata(&path).map_err(|error| io_error("inspect repair receipt", error))?;
    Ok(ArtifactRegistration {
        artifact_id: format!("{}.admin-repair", case.attempt.attempt_id),
        kind: "admin_repair".to_string(),
        relative_path: relative_path.to_string(),
        digest: sha256_file(&path).map_err(map_universal_error)?,
        media_type: "application/json".to_string(),
        byte_length: metadata.len(),
        truncated: false,
    })
}

fn prepare_final_lost_terminal(
    store_root: &Path,
    case: &RuntimeDoctorCase,
    audit: &AdminRepairAudit,
) -> RuntimeResult<TerminalCommit> {
    if case.attempt.state != AttemptState::Lost
        || case.runner_result_present
        || case.job.resolution != Some(super::JobResolution::Lost)
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::OrphanRemediationDenied,
            "finalize-lost requires an existing Lost record without Runner result",
            Some("attemptId"),
            false,
        ));
    }
    let receipt = write_admin_receipt(store_root, case, audit)?;
    let artifacts = vec![receipt.clone()];
    Ok(TerminalCommit {
        attempt_id: case.attempt.attempt_id.clone(),
        expected_row_version: case.attempt.row_version,
        state: AttemptState::Lost,
        result_digest: receipt.digest,
        exit_code: None,
        infrastructure_error_digest: Some(sha256_bytes(
            b"admin confirmed lost: Runner result unavailable",
        )),
        finished_at_ms: audit.observed_at_ms,
        artifacts,
        reason_code: "ADMIN_CONFIRMED_LOST_NO_RUNNER_RESULT".to_string(),
    })
}

fn action(
    case: &RuntimeDoctorCase,
    kind: RuntimeRepairActionKind,
    new_state: AttemptState,
) -> RuntimeRepairAction {
    RuntimeRepairAction {
        job_id: case.job.job_id.clone(),
        attempt_id: case.attempt.attempt_id.clone(),
        case_fingerprint: case.fingerprint.clone(),
        kind,
        previous_state: case.attempt.state,
        new_state,
    }
}

fn map_universal_error(error: crate::UniversalExecError) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IoError,
        error.message,
        error.field.as_deref(),
        error.retryable,
    )
}

fn io_error(context: &str, error: std::io::Error) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IoError,
        format!("{context}: {error}"),
        None,
        false,
    )
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
