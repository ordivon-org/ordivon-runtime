use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
fn explicit_root_workspace_identity_is_not_forbidden_by_policy() {
    let sandbox = Sandbox::new("explicit-root-workspace-owner");
    let mut config = sandbox.config();
    config.workspace_uid = Some(0);
    config.workspace_gid = Some(0);
    config.validate().unwrap();
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
fn workspace_open_missing_revision_is_classified_before_creation() {
    let sandbox = Sandbox::new("workspace-missing-revision");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-missing-revision";

    let error = create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "definitely-missing-revision".to_string(),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, UniversalExecErrorCode::RevisionNotFound);
    assert_eq!(error.field.as_deref(), Some("sourceRevision"));
    assert!(!config.workspace_path(workspace_id).exists());
    assert!(!config.workspace_record_path(workspace_id).exists());
}

#[test]
fn workspace_open_non_git_source_is_not_misclassified_as_revision() {
    let sandbox = Sandbox::new("workspace-non-git-source");
    let source = sandbox.root.join("source");
    fs::create_dir_all(&source).unwrap();
    let config = sandbox.config();
    let workspace_id = "workspace-non-git-source";

    let error = create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, UniversalExecErrorCode::ToolFailed);
    assert_eq!(error.field.as_deref(), Some("sourceRepo"));
    assert!(!config.workspace_path(workspace_id).exists());
    assert!(!config.workspace_record_path(workspace_id).exists());
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
    assert_eq!(diff.changed_paths, vec!["README.md"]);
    assert_eq!(diff.modified_paths, vec!["README.md"]);
    assert!(diff.added_paths.is_empty());
    assert!(diff.deleted_paths.is_empty());
    assert!(diff.renamed_paths.is_empty());
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
            expected_source_state_digest: None,
        },
    )
    .unwrap();
}

#[test]
fn workspace_read_allows_relative_parent_symlink_that_stays_beneath_root() {
    let sandbox = Sandbox::new("workspace-read-relative-parent-symlink");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-read-relative-parent-symlink";
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
    fs::create_dir_all(workspace.join("real")).unwrap();
    fs::write(workspace.join("real/value.txt"), b"inside\n").unwrap();
    symlink("real", workspace.join("alias")).unwrap();

    let read = read_workspace_text(
        &config,
        &WorkspaceReadRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            relative_path: "alias/value.txt".to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap();
    assert_eq!(read.content, "inside\n");

    let directory_error = read_workspace_text(
        &config,
        &WorkspaceReadRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            relative_path: "real".to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap_err();
    assert_eq!(
        directory_error.code,
        UniversalExecErrorCode::WorkspacePathDenied
    );
}

#[test]
fn workspace_content_rejects_final_symlink_and_preserves_bounded_read() {
    let sandbox = Sandbox::new("workspace-content-fd-boundaries");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-content-fd-boundaries";
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
    fs::create_dir_all(workspace.join("out")).unwrap();
    let png = [b"\x89PNG\r\n\x1a\n".as_slice(), &vec![b'x'; 2048]].concat();
    fs::write(workspace.join("out/large.png"), &png).unwrap();
    symlink("large.png", workspace.join("out/link.png")).unwrap();

    let symlink_error = read_workspace_content(
        &config,
        &WorkspaceContentRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            relative_path: "out/link.png".to_string(),
            expected_digest: sha256_bytes(&png),
            max_bytes: 4096,
        },
    )
    .unwrap_err();
    assert_eq!(
        symlink_error.code,
        UniversalExecErrorCode::WorkspacePathDenied
    );

    let size_error = read_workspace_content(
        &config,
        &WorkspaceContentRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            relative_path: "out/large.png".to_string(),
            expected_digest: sha256_bytes(&png),
            max_bytes: 1024,
        },
    )
    .unwrap_err();
    assert_eq!(size_error.code, UniversalExecErrorCode::OutputLimitExceeded);
    assert_eq!(size_error.field.as_deref(), Some("maxBytes"));
}

#[test]
fn workspace_content_parent_symlink_swap_never_reads_outside_root() {
    use std::os::unix::fs::symlink;
    use std::sync::{Arc, Barrier};
    use std::thread;

    let sandbox = Sandbox::new("workspace-content-parent-symlink-swap");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-content-parent-symlink-swap";
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
    let inside_dir = workspace.join("inside");
    fs::create_dir_all(&inside_dir).unwrap();
    let inside = b"\x89PNG\r\n\x1a\ninside";
    fs::write(inside_dir.join("view.png"), inside).unwrap();

    let outside_dir = sandbox.root.join("outside");
    fs::create_dir_all(&outside_dir).unwrap();
    let outside = b"\x89PNG\r\n\x1a\noutside";
    fs::write(outside_dir.join("view.png"), outside).unwrap();

    let alias = workspace.join("alias");
    symlink("inside", &alias).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let stop = Arc::new(AtomicBool::new(false));
    let writer_barrier = Arc::clone(&barrier);
    let writer_stop = Arc::clone(&stop);
    let writer_alias = alias.clone();
    let writer_outside = outside_dir.clone();
    let writer = thread::spawn(move || {
        writer_barrier.wait();
        let mut outside_turn = true;
        while !writer_stop.load(Ordering::Relaxed) {
            let replacement = writer_alias.with_extension("swap");
            let _ = fs::remove_file(&replacement);
            if outside_turn {
                let _ = symlink(&writer_outside, &replacement);
            } else {
                let _ = symlink("inside", &replacement);
            }
            if replacement.exists() {
                let _ = fs::rename(&replacement, &writer_alias);
            }
            outside_turn = !outside_turn;
        }
    });

    barrier.wait();
    let outside_digest = sha256_bytes(outside);
    let mut denied = 0_u64;
    let mut mismatches = 0_u64;
    let mut missing = 0_u64;
    for _ in 0..20_000 {
        match read_workspace_content(
            &config,
            &WorkspaceContentRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: workspace_id.to_string(),
                relative_path: "alias/view.png".to_string(),
                expected_digest: outside_digest.clone(),
                max_bytes: 1024,
            },
        ) {
            Ok(result) => {
                panic!(
                    "workspace.content escaped root under parent-symlink race: {} bytes digest {}",
                    result.metadata.byte_length, result.metadata.digest
                );
            }
            Err(error) => match error.code {
                UniversalExecErrorCode::WorkspacePathDenied => denied += 1,
                UniversalExecErrorCode::RevisionMismatch => mismatches += 1,
                UniversalExecErrorCode::WorkspacePathNotFound => missing += 1,
                other => panic!("unexpected race error {other:?}: {}", error.message),
            },
        }
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    eprintln!("parent-symlink-race denied={denied} mismatches={mismatches} missing={missing}");
    assert!(denied + mismatches + missing > 0);
}

