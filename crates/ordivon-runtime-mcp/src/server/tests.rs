use super::*;
use ordivon_runtime_core::{
    ArtifactDescriptor, AttemptState, AttemptTerminationIntent, JobDesiredState, RegistryConfig,
    RuntimeDeliveryDisposition, TaskObservation,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
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
                windows: None,
            },
            input_authorities: Vec::new(),
            execution: ExecutionContext {
                principal: "principal:mcp-test".to_string(),
                global_limit: 4,
            },
            release: None,
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

fn exec_tool_request(
    timeout_ms: Option<u64>,
    stdout_limit_bytes: Option<u64>,
    stderr_limit_bytes: Option<u64>,
) -> WorkspaceExecRequest {
    WorkspaceExecRequest {
        schema_version: 1,
        client_request_id: "request:mcp-proposal".to_string(),
        execution: ExecutionProposal {
            workspace_id: "workspace:test".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: Default::default(),
            timeout_ms,
            stdout_limit_bytes,
            stderr_limit_bytes,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: ExecutionTarget::LocalLinux,
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    }
}

fn plan_step(id: &str, timeout_ms: Option<u64>) -> ExecutionStepProposal {
    ExecutionStepProposal {
        id: id.to_string(),
        executable: "/usr/bin/true".to_string(),
        args: Vec::new(),
        cwd_relative: ".".to_string(),
        env: Default::default(),
        timeout_ms,
        continue_on_error: false,
    }
}

#[test]
fn workspace_exec_schema_exposes_mechanical_limits_as_optional() {
    let server = Sandbox::new("proposal-schema").server();
    let tools = server.tool_router.list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.exec")
        .expect("workspace.exec");
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    assert_eq!(
        schema.pointer("/properties/waitMs/default"),
        Some(&serde_json::json!(2_000))
    );
    assert_eq!(
        schema.pointer("/properties/waitMs/maximum"),
        Some(&serde_json::json!(30_000))
    );
    let required = schema
        .pointer("/$defs/ExecutionProposal/required")
        .and_then(Value::as_array)
        .expect("ExecutionProposal required array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(required.contains(&"workspaceId"));
    assert!(required.contains(&"executable"));
    assert!(required.contains(&"cwdRelative"));
    assert!(!required.contains(&"timeoutMs"));
    assert!(!required.contains(&"stdoutLimitBytes"));
    assert!(!required.contains(&"stderrLimitBytes"));
    assert!(schema
        .pointer("/$defs/ExecutionProposal/properties/hostDependencies")
        .is_some());
    let dependency = schema
        .pointer("/$defs/HostDependencyBinding/properties/expectedDigest")
        .expect("HostDependencyBinding.expectedDigest schema");
    assert!(dependency.is_object());
}

#[test]
fn workspace_exec_bound_schema_exposes_only_named_immutable_input_authority() {
    let server = Sandbox::new("bound-schema").server();
    let tools = server.tool_router.list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.execBound")
        .expect("workspace.execBound");
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    assert_eq!(
        schema.pointer("/properties/waitMs/default"),
        Some(&serde_json::json!(2_000))
    );
    assert_eq!(
        schema.pointer("/properties/inputs/minItems"),
        Some(&serde_json::json!(1))
    );
    assert!(schema
        .pointer("/$defs/WorkspaceExecBoundExecution/properties/executionProfile")
        .is_none());
    let required = schema
        .pointer("/$defs/WorkspaceExecBoundExecution/required")
        .and_then(Value::as_array)
        .expect("WorkspaceExecBoundExecution required")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(required.contains(&"workspaceId"));
    assert!(required.contains(&"executable"));
    assert!(required.contains(&"cwdRelative"));
    assert!(!required.contains(&"timeoutMs"));
    assert!(!required.contains(&"stdoutLimitBytes"));
    assert!(!required.contains(&"stderrLimitBytes"));
    assert_eq!(
        schema.pointer("/$defs/InputBindingRequest/required"),
        Some(&serde_json::json!([
            "authority",
            "relativeObject",
            "expectedDigest",
            "presentationRelativePath"
        ]))
    );
    assert_eq!(
        schema.pointer("/$defs/InputBindingRequest/additionalProperties"),
        Some(&serde_json::json!(false))
    );
    let text = serde_json::to_string(&schema).unwrap();
    assert!(!text.contains("sourcePath"));
}

#[test]
fn workspace_exec_plan_defaults_to_brief_mcp_observation() {
    let server = Sandbox::new("plan-wait-schema").server();
    let tools = server.tool_router.list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.execPlan")
        .expect("workspace.execPlan");
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    assert_eq!(
        schema.pointer("/properties/waitMs/default"),
        Some(&serde_json::json!(2_000))
    );
    assert_eq!(default_exec_wait_ms(), 2_000);
}

#[test]
fn workspace_exec_bound_binding_forces_contained_local() {
    let server = Sandbox::new("bound-binding").server();
    let request = WorkspaceExecBoundRequest {
        schema_version: 1,
        client_request_id: "request:bound".to_string(),
        execution: WorkspaceExecBoundExecution {
            workspace_id: "workspace:test".to_string(),
            executable: "/usr/bin/true".to_string(),
            args: Vec::new(),
            cwd_relative: ".".to_string(),
            env: Default::default(),
            timeout_ms: None,
            stdout_limit_bytes: None,
            stderr_limit_bytes: None,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_target: ExecutionTarget::LocalLinux,
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
        },
        inputs: vec![InputBindingRequest {
            authority: "finance-prepared".to_string(),
            relative_object: "bundle/manifest.json".to_string(),
            expected_digest:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            presentation_relative_path: "finance-lab/bundle/manifest.json".to_string(),
        }],
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    let (proposal, inputs) = server.state.execution.bind_bound(request);
    assert_eq!(
        proposal.execution.execution_profile,
        ExecutionProfile::ContainedLocal
    );
    assert_eq!(proposal.execution.timeout_ms, None);
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].authority, "finance-prepared");
}

#[test]
fn workspace_exec_preserves_legacy_v1_only_for_fully_explicit_requests() {
    let server = Sandbox::new("proposal-bind").server();
    match server
        .state
        .execution
        .bind(exec_tool_request(Some(2_000), Some(4_096), Some(8_192)))
    {
        BoundTaskRun::Legacy(request) => {
            assert_eq!(request.execution.timeout_ms, 2_000);
            assert_eq!(request.execution.stdout_limit_bytes, 4_096);
            assert_eq!(request.execution.stderr_limit_bytes, 8_192);
        }
        BoundTaskRun::Proposal(_) => panic!("fully explicit legacy request changed identity mode"),
    }

    match server
        .state
        .execution
        .bind(exec_tool_request(Some(2_000), None, None))
    {
        BoundTaskRun::Proposal(proposal) => {
            assert_eq!(proposal.execution.timeout_ms, Some(2_000));
            assert_eq!(proposal.execution.stdout_limit_bytes, None);
            assert_eq!(proposal.execution.stderr_limit_bytes, None);
        }
        BoundTaskRun::Legacy(_) => panic!("optional request must be a Core proposal"),
    }
}

#[test]
fn workspace_exec_plan_schema_exposes_job_wide_host_dependencies() {
    let server = Sandbox::new("plan-host-dependency-schema").server();
    let tool = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "workspace.execPlan")
        .expect("workspace.execPlan");
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    assert!(schema
        .pointer("/$defs/WorkspaceExecPlanInput/properties/hostDependencies")
        .is_some());
}

