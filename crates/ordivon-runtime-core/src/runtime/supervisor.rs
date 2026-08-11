use super::{AttemptState, RuntimeError, RuntimeErrorCode, RuntimeResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SupervisorIdentity {
    pub boot_id: String,
    pub unit_name: String,
    pub invocation_id: String,
    pub control_group: String,
    pub main_pid: u32,
    pub main_process_start_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SupervisorUnitState {
    Running,
    Terminal,
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SupervisorObservation {
    pub boot_id: String,
    pub unit_state: SupervisorUnitState,
    pub invocation_id: Option<String>,
    pub control_group: Option<String>,
    pub main_pid: Option<u32>,
    pub main_process_start_identity: Option<String>,
    pub recorded_pid_alive: bool,
    pub recorded_pid_start_identity: Option<String>,
    pub result: Option<String>,
    pub exec_main_code: Option<i32>,
    pub exec_main_status: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SupervisorRecoveryDisposition {
    Running,
    Terminal(AttemptState),
    Lost,
    Orphaned(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminationIntent {
    Natural,
    StopRequested,
    DeadlineExceeded,
}

pub(crate) fn classify_supervisor_recovery(
    expected: &SupervisorIdentity,
    observation: &SupervisorObservation,
    termination_intent: TerminationIntent,
) -> RuntimeResult<SupervisorRecoveryDisposition> {
    validate_identity(expected)?;
    if observation.boot_id != expected.boot_id {
        return Ok(SupervisorRecoveryDisposition::Lost);
    }
    match observation.unit_state {
        SupervisorUnitState::Running => match running_identity_mismatch(expected, observation) {
            Some(reason) => Ok(SupervisorRecoveryDisposition::Orphaned(reason)),
            None => Ok(SupervisorRecoveryDisposition::Running),
        },
        SupervisorUnitState::Terminal => match terminal_identity_mismatch(expected, observation) {
            Some(reason) => Ok(SupervisorRecoveryDisposition::Orphaned(reason)),
            None => Ok(SupervisorRecoveryDisposition::Terminal(
                classify_terminal_state(
                    termination_intent,
                    observation.result.as_deref(),
                    observation.exec_main_code,
                    observation.exec_main_status,
                ),
            )),
        },
        SupervisorUnitState::NotFound => {
            if observation.recorded_pid_alive
                && observation.recorded_pid_start_identity.as_deref()
                    == Some(expected.main_process_start_identity.as_str())
            {
                return Ok(SupervisorRecoveryDisposition::Orphaned(
                    "recorded process identity is alive but supervisor unit is missing".to_string(),
                ));
            }
            Ok(match termination_intent {
                TerminationIntent::StopRequested => {
                    SupervisorRecoveryDisposition::Terminal(AttemptState::Cancelled)
                }
                TerminationIntent::DeadlineExceeded => {
                    SupervisorRecoveryDisposition::Terminal(AttemptState::TimedOut)
                }
                TerminationIntent::Natural => SupervisorRecoveryDisposition::Lost,
            })
        }
    }
}

fn classify_terminal_state(
    intent: TerminationIntent,
    result: Option<&str>,
    exec_main_code: Option<i32>,
    exec_main_status: Option<i32>,
) -> AttemptState {
    match intent {
        TerminationIntent::StopRequested => AttemptState::Cancelled,
        TerminationIntent::DeadlineExceeded => AttemptState::TimedOut,
        TerminationIntent::Natural => {
            if matches!(result, Some("success"))
                || (exec_main_code == Some(1) && exec_main_status == Some(0))
            {
                AttemptState::Succeeded
            } else {
                AttemptState::Failed
            }
        }
    }
}

fn validate_identity(identity: &SupervisorIdentity) -> RuntimeResult<()> {
    if identity.boot_id.is_empty()
        || identity.invocation_id.is_empty()
        || identity.main_process_start_identity.is_empty()
        || identity.main_pid == 0
        || !identity.unit_name.ends_with(".service")
        || !identity.control_group.starts_with('/')
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            "persisted supervisor identity is incomplete",
            Some("supervisorIdentity"),
            false,
        ));
    }
    Ok(())
}

fn terminal_identity_mismatch(
    expected: &SupervisorIdentity,
    observed: &SupervisorObservation,
) -> Option<String> {
    if observed.invocation_id.as_deref() != Some(expected.invocation_id.as_str()) {
        return Some("invocationId does not match persisted supervisor identity".to_string());
    }
    if observed
        .control_group
        .as_deref()
        .is_some_and(|actual| actual != expected.control_group)
    {
        return Some("controlGroup does not match persisted supervisor identity".to_string());
    }
    None
}

fn running_identity_mismatch(
    expected: &SupervisorIdentity,
    observed: &SupervisorObservation,
) -> Option<String> {
    for (field, actual, expected_value) in [
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
    ] {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expected() -> SupervisorIdentity {
        SupervisorIdentity {
            boot_id: "boot-a".into(),
            unit_name: "ordivon-job-01.service".into(),
            invocation_id: "invocation-a".into(),
            control_group: "/system.slice/ordivon-job-01.service".into(),
            main_pid: 42,
            main_process_start_identity: "9001".into(),
        }
    }

    fn running() -> SupervisorObservation {
        SupervisorObservation {
            boot_id: "boot-a".into(),
            unit_state: SupervisorUnitState::Running,
            invocation_id: Some("invocation-a".into()),
            control_group: Some("/system.slice/ordivon-job-01.service".into()),
            main_pid: Some(42),
            main_process_start_identity: Some("9001".into()),
            recorded_pid_alive: true,
            recorded_pid_start_identity: Some("9001".into()),
            result: None,
            exec_main_code: None,
            exec_main_status: None,
        }
    }

    #[test]
    fn current_identity_recovers_running() {
        assert_eq!(
            classify_supervisor_recovery(&expected(), &running(), TerminationIntent::Natural,)
                .unwrap(),
            SupervisorRecoveryDisposition::Running
        );
    }

    #[test]
    fn identity_reuse_is_orphaned() {
        let mut observation = running();
        observation.invocation_id = Some("replacement".into());
        assert!(matches!(
            classify_supervisor_recovery(&expected(), &observation, TerminationIntent::Natural,)
                .unwrap(),
            SupervisorRecoveryDisposition::Orphaned(_)
        ));
    }

    #[test]
    fn terminal_pid_reuse_does_not_orphan_matching_invocation() {
        let mut observation = running();
        observation.unit_state = SupervisorUnitState::Terminal;
        observation.main_pid = Some(84);
        observation.main_process_start_identity = Some("replacement-process".into());
        observation.recorded_pid_alive = false;
        observation.recorded_pid_start_identity = Some("replacement-process".into());
        observation.result = Some("success".into());
        observation.exec_main_code = Some(1);
        observation.exec_main_status = Some(0);
        assert_eq!(
            classify_supervisor_recovery(&expected(), &observation, TerminationIntent::Natural)
                .unwrap(),
            SupervisorRecoveryDisposition::Terminal(AttemptState::Succeeded)
        );
    }

    #[test]
    fn missing_unit_after_committed_stop_is_cancelled_when_recorded_pid_is_gone() {
        let mut observation = running();
        observation.unit_state = SupervisorUnitState::NotFound;
        observation.recorded_pid_alive = false;
        observation.recorded_pid_start_identity = None;
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                TerminationIntent::StopRequested,
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal(AttemptState::Cancelled)
        );
    }

    #[test]
    fn missing_unit_after_committed_deadline_is_timed_out_when_recorded_pid_is_gone() {
        let mut observation = running();
        observation.unit_state = SupervisorUnitState::NotFound;
        observation.recorded_pid_alive = false;
        observation.recorded_pid_start_identity = None;
        assert_eq!(
            classify_supervisor_recovery(
                &expected(),
                &observation,
                TerminationIntent::DeadlineExceeded,
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal(AttemptState::TimedOut)
        );
    }

    #[test]
    fn boot_change_is_lost() {
        let mut observation = running();
        observation.boot_id = "boot-b".into();
        assert_eq!(
            classify_supervisor_recovery(&expected(), &observation, TerminationIntent::Natural,)
                .unwrap(),
            SupervisorRecoveryDisposition::Lost
        );
    }

    #[test]
    fn stop_and_deadline_intent_control_terminal_state() {
        assert_eq!(
            classify_terminal_state(
                TerminationIntent::StopRequested,
                Some("timeout"),
                Some(2),
                Some(9)
            ),
            AttemptState::Cancelled
        );
        assert_eq!(
            classify_terminal_state(
                TerminationIntent::DeadlineExceeded,
                Some("timeout"),
                Some(2),
                Some(9)
            ),
            AttemptState::TimedOut
        );
    }
}
