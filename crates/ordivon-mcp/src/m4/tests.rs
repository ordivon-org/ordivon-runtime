use super::tasks::{list_native_task_ids, task_from_snapshot};
use super::*;
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
        let root = std::env::temp_dir().join(format!(
            "ordivon-m4-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn server(&self) -> M4Server {
        M4Server::new(M4ServerConfig {
            executor: UniversalExecutorConfig {
                store_root: self.root.join("store"),
                runner_path: PathBuf::from("/usr/bin/true"),
                allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
                max_runtime_ms: 10_000,
                max_output_bytes: 1024 * 1024,
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

#[test]
fn tool_catalog_is_thin_and_exec_is_optional_task() {
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
}

#[test]
fn structured_failure_is_a_tool_error_not_protocol_failure() {
    let outcome = M4Outcome::<String> {
        ok: false,
        result: None,
        error: Some(M4Error::invalid("digest mismatch", "expectedDigest")),
        trace: None,
    };
    let result = outcome.into_call_tool_result().unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result
            .structured_content
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(|value| value.get("field"))
            .and_then(Value::as_str),
        Some("expectedDigest")
    );
}

#[test]
fn native_task_projection_paginates_without_becoming_task_truth() {
    let sandbox = Sandbox::new("pagination");
    let server = sandbox.server();
    for index in 0..101 {
        write_native_projection(
            &server.state.config.executor,
            &NativeTaskProjection {
                schema_version: 1,
                task_id: format!("task-{index:03}"),
                stdout_tail_bytes: 128,
                stderr_tail_bytes: 128,
                ttl_ms: Some(60_000),
            },
        )
        .unwrap();
    }
    let first = list_native_task_ids(&server.state.config.executor, None).unwrap();
    assert_eq!(first.0.len(), 100);
    assert_eq!(first.1.as_deref(), Some("task-099"));
    let second = list_native_task_ids(&server.state.config.executor, first.1.clone()).unwrap();
    assert_eq!(second.0, vec!["task-100"]);
    assert_eq!(second.1, None);
    assert!(!server
        .state
        .config
        .executor
        .tasks_root()
        .join("task-000")
        .exists());
}

#[test]
fn durable_snapshot_maps_to_protocol_task_without_session_state() {
    let task = task_from_snapshot(
        DurableTaskSnapshot {
            task_id: "task-stable".to_string(),
            status: MigrationTaskStatus::Working,
            status_message: "working".to_string(),
            created_unix_ms: 1_700_000_000_000,
            updated_unix_ms: 1_700_000_000_100,
            poll_after_ms: Some(250),
            result_available: false,
        },
        Some(60_000),
    )
    .unwrap();
    assert_eq!(task.task_id, "task-stable");
    assert_eq!(task.status, TaskStatus::Working);
    assert_eq!(task.poll_interval, Some(250));
    assert_eq!(task.ttl, Some(60_000));
    assert!(task.created_at.starts_with("2023-"));
}
