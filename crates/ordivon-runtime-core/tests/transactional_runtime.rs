#![cfg(feature = "transactional-runtime")]

use ordivon_runtime_core::{
    create_git_workspace, remove_git_workspace, write_workspace_text, ArtifactReadRequest,
    AttemptState, ExecutionBudget, ForeignReference, GitWorkspaceCreateRequest, InputAuthority,
    InputBindingRequest, RegistryConfig, Runtime, RuntimeConfig, RuntimeExecutionPlan,
    RuntimeJobListRequest, SubmitRequest, TaskCancelRequest, TaskObserveRequest,
    TaskObserveWaitUntil, TaskRunRequest, UniversalExecutionRequest, UniversalExecutorConfig,
    WindowsExecutionConfig, WorkspaceCloseRequest, WorkspaceMutateRequest, WorkspaceMutation,
    WorkspaceMutationMode, WorkspaceWriteRequest, RUNTIME_SCHEMA_VERSION,
    UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
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

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(path.is_file(), "{} was not written in time", path.display());
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
        max_runtime_ms: 24 * 60 * 60 * 1000,
        max_output_bytes: 64 * 1024 * 1024,
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
        registry: ordivon_runtime_core::RegistryConfig {
            db_path: root.join("registry/registry.sqlite3"),
            store_root: root.join("registry"),
            busy_timeout_ms: 5000,
        },
        executor: executor.clone(),
        startup_grace_ms: 2000,
        windows: None,
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
            steps: Vec::new(),
            budget: ordivon_runtime_core::ExecutionBudget::default(),
            execution_profile: ordivon_runtime_core::ExecutionProfile::TrustedLocal,
            execution_target: ordivon_runtime_core::ExecutionTarget::LocalLinux,
            foreign_references: Vec::new(),
        },
        wait_ms: 30_000,
        stdout_tail_bytes: 4096,
        stderr_tail_bytes: 4096,
    };
    let first = runtime.run_task(&request).unwrap();
    assert_eq!(first.status, "succeeded");
    assert!(first.stdout_tail.contains("RUNTIME_OK"));
    let stdout_descriptor = first
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == "stdout")
        .unwrap();
    assert_eq!(
        stdout_descriptor.artifact_id,
        format!("{}.stdout", first.attempt_id.as_deref().unwrap())
    );
    assert_eq!(stdout_descriptor.dropped_bytes, Some(0));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);

    let replay = runtime.run_task(&request).unwrap();
    assert_eq!(replay.job_id, first.job_id);
    assert_eq!(replay.status, "succeeded");

    let listed = runtime
        .list_jobs(&RuntimeJobListRequest {
            limit: 10,
            cursor: None,
            client_request_id: None,
            workspace_id: None,
        })
        .unwrap();
    assert_eq!(listed.jobs.len(), 1);
    assert_eq!(listed.jobs[0].job_id, first.job_id);
    assert_eq!(listed.jobs[0].client_request_id, request.client_request_id);
    assert_eq!(listed.jobs[0].workspace_id, workspace_id);
    assert_eq!(listed.jobs[0].executable_name, "python3.14");
    assert_eq!(listed.jobs[0].artifact_count, 4);
    let artifacts = runtime.registry().list_artifacts(&first.job_id).unwrap();
    let artifact_kinds = artifacts
        .iter()
        .map(|artifact| artifact.kind.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        artifact_kinds,
        std::collections::BTreeSet::from([
            "execution_result",
            "stderr",
            "stdout",
            "terminal_evidence",
        ])
    );
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
    remove_git_workspace(
        &executor,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.clone(),
            force: true,
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    fs::remove_dir_all(&root).unwrap();
}

