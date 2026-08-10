#![cfg(feature = "transactional-runtime")]

use ordivon_runtime_core::{
    create_git_workspace, remove_git_workspace, write_workspace_text, ArtifactReadRequest,
    AttemptState, ExecutionBudget, ForeignReference, GitWorkspaceCreateRequest,
    HostDependencyBinding, InputAuthority, InputBindingRequest, RegistryConfig, Runtime,
    RuntimeConfig, RuntimeExecutionPlan, RuntimeJobListRequest, SubmitRequest, TaskCancelRequest,
    TaskObserveRequest, TaskObserveWaitUntil, TaskRunRequest, UniversalExecutionRequest,
    UniversalExecutorConfig, WindowsExecutionConfig, WorkspaceCloseRequest, WorkspaceMutateRequest,
    WorkspaceMutation, WorkspaceMutationMode, WorkspaceWriteRequest, RUNTIME_SCHEMA_VERSION,
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
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
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

    fn registry_marker_exists(marker: &str) -> bool {
        let escaped = marker.replace('\'', "''");
        let script = format!(
            "if (Test-Path 'HKLM:\\SOFTWARE\\OrdivonRuntimeRw3\\{escaped}') {{ Write-Output 1 }} else {{ Write-Output 0 }}"
        );
        let output = Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim() == "1"
    }
    fn power_request_present(attempt_id: &str) -> bool {
        let output = Command::new("/mnt/c/Windows/System32/powercfg.exe")
            .arg("/requests")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let observed = String::from_utf8_lossy(&output.stdout);
        let expected = format!("Ordivon Runtime Attempt {attempt_id}");
        observed
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
    }

    fn windows_marker_process_count(marker: &str) -> usize {
        let escaped = marker.replace('\'', "''");
        let script = format!(
            "$m='{escaped}'; $rows=Get-CimInstance Win32_Process | Where-Object {{$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}}; Write-Output @($rows).Count"
        );
        let mut last_error = String::new();
        for _ in 0..3 {
            let output =
                Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                    .unwrap();
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<usize>()
                    .unwrap();
            }
            last_error = String::from_utf8_lossy(&output.stderr).into_owned();
            thread::sleep(Duration::from_millis(100));
        }
        panic!("Windows process observation failed: {last_error}");
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
                "$ORDIVON_LITERAL".to_string(),
                "${ORDIVON_LITERAL}".to_string(),
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
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
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
    assert!(!first
        .stdout_tail
        .contains("W1_ECHO_SYSTEMROOT_B64=PG51bGw+"));
    assert!(!first.stdout_tail.contains("W1_ECHO_PATH_B64=PG51bGw+"));
    assert!(first
        .stdout_tail
        .contains("W1_ECHO_WSL_DISTRO_B64=PG51bGw+"));
    assert!(first.stdout_tail.contains("W1_ECHO_ARGC=4"));
    assert!(first
        .stdout_tail
        .contains("W1_ECHO_ARG_2_B64=JE9SRElWT05fTElURVJBTA=="));
    assert!(first
        .stdout_tail
        .contains("W1_ECHO_ARG_3_B64=JHtPUkRJVk9OX0xJVEVSQUx9"));
    let committed_job = runtime.registry().get_job(&first.job_id).unwrap();
    let committed_plan: RuntimeExecutionPlan =
        serde_json::from_str(&committed_job.execution_plan_json).unwrap();
    assert_eq!(
        committed_plan.execution_target,
        ordivon_runtime_core::ExecutionTarget::WindowsNative
    );
    let committed_windows = committed_plan.windows_execution_context.as_ref().unwrap();
    assert_eq!(
        committed_windows.token_class,
        ordivon_runtime_core::WindowsTokenClass::Limited
    );
    assert_eq!(
        committed_windows.environment_source,
        "windows_user_machine_profile_allowlist_v1"
    );
    assert!(committed_plan
        .env
        .get("SystemRoot")
        .is_some_and(|value| !value.is_empty()));
    assert!(committed_plan
        .env
        .get("Path")
        .is_some_and(|value| !value.is_empty()));
    assert_eq!(
        committed_plan.env.get("W1_ENV").map(String::as_str),
        Some("runtime-w1")
    );
    assert!(!committed_plan
        .env
        .keys()
        .any(|name| name.eq_ignore_ascii_case("PNPM_HOME")));
    assert!(!committed_plan
        .env
        .keys()
        .any(|name| name.eq_ignore_ascii_case("WSL_DISTRO_NAME")));
    assert!(!committed_plan
        .env
        .keys()
        .any(|name| name.contains('(') || name.contains(')')));
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
    assert_eq!(windows_evidence["tokenSelection"], "lua_medium_filtered");
    assert_eq!(windows_evidence["tokenType"], 1);
    assert_eq!(windows_evidence["tokenIsElevated"], false);
    assert_eq!(windows_evidence["tokenIntegrityLevelRid"], 8192);
    assert_eq!(windows_evidence["powerRequestType"], "system_required");
    assert_eq!(windows_evidence["powerRequestAcquired"], true);
    assert!(!power_request_present(&attempt_id));
    let admin_attrs = windows_evidence["administratorsGroupAttributes"]
        .as_u64()
        .unwrap();
    assert_eq!(admin_attrs & 0x4, 0);
    assert_ne!(admin_attrs & 0x10, 0);
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
    assert_eq!(terminal_json["windowsAuthority"], "limited");
    assert_eq!(
        terminal_json["windowsExecutionContext"]["tokenClass"],
        "limited"
    );
    assert_eq!(
        terminal_json["windowsExecutionContext"]["tokenUserSid"],
        windows_evidence["tokenUserSid"]
    );
    assert_eq!(
        terminal_json["windowsExecutionContext"]["environmentSource"],
        "windows_user_machine_profile_allowlist_v1"
    );
    assert!(terminal_json["terminalArtifactIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == &serde_json::Value::String(format!("{attempt_id}.windows-start"))));

    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);

    let limited_admin_marker = format!("LIMITED_{}", Uuid::now_v7());
    let mut limited_admin_request = request.clone();
    limited_admin_request.client_request_id =
        format!("request:windows-limited-admin:{}", Uuid::now_v7());
    limited_admin_request.execution.args =
        vec!["authority-probe".to_string(), limited_admin_marker.clone()];
    let limited_admin = runtime.run_task(&limited_admin_request).unwrap();
    assert_eq!(
        limited_admin.status, "succeeded",
        "{}",
        limited_admin.stderr_tail
    );
    assert!(limited_admin.stdout_tail.contains(&format!(
        "W1_AUTHORITY_HKLM=denied marker={limited_admin_marker}"
    )));
    assert!(!registry_marker_exists(&limited_admin_marker));

    let elevated_marker = format!("ELEVATED_{}", Uuid::now_v7());
    let mut elevated_request = request.clone();
    elevated_request.client_request_id = format!("request:windows-elevated:{}", Uuid::now_v7());
    elevated_request.execution.windows_authority = ordivon_runtime_core::WindowsAuthority::Elevated;
    elevated_request.execution.args = vec!["authority-probe".to_string(), elevated_marker.clone()];
    let elevated = runtime.run_task(&elevated_request).unwrap();
    assert_eq!(elevated.status, "succeeded", "{}", elevated.stderr_tail);
    assert!(elevated.stdout_tail.contains(&format!(
        "W1_AUTHORITY_HKLM=allowed marker={elevated_marker}"
    )));
    assert!(!registry_marker_exists(&elevated_marker));
    let elevated_attempt_id = elevated.attempt_id.clone().unwrap();
    let elevated_job = runtime.registry().get_job(&elevated.job_id).unwrap();
    let elevated_plan: RuntimeExecutionPlan =
        serde_json::from_str(&elevated_job.execution_plan_json).unwrap();
    assert_eq!(
        elevated_plan.windows_authority,
        ordivon_runtime_core::WindowsAuthority::Elevated
    );
    let elevated_context = elevated_plan.windows_execution_context.as_ref().unwrap();
    assert_eq!(
        elevated_context.token_class,
        ordivon_runtime_core::WindowsTokenClass::Elevated
    );
    assert_eq!(
        elevated_context.token_user_sid,
        committed_windows.token_user_sid
    );
    let elevated_artifacts = runtime.registry().list_artifacts(&elevated.job_id).unwrap();
    let elevated_start = elevated_artifacts
        .iter()
        .find(|artifact| artifact.kind == "windows_start")
        .unwrap();
    let elevated_start_value: serde_json::Value = serde_json::from_str(
        &runtime
            .read_artifact(&ArtifactReadRequest {
                schema_version: RUNTIME_SCHEMA_VERSION,
                job_id: elevated.job_id.clone(),
                artifact_id: elevated_start.artifact_id.clone(),
                offset: 0,
                max_bytes: 65_536,
            })
            .unwrap()
            .content,
    )
    .unwrap();
    assert_eq!(elevated_start_value["tokenSelection"], "current_elevated");
    assert_eq!(
        elevated_start_value["tokenUserSid"],
        committed_windows.token_user_sid
    );
    assert_eq!(elevated_start_value["tokenType"], 1);
    assert_eq!(elevated_start_value["tokenIsElevated"], true);
    assert_eq!(elevated_start_value["powerRequestType"], "system_required");
    assert_eq!(elevated_start_value["powerRequestAcquired"], true);
    assert!(!power_request_present(&elevated_attempt_id));
    assert!(
        elevated_start_value["tokenIntegrityLevelRid"]
            .as_i64()
            .unwrap()
            >= 12288
    );
    let elevated_admin_attrs = elevated_start_value["administratorsGroupAttributes"]
        .as_u64()
        .unwrap();
    assert_ne!(elevated_admin_attrs & 0x4, 0);
    assert_eq!(elevated_admin_attrs & 0x10, 0);
    let elevated_terminal = elevated_artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let elevated_terminal_value: serde_json::Value = serde_json::from_str(
        &runtime
            .read_artifact(&ArtifactReadRequest {
                schema_version: RUNTIME_SCHEMA_VERSION,
                job_id: elevated.job_id.clone(),
                artifact_id: elevated_terminal.artifact_id.clone(),
                offset: 0,
                max_bytes: 65_536,
            })
            .unwrap()
            .content,
    )
    .unwrap();
    assert_eq!(elevated_terminal_value["windowsAuthority"], "elevated");
    assert_eq!(
        elevated_terminal_value["windowsExecutionContext"]["tokenClass"],
        "elevated"
    );
    assert_eq!(
        elevated_terminal_value["windowsExecutionContext"]["tokenUserSid"],
        committed_windows.token_user_sid
    );
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    println!(
        "RW3_WINDOWS_ELEVATED jobId={} attemptId={} windowsPid={} integrity={} adminAttrs={} hklm=allowed",
        elevated.job_id,
        elevated_attempt_id,
        elevated_start_value["processId"].as_u64().unwrap(),
        elevated_start_value["tokenIntegrityLevelRid"].as_i64().unwrap(),
        elevated_admin_attrs,
    );

    let reconnect_marker = format!("ORDIVON_RW5_RECONNECT_{}", Uuid::now_v7());
    let mut reconnect_request = request.clone();
    reconnect_request.client_request_id = format!("request:windows-reconnect:{}", Uuid::now_v7());
    reconnect_request.execution.args = vec!["tree".to_string(), reconnect_marker.clone()];
    reconnect_request.execution.timeout_ms = 20_000;
    reconnect_request.execution.budget.tasks_max = Some(8);
    reconnect_request.wait_ms = 0;
    let reconnect_started = runtime.run_task(&reconnect_request).unwrap();
    assert!(matches!(
        reconnect_started.status.as_str(),
        "queued" | "working"
    ));
    let reconnect_deadline = Instant::now() + Duration::from_secs(10);
    let reconnect_attempt = loop {
        let attempt = runtime
            .registry()
            .get_latest_attempt(&reconnect_started.job_id)
            .unwrap()
            .unwrap();
        if attempt.state == AttemptState::Running
            && attempt.control_group.is_some()
            && attempt.invocation_id.is_some()
        {
            break attempt;
        }
        assert!(
            Instant::now() < reconnect_deadline,
            "Windows reconnect Attempt did not become running"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert!(power_request_present(&reconnect_attempt.attempt_id));
    let marker_deadline = Instant::now() + Duration::from_secs(5);
    while windows_marker_process_count(&reconnect_marker) == 0 {
        assert!(
            Instant::now() < marker_deadline,
            "Windows reconnect marker process never appeared"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let reconnect_job_id = reconnect_started.job_id.clone();
    let reconnect_attempt_id = reconnect_attempt.attempt_id.clone();
    drop(runtime);
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
    let reattached = runtime.run_task(&reconnect_request).unwrap();
    assert_eq!(reattached.job_id, reconnect_job_id);
    assert_eq!(
        reattached.attempt_id.as_deref(),
        Some(reconnect_attempt_id.as_str())
    );
    assert!(!reattached.execution_terminal);
    assert_eq!(
        runtime
            .registry()
            .get_latest_attempt(&reconnect_job_id)
            .unwrap()
            .unwrap()
            .attempt_id,
        reconnect_attempt_id
    );
    assert!(power_request_present(&reconnect_attempt_id));
    let reconnect_cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: reconnect_job_id.clone(),
        })
        .unwrap();
    assert_eq!(reconnect_cancelled.status, "cancelled");
    assert!(reconnect_cancelled.execution_terminal);
    assert!(!power_request_present(&reconnect_attempt_id));
    thread::sleep(Duration::from_millis(300));
    assert_eq!(windows_marker_process_count(&reconnect_marker), 0);
    println!(
        "RW5_WINDOWS_RUNTIME_RECONNECT jobId={} attemptId={} duplicateDispatch=false remaining=0",
        reconnect_job_id, reconnect_attempt_id,
    );
    let reconnect_unit = format!("ordivon-{reconnect_attempt_id}.service");
    let _ = Command::new("systemctl")
        .args(["stop", &reconnect_unit])
        .output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &reconnect_unit])
        .output();

    let crash_marker = format!("ORDIVON_RW5_LAUNCHER_CRASH_{}", Uuid::now_v7());
    let mut crash_request = request.clone();
    crash_request.client_request_id = format!("request:windows-launcher-crash:{}", Uuid::now_v7());
    crash_request.execution.args = vec!["tree".to_string(), crash_marker.clone()];
    crash_request.execution.timeout_ms = 20_000;
    crash_request.execution.budget.tasks_max = Some(8);
    crash_request.wait_ms = 0;
    let crash_started = runtime.run_task(&crash_request).unwrap();
    assert!(matches!(
        crash_started.status.as_str(),
        "queued" | "working"
    ));
    let crash_deadline = Instant::now() + Duration::from_secs(10);
    let crash_attempt = loop {
        let attempt = runtime
            .registry()
            .get_latest_attempt(&crash_started.job_id)
            .unwrap()
            .unwrap();
        if attempt.state == AttemptState::Running
            && attempt.control_group.is_some()
            && attempt.invocation_id.is_some()
        {
            break attempt;
        }
        assert!(
            Instant::now() < crash_deadline,
            "Windows launcher-crash Attempt did not become running"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert!(power_request_present(&crash_attempt.attempt_id));
    let crash_marker_deadline = Instant::now() + Duration::from_secs(5);
    while windows_marker_process_count(&crash_marker) == 0 {
        assert!(
            Instant::now() < crash_marker_deadline,
            "Windows crash marker process never appeared"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let crash_unit = format!("ordivon-{}.service", crash_attempt.attempt_id);
    let killed = Command::new("systemctl")
        .args(["kill", "--kill-who=main", "--signal=KILL", &crash_unit])
        .output()
        .unwrap();
    assert!(
        killed.status.success(),
        "{}",
        String::from_utf8_lossy(&killed.stderr)
    );
    let crash_observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: crash_started.job_id.clone(),
            wait_ms: 10_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4096,
            stderr_tail_bytes: 4096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert!(crash_observed.execution_terminal);
    assert!(matches!(
        crash_observed.status.as_str(),
        "failed" | "lost" | "orphaned"
    ));
    assert_eq!(
        crash_observed.attempt_id.as_deref(),
        Some(crash_attempt.attempt_id.as_str())
    );
    assert!(!power_request_present(&crash_attempt.attempt_id));
    thread::sleep(Duration::from_millis(300));
    assert_eq!(windows_marker_process_count(&crash_marker), 0);
    let crash_replay = runtime.run_task(&crash_request).unwrap();
    assert_eq!(crash_replay.job_id, crash_started.job_id);
    assert_eq!(crash_replay.attempt_id, crash_observed.attempt_id);
    assert_eq!(crash_replay.status, crash_observed.status);
    println!(
        "RW5_WINDOWS_LAUNCHER_CRASH jobId={} attemptId={} status={} duplicateDispatch=false remaining=0",
        crash_started.job_id, crash_attempt.attempt_id, crash_observed.status,
    );
    let _ = Command::new("systemctl")
        .args(["stop", &crash_unit])
        .output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &crash_unit])
        .output();

    println!(
        "RW2_WINDOWS_RUNTIME jobId={} attemptId={} windowsPid={} creationFileTime={} imageDigest={} artifacts={}",
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
    assert!(!power_request_present(
        timed_out.attempt_id.as_deref().unwrap()
    ));

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
    assert!(power_request_present(&cancel_attempt.attempt_id));
    let cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started_cancel.job_id.clone(),
        })
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(cancelled.execution_terminal);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    assert!(!power_request_present(&cancel_attempt.attempt_id));

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

    let elevated_timeout_marker = format!("ORDIVON_RW3_ELEVATED_TIMEOUT_{}", Uuid::now_v7());
    let mut elevated_timeout_request = elevated_request.clone();
    elevated_timeout_request.client_request_id =
        format!("request:windows-elevated-timeout:{}", Uuid::now_v7());
    elevated_timeout_request.execution.args =
        vec!["tree".to_string(), elevated_timeout_marker.clone()];
    elevated_timeout_request.execution.timeout_ms = 300;
    elevated_timeout_request.execution.budget.tasks_max = Some(8);
    let elevated_timeout_started = Instant::now();
    let elevated_timed_out = runtime.run_task(&elevated_timeout_request).unwrap();
    assert_eq!(
        elevated_timed_out.status, "timed_out",
        "{}",
        elevated_timed_out.stderr_tail
    );
    assert!(elevated_timeout_started.elapsed() < Duration::from_secs(3));
    assert!(elevated_timed_out.execution_terminal);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    assert!(!power_request_present(
        elevated_timed_out.attempt_id.as_deref().unwrap(),
    ));
    thread::sleep(Duration::from_millis(500));
    let elevated_timeout_probe = format!(
        "$m='{}'; $rows=Get-CimInstance Win32_Process | Where-Object {{$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}}; Write-Output @($rows).Count",
        elevated_timeout_marker
    );
    let elevated_timeout_remaining =
        Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
            .args(["-NoProfile", "-Command", &elevated_timeout_probe])
            .output()
            .unwrap();
    assert!(
        elevated_timeout_remaining.status.success(),
        "{}",
        String::from_utf8_lossy(&elevated_timeout_remaining.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&elevated_timeout_remaining.stdout).trim(),
        "0"
    );
    println!(
        "RW3_WINDOWS_ELEVATED_TIMEOUT jobId={} attemptId={} elapsedMs={} remaining=0",
        elevated_timed_out.job_id,
        elevated_timed_out.attempt_id.as_deref().unwrap(),
        elevated_timeout_started.elapsed().as_millis(),
    );
    let elevated_timeout_unit = format!(
        "ordivon-{}.service",
        elevated_timed_out.attempt_id.as_deref().unwrap()
    );
    let _ = Command::new("systemctl")
        .args(["stop", &elevated_timeout_unit])
        .output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &elevated_timeout_unit])
        .output();

    let elevated_cancel_marker = format!("ORDIVON_RW3_ELEVATED_CANCEL_{}", Uuid::now_v7());
    let mut elevated_cancel_request = elevated_request.clone();
    elevated_cancel_request.client_request_id =
        format!("request:windows-elevated-cancel:{}", Uuid::now_v7());
    elevated_cancel_request.execution.args =
        vec!["tree".to_string(), elevated_cancel_marker.clone()];
    elevated_cancel_request.execution.timeout_ms = 20_000;
    elevated_cancel_request.execution.budget.tasks_max = Some(8);
    elevated_cancel_request.wait_ms = 0;
    let started_elevated_cancel = runtime.run_task(&elevated_cancel_request).unwrap();
    assert!(matches!(
        started_elevated_cancel.status.as_str(),
        "queued" | "working"
    ));
    let elevated_running_deadline = Instant::now() + Duration::from_secs(10);
    let elevated_cancel_attempt = loop {
        let attempt = runtime
            .registry()
            .get_latest_attempt(&started_elevated_cancel.job_id)
            .unwrap()
            .unwrap();
        if attempt.state == AttemptState::Running
            && attempt.control_group.is_some()
            && attempt.invocation_id.is_some()
        {
            break attempt;
        }
        assert!(
            Instant::now() < elevated_running_deadline,
            "Elevated Windows cancel Attempt did not become running"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert!(power_request_present(&elevated_cancel_attempt.attempt_id));
    let elevated_cancelled = runtime
        .cancel_task(&TaskCancelRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started_elevated_cancel.job_id.clone(),
        })
        .unwrap();
    assert_eq!(elevated_cancelled.status, "cancelled");
    assert!(elevated_cancelled.execution_terminal);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    assert!(!power_request_present(&elevated_cancel_attempt.attempt_id));
    thread::sleep(Duration::from_millis(500));
    let elevated_cancel_probe = format!(
        "$m='{}'; $rows=Get-CimInstance Win32_Process | Where-Object {{$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}}; Write-Output @($rows).Count",
        elevated_cancel_marker
    );
    let elevated_cancel_remaining =
        Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
            .args(["-NoProfile", "-Command", &elevated_cancel_probe])
            .output()
            .unwrap();
    assert!(
        elevated_cancel_remaining.status.success(),
        "{}",
        String::from_utf8_lossy(&elevated_cancel_remaining.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&elevated_cancel_remaining.stdout).trim(),
        "0"
    );
    println!(
        "RW3_WINDOWS_ELEVATED_CANCEL jobId={} attemptId={} remaining=0",
        started_elevated_cancel.job_id, elevated_cancel_attempt.attempt_id,
    );
    let elevated_cancel_unit = format!("ordivon-{}.service", elevated_cancel_attempt.attempt_id);
    let _ = Command::new("systemctl")
        .args(["stop", &elevated_cancel_unit])
        .output();
    let _ = Command::new("systemctl")
        .args(["reset-failed", &elevated_cancel_unit])
        .output();

    let launcher_unavailable = launcher.with_extension("exe.replay-proof-unavailable");
    fs::rename(&launcher, &launcher_unavailable).unwrap();
    let replay = runtime.run_task(&request).unwrap();
    assert_eq!(replay.job_id, first.job_id);
    assert_eq!(replay.attempt_id, first.attempt_id);
    assert_eq!(replay.status, "succeeded");
    let elevated_replay = runtime.run_task(&elevated_request).unwrap();
    assert_eq!(elevated_replay.job_id, elevated.job_id);
    assert_eq!(elevated_replay.attempt_id, elevated.attempt_id);
    assert_eq!(elevated_replay.status, "succeeded");
    assert!(!power_request_present(&attempt_id));
    assert!(!power_request_present(&elevated_attempt_id));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    fs::rename(&launcher_unavailable, &launcher).unwrap();
    println!(
        "RW2_WINDOWS_REPLAY jobId={} attemptId={} launcherAvailableDuringReplay=false",
        replay.job_id,
        replay.attempt_id.as_deref().unwrap(),
    );

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
#[ignore = "requires WSL restart, Windows interop, root/systemd, .NET csc, and explicit local opt-in"]
fn runtime_windows_native_wsl_restart_prepare_or_recover() {
    let phase = match std::env::var("ORDIVON_RUN_WINDOWS_WSL_RESTART_PHASE") {
        Ok(value) if value == "prepare" || value == "recover" => value,
        _ => return,
    };
    let repo = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let revision = command_output("git", &["rev-parse", "HEAD"], &repo);
    let root = PathBuf::from("/root/.local/share/ordivon-windows-rw5-wsl-restart");
    let public_root = PathBuf::from("/mnt/c/Users/Public/ordivon-rw5-wsl-restart");
    let manifest_path = public_root.join("manifest.json");
    let launcher = public_root.join("ordivon-windows-job-launcher.exe");
    let fixture = public_root.join("ordivon-windows-job-fixture.exe");
    let workspace_id = "runtime-windows-rw5-wsl-restart".to_string();
    let executor = UniversalExecutorConfig {
        store_root: root.join("store"),
        workspace_root: None,
        workspace_uid: None,
        workspace_gid: None,
        runner_path: PathBuf::from("/usr/bin/true"),
        allowed_executable_roots: vec![public_root.clone()],
        max_runtime_ms: 600_000,
        max_output_bytes: 1024 * 1024,
    };
    let registry = RegistryConfig {
        db_path: root.join("registry/registry.sqlite3"),
        store_root: root.join("registry"),
        busy_timeout_ms: 5_000,
    };
    let runtime_config = || RuntimeConfig {
        registry: registry.clone(),
        executor: executor.clone(),
        startup_grace_ms: 2_000,
        windows: Some(WindowsExecutionConfig {
            launcher_path: launcher.clone(),
            wsl_distribution: std::env::var("WSL_DISTRO_NAME")
                .unwrap_or_else(|_| "archlinux".to_string()),
        }),
    };
    let windows_drive_path = |path: &Path| -> String {
        let text = path.to_str().unwrap();
        let rest = text
            .strip_prefix("/mnt/c/")
            .expect("R-W5 Windows path must be on C:");
        format!("C:\\{}", rest.replace('/', "\\"))
    };
    let windows_marker_count = |marker: &str| -> usize {
        let escaped = marker.replace('\'', "''");
        let script = format!(
            "$m='{escaped}'; $rows=Get-CimInstance Win32_Process | Where-Object {{$_.ProcessId -ne $PID -and $_.CommandLine -like ('*'+$m+'*')}}; Write-Output @($rows).Count"
        );
        let output = Command::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<usize>()
            .unwrap()
    };
    let power_present = |attempt_id: &str| -> bool {
        let output = Command::new("/mnt/c/Windows/System32/powercfg.exe")
            .arg("/requests")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = format!("Ordivon Runtime Attempt {attempt_id}").to_ascii_lowercase();
        String::from_utf8_lossy(&output.stdout)
            .to_ascii_lowercase()
            .contains(&expected)
    };

    if phase == "prepare" {
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let pruned = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            pruned.status.success(),
            "{}",
            String::from_utf8_lossy(&pruned.stderr)
        );
        if public_root.exists() {
            fs::remove_dir_all(&public_root).unwrap();
        }
        fs::create_dir_all(&public_root).unwrap();
        let csc = PathBuf::from("/mnt/c/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe");
        let launcher_source = public_root.join("Ordivon.WindowsJobLauncher.cs");
        let fixture_source = public_root.join("Ordivon.WindowsJobFixture.cs");
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
        executor.ensure_store().unwrap();
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
        let runtime = Runtime::new(runtime_config()).unwrap();
        let marker = format!("ORDIVON_RW5_WSL_RESTART_{}", Uuid::now_v7());
        let request = TaskRunRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            client_request_id: format!("request:windows-wsl-restart:{}", Uuid::now_v7()),
            principal: "principal:windows-wsl-restart".to_string(),
            global_limit: 1,
            execution: UniversalExecutionRequest {
                workspace_id: workspace_id.clone(),
                executable: fixture.to_string_lossy().into_owned(),
                args: vec!["tree".to_string(), marker.clone()],
                cwd_relative: ".".to_string(),
                env: BTreeMap::new(),
                timeout_ms: 600_000,
                stdout_limit_bytes: 65_536,
                stderr_limit_bytes: 65_536,
                steps: Vec::new(),
                budget: ExecutionBudget {
                    memory_max_bytes: Some(128 * 1024 * 1024),
                    tasks_max: Some(8),
                    cpu_quota_percent: Some(100),
                },
                execution_profile: ordivon_runtime_core::ExecutionProfile::TrustedLocal,
                execution_target: ordivon_runtime_core::ExecutionTarget::WindowsNative,
                windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
                foreign_references: Vec::new(),
                host_dependencies: Vec::new(),
            },
            wait_ms: 0,
            stdout_tail_bytes: 4096,
            stderr_tail_bytes: 4096,
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
                "R-W5 restart Attempt did not become running"
            );
            thread::sleep(Duration::from_millis(25));
        };
        let marker_deadline = Instant::now() + Duration::from_secs(5);
        while windows_marker_count(&marker) == 0 {
            assert!(
                Instant::now() < marker_deadline,
                "R-W5 restart marker never appeared"
            );
            thread::sleep(Duration::from_millis(50));
        }
        assert!(power_present(&attempt.attempt_id));
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "jobId": started.job_id,
            "attemptId": attempt.attempt_id,
            "clientRequestId": request.client_request_id,
            "marker": marker,
            "unitName": attempt.unit_name,
            "preBootId": fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap().trim(),
            "workspaceId": workspace_id,
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        println!("RW5_WSL_RESTART_PREPARED {}", manifest);
        return;
    }

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let job_id = manifest["jobId"].as_str().unwrap().to_string();
    let attempt_id = manifest["attemptId"].as_str().unwrap().to_string();
    let client_request_id = manifest["clientRequestId"].as_str().unwrap().to_string();
    let marker = manifest["marker"].as_str().unwrap().to_string();
    let pre_boot = manifest["preBootId"].as_str().unwrap();
    let current_boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    let boot_changed = current_boot.trim() != pre_boot;
    let watchdog_text = fs::read_to_string(public_root.join("watchdog-result.json")).unwrap();
    let watchdog: serde_json::Value =
        serde_json::from_str(watchdog_text.trim_start_matches('\u{feff}')).unwrap();
    assert_eq!(watchdog["completed"], true);
    assert!(watchdog["beforeMarkerCount"].as_u64().unwrap() > 0);
    assert_eq!(watchdog["beforePowerPresent"], true);
    assert_eq!(watchdog["terminateExitCode"], 0);
    assert_eq!(watchdog["afterTerminateMarkerCount"], 0);
    assert_eq!(watchdog["afterTerminatePowerPresent"], false);
    assert_eq!(watchdog["restartExitCode"], 0);
    assert_eq!(watchdog["afterRestartMarkerCount"], 0);
    assert_eq!(watchdog["afterRestartPowerPresent"], false);
    let runtime = Runtime::new(runtime_config()).unwrap();
    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: job_id.clone(),
            wait_ms: 10_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4096,
            stderr_tail_bytes: 4096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert!(observed.execution_terminal);
    assert_eq!(observed.attempt_id.as_deref(), Some(attempt_id.as_str()));
    assert_eq!(observed.status, "failed");
    assert_eq!(windows_marker_count(&marker), 0);
    assert!(!power_present(&attempt_id));
    let replay_request = TaskRunRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        client_request_id,
        principal: "principal:windows-wsl-restart".to_string(),
        global_limit: 1,
        execution: UniversalExecutionRequest {
            workspace_id: workspace_id.clone(),
            executable: fixture.to_string_lossy().into_owned(),
            args: vec!["tree".to_string(), marker.clone()],
            cwd_relative: ".".to_string(),
            env: BTreeMap::new(),
            timeout_ms: 600_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            steps: Vec::new(),
            budget: ExecutionBudget {
                memory_max_bytes: Some(128 * 1024 * 1024),
                tasks_max: Some(8),
                cpu_quota_percent: Some(100),
            },
            execution_profile: ordivon_runtime_core::ExecutionProfile::TrustedLocal,
            execution_target: ordivon_runtime_core::ExecutionTarget::WindowsNative,
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: Vec::new(),
            host_dependencies: Vec::new(),
        },
        wait_ms: 0,
        stdout_tail_bytes: 4096,
        stderr_tail_bytes: 4096,
    };
    let replay = runtime.run_task(&replay_request).unwrap();
    assert_eq!(replay.job_id, job_id);
    assert_eq!(replay.attempt_id.as_deref(), Some(attempt_id.as_str()));
    assert_eq!(replay.status, observed.status);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
    println!(
        "RW5_WSL_RESTART_RECOVERED jobId={} attemptId={} status={} bootChanged={} duplicateDispatch=false remaining=0",
        job_id, attempt_id, observed.status, boot_changed
    );
    let _ = Command::new("systemctl")
        .args(["reset-failed", &format!("ordivon-{attempt_id}.service")])
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
                windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
                foreign_references: Vec::new(),
                host_dependencies: Vec::new(),
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
    let runtime_for_call = context.runtime(2_000);
    let marker = context.root.join("timeout-target-started");
    let mut request = context.request("timeout-descendant-pipes", 10_000);
    request.execution.executable = "/usr/bin/bash".to_string();
    request.execution.args = vec![
        "-lc".to_string(),
        format!("printf started > '{}'; sleep 30 & wait", marker.display()),
    ];
    request.execution.timeout_ms = 1_000;
    let call = thread::spawn(move || runtime_for_call.run_task(&request));
    let marker_deadline = Instant::now() + Duration::from_secs(20);
    while !marker.is_file() && Instant::now() < marker_deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.is_file(), "target did not start in time");
    let target_started = Instant::now();
    let result = call.join().unwrap().unwrap();
    assert_eq!(result.status, "timed_out");
    assert!(target_started.elapsed() < Duration::from_secs(10));
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
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: vec![ordivon_runtime_core::ForeignReference {
                namespace: "ordivon.edge".to_string(),
                reference_type: "supervisor_generation".to_string(),
                id: "contained-integration-supervisor".to_string(),
                generation: Some("1".to_string()),
                digest: None,
            }],
            host_dependencies: Vec::new(),
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
    let gate = context.root.join("cancel-completed-gate");
    context.write(
        "runtime_cancel_completed.py",
        &format!(
            "import pathlib,time\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('RESULT_ALREADY_FINISHED', flush=True)\n",
            gate = gate.to_string_lossy(),
        ),
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_cancel_completed.py", 0))
        .unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    fs::write(&gate, b"go").unwrap();
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
    let bad_gate = context.root.join("reconcile-bad-gate");
    let good_gate = context.root.join("reconcile-good-gate");
    context.write(
        "runtime_isolation_bad.py",
        &format!(
            "import pathlib,time\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('BAD_JOB_RESULT', flush=True)\n",
            gate = bad_gate.to_string_lossy(),
        ),
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
            content: format!(
                "import pathlib,time\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('GOOD_JOB_RESULT', flush=True)\n",
                gate = good_gate.to_string_lossy(),
            ),
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
    assert!(matches!(bad.status.as_str(), "queued" | "working"));
    assert!(matches!(good.status.as_str(), "queued" | "working"));
    fs::write(&bad_gate, b"go").unwrap();
    fs::write(&good_gate, b"go").unwrap();
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
    let gate = context.root.join("interactive-close-gate");
    context.write(
        "runtime_interactive_close.py",
        &format!(
            "import pathlib,time\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('INTERACTIVE_CLOSE_DONE', flush=True)\n",
            gate = gate.to_string_lossy(),
        ),
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_interactive_close.py", 0))
        .unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    fs::write(&gate, b"go").unwrap();
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
    let gate = context.root.join("interactive-list-gate");
    context.write(
        "runtime_interactive_list.py",
        &format!(
            "import pathlib,time\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('INTERACTIVE_LIST_DONE', flush=True)\n",
            gate = gate.to_string_lossy(),
        ),
    );
    let runtime = context.runtime(2000);
    let started = runtime
        .run_task(&context.request("runtime_interactive_list.py", 0))
        .unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    fs::write(&gate, b"go").unwrap();
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
            execution_provider: None,
            runtime_release_effect: None,
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
                windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
                windows_execution_context: None,
                foreign_references: Vec::new(),
                input_set_id: None,
                effective_inputs: Vec::new(),
                principal: "principal:integration".to_string(),
            },
            global_limit,
            host_dependencies: Vec::new(),
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
            windows_authority: ordivon_runtime_core::WindowsAuthority::Limited,
            foreign_references: vec![ForeignReference {
                namespace: "ordivon.finance".to_string(),
                reference_type: "state_version".to_string(),
                id: "234:bb27b51397f6add8".to_string(),
                generation: Some(finance_revision.clone()),
                digest: None,
            }],
            host_dependencies: Vec::new(),
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

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_legacy_plan_without_provider_snapshot_can_dispatch_through_changed_linux_runner() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let mut context = IntegrationContext::new("provider-drift-linux");
    let original_runner = context.executor.runner_path.clone();
    let staged_runner = context.root.join("provider-runner");
    fs::copy(&original_runner, &staged_runner).unwrap();
    fs::set_permissions(&staged_runner, fs::Permissions::from_mode(0o755)).unwrap();
    context.executor.runner_path = staged_runner.clone();

    let runtime = context.runtime(1_000);
    let created = created_admission(
        runtime
            .registry()
            .submit(&context.direct_submit("request:provider-drift-linux", 1))
            .unwrap(),
    );
    assert_eq!(created.attempt.state, AttemptState::Accepted);

    let committed_plan: serde_json::Value =
        serde_json::from_str(&created.job.execution_plan_json).unwrap();
    assert_eq!(committed_plan["executable"], "/usr/bin/true");
    assert!(committed_plan.get("executionProvider").is_none());
    assert!(committed_plan.get("runnerDigest").is_none());

    let marker = context.root.join("drifted-provider-executed.txt");
    let replacement = format!(
        "#!/bin/sh\nprintf 'DRIFTED_PROVIDER_EXECUTED\\n' > '{}'\nexit 91\n",
        marker.display()
    );
    fs::write(&staged_runner, replacement).unwrap();
    fs::set_permissions(&staged_runner, fs::Permissions::from_mode(0o755)).unwrap();

    let _ = runtime.observe_task(&TaskObserveRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        job_id: created.job.job_id.clone(),
        wait_ms: 5_000,
        wait_until: TaskObserveWaitUntil::Terminal,
        stdout_tail_bytes: 4_096,
        stderr_tail_bytes: 4_096,
        stdout_offset: None,
        stderr_offset: None,
    });

    assert_eq!(
        fs::read_to_string(&marker).unwrap().trim(),
        "DRIFTED_PROVIDER_EXECUTED",
        "the post-admission replacement Runner did not cross the physical dispatch boundary"
    );
    assert_ne!(file_digest(&staged_runner), file_digest(&original_runner));

    let attempt = runtime
        .registry()
        .get_latest_attempt(&created.job.job_id)
        .unwrap()
        .unwrap();
    assert!(
        attempt.runner_start_digest.is_none(),
        "a replacement that never emitted committed Runner start evidence unexpectedly bound as the admitted provider"
    );
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_provider_bound_job_rejects_linux_runner_drift_before_dispatch() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let mut context = IntegrationContext::new("provider-bound-linux");
    let original_runner = context.executor.runner_path.clone();
    let staged_runner = context.root.join("provider-runner");
    fs::copy(&original_runner, &staged_runner).unwrap();
    fs::set_permissions(&staged_runner, fs::Permissions::from_mode(0o755)).unwrap();
    context.executor.runner_path = staged_runner.clone();

    let runtime = context.runtime(1_000);
    let mut submit = context.direct_submit("request:provider-bound-linux", 1);
    submit.execution_provider = Some(ordivon_runtime_core::ExecutionProviderSnapshot {
        contract: ordivon_runtime_core::ExecutionProviderContract::LocalLinuxRunnerV1,
        executable_digest: file_digest(&staged_runner),
        wsl_distribution: None,
    });
    let created = created_admission(runtime.registry().submit(&submit).unwrap());
    assert_eq!(created.attempt.state, AttemptState::Accepted);

    let marker = context.root.join("drifted-provider-must-not-run.txt");
    let replacement = format!(
        "#!/bin/sh\nprintf 'PROVIDER_DRIFT_CROSSED_DISPATCH\\n' > '{}'\nexit 91\n",
        marker.display()
    );
    fs::write(&staged_runner, replacement).unwrap();
    fs::set_permissions(&staged_runner, fs::Permissions::from_mode(0o755)).unwrap();
    assert_ne!(file_digest(&staged_runner), file_digest(&original_runner));

    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id.clone(),
            wait_ms: 5_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4_096,
            stderr_tail_bytes: 4_096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();

    assert_eq!(observed.status, "failed");
    assert_eq!(
        observed.execution_reason_code.as_deref(),
        Some("EXECUTION_PROVIDER_PRECONDITION_DRIFT")
    );
    assert!(observed.execution_terminal);
    assert!(
        !marker.exists(),
        "drifted Runner crossed the dispatch boundary"
    );

    let attempt = runtime
        .registry()
        .get_latest_attempt(&created.job.job_id)
        .unwrap()
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Failed);
    assert!(attempt.runner_start_digest.is_none());
    let unit = Command::new("systemctl")
        .args([
            "show",
            &attempt.unit_name,
            "--property=LoadState",
            "--value",
        ])
        .output()
        .unwrap();
    assert_ne!(String::from_utf8_lossy(&unit.stdout).trim(), "loaded");

    let terminal = observed
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: observed.job_id.clone(),
            artifact_id: terminal.artifact_id.clone(),
            offset: 0,
            max_bytes: 65_536,
        })
        .unwrap();
    let evidence: serde_json::Value = serde_json::from_str(&evidence.content).unwrap();
    assert_eq!(
        evidence["executionProvider"]["contract"],
        "local_linux_runner_v1"
    );
    assert_eq!(
        evidence["executionProvider"]["executableDigest"],
        submit
            .execution_provider
            .as_ref()
            .unwrap()
            .executable_digest
    );
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_host_dependency_drift_fails_before_dispatch() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("host-dependency-drift");
    let runtime = context.runtime(1_000);
    let dependency = context.root.join("runtime-prerequisite.bin");
    fs::write(&dependency, b"HOST_DEP_V1").unwrap();
    let expected_digest = file_digest(&dependency);
    let mut submit = context.direct_submit("request:host-dependency-drift", 1);
    submit.execution_provider = Some(ordivon_runtime_core::ExecutionProviderSnapshot {
        contract: ordivon_runtime_core::ExecutionProviderContract::LocalLinuxRunnerV1,
        executable_digest: file_digest(&context.executor.runner_path),
        wsl_distribution: None,
    });
    submit.host_dependencies = vec![HostDependencyBinding {
        path: dependency.to_string_lossy().into_owned(),
        expected_digest: expected_digest.clone(),
    }];
    let created = created_admission(runtime.registry().submit(&submit).unwrap());
    assert_eq!(created.attempt.state, AttemptState::Accepted);
    fs::write(&dependency, b"HOST_DEP_V2").unwrap();
    assert_ne!(file_digest(&dependency), expected_digest);
    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id.clone(),
            wait_ms: 5_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4_096,
            stderr_tail_bytes: 4_096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "failed");
    assert_eq!(
        observed.execution_reason_code.as_deref(),
        Some("HOST_DEPENDENCY_PRECONDITION_DRIFT")
    );
    assert!(observed.execution_terminal);
    let attempt = runtime
        .registry()
        .get_latest_attempt(&created.job.job_id)
        .unwrap()
        .unwrap();
    assert_eq!(attempt.state, AttemptState::Failed);
    assert!(attempt.bundle_digest.is_none());
    assert!(attempt.runner_start_digest.is_none());
    let unit = Command::new("systemctl")
        .args([
            "show",
            &attempt.unit_name,
            "--property=LoadState",
            "--value",
        ])
        .output()
        .unwrap();
    assert_ne!(String::from_utf8_lossy(&unit.stdout).trim(), "loaded");
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_host_dependency_runtime_drift_is_witnessed_after_target_start() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("host-dependency-runtime-drift");
    let dependency = context.root.join("runtime-live-dependency.txt");
    fs::write(&dependency, b"RUNTIME_V1\n").unwrap();
    let gate = context.root.join("runtime-live-gate");
    context.write(
        "runtime_host_dependency_live.py",
        &format!(
            "import pathlib,time\nprint('READY', flush=True)\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint(pathlib.Path({dependency:?}).read_text().strip(), flush=True)\n",
            gate = gate.to_string_lossy(),
            dependency = dependency.to_string_lossy(),
        ),
    );
    let runtime = context.runtime(2_000);
    let mut request = context.request("runtime_host_dependency_live.py", 0);
    request.execution.host_dependencies = vec![HostDependencyBinding {
        path: dependency.to_string_lossy().into_owned(),
        expected_digest: file_digest(&dependency),
    }];
    let started = runtime.run_task(&request).unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    let attempt = runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    let stdout_path = Path::new(&attempt.bundle_path).join("stdout.log");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ready = false;
    while Instant::now() < deadline {
        if fs::read_to_string(&stdout_path)
            .ok()
            .is_some_and(|text| text.contains("READY\n"))
        {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(ready, "target never reached READY after Runner validation");
    let replacement = dependency.with_extension("txt.new");
    fs::write(&replacement, b"RUNTIME_V2\n").unwrap();
    fs::rename(&replacement, &dependency).unwrap();
    fs::write(&gate, b"go").unwrap();
    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id.clone(),
            wait_ms: 10_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 8_192,
            stderr_tail_bytes: 8_192,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "failed");
    assert_eq!(
        observed.execution_reason_code.as_deref(),
        Some("HOST_DEPENDENCY_RUNTIME_DRIFT")
    );
    assert!(observed.execution_terminal);
    let terminal = observed
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == "terminal_evidence")
        .unwrap();
    let evidence = runtime
        .read_artifact(&ArtifactReadRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: observed.job_id.clone(),
            artifact_id: terminal.artifact_id.clone(),
            offset: 0,
            max_bytes: 65_536,
        })
        .unwrap();
    let evidence: serde_json::Value = serde_json::from_str(&evidence.content).unwrap();
    assert_eq!(
        evidence["hostDependencyContinuity"],
        "runtime_path_drift_detected"
    );
    assert_eq!(
        evidence["hostDependencies"][0]["path"],
        dependency.to_string_lossy().as_ref()
    );
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_provider_bound_runner_start_binds_actual_runner_image() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("provider-actual-image");
    let runtime = context.runtime(5_000);
    let mut submit = context.direct_submit("request:provider-actual-image", 1);
    let provider_digest = file_digest(&context.executor.runner_path);
    submit.execution_provider = Some(ordivon_runtime_core::ExecutionProviderSnapshot {
        contract: ordivon_runtime_core::ExecutionProviderContract::LocalLinuxRunnerV1,
        executable_digest: provider_digest.clone(),
        wsl_distribution: None,
    });
    let executable = fs::canonicalize("/usr/bin/sleep").unwrap();
    submit.plan.executable = executable.to_string_lossy().into_owned();
    submit.plan.executable_digest = file_digest(&executable);
    submit.plan.args = vec!["0.75".to_string()];
    submit.plan.timeout_ms = 5_000;
    let created = created_admission(runtime.registry().submit(&submit).unwrap());
    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id.clone(),
            wait_ms: 10_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4_096,
            stderr_tail_bytes: 4_096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "succeeded");
    let attempt = runtime
        .registry()
        .get_latest_attempt(&created.job.job_id)
        .unwrap()
        .unwrap();
    assert!(attempt.runner_start_digest.is_some());
    let runner_start: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&attempt.bundle_path).join("runner-start.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(runner_start["runnerExecutableDigest"], provider_digest);
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, built Runner, and explicit local opt-in"]
fn runtime_executable_runtime_drift_is_witnessed_without_rewriting_script_identity() {
    if std::env::var("ORDIVON_RUN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let context = IntegrationContext::new("executable-runtime-drift");
    let executable = context.root.join("live-agent-script");
    let gate = context.root.join("live-agent-gate");
    fs::write(
        &executable,
        format!(
            "#!/usr/bin/python3\nimport pathlib,time\nprint('FILE='+__file__, flush=True)\ngate=pathlib.Path({gate:?})\nfor _ in range(1000):\n    if gate.exists(): break\n    time.sleep(0.01)\nprint('ORIGINAL_DONE', flush=True)\n",
            gate = gate.to_string_lossy(),
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    let mut executor = context.executor.clone();
    executor.allowed_executable_roots.push(context.root.clone());
    let runtime = Runtime::new(RuntimeConfig {
        registry: context.registry.clone(),
        executor,
        startup_grace_ms: 2_000,
        windows: None,
    })
    .unwrap();
    let mut request = context.request("unused.py", 0);
    request.execution.executable = executable.to_string_lossy().into_owned();
    request.execution.args.clear();
    let started = runtime.run_task(&request).unwrap();
    assert!(matches!(started.status.as_str(), "queued" | "working"));
    let attempt = runtime
        .registry()
        .get_latest_attempt(&started.job_id)
        .unwrap()
        .unwrap();
    let stdout_path = Path::new(&attempt.bundle_path).join("stdout.log");
    let expected_identity = format!("FILE={}\n", executable.display());
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ready = false;
    while Instant::now() < deadline {
        if fs::read_to_string(&stdout_path)
            .ok()
            .is_some_and(|text| text.contains(&expected_identity))
        {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        ready,
        "script did not observe its original pathname identity"
    );
    let replacement = executable.with_extension("new");
    fs::write(&replacement, "#!/bin/sh\nprintf 'REPLACEMENT\\n'\n").unwrap();
    let mut permissions = fs::metadata(&replacement).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&replacement, permissions).unwrap();
    fs::rename(&replacement, &executable).unwrap();
    fs::write(&gate, b"go").unwrap();
    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: started.job_id.clone(),
            wait_ms: 10_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 8_192,
            stderr_tail_bytes: 8_192,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "failed");
    assert_eq!(
        observed.execution_reason_code.as_deref(),
        Some("EXECUTABLE_RUNTIME_DRIFT")
    );
    assert!(observed.stdout_tail.contains(&expected_identity));
    assert!(!observed.stdout_tail.contains("REPLACEMENT"));
    assert_eq!(runtime.registry().active_reservation_count().unwrap(), 0);
}

#[test]
#[ignore = "requires WSL/Windows, root/systemd, a staged launcher source, and explicit local opt-in"]
fn runtime_provider_bound_job_rejects_windows_launcher_drift_before_dispatch() {
    if std::env::var("ORDIVON_RUN_WINDOWS_PROVIDER_DRIFT").as_deref() != Ok("1") {
        return;
    }

    let source_launcher = PathBuf::from(
        std::env::var("ORDIVON_WINDOWS_LAUNCHER_PATH").expect("ORDIVON_WINDOWS_LAUNCHER_PATH"),
    );
    let wsl_distribution = std::env::var("ORDIVON_WINDOWS_WSL_DISTRIBUTION")
        .unwrap_or_else(|_| "archlinux".to_string());
    let public_root = PathBuf::from("/mnt/c/Users/Public")
        .join(format!("ordivon-provider-drift-{}", Uuid::now_v7()));
    fs::create_dir(&public_root).unwrap();
    let staged_launcher = public_root.join("ordivon-windows-job-launcher.exe");
    fs::copy(&source_launcher, &staged_launcher).unwrap();

    let context = IntegrationContext::new("provider-bound-windows");
    let mut executor = context.executor.clone();
    executor
        .allowed_executable_roots
        .push(PathBuf::from("/mnt/c/Windows/System32"));
    let runtime = Runtime::new(RuntimeConfig {
        registry: context.registry.clone(),
        executor,
        startup_grace_ms: 1_000,
        windows: Some(WindowsExecutionConfig {
            launcher_path: staged_launcher.clone(),
            wsl_distribution: wsl_distribution.clone(),
        }),
    })
    .unwrap();

    let powershell = PathBuf::from("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut submit = context.direct_submit("request:provider-bound-windows", 1);
    submit.plan.execution_target = ordivon_runtime_core::ExecutionTarget::WindowsNative;
    submit.plan.executable = powershell.to_string_lossy().into_owned();
    submit.plan.executable_digest = file_digest(&powershell);
    submit.execution_provider = Some(ordivon_runtime_core::ExecutionProviderSnapshot {
        contract: ordivon_runtime_core::ExecutionProviderContract::WindowsNativeLauncherV1,
        executable_digest: file_digest(&staged_launcher),
        wsl_distribution: Some(wsl_distribution),
    });
    let created = created_admission(runtime.registry().submit(&submit).unwrap());
    assert_eq!(created.attempt.state, AttemptState::Accepted);

    fs::copy("/mnt/c/Windows/System32/cmd.exe", &staged_launcher).unwrap();
    assert_ne!(
        file_digest(&staged_launcher),
        submit
            .execution_provider
            .as_ref()
            .unwrap()
            .executable_digest
    );

    let observed = runtime
        .observe_task(&TaskObserveRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            job_id: created.job.job_id.clone(),
            wait_ms: 5_000,
            wait_until: TaskObserveWaitUntil::Terminal,
            stdout_tail_bytes: 4_096,
            stderr_tail_bytes: 4_096,
            stdout_offset: None,
            stderr_offset: None,
        })
        .unwrap();
    assert_eq!(observed.status, "failed");
    assert_eq!(
        observed.execution_reason_code.as_deref(),
        Some("EXECUTION_PROVIDER_PRECONDITION_DRIFT")
    );
    assert!(!observed
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == "windows_start"));
    let attempt = runtime
        .registry()
        .get_latest_attempt(&created.job.job_id)
        .unwrap()
        .unwrap();
    let unit = Command::new("systemctl")
        .args([
            "show",
            &attempt.unit_name,
            "--property=LoadState",
            "--value",
        ])
        .output()
        .unwrap();
    assert_ne!(String::from_utf8_lossy(&unit.stdout).trim(), "loaded");

    fs::remove_dir_all(public_root).unwrap();
}
