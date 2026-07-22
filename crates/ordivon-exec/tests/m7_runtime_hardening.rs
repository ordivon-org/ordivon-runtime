#![cfg(feature = "runtime-hardening-m7")]

use ordivon_exec::{
    create_git_workspace, remove_git_workspace, write_workspace_text, AttemptState,
    GitWorkspaceCreateRequest, M6RegistryConfig, M6Runtime, M6RuntimeConfig, M6TaskCancelRequest,
    M6TaskRunRequest, M6UniversalExecutionRequest, M7LifecyclePolicy, M7OrphanRemediator,
    M7RuntimeHardeningConfig, M7WorkerIdentity, TerminalCommitM6, UniversalExecutorConfig,
    WorkspaceWriteRequest, M6_SCHEMA_VERSION, MAX_UNIVERSAL_OUTPUT_BYTES, MAX_UNIVERSAL_RUNTIME_MS,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

fn digest(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
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

fn worker_identity() -> M7WorkerIdentity {
    let output = Command::new("getent")
        .args(["passwd", "ordivon-worker"])
        .output()
        .unwrap();
    assert!(output.status.success(), "ordivon-worker is not installed");
    let line = String::from_utf8(output.stdout).unwrap();
    let fields: Vec<_> = line.trim().split(':').collect();
    M7WorkerIdentity {
        user: "ordivon-worker".to_string(),
        group: "ordivon-worker".to_string(),
        uid: fields[2].parse().unwrap(),
        gid: fields[3].parse().unwrap(),
    }
}

struct Context {
    id: String,
    control_root: PathBuf,
    worker_root: PathBuf,
    cache_root: PathBuf,
    view_root: PathBuf,
    executor: UniversalExecutorConfig,
    runtime: M6Runtime,
    hardening: M7RuntimeHardeningConfig,
    workspace_id: String,
    other_workspace_id: String,
}

impl Context {
    fn new(label: &str) -> Self {
        let id = format!("{}-{}", label, Uuid::now_v7());
        let control_root = PathBuf::from("/var/lib/ordivon/control/m7-tests").join(&id);
        let worker_root = PathBuf::from("/var/lib/ordivon/worker/m7-tests").join(&id);
        let cache_root = PathBuf::from("/var/cache/ordivon-worker/m7-tests").join(&id);
        let view_root = PathBuf::from("/run/ordivon/m7-tests").join(&id);
        let worker = worker_identity();
        let executor = UniversalExecutorConfig {
            store_root: control_root.join("executor"),
            workspace_root: Some(worker_root.join("workspaces")),
            workspace_uid: Some(worker.uid),
            workspace_gid: Some(worker.gid),
            runner_path: PathBuf::from("/usr/lib/ordivon/ordivon-task-runner"),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: MAX_UNIVERSAL_RUNTIME_MS,
            max_output_bytes: MAX_UNIVERSAL_OUTPUT_BYTES,
        };
        executor.ensure_store().unwrap();
        let repo =
            fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        let revision = command_output("git", &["rev-parse", "HEAD"], &repo);
        let workspace_id = format!("m7-workspace-{}", Uuid::now_v7());
        let other_workspace_id = format!("m7-other-{}", Uuid::now_v7());
        for workspace_id in [&workspace_id, &other_workspace_id] {
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
        }
        write_workspace_text(
            &executor,
            &WorkspaceWriteRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: other_workspace_id.clone(),
                relative_path: "m7_other_secret.txt".to_string(),
                content: "OTHER_WORKSPACE_SECRET\n".to_string(),
                expected_digest: None,
            },
        )
        .unwrap();
        let hardening = M7RuntimeHardeningConfig {
            worker,
            control_root: control_root.clone(),
            worker_root: worker_root.clone(),
            cache_root: cache_root.clone(),
            runtime_view_root: view_root.clone(),
            lifecycle_policy: M7LifecyclePolicy {
                schema_version: 1,
                retention_ms: 86_400_000,
                max_retained_artifact_bytes: 1_073_741_824,
                max_single_job_artifact_bytes: 33_554_432,
                max_gc_items: 1000,
            },
        };
        let runtime = M6Runtime::new(M6RuntimeConfig {
            registry: M6RegistryConfig {
                db_path: control_root.join("registry/registry.sqlite3"),
                store_root: control_root.join("registry"),
                busy_timeout_ms: 5000,
            },
            executor: executor.clone(),
            startup_grace_ms: 3000,
            hardening: Some(hardening.clone()),
        })
        .unwrap();
        Self {
            id,
            control_root,
            worker_root,
            cache_root,
            view_root,
            executor,
            runtime,
            hardening,
            workspace_id,
            other_workspace_id,
        }
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

    fn request(&self, script: &str, wait_ms: u64) -> M6TaskRunRequest {
        let mut env = BTreeMap::new();
        env.insert(
            "M7_CONTROL_ROOT".to_string(),
            self.control_root.to_string_lossy().into_owned(),
        );
        env.insert(
            "M7_OTHER_WORKSPACE".to_string(),
            self.executor
                .workspaces_root()
                .join(&self.other_workspace_id)
                .to_string_lossy()
                .into_owned(),
        );
        M6TaskRunRequest {
            schema_version: M6_SCHEMA_VERSION,
            client_request_id: format!("m7:{}:{}", self.id, Uuid::now_v7()),
            principal: "principal:m7-integration".to_string(),
            authority_ref: "authority:m7-local".to_string(),
            policy_id: "policy:m7-worker".to_string(),
            policy_version: "1".to_string(),
            policy_digest: digest(b"policy:m7-worker:1"),
            profile_id: None,
            global_limit: 2,
            profile_limit: None,
            execution: M6UniversalExecutionRequest {
                workspace_id: self.workspace_id.clone(),
                executable: "/usr/bin/python3.14".to_string(),
                args: vec![script.to_string()],
                cwd_relative: ".".to_string(),
                env,
                timeout_ms: 30_000,
                stdout_limit_bytes: 65_536,
                stderr_limit_bytes: 65_536,
            },
            wait_ms,
            stdout_tail_bytes: 16_384,
            stderr_tail_bytes: 16_384,
        }
    }
}

fn force_orphan(context: &Context, attempt_id: &str) {
    let attempt = context.runtime.registry().get_attempt(attempt_id).unwrap();
    context
        .runtime
        .registry()
        .commit_terminal(&TerminalCommitM6 {
            attempt_id: attempt.attempt_id,
            expected_row_version: attempt.row_version,
            state: AttemptState::Orphaned,
            result_digest: digest(b"m7-forced-orphan"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"m7-forced-orphan")),
            finished_at_ms: 999,
            artifacts: Vec::new(),
            reason_code: "M7_TEST_FORCED_ORPHAN".to_string(),
        })
        .unwrap();
}

impl Drop for Context {
    fn drop(&mut self) {
        for workspace in [&self.workspace_id, &self.other_workspace_id] {
            let _ = remove_git_workspace(&self.executor, workspace);
        }
        let _ = fs::remove_dir_all(&self.control_root);
        let _ = fs::remove_dir_all(&self.worker_root);
        let _ = fs::remove_dir_all(&self.cache_root);
        let _ = fs::remove_dir_all(&self.view_root);
    }
}

fn enabled() -> bool {
    std::env::var("ORDIVON_RUN_M7_INTEGRATION").as_deref() == Ok("1")
}

#[test]
#[ignore = "requires root, systemd, static ordivon-worker, installed M7 Runner, and explicit opt-in"]
fn payload_runs_as_worker_and_cannot_cross_trust_boundaries() {
    if !enabled() {
        return;
    }
    let context = Context::new("identity");
    context.write(
        "m7_identity.py",
        r#"import json, os, pathlib

def denied(path):
    try:
        pathlib.Path(path).read_bytes()
        return False
    except OSError:
        return True

cwd = pathlib.Path.cwd()
(cwd / 'payload-write.txt').write_text('worker-write')
result = {
    'uid': os.getuid(),
    'gid': os.getgid(),
    'groups': os.getgroups(),
    'cwd': str(cwd),
    'home': os.environ.get('HOME'),
    'tmpdir': os.environ.get('TMPDIR'),
    'cache': os.environ.get('XDG_CACHE_HOME'),
    'controlHidden': denied(os.environ['M7_CONTROL_ROOT']),
    'otherWorkspaceHidden': denied(pathlib.Path(os.environ['M7_OTHER_WORKSPACE']) / 'm7_other_secret.txt'),
    'gitHidden': denied(cwd / '.git'),
    'dockerHidden': denied('/run/docker.sock'),
}
print(json.dumps(result, sort_keys=True), flush=True)
"#,
    );
    let observation = context
        .runtime
        .run_task(&context.request("m7_identity.py", 30_000))
        .unwrap();
    assert_eq!(
        observation.status,
        "succeeded",
        "{}",
        observation.error_summary.unwrap_or_default()
    );
    let value: serde_json::Value = serde_json::from_str(observation.stdout_tail.trim()).unwrap();
    let worker = worker_identity();
    assert_eq!(value["uid"].as_u64(), Some(u64::from(worker.uid)));
    assert_eq!(value["gid"].as_u64(), Some(u64::from(worker.gid)));
    assert_eq!(value["groups"].as_array().unwrap().len(), 0);
    for field in [
        "controlHidden",
        "otherWorkspaceHidden",
        "gitHidden",
        "dockerHidden",
    ] {
        assert_eq!(value[field], true, "{field}: {value}");
    }
    assert!(value["cwd"].as_str().unwrap().starts_with("/run/ordivon/"));
    assert!(value["home"].as_str().unwrap().starts_with("/run/ordivon/"));
    let attempt_id = observation.attempt_id.unwrap();
    let attempt = context.runtime.registry().get_attempt(&attempt_id).unwrap();
    let result_path = Path::new(&attempt.bundle_path).join("result.json");
    let result_metadata = fs::metadata(&result_path).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(result_metadata.uid(), 0);
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(result_path).unwrap()).unwrap();
    assert_eq!(result["payloadUid"].as_u64(), Some(u64::from(worker.uid)));
    assert_eq!(result["payloadGid"].as_u64(), Some(u64::from(worker.gid)));
    assert!(!context.view_root.join(&attempt_id).exists());
}