#[test]
fn workspace_content_reads_exact_verified_png_bytes() {
    let sandbox = Sandbox::new("workspace-content-png");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-content-png".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path("workspace-content-png");
    fs::create_dir_all(workspace.join("out")).unwrap();
    let png = b"\x89PNG\r\n\x1a\nverified-pixels";
    fs::write(workspace.join("out/view.png"), png).unwrap();
    let expected_digest = sha256_bytes(png);

    let read = read_workspace_content(
        &config,
        &WorkspaceContentRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-content-png".to_string(),
            relative_path: "out/view.png".to_string(),
            expected_digest: expected_digest.clone(),
            max_bytes: 1024,
        },
    )
    .unwrap();

    assert_eq!(read.bytes, png);
    assert_eq!(read.metadata.digest, expected_digest);
    assert_eq!(read.metadata.media_type, "image/png");
    assert_eq!(read.metadata.byte_length, png.len() as u64);
}

#[test]
fn workspace_read_never_follows_replaced_symlink_after_fd_binding() {
    let sandbox = Sandbox::new("workspace-read-toctou-probe");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-read-toctou-probe";
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
    let out = workspace.join("out");
    fs::create_dir_all(&out).unwrap();
    let target = out.join("view.txt");
    let safe_swap = out.join("safe.swap");
    let link_swap = out.join("link.swap");
    let outside = sandbox.root.join("outside.txt");
    let safe = b"workspace-safe\n";
    let escaped = b"outside-secret\n";
    fs::write(&target, safe).unwrap();
    fs::write(&outside, escaped).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let attacker_stop = Arc::clone(&stop);
    let attacker_target = target.clone();
    let attacker_safe_swap = safe_swap.clone();
    let attacker_link_swap = link_swap.clone();
    let attacker_outside = outside.clone();
    let attacker = thread::spawn(move || {
        while !attacker_stop.load(Ordering::Relaxed) {
            fs::write(&attacker_safe_swap, safe).unwrap();
            fs::rename(&attacker_safe_swap, &attacker_target).unwrap();
            symlink(&attacker_outside, &attacker_link_swap).unwrap();
            fs::rename(&attacker_link_swap, &attacker_target).unwrap();
        }
    });

    let mut observed_escape = None;
    for iteration in 0..20_000_u64 {
        let result = read_workspace_text(
            &config,
            &WorkspaceReadRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: workspace_id.to_string(),
                relative_path: "out/view.txt".to_string(),
                max_bytes: 1024,
            },
        );
        if let Ok(read) = result {
            if read.content.as_bytes() == escaped {
                observed_escape = Some(iteration);
                break;
            }
        }
        if iteration % 128 == 0 {
            thread::yield_now();
        }
    }
    stop.store(true, Ordering::Relaxed);
    attacker.join().unwrap();

    assert_eq!(
        observed_escape, None,
        "workspace.read returned bytes from outside the Workspace after FD binding"
    );
}

#[test]
fn workspace_content_never_follows_replaced_symlink_after_fd_binding() {
    let sandbox = Sandbox::new("workspace-content-toctou-probe");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-content-toctou-probe";
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
    let out = workspace.join("out");
    fs::create_dir_all(&out).unwrap();
    let target = out.join("view.png");
    let safe_swap = out.join("safe.swap");
    let link_swap = out.join("link.swap");
    let outside = sandbox.root.join("outside.png");
    let safe = b"\x89PNG\r\n\x1a\nworkspace-safe";
    let escaped = b"\x89PNG\r\n\x1a\noutside-secret";
    fs::write(&target, safe).unwrap();
    fs::write(&outside, escaped).unwrap();
    let escaped_digest = sha256_bytes(escaped);

    let stop = Arc::new(AtomicBool::new(false));
    let attacker_stop = Arc::clone(&stop);
    let attacker_target = target.clone();
    let attacker_safe_swap = safe_swap.clone();
    let attacker_link_swap = link_swap.clone();
    let attacker_outside = outside.clone();
    let attacker = thread::spawn(move || {
        while !attacker_stop.load(Ordering::Relaxed) {
            fs::write(&attacker_safe_swap, safe).unwrap();
            fs::rename(&attacker_safe_swap, &attacker_target).unwrap();
            symlink(&attacker_outside, &attacker_link_swap).unwrap();
            fs::rename(&attacker_link_swap, &attacker_target).unwrap();
        }
    });

    let mut observed_escape = None;
    for iteration in 0..20_000_u64 {
        let result = read_workspace_content(
            &config,
            &WorkspaceContentRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: workspace_id.to_string(),
                relative_path: "out/view.png".to_string(),
                expected_digest: escaped_digest.clone(),
                max_bytes: 1024,
            },
        );
        if let Ok(read) = result {
            if read.bytes == escaped {
                observed_escape = Some(iteration);
                break;
            }
        }
        if iteration % 128 == 0 {
            thread::yield_now();
        }
    }
    stop.store(true, Ordering::Relaxed);
    attacker.join().unwrap();

    assert_eq!(
        observed_escape, None,
        "workspace.content returned bytes from outside the Workspace after FD binding"
    );
}