#[test]
#[ignore = "requires WSL, Windows interop, root/systemd, .NET csc, and explicit local opt-in"]
fn runtime_windows_native_executes_as_real_job_attempt_and_replays() {
    if std::env::var("ORDIVON_RUN_WINDOWS_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = fs::canonicalize(repo).unwrap();
    let revision = command_output("git", &["rev-parse", "HEAD"], &repo);
    let csc = PathBuf::from("/mnt/c/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe");
    assert!(csc.is_file(), "{} is unavailable", csc.display());
    let public_root = PathBuf::from("/mnt/c/Users/Public")
        .join(format!("ordivon-runtime-rw1-{}", Uuid::now_v7()));
    fs::create_dir(&public_root).unwrap();
    let launcher_source = public_root.join("Ordivon.WindowsJobLauncher.cs");
    let fixture_source = public_root.join("Ordivon.WindowsJobFixture.cs");
    let launcher = public_root.join("ordivon-windows-job-launcher.exe");
    let fixture = public_root.join("ordivon-windows-job-fixture.exe");
    fs::copy(
        repo.join("platform/windows/Ordivon.WindowsJobLauncher.cs"),
        &launcher_source,
    )
    .unwrap();
    fs::copy(
        repo.join("platform/windows/Ordivon.WindowsJobFixture.cs"),
        &fixture_source,
    )
    .unwrap();

    fn windows_drive_path(path: &Path) -> String {
        let text = path.to_str().unwrap();
        let rest = text
            .strip_prefix("/mnt/c/")
            .expect("test path must be on C:");
        format!("C:\\{}", rest.replace('/', "\\"))
    }
    for (source, output) in [(&launcher_source, &launcher), (&fixture_source, &fixture)] {
        let compiled = Command::new(&csc)
            .args([
                "/nologo".to_string(),
                "/optimize+".to_string(),
                format!("/out:{}", windows_drive_path(output)),
                windows_drive_path(source),
            ])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
    }

    let root = PathBuf::from("/root/.local/share/ordivon-windows-integration")
        .join(Uuid::now_v7().to_string());
    let store = root.join("store");
    let executor = UniversalExecutorConfig {
        store_root: store.clone(),
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
        runner_path: PathBuf::from("/usr/bin/true"),
        allowed_executable_roots: vec![public_root.clone()],
        max_runtime_ms: 60_000,
        max_output_bytes: 1024 * 1024,
    };
    executor.ensure_store().unwrap();
    let workspace_id = format!("runtime-windows-it-{}", Uuid::now_v7());
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
    let runtime = Runtime::new(RuntimeConfig {
        registry: RegistryConfig {
            db_path: root.join("registry/registry.sqlite3"),
            store_root: root.join("registry"),
            busy_timeout_ms: 5_000,
        },
        executor: executor.clone(),
        startup_grace_ms: 2_000,
        windows: Some(WindowsExecutionConfig {
            launcher_path: launcher.clone(),
            wsl_distribution: std::env::var("WSL_DISTRO_NAME")
                .unwrap_or_else(|_| "archlinux".to_string()),
        }),
    })
    .unwrap();
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: format!("request:windows-rw1:{}", Uuid::now_v7()),
        principal: "principal:windows-integration".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: workspace_id.clone(),
            executable: fixture.to_string_lossy().into_owned(),
            args: vec![
                "echo".to_string(),
                "rw1-arg with space".to_string(),
                "quote\"arg".to_string(),
            ],
            cwd_relative: ".".to_string(),
            env: BTreeMap::from([("W1_ENV".to_string(), "runtime-w1".to_string())]),
            timeout_ms: 20_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            steps: Vec::new(),
            budget: ExecutionBudget {
                memory_max_bytes: Some(128 * 1024 * 1024),
                tasks_max: Some(4),
                cpu_quota_percent: Some(100),
            },
            execution_profile: ordivon_runtime_core::ExecutionProfile::TrustedLocal,
            execution_target: ordivon_runtime_core::ExecutionTarget::WindowsNative,
            foreign_references: Vec::new(),
        },
        wait_ms: 30_000,
        stdout_tail_bytes: 16_384,
        stderr_tail_bytes: 16_384,
    };
    let first = runtime.run_task(&request).unwrap();
    assert_eq!(first.status, "succeeded", "{}", first.stderr_tail);
    assert!(first.execution_terminal);
    assert_eq!(first.exit_code, Some(0));
    assert!(first
        .stdout_tail
        .contains("W1_ECHO_ENV_B64=cnVudGltZS13MQ=="));
    assert!(first
        .stdout_tail
        .contains("W1_ECHO_SYSTEMROOT_B64=PG51bGw+"));
    assert!(first.stdout_tail.contains("W1_ECHO_ARGC=2"));
    let attempt_id = first.attempt_id.clone().unwrap();
    let artifacts = runtime.registry().list_artifacts(&first.job_id).unwrap();
    assert!(artifacts
        .iter()
        .any(|artifact| artifact.kind == "windows_start"));
    let windows_start = artifacts
        .iter()
        .find(|artifact| artifact.kind == "windows_start")
        .unwrap();
    let evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: first.job_id.clone(),
            artifact_id: windows_start.artifact_id.clone(),
            offset: 0,
            max_bytes: 65_536,
        })
        .unwrap();
    let windows_evidence: serde_json::Value = serde_json::from_str(&evidence.content).unwrap();
    assert_eq!(windows_evidence["jobId"], first.job_id);
    assert_eq!(windows_evidence["attemptId"], attempt_id);
    assert_eq!(windows_evidence["imageDigest"], file_digest(&fixture));
    assert!(windows_evidence["processId"].as_u64().unwrap() > 0);
    assert!(
        windows_evidence["processCreationTimeFileTime"]
            .as_u64()
            .unwrap()
            > 0
    );

    let terminal = artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let terminal_evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: first.job_id.clone(),
            artifact_id: terminal.artifact_id.clone(),
            offset: 0,
            max_bytes: 65_536,
        })
        .unwrap();
    let terminal_json: serde_json::Value =
        serde_json::from_str(&terminal_evidence.content).unwrap();
    assert_eq!(terminal_json["executionTarget"], "windows_native");
    assert!(terminal_json["terminalArtifactIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == &serde_json::Value::String(format!("{attempt_id}.windows-start"))));

    let replay = runtime.run_task(&request).unwrap();
    assert_eq!(replay.job_id, first.job_id);
    assert_eq!(replay.attempt_id, first.attempt_id);
    assert_eq!(replay.status, "succeeded");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    println!(
        "RW1_WINDOWS_RUNTIME jobId={} attemptId={} windowsPid={} creationFileTime={} imageDigest={} artifacts={}",
        first.job_id,
        attempt_id,
        windows_evidence["processId"].as_u64().unwrap(),
        windows_evidence["processCreationTimeFileTime"].as_u64().unwrap(),
        windows_evidence["imageDigest"].as_str().unwrap(),
        artifacts.len(),
    );

    let timeout_marker = format!("ORDIVON_RW2_TIMEOUT_{}", Uuid::now_v7());
    let mut timeout_request = request.clone();
    timeout_request.client_request_id = format!("request:windows-timeout:{}", Uuid::now_v7());
    timeout_request.execution.args = vec!["tree".to_string(), timeout_marker.clone()];
    timeout_request.execution.timeout_ms = 300;
    timeout_request.execution.budget.tasks_max = Some(8);
    let timeout_started = Instant::now();
    let timed_out = runtime.run_task(&timeout_request).unwrap();
    assert_eq!(timed_out.status, "timed_out", "{}", timed_out.stderr_tail);
    assert!(timeout_started.elapsed() < Duration::from_secs(3));
    assert!(timed_out.execution_terminal);
    assert!(timed_out
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == "windows_start"));
    assert!(timed_out
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == "execution_result"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);

    thread::sleep(Duration::from_millis(500));
    let process_probe = format!(
        "$m='{}'; $rows=Get-CimInstance Win32_Process | Where-Object {{$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}}; Write-Output @($rows).Count",
        timeout_marker
    );
    let remaining = Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
        .args(["-NoProfile", "-Command", &process_probe])
        .output()
        .unwrap();
    assert!(
        remaining.status.success(),
        "{}",
        String::from_utf8_lossy(&remaining.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&remaining.stdout).trim(), "0");
    println!(
        "RW2_WINDOWS_TIMEOUT jobId={} attemptId={} elapsedMs={} remaining=0",
        timed_out.job_id,
        timed_out.attempt_id.as_deref().unwrap(),
        timeout_started.elapsed().as_millis(),
    );

    let timeout_unit = format!(
        "ordivon-{}.service",
        timed_out.attempt_id.as_deref().unwrap()
    );
    let _ = Command::new("systemctl")
        .args(["stop", &timeout_unit])
        .output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &timeout_unit])
        .output();

    let cancel_marker = format!("ORDIVON_RW2_CANCEL_{}", Uuid::now_v7());
    let mut cancel_request = request.clone();
    cancel_request.client_request_id = format!("request:windows-cancel:{}", Uuid::now_v7());
    cancel_request.execution.args = vec!["tree".to_string(), cancel_marker.clone()];
    cancel_request.execution.timeout_ms = 20_000;
    cancel_request.execution.budget.tasks_max = Some(8);
    cancel_request.wait_ms = 0;
    let started_cancel = runtime.run_task(&cancel_request).unwrap();
    assert!(matches!(
        started_cancel.status.as_str(),
        "queued" | "working"
    ));
    let running_deadline = Instant::now() + Duration::from_secs(10);
    let cancel_attempt = loop {
        let attempt = runtime
            .registry()
            .get_latest_attempt(&started_cancel.job_id)
            .unwrap()
            .unwrap();
        if attempt.state == AttemptState::Running
            && attempt.control_group.is_some()
            && attempt.invocation_id.is_some()
        {
            break attempt;
        }
        assert!(
            Instant::now() < running_deadline,
            "Windows cancel Attempt did not become running"
        );
        thread::sleep(Duration::from_millis(25));
    };
    let cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started_cancel.job_id.clone(),
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(cancelled.execution_terminal);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);

    thread::sleep(Duration::from_millis(500));
    let cancel_probe = format!(
        "$m='{}'; $rows=Get-CimInstance Win32_Process | Where-Object {{$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}}; Write-Output @($rows).Count",
        cancel_marker
    );
    let cancel_remaining =
        Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
            .args(["-NoProfile", "-Command", &cancel_probe])
            .output()
            .unwrap();
    assert!(
        cancel_remaining.status.success(),
        "{}",
        String::from_utf8_lossy(&cancel_remaining.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&cancel_remaining.stdout).trim(),
        "0"
    );
    println!(
        "RW2_WINDOWS_CANCEL jobId={} attemptId={} remaining=0",
        started_cancel.job_id, cancel_attempt.attempt_id,
    );
    let cancel_unit = format!("ordivon-{}.service", cancel_attempt.attempt_id);
    let _ = Command::new("systemctl")
        .args(["stop", &cancel_unit])
        .output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &cancel_unit])
        .output();

    let unit = format!("ordivon-{attempt_id}.service");
    let _ = Command::new("systemctl").args(["stop", &unit]).output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &unit])
        .output();
    remove_git_workspace(
        &executor,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id,
            force: true,
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    fs::remove_dir_all(&root).unwrap();
    fs::remove_dir_all(&public_root).unwrap();
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_immutable_input_freezes_bytes_and_replays_without_source_authority() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("immutable-input-replay");
    context.write(
        "input_probe.py",
        r#"import json, os, time
from pathlib import Path
root = Path(os.environ["ORDIVON_INPUT_ROOT"])
path = root / "finance" / "data.txt"
write_result = "unexpected-success"
try:
    path.write_text("PAYLOAD-MUTATION\n")
except Exception as error:
    write_result = f"{type(error).__name__}:{error}"
time.sleep(1.2)
print(json.dumps({"input": path.read_text().strip(), "write": write_result}, sort_keys=True), flush=True)
"#,
    );

    let authority_root = context.root.join("finance-authority");
    fs::create_dir_all(&authority_root).unwrap();
    let source = authority_root.join("data.txt");
    fs::write(&source, b"S0\n").unwrap();
    let expected_digest = digest(b"S0\n");
    let inputs = vec![InputBindingRequest {
        authority: "finance".to_string(),
        relative_object: "data.txt".to_string(),
        expected_digest: expected_digest.clone(),
        presentation_relative_path: "finance/data.txt".to_string(),
    }];
    let authorities = vec![InputAuthority {
        name: "finance".to_string(),
        root: authority_root.clone(),
    }];

    let runtime = context.runtime_with_input_authorities(2_000, authorities);
    let mut request = context.request("input_probe.py", 0);
    request.client_request_id = format!("request:immutable-input:{}", Uuid::now_v7());
    request.execution.execution_profile = ordivon_runtime_core::ExecutionProfile::ContainedLocal;
    let submitted = runtime.run_task_with_inputs(&request, &inputs).unwrap();
    fs::write(&source, b"S1\n").unwrap();

    let final_observation = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: submitted.job_id.clone(),
            wait_ms: 30_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 16_384,
            stderr_tail_bytes: 16_384,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(final_observation.status, "succeeded");
    let stdout: serde_json::Value =
        serde_json::from_str(final_observation.stdout_tail.trim()).unwrap();
    assert_eq!(stdout["input"], "S0");
    assert!(stdout["write"]
        .as_str()
        .unwrap()
        .contains("Read-only file system"));
    assert_eq!(fs::read_to_string(&source).unwrap(), "S1\n");

    let terminal = final_observation
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: final_observation.job_id.clone(),
            artifact_id: terminal.artifact_id.clone(),
            offset: 0,
            max_bytes: 65_536,
        })
        .unwrap();
    let evidence_text = evidence.content.clone();
    let evidence: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();
    assert!(evidence["inputSetId"].as_str().is_some());
    assert!(!evidence_text.contains(&authority_root.to_string_lossy().into_owned()));
    assert!(!evidence_text.contains("input-materializations/"));
    let bindings = evidence["effectiveInputs"].as_array().unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0]["authority"], "finance");
    assert_eq!(bindings[0]["digest"], expected_digest);
    assert_eq!(bindings[0]["presentationRelativePath"], "finance/data.txt");
    assert_eq!(bindings[0]["access"], "read_only");

    let owned_root = context.executor.job_input_path(&final_observation.job_id);
    assert_eq!(
        fs::read(owned_root.join("finance/data.txt")).unwrap(),
        b"S0\n"
    );
    assert!(fs::read_dir(context.executor.input_materializations_root())
        .unwrap()
        .filter_map(Result::ok)
        .all(|entry| entry.file_name().to_string_lossy().starts_with('.')));

    drop(runtime);
    fs::remove_dir_all(&authority_root).unwrap();
    // Durable replay must not consult current authority or require it to be configured.
    let restarted = context.runtime(2_000);
    let replay = restarted.run_task_with_inputs(&request, &inputs).unwrap();
    assert_eq!(replay.job_id, final_observation.job_id);
    assert_eq!(replay.status, "succeeded");

    let mut changed_inputs = inputs.clone();
    changed_inputs[0].expected_digest = digest(b"different-input\n");
    let conflict = restarted
        .run_task_with_inputs(&request, &changed_inputs)
        .unwrap_err();
    assert_eq!(
        conflict.code,
        ordivon_runtime_core::RuntimeErrorCode::IdempotencyConflict
    );
    assert_eq!(restarted.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_failed_capacity_admission_discards_prepared_state_and_rechecks_current_authority() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("immutable-input-capacity-cleanup");
    let authority_root = context.root.join("authority");
    fs::create_dir_all(&authority_root).unwrap();
    let source = authority_root.join("input.bin");
    fs::write(&source, b"AUTHORIZED-S0").unwrap();
    let runtime = context.runtime_with_input_authorities(
        2_000,
        vec![InputAuthority {
            name: "finance-state".to_string(),
            root: authority_root.clone(),
        }],
    );
    let mut holder = context.request("input-capacity-holder", 0);
    holder.execution.executable = "/usr/bin/sleep".to_string();
    holder.execution.args = vec!["5".to_string()];
    let holder = runtime.run_task(&holder).unwrap();
    assert!(!holder.execution_terminal);
    let inputs = vec![InputBindingRequest {
        authority: "finance-state".to_string(),
        relative_object: "input.bin".to_string(),
        expected_digest: file_digest(&source),
        presentation_relative_path: "state/input.bin".to_string(),
    }];
    let mut blocked = context.request("input-capacity-blocked", 0);
    blocked.client_request_id = format!("request:input-capacity-blocked:{}", Uuid::now_v7());
    blocked.execution.execution_profile = ordivon_runtime_core::ExecutionProfile::ContainedLocal;
    blocked.execution.executable = "/usr/bin/true".to_string();
    blocked.execution.args.clear();
    let error = runtime.run_task_with_inputs(&blocked, &inputs).unwrap_err();
    assert_eq!(
        error.code,
        ordivon_runtime_core::RuntimeErrorCode::ConcurrencyLimit
    );
    assert!(fs::read_dir(context.executor.input_materializations_root())
        .unwrap()
        .filter_map(Result::ok)
        .all(|entry| entry.file_name().to_string_lossy().starts_with('.')));
    fs::write(&source, b"AUTHORIZED-S1").unwrap();
    runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: holder.job_id.clone(),
        })
        .unwrap();
    runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: holder.job_id,
            wait_ms: 10_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 1024,
            stderr_tail_bytes: 1024,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    let drift = runtime.run_task_with_inputs(&blocked, &inputs).unwrap_err();
    assert_eq!(
        drift.code,
        ordivon_runtime_core::RuntimeErrorCode::InvalidRequest
    );
    assert_eq!(drift.field.as_deref(), Some("inputs[0].expectedDigest"));
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_opened_input_authority_survives_configured_path_replacement() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("immutable-input-authority-capability");
    let authority_path = context.root.join("authority");
    let outside = context.root.join("outside");
    fs::create_dir_all(&authority_path).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(authority_path.join("data.bin"), b"ALLOWED").unwrap();
    fs::write(outside.join("data.bin"), b"SECRET").unwrap();
    let runtime = context.runtime_with_input_authorities(
        2_000,
        vec![InputAuthority {
            name: "finance-state".to_string(),
            root: authority_path.clone(),
        }],
    );
    let original = context.root.join("authority-original");
    fs::rename(&authority_path, &original).unwrap();
    std::os::unix::fs::symlink(&outside, &authority_path).unwrap();
    let mut request = context.request("authority-capability", 30_000);
    request.client_request_id = format!("request:authority-capability:{}", Uuid::now_v7());
    request.execution.execution_profile = ordivon_runtime_core::ExecutionProfile::ContainedLocal;
    request.execution.executable = "/usr/bin/python3.14".to_string();
    request.execution.args = vec![
        "-c".to_string(),
        "from pathlib import Path; import os; print((Path(os.environ['ORDIVON_INPUT_ROOT'])/'data.bin').read_text())".to_string(),
    ];
    let inputs = vec![InputBindingRequest {
        authority: "finance-state".to_string(),
        relative_object: "data.bin".to_string(),
        expected_digest: digest(b"ALLOWED"),
        presentation_relative_path: "data.bin".to_string(),
    }];
    let terminal = runtime.run_task_with_inputs(&request, &inputs).unwrap();
    assert_eq!(terminal.status, "succeeded", "{}", terminal.stderr_tail);
    assert_eq!(terminal.stdout_tail.trim(), "ALLOWED");
    assert_eq!(
        fs::read(
            context
                .executor
                .job_input_path(&terminal.job_id)
                .join("data.bin")
        )
        .unwrap(),
        b"ALLOWED"
    );
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
            max_runtime_ms: 24 * 60 * 60 * 1000,
            max_output_bytes: 64 * 1024 * 1024,
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
            windows: None,
        })
        .unwrap()
    }

    fn runtime_with_input_authorities(
        &self,
        startup_grace_ms: u64,
        authorities: Vec<InputAuthority>,
    ) -> Runtime {
        Runtime::new_with_input_authorities(
            RuntimeConfig {
                registry: self.registry.clone(),
                executor: self.executor.clone(),
                startup_grace_ms,
                windows: None,
            },
            authorities,
        )
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
                steps: Vec::new(),
                budget: ordivon_runtime_core::ExecutionBudget::default(),
                execution_profile: ordivon_runtime_core::ExecutionProfile::TrustedLocal,
                execution_target: ordivon_runtime_core::ExecutionTarget::LocalLinux,
                foreign_references: Vec::new(),
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
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_two_observers_do_not_race_dispatch_of_one_accepted_attempt() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("cross-runtime-observe-race");
    let seed = context.runtime(2_000);
    let created = created_admission(
        seed.registry()
            .submit(&context.direct_submit("request:cross-runtime-observe-race", 8))
            .unwrap(),
    );
    assert_eq!(created.attempt.state, AttemptState::Accepted);
    drop(seed);

    let runtime_a = context.runtime(2_000);
    let runtime_b = context.runtime(2_000);
    let barrier = Arc::new(Barrier::new(3));
    let request = TaskObserveRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        job_id: created.job.job_id.clone(),
        wait_ms: 0,
        wait_until: TaskObserveWaitUntil::Terminal,
        stdout_tail_bytes: 4096,
        stderr_tail_bytes: 4096,
        stdout_offset: None,
        stderr_offset: None,
    };
    let spawn = |runtime: Runtime| {
        let barrier = Arc::clone(&barrier);
        let request = request.clone();
        thread::spawn(move || {
            barrier.wait();
            runtime.observe_task(&request)
        })
    };
    let observer_a = spawn(runtime_a);
    let observer_b = spawn(runtime_b);
    barrier.wait();
    let result_a = observer_a.join().unwrap();
    let result_b = observer_b.join().unwrap();

    assert!(result_a.is_ok(), "observer A failed: {result_a:?}");
    assert!(result_b.is_ok(), "observer B failed: {result_b:?}");
    let latest = context
        .runtime(2_000)
        .registry()
        .get_attempt(&created.attempt.attempt_id)
        .unwrap();
    assert!(matches!(
        latest.state,
        AttemptState::Starting | AttemptState::Running | AttemptState::Succeeded
    ));
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_timeout_preserves_result_when_descendants_hold_output_pipes() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("timeout-descendant-pipes");
    let runtime = context.runtime(2_000);
    let mut request = context.request("timeout-descendant-pipes", 10_000);
    request.execution.executable = "/usr/bin/bash".to_string();
    request.execution.args = vec!["-lc".to_string(), "sleep 5 & wait".to_string()];
    request.execution.timeout_ms = 100;
    let started = Instant::now();
    let result = runtime.run_task(&request).unwrap();
    assert_eq!(result.status, "timed_out");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(result
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == "execution_result"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn contained_local_hides_unmounted_state_blocks_egress_and_preserves_evidence() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let runner_path =
        PathBuf::from(std::env::var("ORDIVON_RUNNER_PATH").expect("ORDIVON_RUNNER_PATH"));
    let repo = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let revision = command_output("git", &["rev-parse", "HEAD"], &repo);
    let root =
        PathBuf::from("/var/lib/ordivon-contained-integration").join(Uuid::now_v7().to_string());
    fs::create_dir_all(&root).unwrap();
    let secret_path = root.join("unmounted-secret.txt");
    fs::write(&secret_path, "MUST_NOT_BE_VISIBLE").unwrap();
    let executor = UniversalExecutorConfig {
        store_root: root.join("store"),
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
        runner_path,
        allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
        max_runtime_ms: 24 * 60 * 60 * 1000,
        max_output_bytes: 64 * 1024 * 1024,
    };
    executor.ensure_store().unwrap();
    let workspace_id = format!("runtime-contained-{}", Uuid::now_v7());
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
            relative_path: "contained_probe.py".to_string(),
            content: format!(
                r#"import os
import pathlib
import socket
secret = pathlib.Path({secret:?})
print("SECRET_VISIBLE=" + str(secret.exists()), flush=True)
network = "connected"
sock = None
try:
    sock = socket.socket()
    sock.settimeout(0.5)
    sock.connect(("1.1.1.1", 53))
except OSError:
    network = "blocked"
finally:
    if sock is not None:
        sock.close()
print("NETWORK=" + network, flush=True)
print("GITHUB_TOKEN=" + str(os.environ.get("GITHUB_TOKEN")), flush=True)
pathlib.Path("contained-output.txt").write_text("ok")
print("WRITE_OK=" + pathlib.Path("contained-output.txt").read_text(), flush=True)
"#,
                secret = secret_path.to_string_lossy()
            ),
            expected_digest: None,
        },
    )
    .unwrap();
    let runtime = Runtime::new(RuntimeConfig {
        registry: RegistryConfig {
            db_path: root.join("registry/registry.sqlite3"),
            store_root: root.join("registry"),
            busy_timeout_ms: 5000,
        },
        executor: executor.clone(),
        startup_grace_ms: 5000,
        windows: None,
    })
    .unwrap();
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: format!("request:contained:{}", Uuid::now_v7()),
        principal: "principal:integration".to_string(),
        global_limit: 2,
        execution: UniversalExecutionRequest {
            workspace_id: workspace_id.clone(),
            executable: "/usr/bin/python3.14".to_string(),
            args: vec!["contained_probe.py".to_string()],
            cwd_relative: ".".to_string(),
            env: Default::default(),
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ordivon_runtime_core::ExecutionProfile::ContainedLocal,
            execution_target: ordivon_runtime_core::ExecutionTarget::LocalLinux,
            foreign_references: vec![ordivon_runtime_core::ForeignReference {
                namespace: "ordivon.edge".to_string(),
                reference_type: "supervisor_generation".to_string(),
                id: "contained-integration-supervisor".to_string(),
                generation: Some("1".to_string()),
                digest: None,
            }],
        },
        wait_ms: 30_000,
        stdout_tail_bytes: 8192,
        stderr_tail_bytes: 8192,
    };
    let result = runtime.run_task(&request).unwrap();
    assert_eq!(result.status, "succeeded", "{}", result.stderr_tail);
    assert!(result.stdout_tail.contains("SECRET_VISIBLE=False"));
    assert!(result.stdout_tail.contains("NETWORK=blocked"));
    assert!(result.stdout_tail.contains("GITHUB_TOKEN=None"));
    assert!(result.stdout_tail.contains("WRITE_OK=ok"));
    let evidence = result
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: result.job_id.clone(),
            artifact_id: evidence.artifact_id.clone(),
            offset: 0,
            max_bytes: 65_536,
        })
        .unwrap();
    let evidence: serde_json::Value = serde_json::from_str(&evidence.content).unwrap();
    assert_eq!(evidence["executionProfile"], "contained_local");
    assert_eq!(evidence["processTreeDisposition"], "terminal_clean");
    assert_eq!(
        evidence["foreignReferences"][0]["id"],
        "contained-integration-supervisor"
    );

    let attempt_id = result.attempt_id.unwrap();
    let unit = format!("ordivon-{attempt_id}.service");
    let _ = Command::new("systemctl").args(["stop", &unit]).output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &unit])
        .output();
    remove_git_workspace(
        &executor,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id,
            force: true,
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_replays_same_request_after_effect_changes_or_workspace_closure() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("request-replay-world-change");
    context.write(
        "source_identity.py",
        "from pathlib import Path\nPath('self-effect.txt').write_text('changed by command')\nprint('FIRST_WORLD', flush=True)\n",
    );
    let runtime = context.runtime(2_000);
    let request = context.request("source_identity.py", 30_000);
    let first = runtime.run_task(&request).unwrap();
    assert_eq!(first.status, "succeeded");
    assert!(first.stdout_tail.contains("FIRST_WORLD"));
    let job = runtime.registry().get_job(&first.job_id).unwrap();
    assert!(job.request_digest.starts_with("runtime-request-v1:"));
    let plan: RuntimeExecutionPlan = serde_json::from_str(&job.execution_plan_json).unwrap();
    let committed_source = plan
        .workspace_source_digest
        .clone()
        .expect("new Jobs must commit Workspace source state");
    let attempt = runtime
        .registry()
        .get_latest_attempt(&first.job_id)
        .unwrap()
        .unwrap();
    let runner_request: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&attempt.bundle_path).join("request.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        runner_request
            .get("workspaceSourceDigest")
            .and_then(serde_json::Value::as_str),
        Some(committed_source.as_str())
    );

    let replay_after_self_effect = runtime.run_task(&request).unwrap();
    assert_eq!(replay_after_self_effect.job_id, first.job_id);
    assert!(replay_after_self_effect.stdout_tail.contains("FIRST_WORLD"));

    let script = context
        .executor
        .store_root
        .join("workspaces")
        .join(&context.workspace_id)
        .join("source_identity.py");
    let expected_digest = file_digest(&script);
    runtime
        .mutate_workspace(&WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            mutations: vec![WorkspaceMutation {
                relative_path: "source_identity.py".to_string(),
                mode: WorkspaceMutationMode::Write,
                content: "print('SECOND_WORLD', flush=True)\n".to_string(),
                expected_digest: Some(expected_digest),
                expected_text: None,
            }],
        })
        .unwrap();
    let replay_after_later_mutation = runtime.run_task(&request).unwrap();
    assert_eq!(replay_after_later_mutation.job_id, first.job_id);
    assert!(replay_after_later_mutation
        .stdout_tail
        .contains("FIRST_WORLD"));

    let mut changed_request = request.clone();
    changed_request
        .execution
        .args
        .push("different-request".to_string());
    let error = runtime.run_task(&changed_request).unwrap_err();
    assert_eq!(
        error.code,
        ordivon_runtime_core::RuntimeErrorCode::IdempotencyConflict
    );

    runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    let replay_after_close = runtime.run_task(&request).unwrap();
    assert_eq!(replay_after_close.job_id, first.job_id);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_blocks_workspace_mutation_while_source_state_is_committed_by_active_job() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("active-source-commitment");
    context.write(
        "active_source.py",
        "import time\nprint('SOURCE_COMMITTED', flush=True)\ntime.sleep(30)\n",
    );
    let runtime = context.runtime(2_000);
    let request = context.request("active_source.py", 0);
    let started = runtime.run_task(&request).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let attempt = runtime
            .registry()
            .get_latest_attempt(&started.job_id)
            .unwrap()
            .unwrap();
        if attempt.state == AttemptState::Running {
            break;
        }
        assert!(Instant::now() < deadline, "Attempt did not become running");
        thread::sleep(Duration::from_millis(25));
    }

    let error = runtime
        .mutate_workspace(&WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            mutations: vec![WorkspaceMutation {
                relative_path: "should-not-exist.txt".to_string(),
                mode: WorkspaceMutationMode::Write,
                content: "blocked".to_string(),
                expected_digest: None,
                expected_text: None,
            }],
        })
        .unwrap_err();
    assert_eq!(
        error.code,
        ordivon_runtime_core::RuntimeErrorCode::WorkspaceBusy
    );
    assert!(!context
        .executor
        .store_root
        .join("workspaces")
        .join(&context.workspace_id)
        .join("should-not-exist.txt")
        .exists());

    let cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id,
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    runtime
        .mutate_workspace(&WorkspaceMutateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            mutations: vec![WorkspaceMutation {
                relative_path: "after-terminal.txt".to_string(),
                mode: WorkspaceMutationMode::Write,
                content: "allowed".to_string(),
                expected_digest: None,
                expected_text: None,
            }],
        })
        .unwrap();
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_systemd_path_rejects_source_drift_before_target_spawn() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("systemd-source-drift");
    context.write(
        "source_drift.py",
        "from pathlib import Path\nPath('effect-marker').write_text('spawned')\n",
    );
    let real_runner = context.executor.runner_path.clone();
    let delayed_runner = context.root.join("delayed-runner.sh");
    let workspace = context
        .executor
        .store_root
        .join("workspaces")
        .join(&context.workspace_id);
    fs::write(
        &delayed_runner,
        format!(
            "#!/bin/sh\nprintf 'trusted-host drift\n' > '{}/README.md'\nexec '{}' \"$@\"\n",
            workspace.display(),
            real_runner.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&delayed_runner).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&delayed_runner, permissions).unwrap();

    let mut executor = context.executor.clone();
    executor.runner_path = delayed_runner;
    let runtime = Runtime::new(RuntimeConfig {
        registry: context.registry.clone(),
        executor,
        startup_grace_ms: 10_000,
        windows: None,
    })
    .unwrap();
    let request = context.request("source_drift.py", 10_000);
    let observed = runtime.run_task(&request).unwrap();
    assert_eq!(observed.status, "failed");
    assert_eq!(observed.exit_code, None);
    assert!(observed
        .error_summary
        .as_deref()
        .is_some_and(|message| message.contains("WorkspaceStateMismatch")));
    assert!(!workspace.join("effect-marker").exists());

    let attempt = runtime
        .registry()
        .get_latest_attempt(&observed.job_id)
        .unwrap()
        .unwrap();
    let runner_start: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&attempt.bundle_path).join("runner-start.json")).unwrap(),
    )
    .unwrap();
    let runner_observed_source = runner_start
        .get("observedWorkspaceSourceDigest")
        .and_then(serde_json::Value::as_str)
        .expect("Runner start must record observed Workspace source state");
    let job = runtime.registry().get_job(&observed.job_id).unwrap();
    let plan: RuntimeExecutionPlan = serde_json::from_str(&job.execution_plan_json).unwrap();
    assert_ne!(
        runner_observed_source,
        plan.workspace_source_digest
            .as_deref()
            .expect("Job must commit Workspace source state")
    );

    let connection = Connection::open(&context.registry.db_path).unwrap();
    let reason: String = connection
        .query_row(
            "SELECT reason_code FROM job_events WHERE job_id=?1 AND event_type='JOB_TERMINAL'",
            [&observed.job_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(reason, "WORKSPACE_SOURCE_PRECONDITION_DRIFT");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_resource_budget_is_enforced_by_the_attempt_cgroup() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("resource-budget");
    context.write(
        "runtime_resource_budget.py",
        "import time\nprint('RESOURCE_BUDGET_READY', flush=True)\ntime.sleep(30)\n",
    );
    let runtime = context.runtime(2_000);
    let mut request = context.request("runtime_resource_budget.py", 0);
    request.execution.budget = ExecutionBudget {
        memory_max_bytes: Some(128 * 1024 * 1024),
        tasks_max: Some(32),
        cpu_quota_percent: Some(200),
    };
    let started = runtime.run_task(&request).unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));

    let deadline = Instant::now() + Duration::from_secs(10);
    let attempt = loop {
        let attempt = runtime
            .registry()
            .get_latest_attempt(&started.job_id)
            .unwrap()
            .unwrap();
        if attempt.state == AttemptState::Running
            && attempt.control_group.is_some()
            && attempt.invocation_id.is_some()
        {
            break attempt;
        }
        assert!(
            Instant::now() < deadline,
            "Attempt did not bind to systemd in time"
        );
        thread::sleep(Duration::from_millis(25));
    };

    let properties = command_output(
        "systemctl",
        &[
            "show",
            &attempt.unit_name,
            "--property=MemoryMax,TasksMax,CPUQuotaPerSecUSec",
        ],
        &context.repo,
    );
    assert!(properties.contains("MemoryMax=134217728"), "{properties}");
    assert!(properties.contains("TasksMax=32"), "{properties}");
    assert!(
        properties
            .lines()
            .find(|line| line.starts_with("CPUQuotaPerSecUSec="))
            .is_some_and(|line| line != "CPUQuotaPerSecUSec=infinity"),
        "{properties}"
    );

    let cgroup = PathBuf::from("/sys/fs/cgroup").join(
        attempt
            .control_group
            .as_deref()
            .unwrap()
            .trim_start_matches('/'),
    );
    assert_eq!(
        fs::read_to_string(cgroup.join("memory.max"))
            .unwrap()
            .trim(),
        "134217728"
    );
    assert_eq!(
        fs::read_to_string(cgroup.join("pids.max")).unwrap().trim(),
        "32"
    );
    assert_eq!(
        fs::read_to_string(cgroup.join("cpu.max")).unwrap().trim(),
        "200000 100000"
    );

    let cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id,
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_incremental_observe_and_safe_close_preserve_active_work() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("incremental-safe-close");
    context.write(
        "runtime_incremental.py",
        "import time\nfor value in ['alpha','beta','gamma']:\n print(value, flush=True)\n time.sleep(0.25)\ntime.sleep(10)\n",
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_incremental.py", 0))
        .unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));

    let close_error = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap_err();
    assert_eq!(
        close_error.code,
        ordivon_runtime_core::RuntimeErrorCode::WorkspaceBusy
    );

    let mut stdout_offset = 0;
    let mut stdout = String::new();
    for _ in 0..20 {
        let observed = runtime
            .observe_task(&TaskObserveRequest {
                schema_version: RUNTIME_SCHEMA_VERSION,
                job_id: started.job_id.clone(),
                wait_ms: 200,
                wait_until: ordivon_runtime_core::TaskObserveWaitUntil::Terminal,
                stdout_tail_bytes: 5,
                stderr_tail_bytes: 5,
                stdout_offset: Some(stdout_offset),
                stderr_offset: Some(0),
            })
            .unwrap();
        stdout.push_str(&observed.stdout_tail);
        let next = observed.stdout_next_offset.unwrap();
        assert!(next >= stdout_offset);
        stdout_offset = next;
        if stdout.contains("alpha\nbeta\ngamma\n") {
            break;
        }
    }
    assert_eq!(stdout, "alpha\nbeta\ngamma\n");

    let cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id,
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let closed = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    assert!(closed.removed);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_cancel_reconciles_a_completed_runner_result_before_stop_intent() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("cancel-completed-result");
    context.write(
        "runtime_cancel_completed.py",
        "import time\ntime.sleep(0.5)\nprint('RESULT_ALREADY_FINISHED', flush=True)\n",
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_cancel_completed.py", 0))
        .unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    let attempt = runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    let result_path = Path::new(&attempt.bundle_path).join("result.json");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !result_path.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        result_path.is_file(),
        "Runner result was not written in time"
    );
    assert_eq!(
        runtime
            .registry()
            .get_attempt(&attempt.attempt_id)
            .unwrap()
            .state,
        AttemptState::Running
    );

    let completed = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id,
        })
        .unwrap();

    assert_eq!(completed.status, "succeeded");
    assert!(completed.stdout_tail.contains("RESULT_ALREADY_FINISHED"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_reconcile_all_isolates_one_broken_job_and_converges_another() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("reconcile-isolation");
    context.write(
        "runtime_isolation_bad.py",
        "import time\ntime.sleep(0.4)\nprint('BAD_JOB_RESULT', flush=True)\n",
    );
    let second_workspace = format!("runtime-reconcile-good-{}", Uuid::now_v7());
    create_git_workspace(
        &context.executor,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: second_workspace.clone(),
            source_repo: context.repo.to_string_lossy().into_owned(),
            source_revision: context.revision.clone(),
        },
    )
    .unwrap();
    write_workspace_text(
        &context.executor,
        &WorkspaceWriteRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: second_workspace.clone(),
            relative_path: "runtime_isolation_good.py".to_string(),
            content: "import time\ntime.sleep(0.6)\nprint('GOOD_JOB_RESULT', flush=True)\n"
                .to_string(),
            expected_digest: None,
        },
    )
    .unwrap();

    let runtime = context.runtime(2000);
    let bad = runtime
        .run_task(&context.request("runtime_isolation_bad.py", 0))
        .unwrap();
    let mut good_request = context.request("runtime_isolation_good.py", 0);
    good_request.execution.workspace_id = second_workspace;
    good_request.client_request_id = format!("request:isolation-good:{}", Uuid::now_v7());
    let good = runtime.run_task(&good_request).unwrap();
    let bad_attempt = runtime
        .registry()
        .get_latest_attempt(&bad.job_id)
        .unwrap()
        .unwrap();
    let good_attempt = runtime
        .registry()
        .get_latest_attempt(&good.job_id)
        .unwrap()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while (!(Path::new(&bad_attempt.bundle_path)
        .join("result.json")
        .is_file()
        && Path::new(&good_attempt.bundle_path)
            .join("result.json")
            .is_file()))
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(Path::new(&bad_attempt.bundle_path)
        .join("result.json")
        .is_file());
    assert!(Path::new(&good_attempt.bundle_path)
        .join("result.json")
        .is_file());

    let connection = Connection::open(&context.registry.db_path).unwrap();
    connection
        .execute(
            "UPDATE jobs SET resolution='lost' WHERE job_id=?1",
            [&bad.job_id],
        )
        .unwrap();
    drop(connection);

    let report = runtime.reconcile_all().unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(
        report.failures[0].code,
        ordivon_runtime_core::RuntimeErrorCode::JobAlreadyResolved
    );
    assert_eq!(report.failures[0].job_id, bad.job_id);
    assert_eq!(
        runtime.registry().project_job(&good.job_id).unwrap().status,
        "succeeded"
    );

    let connection = Connection::open(&context.registry.db_path).unwrap();
    let recovery_required: String = connection
        .query_row(
            "SELECT status FROM attempt_conditions WHERE attempt_id=?1 AND condition_type='recovery_required'",
            [&bad_attempt.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(recovery_required, "true");
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_interactive_close_blocks_until_exact_job_is_reconciled() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("interactive-close");
    context.write(
        "runtime_interactive_close.py",
        "import time\ntime.sleep(0.4)\nprint('INTERACTIVE_CLOSE_DONE', flush=True)\n",
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_interactive_close.py", 0))
        .unwrap();
    let attempt = runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    wait_for_file(&Path::new(&attempt.bundle_path).join("result.json"));

    let blocked = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap_err();
    assert_eq!(
        blocked.code,
        ordivon_runtime_core::RuntimeErrorCode::WorkspaceBusy
    );
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 1);
    assert!(
        !runtime
            .registry()
            .project_job(&started.job_id)
            .unwrap()
            .execution_terminal
    );

    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id.clone(),
            wait_ms: 0,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4096,
            stderr_tail_bytes: 4096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "succeeded");
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);

    let closed = runtime
        .close_workspace(&WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: context.workspace_id.clone(),
            force: true,
            expected_source_state_digest: None,
        })
        .unwrap();
    assert!(closed.removed);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_interactive_admission_reconciles_previous_same_workspace_job() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("interactive-admission");
    context.write(
        "runtime_interactive_first.py",
        "import time\ntime.sleep(0.4)\nprint('FIRST_UNOBSERVED_DONE', flush=True)\n",
    );
    context.write(
        "runtime_interactive_second.py",
        "print('SECOND_ADMITTED', flush=True)\n",
    );
    let runtime = context.runtime(2000);
    let first = runtime
        .run_task(&context.request("runtime_interactive_first.py", 0))
        .unwrap();
    let first_attempt = runtime
        .registry()
        .get_latest_attempt(&first.job_id)
        .unwrap()
        .unwrap();
    wait_for_file(&Path::new(&first_attempt.bundle_path).join("result.json"));

    let second = runtime
        .run_task(&context.request("runtime_interactive_second.py", 30_000))
        .unwrap();

    assert_eq!(second.status, "succeeded");
    assert!(second.stdout_tail.contains("SECOND_ADMITTED"));
    assert_eq!(
        runtime
            .registry()
            .project_job(&first.job_id)
            .unwrap()
            .status,
        "succeeded"
    );
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_interactive_list_is_projection_only_and_observe_reconciles_exact_job() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("interactive-list");
    context.write(
        "runtime_interactive_list.py",
        "import time\ntime.sleep(0.4)\nprint('INTERACTIVE_LIST_DONE', flush=True)\n",
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_interactive_list.py", 0))
        .unwrap();
    let attempt = runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    wait_for_file(&Path::new(&attempt.bundle_path).join("result.json"));

    let listed = runtime
        .list_jobs(&RuntimeJobListRequest {
            limit: 10,
            cursor: None,
            client_request_id: None,
            workspace_id: None,
        })
        .unwrap();
    let job = listed
        .jobs
        .iter()
        .find(|job| job.job_id == started.job_id)
        .unwrap();

    assert!(!job.execution_terminal);
    assert!(!job.result_available);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 1);

    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id.clone(),
            wait_ms: 0,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4096,
            stderr_tail_bytes: 4096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "succeeded");
    assert!(observed.execution_terminal);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
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
            wait_until: ordivon_runtime_core::TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 8192,
            stderr_tail_bytes: 8192,
            stdout_offset: None,
            stderr_offset: None,
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
            request_identity_digest: None,
            plan: RuntimeExecutionPlan {
                schema_version: RUNTIME_SCHEMA_VERSION,
                workspace_id: self.workspace_id.clone(),
                workspace_path: workspace.to_string_lossy().into_owned(),
                source_revision: self.revision.clone(),
                workspace_source_digest: None,
                workspace_git_common_dir: None,
                executable: executable.to_string_lossy().into_owned(),
                executable_digest: file_digest(&executable),
                args: Vec::new(),
                cwd: workspace.to_string_lossy().into_owned(),
                env: Default::default(),
                timeout_ms: 10_000,
                stdout_limit_bytes: 65_536,
                stderr_limit_bytes: 65_536,
                steps: Vec::new(),
                budget: ordivon_runtime_core::ExecutionBudget::default(),
                execution_profile: ordivon_runtime_core::ExecutionProfile::TrustedLocal,
                execution_target: ordivon_runtime_core::ExecutionTarget::LocalLinux,
                foreign_references: Vec::new(),
                input_set_id: None,
                effective_inputs: Vec::new(),
                principal: "principal:integration".to_string(),
            },
            global_limit,
        }
    }
}

