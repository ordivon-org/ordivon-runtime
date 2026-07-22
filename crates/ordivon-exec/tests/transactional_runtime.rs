#![cfg(feature = "transactional-runtime")]

use ordivon_exec::{
    create_git_workspace, remove_git_workspace, write_workspace_text, ArtifactReadRequest,
    AttemptState, GitWorkspaceCreateRequest, RegistryConfig, Runtime, RuntimeConfig,
    RuntimeExecutionPlan, RuntimeJobListRequest, SubmitRequest, TaskCancelRequest,
    TaskObserveRequest, TaskRunRequest, UniversalExecutionRequest, UniversalExecutorConfig,
    WorkspaceWriteRequest, MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS,
    RUNTIME_SCHEMA_VERSION, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

fn digest(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
}

fn file_digest(path: &Path) -> String {
    digest(&fs::read(path).unwrap())
}

fn command_output(program: &str, args: &[&str], cwd: &Path) -> String {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_transactional_runtime_executes_replays_and_releases_capacity() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let runner_path =
        PathBuf::from(std::env::var("ORDIVON_RUNNER_PATH").expect("ORDIVON_RUNNER_PATH"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = fs::canonicalize(repo).unwrap();
    let revision = command_output("git", &["rev-parse", "HEAD"], &repo);
    let root =
        PathBuf::from("/root/.local/share/ordivon-integration").join(Uuid::now_v7().to_string());
    let store = root.join("store");
    let executor = UniversalExecutorConfig {
        store_root: store.clone(),
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
        runner_path,
        allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
        max_runtime_ms: MAX_UNIVERSAL_RUNTIME_MS,
        max_output_bytes: MAX_UNIVERSAL_OUTPUT_BYTES,
    };
    executor.ensure_store().unwrap();
    let workspace_id = format!("runtime-it-{}", Uuid::now_v7());
    create_git_workspace(
        &executor,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.clone(),
            source_repo: repo.to_string_lossy().into_owned(),
            source_revision: revision,
        },
    )
    .unwrap();

    write_workspace_text(
        &executor,
        &WorkspaceWriteRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.clone(),
            relative_path: "runtime_it.py".to_string(),
            content: "print('RUNTIME_OK', flush=True)\n".to_string(),
            expected_digest: None,
        },
    )
    .unwrap();
    let runtime = Runtime::new(RuntimeConfig {
        registry: ordivon_exec::RegistryConfig {
            db_path: root.join("registry/registry.sqlite3"),
            store_root: root.join("registry"),
            busy_timeout_ms: 5000,
        },
        executor: executor.clone(),
        startup_grace_ms: 2000,
        #[cfg(feature = "isolated-execution")]
        hardening: None,
    })
    .unwrap();
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: format!("request:it:{}", Uuid::now_v7()),
        principal: "principal:integration".to_string(),
        global_limit: 2,
        execution: UniversalExecutionRequest {
            workspace_id: workspace_id.clone(),
            executable: "/usr/bin/python3.14".to_string(),
            args: vec!["runtime_it.py".to_string()],
            cwd_relative: ".".to_string(),
            env: Default::default(),
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
        },
        wait_ms: 30_000,
        stdout_tail_bytes: 4096,
        stderr_tail_bytes: 4096,
    };
    let first = runtime.run_task(&request).unwrap();
    assert_eq!(first.status, "succeeded");
    assert!(first.stdout_tail.contains("RUNTIME_OK"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);

    let replay = runtime.run_task(&request).unwrap();
    assert_eq!(replay.job_id, first.job_id);
    assert_eq!(replay.status, "succeeded");

    let listed = runtime
        .list_jobs(&RuntimeJobListRequest {
            limit: 10,
            cursor: None,
        })
        .unwrap();
    assert_eq!(listed.jobs.len(), 1);
    assert_eq!(listed.jobs[0].job_id, first.job_id);
    let artifacts = runtime.registry().list_artifacts(&first.job_id).unwrap();
    assert_eq!(artifacts.len(), 3);
    let stdout = artifacts
        .iter()
        .find(|artifact| artifact.kind == "stdout")
        .unwrap();
    let read = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: first.job_id.clone(),
            artifact_id: stdout.artifact_id.clone(),
            offset: 0,
            max_bytes: 4096,
        })
        .unwrap();
    assert!(read.content.contains("RUNTIME_OK"));
    assert!(read.eof);

    let attempt_id = first.attempt_id.unwrap();
    let unit = format!("ordivon-{attempt_id}.service");
    let _ = Command::new("systemctl").args(["stop", &unit]).output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &unit])
        .output();
    remove_git_workspace(&executor, &workspace_id).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

