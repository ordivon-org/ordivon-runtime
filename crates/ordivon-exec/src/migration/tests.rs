use super::*;

fn route_request() -> MigrationRouteRequest {
    MigrationRouteRequest {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        capability: MigrationCapability::WorkspaceExec,
        operation_class: MigrationOperationClass::WorkspaceBoundMutation,
        ordivon_support: BackendSupport::Available,
        legacy_support: BackendSupport::Available,
        fallback_policy: LegacyFallbackPolicy::WorkspaceOnly,
        request_shadow_compare: false,
    }
}

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn artifact(task_id: &str) -> ArtifactReference {
    ArtifactReference {
        artifact_id: "artifact:stdout:1".to_string(),
        task_id: task_id.to_string(),
        kind: ArtifactKind::Stdout,
        digest: digest('a'),
        media_type: "text/plain".to_string(),
        byte_length: 12,
    }
}

fn benchmark(backend: MigrationBackend) -> MigrationBenchmarkSample {
    MigrationBenchmarkSample {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        sample_id: format!("sample:{backend:?}"),
        capability: MigrationCapability::WorkspaceExec,
        backend,
        succeeded: true,
        elapsed_ms: 1_000,
        tool_calls: 10,
        remote_round_trips: 8,
        context_bytes: 40_000,
        output_bytes: 30_000,
        recovered_after_disconnect: false,
        fallback_count: 0,
    }
}

#[test]
fn available_ordivon_is_always_the_primary_route() {
    let decision = decide_backend_route(&route_request()).unwrap();
    assert_eq!(decision.selected_backend, Some(MigrationBackend::Ordivon));
    assert_eq!(decision.reason, MigrationRouteReason::OrdivonPrimary);
    assert!(!decision.fallback_used);
    assert!(decision.safe_to_execute);
}

#[test]
fn workspace_operations_can_fallback_when_ordivon_is_unavailable() {
    let mut request = route_request();
    request.ordivon_support = BackendSupport::Unsupported;
    let decision = decide_backend_route(&request).unwrap();
    assert_eq!(
        decision.selected_backend,
        Some(MigrationBackend::LegacyDesktopCommander)
    );
    assert_eq!(decision.reason, MigrationRouteReason::LegacyFallback);
    assert!(decision.fallback_used);
}

#[test]
fn host_or_external_side_effects_never_automatically_fallback() {
    let mut request = route_request();
    request.ordivon_support = BackendSupport::Unavailable;
    request.operation_class = MigrationOperationClass::HostOrExternalSideEffect;
    let decision = decide_backend_route(&request).unwrap();
    assert_eq!(decision.selected_backend, None);
    assert_eq!(decision.reason, MigrationRouteReason::UnsafeFallbackDenied);
    assert!(!decision.safe_to_execute);
}

#[test]
fn shadow_comparison_is_read_only_and_requires_both_backends() {
    let mut request = route_request();
    request.operation_class = MigrationOperationClass::ReadOnly;
    request.request_shadow_compare = true;
    let decision = decide_backend_route(&request).unwrap();
    assert!(decision.shadow_compare);

    request.operation_class = MigrationOperationClass::WorkspaceBoundMutation;
    assert_eq!(
        decide_backend_route(&request).unwrap_err().code,
        MigrationContractErrorCode::InvalidContract
    );
}

#[test]
fn working_task_has_polling_and_no_result() {
    let task = MigrationTaskHandle {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        task_id: "task:1".to_string(),
        backend: MigrationBackend::Ordivon,
        status: MigrationTaskStatus::Working,
        status_message: "Running the target test.".to_string(),
        result_available: false,
        poll_after_ms: Some(1_000),
        event_cursor: Some("cursor:4".to_string()),
        required_input: None,
        artifacts: Vec::new(),
    };
    task.validate_shape().unwrap();
}

#[test]
fn terminal_task_requires_result_and_rejects_polling() {
    let task = MigrationTaskHandle {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        task_id: "task:1".to_string(),
        backend: MigrationBackend::Ordivon,
        status: MigrationTaskStatus::Completed,
        status_message: "Target test passed.".to_string(),
        result_available: true,
        poll_after_ms: Some(1_000),
        event_cursor: None,
        required_input: None,
        artifacts: vec![artifact("task:1")],
    };
    assert_eq!(
        task.validate_shape().unwrap_err().code,
        MigrationContractErrorCode::InvalidContract
    );
}