#[test]
fn workspace_content_fails_closed_on_digest_drift_and_forged_image_type() {
    let sandbox = Sandbox::new("workspace-content-drift");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-content-drift".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path("workspace-content-drift");
    fs::create_dir_all(workspace.join("out")).unwrap();
    let original = b"\x89PNG\r\n\x1a\nfirst";
    let path = workspace.join("out/view.png");
    fs::write(&path, original).unwrap();
    let expected_digest = sha256_bytes(original);
    fs::write(&path, b"\x89PNG\r\n\x1a\nsecond").unwrap();

    let drift = read_workspace_content(
        &config,
        &WorkspaceContentRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-content-drift".to_string(),
            relative_path: "out/view.png".to_string(),
            expected_digest,
            max_bytes: 1024,
        },
    )
    .unwrap_err();
    assert_eq!(drift.code, UniversalExecErrorCode::RevisionMismatch);
    assert_eq!(drift.field.as_deref(), Some("expectedDigest"));

    fs::write(&path, b"not-a-png").unwrap();
    let forged_bytes = fs::read(&path).unwrap();
    let forged = read_workspace_content(
        &config,
        &WorkspaceContentRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-content-drift".to_string(),
            relative_path: "out/view.png".to_string(),
            expected_digest: sha256_bytes(&forged_bytes),
            max_bytes: 1024,
        },
    )
    .unwrap_err();
    assert_eq!(forged.code, UniversalExecErrorCode::InvalidRequest);
    assert_eq!(forged.field.as_deref(), Some("relativePath"));
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
    assert_eq!(diff.changed_paths, vec!["README.md"]);
    assert_eq!(diff.modified_paths, vec!["README.md"]);

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
fn workspace_diff_keeps_complete_path_sets_beyond_legacy_caps() {
    let sandbox = Sandbox::new("workspace-large-path-set");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    for index in 0..520 {
        fs::write(source.join(format!("tracked-{index:03}.txt")), "base\n").unwrap();
    }
    run_git(&source, ["add", "."]);
    run_git(&source, ["commit", "-qm", "add large path fixture"]);
    let config = sandbox.config();
    let workspace_id = "workspace-large-path-set";
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
    for index in 0..520 {
        fs::write(
            workspace.join(format!("tracked-{index:03}.txt")),
            format!("changed-{index}\n"),
        )
        .unwrap();
    }
    for index in 0..300 {
        fs::write(
            workspace.join(format!("untracked-{index:03}.txt")),
            format!("new-{index}\n"),
        )
        .unwrap();
    }

    let diff = workspace_diff(
        &config,
        &WorkspaceDiffRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            max_bytes: 1024,
        },
    )
    .unwrap();
    assert!(diff.truncated);
    assert_eq!(diff.changed_paths.len(), 520);
    assert_eq!(diff.modified_paths.len(), 520);
    assert_eq!(diff.untracked_paths.len(), 300);
}

#[test]
fn workspace_diff_reports_structured_modified_added_deleted_and_renamed_paths() {
    let sandbox = Sandbox::new("workspace-structured-diff");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    fs::write(source.join("old-name.txt"), "rename me\n").unwrap();
    fs::write(source.join("delete-me.txt"), "delete me\n").unwrap();
    run_git(&source, ["add", "old-name.txt", "delete-me.txt"]);
    run_git(&source, ["commit", "-qm", "add diff fixtures"]);
    let config = sandbox.config();
    let workspace_id = "workspace-structured-diff";
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
    fs::write(workspace.join("README.md"), "modified\n").unwrap();
    fs::write(workspace.join("added.txt"), "new unique bytes\n").unwrap();
    run_git(&workspace, ["add", "added.txt"]);
    fs::remove_file(workspace.join("delete-me.txt")).unwrap();
    run_git(&workspace, ["mv", "old-name.txt", "renamed.txt"]);
    fs::write(workspace.join("untracked.txt"), "untracked\n").unwrap();

    let diff = workspace_diff(
        &config,
        &WorkspaceDiffRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            max_bytes: 64 * 1024,
        },
    )
    .unwrap();
    assert_eq!(
        diff.changed_paths,
        vec![
            "README.md",
            "added.txt",
            "delete-me.txt",
            "old-name.txt",
            "renamed.txt",
        ]
    );
    assert_eq!(diff.modified_paths, vec!["README.md"]);
    assert_eq!(diff.added_paths, vec!["added.txt"]);
    assert_eq!(diff.deleted_paths, vec!["delete-me.txt"]);
    assert_eq!(
        diff.renamed_paths,
        vec![WorkspaceRenamedPath {
            from_path: "old-name.txt".to_string(),
            to_path: "renamed.txt".to_string(),
        }]
    );
    assert_eq!(diff.untracked_paths, vec!["untracked.txt"]);
}

#[test]
fn workspace_changes_pages_across_tracked_and_untracked_without_loss() {
    let sandbox = Sandbox::new("workspace-changes-pages");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    for index in 0..150 {
        fs::write(
            source.join(format!("tracked-{index:03}.txt")),
            format!("before-{index}\n"),
        )
        .unwrap();
    }
    run_git(&source, ["add", "."]);
    run_git(&source, ["commit", "-qm", "add tracked page fixtures"]);
    let config = sandbox.config();
    let workspace_id = "workspace-changes-pages";
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
    for index in 0..150 {
        fs::write(
            workspace.join(format!("tracked-{index:03}.txt")),
            format!("after-{index}\n"),
        )
        .unwrap();
    }
    for index in 0..90 {
        fs::write(
            workspace.join(format!("untracked-{index:03}.txt")),
            format!("new-{index}\n"),
        )
        .unwrap();
    }

    let mut cursor = None;
    let mut observed = Vec::new();
    let mut change_set_digest = None;
    for _ in 0..32 {
        let page = workspace_changes_page(
            &config,
            &WorkspaceChangePageRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: workspace_id.to_string(),
                limit: 17,
                max_bytes: 4096,
                cursor: cursor.clone(),
            },
        )
        .unwrap();
        assert!(page.entries.len() <= 17);
        assert!(page.entry_bytes <= 4096);
        assert_eq!(page.total_entries, 240);
        assert_eq!(
            page.remaining_entries + page.entries.len() as u64,
            240 - observed.len() as u64
        );
        if let Some(expected) = &change_set_digest {
            assert_eq!(&page.change_set_digest, expected);
        } else {
            change_set_digest = Some(page.change_set_digest.clone());
        }
        observed.extend(page.entries);
        if page.complete {
            assert!(page.next_cursor.is_none());
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(observed.len(), 240);
    let modified: BTreeSet<_> = observed
        .iter()
        .filter(|entry| entry.kind == WorkspaceChangeKind::Modified)
        .map(|entry| entry.path.clone())
        .collect();
    let untracked: BTreeSet<_> = observed
        .iter()
        .filter(|entry| entry.kind == WorkspaceChangeKind::Untracked)
        .map(|entry| entry.path.clone())
        .collect();
    assert_eq!(modified.len(), 150);
    assert_eq!(untracked.len(), 90);
    assert!(modified.contains("tracked-000.txt"));
    assert!(modified.contains("tracked-149.txt"));
    assert!(untracked.contains("untracked-000.txt"));
    assert!(untracked.contains("untracked-089.txt"));
}

#[test]
fn workspace_changes_cursor_fails_closed_after_workspace_drift() {
    let sandbox = Sandbox::new("workspace-changes-drift");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-changes-drift";
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
    fs::write(workspace.join("one.txt"), "one\n").unwrap();
    fs::write(workspace.join("two.txt"), "two\n").unwrap();
    let first = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 1,
            max_bytes: 4096,
            cursor: None,
        },
    )
    .unwrap();
    let cursor = first.next_cursor.expect("continuation cursor");
    fs::write(workspace.join("three.txt"), "three\n").unwrap();
    let error = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 1,
            max_bytes: 4096,
            cursor: Some(cursor),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::WorkspaceStateMismatch);
    assert_eq!(error.field.as_deref(), Some("cursor.changeSetDigest"));
}

