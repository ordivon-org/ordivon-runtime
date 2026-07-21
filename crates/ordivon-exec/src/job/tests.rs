use super::*;
use serde_json::json;
use std::collections::BTreeMap;

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn request() -> JobStartRequest {
    JobStartRequest {
        profile_id: "cargo.build".to_string(),
        args: vec![
            "--release".to_string(),
            "-p".to_string(),
            "ordivon-mcp".to_string(),
        ],
        cwd: "/root/projects/ordivon-structured-exec-v0".to_string(),
        timeout_ms: 900_000,
        env_overrides: BTreeMap::new(),
        stdout_retention_bytes: 16 * 1024 * 1024,
        stderr_retention_bytes: 16 * 1024 * 1024,
        client_request_id: "request:release-build:1".to_string(),
    }
}

fn profile() -> ExecutionProfile {
    ExecutionProfile {
        profile_id: "cargo.build".to_string(),
        enabled: true,
        executable: "/root/.local/share/mise/shims/cargo".to_string(),
        executable_digest: digest('a'),
        fixed_args: vec!["build".to_string()],
        allowed_argument_vectors: vec![vec![
            "--release".to_string(),
            "-p".to_string(),
            "ordivon-mcp".to_string(),
        ]],
        allowed_cwd_roots: vec!["/root/projects/ordivon-structured-exec-v0".to_string()],
        environment_rules: Vec::new(),
        max_runtime_ms: 900_000,
        max_stdout_bytes: 16 * 1024 * 1024,
        max_stderr_bytes: 16 * 1024 * 1024,
        max_concurrency: 1,
        terminate_on_output_limit: false,
    }
}

fn output() -> JobOutputMetadata {
    JobOutputMetadata {
        generation: 1,
        retained_bytes: 0,
        dropped_bytes: 0,
        truncated: false,
        digest: None,
    }
}

fn record(state: JobInternalState) -> JobRecord {
    JobRecord {
        schema_version: JOB_CONTRACT_SCHEMA_VERSION,
        job_id: "019beef0-job".to_string(),
        client_request_id: "request:1".to_string(),
        request_digest: digest('b'),
        policy_id: "ordivon.execution.dev.v1".to_string(),
        policy_version: "1".to_string(),
        policy_digest: digest('c'),
        profile_id: "cargo.build".to_string(),
        principal: "chatgpt:user".to_string(),
        authority_ref: "authority:job:1".to_string(),
        internal_state: state,
        created_at: "2026-07-21T10:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
        unit_name: None,
        runner_pid: None,
        process_start_identity: None,
        exit_code: None,
        termination_reason: None,
        stdout: output(),
        stderr: output(),
    }
}

#[test]
fn generated_start_schema_has_no_shell_command_or_executable_property() {
    let schema = schemars::schema_for!(JobStartRequest);
    let value = serde_json::to_value(schema).unwrap();
    let properties = value
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .unwrap();
    assert!(properties.contains_key("profileId"));
    assert!(properties.contains_key("args"));
    assert!(!properties.contains_key("command"));
    assert!(!properties.contains_key("shell"));
    assert!(!properties.contains_key("executable"));
}

#[test]
fn public_start_contract_has_no_shell_command_or_executable_field() {
    let value = serde_json::to_value(request()).unwrap();
    assert!(value.get("profileId").is_some());
    assert!(value.get("args").is_some());
    assert!(value.get("command").is_none());
    assert!(value.get("executable").is_none());

    let mut forged = value;
    forged["command"] = json!("rm -rf /");
    assert!(serde_json::from_value::<JobStartRequest>(forged).is_err());
}

#[test]
fn start_request_rejects_relative_cwd_and_invalid_environment() {
    let mut relative = request();
    relative.cwd = "repo".to_string();
    assert_eq!(
        relative.validate_shape().unwrap_err().code,
        JobContractErrorCode::InvalidCwd
    );

    let mut environment = request();
    environment
        .env_overrides
        .insert("LD-PRELOAD".to_string(), "evil".to_string());
    assert_eq!(
        environment.validate_shape().unwrap_err().code,
        JobContractErrorCode::EnvironmentDenied
    );
}

