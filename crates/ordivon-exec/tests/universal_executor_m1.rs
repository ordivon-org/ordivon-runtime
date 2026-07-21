#![cfg(feature = "universal-executor-m1")]

use ordivon_exec::{remove_git_workspace, UniversalExecutorConfig};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct M1Sandbox {
    store_root: PathBuf,
    workspace_id: String,
    task_ids: Vec<String>,
}

impl Drop for M1Sandbox {
    fn drop(&mut self) {
        for task_id in &self.task_ids {
            let unit = format!("ordivon-m1-{task_id}.service");
            let _ = Command::new("systemctl").args(["stop", &unit]).output();
            let _ = Command::new("systemctl")
                .args(["reset-failed", &unit])
                .output();
        }
        let config = executor_config(&self.store_root);
        let _ = remove_git_workspace(&config, &self.workspace_id);
        let _ = fs::remove_dir_all(&self.store_root);
    }
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, and explicit local opt-in"]
fn independent_cli_calls_complete_and_cancel_durable_tasks() {
    require_opt_in();
    let started = Instant::now();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let store_root = PathBuf::from(format!(
        "/root/.local/share/ordivon-m1-tests/{}-{unique}",
        std::process::id()
    ));
    let source_repo = repository_root();
    let workspace_id = format!("m1-workspace-{}-{unique}", std::process::id());
    let complete_task_id = format!("m1-complete-{}-{unique}", std::process::id());
    let cancel_task_id = format!("m1-cancel-{}-{unique}", std::process::id());
    let sandbox = M1Sandbox {
        store_root: store_root.clone(),
        workspace_id: workspace_id.clone(),
        task_ids: vec![complete_task_id.clone(), cancel_task_id.clone()],
    };
    let mut cli_calls = 0_u64;

    let create = cli_ok(
        "workspace-create",
        json!({
            "schemaVersion": 1,
            "workspaceId": workspace_id,
            "sourceRepo": source_repo,
            "sourceRevision": "HEAD"
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(create["workspaceId"], sandbox.workspace_id);

    let read = cli_ok(
        "workspace-read",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "crates/ordivon-exec/README.md",
            "maxBytes": 1048576
        }),
        &store_root,
        &mut cli_calls,
    );
    let original = read["content"].as_str().unwrap();
    let marker = format!("\nM1 durable executor marker {unique}\n");
    let updated = format!("{original}{marker}");
    let write = cli_ok(
        "workspace-write",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "crates/ordivon-exec/README.md",
            "content": updated,
            "expectedDigest": read["digest"]
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_ne!(write["beforeDigest"], write["afterDigest"]);

    let script = format!(
        "from pathlib import Path\nimport subprocess,sys,time\ntext=Path('crates/ordivon-exec/README.md').read_text()\ntry:\n    Path('/etc/ordivon-m1-denied').write_text('must-not-exist')\n    host_write='allowed'\nexcept OSError:\n    host_write='denied'\nsystemctl=subprocess.run(['/usr/bin/systemctl','is-system-running'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\ncontrol='allowed' if systemctl.returncode == 0 else 'denied'\nPath('m1-output.txt').write_text('marker=' + str({marker:?} in text) + ';hostWrite=' + host_write + ';systemControl=' + control)\nprint('M1_STDOUT marker observed')\nprint('M1_STDERR diagnostic', file=sys.stderr)\ntime.sleep(1.5)\n"
    );
    cli_ok(
        "workspace-write",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "m1_tool.py",
            "content": script,
            "expectedDigest": null
        }),
        &store_root,
        &mut cli_calls,
    );

    let python = fs::canonicalize("/usr/bin/python3").unwrap();
    let start = cli_ok(
        "task-start",
        json!({
            "schemaVersion": 1,
            "taskId": complete_task_id,
            "workspaceId": sandbox.workspace_id,
            "executable": python,
            "args": ["m1_tool.py"],
            "cwdRelative": ".",
            "env": {"PYTHONUNBUFFERED": "1"},
            "timeoutMs": 10000,
            "stdoutLimitBytes": 65536,
            "stderrLimitBytes": 65536
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(start["status"], "WORKING");

    thread::sleep(Duration::from_millis(150));
    let independently_observed = cli_ok(
        "task-get",
        json!({"schemaVersion": 1, "taskId": complete_task_id, "waitMs": 0}),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(independently_observed["status"], "WORKING");

    let completed = poll_terminal(&complete_task_id, &store_root, &mut cli_calls);
    assert_eq!(completed["status"], "COMPLETED");
    assert_eq!(completed["artifacts"].as_array().unwrap().len(), 3);

    let stdout = cli_ok(
        "artifact-read",
        json!({
            "schemaVersion": 1,
            "taskId": complete_task_id,
            "artifactId": format!("{complete_task_id}.stdout"),
            "offset": 0,
            "maxBytes": 65536
        }),
        &store_root,
        &mut cli_calls,
    );
    assert!(stdout["content"]
        .as_str()
        .unwrap()
        .contains("M1_STDOUT marker observed"));
    let stderr = cli_ok(
        "artifact-read",
        json!({
            "schemaVersion": 1,
            "taskId": complete_task_id,
            "artifactId": format!("{complete_task_id}.stderr"),
            "offset": 0,
            "maxBytes": 65536
        }),
        &store_root,
        &mut cli_calls,
    );
    assert!(stderr["content"]
        .as_str()
        .unwrap()
        .contains("M1_STDERR diagnostic"));

    let generated = cli_ok(
        "workspace-read",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "m1-output.txt",
            "maxBytes": 1024
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(
        generated["content"],
        "marker=True;hostWrite=denied;systemControl=denied"
    );
    assert!(!Path::new("/etc/ordivon-m1-denied").exists());
    let diff = cli_ok(
        "workspace-diff",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "maxBytes": 1048576
        }),
        &store_root,
        &mut cli_calls,
    );
    assert!(diff["diff"]
        .as_str()
        .unwrap()
        .contains(&format!("M1 durable executor marker {unique}")));
    let untracked = diff["untrackedPaths"].as_array().unwrap();
    assert!(untracked.iter().any(|path| path == "m1_tool.py"));
    assert!(untracked.iter().any(|path| path == "m1-output.txt"));

    cli_ok(
        "workspace-write",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "m1_cancel.py",
            "content": "import time\nprint('cancel target started', flush=True)\ntime.sleep(30)\n",
            "expectedDigest": null
        }),
        &store_root,
        &mut cli_calls,
    );
    let cancel_start = cli_ok(
        "task-start",
        json!({
            "schemaVersion": 1,
            "taskId": cancel_task_id,
            "workspaceId": sandbox.workspace_id,
            "executable": fs::canonicalize("/usr/bin/python3").unwrap(),
            "args": ["m1_cancel.py"],
            "cwdRelative": ".",
            "env": {"PYTHONUNBUFFERED": "1"},
            "timeoutMs": 60000,
            "stdoutLimitBytes": 65536,
            "stderrLimitBytes": 65536
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(cancel_start["status"], "WORKING");
    thread::sleep(Duration::from_millis(200));
    let cancelled = cli_ok(
        "task-cancel",
        json!({"schemaVersion": 1, "taskId": cancel_task_id}),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(cancelled["status"], "CANCELLED");

    let elapsed_ms = started.elapsed().as_millis();
    if let Ok(evidence_path) = std::env::var("ORDIVON_M1_EVIDENCE_PATH") {
        let evidence = json!({
            "schemaVersion": 1,
            "phase": "M1",
            "workspaceId": sandbox.workspace_id,
            "sourceRevision": create["sourceRevision"],
            "completedTaskId": complete_task_id,
            "completedStatus": completed["status"],
            "completedArtifacts": completed["artifacts"],
            "cancelledTaskId": cancel_task_id,
            "cancelledStatus": cancelled["status"],
            "workspaceAfterDigest": write["afterDigest"],
            "generatedOutputDigest": generated["digest"],
            "diffDigest": diff["digest"],
            "untrackedPaths": diff["untrackedPaths"],
            "stdoutDigest": stdout["digest"],
            "stderrDigest": stderr["digest"],
            "hostWriteDenied": true,
            "systemControlDenied": true,
            "elapsedMs": elapsed_ms,
            "cliCalls": cli_calls,
            "longPollingEnabled": true,
            "legacyComparison": "pending_m2"
        });
        fs::write(evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    eprintln!(
        "M1_METRICS elapsedMs={} cliCalls={} completedTask={} cancelledTask={} storeRoot={}",
        elapsed_ms,
        cli_calls,
        complete_task_id,
        cancel_task_id,
        store_root.display()
    );
}

fn poll_terminal(task_id: &str, store_root: &Path, cli_calls: &mut u64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (success, body) = cli_raw(
            "task-get",
            json!({"schemaVersion": 1, "taskId": task_id, "waitMs": 5000}),
            store_root,
            cli_calls,
        );
        if success {
            let result = body["result"].clone();
            if result["status"] != "WORKING" {
                return result;
            }
        } else {
            assert_eq!(body["error"]["code"], "TASK_STATE_UNAVAILABLE");
        }
        assert!(
            Instant::now() < deadline,
            "task did not reach terminal state"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn cli_ok(command: &str, request: Value, store_root: &Path, cli_calls: &mut u64) -> Value {
    let (success, body) = cli_raw(command, request, store_root, cli_calls);
    assert!(success, "CLI {command} failed: {body}");
    body["result"].clone()
}

fn cli_raw(command: &str, request: Value, store_root: &Path, cli_calls: &mut u64) -> (bool, Value) {
    *cli_calls += 1;
    let mut child = Command::new(env!("CARGO_BIN_EXE_ordivon-m1-cli"))
        .arg(command)
        .env("ORDIVON_M1_STORE_ROOT", store_root)
        .env(
            "ORDIVON_M1_RUNNER_PATH",
            env!("CARGO_BIN_EXE_ordivon-task-runner"),
        )
        .env("ORDIVON_M1_ALLOWED_EXECUTABLE_ROOTS", "/usr/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&request).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let body: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid CLI JSON: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), body)
}

fn executor_config(store_root: &Path) -> UniversalExecutorConfig {
    UniversalExecutorConfig {
        store_root: store_root.to_path_buf(),
        runner_path: PathBuf::from(env!("CARGO_BIN_EXE_ordivon-task-runner")),
        allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
        max_runtime_ms: 900_000,
        max_output_bytes: 16 * 1024 * 1024,
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn require_opt_in() {
    assert_eq!(std::env::var("ORDIVON_RUN_M1").as_deref(), Ok("1"));
    assert_eq!(
        unsafe { libc::geteuid() },
        0,
        "M1 systemd test requires root"
    );
    assert_eq!(
        fs::read_to_string("/proc/1/comm").unwrap().trim(),
        "systemd"
    );
    let output = Command::new("stat")
        .args(["-fc", "%T", "/sys/fs/cgroup"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "cgroup2fs"
    );
}
