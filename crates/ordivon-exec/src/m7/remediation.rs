use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::{AttemptState, M6Error, M6ErrorCode, M6Registry, M6Result, ReservationState};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7RemediationResult {
    pub attempt_id: String,
    pub reservation_released: bool,
    pub termination_requested: bool,
}

#[derive(Clone)]
pub struct M7OrphanRemediator {
    registry: M6Registry,
}

#[derive(Debug)]
struct OrphanObservation {
    matching_unit_active: bool,
    live_cgroup_processes: Vec<u32>,
    recorded_pid_identity_matches: bool,
}

impl OrphanObservation {
    fn original_process_tree_alive(&self) -> bool {
        self.matching_unit_active
            || !self.live_cgroup_processes.is_empty()
            || self.recorded_pid_identity_matches
    }
}

impl M7OrphanRemediator {
    pub fn new(registry: M6Registry) -> Self {
        Self { registry }
    }

    pub fn remediate(
        &self,
        attempt_id: &str,
        terminate_matching_unit: bool,
        observed_at_ms: u64,
    ) -> M6Result<M7RemediationResult> {
        let attempt = self.registry.get_attempt(attempt_id)?;
        if attempt.state != AttemptState::Orphaned {
            return Err(M6Error::new(
                M6ErrorCode::OrphanRemediationDenied,
                "only orphaned Attempts may be remediated",
                Some("attemptId"),
                false,
            ));
        }
        let reservation = self.registry.get_reservation(attempt_id)?;
        if reservation.state != ReservationState::HeldOrphaned {
            return Err(M6Error::new(
                M6ErrorCode::ReservationStateConflict,
                "orphaned Attempt does not hold a reservation",
                Some("attemptId"),
                false,
            ));
        }

        let initial = observe_original_process_tree(&attempt)?;
        let mut termination_requested = false;
        if initial.matching_unit_active && terminate_matching_unit {
            let output = Command::new("systemctl")
                .args(["stop", &attempt.unit_name])
                .output()
                .map_err(|error| tool_error("cannot stop orphaned unit", error))?;
            if !output.status.success() {
                return Err(M6Error::new(
                    M6ErrorCode::OrphanRemediationDenied,
                    format!(
                        "systemd refused orphan termination: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                    Some("attemptId"),
                    true,
                ));
            }
            termination_requested = true;
        }

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let observation = observe_original_process_tree(&attempt)?;
            if !observation.original_process_tree_alive() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(M6Error::new(
                    M6ErrorCode::OrphanRemediationDenied,
                    "original orphan process tree is still live",
                    Some("attemptId"),
                    true,
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }

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
                "Attempt changed before reservation release",
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
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit orphan release"))?;

        Ok(M7RemediationResult {
            attempt_id: attempt_id.to_string(),
            reservation_released: true,
            termination_requested,
        })
    }
}

fn observe_original_process_tree(attempt: &crate::AttemptRecordM6) -> M6Result<OrphanObservation> {
    let properties = systemd_properties(&attempt.unit_name)?;
    let active = properties.get("ActiveState").is_some_and(|state| {
        matches!(
            state.as_str(),
            "active" | "activating" | "deactivating" | "reloading"
        )
    });
    let matching_unit_active = active
        && attempt
            .invocation_id
            .as_deref()
            .zip(nonempty(&properties, "InvocationID").as_deref())
            .is_some_and(|(expected, observed)| expected == observed);
    let live_cgroup_processes = attempt
        .control_group
        .as_deref()
        .map(cgroup_processes)
        .transpose()?
        .unwrap_or_default();
    let recorded_pid_identity_matches = attempt.main_pid.is_some_and(|pid| {
        process_identity(pid)
            .as_deref()
            .zip(attempt.process_start_identity.as_deref())
            .is_some_and(|(observed, expected)| observed == expected)
    });
    Ok(OrphanObservation {
        matching_unit_active,
        live_cgroup_processes,
        recorded_pid_identity_matches,
    })
}

fn systemd_properties(unit_name: &str) -> M6Result<BTreeMap<String, String>> {
    let output = Command::new("systemctl")
        .args([
            "show",
            unit_name,
            "--property=LoadState,ActiveState,InvocationID",
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

fn cgroup_processes(control_group: &str) -> M6Result<Vec<u32>> {
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
        return Ok(Vec::new());
    }
    let mut processes = fs::read_to_string(&path)
        .map_err(|error| tool_error("cannot read orphan cgroup", error))?
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect::<Vec<_>>();
    processes.sort_unstable();
    processes.dedup();
    Ok(processes)
}

fn process_identity(pid: u32) -> Option<String> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(") ")?;
    let fields = text[close + 2..].split_whitespace().collect::<Vec<_>>();
    let start_time = fields.get(19)?;
    Some(format!("pid:{pid}:start:{start_time}"))
}

fn tool_error(context: &str, error: std::io::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::ToolFailed,
        format!("{context}: {error}"),
        None,
        true,
    )
}
