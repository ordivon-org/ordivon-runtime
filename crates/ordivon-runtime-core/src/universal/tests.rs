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

    remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-1".to_string(),
            force: true,
        },
    )
    .unwrap();
}

#[test]
fn workspace_diff_includes_staged_changes_and_workspace_listing_recovers_open_handles() {
    let sandbox = Sandbox::new("workspace-list");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-list";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path(workspace_id);
    fs::write(workspace.join("README.md"), "staged\n").unwrap();
    run_git(&workspace, ["add", "README.md"]);
    let diff = workspace_diff(
        &config,
        &WorkspaceDiffRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            max_bytes: 4096,
        },
    )
    .unwrap();
    assert!(diff.diff.contains("+staged"));

    let stale_id = "workspace-list-stale";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: stale_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    run_git(
        &source,
        [
            "worktree",
            "remove",
            "--force",
            config.workspace_path(stale_id).to_str().unwrap(),
        ],
    );

    let listed = list_workspace_records(&config, 10).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].workspace_id, workspace_id);
}

#[test]
fn workspace_source_state_digest_tracks_source_but_ignores_ignored_cache() {
    let sandbox = Sandbox::new("source-state");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    fs::write(source.join(".gitignore"), "*.cache\n").unwrap();
    run_git(&source, ["add", ".gitignore"]);
    run_git(&source, ["commit", "-qm", "ignore cache"]);
    let config = sandbox.config();
    let workspace_id = "workspace-source-state";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path(workspace_id);

    let baseline = workspace_source_state_digest(&config, workspace_id).unwrap();
    fs::write(workspace.join("compiler.cache"), "ignored bytes").unwrap();
    assert_eq!(
        workspace_source_state_digest(&config, workspace_id).unwrap(),
        baseline
    );

    fs::write(workspace.join("README.md"), "tracked change\n").unwrap();
    let unstaged = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(unstaged, baseline);
    run_git(&workspace, ["add", "README.md"]);
    let staged = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(staged, unstaged);
    run_git(&workspace, ["reset", "-q", "HEAD", "--", "README.md"]);
    assert_eq!(
        workspace_source_state_digest(&config, workspace_id).unwrap(),
        unstaged
    );

    fs::write(workspace.join("new-source.txt"), "first").unwrap();
    let untracked_first = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(untracked_first, unstaged);
    fs::write(workspace.join("new-source.txt"), "second").unwrap();
    let untracked_second = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(untracked_second, untracked_first);

    symlink("README.md", workspace.join("source-link")).unwrap();
    assert_ne!(
        workspace_source_state_digest(&config, workspace_id).unwrap(),
        untracked_second
    );
}

#[test]
fn workspace_source_state_cannot_be_blinded_by_git_index_flags() {
    let sandbox = Sandbox::new("source-state-index-flags");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-source-state-index-flags";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path(workspace_id);
    let baseline = workspace_source_state_digest(&config, workspace_id).unwrap();

    run_git(
        &workspace,
        ["update-index", "--assume-unchanged", "README.md"],
    );
    let assume_flag = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(assume_flag, baseline);
    fs::write(
        workspace.join("README.md"),
        "hidden assume-unchanged bytes\n",
    )
    .unwrap();
    let hidden_assume = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(hidden_assume, assume_flag);

    run_git(
        &workspace,
        ["update-index", "--no-assume-unchanged", "README.md"],
    );
    run_git(&workspace, ["reset", "--hard", "-q", "HEAD"]);
    let restored = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_eq!(restored, baseline);

    run_git(&workspace, ["update-index", "--skip-worktree", "README.md"]);
    let skip_flag = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(skip_flag, baseline);
    fs::write(workspace.join("README.md"), "hidden skip-worktree bytes\n").unwrap();
    let hidden_skip = workspace_source_state_digest(&config, workspace_id).unwrap();
    assert_ne!(hidden_skip, skip_flag);
}

