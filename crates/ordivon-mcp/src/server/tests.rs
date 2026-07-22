use super::tasks::task_from_job;
use super::*;
use ordivon_exec::{
    AdmissionOutcomeM6, M6ExecutionPlan, M6RegistryConfig, M6SubmitRequest, PlanKind,
    M6_SCHEMA_VERSION,
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
        let root = PathBuf::from("/root/.local/share").join(format!(
            "ordivon-mcp-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn server(&self) -> OrdivonServer {
        OrdivonServer::new(ServerConfig {
            runtime: M6RuntimeConfig {
                registry: M6RegistryConfig {
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
                hardening: None,
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

fn submit(server: &OrdivonServer, client_request_id: &str) -> ordivon_exec::CreatedAdmissionM6 {
    let outcome = server
        .state
        .runtime
        .registry()
        .submit(&M6SubmitRequest {
            schema_version: M6_SCHEMA_VERSION,
            client_request_id: client_request_id.to_string(),
            plan: M6ExecutionPlan {
                schema_version: M6_SCHEMA_VERSION,
                plan_kind: PlanKind::UniversalSandbox,
                workspace_id: "workspace:mcp-test".to_string(),
                workspace_path: "/root/.local/share/ordivon-mcp-workspace".to_string(),
                source_revision: "revision:test".to_string(),
                executable: "/usr/bin/true".to_string(),
                executable_digest: format!("sha256:{}", "a".repeat(64)),
                args: Vec::new(),
                cwd: "/root/.local/share/ordivon-mcp-workspace".to_string(),
                env: Default::default(),
                timeout_ms: 1000,
                stdout_limit_bytes: 1024,
                stderr_limit_bytes: 1024,
                policy_id: "policy:mcp-test".to_string(),
                policy_version: "1".to_string(),
                policy_digest: format!("sha256:{}", "b".repeat(64)),
                profile_id: None,
                principal: "principal:mcp-test".to_string(),
                authority_ref: "authority:mcp-test".to_string(),
            },
            global_limit: 4,
            profile_limit: None,
        })
        .unwrap();
    match outcome {
        AdmissionOutcomeM6::Created(created) => *created,
        AdmissionOutcomeM6::Existing { .. } => panic!("expected a new Job"),
    }
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
            "workspace.diff",
            "workspace.exec",
            "workspace.mutate",
            "workspace.open",
            "workspace.read",
        ]
    );
    let exec = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.exec")
        .unwrap();
    assert_eq!(exec.task_support(), TaskSupport::Optional);
    assert!(tools
        .iter()
        .filter(|tool| tool.name.as_ref() != "workspace.exec")
        .all(|tool| tool.task_support() == TaskSupport::Forbidden));
    let schema = serde_json::to_string(&exec.input_schema).unwrap();
    assert!(schema.contains("clientRequestId"));
    assert!(!schema.contains("taskId"));
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
