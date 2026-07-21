use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    validation::{metadata_corrupt, validate_absolute_path, validate_identifier},
    JobContractError, JobContractErrorCode, JobInternalState,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorIdentity {
    pub boot_id: String,
    pub unit_name: String,
    pub invocation_id: String,
    pub control_group: String,
    pub main_pid: u32,
    pub main_process_start_identity: String,
}

impl SupervisorIdentity {
    pub fn validate(&self) -> Result<(), JobContractError> {
        validate_identifier(&self.boot_id, "bootId")?;
        validate_identifier(&self.unit_name, "unitName")?;
        validate_identifier(&self.invocation_id, "invocationId")?;
        validate_absolute_path(
            &self.control_group,
            "controlGroup",
            JobContractErrorCode::JobMetadataCorrupt,
        )?;
        if !self.unit_name.ends_with(".service") {
            return Err(metadata_corrupt(
                "unitName must identify a service unit",
                "unitName",
            ));
        }
        if self.main_pid == 0 {
            return Err(metadata_corrupt("mainPid must be non-zero", "mainPid"));
        }
        validate_identifier(
            &self.main_process_start_identity,
            "mainProcessStartIdentity",
        )
        .map_err(|error| metadata_error(error, "mainProcessStartIdentity"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorUnitState {
    Running,
    Terminal,
    NotFound,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupervisorObservation {
    pub boot_id: String,
    pub unit_state: SupervisorUnitState,
    #[serde(default)]
    pub invocation_id: Option<String>,
    #[serde(default)]
    pub control_group: Option<String>,
    #[serde(default)]
    pub main_pid: Option<u32>,
    #[serde(default)]
    pub main_process_start_identity: Option<String>,
    pub recorded_pid_alive: bool,
    #[serde(default)]
    pub recorded_pid_start_identity: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub exec_main_code: Option<i32>,
    #[serde(default)]
    pub exec_main_status: Option<i32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerResultObservation {
    Missing,
    Corrupt,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

impl RunnerResultObservation {
    fn terminal_state(self) -> Option<JobInternalState> {
        match self {
            Self::Missing | Self::Corrupt => None,
            Self::Succeeded => Some(JobInternalState::Succeeded),
            Self::Failed => Some(JobInternalState::Failed),
            Self::TimedOut => Some(JobInternalState::TimedOut),
            Self::Cancelled => Some(JobInternalState::Cancelled),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEvidenceSource {
    Supervisor,
    RunnerResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", tag = "disposition", content = "detail")]
pub enum SupervisorRecoveryDisposition {
    Running,
    Terminal {
        state: JobInternalState,
        source: RecoveryEvidenceSource,
    },
    Lost,
    Orphaned {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationIntent {
    Natural,
    StopRequested,
    DeadlineExceeded,
}

pub fn classify_supervisor_recovery(
    expected: &SupervisorIdentity,
    observation: &SupervisorObservation,
    runner_result: RunnerResultObservation,
    termination_intent: TerminationIntent,
) -> Result<SupervisorRecoveryDisposition, JobContractError> {
    expected.validate()?;
    if matches!(runner_result, RunnerResultObservation::Corrupt) {
        return Err(metadata_corrupt(
            "runner terminal result is corrupt",
            "runnerResult",
        ));
    }
    if let Some(state) = runner_result.terminal_state() {
        return Ok(SupervisorRecoveryDisposition::Terminal {
            state,
            source: RecoveryEvidenceSource::RunnerResult,
        });
    }
    if observation.boot_id != expected.boot_id {
        return Ok(SupervisorRecoveryDisposition::Lost);
    }

    match observation.unit_state {
        SupervisorUnitState::Running => {
            if let Some(reason) = identity_mismatch(expected, observation) {
                return Ok(SupervisorRecoveryDisposition::Orphaned { reason });
            }
            Ok(SupervisorRecoveryDisposition::Running)
        }
        SupervisorUnitState::Terminal => {
            if let Some(reason) = identity_mismatch(expected, observation) {
                return Ok(SupervisorRecoveryDisposition::Orphaned { reason });
            }
            Ok(SupervisorRecoveryDisposition::Terminal {
                state: classify_terminal_state(
                    termination_intent,
                    observation.result.as_deref(),
                    observation.exec_main_code,
                    observation.exec_main_status,
                ),
                source: RecoveryEvidenceSource::Supervisor,
            })
        }
        SupervisorUnitState::NotFound => {
            if observation.recorded_pid_alive
                && observation.recorded_pid_start_identity.as_deref()
                    == Some(expected.main_process_start_identity.as_str())
            {
                return Ok(SupervisorRecoveryDisposition::Orphaned {
                    reason: "recorded process identity is alive but supervisor unit is missing"
                        .to_string(),
                });
            }
            Ok(SupervisorRecoveryDisposition::Lost)
        }
    }
}

pub fn classify_terminal_state(
    intent: TerminationIntent,
    result: Option<&str>,
    exec_main_code: Option<i32>,
    exec_main_status: Option<i32>,
) -> JobInternalState {
    match intent {
        TerminationIntent::StopRequested => JobInternalState::Cancelled,
        TerminationIntent::DeadlineExceeded => JobInternalState::TimedOut,
        TerminationIntent::Natural => {
            if matches!(result, Some("success"))
                || (exec_main_code == Some(1) && exec_main_status == Some(0))
            {
                JobInternalState::Succeeded
            } else {
                JobInternalState::Failed
            }
        }
    }
}

fn identity_mismatch(
    expected: &SupervisorIdentity,
    observed: &SupervisorObservation,
) -> Option<String> {
    let checks = [
        (
            "invocationId",
            observed.invocation_id.as_deref(),
            expected.invocation_id.as_str(),
        ),
        (
            "controlGroup",
            observed.control_group.as_deref(),
            expected.control_group.as_str(),
        ),
        (
            "mainProcessStartIdentity",
            observed.main_process_start_identity.as_deref(),
            expected.main_process_start_identity.as_str(),
        ),
    ];
    for (field, actual, expected_value) in checks {
        if actual != Some(expected_value) {
            return Some(format!(
                "{field} does not match persisted supervisor identity"
            ));
        }
    }
    if observed.main_pid != Some(expected.main_pid) {
        return Some("mainPid does not match persisted supervisor identity".to_string());
    }
    None
}

fn metadata_error(mut error: JobContractError, field: &str) -> JobContractError {
    error.code = JobContractErrorCode::JobMetadataCorrupt;
    error.field = Some(field.to_string());
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected() -> SupervisorIdentity {
        SupervisorIdentity {
            boot_id: "boot-a".to_string(),
            unit_name: "ordivon-job-01.service".to_string(),
            invocation_id: "0123456789abcdef".to_string(),
            control_group: "/system.slice/ordivon-job-01.service".to_string(),
            main_pid: 42,
            main_process_start_identity: "9001".to_string(),
        }
    }

    fn running() -> SupervisorObservation {
        SupervisorObservation {
            boot_id: "boot-a".to_string(),
            unit_state: SupervisorUnitState::Running,
            invocation_id: Some("0123456789abcdef".to_string()),
            control_group: Some("/system.slice/ordivon-job-01.service".to_string()),
            main_pid: Some(42),
            main_process_start_identity: Some("9001".to_string()),
            recorded_pid_alive: true,
            recorded_pid_start_identity: Some("9001".to_string()),
            result: Some("success".to_string()),
            exec_main_code: Some(0),
            exec_main_status: Some(0),
        }
    }

    #[test]
    fn matching_running_unit_recovers_as_running() {
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &running(),
                RunnerResultObservation::Missing,
                TerminationIntent::Natural,
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Running
        );
    }

    #[test]
    fn reused_unit_name_is_orphaned_not_running() {
        let mut observation = running();
        observation.invocation_id = Some("replacement-invocation".to_string());
        assert!(matches!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Missing,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Orphaned { .. }
        ));
    }

    #[test]
    fn missing_unit_with_same_live_process_identity_is_orphaned() {
        let observation = SupervisorObservation {
            boot_id: "boot-a".to_string(),
            unit_state: SupervisorUnitState::NotFound,
            invocation_id: None,
            control_group: None,
            main_pid: None,
            main_process_start_identity: None,
            recorded_pid_alive: true,
            recorded_pid_start_identity: Some("9001".to_string()),
            result: None,
            exec_main_code: None,
            exec_main_status: None,
        };
        assert!(matches!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Missing,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Orphaned { .. }
        ));
    }

    #[test]
    fn missing_unit_without_terminal_result_is_lost() {
        let observation = SupervisorObservation {
            boot_id: "boot-a".to_string(),
            unit_state: SupervisorUnitState::NotFound,
            invocation_id: None,
            control_group: None,
            main_pid: None,
            main_process_start_identity: None,
            recorded_pid_alive: false,
            recorded_pid_start_identity: None,
            result: None,
            exec_main_code: None,
            exec_main_status: None,
        };
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Missing,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Lost
        );
    }

    #[test]
    fn successful_unit_gc_recovers_from_runner_result() {
        let observation = SupervisorObservation {
            boot_id: "boot-a".to_string(),
            unit_state: SupervisorUnitState::NotFound,
            invocation_id: None,
            control_group: None,
            main_pid: None,
            main_process_start_identity: None,
            recorded_pid_alive: false,
            recorded_pid_start_identity: None,
            result: Some("success".to_string()),
            exec_main_code: Some(0),
            exec_main_status: Some(0),
        };
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Succeeded,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal {
                state: JobInternalState::Succeeded,
                source: RecoveryEvidenceSource::RunnerResult,
            }
        );
    }

    #[test]
    fn valid_runner_result_wins_over_a_reused_running_unit() {
        let mut observation = running();
        observation.invocation_id = Some("replacement-invocation".to_string());
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Succeeded,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal {
                state: JobInternalState::Succeeded,
                source: RecoveryEvidenceSource::RunnerResult,
            }
        );
    }

    #[test]
    fn loaded_failed_unit_can_supply_degraded_terminal_evidence() {
        let mut observation = running();
        observation.unit_state = SupervisorUnitState::Terminal;
        observation.result = Some("exit-code".to_string());
        observation.exec_main_code = Some(1);
        observation.exec_main_status = Some(7);
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Missing,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal {
                state: JobInternalState::Failed,
                source: RecoveryEvidenceSource::Supervisor,
            }
        );
    }

    #[test]
    fn explicit_stop_timeout_is_cancelled_not_job_timeout() {
        assert_eq!(
            classify_terminal_state(
                TerminationIntent::StopRequested,
                Some("timeout"),
                Some(2),
                Some(9)
            ),
            JobInternalState::Cancelled
        );
    }

    #[test]
    fn deadline_timeout_uses_same_os_evidence_but_different_intent() {
        assert_eq!(
            classify_terminal_state(
                TerminationIntent::DeadlineExceeded,
                Some("timeout"),
                Some(2),
                Some(9)
            ),
            JobInternalState::TimedOut
        );
    }

    #[test]
    fn natural_zero_exit_is_succeeded_and_nonzero_is_failed() {
        assert_eq!(
            classify_terminal_state(
                TerminationIntent::Natural,
                Some("success"),
                Some(1),
                Some(0)
            ),
            JobInternalState::Succeeded
        );
        assert_eq!(
            classify_terminal_state(
                TerminationIntent::Natural,
                Some("exit-code"),
                Some(1),
                Some(7)
            ),
            JobInternalState::Failed
        );
    }

    #[test]
    fn boot_change_without_terminal_result_is_lost() {
        let mut observation = running();
        observation.boot_id = "boot-b".to_string();
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Missing,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Lost
        );
    }

    #[test]
    fn corrupt_runner_result_is_not_downgraded_to_lost() {
        let observation = SupervisorObservation {
            boot_id: "boot-a".to_string(),
            unit_state: SupervisorUnitState::NotFound,
            invocation_id: None,
            control_group: None,
            main_pid: None,
            main_process_start_identity: None,
            recorded_pid_alive: false,
            recorded_pid_start_identity: None,
            result: None,
            exec_main_code: None,
            exec_main_status: None,
        };
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                RunnerResultObservation::Corrupt,
                TerminationIntent::Natural
            )
            .unwrap_err()
            .code,
            JobContractErrorCode::JobMetadataCorrupt
        );
    }
}