#[test]
fn workspace_changes_represents_staged_rename_as_delete_plus_add() {
    let sandbox = Sandbox::new("workspace-changes-atomic-rename");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    fs::write(source.join("old-name.txt"), "rename body\n").unwrap();
    run_git(&source, ["add", "."]);
    run_git(&source, ["commit", "-qm", "add rename fixture"]);
    let config = sandbox.config();
    let workspace_id = "workspace-changes-atomic-rename";
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
    fs::rename(
        workspace.join("old-name.txt"),
        workspace.join("new-name.txt"),
    )
    .unwrap();
    run_git(&workspace, ["add", "-A"]);
    let page = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 8,
            max_bytes: 4096,
            cursor: None,
        },
    )
    .unwrap();
    assert!(page.complete);
    assert_eq!(page.total_entries, 2);
    assert_eq!(page.remaining_entries, 0);
    let observed: BTreeSet<_> = page
        .entries
        .into_iter()
        .map(|entry| (entry.kind, entry.path))
        .collect();
    assert_eq!(
        observed,
        BTreeSet::from([
            (WorkspaceChangeKind::Added, "new-name.txt".to_string()),
            (WorkspaceChangeKind::Deleted, "old-name.txt".to_string()),
        ])
    );
}

#[test]
fn workspace_changes_rejects_forged_after_path_kind() {
    let sandbox = Sandbox::new("workspace-changes-forged-cursor");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-changes-forged-cursor";
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
    fs::write(workspace.join("one.txt"), "one\n").unwrap();
    fs::write(workspace.join("two.txt"), "two\n").unwrap();

    let first = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 1,
            max_bytes: 4096,
            cursor: None,
        },
    )
    .unwrap();
    let mut cursor = first.next_cursor.expect("continuation cursor");
    cursor.after_path = "definitely-not-a-change-member.txt".to_string();
    cursor.after_kind = WorkspaceChangeKind::Modified;

    let error = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 1,
            max_bytes: 4096,
            cursor: Some(cursor),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::WorkspaceStateMismatch);
    assert_eq!(error.field.as_deref(), Some("cursor.afterPath"));
}

#[test]
fn workspace_changes_cursor_survives_content_only_drift_with_same_change_set() {
    let sandbox = Sandbox::new("workspace-changes-content-only-drift");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-changes-content-only-drift";
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
    fs::write(workspace.join("README.md"), "first modified body\n").unwrap();
    fs::write(workspace.join("untracked.txt"), "first untracked body\n").unwrap();

    let first = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 1,
            max_bytes: 4096,
            cursor: None,
        },
    )
    .unwrap();
    let cursor = first.next_cursor.clone().expect("continuation cursor");

    fs::write(
        workspace.join("README.md"),
        "second modified body with different bytes\n",
    )
    .unwrap();
    fs::write(
        workspace.join("untracked.txt"),
        "second untracked body with different bytes\n",
    )
    .unwrap();

    let second = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 1,
            max_bytes: 4096,
            cursor: Some(cursor.clone()),
        },
    )
    .unwrap();
    assert_eq!(second.change_set_digest, cursor.change_set_digest);
    assert_eq!(second.entries.len(), 1);
    assert!(second.complete);
    assert!(second.next_cursor.is_none());
}

#[test]
fn workspace_changes_rejects_an_entry_larger_than_page_byte_budget() {
    let sandbox = Sandbox::new("workspace-changes-entry-budget");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-changes-entry-budget";
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
    fs::write(workspace.join(format!("{}.txt", "x".repeat(120))), "x\n").unwrap();
    let error = workspace_changes_page(
        &config,
        &WorkspaceChangePageRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            limit: 8,
            max_bytes: 16,
            cursor: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::OutputLimitExceeded);
    assert_eq!(error.field.as_deref(), Some("maxBytes"));
}

#[test]
fn current_workspace_inventory_read_does_not_create_missing_store() {
    let root = std::env::temp_dir().join(format!(
        "ordivon-list-readonly-missing-store-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let config = UniversalExecutorConfig {
        store_root: root.join("runtime"),
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
        runner_path: PathBuf::from("/usr/bin/true"),
        allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
        max_runtime_ms: 60_000,
        max_output_bytes: 1_048_576,
    };
    assert!(!config.store_root.exists());
    let error = list_open_workspace_record_inventory(&config).unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::IoError);
    assert!(!config.store_root.exists());
}

#[test]
fn workspace_head_and_dirty_probe_combines_detached_head_and_worktree_state() {
    let sandbox = Sandbox::new("workspace-head-dirty-probe");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let expected = git_text(&source, ["rev-parse", "HEAD"]);
    let config = sandbox.config();
    let workspace_id = "workspace-head-dirty-probe";
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
    let (head, dirty) = workspace_head_and_dirty_at(&workspace).unwrap();
    assert_eq!(head, expected);
    assert!(!dirty);

    fs::write(workspace.join("untracked.txt"), "visible").unwrap();
    let (head, dirty) = workspace_head_and_dirty_at(&workspace).unwrap();
    assert_eq!(head, expected);
    assert!(dirty);
}

