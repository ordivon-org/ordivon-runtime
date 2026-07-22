use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::universal::sha256_bytes;
use crate::{
    AttemptState, M6Error, M6ErrorCode, M6Registry, M6Result, M7RuntimeHardeningConfig,
    ReservationState,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7OrphanEvidence {
    pub attempt_id: String,
    pub evidence_digest: String,
    pub recorded_unit: String,
    pub observed_invocation_id: Option<String>,
    pub invocation_matches: bool,
    pub recorded_cgroup: Option<String>,
    pub live_processes: Vec<u32>,
    pub recorded_pid_alive: bool,
    pub recorded_pid_identity_matches: bool,
    pub unit_active: bool,
    pub cgroup_exists: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7RemediationResult {
    pub attempt_id: String,
    pub reservation_released: bool,
    pub evidence_digest: String,
    pub termination_requested: bool,
}

#[derive(Clone)]
pub struct M7OrphanRemediator {
    registry: M6Registry,
    hardening: M7RuntimeHardeningConfig,
}

struct RemediationRecord<'a> {
    attempt_id: &'a str,
    operator_id: &'a str,
    expected_digest: &'a str,
    observed_digest: &'a str,
    action: &'a str,
    outcome: &'a str,
    detail: serde_json::Value,
    observed_at_ms: u64,
}

impl M7OrphanRemediator {
    pub fn new(registry: M6Registry, hardening: M7RuntimeHardeningConfig) -> M6Result<Self> {
        hardening.validate()?;
        Ok(Self {
            registry,
            hardening,
        })
    }

    pub fn inspect(
        &self,
        attempt_id: &str,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<M7OrphanEvidence> {
        validate_operator(operator_id)?;
        let evidence = self.observe(attempt_id)?;
        self.record(RemediationRecord {
            attempt_id,
            operator_id,
            expected_digest: &evidence.evidence_digest,
            observed_digest: &evidence.evidence_digest,
            action: "inspect",
            outcome: "observed",
            detail: serde_json::json!({
                "unitActive": evidence.unit_active,
                "invocationMatches": evidence.invocation_matches,
                "liveProcesses": evidence.live_processes,
                "recordedPidAlive": evidence.recorded_pid_alive,
                "recordedPidIdentityMatches": evidence.recorded_pid_identity_matches,
            }),
            observed_at_ms,
        })?;
        Ok(evidence)
    }

    pub fn remediate(
        &self,
        attempt_id: &str,
        operator_id: &str,
        expected_evidence_digest: &str,
        terminate_matching_unit: bool,
        observed_at_ms: u64,
    ) -> M6Result<M7RemediationResult> {
        validate_operator(operator_id)?;
        let initial = self.observe(attempt_id)?;
        if initial.evidence_digest != expected_evidence_digest {
            self.record(RemediationRecord {
                attempt_id,
                operator_id,
                expected_digest: expected_evidence_digest,
                observed_digest: &initial.evidence_digest,
                action: "release",
                outcome: "denied",
                detail: serde_json::json!({"reason": "EVIDENCE_CHANGED"}),
                observed_at_ms,
            })?;
            return Err(M6Error::new(
                M6ErrorCode::OrphanRemediationDenied,
                "orphan evidence changed after operator inspection",
                Some("expectedEvidenceDigest"),
                false,
            ));
        }

        let mut termination_requested = false;
        if initial.unit_active && initial.invocation_matches && terminate_matching_unit {
            let attempt = self.registry.get_attempt(attempt_id)?;
            let output = Command::new("systemctl")
                .args(["stop", &attempt.unit_name])
                .output()
                .map_err(|error| tool_error("cannot stop orphaned unit", error))?;
            if !output.status.success() {
                self.record(RemediationRecord {
                    attempt_id,
                    operator_id,
                    expected_digest: expected_evidence_digest,
                    observed_digest: &initial.evidence_digest,
                    action: "terminate",
                    outcome: "failed",
                    detail: serde_json::json!({
                        "stderrDigest": sha256_bytes(&output.stderr),
                    }),
                    observed_at_ms,
                })?;
                return Err(M6Error::new(
                    M6ErrorCode::OrphanRemediationDenied,
                    "systemd refused orphan termination",
                    Some("attemptId"),
                    true,
                ));
            }
            termination_requested = true;
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                let evidence = self.observe(attempt_id)?;
                if !has_original_process_evidence(&evidence) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }

        let final_evidence = self.observe(attempt_id)?;
        if has_original_process_evidence(&final_evidence) {
            self.record(RemediationRecord {
                attempt_id,
                operator_id,
                expected_digest: expected_evidence_digest,
                observed_digest: &final_evidence.evidence_digest,
                action: "release",
                outcome: "denied",
                detail: serde_json::json!({"reason": "PROCESS_TREE_STILL_LIVE"}),
                observed_at_ms,
            })?;
            return Err(M6Error::new(
                M6ErrorCode::OrphanRemediationDenied,
                "orphan reservation remains held because the original process tree is still live",
                Some("attemptId"),
                true,
            ));
        }
        self.release_reservation(
            attempt_id,
            operator_id,
            expected_evidence_digest,
            &final_evidence,
            termination_requested,
            observed_at_ms,
        )?;
        Ok(M7RemediationResult {
            attempt_id: attempt_id.to_string(),
            reservation_released: true,
            evidence_digest: final_evidence.evidence_digest,
            termination_requested,
        })
    }

    fn observe(&self, attempt_id: &str) -> M6Result<M7OrphanEvidence> {
        let attempt = self.registry.get_attempt(attempt_id)?;
        if attempt.state != AttemptState::Orphaned {
            return Err(M6Error::new(
                M6ErrorCode::OrphanRemediationDenied,
                "only orphaned Attempts may enter operator remediation",
                Some("attemptId"),
                false,
            ));
        }
        let reservation = self.registry.get_reservation(attempt_id)?;
        if reservation.state != ReservationState::HeldOrphaned {
            return Err(M6Error::new(
                M6ErrorCode::ReservationStateConflict,
                "orphaned Attempt does not hold capacity",
                Some("attemptId"),
                false,
            ));
        }
        let properties = systemd_properties(&attempt.unit_name)?;
        let observed_invocation_id = nonempty(&properties, "InvocationID");
        let active_state = properties
            .get("ActiveState")
            .map(String::as_str)
            .unwrap_or("unknown");
        let unit_active = matches!(
            active_state,
            "active" | "activating" | "deactivating" | "reloading"
        );
        let invocation_matches = match (&attempt.invocation_id, &observed_invocation_id) {
            (Some(expected), Some(observed)) => expected == observed,
            _ => false,
        };
        let recorded_cgroup = attempt.control_group.clone();
        let (cgroup_exists, live_processes) = recorded_cgroup
            .as_deref()
            .map(cgroup_processes)
            .transpose()?
            .unwrap_or((false, Vec::new()));
        let recorded_pid_alive = attempt
            .main_pid
            .is_some_and(|pid| Path::new(&format!("/proc/{pid}")).exists());
        let recorded_pid_identity_matches = attempt.main_pid.is_some_and(|pid| {
            process_identity(pid)
                .as_deref()
                .zip(attempt.process_start_identity.as_deref())
                .is_some_and(|(observed, expected)| observed == expected)
        });
        let material = serde_json::json!({
            "attemptId": attempt.attempt_id,
            "unitName": attempt.unit_name,
            "expectedInvocationId": attempt.invocation_id,
            "observedInvocationId": observed_invocation_id,
            "invocationMatches": invocation_matches,
            "recordedCgroup": recorded_cgroup,
            "liveProcesses": live_processes,
            "recordedPid": attempt.main_pid,
            "recordedPidAlive": recorded_pid_alive,
            "recordedPidIdentityMatches": recorded_pid_identity_matches,
            "unitActive": unit_active,
            "cgroupExists": cgroup_exists,
        });
        let bytes = serde_json::to_vec(&material).map_err(serialization_error)?;
        Ok(M7OrphanEvidence {
            attempt_id: attempt_id.to_string(),
            evidence_digest: sha256_bytes(&bytes),
            recorded_unit: attempt.unit_name,
            observed_invocation_id,
            invocation_matches,
            recorded_cgroup,
            live_processes,
            recorded_pid_alive,
            recorded_pid_identity_matches,
            unit_active,
            cgroup_exists,
        })
    }

    fn release_reservation(
        &self,
        attempt_id: &str,
        operator_id: &str,
        expected_evidence_digest: &str,
        evidence: &M7OrphanEvidence,
        termination_requested: bool,
        observed_at_ms: u64,
    ) -> M6Result<()> {
        let mut connection = self.registry.open_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| M6Error::from_sql(error, "cannot begin orphan release"))?;
        let state: String = transaction
            .query_row(
                "SELECT state FROM attempts WHERE attempt_id=?1",
                [attempt_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot verify orphan state"))?
            .ok_or_else(|| {
                M6Error::new(
                    M6ErrorCode::AttemptNotFound,
                    "Attempt was not found during orphan release",
                    Some("attemptId"),
                    false,
                )
            })?;
        if state != "orphaned" {
            return Err(M6Error::new(
                M6ErrorCode::OrphanRemediationDenied,
                "Attempt changed after orphan inspection",
                Some("attemptId"),
                false,
            ));
        }
        let updated = transaction
            .execute(
                "UPDATE concurrency_reservations SET state='released',released_at_ms=?1,release_reason='OPERATOR_ORPHAN_REMEDIATED' WHERE attempt_id=?2 AND state='held_orphaned'",
                params![observed_at_ms, attempt_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot release orphan reservation"))?;
        if updated != 1 {
            return Err(M6Error::new(
                M6ErrorCode::ReservationStateConflict,
                "orphan reservation was not held",
                Some("attemptId"),
                false,
            ));
        }
        insert_remediation(
            &transaction,
            &RemediationRecord {
                attempt_id,
                operator_id,
                expected_digest: expected_evidence_digest,
                observed_digest: &evidence.evidence_digest,
                action: "release",
                outcome: "released",
                detail: serde_json::json!({"terminationRequested": termination_requested}),
                observed_at_ms,
            },
        )?;
        append_lifecycle_event(
            &transaction,
            "ORPHAN_RESERVATION_RELEASED",
            operator_id,
            attempt_id,
            serde_json::json!({
                "expectedEvidenceDigest": expected_evidence_digest,
                "observedEvidenceDigest": evidence.evidence_digest,
                "terminationRequested": termination_requested,
            }),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit orphan release"))?;
        if let Some(view) = attempt_view_path(&self.hardening, attempt_id) {
            let _ = fs::remove_dir_all(view);
        }
        Ok(())
    }

    fn record(&self, record: RemediationRecord<'_>) -> M6Result<()> {
        let connection = self.registry.open_connection()?;
        insert_remediation(&connection, &record)
    }
}

fn insert_remediation(
    connection: &rusqlite::Connection,
    record: &RemediationRecord<'_>,
) -> M6Result<()> {
    let detail_json = serde_json::to_string(&record.detail).map_err(serialization_error)?;
    connection
        .execute(
            "INSERT INTO m7_orphan_remediations(remediation_id,attempt_id,operator_id,expected_evidence_digest,observed_evidence_digest,action,outcome,detail_digest,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                format!("remediation-{}", Uuid::now_v7()),
                record.attempt_id,
                record.operator_id,
                record.expected_digest,
                record.observed_digest,
                record.action,
                record.outcome,
                sha256_bytes(detail_json.as_bytes()),
                record.observed_at_ms,
            ],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot record orphan remediation"))?;
    Ok(())
}

fn append_lifecycle_event(
    connection: &rusqlite::Connection,
    event_type: &str,
    operator_id: &str,
    attempt_id: &str,
    detail: serde_json::Value,
    observed_at_ms: u64,
) -> M6Result<()> {
    let detail_json = serde_json::to_string(&detail).map_err(serialization_error)?;
    let job_id: String = connection
        .query_row(
            "SELECT job_id FROM attempts WHERE attempt_id=?1",
            [attempt_id],
            |row| row.get(0),
        )
        .map_err(|error| M6Error::from_sql(error, "cannot load orphan Job"))?;
    connection
        .execute(
            "INSERT INTO m7_lifecycle_events(event_id,event_type,operator_id,job_id,attempt_id,detail_json,detail_digest,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                format!("lifecycle-event-{}", Uuid::now_v7()),
                event_type,
                operator_id,
                job_id,
                attempt_id,
                detail_json,
                sha256_bytes(detail_json.as_bytes()),
                observed_at_ms,
            ],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot append orphan lifecycle event"))?;
    Ok(())
}

fn systemd_properties(unit_name: &str) -> M6Result<BTreeMap<String, String>> {
    let output = Command::new("systemctl")
        .args([
            "show",
            unit_name,
            "--property=LoadState,ActiveState,SubState,InvocationID,ControlGroup,MainPID",
        ])
        .output()
        .map_err(|error| tool_error("cannot inspect orphaned unit", error))?;
    if !output.status.success() {
        return Ok(BTreeMap::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect())
}

fn nonempty(properties: &BTreeMap<String, String>, key: &str) -> Option<String> {
    properties
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn cgroup_processes(control_group: &str) -> M6Result<(bool, Vec<u32>)> {
    if !control_group.starts_with('/')
        || control_group
            .split('/')
            .any(|part| part == ".." || part.contains('\0'))
    {
        return Err(M6Error::new(
            M6ErrorCode::RegistryCorrupt,
            "recorded cgroup path is invalid",
            Some("attemptId"),
            false,
        ));
    }
    let path = Path::new("/sys/fs/cgroup")
        .join(control_group.trim_start_matches('/'))
        .join("cgroup.procs");
    if !path.is_file() {
        return Ok((false, Vec::new()));
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| tool_error("cannot read orphan cgroup", error))?;
    let mut processes = text
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect::<Vec<_>>();
    processes.sort_unstable();
    processes.dedup();
    Ok((true, processes))
}

fn process_identity(pid: u32) -> Option<String> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(") ")?;
    let fields = text[close + 2..].split_whitespace().collect::<Vec<_>>();
    let start_time = fields.get(19)?;
    Some(format!("pid:{pid}:start:{start_time}"))
}

fn has_original_process_evidence(evidence: &M7OrphanEvidence) -> bool {
    if evidence.unit_active && !evidence.invocation_matches {
        return evidence.recorded_pid_identity_matches;
    }
    !evidence.live_processes.is_empty()
        || evidence.recorded_pid_identity_matches
        || (evidence.unit_active && evidence.invocation_matches)
}

fn attempt_view_path(
    hardening: &M7RuntimeHardeningConfig,
    attempt_id: &str,
) -> Option<std::path::PathBuf> {
    (!attempt_id.contains('/') && !attempt_id.contains(".."))
        .then(|| hardening.payload_view_root(attempt_id))
}

fn validate_operator(value: &str) -> M6Result<()> {
    if value.len() < 3
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:./@".contains(&byte))
    {
        return Err(M6Error::invalid("invalid operator identity", "operatorId"));
    }
    Ok(())
}

fn serialization_error(error: serde_json::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::RegistryCorrupt,
        format!("cannot serialize orphan evidence: {error}"),
        None,
        false,
    )
}

fn tool_error(context: &str, error: std::io::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::ToolFailed,
        format!("{context}: {error}"),
        None,
        true,
    )
}
