use super::tasks::task_from_job;
use super::*;
use ordivon_runtime_core::{
    AdmissionOutcome, ArtifactDescriptor, RegistryConfig, RuntimeExecutionPlan, SubmitRequest,
    TaskObservation, RUNTIME_SCHEMA_VERSION,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::current_dir()
            .unwrap()
            .join("target/ordivon-tests")
            .join(format!(
                "ordivon-runtime-mcp-{label}-{}-{unique}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn server(&self) -> RuntimeServer {
        RuntimeServer::new(ServerConfig {
            runtime: RuntimeConfig {
                registry: RegistryConfig {
                    db_path: self.root.join("registry/registry.sqlite3"),
                    store_root: self.root.join("registry"),
                    busy_timeout_ms: 5000,
                },
                executor: UniversalExecutorConfig {
                    store_root: self.root.join("store"),
                    workspace_root: None,
                    workspace_uid: None,
                    workspace_gid: None,
                    runner_path: PathBuf::from("/usr/bin/true"),
                    allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
                    max_runtime_ms: 10_000,
                    max_output_bytes: 1024 * 1024,
                },
                startup_grace_ms: 1000,
            },
            execution: ExecutionContext {
                principal: "principal:mcp-test".to_string(),
                global_limit: 4,
            },
            trace_path: None,
        })
        .unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn submit(
    server: &RuntimeServer,
    client_request_id: &str,
) -> ordivon_runtime_core::CreatedAdmission {
    let workspace = std::env::current_dir()
        .unwrap()
        .join("target/ordivon-tests/ordivon-runtime-mcp-workspace");
    let workspace = workspace.to_string_lossy().into_owned();
    let outcome = server
        .state
        .runtime
        .registry()
        .submit(&SubmitRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: client_request_id.to_string(),
            request_identity_digest: None,
            plan: RuntimeExecutionPlan {
                schema_version: RUNTIME_SCHEMA_VERSION,
                workspace_id: "workspace:mcp-test".to_string(),
                workspace_path: workspace.clone(),
                source_revision: "revision:test".to_string(),
                workspace_source_digest: None,
                executable: "/usr/bin/true".to_string(),
                executable_digest: format!("sha256:{}", "a".repeat(64)),
                args: Vec::new(),
                cwd: workspace,
                env: Default::default(),
                timeout_ms: 1000,
                stdout_limit_bytes: 1024,
                stderr_limit_bytes: 1024,
                steps: Vec::new(),
                budget: ordivon_runtime_core::ExecutionBudget::default(),
                principal: "principal:mcp-test".to_string(),
            },
            global_limit: 4,
        })
        .unwrap();
    match outcome {
        AdmissionOutcome::Created(created) => *created,
        AdmissionOutcome::Existing { .. } => panic!("expected a new Job"),
    }
}

#[test]
fn server_clones_share_one_runtime_state() {
    let sandbox = Sandbox::new("shared-state");
    let server = sandbox.server();
    let cloned = server.clone();
    assert!(Arc::ptr_eq(&server.state, &cloned.state));
}

#[test]
fn server_identity_names_the_runtime_component() {
    let sandbox = Sandbox::new("identity");
    let info = serde_json::to_value(sandbox.server().get_info()).unwrap();
    assert_eq!(
        info.pointer("/serverInfo/name").and_then(Value::as_str),
        Some("ordivon-runtime-mcp")
    );
    assert_eq!(
        info.pointer("/serverInfo/title").and_then(Value::as_str),
        Some("Ordivon Runtime")
    );
}

#[test]
fn tool_catalog_uses_transactional_job_contract() {
    let sandbox = Sandbox::new("catalog");
    let server = sandbox.server();
    let mut tools = server.tool_router.list_all();
    tools.sort_by(|left, right| left.name.cmp(&right.name));
    let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "artifact.read",
            "task.cancel",
            "task.list",
            "task.observe",
            "workspace.close",
            "workspace.diff",
            "workspace.exec",
            "workspace.execPlan",
            "workspace.get",
            "workspace.list",
            "workspace.mutate",
            "workspace.open",
            "workspace.patch",
            "workspace.read",
        ]
    );
    for tool in tools
        .iter()
        .filter(|tool| tool.name.as_ref() != "task.list")
    {
        let schema = serde_json::to_value(&tool.input_schema).unwrap();
        assert_eq!(
            schema.pointer("/properties/schemaVersion/const"),
            Some(&serde_json::json!(1)),
            "{} schemaVersion const drifted",
            tool.name
        );
        assert_eq!(
            schema.pointer("/properties/schemaVersion/minimum"),
            Some(&serde_json::json!(1)),
            "{} schemaVersion minimum drifted",
            tool.name
        );
        assert_eq!(
            schema.pointer("/properties/schemaVersion/maximum"),
            Some(&serde_json::json!(1)),
            "{} schemaVersion maximum drifted",
            tool.name
        );
    }

    let exec = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.exec")
        .unwrap();
    assert_eq!(exec.task_support(), TaskSupport::Optional);
    assert_eq!(
        exec.annotations
            .as_ref()
            .and_then(|annotations| annotations.idempotent_hint),
        Some(false)
    );
    let exec_plan = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.execPlan")
        .unwrap();
    assert_eq!(exec_plan.task_support(), TaskSupport::Optional);
    assert!(tools
        .iter()
        .filter(|tool| !matches!(tool.name.as_ref(), "workspace.exec" | "workspace.execPlan"))
        .all(|tool| tool.task_support() == TaskSupport::Forbidden));
    let schema = serde_json::to_string(&exec.input_schema).unwrap();
    assert!(schema.contains("clientRequestId"));
    assert!(!schema.contains("taskId"));
    let exec_schema = serde_json::to_value(&exec.input_schema).unwrap();
    assert!(exec_schema
        .pointer("/$defs/UniversalExecutionRequest/properties/executable/description")
        .and_then(Value::as_str)
        .is_some_and(|description| description.contains("Absolute host path")));
    assert!(exec_schema
        .pointer("/$defs/UniversalExecutionRequest/properties/cwdRelative/description")
        .and_then(Value::as_str)
        .is_some_and(|description| description.contains("relative to the Workspace root")));
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionBudget/properties/memoryMaxBytes/maximum"),
        Some(&serde_json::json!(
            ordivon_runtime_core::MAX_MEMORY_MAX_BYTES
        ))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionBudget/properties/tasksMax/maximum"),
        Some(&serde_json::json!(ordivon_runtime_core::MAX_TASKS_MAX))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionBudget/properties/cpuQuotaPercent/maximum"),
        Some(&serde_json::json!(
            ordivon_runtime_core::MAX_CPU_QUOTA_PERCENT
        ))
    );
    for server_owned in ["principal", "globalLimit", "profileLimit"] {
        assert!(
            !schema.contains(server_owned),
            "schema exposes {server_owned}"
        );
    }

    let mutate = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.mutate")
        .unwrap();
    let mutate_schema = serde_json::to_value(&mutate.input_schema).unwrap();
    assert_eq!(
        mutate_schema.pointer("/$defs/WorkspaceMutation/properties/mode/enum"),
        Some(&serde_json::json!(["WRITE", "APPEND", "REPLACE_EXACT"]))
    );
    assert_eq!(
        mutate_schema.pointer("/properties/mutations/minItems"),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        mutate_schema.pointer("/properties/mutations/maxItems"),
        Some(&serde_json::json!(32))
    );
    assert!(
        mutate_schema
            .pointer("/$defs/WorkspaceMutation/properties/expectedDigest/description")
            .and_then(Value::as_str)
            .is_some_and(
                |description| description.contains("Required when the target already exists")
            )
    );

    let observe = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "task.observe")
        .unwrap();
    let observe_schema = serde_json::to_value(&observe.input_schema).unwrap();
    assert_eq!(
        observe_schema.pointer("/properties/waitMs/maximum"),
        Some(&serde_json::json!(30_000))
    );
    assert_eq!(
        observe_schema.pointer("/properties/stdoutTailBytes/maximum"),
        Some(&serde_json::json!(65_536))
    );
    assert!(observe_schema.pointer("/properties/stdoutOffset").is_some());
    assert!(observe_schema.pointer("/properties/stderrOffset").is_some());

    let list = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "task.list")
        .unwrap();
    let list_schema = serde_json::to_value(&list.input_schema).unwrap();
    assert_eq!(
        list_schema.pointer("/properties/limit/maximum"),
        Some(&serde_json::json!(100))
    );
    assert_eq!(
        list_schema.pointer("/properties/limit/default"),
        Some(&serde_json::json!(20))
    );
    assert!(list_schema.pointer("/properties/clientRequestId").is_some());

    let close = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.close")
        .unwrap();
    let close_schema = serde_json::to_value(&close.input_schema).unwrap();
    assert_eq!(
        close_schema.pointer("/properties/force/default"),
        Some(&serde_json::json!(false))
    );
}