#[test]
fn runner_rejects_workspace_source_drift_before_spawning_target() {
    let sandbox = Sandbox::new("runner-source-drift");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-runner-source-drift";
    let record = create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = PathBuf::from(&record.workspace_path);
    fs::write(
        workspace.join("effect.py"),
        "from pathlib import Path\nPath('effect-marker').write_text('spawned')\n",
    )
    .unwrap();
    let committed = workspace_source_state_digest(&config, workspace_id).unwrap();
    fs::write(workspace.join("README.md"), "drifted after admission\n").unwrap();

    let task_dir = sandbox.root.join("task-source-drift");
    fs::create_dir_all(&task_dir).unwrap();
    let executable = real_executable("/usr/bin/python3");
    let request = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        job_id: None,
        attempt_id: None,
        launch_token: None,
        unit_name: None,
        payload: None,
        inherit_host_environment: true,
        task_id: "task-source-drift".to_string(),
        workspace_id: workspace_id.to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: Some(committed),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["effect.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 2_000,
        stdout_limit_bytes: 1_024,
        stderr_limit_bytes: 1_024,
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();

    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert!(result.exit_code.is_none());
    assert_eq!(
        result.infrastructure_error_code.as_deref(),
        Some("WORKSPACE_STATE_MISMATCH")
    );
    assert!(result
        .infrastructure_error
        .as_deref()
        .is_some_and(|message| message.contains("WorkspaceStateMismatch")));
    assert!(!workspace.join("effect-marker").exists());
}

#[test]
fn workspace_close_rejects_dirty_state_unless_force_is_explicit() {
    let sandbox = Sandbox::new("safe-close");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-safe-close".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path("workspace-safe-close");
    fs::write(workspace.join("README.md"), "dirty tracked\n").unwrap();
    fs::write(workspace.join("untracked.txt"), "untracked\n").unwrap();
    let error = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-safe-close".to_string(),
            force: false,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::WorkspaceDirty);
    assert!(error.message.contains("README.md"));
    assert!(error.message.contains("untracked.txt"));
    assert!(workspace.exists());

    let closed = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-safe-close".to_string(),
            force: true,
        },
    )
    .unwrap();
    assert!(closed.removed);
    assert!(!workspace.exists());
}

#[test]
fn clean_workspace_close_succeeds_without_force() {
    let sandbox = Sandbox::new("clean-close");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-clean-close".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let closed = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-clean-close".to_string(),
            force: false,
        },
    )
    .unwrap();
    assert!(closed.removed);
}

#[test]
fn workspace_close_preserves_changed_head_and_is_idempotent() {
    let sandbox = Sandbox::new("close-rescue-ref");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-close-rescue";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path(workspace_id);
    fs::write(
        workspace.join("README.md"),
        "committed result
",
    )
    .unwrap();
    run_git(&workspace, ["add", "README.md"]);
    run_git(&workspace, ["commit", "-qm", "detached result"]);
    let final_head = git_text(&workspace, ["rev-parse", "HEAD"]);

    let first = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
        },
    )
    .unwrap();
    assert!(first.removed);
    assert!(!workspace.exists());
    assert_eq!(
        git_text(
            &source,
            ["rev-parse", "refs/ordivon/closed/workspace-close-rescue"]
        ),
        final_head
    );
    let tombstone: serde_json::Value =
        serde_json::from_slice(&fs::read(config.workspace_record_path(workspace_id)).unwrap())
            .unwrap();
    assert_eq!(tombstone["state"], "closed");
    assert_eq!(tombstone["finalHead"], final_head);
    assert_eq!(tombstone["removalResult"], "removed");

    let second = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
        },
    )
    .unwrap();
    assert!(!second.removed);
    assert_eq!(
        create_git_workspace(
            &config,
            &GitWorkspaceCreateRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: workspace_id.to_string(),
                source_repo: source.to_string_lossy().into_owned(),
                source_revision: "HEAD".to_string(),
            },
        )
        .unwrap_err()
        .code,
        UniversalExecErrorCode::WorkspaceExists
    );
}

#[test]
fn workspace_close_recovers_final_head_after_physical_removal() {
    let sandbox = Sandbox::new("close-crash-window");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-crash-window";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path(workspace_id);
    fs::write(workspace.join("README.md"), "detached result\n").unwrap();
    run_git(&workspace, ["add", "README.md"]);
    run_git(&workspace, ["commit", "-qm", "detached result"]);
    let final_head = git_text(&workspace, ["rev-parse", "HEAD"]);
    run_git(
        &source,
        [
            "update-ref",
            "refs/ordivon/closed/workspace-crash-window",
            &final_head,
        ],
    );
    run_git(
        &source,
        ["worktree", "remove", "--force", workspace.to_str().unwrap()],
    );

    let closed = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
        },
    )
    .unwrap();
    assert!(!closed.removed);
    let tombstone: serde_json::Value =
        serde_json::from_slice(&fs::read(config.workspace_record_path(workspace_id)).unwrap())
            .unwrap();
    assert_eq!(tombstone["state"], "closed");
    assert_eq!(tombstone["finalHead"], final_head);
    assert_eq!(tombstone["removalResult"], "already_missing");
}