struct IntegrationContext {
    root: PathBuf,
    repo: PathBuf,
    revision: String,
    executor: UniversalExecutorConfig,
    registry: RegistryConfig,
    workspace_id: String,
}

impl IntegrationContext {
    fn new(label: &str) -> Self {
        let runner_path =
            PathBuf::from(std::env::var("ORDIVON_RUNNER_PATH").expect("ORDIVON_RUNNER_PATH"));
        let repo =
            fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        let revision = command_output("git", &["rev-parse", "HEAD"], &repo);
        let root = PathBuf::from("/root/.local/share/ordivon-integration")
            .join(format!("{label}-{}", Uuid::now_v7()));
        let executor = UniversalExecutorConfig {
            store_root: root.join("store"),
            workspace_root: None,
            workspace_uid: None,
            workspace_gid: None,
            runner_path,
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: MAX_UNIVERSAL_RUNTIME_MS,
            max_output_bytes: MAX_UNIVERSAL_OUTPUT_BYTES,
        };
        executor.ensure_store().unwrap();
        let workspace_id = format!("runtime-{label}-{}", Uuid::now_v7());
        create_git_workspace(
            &executor,
            &GitWorkspaceCreateRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: workspace_id.clone(),
                source_repo: repo.to_string_lossy().into_owned(),
                source_revision: revision.clone(),
            },
        )
        .unwrap();
        Self {
            registry: RegistryConfig {
                db_path: root.join("registry/registry.sqlite3"),
                store_root: root.join("registry"),
                busy_timeout_ms: 5000,
            },
            root,
            repo,
            revision,
            executor,
            workspace_id,
        }
    }

    fn runtime(&self, startup_grace_ms: u64) -> Runtime {
        Runtime::new(RuntimeConfig {
            registry: self.registry.clone(),
            executor: self.executor.clone(),
            startup_grace_ms,
            #[cfg(feature = "isolated-execution")]
            hardening: None,
        })
        .unwrap()
    }

    fn write(&self, path: &str, content: &str) {
        write_workspace_text(
            &self.executor,
            &WorkspaceWriteRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: self.workspace_id.clone(),
                relative_path: path.to_string(),
                content: content.to_string(),
                expected_digest: None,
            },
        )
        .unwrap();
    }

    fn request(&self, script: &str, wait_ms: u64) -> TaskRunRequest {
        TaskRunRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: format!("request:{script}:{}", Uuid::now_v7()),
            principal: "principal:integration".to_string(),
            global_limit: 8,
            execution: UniversalExecutionRequest {
                workspace_id: self.workspace_id.clone(),
                executable: "/usr/bin/python3.14".to_string(),
                args: vec![script.to_string()],
                cwd_relative: ".".to_string(),
                env: Default::default(),
                timeout_ms: 60_000,
                stdout_limit_bytes: 1_048_576,
                stderr_limit_bytes: 1_048_576,
            },
            wait_ms,
            stdout_tail_bytes: 8192,
            stderr_tail_bytes: 8192,
        }
    }
}

