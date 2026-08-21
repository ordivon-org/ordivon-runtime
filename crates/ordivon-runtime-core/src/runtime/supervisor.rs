use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "contract")]
pub(crate) enum AttemptSupervisorOwner {
    #[serde(rename = "windows_launcher_v1")]
    WindowsLauncherV1 {
        launcher_process_id: u32,
        launcher_process_creation_time_file_time: u64,
        launcher_image_digest: String,
        job_name: String,
        start_evidence_digest: String,
    },
}

impl AttemptSupervisorOwner {
    pub(crate) fn start_evidence_digest(&self) -> &str {
        match self {
            AttemptSupervisorOwner::WindowsLauncherV1 {
                start_evidence_digest,
                ..
            } => start_evidence_digest,
        }
    }

    pub(crate) fn contract_name(&self) -> &'static str {
        match self {
            AttemptSupervisorOwner::WindowsLauncherV1 { .. } => "windows_launcher_v1",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowsLauncherOwnerObservation {
    pub process_alive: bool,
    pub process_creation_time_file_time: Option<u64>,
}

pub(crate) fn validate_attempt_supervisor_owner(
    owner: &AttemptSupervisorOwner,
) -> RuntimeResult<()> {
    match owner {
        AttemptSupervisorOwner::WindowsLauncherV1 {
            launcher_process_id,
            launcher_process_creation_time_file_time,
            launcher_image_digest,
            job_name,
            start_evidence_digest,
        } => {
            if *launcher_process_id == 0
                || *launcher_process_creation_time_file_time == 0
                || job_name.is_empty()
                || !job_name.starts_with("Ordivon.")
                || !is_sha256_digest(launcher_image_digest)
                || !is_sha256_digest(start_evidence_digest)
            {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    "persisted Windows launcher owner identity is incomplete",
                    Some("attemptSupervisorOwner"),
                    false,
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn classify_windows_launcher_recovery(
    expected: &AttemptSupervisorOwner,
    observation: &WindowsLauncherOwnerObservation,
    termination_intent: TerminationIntent,
) -> RuntimeResult<SupervisorRecoveryDisposition> {
    validate_attempt_supervisor_owner(expected)?;
    let AttemptSupervisorOwner::WindowsLauncherV1 {
        launcher_process_creation_time_file_time,
        launcher_image_digest,
        ..
    } = expected;
    if observation.process_alive {
        if observation.process_creation_time_file_time
            != Some(*launcher_process_creation_time_file_time)
        {
            return Ok(SupervisorRecoveryDisposition::Orphaned(
                "launcher process creation identity does not match persisted owner".to_string(),
            ));
        }
        let _ = launcher_image_digest;
        return Ok(SupervisorRecoveryDisposition::Running);
    }
    Ok(SupervisorRecoveryDisposition::Terminal(
        match termination_intent {
            TerminationIntent::StopRequested => AttemptState::Cancelled,
            TerminationIntent::DeadlineExceeded => AttemptState::TimedOut,
            // The Windows launcher is the sole owner of the KILL_ON_JOB_CLOSE handle. If that
            // process identity is gone and no result evidence was observed by the caller, the
            // native process tree cannot still be executing under this Attempt.
            TerminationIntent::Natural => AttemptState::Failed,
        },
    ))
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
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

    fn windows_owner() -> AttemptSupervisorOwner {
        AttemptSupervisorOwner::WindowsLauncherV1 {
            launcher_process_id: 4242,
            launcher_process_creation_time_file_time: 123_456_789,
            launcher_image_digest:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            job_name: "Ordivon.attempt-1".to_string(),
            start_evidence_digest:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
        }
    }

    #[test]
    fn native_windows_launcher_identity_recovers_running() {
        let observed = WindowsLauncherOwnerObservation {
            process_alive: true,
            process_creation_time_file_time: Some(123_456_789),
        };
        assert_eq!(
            classify_windows_launcher_recovery(
                &windows_owner(),
                &observed,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Running
        );
    }

    #[test]
    fn native_windows_launcher_pid_reuse_is_orphaned() {
        let observed = WindowsLauncherOwnerObservation {
            process_alive: true,
            process_creation_time_file_time: Some(987_654_321),
        };
        assert!(matches!(
            classify_windows_launcher_recovery(
                &windows_owner(),
                &observed,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Orphaned(_)
        ));
    }

    #[test]
    fn absent_native_windows_launcher_is_definitively_terminal() {
        let observed = WindowsLauncherOwnerObservation {
            process_alive: false,
            process_creation_time_file_time: None,
        };
        assert_eq!(
            classify_windows_launcher_recovery(
                &windows_owner(),
                &observed,
                TerminationIntent::Natural
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal(AttemptState::Failed)
        );
        assert_eq!(
            classify_windows_launcher_recovery(
                &windows_owner(),
                &observed,
                TerminationIntent::StopRequested
            )
            .unwrap(),
            SupervisorRecoveryDisposition::Terminal(AttemptState::Cancelled)
        );
    }
}