#[test]
fn policy_requires_absolute_digest_bound_executable_and_exact_args() {
    let mut valid = profile();
    valid.validate_shape().unwrap();

    valid.executable = "cargo".to_string();
    assert_eq!(
        valid.validate_shape().unwrap_err().code,
        JobContractErrorCode::PolicyInvalid
    );

    let mut no_vectors = profile();
    no_vectors.allowed_argument_vectors.clear();
    assert_eq!(
        no_vectors.validate_shape().unwrap_err().code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn valid_policy_freezes_one_exact_cargo_build_invocation() {
    let policy = CapabilityPolicy {
        schema_version: JOB_CONTRACT_SCHEMA_VERSION,
        policy_id: "ordivon.execution.dev.v1".to_string(),
        policy_version: "1".to_string(),
        allowed_roots: vec!["/root/projects".to_string()],
        global_max_concurrency: 1,
        profiles: vec![profile()],
    };
    policy.validate_shape().unwrap();
    assert_eq!(
        policy.profiles[0].allowed_argument_vectors,
        vec![vec![
            "--release".to_string(),
            "-p".to_string(),
            "ordivon-mcp".to_string(),
        ]]
    );
}

#[test]
fn policy_rejects_duplicate_profiles_and_roots() {
    let policy = CapabilityPolicy {
        schema_version: JOB_CONTRACT_SCHEMA_VERSION,
        policy_id: "ordivon.execution.dev.v1".to_string(),
        policy_version: "1".to_string(),
        allowed_roots: vec!["/root/projects".to_string(), "/root/projects".to_string()],
        global_max_concurrency: 1,
        profiles: vec![profile(), profile()],
    };
    assert_eq!(
        policy.validate_shape().unwrap_err().code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn internal_states_project_to_stable_public_states() {
    assert_eq!(
        JobInternalState::Accepted.public_state(),
        JobPublicState::Queued
    );
    assert_eq!(
        JobInternalState::Recovering.public_state(),
        JobPublicState::Running
    );
    assert_eq!(
        JobInternalState::Orphaned.public_state(),
        JobPublicState::Orphaned
    );
}

#[test]
fn state_machine_allows_recovery_but_never_reopens_terminal_jobs() {
    assert!(JobInternalState::Running.can_transition_to(JobInternalState::Recovering));
    assert!(JobInternalState::Recovering.can_transition_to(JobInternalState::Running));
    assert!(JobInternalState::Stopping.can_transition_to(JobInternalState::Cancelled));
    assert!(!JobInternalState::Succeeded.can_transition_to(JobInternalState::Running));
    assert!(!JobInternalState::Failed.can_transition_to(JobInternalState::Starting));
}

#[test]
fn invalid_transition_returns_job_state_conflict() {
    let transition = JobStateTransition {
        from: JobInternalState::Succeeded,
        to: JobInternalState::Running,
        at: "2026-07-21T10:00:00Z".to_string(),
        reason_code: "RESTART".to_string(),
    };
    assert_eq!(
        transition.validate().unwrap_err().code,
        JobContractErrorCode::JobStateConflict
    );
}

#[test]
fn running_record_requires_supervisor_identity() {
    let mut running = record(JobInternalState::Running);
    running.started_at = Some("2026-07-21T10:00:01Z".to_string());
    assert_eq!(
        running.validate_shape().unwrap_err().code,
        JobContractErrorCode::JobMetadataCorrupt
    );
    running.unit_name = Some("ordivon-job-019beef0.service".to_string());
    running.validate_shape().unwrap();
}

#[test]
fn succeeded_record_requires_zero_exit_and_terminal_metadata() {
    let mut succeeded = record(JobInternalState::Succeeded);
    succeeded.finished_at = Some("2026-07-21T10:00:10Z".to_string());
    succeeded.termination_reason = Some("PROCESS_EXITED".to_string());
    succeeded.exit_code = Some(1);
    assert_eq!(
        succeeded.validate_shape().unwrap_err().code,
        JobContractErrorCode::JobMetadataCorrupt
    );
    succeeded.exit_code = Some(0);
    succeeded.validate_shape().unwrap();
}

#[test]
fn dropped_output_requires_truncation_marker() {
    let invalid = JobOutputMetadata {
        generation: 1,
        retained_bytes: 10,
        dropped_bytes: 1,
        truncated: false,
        digest: None,
    };
    assert_eq!(
        invalid.validate_shape().unwrap_err().code,
        JobContractErrorCode::JobMetadataCorrupt
    );
}

#[test]
fn read_cursor_is_bound_to_job_stream_and_generation() {
    let request = JobReadRequest {
        job_id: "job:one".to_string(),
        stream: JobOutputStream::Stdout,
        cursor: Some(JobOutputCursor {
            schema_version: JOB_CONTRACT_SCHEMA_VERSION,
            job_id: "job:two".to_string(),
            stream: JobOutputStream::Stdout,
            generation: 1,
            byte_offset: 0,
        }),
        max_bytes: 1024,
        encoding: JobOutputEncoding::Utf8Lossy,
    };
    assert_eq!(
        request.validate_shape().unwrap_err().code,
        JobContractErrorCode::CursorInvalid
    );
}

#[test]
fn reads_and_lists_are_strictly_bounded() {
    let too_large = JobReadRequest {
        job_id: "job:one".to_string(),
        stream: JobOutputStream::Stdout,
        cursor: None,
        max_bytes: MAX_JOB_READ_BYTES + 1,
        encoding: JobOutputEncoding::Base64,
    };
    assert_eq!(
        too_large.validate_shape().unwrap_err().code,
        JobContractErrorCode::InvalidRequest
    );
    let list = JobListRequest {
        limit: MAX_JOB_LIST_LIMIT + 1,
        cursor: None,
        states: Vec::new(),
        created_after: None,
        created_before: None,
    };
    assert_eq!(
        list.validate_shape().unwrap_err().code,
        JobContractErrorCode::InvalidRequest
    );
}

#[test]
fn wire_format_cannot_claim_ai_written_operational_observation() {
    let value = json!({
        "schemaVersion": 1,
        "eventId": "event:1",
        "operationId": "operation:1",
        "jobId": "job:1",
        "clientRequestId": "request:1",
        "timestamp": "2026-07-21T10:00:00Z",
        "actor": "ai",
        "eventType": "PROCESS_EXITED",
        "origin": "AI_WRITTEN",
        "requestDigest": digest('d'),
        "policyDigest": digest('e'),
        "reasonCode": "EXITED",
        "detailDigest": digest('f')
    });
    assert!(serde_json::from_value::<OperationalReceiptEvent>(value).is_err());
}

#[test]
fn process_exit_requires_system_observed_origin() {
    let event = OperationalReceiptEvent {
        schema_version: JOB_CONTRACT_SCHEMA_VERSION,
        event_id: "event:1".to_string(),
        operation_id: "operation:1".to_string(),
        job_id: Some("job:1".to_string()),
        client_request_id: "request:1".to_string(),
        timestamp: "2026-07-21T10:00:00Z".to_string(),
        actor: "ordivon-core".to_string(),
        event_type: OperationalReceiptEventType::ProcessExited,
        origin: OperationalEventOrigin::SystemDerived,
        request_digest: digest('d'),
        policy_digest: digest('e'),
        unit_name: Some("ordivon-job-1.service".to_string()),
        previous_state: Some(JobInternalState::Running),
        new_state: Some(JobInternalState::Succeeded),
        reason_code: "EXITED_ZERO".to_string(),
        detail_digest: digest('f'),
    };
    assert_eq!(
        event.validate_shape().unwrap_err().code,
        JobContractErrorCode::JobMetadataCorrupt
    );
}

#[test]
fn authorization_denial_can_be_receipted_without_creating_a_job() {
    let event = OperationalReceiptEvent {
        schema_version: JOB_CONTRACT_SCHEMA_VERSION,
        event_id: "event:denied".to_string(),
        operation_id: "operation:denied".to_string(),
        job_id: None,
        client_request_id: "request:denied".to_string(),
        timestamp: "2026-07-21T10:00:00Z".to_string(),
        actor: "ordivon-core".to_string(),
        event_type: OperationalReceiptEventType::AuthorizationDenied,
        origin: OperationalEventOrigin::SystemDerived,
        request_digest: digest('d'),
        policy_digest: digest('e'),
        unit_name: None,
        previous_state: None,
        new_state: None,
        reason_code: "PROFILE_DISABLED".to_string(),
        detail_digest: digest('f'),
    };
    event.validate_shape().unwrap();
}

#[test]
fn error_codes_have_stable_screaming_snake_case_wire_values() {
    assert_eq!(
        serde_json::to_value(JobContractErrorCode::ProfileNotFound).unwrap(),
        json!("PROFILE_NOT_FOUND")
    );
    assert_eq!(
        serde_json::to_value(JobContractErrorCode::CursorInvalid).unwrap(),
        json!("CURSOR_INVALID")
    );
}