impl Drop for IntegrationContext {
    fn drop(&mut self) {
        if let Ok(entries) = fs::read_dir(self.root.join("registry/attempts")) {
            for entry in entries.flatten() {
                if let Some(attempt_id) = entry.file_name().to_str() {
                    let unit = format!("ordivon-{attempt_id}.service");
                    let _ = Command::new("systemctl").args(["stop", &unit]).output();
                    let _ = Command::new("systemctl")
                        .args(["reset-failed", &unit])
                        .output();
                }
            }
        }
        let worktrees = command_output("git", &["worktree", "list", "--porcelain"], &self.repo);
        for line in worktrees
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
        {
            let path = PathBuf::from(line);
            if path.starts_with(&self.root) {
                let _ = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&path)
                    .current_dir(&self.repo)
                    .output();
            }
        }
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo)
            .output();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_core_restart_recovers_running_attempt_and_terminal_result() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("recovery");
    context.write(
        "runtime_recover.py",
        "import time\nprint('RUNTIME_RECOVER_START', flush=True)\ntime.sleep(1.5)\nprint('RUNTIME_RECOVER_DONE', flush=True)\n",
    );
    let first_runtime = context.runtime(2000);
    let started = first_runtime
        .run_task(&context.request("runtime_recover.py", 0))
        .unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    let attempt = first_runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Running);
    drop(first_runtime);

    let recovered_runtime = context.runtime(2000);
    let completed = recovered_runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id,
            wait_ms: 10_000,
            stdout_tail_bytes: 8192,
            stderr_tail_bytes: 8192,
        })
        .unwrap();
    assert_eq!(completed.status, "succeeded");
    assert!(completed.stdout_tail.contains("RUNTIME_RECOVER_START"));
    assert!(completed.stdout_tail.contains("RUNTIME_RECOVER_DONE"));
    assert_eq!(
        recovered_runtime
            .registry()
            .active_reservation_count()
            .unwrap(),
        0
    );
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_cancel_intent_survives_runtime_reconstruction_and_cleans_cgroup() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("cancel");
    context.write(
        "runtime_cancel.py",
        "import signal,subprocess,sys,time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nchild=subprocess.Popen([sys.executable,'-c','import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)'])\nprint(f'RUNTIME_CANCEL_CHILD={child.pid}', flush=True)\ntime.sleep(30)\n",
    );
    let first_runtime = context.runtime(2000);
    let started = first_runtime
        .run_task(&context.request("runtime_cancel.py", 0))
        .unwrap();
    assert_eq!(started.status, "working");
    drop(first_runtime);

    let cancelling_runtime = context.runtime(2000);
    let cancelled = cancelling_runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id.clone(),
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(
        cancelling_runtime
            .registry()
            .active_reservation_count()
            .unwrap(),
        0
    );
    let attempt = cancelling_runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    let active = Command::new("systemctl")
        .args(["is-active", &attempt.unit_name])
        .output()
        .unwrap();
    assert!(!active.status.success());
}

impl IntegrationContext {
    fn direct_submit(&self, client_request_id: &str, global_limit: u32) -> SubmitRequest {
        let workspace = fs::canonicalize(
            self.executor
                .store_root
                .join("workspaces")
                .join(&self.workspace_id),
        )
        .unwrap();
        let executable = fs::canonicalize("/usr/bin/true").unwrap();
        SubmitRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: client_request_id.to_string(),
            plan: RuntimeExecutionPlan {
                schema_version: RUNTIME_SCHEMA_VERSION,
                workspace_id: self.workspace_id.clone(),
                workspace_path: workspace.to_string_lossy().into_owned(),
                source_revision: self.revision.clone(),
                executable: executable.to_string_lossy().into_owned(),
                executable_digest: file_digest(&executable),
                args: Vec::new(),
                cwd: workspace.to_string_lossy().into_owned(),
                env: Default::default(),
                timeout_ms: 10_000,
                stdout_limit_bytes: 65_536,
                stderr_limit_bytes: 65_536,
                principal: "principal:integration".to_string(),
            },
            global_limit,
        }
    }
}

