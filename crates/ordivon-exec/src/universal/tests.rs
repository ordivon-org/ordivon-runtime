use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
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
            "ordivon-universal-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn config(&self) -> UniversalExecutorConfig {
        UniversalExecutorConfig {
            store_root: self.root.join("store"),
            workspace_root: None,
            workspace_uid: None,
            workspace_gid: None,
            runner_path: real_executable("/usr/bin/true"),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: 10_000,
            max_output_bytes: 1024 * 1024,
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn public_requests_reject_unknown_fields_and_path_escape() {
    let forged = serde_json::json!({
        "schemaVersion": 1,
        "workspaceId": "workspace-1",
        "relativePath": "README.md",
        "maxBytes": 1024,
        "command": "rm -rf /"
    });
    assert!(serde_json::from_value::<WorkspaceReadRequest>(forged).is_err());

    let request = WorkspaceReadRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        relative_path: "../outside".to_string(),
        max_bytes: 1024,
    };
    assert_eq!(
        request.validate_shape().unwrap_err().code,
        UniversalExecErrorCode::WorkspacePathDenied
    );
}

#[test]
fn workspace_round_trip_is_isolated_and_digest_guarded() {
    let sandbox = Sandbox::new("workspace");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let record = create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-1".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    assert_ne!(Path::new(&record.workspace_path), source);

    let read = read_workspace_text(
        &config,
        &WorkspaceReadRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-1".to_string(),
            relative_path: "README.md".to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap();
    assert_eq!(read.content, "baseline\n");

    let wrong = WorkspaceWriteRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        relative_path: "README.md".to_string(),
        content: "changed\n".to_string(),
        expected_digest: Some(format!("sha256:{}", "0".repeat(64))),
    };
    assert_eq!(
        write_workspace_text(&config, &wrong).unwrap_err().code,
        UniversalExecErrorCode::RevisionMismatch
    );

    let write = write_workspace_text(
        &config,
        &WorkspaceWriteRequest {
            expected_digest: Some(read.digest),
            ..wrong
        },
    )
    .unwrap();
    assert_ne!(write.before_digest, Some(write.after_digest.clone()));
    let diff = workspace_diff(
        &config,
        &WorkspaceDiffRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-1".to_string(),
            max_bytes: 4096,
        },
    )
    .unwrap();
    assert!(diff.diff.contains("-baseline"));
    assert!(diff.diff.contains("+changed"));
    assert!(diff.untracked_paths.is_empty());
    assert_eq!(
        fs::read_to_string(source.join("README.md")).unwrap(),
        "baseline\n"
    );

    let outside = sandbox.root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, Path::new(&record.workspace_path).join("escape")).unwrap();
    let escape = WorkspaceWriteRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-1".to_string(),
        relative_path: "escape/nested/payload".to_string(),
        content: "denied".to_string(),
        expected_digest: None,
    };
    assert_eq!(
        write_workspace_text(&config, &escape).unwrap_err().code,
        UniversalExecErrorCode::WorkspacePathDenied
    );
    assert!(!outside.join("nested").exists());

    remove_git_workspace(&config, "workspace-1").unwrap();
}

#[test]
fn runner_executes_model_authored_script_and_bounds_output() {
    let sandbox = Sandbox::new("runner");
    let workspace = sandbox.root.join("workspace");
    let task_dir = sandbox.root.join("task");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(
        workspace.join("tool.py"),
        "from pathlib import Path\nimport sys\nPath('result.txt').write_text('created-by-tool')\nprint('stdout-0123456789')\nprint('stderr-0123456789', file=sys.stderr)\n",
    )
    .unwrap();
    let executable = real_executable("/usr/bin/python3");
    let request = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        job_id: None,
        attempt_id: None,
        launch_token: None,
        unit_name: None,
        payload: None,
        task_id: "task-runner".to_string(),
        workspace_id: "workspace-runner".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["tool.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        timeout_ms: 2000,
        stdout_limit_bytes: 8,
        stderr_limit_bytes: 9,
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Completed);
    assert!(result.stdout.truncated);
    assert!(result.stderr.truncated);
    assert_eq!(result.stdout.retained_bytes, 8);
    assert_eq!(result.stderr.retained_bytes, 9);
    assert!(result.stdout.dropped_bytes > 0);
    assert_eq!(
        fs::read_to_string(workspace.join("result.txt")).unwrap(),
        "created-by-tool"
    );
}