#[test]
#[ignore = "requires root, systemd, static ordivon-worker, installed M7 Runner, and explicit opt-in"]
fn active_unit_has_minimal_capabilities_and_cancel_cleans_worker_descendants() {
    if !enabled() {
        return;
    }
    let context = Context::new("cancel");
    context.write(
        "m7_long.py",
        "import os,time\nprint(f'M7_LONG_UID={os.getuid()}', flush=True)\ntime.sleep(30)\n",
    );
    let observation = context
        .runtime
        .run_task(&context.request("m7_long.py", 0))
        .unwrap();
    let attempt_id = observation.attempt_id.clone().unwrap();
    let attempt = context.runtime.registry().get_attempt(&attempt_id).unwrap();
    thread::sleep(Duration::from_millis(200));
    let output = Command::new("systemctl")
        .args([
            "show",
            &attempt.unit_name,
            "--property=User,Group,CapabilityBoundingSet,AmbientCapabilities,NoNewPrivileges,ProtectHome,ControlGroup",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let properties = String::from_utf8(output.stdout).unwrap();
    assert!(
        properties.contains("CapabilityBoundingSet=cap_setgid cap_setuid")
            || properties.contains("CapabilityBoundingSet=cap_setuid cap_setgid"),
        "{properties}"
    );
    assert!(
        properties.contains("AmbientCapabilities=\n"),
        "{properties}"
    );
    assert!(properties.contains("NoNewPrivileges=yes"), "{properties}");
    assert!(properties.contains("ProtectHome=yes"), "{properties}");
    let cgroup = attempt.control_group.clone().unwrap();
    let procs = fs::read_to_string(format!("/sys/fs/cgroup{cgroup}/cgroup.procs")).unwrap();
    let worker = worker_identity();
    let mut saw_root = false;
    let mut saw_worker = false;
    for pid in procs.lines() {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
        let uid_line = status
            .lines()
            .find(|line| line.starts_with("Uid:"))
            .unwrap();
        let uid: u32 = uid_line.split_whitespace().nth(1).unwrap().parse().unwrap();
        saw_root |= uid == 0;
        saw_worker |= uid == worker.uid;
    }
    assert!(
        saw_root && saw_worker,
        "cgroup did not contain both supervisor and payload: {procs}"
    );
    let cancelled = context
        .runtime
        .cancel_task(&M6TaskCancelRequest {
            schema_version: M6_SCHEMA_VERSION,
            job_id: observation.job_id,
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    thread::sleep(Duration::from_millis(100));
    assert!(
        !Path::new(&format!("/sys/fs/cgroup{cgroup}")).exists()
            || fs::read_to_string(format!("/sys/fs/cgroup{cgroup}/cgroup.procs"))
                .unwrap_or_default()
                .trim()
                .is_empty()
    );
    assert!(!context.view_root.join(&attempt_id).exists());
}

#[test]
#[ignore = "requires root, systemd, static ordivon-worker, installed M7 Runner, and explicit opt-in"]
fn orphan_remediation_holds_live_tree_then_terminates_and_releases() {
    if !enabled() {
        return;
    }
    let context = Context::new("orphan-live");
    context.write(
        "m7_orphan.py",
        "import time\nprint('M7_ORPHAN_LIVE', flush=True)\ntime.sleep(30)\n",
    );
    let observation = context
        .runtime
        .run_task(&context.request("m7_orphan.py", 0))
        .unwrap();
    let attempt_id = observation.attempt_id.unwrap();
    thread::sleep(Duration::from_millis(200));
    force_orphan(&context, &attempt_id);
    let remediator = M7OrphanRemediator::new(
        context.runtime.registry().clone(),
        context.hardening.clone(),
    )
    .unwrap();
    let evidence = remediator
        .inspect(&attempt_id, "operator:m7-test", 1000)
        .unwrap();
    assert!(!evidence.live_processes.is_empty());
    let denied = remediator
        .remediate(
            &attempt_id,
            "operator:m7-test",
            &evidence.evidence_digest,
            false,
            1001,
        )
        .unwrap_err();
    assert_eq!(
        denied.code,
        ordivon_exec::M6ErrorCode::OrphanRemediationDenied
    );
    assert_eq!(
        context
            .runtime
            .registry()
            .get_reservation(&attempt_id)
            .unwrap()
            .state,
        ordivon_exec::ReservationState::HeldOrphaned
    );
    let current = remediator
        .inspect(&attempt_id, "operator:m7-test", 1002)
        .unwrap();
    let released = remediator
        .remediate(
            &attempt_id,
            "operator:m7-test",
            &current.evidence_digest,
            true,
            1003,
        )
        .unwrap();
    assert!(released.reservation_released);
    assert!(released.termination_requested);
    assert_eq!(
        context
            .runtime
            .registry()
            .active_reservation_count()
            .unwrap(),
        0
    );
}

#[test]
#[ignore = "requires root, systemd, static ordivon-worker, installed M7 Runner, and explicit opt-in"]
fn reused_unit_is_not_terminated_while_original_reservation_is_released() {
    if !enabled() {
        return;
    }
    let context = Context::new("orphan-reused");
    context.write(
        "m7_reused.py",
        "import time\nprint('M7_REUSED_ORIGINAL', flush=True)\ntime.sleep(30)\n",
    );
    let observation = context
        .runtime
        .run_task(&context.request("m7_reused.py", 0))
        .unwrap();
    let attempt_id = observation.attempt_id.unwrap();
    thread::sleep(Duration::from_millis(200));
    let original = context.runtime.registry().get_attempt(&attempt_id).unwrap();
    force_orphan(&context, &attempt_id);
    Command::new("systemctl")
        .args(["stop", &original.unit_name])
        .status()
        .unwrap();
    Command::new("systemctl")
        .args(["reset-failed", &original.unit_name])
        .status()
        .unwrap();
    thread::sleep(Duration::from_millis(100));
    let unit_base = original.unit_name.trim_end_matches(".service");
    let output = Command::new("systemd-run")
        .args([
            "--quiet",
            "--unit",
            unit_base,
            "--collect",
            "/usr/bin/sleep",
            "30",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    thread::sleep(Duration::from_millis(100));
    let remediator = M7OrphanRemediator::new(
        context.runtime.registry().clone(),
        context.hardening.clone(),
    )
    .unwrap();
    let evidence = remediator
        .inspect(&attempt_id, "operator:m7-test", 2000)
        .unwrap();
    assert!(evidence.unit_active);
    assert!(!evidence.invocation_matches);
    let released = remediator
        .remediate(
            &attempt_id,
            "operator:m7-test",
            &evidence.evidence_digest,
            true,
            2001,
        )
        .unwrap();
    assert!(released.reservation_released);
    assert!(!released.termination_requested);
    assert!(Command::new("systemctl")
        .args(["is-active", "--quiet", &original.unit_name])
        .status()
        .unwrap()
        .success());
    Command::new("systemctl")
        .args(["stop", &original.unit_name])
        .status()
        .unwrap();
    Command::new("systemctl")
        .args(["reset-failed", &original.unit_name])
        .status()
        .unwrap();
}