fn created_admission(outcome: ordivon_exec::AdmissionOutcome) -> ordivon_exec::CreatedAdmission {
    match outcome {
        ordivon_exec::AdmissionOutcome::Created(created) => *created,
        ordivon_exec::AdmissionOutcome::Existing { .. } => {
            panic!("expected a new admission")
        }
    }
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_ambiguous_dispatch_is_lost_without_automatic_redispatch() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("ambiguous");
    let runtime = context.runtime(1);
    let created = created_admission(
        runtime
            .registry()
            .submit(&context.direct_submit("request:ambiguous", 1))
            .unwrap(),
    );
    let attempt = runtime
        .registry()
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"simulated-bundle"),
            1,
        )
        .unwrap();
    let attempt = runtime
        .registry()
        .mark_dispatch_issued(&attempt.attempt_id, attempt.row_version, 2)
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    runtime.reconcile_attempt(&attempt.attempt_id).unwrap();
    let projection = runtime.registry().project_job(&created.job.job_id).unwrap();
    assert_eq!(projection.status, "lost");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    let loaded = Command::new("systemctl")
        .args(["show", &attempt.unit_name, "--property=LoadState"])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&loaded.stdout).contains("LoadState=loaded"));
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_live_unit_without_launch_token_is_orphaned_and_holds_capacity() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("orphaned");
    let runtime = context.runtime(1);
    let created = created_admission(
        runtime
            .registry()
            .submit(&context.direct_submit("request:orphaned-live", 1))
            .unwrap(),
    );
    let attempt = runtime
        .registry()
        .mark_bundle_ready(
            &created.attempt.attempt_id,
            created.attempt.row_version,
            &digest(b"simulated-bundle"),
            1,
        )
        .unwrap();
    let attempt = runtime
        .registry()
        .mark_dispatch_issued(&attempt.attempt_id, attempt.row_version, 2)
        .unwrap();
    let launch = Command::new("systemd-run")
        .arg(format!("--unit={}", attempt.unit_name))
        .arg("--collect")
        .arg("--property=Type=exec")
        .arg("/usr/bin/sleep")
        .arg("30")
        .output()
        .unwrap();
    assert!(launch.status.success());
    std::thread::sleep(std::time::Duration::from_millis(20));
    runtime.reconcile_attempt(&attempt.attempt_id).unwrap();
    let projection = runtime.registry().project_job(&created.job.job_id).unwrap();
    assert_eq!(projection.status, "orphaned");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 1);
    assert_eq!(
        runtime
            .registry()
            .get_reservation(&attempt.attempt_id)
            .unwrap()
            .state,
        ordivon_exec::ReservationState::HeldOrphaned
    );
    let _ = Command::new("systemctl")
        .args(["stop", &attempt.unit_name])
        .output();
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_reconciler_rebuilds_bundle_after_admission_commit() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("bundle-rebuild");
    let runtime = context.runtime(2000);
    let created = created_admission(
        runtime
            .registry()
            .submit(&context.direct_submit("request:bundle-rebuild", 1))
            .unwrap(),
    );
    let attempts_root = context.registry.store_root.join("attempts");
    let stale = attempts_root.join(format!(
        ".{}.staging-crashed-core",
        created.attempt.attempt_id
    ));
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("partial"), b"partial bundle").unwrap();

    runtime.reconcile_all().unwrap();
    let completed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id,
            wait_ms: 10_000,
            stdout_tail_bytes: 1024,
            stderr_tail_bytes: 1024,
        })
        .unwrap();
    assert_eq!(completed.status, "succeeded");
    assert!(!stale.exists());
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_corrupt_runner_result_is_orphaned_and_quarantined() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("corrupt-result");
    context.write(
        "runtime_corrupt.py",
        "import time\nprint('RUNTIME_CORRUPT_RUNNING', flush=True)\ntime.sleep(30)\n",
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_corrupt.py", 0))
        .unwrap();
    assert_eq!(started.status, "working");
    let attempt = runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    fs::write(
        Path::new(&attempt.bundle_path).join("result.json"),
        b"{corrupt",
    )
    .unwrap();
    runtime.reconcile_attempt(&attempt.attempt_id).unwrap();
    let observation = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id,
            wait_ms: 0,
            stdout_tail_bytes: 1024,
            stderr_tail_bytes: 1024,
        })
        .unwrap();
    assert_eq!(observation.status, "orphaned");
    assert!(observation
        .error_summary
        .as_deref()
        .is_some_and(|message| message.contains("invalid Runner result")));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 1);
    assert_eq!(
        runtime
            .registry()
            .get_reservation(&attempt.attempt_id)
            .unwrap()
            .state,
        ordivon_exec::ReservationState::HeldOrphaned
    );
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_fast_failures_never_race_into_lost() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("fast-failure-race");
    context.write(
        "runtime_fast_fail.py",
        "import sys\nprint('RUNTIME_FAST_FAILURE', flush=True)\nsys.exit(7)\n",
    );
    let runtime = context.runtime(2000);
    for index in 0..10 {
        let mut request = context.request("runtime_fast_fail.py", 10_000);
        request.client_request_id = format!("request:fast-failure:{index}:{}", Uuid::now_v7());
        let observation = runtime.run_task(&request).unwrap();
        assert_eq!(
            observation.status, "failed",
            "fast failure {index} was misclassified as {}",
            observation.status
        );
        assert!(observation.stdout_tail.contains("RUNTIME_FAST_FAILURE"));
    }
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}