#[test]
fn runner_timeout_is_a_durable_failed_result() {
    let sandbox = Sandbox::new("timeout");
    let workspace = sandbox.root.join("workspace");
    let task_dir = sandbox.root.join("task");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(workspace.join("tool.py"), "import time\ntime.sleep(5)\n").unwrap();
    let executable = real_executable("/usr/bin/python3");
    let request = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        job_id: None,
        attempt_id: None,
        launch_token: None,
        unit_name: None,
        payload: None,
        task_id: "task-timeout".to_string(),
        workspace_id: "workspace-timeout".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["tool.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        timeout_ms: 50,
        stdout_limit_bytes: 1024,
        stderr_limit_bytes: 1024,
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert!(result.timed_out);
}

#[test]
fn workspace_batch_mutation_preflights_before_writing() {
    let sandbox = Sandbox::new("batch");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-batch".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let read = read_workspace_text(
        &config,
        &WorkspaceReadRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-batch".to_string(),
            relative_path: "README.md".to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap();
    let bad = WorkspaceMutateRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-batch".to_string(),
        mutations: vec![
            WorkspaceMutation {
                relative_path: "README.md".to_string(),
                mode: WorkspaceMutationMode::Append,
                content: "first\n".to_string(),
                expected_digest: Some(read.digest.clone()),
                expected_text: None,
            },
            WorkspaceMutation {
                relative_path: "missing.txt".to_string(),
                mode: WorkspaceMutationMode::ReplaceExact,
                content: "replacement".to_string(),
                expected_digest: None,
                expected_text: Some("missing".to_string()),
            },
        ],
    };
    assert_eq!(
        mutate_workspace(&config, &bad).unwrap_err().code,
        UniversalExecErrorCode::WorkspaceNotFound
    );
    assert_eq!(
        fs::read_to_string(config.workspace_path("workspace-batch").join("README.md")).unwrap(),
        "baseline\n"
    );

    let result = mutate_workspace(
        &config,
        &WorkspaceMutateRequest {
            mutations: vec![
                WorkspaceMutation {
                    relative_path: "README.md".to_string(),
                    mode: WorkspaceMutationMode::Append,
                    content: "marker\n".to_string(),
                    expected_digest: Some(read.digest),
                    expected_text: None,
                },
                WorkspaceMutation {
                    relative_path: "tool.py".to_string(),
                    mode: WorkspaceMutationMode::Write,
                    content: "print('ok')\n".to_string(),
                    expected_digest: None,
                    expected_text: None,
                },
            ],
            ..bad
        },
    )
    .unwrap();
    assert_eq!(result.mutations.len(), 2);
    assert!(
        fs::read_to_string(config.workspace_path("workspace-batch").join("README.md"))
            .unwrap()
            .ends_with("marker\n")
    );
    assert_eq!(
        fs::read_to_string(config.workspace_path("workspace-batch").join("tool.py")).unwrap(),
        "print('ok')\n"
    );
}

#[test]
fn workspace_slice_returns_full_digest_and_utf8_safe_range() {
    let sandbox = Sandbox::new("slice");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-slice".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let slice = read_workspace_slice(
        &config,
        &WorkspaceReadSliceRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-slice".to_string(),
            relative_path: "README.md".to_string(),
            offset: 0,
            max_bytes: 4,
        },
    )
    .unwrap();
    assert_eq!(slice.content, "base");
    assert!(!slice.eof);
    assert_eq!(slice.file_byte_length, 9);
    assert!(slice.file_digest.starts_with("sha256:"));
}

fn init_git_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    run_git(path, ["init", "-q"]);
    run_git(path, ["config", "user.name", "Ordivon Test"]);
    run_git(path, ["config", "user.email", "ordivon@example.invalid"]);
    fs::write(path.join("README.md"), "baseline\n").unwrap();
    run_git(path, ["add", "README.md"]);
    run_git(path, ["commit", "-qm", "baseline"]);
}

fn run_git<'a>(path: &Path, args: impl IntoIterator<Item = &'a str>) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn real_executable(path: &str) -> PathBuf {
    fs::canonicalize(path).unwrap()
}