#[test]
fn workspace_dirty_probe_is_lightweight_and_respects_git_ignores() {
    let sandbox = Sandbox::new("workspace-dirty-probe");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    fs::write(source.join(".gitignore"), "*.cache\n").unwrap();
    run_git(&source, ["add", ".gitignore"]);
    run_git(&source, ["commit", "-qm", "ignore cache"]);
    let config = sandbox.config();
    let workspace_id = "workspace-dirty-probe";
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
    assert!(!workspace_is_dirty(&config, workspace_id).unwrap());
    fs::write(workspace.join("compiler.cache"), "ignored").unwrap();
    assert!(!workspace_is_dirty(&config, workspace_id).unwrap());
    fs::write(workspace.join("untracked.txt"), "visible").unwrap();
    assert!(workspace_is_dirty(&config, workspace_id).unwrap());
    fs::remove_file(workspace.join("untracked.txt")).unwrap();
    fs::write(workspace.join("README.md"), "tracked change\n").unwrap();
    assert!(workspace_is_dirty(&config, workspace_id).unwrap());
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
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["effect.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 2_000,
        stdout_limit_bytes: 1_024,
        stderr_limit_bytes: 1_024,
        host_dependencies: Vec::new(),
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
fn runner_projects_private_build_target_through_stable_inherited_fd() {
    let sandbox = Sandbox::new("runner-stable-build-target");
    let workspace = sandbox.root.join("workspace-stable-build-target");
    let backing = sandbox.root.join("private-target-backing");
    let task_dir = sandbox.root.join("task-stable-build-target");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&backing).unwrap();
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(
        workspace.join("probe.py"),
        "import os\nfrom pathlib import Path\nroot=Path(os.environ['CARGO_TARGET_DIR'])\n(root/'probe.txt').write_text('PRIVATE')\nprint(os.environ['CARGO_TARGET_DIR'])\nprint((root/'probe.txt').read_text())\n",
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
        task_id: "task-stable-build-target".to_string(),
        workspace_id: "workspace-stable-build-target".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: Some(backing.to_string_lossy().into_owned()),
        input_presentation_root: None,
        input_commitments: Vec::new(),
        host_dependencies: Vec::new(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["probe.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::from([(
            "CARGO_TARGET_DIR".to_string(),
            "/proc/self/fd/198".to_string(),
        )]),
        steps: Vec::new(),
        timeout_ms: 2_000,
        stdout_limit_bytes: 1_024,
        stderr_limit_bytes: 1_024,
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();

    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Completed);
    assert_eq!(
        fs::read_to_string(backing.join("probe.txt")).unwrap(),
        "PRIVATE"
    );
    assert_eq!(
        fs::read_to_string(task_dir.join("stdout.log")).unwrap(),
        "/proc/self/fd/198\nPRIVATE\n"
    );
}

#[test]
fn runner_rejects_host_dependency_drift_before_spawning_target() {
    let sandbox = Sandbox::new("runner-host-dependency-drift");
    let workspace = sandbox.root.join("workspace-host-dependency-drift");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(
        workspace.join("effect.py"),
        "from pathlib import Path\nPath('effect-marker').write_text('spawned')\n",
    )
    .unwrap();
    let dependency = sandbox.root.join("host-dependency.so");
    fs::write(&dependency, b"CHANGED").unwrap();
    let task_dir = sandbox.root.join("task-host-dependency-drift");
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
        task_id: "task-host-dependency-drift".to_string(),
        workspace_id: "workspace-host-dependency-drift".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        host_dependencies: vec![RunnerHostDependencyCommitment {
            path: dependency.to_string_lossy().into_owned(),
            digest: sha256_bytes(b"ORIGINAL"),
        }],
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
        Some("INPUT_STATE_MISMATCH")
    );
    assert!(result
        .infrastructure_error
        .as_deref()
        .is_some_and(|message| message.contains("Host Dependency")));
    assert!(!workspace.join("effect-marker").exists());
}