#[test]
fn workspace_close_repairs_missing_directory_but_rejects_orphan_directory() {
    let sandbox = Sandbox::new("close-missing-directory");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-missing-directory";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path(workspace_id);
    run_git(
        &source,
        ["worktree", "remove", "--force", workspace.to_str().unwrap()],
    );
    let repaired = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
        },
    )
    .unwrap();
    assert!(!repaired.removed);
    let tombstone: serde_json::Value =
        serde_json::from_slice(&fs::read(config.workspace_record_path(workspace_id)).unwrap())
            .unwrap();
    assert_eq!(tombstone["state"], "closed");
    assert_eq!(tombstone["removalResult"], "already_missing");

    let orphan_id = "workspace-orphan-directory";
    config.ensure_store().unwrap();
    fs::create_dir_all(config.workspace_path(orphan_id)).unwrap();
    assert_eq!(
        remove_git_workspace(
            &config,
            &WorkspaceCloseRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: orphan_id.to_string(),
                force: false,
            },
        )
        .unwrap_err()
        .code,
        UniversalExecErrorCode::MetadataCorrupt
    );
}

#[test]
fn workspace_close_uses_live_git_identity_when_source_record_drifts() {
    let sandbox = Sandbox::new("close-source-drift");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-source-drift";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let record_path = config.workspace_record_path(workspace_id);
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    record["sourceRepo"] = serde_json::Value::String("/missing/legacy/source".to_string());
    write_json_atomic(&record_path, &record).unwrap();
    let closed = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
        },
    )
    .unwrap();
    assert!(closed.removed);
}

#[test]
fn mutation_failures_identify_the_exact_batch_item() {
    let sandbox = Sandbox::new("mutation-index");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-mutation-index".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let error = mutate_workspace(
        &config,
        &WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-mutation-index".to_string(),
            mutations: vec![
                WorkspaceMutation {
                    relative_path: "new.txt".to_string(),
                    mode: WorkspaceMutationMode::Write,
                    content: "ok".to_string(),
                    expected_digest: None,
                    expected_text: None,
                },
                WorkspaceMutation {
                    relative_path: "missing.txt".to_string(),
                    mode: WorkspaceMutationMode::ReplaceExact,
                    content: "replacement".to_string(),
                    expected_digest: None,
                    expected_text: Some("old".to_string()),
                },
            ],
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::WorkspacePathNotFound);
    assert_eq!(error.field.as_deref(), Some("mutations[1].relativePath"));
    assert!(!config
        .workspace_path("workspace-mutation-index")
        .join("new.txt")
        .exists());
}

#[test]
fn mutation_shape_errors_identify_the_exact_batch_item() {
    let base = WorkspaceMutateRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-shape-index".to_string(),
        mutations: vec![
            WorkspaceMutation {
                relative_path: "first.txt".to_string(),
                mode: WorkspaceMutationMode::Write,
                content: "first".to_string(),
                expected_digest: None,
                expected_text: None,
            },
            WorkspaceMutation {
                relative_path: "second.txt".to_string(),
                mode: WorkspaceMutationMode::ReplaceExact,
                content: "replacement".to_string(),
                expected_digest: None,
                expected_text: None,
            },
        ],
    };
    let missing_text = base.validate_shape().unwrap_err();
    assert_eq!(
        missing_text.field.as_deref(),
        Some("mutations[1].expectedText")
    );

    let invalid_digest = WorkspaceMutateRequest {
        mutations: vec![
            base.mutations[0].clone(),
            WorkspaceMutation {
                expected_digest: Some("not-a-digest".to_string()),
                expected_text: Some("old".to_string()),
                ..base.mutations[1].clone()
            },
        ],
        ..base.clone()
    }
    .validate_shape()
    .unwrap_err();
    assert_eq!(
        invalid_digest.field.as_deref(),
        Some("mutations[1].expectedDigest")
    );

    let duplicate = WorkspaceMutateRequest {
        mutations: vec![
            base.mutations[0].clone(),
            WorkspaceMutation {
                relative_path: "first.txt".to_string(),
                mode: WorkspaceMutationMode::Append,
                content: "again".to_string(),
                expected_digest: None,
                expected_text: None,
            },
        ],
        ..base
    }
    .validate_shape()
    .unwrap_err();
    assert_eq!(
        duplicate.field.as_deref(),
        Some("mutations[1].relativePath")
    );
}