#[test]
fn task_observation_serializes_discoverable_artifacts() {
    let observation = TaskObservation {
        job_id: "job-test".to_string(),
        status: "succeeded".to_string(),
        attempt_id: Some("attempt-test".to_string()),
        exit_code: Some(0),
        stdout_tail: "ok\n".to_string(),
        stderr_tail: String::new(),
        stdout_offset: None,
        stdout_next_offset: None,
        stdout_available_bytes: None,
        stdout_eof: None,
        stderr_offset: None,
        stderr_next_offset: None,
        stderr_available_bytes: None,
        stderr_eof: None,
        stdout_truncated: false,
        stderr_truncated: false,
        artifacts_available: true,
        artifacts: vec![ArtifactDescriptor {
            artifact_id: "attempt-test.stdout".to_string(),
            kind: "stdout".to_string(),
            digest: format!("sha256:{}", "a".repeat(64)),
            retained_bytes: 3,
            dropped_bytes: Some(0),
            truncated: false,
        }],
        poll_after_ms: None,
        elapsed_ms: None,
        last_output_at_ms: None,
        progress_revision: None,
        completed_steps: None,
        total_steps: None,
        current_step_id: None,
        current_step_index: None,
        current_step_elapsed_ms: None,
        failed_step_id: None,
        failed_step_index: None,
        error_summary: None,
    };
    let value = serde_json::to_value(observation).unwrap();
    assert_eq!(
        value
            .pointer("/artifacts/0/artifactId")
            .and_then(Value::as_str),
        Some("attempt-test.stdout")
    );
    assert_eq!(
        value
            .pointer("/artifacts/0/droppedBytes")
            .and_then(Value::as_u64),
        Some(0)
    );
}