#[test]
fn runner_fails_when_host_dependency_drifts_after_target_start() {
    let sandbox = Sandbox::new("runner-host-dependency-runtime-drift");
    let workspace = sandbox.root.join("workspace-host-dependency-runtime-drift");
    fs::create_dir_all(&workspace).unwrap();
    let dependency = sandbox.root.join("runtime-dependency.txt");
    fs::write(&dependency, b"RUNTIME_V1\n").unwrap();
    let gate = sandbox.root.join("runtime-gate");
    let script = workspace.join("delayed-read.py");
    fs::write(
        &script,
        format!(
            "import pathlib,time\nprint('READY', flush=True)\ngate=pathlib.Path({gate:?})\nfor _ in range(500):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint(pathlib.Path({dependency:?}).read_text().strip(), flush=True)\n",
            gate = gate.to_string_lossy(),
            dependency = dependency.to_string_lossy(),
        ),
    )
    .unwrap();
    let task_dir = sandbox.root.join("task-host-dependency-runtime-drift");
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
        task_id: "task-host-dependency-runtime-drift".to_string(),
        workspace_id: "workspace-host-dependency-runtime-drift".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        host_dependencies: vec![RunnerHostDependencyCommitment {
            path: dependency.to_string_lossy().into_owned(),
            digest: sha256_file(&dependency).unwrap(),
        }],
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec![script.to_string_lossy().into_owned()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 5_000,
        stdout_limit_bytes: 4_096,
        stderr_limit_bytes: 4_096,
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    let runner_task_dir = task_dir.clone();
    let runner = thread::spawn(move || run_task_runner(&runner_task_dir));
    let stdout = task_dir.join("stdout.log");
    let mut ready = false;
    for _ in 0..500 {
        if fs::read_to_string(&stdout)
            .ok()
            .is_some_and(|text| text.contains("READY\n"))
        {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        ready,
        "target never reached the post-validation READY point"
    );
    let replacement = dependency.with_extension("txt.new");
    fs::write(&replacement, b"RUNTIME_V2\n").unwrap();
    fs::rename(&replacement, &dependency).unwrap();
    fs::write(&gate, b"go").unwrap();
    runner.join().unwrap().unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert_eq!(
        result.infrastructure_error_code.as_deref(),
        Some("HOST_DEPENDENCY_RUNTIME_DRIFT")
    );
    assert!(result
        .infrastructure_error
        .as_deref()
        .is_some_and(|message| message.contains("Host Dependency")));
}

#[test]
fn runner_preserves_shebang_path_semantics_but_fails_on_runtime_executable_drift() {
    let sandbox = Sandbox::new("runner-script-runtime-drift");
    let workspace = sandbox.root.join("workspace-script-runtime-drift");
    fs::create_dir_all(&workspace).unwrap();
    let gate = sandbox.root.join("script-gate");
    let executable = workspace.join("agent-script");
    fs::write(
        &executable,
        format!(
            "#!/usr/bin/python3\nimport pathlib,time\nprint('FILE='+__file__, flush=True)\ngate=pathlib.Path({gate:?})\nfor _ in range(500):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('SCRIPT_V1_DONE', flush=True)\n",
            gate = gate.to_string_lossy(),
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    let expected_digest = sha256_file(&executable).unwrap();
    let task_dir = sandbox.root.join("task-script-runtime-drift");
    fs::create_dir_all(&task_dir).unwrap();
    let request = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        job_id: None,
        attempt_id: None,
        launch_token: None,
        unit_name: None,
        payload: None,
        inherit_host_environment: true,
        task_id: "task-script-runtime-drift".to_string(),
        workspace_id: "workspace-script-runtime-drift".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        host_dependencies: Vec::new(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: expected_digest,
        args: Vec::new(),
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 5_000,
        stdout_limit_bytes: 4_096,
        stderr_limit_bytes: 4_096,
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    let runner_task_dir = task_dir.clone();
    let runner = thread::spawn(move || run_task_runner(&runner_task_dir));
    let stdout = task_dir.join("stdout.log");
    let expected_file_line = format!("FILE={}\n", executable.display());
    let mut ready = false;
    for _ in 0..500 {
        if fs::read_to_string(&stdout)
            .ok()
            .is_some_and(|text| text.contains(&expected_file_line))
        {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        ready,
        "shebang target did not preserve its original file path"
    );
    let replacement = executable.with_extension("new");
    fs::write(&replacement, "#!/bin/sh\nprintf 'SCRIPT_V2\\n'\n").unwrap();
    let mut permissions = fs::metadata(&replacement).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&replacement, permissions).unwrap();
    fs::rename(&replacement, &executable).unwrap();
    fs::write(&gate, b"go").unwrap();
    runner.join().unwrap().unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert_eq!(
        result.infrastructure_error_code.as_deref(),
        Some("EXECUTABLE_RUNTIME_DRIFT")
    );
    assert!(fs::read_to_string(stdout)
        .unwrap()
        .contains(&expected_file_line));
}

#[test]
fn runner_rejects_immutable_input_drift_before_spawning_target() {
    let sandbox = Sandbox::new("runner-input-drift");
    let workspace = sandbox.root.join("workspace-input-drift");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(
        workspace.join("effect.py"),
        "from pathlib import Path\nPath('effect-marker').write_text('spawned')\n",
    )
    .unwrap();
    let input_view = sandbox.root.join("input-view");
    fs::create_dir_all(&input_view).unwrap();
    let presented = input_view.join("presented-input.bin");
    fs::write(&presented, b"BADD").unwrap();

    let task_dir = sandbox.root.join("task-input-drift");
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
        task_id: "task-input-drift".to_string(),
        workspace_id: "workspace-input-drift".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: Some(input_view.to_string_lossy().into_owned()),
        input_commitments: vec![RunnerInputCommitment {
            presentation_path: presented.to_string_lossy().into_owned(),
            digest: sha256_bytes(b"GOOD"),
            byte_length: 4,
        }],
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["effect.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 2_000,
        stdout_limit_bytes: 1_024,
        stderr_limit_bytes: 1_024,
        host_dependencies: Vec::new(),
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();

    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert!(result.exit_code.is_none());
    assert_eq!(
        result.infrastructure_error_code.as_deref(),
        Some("INPUT_STATE_MISMATCH")
    );
    assert!(result
        .infrastructure_error
        .as_deref()
        .is_some_and(|message| message.contains("digest changed after admission")));
    assert!(!workspace.join("effect-marker").exists());
}

#[test]
fn runner_rejects_undeclared_input_directory_before_spawning_target() {
    let sandbox = Sandbox::new("runner-input-extra-directory");
    let workspace = sandbox.root.join("workspace-input-extra-directory");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(
        workspace.join("effect.py"),
        "from pathlib import Path\nPath('effect-marker').write_text('spawned')\n",
    )
    .unwrap();
    let input_view = sandbox.root.join("input-view-extra-directory");
    fs::create_dir_all(input_view.join("state")).unwrap();
    fs::create_dir_all(input_view.join("undeclared-empty")).unwrap();
    let presented = input_view.join("state/declared.bin");
    fs::write(&presented, b"GOOD").unwrap();

    let task_dir = sandbox.root.join("task-input-extra-directory");
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
        task_id: "task-input-extra-directory".to_string(),
        workspace_id: "workspace-input-extra-directory".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: Some(input_view.to_string_lossy().into_owned()),
        input_commitments: vec![RunnerInputCommitment {
            presentation_path: presented.to_string_lossy().into_owned(),
            digest: sha256_bytes(b"GOOD"),
            byte_length: 4,
        }],
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["effect.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 2_000,
        stdout_limit_bytes: 1_024,
        stderr_limit_bytes: 1_024,
        host_dependencies: Vec::new(),
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();

    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert_eq!(
        result.infrastructure_error_code.as_deref(),
        Some("INPUT_STATE_MISMATCH")
    );
    assert!(result
        .infrastructure_error
        .as_deref()
        .is_some_and(|message| message.contains("undeclared-empty")));
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
            expected_source_state_digest: None,
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
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(closed.removed);
    assert_eq!(
        closed.closure_disposition,
        WorkspaceClosureDisposition::Removed
    );
    assert!(!workspace.exists());
}

#[test]
fn workspace_close_distinguishes_already_absent_from_closed_tombstone() {
    let sandbox = Sandbox::new("close-already-absent");
    let config = sandbox.config();
    config.ensure_store().unwrap();
    let closed = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-never-created".to_string(),
            force: false,
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(!closed.removed);
    assert_eq!(
        closed.closure_disposition,
        WorkspaceClosureDisposition::AlreadyAbsent
    );
    assert!(closed.source_state_digest.is_none());
}

#[test]
fn workspace_close_fences_exact_source_state_and_replays_tombstone() {
    let sandbox = Sandbox::new("close-source-state-fence");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-source-state-fence";
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
    fs::write(workspace.join("README.md"), "verified state\n").unwrap();
    let verified = workspace_source_state_digest(&config, workspace_id).unwrap();
    fs::write(workspace.join("README.md"), "raced state\n").unwrap();

    let error = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: true,
            expected_source_state_digest: Some(verified),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::RevisionMismatch);
    assert!(workspace.exists());

    let current = workspace_source_state_digest(&config, workspace_id).unwrap();
    let first = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: true,
            expected_source_state_digest: Some(current.clone()),
        },
    )
    .unwrap();
    assert!(first.removed);
    assert_eq!(
        first.closure_disposition,
        WorkspaceClosureDisposition::Removed
    );
    assert_eq!(first.source_state_digest.as_deref(), Some(current.as_str()));

    let replay = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: true,
            expected_source_state_digest: Some(current.clone()),
        },
    )
    .unwrap();
    assert!(!replay.removed);
    assert_eq!(
        replay.closure_disposition,
        WorkspaceClosureDisposition::AlreadyClosed
    );
    assert_eq!(
        replay.source_state_digest.as_deref(),
        Some(current.as_str())
    );

    let mismatch = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: true,
            expected_source_state_digest: Some(sha256_bytes(b"another-state")),
        },
    )
    .unwrap_err();
    assert_eq!(mismatch.code, UniversalExecErrorCode::RevisionMismatch);
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
    let workspace_id = "workspace-clean-close";
    for cache in [
        config.workspace_cache_path(workspace_id),
        config.workspace_build_cache_path(workspace_id),
        config.workspace_tmp_path(workspace_id),
    ] {
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("cached"), b"cache").unwrap();
    }
    let closed = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(closed.removed);
    assert!(!config.workspace_cache_path(workspace_id).exists());
    assert!(!config.workspace_build_cache_path(workspace_id).exists());
    assert!(!config.workspace_tmp_path(workspace_id).exists());
}

