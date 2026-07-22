#![cfg(feature = "runtime-hardening-m7")]

use ordivon_exec::{
    create_git_workspace, write_workspace_text, AdmissionOutcomeM6, AttemptState,
    GitWorkspaceCreateRequest, M6ExecutionPlan, M6Registry, M6RegistryConfig, M6Runtime,
    M6RuntimeConfig, M6SubmitRequest, M6TaskRunRequest, M6UniversalExecutionRequest,
    M7LifecyclePolicy, M7RuntimeHardeningConfig, M7WorkerIdentity, PlanKind, TerminalCommitM6,
    UniversalExecutorConfig, WorkspaceWriteRequest, M6_SCHEMA_VERSION, MAX_UNIVERSAL_OUTPUT_BYTES,
    MAX_UNIVERSAL_RUNTIME_MS, UNIVERSAL_EXEC_SCHEMA_VERSION,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const RUNNER: &str = "/usr/lib/ordivon/ordivon-task-runner";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RebootManifest {
    schema_version: u32,
    run_id: String,
    source_revision: String,
    old_boot_id: String,
    control_root: String,
    worker_root: String,
    cache_root: String,
    runtime_view_root: String,
    workspace_id: String,
    scenarios: BTreeMap<String, ScenarioIdentity>,
    attempt_count_before: u64,
    prepared_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScenarioIdentity {
    job_id: String,
    attempt_id: String,
    expected_after_reboot: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioResult {
    job_id: String,
    attempt_id: String,
    state: String,
    reservation_state: String,
    expected_states: Vec<String>,
    passed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RebootEvidence {
    schema_version: u32,
    phase: String,
    generated_at_ms: u64,
    source_revision: String,
    old_boot_id: String,
    new_boot_id: String,
    boot_id_changed: bool,
    scenarios: BTreeMap<String, ScenarioResult>,
    attempt_count_before: u64,
    attempt_count_after: u64,
    no_new_attempts: bool,
    nonterminal_after: usize,
    active_reservations_after: u32,
    active_units_after: Vec<String>,
    automatic_redispatch_detected: bool,
    passed: bool,
    claims_not_made: Vec<String>,
}

struct Harness {
    run_id: String,
    repo: PathBuf,
    revision: String,
    control_root: PathBuf,
    worker_root: PathBuf,
    cache_root: PathBuf,
    runtime_view_root: PathBuf,
    workspace_id: String,
    executor: UniversalExecutorConfig,
    runtime: M6Runtime,
}

impl Harness {
    fn new(run_id: &str) -> Self {
        let repo =
            fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        let revision = git(&repo, &["rev-parse", "HEAD"]);
        let control_root = PathBuf::from("/var/lib/ordivon/control/m7-reboot").join(run_id);
        let worker_root = PathBuf::from("/var/lib/ordivon/worker/m7-reboot").join(run_id);
        let cache_root = PathBuf::from("/var/cache/ordivon-worker/m7-reboot").join(run_id);
        let runtime_view_root = PathBuf::from("/run/ordivon/m7-reboot").join(run_id);
        let worker = worker_identity();
        let executor = UniversalExecutorConfig {
            store_root: control_root.join("executor"),
            workspace_root: Some(worker_root.join("workspaces")),
            workspace_uid: Some(worker.uid),
            workspace_gid: Some(worker.gid),
            runner_path: PathBuf::from(RUNNER),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: MAX_UNIVERSAL_RUNTIME_MS,
            max_output_bytes: MAX_UNIVERSAL_OUTPUT_BYTES,
        };
        executor.ensure_store().unwrap();
        let hardening = M7RuntimeHardeningConfig {
            worker,
            control_root: control_root.clone(),
            worker_root: worker_root.clone(),
            cache_root: cache_root.clone(),
            runtime_view_root: runtime_view_root.clone(),
            lifecycle_policy: M7LifecyclePolicy {
                schema_version: 1,
                retention_ms: 604_800_000,
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
            run_id: run_id.to_string(),
            repo,
            revision,
            control_root,
            worker_root,
            cache_root,
            runtime_view_root,
            workspace_id: format!("m7-reboot-workspace-{run_id}"),
            executor,
            runtime,
        }
    }

    fn open_workspace(&self) {
        if self
            .executor
            .workspaces_root()
            .join(&self.workspace_id)
            .exists()
        {
            return;
        }
        create_git_workspace(
            &self.executor,
            &GitWorkspaceCreateRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: self.workspace_id.clone(),
                source_repo: self.repo.to_string_lossy().into_owned(),
                source_revision: self.revision.clone(),
            },
        )
        .unwrap();
    }

    fn write_script(&self, name: &str, content: &str) {
        write_workspace_text(
            &self.executor,
            &WorkspaceWriteRequest {
                schema_version: UNIVERSAL_EXEC_SCHEMA_VERSION,
                workspace_id: self.workspace_id.clone(),
                relative_path: name.to_string(),
                content: content.to_string(),
                expected_digest: None,
            },
        )
        .unwrap();
    }

    fn run_request(&self, label: &str, script: &str) -> M6TaskRunRequest {
        M6TaskRunRequest {
            schema_version: M6_SCHEMA_VERSION,
            client_request_id: format!("m7-reboot:{}:{label}", self.run_id),
            principal: "principal:m7-reboot".to_string(),
            authority_ref: "authority:m7-reboot-local".to_string(),
            policy_id: "policy:m7-reboot".to_string(),
            policy_version: "1".to_string(),
            policy_digest: digest(b"policy:m7-reboot:1"),
            profile_id: None,
            global_limit: 8,
            profile_limit: None,
            execution: M6UniversalExecutionRequest {
                workspace_id: self.workspace_id.clone(),
                executable: "/usr/bin/python3.14".to_string(),
                args: vec![script.to_string()],
                cwd_relative: ".".to_string(),
                env: BTreeMap::new(),
                timeout_ms: 120_000,
                stdout_limit_bytes: 65_536,
                stderr_limit_bytes: 65_536,
            },
            wait_ms: 0,
            stdout_tail_bytes: 4096,
            stderr_tail_bytes: 4096,
        }
    }

    fn registry(&self) -> &M6Registry {
        self.runtime.registry()
    }
}

fn prepare(run_id: &str, manifest_path: &Path) {
    let harness = Harness::new(run_id);
    harness.open_workspace();
    harness.write_script(
        "m7_reboot_long.py",
        "import os,time\nprint(f'M7_REBOOT_LONG_UID={os.getuid()}',flush=True)\ntime.sleep(120)\n",
    );
    harness.write_script(
        "m7_reboot_cancel.py",
        "import os,time\nprint(f'M7_REBOOT_CANCEL_UID={os.getuid()}',flush=True)\ntime.sleep(120)\n",
    );
    harness.write_script(
        "m7_reboot_result.py",
        "import os,time\ntime.sleep(1)\nprint(f'M7_REBOOT_RESULT_UID={os.getuid()}',flush=True)\n",
    );

    let mut scenarios = BTreeMap::new();
    let long = harness
        .runtime
        .run_task(&harness.run_request("running", "m7_reboot_long.py"))
        .unwrap();
    let long_attempt = long.attempt_id.clone().unwrap();
    wait_for_state(harness.registry(), &long_attempt, &[AttemptState::Running]);
    scenarios.insert(
        "runningAtReboot".to_string(),
        ScenarioIdentity {
            job_id: long.job_id,
            attempt_id: long_attempt,
            expected_after_reboot: vec!["lost".to_string()],
        },
    );

    let cancel = harness
        .runtime
        .run_task(&harness.run_request("cancel-pending", "m7_reboot_cancel.py"))
        .unwrap();
    let cancel_attempt = cancel.attempt_id.clone().unwrap();
    wait_for_state(
        harness.registry(),
        &cancel_attempt,
        &[AttemptState::Running],
    );
    harness
        .registry()
        .request_cancel(&cancel.job_id, now_ms())
        .unwrap();
    scenarios.insert(
        "cancelIntentPending".to_string(),
        ScenarioIdentity {
            job_id: cancel.job_id,
            attempt_id: cancel_attempt,
            expected_after_reboot: vec!["lost".to_string(), "cancelled".to_string()],
        },
    );

    let result_pending = harness
        .runtime
        .run_task(&harness.run_request("result-pending", "m7_reboot_result.py"))
        .unwrap();
    let result_attempt = result_pending.attempt_id.clone().unwrap();
    wait_for_file(
        &PathBuf::from(
            harness
                .registry()
                .get_attempt(&result_attempt)
                .unwrap()
                .bundle_path,
        )
        .join("result.json"),
    );
    scenarios.insert(
        "resultPendingCommit".to_string(),
        ScenarioIdentity {
            job_id: result_pending.job_id,
            attempt_id: result_attempt,
            expected_after_reboot: vec!["succeeded".to_string()],
        },
    );

    let ambiguous = created(
        harness
            .registry()
            .submit(&direct_submit(&harness, "ambiguous-dispatch"))
            .unwrap(),
    );
    let ambiguous_bundle = PathBuf::from(&ambiguous.attempt.bundle_path);
    fs::create_dir_all(&ambiguous_bundle).unwrap();
    fs::write(ambiguous_bundle.join("request.json"), b"{}\n").unwrap();
    let ambiguous_attempt = harness
        .registry()
        .mark_bundle_ready(
            &ambiguous.attempt.attempt_id,
            0,
            &digest(b"m7-reboot-ambiguous-bundle"),
            now_ms(),
        )
        .unwrap();
    let ambiguous_attempt = harness
        .registry()
        .mark_dispatch_issued(
            &ambiguous_attempt.attempt_id,
            ambiguous_attempt.row_version,
            now_ms(),
        )
        .unwrap();
    scenarios.insert(
        "dispatchIntentUnbound".to_string(),
        ScenarioIdentity {
            job_id: ambiguous.job.job_id,
            attempt_id: ambiguous_attempt.attempt_id,
            expected_after_reboot: vec!["lost".to_string()],
        },
    );

    let orphan = created(
        harness
            .registry()
            .submit(&direct_submit(&harness, "held-orphan"))
            .unwrap(),
    );
    harness
        .registry()
        .commit_terminal(&TerminalCommitM6 {
            attempt_id: orphan.attempt.attempt_id.clone(),
            expected_row_version: 0,
            state: AttemptState::Orphaned,
            result_digest: digest(b"m7-reboot-held-orphan"),
            exit_code: None,
            infrastructure_error_digest: Some(digest(b"m7-reboot-held-orphan")),
            finished_at_ms: now_ms(),
            artifacts: Vec::new(),
            reason_code: "M7_REBOOT_HELD_ORPHAN".to_string(),
        })
        .unwrap();
    scenarios.insert(
        "heldOrphaned".to_string(),
        ScenarioIdentity {
            job_id: orphan.job.job_id,
            attempt_id: orphan.attempt.attempt_id,
            expected_after_reboot: vec!["orphaned".to_string()],
        },
    );

    let manifest = RebootManifest {
        schema_version: 1,
        run_id: run_id.to_string(),
        source_revision: harness.revision.clone(),
        old_boot_id: boot_id(),
        control_root: harness.control_root.to_string_lossy().into_owned(),
        worker_root: harness.worker_root.to_string_lossy().into_owned(),
        cache_root: harness.cache_root.to_string_lossy().into_owned(),
        runtime_view_root: harness.runtime_view_root.to_string_lossy().into_owned(),
        workspace_id: harness.workspace_id.clone(),
        scenarios,
        attempt_count_before: attempt_count(harness.registry().config().db_path.as_path()),
        prepared_at_ms: now_ms(),
    };
    atomic_json(manifest_path, &manifest);
    println!("M7_REBOOT_PREPARED {}", manifest_path.display());
}

fn collect(manifest_path: &Path, evidence_path: &Path) {
    let manifest: RebootManifest =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    let harness = Harness::from_manifest(&manifest);
    let new_boot_id = boot_id();
    let reconciliation = harness.runtime.reconcile_all().unwrap();
    let mut scenarios = BTreeMap::new();
    for (name, identity) in &manifest.scenarios {
        let attempt = harness
            .registry()
            .get_attempt(&identity.attempt_id)
            .unwrap();
        let reservation = harness
            .registry()
            .get_reservation(&identity.attempt_id)
            .unwrap();
        let state = format!("{:?}", attempt.state).to_ascii_lowercase();
        let reservation_state = match reservation.state {
            ordivon_exec::ReservationState::Active => "active",
            ordivon_exec::ReservationState::HeldOrphaned => "held_orphaned",
            ordivon_exec::ReservationState::Released => "released",
        }
        .to_string();
        let passed = identity.expected_after_reboot.contains(&state)
            && if name == "heldOrphaned" {
                reservation_state == "held_orphaned"
            } else {
                reservation_state == "released"
            };
        scenarios.insert(
            name.clone(),
            ScenarioResult {
                job_id: identity.job_id.clone(),
                attempt_id: identity.attempt_id.clone(),
                state,
                reservation_state,
                expected_states: identity.expected_after_reboot.clone(),
                passed,
            },
        );
    }
    let attempt_count_after = attempt_count(harness.registry().config().db_path.as_path());
    let active_units_after = active_attempt_units();
    let nonterminal_after = harness
        .registry()
        .list_nonterminal_attempts()
        .unwrap()
        .len();
    let active_reservations_after = harness.registry().active_reservation_count().unwrap();
    let no_new_attempts = attempt_count_after == manifest.attempt_count_before;
    let automatic_redispatch_detected = !active_units_after.is_empty()
        || reconciliation.iter().any(|item| {
            !manifest
                .scenarios
                .values()
                .any(|scenario| scenario.job_id == item.job_id)
        });
    let passed = new_boot_id != manifest.old_boot_id
        && no_new_attempts
        && !automatic_redispatch_detected
        && scenarios.values().all(|scenario| scenario.passed)
        && nonterminal_after == 0
        && active_reservations_after == 1;
    let boot_id_changed = new_boot_id != manifest.old_boot_id;
    let evidence = RebootEvidence {
        schema_version: 1,
        phase: "ORDIVON-MIGRATION-M7-REBOOT-RECOVERY-2026-07-22".to_string(),
        generated_at_ms: now_ms(),
        source_revision: manifest.source_revision,
        old_boot_id: manifest.old_boot_id,
        new_boot_id,
        boot_id_changed,
        scenarios,
        attempt_count_before: manifest.attempt_count_before,
        attempt_count_after,
        no_new_attempts,
        nonterminal_after,
        active_reservations_after,
        active_units_after,
        automatic_redispatch_detected,
        passed,
        claims_not_made: vec![
            "One local WSL reboot does not establish production reboot reliability.".to_string(),
            "The evidence does not authorize automatic retry after ambiguous dispatch.".to_string(),
            "The held orphan remains intentionally capacity-bearing after reboot.".to_string(),
            "The reboot result does not authorize production routing or remote execution."
                .to_string(),
        ],
    };
    atomic_json(evidence_path, &evidence);
    if !evidence.passed {
        panic!("M7 reboot evidence failed: {}", evidence_path.display());
    }
    println!("M7_REBOOT_EVIDENCE_PASS {}", evidence_path.display());
}

impl Harness {
    fn from_manifest(manifest: &RebootManifest) -> Self {
        let control_root = PathBuf::from(&manifest.control_root);
        let worker_root = PathBuf::from(&manifest.worker_root);
        let cache_root = PathBuf::from(&manifest.cache_root);
        let runtime_view_root = PathBuf::from(&manifest.runtime_view_root);
        let worker = worker_identity();
        let executor = UniversalExecutorConfig {
            store_root: control_root.join("executor"),
            workspace_root: Some(worker_root.join("workspaces")),
            workspace_uid: Some(worker.uid),
            workspace_gid: Some(worker.gid),
            runner_path: PathBuf::from(RUNNER),
            allowed_executable_roots: vec![PathBuf::from("/usr/bin")],
            max_runtime_ms: MAX_UNIVERSAL_RUNTIME_MS,
            max_output_bytes: MAX_UNIVERSAL_OUTPUT_BYTES,
        };
        let hardening = M7RuntimeHardeningConfig {
            worker,
            control_root: control_root.clone(),
            worker_root: worker_root.clone(),
            cache_root: cache_root.clone(),
            runtime_view_root: runtime_view_root.clone(),
            lifecycle_policy: M7LifecyclePolicy {
                schema_version: 1,
                retention_ms: 604_800_000,
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
            run_id: manifest.run_id.clone(),
            repo: fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .unwrap(),
            revision: manifest.source_revision.clone(),
            control_root,
            worker_root,
            cache_root,
            runtime_view_root,
            workspace_id: manifest.workspace_id.clone(),
            executor,
            runtime,
        }
    }
}

fn direct_submit(harness: &Harness, label: &str) -> M6SubmitRequest {
    let executable = fs::canonicalize("/usr/bin/true").unwrap();
    let workspace_path = harness
        .executor
        .workspaces_root()
        .join(&harness.workspace_id);
    M6SubmitRequest {
        schema_version: M6_SCHEMA_VERSION,
        client_request_id: format!("m7-reboot:{}:{label}", harness.run_id),
        plan: M6ExecutionPlan {
            schema_version: M6_SCHEMA_VERSION,
            plan_kind: PlanKind::UniversalSandbox,
            workspace_id: harness.workspace_id.clone(),
            workspace_path: workspace_path.to_string_lossy().into_owned(),
            source_revision: harness.revision.clone(),
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: file_digest(&executable),
            args: Vec::new(),
            cwd: workspace_path.to_string_lossy().into_owned(),
            env: BTreeMap::new(),
            timeout_ms: 10_000,
            stdout_limit_bytes: 1024,
            stderr_limit_bytes: 1024,
            policy_id: "policy:m7-reboot".to_string(),
            policy_version: "1".to_string(),
            policy_digest: digest(b"policy:m7-reboot:1"),
            profile_id: None,
            principal: "principal:m7-reboot".to_string(),
            authority_ref: "authority:m7-reboot-local".to_string(),
        },
        global_limit: 8,
        profile_limit: None,
        lifecycle_quota: None,
    }
}

fn created(outcome: AdmissionOutcomeM6) -> ordivon_exec::CreatedAdmissionM6 {
    match outcome {
        AdmissionOutcomeM6::Created(value) => *value,
        AdmissionOutcomeM6::Existing { .. } => panic!("expected created admission"),
    }
}

fn worker_identity() -> M7WorkerIdentity {
    let output = Command::new("getent")
        .args(["passwd", "ordivon-worker"])
        .output()
        .unwrap();
    assert!(output.status.success(), "ordivon-worker is not installed");
    let line = String::from_utf8(output.stdout).unwrap();
    let fields = line.trim().split(':').collect::<Vec<_>>();
    M7WorkerIdentity {
        user: "ordivon-worker".to_string(),
        group: "ordivon-worker".to_string(),
        uid: fields[2].parse().unwrap(),
        gid: fields[3].parse().unwrap(),
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn file_digest(path: &Path) -> String {
    digest(&fs::read(path).unwrap())
}

fn boot_id() -> String {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .unwrap()
        .trim()
        .to_string()
}

fn now_ms() -> u64 {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn wait_for_state(registry: &M6Registry, attempt_id: &str, expected: &[AttemptState]) {
    let deadline = SystemTime::now() + Duration::from_secs(10);
    loop {
        let attempt = registry.get_attempt(attempt_id).unwrap();
        if expected.contains(&attempt.state) {
            return;
        }
        assert!(
            SystemTime::now() < deadline,
            "Attempt {attempt_id} remained {:?}",
            attempt.state
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_file(path: &Path) {
    let deadline = SystemTime::now() + Duration::from_secs(10);
    while !path.is_file() {
        assert!(
            SystemTime::now() < deadline,
            "file did not appear: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn attempt_count(database: &Path) -> u64 {
    let connection = Connection::open(database).unwrap();
    connection
        .query_row("SELECT COUNT(*) FROM attempts", [], |row| row.get(0))
        .unwrap()
}

fn active_attempt_units() -> Vec<String> {
    let output = Command::new("systemctl")
        .args([
            "list-units",
            "--all",
            "ordivon-m6-attempt-*",
            "--no-legend",
            "--plain",
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(ToString::to_string)
        .collect()
}

fn atomic_json<T: Serialize>(path: &Path, value: &T) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let temporary = path.with_extension(format!("tmp-{}", Uuid::now_v7()));
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    fs::write(&temporary, bytes).unwrap();
    fs::rename(temporary, path).unwrap();
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    match args.as_slice() {
        [_, command, run_id, path] if command == "prepare" => {
            prepare(run_id, Path::new(path));
        }
        [_, command, manifest, evidence] if command == "collect" => {
            collect(Path::new(manifest), Path::new(evidence));
        }
        _ => {
            eprintln!(
                "usage: m7_reboot_harness prepare RUN_ID MANIFEST | collect MANIFEST EVIDENCE"
            );
            std::process::exit(2);
        }
    }
}