#[test]
fn structured_failure_is_a_tool_error_not_protocol_failure() {
    let outcome = ToolOutcome::<String>::Error(ToolError::invalid(
        "idempotency mismatch",
        "clientRequestId",
    ));
    let result = outcome.into_call_tool_result().unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.content.len(), 1);
    assert_eq!(
        result
            .structured_content
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(|value| value.get("field"))
            .and_then(Value::as_str),
        Some("clientRequestId")
    );
}

#[test]
fn job_projection_becomes_native_mcp_task_without_projection_file() {
    let sandbox = Sandbox::new("task-projection");
    let server = sandbox.server();
    let created = submit(&server, "request:projection");
    let projection = server
        .state
        .runtime
        .registry()
        .project_job(&created.job.job_id)
        .unwrap();
    let task = task_from_job(created.job.clone(), Some(&created.attempt), projection).unwrap();
    assert_eq!(task.task_id, created.job.job_id);
    assert_eq!(task.status, TaskStatus::Working);
    assert_eq!(task.poll_interval, Some(250));
    assert!(task.created_at.starts_with("20"));
    assert!(!server
        .state
        .executor
        .store_root
        .join("m4-native-task-projections")
        .exists());
}

#[test]
fn cancelled_job_projects_to_cancelled_native_task() {
    let sandbox = Sandbox::new("cancelled-projection");
    let server = sandbox.server();
    let created = submit(&server, "request:cancelled-projection");
    server
        .state
        .runtime
        .registry()
        .request_cancel(&created.job.job_id, created.job.created_at_ms + 1)
        .unwrap();
    let job = server
        .state
        .runtime
        .registry()
        .get_job(&created.job.job_id)
        .unwrap();
    let attempt = server
        .state
        .runtime
        .registry()
        .get_latest_attempt(&created.job.job_id)
        .unwrap()
        .unwrap();
    let projection = server
        .state
        .runtime
        .registry()
        .project_job(&created.job.job_id)
        .unwrap();
    let task = task_from_job(job, Some(&attempt), projection).unwrap();
    assert_eq!(task.status, TaskStatus::Cancelled);
    assert_eq!(task.poll_interval, None);
}