#[test]
fn workspace_close_cache_failure_does_not_commit_closure() {
    let sandbox = Sandbox::new("close-cache-failure");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    let workspace_id = "workspace-cache-failure";
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
    let cache_path = config.workspace_cache_path(workspace_id);
    fs::write(&cache_path, b"not-a-directory").unwrap();
    let error = remove_git_workspace(
        &config,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            force: false,
            expected_source_state_digest: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::IoError);
    assert!(config.workspace_path(workspace_id).is_dir());
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(config.workspace_record_path(workspace_id)).unwrap())
            .unwrap();
    assert_ne!(record["state"], "closed");
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
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(first.removed);
    assert_eq!(
        first.closure_disposition,
        WorkspaceClosureDisposition::Removed
    );
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
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(!second.removed);
    assert_eq!(
        second.closure_disposition,
        WorkspaceClosureDisposition::AlreadyClosed
    );
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
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(!closed.removed);
    assert_eq!(
        closed.closure_disposition,
        WorkspaceClosureDisposition::RecoveredMissing
    );
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
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    assert!(!repaired.removed);
    assert_eq!(
        repaired.closure_disposition,
        WorkspaceClosureDisposition::RecoveredMissing
    );
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
                expected_source_state_digest: None,
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
            expected_source_state_digest: None,
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
fn large_mutation_batch_preflights_atomically_without_cardinality_cap() {
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
    assert_eq!(mutations.len(), 32);

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
    let large = WorkspaceMutateRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: workspace_id.to_string(),
        mutations,
    };
    assert_eq!(large.mutations.len(), 33);
    large.validate_shape().unwrap();
}

#[test]
fn exec_strings_follow_linux_physical_boundary_without_cardinality_caps() {
    let args = (0..129)
        .map(|index| format!("arg-{index}"))
        .collect::<Vec<_>>();
    validate_args(&args).unwrap();

    let env = (0..65)
        .map(|index| (format!("KEY_{index}"), format!("value-{index}")))
        .collect::<BTreeMap<_, _>>();
    validate_env(&env).unwrap();

    let per_string_limit = linux_exec_string_limit_bytes().unwrap();
    validate_args(&["x".repeat(per_string_limit)]).unwrap();
    let too_large = validate_args(&["x".repeat(per_string_limit + 1)]).unwrap_err();
    assert_eq!(too_large.field.as_deref(), Some("args"));

    let aggregate_limit = linux_exec_payload_limit_bytes().unwrap();
    let chunk = 32 * 1024;
    let aggregate = (0..(aggregate_limit / chunk + 2))
        .map(|_| "x".repeat(chunk))
        .collect::<Vec<_>>();
    let too_large = validate_exec_payload(&aggregate, &BTreeMap::new(), "execution").unwrap_err();
    assert_eq!(too_large.field.as_deref(), Some("execution"));
}