#[test]
fn existing_mutation_requires_digest_before_exact_text_match() {
    let sandbox = Sandbox::new("mutation-preconditions");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-mutation-preconditions".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let read = read_workspace_text(
        &config,
        &WorkspaceReadRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-mutation-preconditions".to_string(),
            relative_path: "README.md".to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap();

    let without_digest = mutate_workspace(
        &config,
        &WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-mutation-preconditions".to_string(),
            mutations: vec![WorkspaceMutation {
                relative_path: "README.md".to_string(),
                mode: WorkspaceMutationMode::ReplaceExact,
                content: "replacement".to_string(),
                expected_digest: None,
                expected_text: Some("not-present".to_string()),
            }],
        },
    )
    .unwrap_err();
    assert_eq!(
        without_digest.field.as_deref(),
        Some("mutations[0].expectedDigest")
    );
    assert!(without_digest
        .message
        .contains("expectedDigest is required"));

    let wrong_text = mutate_workspace(
        &config,
        &WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-mutation-preconditions".to_string(),
            mutations: vec![WorkspaceMutation {
                relative_path: "README.md".to_string(),
                mode: WorkspaceMutationMode::ReplaceExact,
                content: "replacement".to_string(),
                expected_digest: Some(read.digest),
                expected_text: Some("not-present".to_string()),
            }],
        },
    )
    .unwrap_err();
    assert_eq!(
        wrong_text.field.as_deref(),
        Some("mutations[0].expectedText")
    );
    assert_eq!(
        fs::read_to_string(
            config
                .workspace_path("workspace-mutation-preconditions")
                .join("README.md")
        )
        .unwrap(),
        "baseline\n"
    );
}

