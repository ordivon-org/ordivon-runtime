#![cfg(feature = "universal-executor-m1")]

use ordivon_exec::{remove_git_workspace, UniversalExecutorConfig};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct M3Sandbox {
    store_root: PathBuf,
    workspace_id: String,
    task_ids: Vec<String>,
}

impl Drop for M3Sandbox {
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
fn compact_task_run_completes_in_six_model_facing_calls() {
    require_m3_opt_in();
    let started = Instant::now();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let store_root = PathBuf::from(format!(
        "/root/.local/share/ordivon-m3-tests/{}-{unique}",
        std::process::id()
    ));
    let source_repo = repository_root();
    let workspace_id = format!("m3-workspace-{}-{unique}", std::process::id());
    let task_id = format!("m3-task-{}-{unique}", std::process::id());
    let sandbox = M3Sandbox {
        store_root: store_root.clone(),
        workspace_id: workspace_id.clone(),
        task_ids: vec![task_id.clone()],
    };
    let mut cli_calls = 0_u64;

    let create = cli_ok(
        "workspace-open",
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

    let slice = cli_ok(
        "workspace-read-slice-compact",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "crates/ordivon-exec/README.md",
            "offset": 0,
            "maxBytes": 64
        }),
        &store_root,
        &mut cli_calls,
    );
    let marker = format!("M3 compact executor marker {unique}");
    let script = format!(
        "from pathlib import Path\nimport sys,time\ntext=Path('crates/ordivon-exec/README.md').read_text()\nPath('m3-output.txt').write_text('marker=' + str({marker:?} in text))\nprint('M3_STDOUT compact')\nprint('M3_STDERR diagnostic', file=sys.stderr)\ntime.sleep(0.3)\n"
    );
    let mutation = cli_ok(
        "workspace-mutate",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "mutations": [
                {
                    "relativePath": "crates/ordivon-exec/README.md",
                    "mode": "APPEND",
                    "content": format!("\n{marker}\n"),
                    "expectedDigest": slice["fileDigest"]
                },
                {
                    "relativePath": "m3_tool.py",
                    "mode": "WRITE",
                    "content": script,
                    "expectedDigest": null
                }
            ]
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(mutation["mutations"].as_array().unwrap().len(), 2);

    let compact = cli_ok(
        "task-run",
        json!({
            "schemaVersion": 1,
            "execution": {
                "schemaVersion": 1,
                "taskId": task_id,
                "workspaceId": sandbox.workspace_id,
                "executable": fs::canonicalize("/usr/bin/python3").unwrap(),
                "args": ["m3_tool.py"],
                "cwdRelative": ".",
                "env": {"PYTHONUNBUFFERED": "1"},
                "timeoutMs": 10000,
                "stdoutLimitBytes": 65536,
                "stderrLimitBytes": 65536
            },
            "waitMs": 5000,
            "stdoutTailBytes": 1024,
            "stderrTailBytes": 1024
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(compact["status"], "COMPLETED");
    assert_eq!(compact["exitCode"], 0);
    assert!(compact["stdoutTail"]
        .as_str()
        .unwrap()
        .contains("M3_STDOUT compact"));
    assert!(compact["stderrTail"]
        .as_str()
        .unwrap()
        .contains("M3_STDERR diagnostic"));
    assert_eq!(compact["artifactsAvailable"], true);

    let generated = cli_ok(
        "workspace-read-slice-compact",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "relativePath": "m3-output.txt",
            "offset": 0,
            "maxBytes": 1024
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(generated["content"], "marker=True");
    let diff = cli_ok(
        "workspace-diff-compact",
        json!({
            "schemaVersion": 1,
            "workspaceId": sandbox.workspace_id,
            "maxBytes": 1048576
        }),
        &store_root,
        &mut cli_calls,
    );
    assert!(diff["diff"].as_str().unwrap().contains(&marker));
    assert!(diff["untrackedPaths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == "m3_tool.py"));
    assert!(diff["untrackedPaths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == "m3-output.txt"));
    assert_eq!(cli_calls, 6, "the primary M3 journey must use six calls");

    let recovered = cli_ok(
        "task-await",
        json!({
            "schemaVersion": 1,
            "taskId": task_id,
            "waitMs": 0,
            "stdoutTailBytes": 64,
            "stderrTailBytes": 64
        }),
        &store_root,
        &mut cli_calls,
    );
    assert_eq!(recovered["status"], "COMPLETED");
    assert_eq!(recovered["taskId"], task_id);

    eprintln!(
        "M3_METRICS elapsedMs={} journeyCalls=6 recoveryCalls=1 task={} storeRoot={}",
        started.elapsed().as_millis(),
        task_id,
        store_root.display()
    );
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
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
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

fn require_m3_opt_in() {
    assert_eq!(std::env::var("ORDIVON_RUN_M3").as_deref(), Ok("1"));
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