#[test]
fn input_required_task_must_explain_the_input() {
    let task = MigrationTaskHandle {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        task_id: "task:1".to_string(),
        backend: MigrationBackend::Ordivon,
        status: MigrationTaskStatus::InputRequired,
        status_message: "Approval is required.".to_string(),
        result_available: false,
        poll_after_ms: None,
        event_cursor: None,
        required_input: Some(TaskInputRequest {
            kind: "approval".to_string(),
            summary: "Allow network access for this attempt?".to_string(),
            options: vec!["Allow once".to_string(), "Cancel".to_string()],
        }),
        artifacts: Vec::new(),
    };
    task.validate_shape().unwrap();
}

#[test]
fn artifact_must_belong_to_the_task() {
    let task = MigrationTaskHandle {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        task_id: "task:1".to_string(),
        backend: MigrationBackend::Ordivon,
        status: MigrationTaskStatus::Completed,
        status_message: "Done.".to_string(),
        result_available: true,
        poll_after_ms: None,
        event_cursor: None,
        required_input: None,
        artifacts: vec![artifact("task:2")],
    };
    assert_eq!(
        task.validate_shape().unwrap_err().code,
        MigrationContractErrorCode::InvalidArtifact
    );
}

#[test]
fn performance_delta_uses_ordivon_minus_legacy() {
    let legacy = benchmark(MigrationBackend::LegacyDesktopCommander);
    let mut ordivon = benchmark(MigrationBackend::Ordivon);
    ordivon.elapsed_ms = 900;
    ordivon.tool_calls = 5;
    ordivon.remote_round_trips = 3;
    ordivon.context_bytes = 10_000;
    ordivon.output_bytes = 5_000;
    ordivon.recovered_after_disconnect = true;

    let delta = compare_migration_samples(&legacy, &ordivon).unwrap();
    assert_eq!(delta.elapsed_ms_delta, -100);
    assert_eq!(delta.tool_calls_delta, -5);
    assert_eq!(delta.remote_round_trips_delta, -5);
    assert_eq!(delta.context_bytes_delta, -30_000);
    assert!(delta.disconnect_recovery_improved);
}

#[test]
fn benchmark_comparison_requires_same_capability_and_backend_order() {
    let legacy = benchmark(MigrationBackend::LegacyDesktopCommander);
    let mut ordivon = benchmark(MigrationBackend::Ordivon);
    ordivon.capability = MigrationCapability::WorkspaceRead;
    assert_eq!(
        compare_migration_samples(&legacy, &ordivon)
            .unwrap_err()
            .code,
        MigrationContractErrorCode::IncomparableBenchmark
    );
}

#[test]
fn fallback_record_cannot_hide_external_side_effects() {
    let record = LegacyFallbackRecord {
        schema_version: MIGRATION_CONTRACT_SCHEMA_VERSION,
        record_id: "fallback:1".to_string(),
        capability: MigrationCapability::WorkspaceExec,
        operation_class: MigrationOperationClass::HostOrExternalSideEffect,
        legacy_tool: "start_process".to_string(),
        missing_or_degraded_capability: "production service management".to_string(),
        succeeded: true,
        elapsed_ms: 100,
    };
    assert_eq!(
        record.validate_shape().unwrap_err().code,
        MigrationContractErrorCode::InvalidContract
    );
}

#[test]
fn wire_contract_rejects_unknown_fields() {
    let value = serde_json::json!({
        "schemaVersion": 1,
        "capability": "WORKSPACE_READ",
        "operationClass": "READ_ONLY",
        "ordivonSupport": "AVAILABLE",
        "legacySupport": "AVAILABLE",
        "fallbackPolicy": "WORKSPACE_ONLY",
        "requestShadowCompare": false,
        "command": "rm -rf /"
    });
    assert!(serde_json::from_value::<MigrationRouteRequest>(value).is_err());
}