#[test]
fn workspace_exec_plan_keeps_legacy_sum_only_for_legacy_shape() {
    let server = Sandbox::new("proposal-plan-bind").server();
    let legacy = WorkspaceExecPlanRequest {
        schema_version: 1,
        client_request_id: "request:mcp-plan-legacy".to_string(),
        execution: WorkspaceExecPlanInput {
            workspace_id: "workspace:test".to_string(),
            steps: vec![plan_step("one", Some(2_000)), plan_step("two", Some(3_000))],
            timeout_ms: None,
            stdout_limit_bytes: Some(4_096),
            stderr_limit_bytes: Some(8_192),
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: ExecutionTarget::LocalLinux,
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    match server.state.execution.bind_plan(legacy).unwrap() {
        BoundTaskRun::Legacy(request) => {
            assert_eq!(request.execution.timeout_ms, 5_000);
            assert_eq!(
                request
                    .execution
                    .steps
                    .iter()
                    .map(|step| step.timeout_ms)
                    .collect::<Vec<_>>(),
                vec![2_000, 3_000]
            );
        }
        BoundTaskRun::Proposal(_) => panic!("legacy plan changed identity mode"),
    }

    let optional = WorkspaceExecPlanRequest {
        schema_version: 1,
        client_request_id: "request:mcp-plan-proposal".to_string(),
        execution: WorkspaceExecPlanInput {
            workspace_id: "workspace:test".to_string(),
            steps: vec![plan_step("one", Some(2_000)), plan_step("two", None)],
            timeout_ms: None,
            stdout_limit_bytes: None,
            stderr_limit_bytes: None,
            budget: ExecutionBudget::default(),
            execution_profile: ExecutionProfile::TrustedLocal,
            execution_target: ExecutionTarget::LocalLinux,
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 0,
        stderr_tail_bytes: 0,
    };
    match server.state.execution.bind_plan(optional).unwrap() {
        BoundTaskRun::Proposal(proposal) => {
            assert_eq!(proposal.execution.timeout_ms, None);
            assert_eq!(proposal.execution.steps[0].timeout_ms, Some(2_000));
            assert_eq!(proposal.execution.steps[1].timeout_ms, None);
            assert_eq!(proposal.execution.stdout_limit_bytes, None);
        }
        BoundTaskRun::Legacy(_) => panic!("optional plan must be a Core proposal"),
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
fn tool_effect_annotations_match_runtime_behavior() {
    let sandbox = Sandbox::new("effect-annotations");
    let server = sandbox.server();
    let tools = server.tool_router.list_all();
    let expected = [
        ("artifact.read", true, false, true, false),
        ("release.apply", false, true, true, true),
        ("release.get", true, false, true, false),
        ("runtime.describe", true, false, true, false),
        ("task.cancel", false, true, true, false),
        ("task.get", true, false, true, false),
        ("task.list", true, false, true, false),
        ("task.observe", false, true, true, true),
        ("workspace.close", false, true, true, false),
        ("workspace.changes", true, false, true, false),
        ("workspace.content", true, false, true, false),
        ("workspace.diff", true, false, true, false),
        ("workspace.exec", false, true, false, true),
        ("workspace.execBound", false, true, false, false),
        ("workspace.execPlan", false, true, false, true),
        ("workspace.get", true, false, true, false),
        ("workspace.list", true, false, true, false),
        ("workspace.mutate", false, true, false, false),
        ("workspace.open", false, false, false, false),
        ("workspace.patch", false, true, true, false),
        ("workspace.patch.get", false, false, true, false),
        ("workspace.read", true, false, true, false),
    ];
    assert_eq!(tools.len(), expected.len());
    for (name, read_only, destructive, idempotent, open_world) in expected {
        let tool = tools
            .iter()
            .find(|tool| tool.name.as_ref() == name)
            .unwrap_or_else(|| panic!("missing Tool {name}"));
        let annotations = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("Tool {name} omitted annotations"));
        assert_eq!(
            annotations.read_only_hint,
            Some(read_only),
            "{name} readOnlyHint"
        );
        assert_eq!(
            annotations.destructive_hint,
            Some(destructive),
            "{name} destructiveHint"
        );
        assert_eq!(
            annotations.idempotent_hint,
            Some(idempotent),
            "{name} idempotentHint"
        );
        assert_eq!(
            annotations.open_world_hint,
            Some(open_world),
            "{name} openWorldHint"
        );
    }
}

#[test]
fn tool_inputs_default_missing_schema_version_to_pinned_version() {
    use ordivon_runtime_core::{
        RuntimeWorkspaceListRequest, TaskCancelRequest, WorkspaceCloseRequest,
        WorkspaceContentRequest, WorkspaceMutateRequest,
    };
    // Core-crate request structs used by MCP tools: omitted schemaVersion
    // must deserialize to the pinned version instead of failing.
    let list: RuntimeWorkspaceListRequest = serde_json::from_str(r#"{"limit": 5}"#).unwrap();
    assert_eq!(list.schema_version, 1);
    let cancel: TaskCancelRequest = serde_json::from_str(r#"{"jobId":"job-1"}"#).unwrap();
    assert_eq!(cancel.schema_version, 1);
    let close: WorkspaceCloseRequest = serde_json::from_str(r#"{"workspaceId":"ws-1"}"#).unwrap();
    assert_eq!(close.schema_version, 1);
    let content: WorkspaceContentRequest = serde_json::from_str(
        r#"{"workspaceId":"ws-1","relativePath":"a.txt","expectedDigest":"x","maxBytes":10}"#,
    )
    .unwrap();
    assert_eq!(content.schema_version, 1);
    let mutate: WorkspaceMutateRequest =
        serde_json::from_str(r#"{"workspaceId":"ws-1","mutations":[]}"#).unwrap();
    assert_eq!(mutate.schema_version, 1);
    // MCP-crate request structs.
    let get: TaskGetRequest = serde_json::from_str(r#"{"jobId":"job-1"}"#).unwrap();
    assert_eq!(get.schema_version, 1);
    let diff: WorkspaceDiffRequest =
        serde_json::from_str(r#"{"workspaceId":"ws-1","maxBytes":10}"#).unwrap();
    assert_eq!(diff.schema_version, 1);
    let describe: RuntimeDescribeRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(describe.schema_version, 1);
    let release_apply: RuntimeReleaseApplyToolRequest = serde_json::from_str(
        r#"{"clientRequestId":"release-1","workspaceId":"ws-1","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","candidateManifestDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expectedToolCount":22}"#,
    )
    .unwrap();
    assert_eq!(release_apply.schema_version, 1);
    let release_get: RuntimeReleaseGetToolRequest =
        serde_json::from_str(r#"{"clientRequestId":"release-1"}"#).unwrap();
    assert_eq!(release_get.schema_version, 1);
    // Explicit non-pinned versions survive deserialization and are rejected
    // by the handler gate ("schemaVersion must be 1") — the pin keeps teeth.
    let wrong: TaskGetRequest =
        serde_json::from_str(r#"{"jobId":"job-1","schemaVersion":2}"#).unwrap();
    assert_eq!(wrong.schema_version, 2);
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
            "release.apply",
            "release.get",
            "runtime.describe",
            "task.cancel",
            "task.get",
            "task.list",
            "task.observe",
            "workspace.changes",
            "workspace.close",
            "workspace.content",
            "workspace.diff",
            "workspace.exec",
            "workspace.execBound",
            "workspace.execPlan",
            "workspace.get",
            "workspace.list",
            "workspace.mutate",
            "workspace.open",
            "workspace.patch",
            "workspace.patch.get",
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

    assert_eq!(
        exec.annotations
            .as_ref()
            .and_then(|annotations| annotations.idempotent_hint),
        Some(false)
    );
    let schema = serde_json::to_string(&exec.input_schema).unwrap();
    assert!(schema.contains("clientRequestId"));
    assert!(!schema.contains("taskId"));
    let exec_schema = serde_json::to_value(&exec.input_schema).unwrap();
    assert!(exec_schema
        .pointer("/$defs/ExecutionProposal/properties/executable/description")
        .and_then(Value::as_str)
        .is_some_and(|description| description.contains("Absolute host path")));
    assert!(exec_schema
        .pointer("/$defs/ExecutionProposal/properties/cwdRelative/description")
        .and_then(Value::as_str)
        .is_some_and(|description| description.contains("relative to the Workspace root")));
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionProposal/properties/executionProfile/default"),
        Some(&serde_json::json!("trusted_local"))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionProfile/enum"),
        Some(&serde_json::json!(["trusted_local", "contained_local"]))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionProposal/properties/executionTarget/default"),
        Some(&serde_json::json!("local_linux"))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionTarget/enum"),
        Some(&serde_json::json!(["local_linux", "windows_native"]))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionProposal/properties/windowsAuthority/default"),
        Some(&serde_json::json!("limited"))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/WindowsAuthority/enum"),
        Some(&serde_json::json!(["limited", "elevated"]))
    );
    assert!(exec_schema
        .pointer("/$defs/ExecutionProposal/properties/foreignReferences/maxItems")
        .is_none());
    assert_eq!(
        exec_schema.pointer("/$defs/ExecutionProposal/properties/foreignReferences/items/$ref"),
        Some(&serde_json::json!("#/$defs/ForeignReference"))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ForeignReference/required"),
        Some(&serde_json::json!(["namespace", "type", "id"]))
    );
    assert_eq!(
        exec_schema.pointer("/$defs/ForeignReference/additionalProperties"),
        Some(&serde_json::json!(false))
    );

    for budget_field in ["memoryMaxBytes", "tasksMax", "cpuQuotaPercent"] {
        assert_eq!(
            exec_schema.pointer(&format!(
                "/$defs/ExecutionBudget/properties/{budget_field}/minimum"
            )),
            Some(&serde_json::json!(1))
        );
        assert!(exec_schema
            .pointer(&format!(
                "/$defs/ExecutionBudget/properties/{budget_field}/maximum"
            ))
            .is_none());
    }
    for server_owned in ["principal", "globalLimit", "profileLimit"] {
        assert!(
            !schema.contains(server_owned),
            "schema exposes {server_owned}"
        );
    }

    let patch = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.patch")
        .unwrap();
    let patch_schema = serde_json::to_value(&patch.input_schema).unwrap();
    assert_eq!(
        patch_schema.pointer("/properties/files/minItems"),
        Some(&serde_json::json!(1))
    );
    assert!(patch_schema.pointer("/properties/files/maxItems").is_none());

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
    assert!(mutate_schema
        .pointer("/properties/mutations/maxItems")
        .is_none());
    assert!(
        mutate_schema
            .pointer("/$defs/WorkspaceMutation/properties/expectedDigest/description")
            .and_then(Value::as_str)
            .is_some_and(
                |description| description.contains("Required when the target already exists")
            )
    );

    let patch = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.patch")
        .unwrap();
    assert_eq!(
        patch
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.idempotent_hint),
        Some(true)
    );
    let patch_schema = serde_json::to_value(&patch.input_schema).unwrap();
    assert!(patch_schema
        .pointer("/properties/clientRequestId")
        .is_some());
    assert!(patch_schema.pointer("/properties/files/minItems").is_some());
    assert!(patch_schema.pointer("/properties/principal").is_none());
    assert_eq!(
        patch_schema.pointer("/$defs/WorkspaceTextPosition/properties/line/minimum"),
        Some(&serde_json::json!(1))
    );

    let patch_get = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.patch.get")
        .unwrap();
    assert_eq!(
        patch_get
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.read_only_hint),
        Some(false)
    );
    let patch_get_schema = serde_json::to_value(&patch_get.input_schema).unwrap();
    assert!(patch_get_schema
        .pointer("/properties/clientRequestId")
        .is_some());
    assert!(patch_get_schema.pointer("/properties/principal").is_none());

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
        desired_state: JobDesiredState::Run,
        attempt_id: Some("attempt-test".to_string()),
        attempt_state: Some(AttemptState::Succeeded),
        termination_intent: Some(AttemptTerminationIntent::Natural),
        exit_code: Some(0),
        execution_terminal: true,
        execution_disposition: Some(ordivon_runtime_core::JobResolution::Succeeded),
        execution_reason_code: Some("PROCESS_EXIT_ZERO".to_string()),
        delivery_disposition: RuntimeDeliveryDisposition::Committed,
        effective_limits: ordivon_runtime_core::EffectiveExecutionLimits {
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 8_192,
            step_timeouts: vec![ordivon_runtime_core::EffectiveStepTimeout {
                id: "step-a".to_string(),
                timeout_ms: 2_000,
            }],
        },
        recovery_required: false,
        semantic_completion_evaluated: false,
        result_available: true,
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
    assert_eq!(
        value
            .pointer("/effectiveLimits/timeoutMs")
            .and_then(Value::as_u64),
        Some(10_000)
    );
    assert_eq!(
        value
            .pointer("/effectiveLimits/stepTimeouts/0/timeoutMs")
            .and_then(Value::as_u64),
        Some(2_000)
    );
    assert_eq!(
        value.pointer("/desiredState").and_then(Value::as_str),
        Some("run")
    );
    assert_eq!(
        value.pointer("/attemptState").and_then(Value::as_str),
        Some("succeeded")
    );
    assert_eq!(
        value.pointer("/terminationIntent").and_then(Value::as_str),
        Some("natural")
    );
    assert_eq!(
        value
            .pointer("/executionDisposition")
            .and_then(Value::as_str),
        Some("succeeded")
    );
    assert_eq!(
        value
            .pointer("/executionReasonCode")
            .and_then(Value::as_str),
        Some("PROCESS_EXIT_ZERO")
    );
    assert_eq!(
        value
            .pointer("/deliveryDisposition")
            .and_then(Value::as_str),
        Some("committed")
    );
    assert_eq!(
        value.pointer("/resultAvailable").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        value
            .pointer("/semanticCompletionEvaluated")
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn workspace_content_projects_native_image_with_digest_bound_structured_metadata() {
    let png = b"\x89PNG\r\n\x1a\nmodel-view".to_vec();
    let metadata = WorkspaceContentMetadata {
        workspace_id: "workspace:model-view".to_string(),
        relative_path: "out/contact-sheet.png".to_string(),
        digest: format!("sha256:{}", "a".repeat(64)),
        media_type: "image/png".to_string(),
        byte_length: png.len() as u64,
    };
    let response = workspace_content_call_result(ToolOutcome::Success(
        ordivon_runtime_core::WorkspaceContentReadResult {
            metadata: metadata.clone(),
            bytes: png.clone(),
        },
    ))
    .unwrap();
    assert_eq!(response.is_error, Some(false));
    assert_eq!(response.content.len(), 1);
    let encoded = serde_json::to_value(&response.content[0]).unwrap();
    assert_eq!(
        encoded.pointer("/type").and_then(Value::as_str),
        Some("image")
    );
    assert_eq!(
        encoded.pointer("/mimeType").and_then(Value::as_str),
        Some("image/png")
    );
    let data = encoded.pointer("/data").and_then(Value::as_str).unwrap();
    assert_eq!(BASE64_STANDARD.decode(data.as_bytes()).unwrap(), png);
    assert_eq!(
        response
            .structured_content
            .as_ref()
            .and_then(|value| value.get("digest"))
            .and_then(Value::as_str),
        Some(metadata.digest.as_str())
    );
    assert_eq!(
        response
            .structured_content
            .as_ref()
            .and_then(|value| value.get("mediaType"))
            .and_then(Value::as_str),
        Some("image/png")
    );
}

#[test]
fn workspace_content_schema_requires_exact_digest_binding() {
    let server = Sandbox::new("content-schema").server();
    let tools = server.tool_router.list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "workspace.content")
        .expect("workspace.content");
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(required.contains(&"workspaceId"));
    assert!(required.contains(&"relativePath"));
    assert!(required.contains(&"expectedDigest"));
    assert!(required.contains(&"maxBytes"));
}

#[test]
fn structured_failure_is_a_tool_error_not_protocol_failure() {
    let outcome = ToolOutcome::<String>::Error(ToolError::invalid(
        "idempotency mismatch",
        "clientRequestId",
    ));
    let response = outcome.into_call_tool_result().unwrap();
    let CallToolResponse::Complete(result) = response else {
        panic!("structured Tool outcome must be complete");
    };
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
fn capacity_failure_preserves_retry_and_scope_metadata() {
    let error = RuntimeError::concurrency(
        "global execution concurrency limit reached (active=4, limit=4)",
        "globalLimit",
        RuntimeCapacity {
            scope: "global".to_string(),
            active: 4,
            limit: 4,
            workspace_id: None,
            holder_job_ids: vec!["job-holder".to_string()],
            holder_workspace_ids: vec!["workspace-holder".to_string()],
            holders_truncated: false,
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
        value
            .pointer("/capacity/holderJobIds/0")
            .and_then(Value::as_str),
        Some("job-holder")
    );
    assert_eq!(
        value
            .pointer("/capacity/holderWorkspaceIds/0")
            .and_then(Value::as_str),
        Some("workspace-holder")
    );
    assert_eq!(
        value
            .pointer("/capacity/holdersTruncated")
            .and_then(Value::as_bool),
        Some(false)
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
fn deployment_in_progress_is_safe_same_request_without_commitment() {
    let error = RuntimeError::deployment_in_progress();
    let value = serde_json::to_value(ToolError::from(error)).unwrap();
    assert_eq!(
        value.pointer("/code").and_then(Value::as_str),
        Some("DEPLOYMENT_IN_PROGRESS")
    );
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("safe_same_request")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("not_started")
    );
    assert_eq!(
        value.pointer("/retryAfterMs").and_then(Value::as_u64),
        Some(1_000)
    );
}

#[test]
fn workspace_exists_guides_reconciliation_instead_of_blind_retry() {
    let error = RuntimeError::new(
        ordivon_runtime_core::RuntimeErrorCode::WorkspaceExists,
        "workspace already exists",
        Some("workspaceId"),
        false,
    );
    let value = serde_json::to_value(ToolError::from(error)).unwrap();
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("reconcile_first")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("not_started")
    );
    assert_eq!(
        value.pointer("/retryable").and_then(Value::as_bool),
        Some(false)
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
fn every_public_tool_publishes_structured_output_contract() {
    let sandbox = Sandbox::new("all-output-schemas");
    let server = sandbox.server();
    let tools = server.tool_router.list_all();
    assert_eq!(tools.len(), 22);
    for tool in tools {
        let schema = tool
            .output_schema
            .as_ref()
            .unwrap_or_else(|| panic!("{} omitted outputSchema", tool.name));
        let value = serde_json::to_value(schema).unwrap();
        assert_eq!(
            value
                .pointer("/oneOf")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
            "{} outputSchema must distinguish success from error",
            tool.name
        );
        assert!(
            serde_json::to_string(&value).unwrap().contains("error"),
            "{} outputSchema omitted the standard error envelope",
            tool.name
        );
    }
}

#[test]
fn workspace_open_output_schema_exposes_success_and_error_contract() {
    let sandbox = Sandbox::new("workspace-open-output-schema");
    let server = sandbox.server();
    let open = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "workspace.open")
        .unwrap();
    let schema = serde_json::to_value(open.output_schema.as_ref().unwrap()).unwrap();
    let encoded = serde_json::to_string(&schema).unwrap();
    assert_eq!(
        schema
            .pointer("/oneOf")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(2)
    );
    for expected in [
        "sourceRevision",
        "error",
        "runtime_core",
        "mcp_adapter",
        "workspace_executor",
        "never",
        "safe_same_request",
        "reconcile_first",
        "not_started",
        "not_committed",
        "committed",
        "unknown",
    ] {
        assert!(
            encoded.contains(expected),
            "output schema omitted {expected}"
        );
    }
}

#[test]
fn task_get_schema_is_projection_only_and_detail_free() {
    let sandbox = Sandbox::new("task-get-schema");
    let server = sandbox.server();
    let task_get = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "task.get")
        .unwrap();
    let input = serde_json::to_value(&task_get.input_schema).unwrap();
    assert_eq!(
        input.pointer("/properties/eventLimit/default"),
        Some(&serde_json::json!(DEFAULT_INSPECTION_EVENT_LIMIT))
    );
    assert_eq!(
        input.pointer("/properties/eventLimit/maximum"),
        Some(&serde_json::json!(MAX_INSPECTION_EVENT_LIMIT))
    );
    assert!(input.pointer("/properties/includeDetail").is_none());
    assert!(input.pointer("/properties/waitMs").is_none());
    assert!(input.pointer("/properties/stdoutTailBytes").is_none());

    let output = serde_json::to_value(task_get.output_schema.as_ref().unwrap()).unwrap();
    let encoded = serde_json::to_string(&output).unwrap();
    for expected in [
        "mechanicallyConverged",
        "semanticCompletionEvaluated",
        "attemptsTruncated",
        "eventsTruncated",
        "timeline",
        "episodes",
        "artifacts",
    ] {
        assert!(
            encoded.contains(expected),
            "task.get output omitted {expected}"
        );
    }
}

#[test]
fn task_list_schema_exposes_workspace_reattachment_filter() {
    let sandbox = Sandbox::new("task-list-workspace-filter-schema");
    let server = sandbox.server();
    let task_list = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "task.list")
        .unwrap();
    let schema = serde_json::to_value(&task_list.input_schema).unwrap();
    assert!(schema.pointer("/properties/workspaceId").is_some());
    let required = schema
        .pointer("/required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!required
        .iter()
        .any(|value| value.as_str() == Some("workspaceId")));
}

#[test]
fn workspace_changes_schema_exposes_bounded_continuation_contract() {
    let sandbox = Sandbox::new("workspace-changes-schema");
    let server = sandbox.server();
    let tool = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "workspace.changes")
        .unwrap();
    let input = serde_json::to_value(&tool.input_schema).unwrap();
    assert_eq!(
        input.pointer("/properties/limit/default"),
        Some(&serde_json::json!(64))
    );
    assert_eq!(
        input.pointer("/properties/limit/maximum"),
        Some(&serde_json::json!(1024))
    );
    assert_eq!(
        input.pointer("/properties/maxBytes/default"),
        Some(&serde_json::json!(262_144))
    );
    assert_eq!(
        input.pointer("/properties/maxBytes/maximum"),
        Some(&serde_json::json!(MAX_WORKSPACE_IO_BYTES))
    );
    assert!(input.pointer("/properties/cursor").is_some());
    assert!(input
        .pointer("/$defs/WorkspaceChangeCursor/properties/changeSetDigest")
        .is_some());
    assert!(input
        .pointer("/$defs/WorkspaceChangeCursor/properties/afterPath")
        .is_some());

    let output = serde_json::to_value(tool.output_schema.as_ref().unwrap()).unwrap();
    let encoded = serde_json::to_string(&output).unwrap();
    assert!(encoded.contains("changeSetDigest"));
    assert!(encoded.contains("nextCursor"));
    assert!(encoded.contains("complete"));
    assert!(encoded.contains("entryBytes"));
    assert!(encoded.contains("totalEntries"));
    assert!(encoded.contains("remainingEntries"));
    assert!(encoded.contains("afterPath"));
    assert!(encoded.contains("afterKind"));
    assert!(encoded.contains("modified"));
    assert!(encoded.contains("deleted"));
    assert!(encoded.contains("untracked"));
    assert!(!encoded.contains("renamed"));
    assert!(!encoded.contains("copied"));
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
fn workspace_projection_output_schema_distinguishes_lineage_from_current_head() {
    let sandbox = Sandbox::new("workspace-revision-output-schema");
    let tools = sandbox.server().tool_router.list_all();
    for name in ["workspace.get", "workspace.list"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name.as_ref() == name)
            .unwrap();
        let schema = serde_json::to_string(tool.output_schema.as_ref().unwrap()).unwrap();
        assert!(
            schema.contains("sourceRevision"),
            "{name} omitted sourceRevision"
        );
        assert!(
            schema.contains("currentHeadRevision"),
            "{name} omitted currentHeadRevision"
        );
    }
}

#[test]
fn workspace_list_schema_makes_exact_source_digest_opt_in() {
    let sandbox = Sandbox::new("workspace-list-schema");
    let server = sandbox.server();
    let tool = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "workspace.list")
        .unwrap();
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    let required = schema
        .pointer("/required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!required
        .iter()
        .any(|value| value.as_str() == Some("includeSourceStateDigest")));
    assert!(!required
        .iter()
        .any(|value| value.as_str() == Some("cursor")));
    assert!(schema.pointer("/properties/cursor").is_some());
    assert!(schema
        .pointer("/$defs/RuntimeWorkspaceListCursor/properties/createdAtMs")
        .is_some());
    assert!(schema
        .pointer("/$defs/RuntimeWorkspaceListCursor/properties/workspaceId")
        .is_some());
    assert_eq!(
        schema
            .pointer("/properties/includeSourceStateDigest/default")
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn committed_operation_error_requires_exact_reattachment() {
    let error = RuntimeError::new(
        ordivon_runtime_core::RuntimeErrorCode::IoError,
        "result projection failed after Job admission",
        Some("bundlePath"),
        false,
    )
    .with_operation_id("job-committed-operation");
    let value = serde_json::to_value(ToolError::from(error)).unwrap();
    assert_eq!(
        value.pointer("/retryClass").and_then(Value::as_str),
        Some("reconcile_first")
    );
    assert_eq!(
        value.pointer("/commitState").and_then(Value::as_str),
        Some("committed")
    );
    assert_eq!(
        value.pointer("/operationId").and_then(Value::as_str),
        Some("job-committed-operation")
    );
    assert_eq!(
        value.pointer("/retryable").and_then(Value::as_bool),
        Some(false)
    );
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

#[test]
fn tool_catalog_digest_is_deterministic_and_discovery_visible() {
    let sandbox = Sandbox::new("catalog-digest");
    let server = sandbox.server();
    let first = server.tool_catalog_digest();
    let second = server.tool_catalog_digest();
    assert_eq!(first, second);
    assert!(first.starts_with("sha256:"));
    assert_eq!(first.len(), 71);

    let result = server.discovery_result();
    assert_eq!(result.ttl_ms, 0);
    assert_eq!(result.cache_scope, CacheScope::Private);
    assert_eq!(
        result
            .meta
            .as_ref()
            .and_then(|meta| meta.0.get("com.ordivon/runtime/toolCatalogDigest"))
            .and_then(serde_json::Value::as_str),
        Some(first.as_str())
    );
    assert_eq!(
        result.supported_versions,
        vec![
            ProtocolVersion::V_2026_07_28,
            ProtocolVersion::V_2025_11_25,
            ProtocolVersion::V_2025_06_18,
        ]
    );
    assert!(result.capabilities.tools.is_some());
    assert!(result.capabilities.extensions.is_none());
}

#[test]
fn runtime_describe_projects_agent_affordances_without_selecting_a_target() {
    let sandbox = Sandbox::new("runtime-describe");
    let server = sandbox.server();
    let capabilities = server.state.runtime.capabilities();
    let result = RuntimeDescribeResult::from_capabilities(
        capabilities,
        server.state.execution.global_limit,
        server.state.release.is_some(),
    );
    assert_eq!(result.schema_version, 1);
    assert_eq!(result.global_execution_limit, 4);
    assert_eq!(result.max_runtime_ms, 10_000);
    assert_eq!(result.max_output_bytes, 1024 * 1024);
    assert_eq!(result.allowed_executable_roots, vec!["/usr/bin"]);
    assert!(result.input_authorities.is_empty());
    assert_eq!(result.targets.len(), 2);
    assert!(!result.structured_release_configured);
    let linux = result
        .targets
        .iter()
        .find(|target| target.target == ExecutionTarget::LocalLinux)
        .unwrap();
    assert!(linux.configured);
    assert!(linux.available);
    assert!(linux.structured_plan);
    assert!(linux.immutable_inputs);
    assert!(linux.host_dependency_commitments);
    let windows = result
        .targets
        .iter()
        .find(|target| target.target == ExecutionTarget::WindowsNative)
        .unwrap();
    assert!(!windows.configured);
    assert!(!windows.available);
    assert!(!windows.structured_plan);
    assert!(!windows.immutable_inputs);
    assert!(!windows.host_dependency_commitments);

    let tool = server
        .tool_router
        .list_all()
        .into_iter()
        .find(|tool| tool.name.as_ref() == "runtime.describe")
        .unwrap();
    let input = serde_json::to_value(&tool.input_schema).unwrap();
    assert_eq!(
        input.pointer("/properties/schemaVersion/default"),
        Some(&serde_json::json!(1))
    );
    let required = input
        .pointer("/required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(required.is_empty());
    let output = serde_json::to_string(tool.output_schema.as_ref().unwrap()).unwrap();
    for expected in [
        "globalExecutionLimit",
        "maxRuntimeMs",
        "maxOutputBytes",
        "allowedExecutableRoots",
        "inputAuthorities",
        "executionProvider",
        "availabilityIssue",
        "structuredPlan",
        "immutableInputs",
        "hostDependencyCommitments",
    ] {
        assert!(
            output.contains(expected),
            "runtime.describe omitted {expected}"
        );
    }
}