#[test]
fn maximum_mutation_batch_preflights_atomically() {
    let sandbox = Sandbox::new("mutation-maximum-batch");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-mutation-maximum-batch";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();

    let mut mutations: Vec<_> = (0..31)
        .map(|index| WorkspaceMutation {
            relative_path: format!("generated-{index:02}.txt"),
            mode: WorkspaceMutationMode::Write,
            content: format!("generated-{index}\n"),
            expected_digest: None,
            expected_text: None,
        })
        .collect();
    mutations.push(WorkspaceMutation {
        relative_path: "README.md".to_string(),
        mode: WorkspaceMutationMode::ReplaceExact,
        content: "replacement\n".to_string(),
        expected_digest: None,
        expected_text: Some("baseline\n".to_string()),
    });
    assert_eq!(mutations.len(), MAX_WORKSPACE_MUTATIONS);

    let error = mutate_workspace(
        &config,
        &WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            mutations: mutations.clone(),
        },
    )
    .unwrap_err();
    assert_eq!(error.field.as_deref(), Some("mutations[31].expectedDigest"));
    for index in 0..31 {
        assert!(!config
            .workspace_path(workspace_id)
            .join(format!("generated-{index:02}.txt"))
            .exists());
    }
    assert_eq!(
        fs::read_to_string(config.workspace_path(workspace_id).join("README.md")).unwrap(),
        "baseline\n"
    );

    mutations.push(WorkspaceMutation {
        relative_path: "overflow.txt".to_string(),
        mode: WorkspaceMutationMode::Write,
        content: "overflow\n".to_string(),
        expected_digest: None,
        expected_text: None,
    });
    let overflow = WorkspaceMutateRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: workspace_id.to_string(),
        mutations,
    }
    .validate_shape()
    .unwrap_err();
    assert_eq!(overflow.field.as_deref(), Some("mutations"));
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
        "from pathlib import Path\nimport os\nimport sys\nPath('result.txt').write_text('created-by-tool')\nPath('home.txt').write_text(os.environ.get('HOME', ''))\nprint('stdout-0123456789')\nprint('stderr-0123456789', file=sys.stderr)\n",
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
        inherit_host_environment: true,
        task_id: "task-runner".to_string(),
        workspace_id: "workspace-runner".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["tool.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
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
    assert_eq!(
        fs::read_to_string(workspace.join("home.txt")).unwrap(),
        std::env::var("HOME").unwrap()
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
        inherit_host_environment: true,
        task_id: "task-timeout".to_string(),
        workspace_id: "workspace-timeout".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["tool.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
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
        UniversalExecErrorCode::WorkspacePathNotFound
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

fn git_text<'a>(path: &Path, args: impl IntoIterator<Item = &'a str>) -> String {
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
    String::from_utf8(output.stdout).unwrap().trim().to_string()
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

#[test]
fn workspace_patch_is_multi_file_multi_hunk_and_preflights_atomically() {
    let sandbox = Sandbox::new("patch-transaction");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-patch-transaction";
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let read = read_workspace_text(
        &config,
        &WorkspaceReadRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            relative_path: "README.md".to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap();

    let conflict = WorkspacePatchRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: workspace_id.to_string(),
        files: vec![
            WorkspaceFilePatch {
                relative_path: "README.md".to_string(),
                expected_digest: Some(read.digest.clone()),
                edits: vec![WorkspaceTextEdit {
                    range: WorkspaceTextRange {
                        start: WorkspaceTextPosition { line: 1, column: 0 },
                        end: WorkspaceTextPosition { line: 1, column: 4 },
                    },
                    expected_text: "base".to_string(),
                    replacement: "core".to_string(),
                }],
            },
            WorkspaceFilePatch {
                relative_path: "new.txt".to_string(),
                expected_digest: None,
                edits: vec![WorkspaceTextEdit {
                    range: WorkspaceTextRange {
                        start: WorkspaceTextPosition { line: 1, column: 0 },
                        end: WorkspaceTextPosition { line: 1, column: 0 },
                    },
                    expected_text: "not-empty".to_string(),
                    replacement: "created\n".to_string(),
                }],
            },
        ],
        max_diff_bytes: 16 * 1024,
    };
    let error = patch_workspace(&config, &conflict).unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::RevisionMismatch);
    assert_eq!(
        fs::read_to_string(config.workspace_path(workspace_id).join("README.md")).unwrap(),
        "baseline\n"
    );
    assert!(!config.workspace_path(workspace_id).join("new.txt").exists());

    let patched = patch_workspace(
        &config,
        &WorkspacePatchRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            files: vec![
                WorkspaceFilePatch {
                    relative_path: "README.md".to_string(),
                    expected_digest: Some(read.digest),
                    edits: vec![
                        WorkspaceTextEdit {
                            range: WorkspaceTextRange {
                                start: WorkspaceTextPosition { line: 1, column: 0 },
                                end: WorkspaceTextPosition { line: 1, column: 4 },
                            },
                            expected_text: "base".to_string(),
                            replacement: "core".to_string(),
                        },
                        WorkspaceTextEdit {
                            range: WorkspaceTextRange {
                                start: WorkspaceTextPosition { line: 1, column: 8 },
                                end: WorkspaceTextPosition { line: 1, column: 8 },
                            },
                            expected_text: String::new(),
                            replacement: "!".to_string(),
                        },
                    ],
                },
                WorkspaceFilePatch {
                    relative_path: "new.txt".to_string(),
                    expected_digest: None,
                    edits: vec![WorkspaceTextEdit {
                        range: WorkspaceTextRange {
                            start: WorkspaceTextPosition { line: 1, column: 0 },
                            end: WorkspaceTextPosition { line: 1, column: 0 },
                        },
                        expected_text: String::new(),
                        replacement: "created\n".to_string(),
                    }],
                },
            ],
            max_diff_bytes: 16 * 1024,
        },
    )
    .unwrap();
    assert_eq!(patched.files.len(), 2);
    assert!(!patched.diff_truncated);
    assert!(patched.diff.contains("README.md"));
    assert!(patched.diff.contains("new.txt"));
    assert_eq!(
        fs::read_to_string(config.workspace_path(workspace_id).join("README.md")).unwrap(),
        "coreline!\n"
    );
    assert_eq!(
        fs::read_to_string(config.workspace_path(workspace_id).join("new.txt")).unwrap(),
        "created\n"
    );
}