#[test]
fn patch_shape_accepts_batches_beyond_legacy_cardinality_caps() {
    let files = (0..33)
        .map(|index| WorkspaceFilePatch {
            relative_path: format!("file-{index}.txt"),
            expected_digest: None,
            edits: vec![WorkspaceTextEdit {
                range: WorkspaceTextRange {
                    start: WorkspaceTextPosition { line: 1, column: 0 },
                    end: WorkspaceTextPosition { line: 1, column: 0 },
                },
                expected_text: String::new(),
                replacement: format!("{index}"),
            }],
        })
        .collect::<Vec<_>>();
    WorkspacePatchRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-large-patch".to_string(),
        files,
        max_diff_bytes: 4096,
    }
    .validate_shape()
    .unwrap();

    let edits = (0..129)
        .map(|index| WorkspaceTextEdit {
            range: WorkspaceTextRange {
                start: WorkspaceTextPosition {
                    line: 1,
                    column: index,
                },
                end: WorkspaceTextPosition {
                    line: 1,
                    column: index,
                },
            },
            expected_text: String::new(),
            replacement: "x".to_string(),
        })
        .collect::<Vec<_>>();
    WorkspacePatchRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        workspace_id: "workspace-many-edits".to_string(),
        files: vec![WorkspaceFilePatch {
            relative_path: "file.txt".to_string(),
            expected_digest: None,
            edits,
        }],
        max_diff_bytes: 4096,
    }
    .validate_shape()
    .unwrap();
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
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["tool.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 2000,
        stdout_limit_bytes: 8,
        stderr_limit_bytes: 9,
        host_dependencies: Vec::new(),
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
fn runner_shared_overall_deadline_is_independent_of_step_timeout_sum() {
    let sandbox = Sandbox::new("runner-shared-overall");
    let workspace = sandbox.root.join("workspace");
    let fast_dir = sandbox.root.join("fast");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&fast_dir).unwrap();
    let true_executable = real_executable("/usr/bin/true");
    let step = |id: &str| RunnerExecutionStep {
        id: id.to_string(),
        executable: true_executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&true_executable).unwrap(),
        args: Vec::new(),
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        timeout_ms: 500,
        continue_on_error: false,
    };
    let fast = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        job_id: None,
        attempt_id: None,
        launch_token: None,
        unit_name: None,
        payload: None,
        inherit_host_environment: true,
        task_id: "task-shared-overall-fast".to_string(),
        workspace_id: "workspace-shared-overall".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        executable: true_executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&true_executable).unwrap(),
        args: Vec::new(),
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: vec![step("one"), step("two")],
        timeout_ms: 500,
        stdout_limit_bytes: 1024,
        stderr_limit_bytes: 1024,
        host_dependencies: Vec::new(),
    };
    assert!(fast.steps.iter().map(|step| step.timeout_ms).sum::<u64>() > fast.timeout_ms);
    write_json_atomic(&fast_dir.join("request.json"), &fast).unwrap();
    run_task_runner(&fast_dir).unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(fast_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Completed);

    let slow_dir = sandbox.root.join("slow");
    fs::create_dir_all(&slow_dir).unwrap();
    let sleep_executable = real_executable("/usr/bin/sleep");
    let mut slow = fast;
    slow.task_id = "task-shared-overall-slow".to_string();
    slow.timeout_ms = 250;
    slow.steps[1] = RunnerExecutionStep {
        id: "two".to_string(),
        executable: sleep_executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&sleep_executable).unwrap(),
        args: vec!["1".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        timeout_ms: 1_000,
        continue_on_error: false,
    };
    write_json_atomic(&slow_dir.join("request.json"), &slow).unwrap();
    run_task_runner(&slow_dir).unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(slow_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert!(result.timed_out);
    assert_eq!(result.failed_step_id.as_deref(), Some("two"));
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
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["tool.py".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 50,
        stdout_limit_bytes: 1024,
        stderr_limit_bytes: 1024,
        host_dependencies: Vec::new(),
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    run_task_runner(&task_dir).unwrap();
    let result: RunnerTaskResult =
        serde_json::from_slice(&fs::read(task_dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result.status, TaskTerminalStatus::Failed);
    assert!(result.timed_out);
}

#[test]
fn runner_timeout_terminates_descendant_pipe_holders_before_result() {
    let sandbox = Sandbox::new("timeout-descendants");
    let workspace = sandbox.root.join("workspace");
    let task_dir = sandbox.root.join("task");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&task_dir).unwrap();
    let executable = real_executable("/usr/bin/bash");
    let request = RunnerTaskRequest {
        schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
        job_id: None,
        attempt_id: None,
        launch_token: None,
        unit_name: None,
        payload: None,
        inherit_host_environment: true,
        task_id: "task-timeout-descendants".to_string(),
        workspace_id: "workspace-timeout-descendants".to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        workspace_source_digest: None,
        build_target_backing: None,
        input_presentation_root: None,
        input_commitments: Vec::new(),
        executable: executable.to_string_lossy().into_owned(),
        executable_digest: sha256_file(&executable).unwrap(),
        args: vec!["-lc".to_string(), "sleep 5 & wait".to_string()],
        cwd: workspace.to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        steps: Vec::new(),
        timeout_ms: 50,
        stdout_limit_bytes: 1024,
        stderr_limit_bytes: 1024,
        host_dependencies: Vec::new(),
    };
    write_json_atomic(&task_dir.join("request.json"), &request).unwrap();
    let started = std::time::Instant::now();
    run_task_runner(&task_dir).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
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

#[test]
fn workspace_slice_streams_large_file_while_preserving_whole_file_digest() {
    let sandbox = Sandbox::new("workspace-slice-large");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-slice-large".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path("workspace-slice-large");
    let content = "abcdefgh".repeat(1_048_576);
    fs::write(workspace.join("large.txt"), content.as_bytes()).unwrap();
    let slice = read_workspace_slice(
        &config,
        &WorkspaceReadSliceRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-slice-large".to_string(),
            relative_path: "large.txt".to_string(),
            offset: 4_194_304,
            max_bytes: 16,
        },
    )
    .unwrap();
    assert_eq!(slice.content, &content[4_194_304..4_194_320]);
    assert_eq!(slice.file_byte_length, content.len() as u64);
    assert_eq!(slice.file_digest, sha256_bytes(content.as_bytes()));
    assert!(!slice.eof);
}

#[test]
fn workspace_slice_still_rejects_invalid_utf8_outside_requested_range() {
    let sandbox = Sandbox::new("workspace-slice-invalid-tail");
    let source = sandbox.root.join("source");
    init_git_repo(&source);
    let config = sandbox.config();
    create_git_workspace(
        &config,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-slice-invalid-tail".to_string(),
            source_repo: source.to_string_lossy().into_owned(),
            source_revision: "HEAD".to_string(),
        },
    )
    .unwrap();
    let workspace = config.workspace_path("workspace-slice-invalid-tail");
    fs::write(workspace.join("bad.txt"), b"visible-prefix\n\xff").unwrap();
    let error = read_workspace_slice(
        &config,
        &WorkspaceReadSliceRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: "workspace-slice-invalid-tail".to_string(),
            relative_path: "bad.txt".to_string(),
            offset: 0,
            max_bytes: 7,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UniversalExecErrorCode::ArtifactNotUtf8);
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