#[test]
fn capacity_failure_preserves_retry_and_scope_metadata() {
    let error = RuntimeError::concurrency(
        "global execution concurrency limit reached (active=4, limit=4)",
        "globalLimit",
        RuntimeCapacity {
            scope: "global".to_string(),
            active: 4,
            limit: 4,
            workspace_id: None,
        },
    );
    let tool_error = ToolError::from(error);
    let value = serde_json::to_value(tool_error).unwrap();
    assert_eq!(
        value.pointer("/retryAfterMs").and_then(Value::as_u64),
        Some(1_000)
    );
    assert_eq!(
        value.pointer("/capacity/scope").and_then(Value::as_str),
        Some("global")
    );
    assert_eq!(
        value.pointer("/capacity/active").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        value.pointer("/capacity/limit").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        value.pointer("/origin").and_then(Value::as_str),
        Some("runtime_core")
    );
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("safe_same_request")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("not_started")
    );
}

#[test]
fn unknown_dispatch_outcome_requires_reconciliation_before_retry() {
    let error = RuntimeError::new(
        ordivon_runtime_core::RuntimeErrorCode::DispatchOutcomeUnknown,
        "launch response was lost after dispatch",
        Some("clientRequestId"),
        true,
    );
    let value = serde_json::to_value(ToolError::from(error)).unwrap();
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("reconcile_first")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(
        value.pointer("/origin").and_then(Value::as_str),
        Some("runtime_core")
    );
}

#[test]
fn workspace_open_schema_prefers_server_generated_handles() {
    let sandbox = Sandbox::new("workspace-open-schema");
    let server = sandbox.server();
    let open = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "workspace.open")
        .unwrap();
    let schema = serde_json::to_value(&open.input_schema).unwrap();
    let required = schema
        .pointer("/required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!required
        .iter()
        .any(|value| value.as_str() == Some("workspaceId")));
    let bound = WorkspaceOpenRequest {
        schema_version: 1,
        workspace_id: None,
        source_repo: "/tmp/repository".to_string(),
        source_revision: "HEAD".to_string(),
    }
    .bind();
    assert!(bound.workspace_id.starts_with("ws-"));
    assert_eq!(bound.workspace_id.len(), 39);
}

#[test]
fn typed_error_envelope_distinguishes_unknown_commit_state() {
    let error = RuntimeError::new(
        ordivon_runtime_core::RuntimeErrorCode::DispatchOutcomeUnknown,
        "dispatch response was lost",
        Some("operationId"),
        true,
    );
    let value = serde_json::to_value(ToolError::from(error)).unwrap();
    assert_eq!(
        value.pointer("/origin").and_then(Value::as_str),
        Some("runtime_core")
    );
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("reconcile_first")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(
        value.pointer("/retryable").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn incomplete_workspace_rollback_requires_reconciliation() {
    let error = UniversalExecError {
        code: ordivon_runtime_core::UniversalExecErrorCode::WorkspaceMutationIncomplete,
        message: "patch failed and rollback could not restore one file".to_string(),
        field: Some("files".to_string()),
        retryable: false,
    };
    let value = serde_json::to_value(ToolError::from(error)).unwrap();
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("reconcile_first")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("unknown")
    );
}