fn created_admission(
    outcome: ordivon_runtime_core::AdmissionOutcome,
) -> ordivon_runtime_core::CreatedAdmission {
    match outcome {
        ordivon_runtime_core::AdmissionOutcome::Created(created) => *created,
        ordivon_runtime_core::AdmissionOutcome::Existing { .. } => {
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
        ordivon_runtime_core::ReservationState::HeldOrphaned
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
            wait_until: ordivon_runtime_core::TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 1024,
            stderr_tail_bytes: 1024,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(completed.status, "succeeded");
    // Unknown staging may belong to another Runtime instance. Recovery must converge
    // the durable Attempt without guessing staging ownership or deleting it.
    assert_eq!(fs::read(stale.join("partial")).unwrap(), b"partial bundle");
    let attempt = runtime
        .registry()
        .get_latest_attempt(&completed.job_id)
        .unwrap()
        .unwrap();
    assert!(Path::new(&attempt.bundle_path)
        .join("bundle-manifest.json")
        .is_file());
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
            wait_until: ordivon_runtime_core::TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 1024,
            stderr_tail_bytes: 1024,
            stdout_offset: None,
            stderr_offset: None,
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
        ordivon_runtime_core::ReservationState::HeldOrphaned
    );
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_failed_unit_is_released_only_after_durable_terminal_commit() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("failed-unit-release");
    context.write(
        "runtime_failed_unit.py",
        "import sys
print('FAILED_UNIT_EVIDENCE', flush=True)
sys.exit(7)
",
    );
    let runtime = context.runtime(2_000);
    let result = runtime
        .run_task(&context.request("runtime_failed_unit.py", 30_000))
        .unwrap();
    assert_eq!(result.status, "failed");
    assert!(result
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == "terminal_evidence"));
    let attempt = runtime
        .registry()
        .get_latest_attempt(&result.job_id)
        .unwrap()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let output = Command::new("systemctl")
            .args([
                "show",
                &attempt.unit_name,
                "--property=LoadState",
                "--value",
            ])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&output.stdout).trim() == "not-found" {
            break;
        }
        assert!(Instant::now() < deadline, "failed unit was not released");
        std::thread::sleep(Duration::from_millis(20));
    }
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

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_fast_successes_never_race_into_orphaned_capacity() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("fast-success-race");
    context.write(
        "runtime_fast_success.py",
        "print('RUNTIME_FAST_SUCCESS', flush=True)\n",
    );
    let runtime = context.runtime(2000);
    for index in 0..20 {
        let mut request = context.request("runtime_fast_success.py", 10_000);
        request.client_request_id = format!("request:fast-success:{index}:{}", Uuid::now_v7());
        let observation = runtime.run_task(&request).unwrap();
        assert_eq!(
            observation.status, "succeeded",
            "fast success {index} was misclassified as {}",
            observation.status
        );
        assert!(observation.stdout_tail.contains("RUNTIME_FAST_SUCCESS"));
        assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    }
    assert!(runtime
        .registry()
        .list_held_orphaned_attempts()
        .unwrap()
        .is_empty());
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, staged Finance Python environment, and explicit local opt-in"]
fn runtime_finance_i8_graduation_matches_canonical_semantics_with_job_owned_inputs() {
    if std::env::var("ORDIVON_FINANCE_GRADUATION").as_deref() != Ok("1") {
        return;
    }
    let runner_path =
        PathBuf::from(std::env::var("ORDIVON_RUNNER_PATH").expect("ORDIVON_RUNNER_PATH"));
    let finance_repo =
        fs::canonicalize(std::env::var("ORDIVON_FINANCE_REPO").expect("ORDIVON_FINANCE_REPO"))
            .unwrap();
    let python =
        fs::canonicalize(std::env::var("ORDIVON_FINANCE_PYTHON").expect("ORDIVON_FINANCE_PYTHON"))
            .unwrap();
    let pythonpath = fs::canonicalize(
        std::env::var("ORDIVON_FINANCE_PYTHONPATH").expect("ORDIVON_FINANCE_PYTHONPATH"),
    )
    .unwrap();
    let environment_root = fs::canonicalize(
        std::env::var("ORDIVON_FINANCE_ENV_ROOT").expect("ORDIVON_FINANCE_ENV_ROOT"),
    )
    .unwrap();
    let finance_revision = command_output("git", &["rev-parse", "HEAD"], &finance_repo);
    assert_eq!(finance_revision, "5c35e8dbcc3efbe10df50261f6308ae7babe3eaa");
    assert!(command_output("git", &["status", "--porcelain"], &finance_repo).is_empty());

    let root = PathBuf::from("/root/.local/share/ordivon-integration")
        .join(format!("finance-i8-{}", Uuid::now_v7()));
    let executor = UniversalExecutorConfig {
        store_root: root.join("store"),
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
        runner_path,
        allowed_executable_roots: vec![PathBuf::from("/usr/bin"), environment_root.clone()],
        max_runtime_ms: 24 * 60 * 60 * 1000,
        max_output_bytes: 64 * 1024 * 1024,
    };
    executor.ensure_store().unwrap();
    let workspace_id = format!("finance-i8-{}", Uuid::now_v7());
    create_git_workspace(
        &executor,
        &GitWorkspaceCreateRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id: workspace_id.clone(),
            source_repo: finance_repo.to_string_lossy().into_owned(),
            source_revision: finance_revision.clone(),
        },
    )
    .unwrap();
    let registry = RegistryConfig {
        db_path: root.join("registry/registry.sqlite3"),
        store_root: root.join("registry"),
        busy_timeout_ms: 5000,
    };
    let runtime = Runtime::new_with_input_authorities(
        RuntimeConfig {
            registry,
            executor: executor.clone(),
            startup_grace_ms: 2_000,
            windows: None,
        },
        vec![InputAuthority {
            name: "finance-state".to_string(),
            root: finance_repo.join("state"),
        }],
    )
    .unwrap();

    let state_root = finance_repo.join("state");
    let mut relative_inputs = vec![
        PathBuf::from("control/finance.db"),
        PathBuf::from("control/finance.db-shm"),
        PathBuf::from("control/finance.db-wal"),
    ];
    let holdability = state_root.join("data/fragments/okx/tradfi/holdability");
    let mut fragments = fs::read_dir(&holdability)
        .unwrap()
        .map(|entry| {
            PathBuf::from("data/fragments/okx/tradfi/holdability").join(entry.unwrap().file_name())
        })
        .collect::<Vec<_>>();
    fragments.sort();
    assert_eq!(fragments.len(), 3);
    relative_inputs.extend(fragments);
    let inputs = relative_inputs
        .iter()
        .map(|relative| {
            let source = state_root.join(relative);
            assert!(
                source.is_file(),
                "missing Finance input {}",
                source.display()
            );
            InputBindingRequest {
                authority: "finance-state".to_string(),
                relative_object: relative.to_string_lossy().into_owned(),
                expected_digest: file_digest(&source),
                presentation_relative_path: Path::new("finance/state")
                    .join(relative)
                    .to_string_lossy()
                    .into_owned(),
            }
        })
        .collect::<Vec<_>>();
    let mut env = BTreeMap::new();
    env.insert(
        "PYTHONPATH".to_string(),
        pythonpath.to_string_lossy().into_owned(),
    );
    env.insert("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string());
    let request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id: format!("request:finance-i8:{}", Uuid::now_v7()),
        principal: "principal:finance-graduation".to_string(),
        global_limit: 8,
        execution: UniversalExecutionRequest {
            workspace_id: workspace_id.clone(),
            executable: python.to_string_lossy().into_owned(),
            args: vec![
                "scripts/lab-run.py".to_string(),
                "experiments/carrier-holding-cost-sensitivity.spec.json".to_string(),
                "--db".to_string(),
                "/run/ordivon/inputs/finance/state/control/finance.db".to_string(),
                "--state-root".to_string(),
                "/run/ordivon/inputs/finance/state".to_string(),
            ],
            cwd_relative: ".".to_string(),
            env,
            timeout_ms: 60_000,
            stdout_limit_bytes: 2 * 1024 * 1024,
            stderr_limit_bytes: 2 * 1024 * 1024,
            steps: Vec::new(),
            budget: ExecutionBudget::default(),
            execution_profile: ordivon_runtime_core::ExecutionProfile::ContainedLocal,
            execution_target: ordivon_runtime_core::ExecutionTarget::LocalLinux,
            foreign_references: vec![ForeignReference {
                namespace: "ordivon.finance".to_string(),
                reference_type: "state_version".to_string(),
                id: "234:bb27b51397f6add8".to_string(),
                generation: Some(finance_revision.clone()),
                digest: None,
            }],
        },
        wait_ms: 30_000,
        stdout_tail_bytes: 64 * 1024,
        stderr_tail_bytes: 64 * 1024,
    };

    let terminal = runtime.run_task_with_inputs(&request, &inputs).unwrap();
    assert_eq!(terminal.status, "succeeded", "{}", terminal.stderr_tail);
    let result: serde_json::Value = serde_json::from_str(&terminal.stdout_tail).unwrap();
    assert_eq!(result["stateVersionBefore"], "234:bb27b51397f6add8");
    assert_eq!(result["stateVersionAfter"], "234:bb27b51397f6add8");
    assert_eq!(
        result["semanticResultDigest"],
        "sha256:f83c6b54bd7b4ac2826e1d8866690236e4daf93d243e5e33a811542788b668da"
    );
    assert_eq!(result["codeBinding"]["revision"], finance_revision);
    assert_eq!(result["codeBinding"]["dirty"], false);
    assert_eq!(result["environmentBinding"]["pythonVersion"], "3.12.13");
    assert_eq!(result["environmentBinding"]["polarsVersion"], "1.42.1");
    assert_eq!(result["environmentBinding"]["duckdbVersion"], "1.5.5");

    let plan = runtime.registry().execution_plan(&terminal.job_id).unwrap();
    let owned_root = executor.job_input_path(&terminal.job_id);
    assert!(plan.input_set_id.is_some());
    assert_eq!(plan.effective_inputs.len(), inputs.len());
    assert!(!executor
        .input_materializations_root()
        .join(&terminal.job_id)
        .exists());
    for binding in &plan.effective_inputs {
        assert!(owned_root
            .join(&binding.presentation_relative_path)
            .is_file());
    }

    let terminal_evidence = terminal
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: terminal.job_id.clone(),
            artifact_id: terminal_evidence.artifact_id.clone(),
            offset: 0,
            max_bytes: 256 * 1024,
        })
        .unwrap();
    let evidence: serde_json::Value = serde_json::from_str(&evidence.content).unwrap();
    assert_eq!(
        evidence["effectiveInputs"].as_array().unwrap().len(),
        inputs.len()
    );
    assert!(evidence.to_string().contains("234:bb27b51397f6add8"));
    assert!(!evidence.to_string().contains("job-inputs"));
    assert!(!evidence.to_string().contains("input-materializations"));

    let workspace_path = executor.store_root.join("workspaces").join(&workspace_id);
    assert!(command_output("git", &["status", "--porcelain"], &workspace_path).is_empty());

    drop(runtime);
    let restarted = Runtime::new(RuntimeConfig {
        registry: RegistryConfig {
            db_path: root.join("registry/registry.sqlite3"),
            store_root: root.join("registry"),
            busy_timeout_ms: 5000,
        },
        executor: executor.clone(),
        startup_grace_ms: 2_000,
        windows: None,
    })
    .unwrap();
    let replay = restarted.run_task_with_inputs(&request, &inputs).unwrap();
    assert_eq!(replay.job_id, terminal.job_id);
    assert_eq!(replay.status, "succeeded");
    eprintln!(
        "FINANCE_I8_RECEIPT={}",
        serde_json::json!({
            "financeRevision": finance_revision,
            "stateVersion": result["stateVersionBefore"],
            "semanticResultDigest": result["semanticResultDigest"],
            "runtimeJobId": terminal.job_id,
            "runtimeAttemptId": terminal.attempt_id,
            "terminalEvidenceArtifactId": terminal_evidence.artifact_id,
            "terminalEvidenceDigest": terminal_evidence.digest,
            "effectiveInputCount": plan.effective_inputs.len(),
            "physicalInputsOwnedByJob": owned_root.is_dir(),
            "inputSetId": plan.input_set_id,
            "replaySameJob": replay.job_id == terminal.job_id,
            "financeWorkspaceDirty": !command_output("git", &["status", "--porcelain"], &workspace_path).is_empty(),
            "executionProfile": "contained_local",
        })
    );
    let replay_plan = restarted.registry().execution_plan(&replay.job_id).unwrap();
    assert_eq!(
        fs::read(
            executor
                .job_input_path(&replay.job_id)
                .join(&replay_plan.effective_inputs[0].presentation_relative_path)
        )
        .unwrap(),
        fs::read(state_root.join(&relative_inputs[0])).unwrap()
    );

    drop(restarted);
    remove_git_workspace(
        &executor,
        &WorkspaceCloseRequest {
            schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
            workspace_id,
            force: false,
            expected_source_state_digest: None,
        },
    )
    .unwrap();
    let _ = fs::remove_dir_all(&root);
}
