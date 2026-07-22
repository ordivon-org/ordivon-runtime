use ordivon_exec::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn digest(value: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(value))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as u64
}

fn request(root: &Path, id: &str, source_revision: &str) -> M6SubmitRequest {
    let executable = fs::canonicalize("/usr/bin/true").expect("canonical executable");
    M6SubmitRequest {
        schema_version: M6_SCHEMA_VERSION,
        client_request_id: id.to_string(),
        plan: M6ExecutionPlan {
            schema_version: M6_SCHEMA_VERSION,
            plan_kind: PlanKind::UniversalSandbox,
            workspace_id: "workspace:bench".to_string(),
            workspace_path: root.to_string_lossy().into_owned(),
            source_revision: source_revision.to_string(),
            executable: executable.to_string_lossy().into_owned(),
            executable_digest: digest(&fs::read(executable).expect("read executable")),
            args: Vec::new(),
            cwd: root.to_string_lossy().into_owned(),
            env: BTreeMap::new(),
            timeout_ms: 10_000,
            stdout_limit_bytes: 65_536,
            stderr_limit_bytes: 65_536,
            policy_id: "policy:bench".to_string(),
            policy_version: "1".to_string(),
            policy_digest: digest(b"policy:bench:1"),
            profile_id: None,
            principal: "principal:bench".to_string(),
            authority_ref: "authority:bench".to_string(),
        },
        global_limit: 1000,
        profile_limit: None,
    }
}

fn created(value: AdmissionOutcomeM6) -> CreatedAdmissionM6 {
    match value {
        AdmissionOutcomeM6::Created(created) => *created,
        AdmissionOutcomeM6::Existing { .. } => panic!("expected new admission"),
    }
}

fn micros(operation: impl FnOnce()) -> u64 {
    let started = Instant::now();
    operation();
    started.elapsed().as_micros() as u64
}

fn summary(values: &[u64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    json!({
        "samples": count,
        "p50Us": sorted[count / 2],
        "p95Us": sorted[(count * 95).div_ceil(100).saturating_sub(1)],
        "maxUs": sorted[count - 1],
    })
}

fn main() {
    let source_revision = std::env::args().nth(1).expect("source revision argument");
    let root = PathBuf::from(format!(
        "/root/.local/share/ordivon-m6-registry-bench-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("workspace")).expect("create benchmark workspace");
    let registry = M6Registry::initialize(M6RegistryConfig {
        db_path: root.join("registry.sqlite3"),
        store_root: root.join("store"),
        busy_timeout_ms: 5000,
    })
    .expect("initialize registry");

    let mut admission = Vec::new();
    for index in 0..50 {
        let request = request(&root, &format!("admission-{index}"), &source_revision);
        admission.push(micros(|| {
            registry.submit(&request).expect("admission");
        }));
    }

    let replay_request = request(&root, "replay", &source_revision);
    let replay_job = created(registry.submit(&replay_request).expect("create replay job")).job;
    let mut replay = Vec::new();
    for _ in 0..100 {
        replay.push(micros(|| {
            registry.submit(&replay_request).expect("replay");
        }));
    }
    let mut status = Vec::new();
    for _ in 0..100 {
        status.push(micros(|| {
            registry.project_job(&replay_job.job_id).expect("status");
        }));
    }

    for index in 0..110 {
        registry
            .submit(&request(&root, &format!("list-{index}"), &source_revision))
            .expect("list fixture");
    }
    let mut list = Vec::new();
    for _ in 0..30 {
        list.push(micros(|| {
            let result = registry
                .list_jobs(&JobListRequestM6 {
                    limit: 100,
                    cursor: None,
                })
                .expect("list jobs");
            assert_eq!(result.jobs.len(), 100);
        }));
    }

    let mut terminal = Vec::new();
    for index in 0..40 {
        let created = created(
            registry
                .submit(&request(
                    &root,
                    &format!("terminal-{index}"),
                    &source_revision,
                ))
                .expect("terminal fixture"),
        );
        let attempt = registry
            .mark_bundle_ready(
                &created.attempt.attempt_id,
                created.attempt.row_version,
                &digest(format!("bundle-{index}").as_bytes()),
                now_ms(),
            )
            .expect("bundle ready");
        let attempt = registry
            .mark_dispatch_issued(&attempt.attempt_id, attempt.row_version, now_ms())
            .expect("dispatch intent");
        let attempt = registry
            .bind_running(
                &attempt.attempt_id,
                attempt.row_version,
                &RunnerIdentityM6 {
                    boot_id: "boot-bench".to_string(),
                    unit_name: attempt.unit_name.clone(),
                    invocation_id: format!("invocation-{index}"),
                    control_group: format!("/bench/{index}"),
                    main_pid: (index + 1) as u32,
                    process_start_identity: format!("start-{index}"),
                    runner_start_digest: digest(format!("runner-{index}").as_bytes()),
                    observed_at_ms: now_ms(),
                },
            )
            .expect("bind running");
        terminal.push(micros(|| {
            registry
                .commit_terminal(&TerminalCommitM6 {
                    attempt_id: attempt.attempt_id.clone(),
                    expected_row_version: attempt.row_version,
                    state: AttemptState::Succeeded,
                    result_digest: digest(format!("result-{index}").as_bytes()),
                    exit_code: Some(0),
                    infrastructure_error_digest: None,
                    finished_at_ms: now_ms(),
                    artifacts: Vec::new(),
                    reason_code: "BENCH_SUCCESS".to_string(),
                })
                .expect("terminal commit");
        }));
    }

    let summaries = json!({
        "admission": summary(&admission),
        "replay": summary(&replay),
        "status": summary(&status),
        "list100": summary(&list),
        "terminal": summary(&terminal),
    });
    let p95 = |name: &str| summaries[name]["p95Us"].as_u64().expect("p95");
    let evidence = json!({
        "schemaVersion": 1,
        "phase": "ORDIVON-MIGRATION-M6-REGISTRY-PERFORMANCE-2026-07-22",
        "sourceRevision": source_revision,
        "rawSamplesUs": {
            "admission": admission,
            "replay": replay,
            "status": status,
            "list100": list,
            "terminal": terminal,
        },
        "summaries": summaries,
        "gates": {
            "admissionP95AtMost50Ms": p95("admission") <= 50_000,
            "replayP95AtMost20Ms": p95("replay") <= 20_000,
            "statusP95AtMost20Ms": p95("status") <= 20_000,
            "list100P95AtMost50Ms": p95("list100") <= 50_000,
            "terminalP95AtMost50Ms": p95("terminal") <= 50_000,
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&evidence).expect("serialize evidence")
    );
    let _ = fs::remove_dir_all(root);
}
