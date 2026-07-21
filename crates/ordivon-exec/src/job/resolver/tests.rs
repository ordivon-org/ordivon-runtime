use super::filesystem::digest_reader;
use super::*;
use crate::job::EnvironmentRule;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::time::{SystemTime, UNIX_EPOCH};

struct Sandbox {
    root: PathBuf,
    executable: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ordivon-capability-resolver-{}-{unique}",
            std::process::id()
        ));
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let executable = root.join("tool");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        Self {
            root,
            executable,
            workspace,
        }
    }

    fn policy(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            schema_version: JOB_CONTRACT_SCHEMA_VERSION,
            policy_id: "ordivon.execution.test.v1".to_string(),
            policy_version: "1".to_string(),
            allowed_roots: vec![self.root.to_string_lossy().to_string()],
            global_max_concurrency: 2,
            profiles: vec![ExecutionProfile {
                profile_id: "fixture.run".to_string(),
                enabled: true,
                executable: self.executable.to_string_lossy().to_string(),
                executable_digest: file_digest(&self.executable),
                fixed_args: vec!["fixed".to_string()],
                allowed_argument_vectors: vec![vec!["allowed".to_string()]],
                allowed_cwd_roots: vec![self.workspace.to_string_lossy().to_string()],
                base_environment: BTreeMap::from([
                    ("HOME".to_string(), "/nonexistent".to_string()),
                    ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                ]),
                environment_rules: vec![EnvironmentRule {
                    name: "RUST_LOG".to_string(),
                    allowed_values: vec!["info".to_string(), "warn".to_string()],
                }],
                max_runtime_ms: 5_000,
                max_stdout_bytes: 4096,
                max_stderr_bytes: 4096,
                max_concurrency: 1,
                terminate_on_output_limit: false,
            }],
        }
    }

    fn request(&self) -> JobStartRequest {
        JobStartRequest {
            profile_id: "fixture.run".to_string(),
            args: vec!["allowed".to_string()],
            cwd: self.workspace.to_string_lossy().to_string(),
            timeout_ms: 1_000,
            env_overrides: BTreeMap::from([("RUST_LOG".to_string(), "info".to_string())]),
            stdout_retention_bytes: 1024,
            stderr_retention_bytes: 1024,
            client_request_id: "request:test:1".to_string(),
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn context() -> CapabilityEvaluationContext {
    CapabilityEvaluationContext {
        principal: "chatgpt:user".to_string(),
        authority_ref: "authority:test:1".to_string(),
        concurrency: ConcurrencySnapshot {
            global_running: 0,
            profile_running: 0,
        },
    }
}

fn file_digest(path: &Path) -> String {
    let mut file = File::open(path).unwrap();
    digest_reader(&mut file).unwrap()
}

fn policy_json(policy: &CapabilityPolicy) -> Vec<u8> {
    serde_json::to_vec_pretty(policy).unwrap()
}

#[test]
fn valid_policy_resolves_and_produces_a_closed_execution_plan() {
    let sandbox = Sandbox::new();
    let policy = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    let plan = evaluate_job_start(&policy, &sandbox.request(), &context()).unwrap();
    assert_eq!(plan.executable, sandbox.executable.to_string_lossy());
    assert_eq!(plan.argv, vec!["fixed", "allowed"]);
    assert_eq!(plan.cwd, sandbox.workspace.to_string_lossy());
    assert_eq!(
        plan.env.get("PATH").map(String::as_str),
        Some("/usr/bin:/bin")
    );
    assert_eq!(plan.env.get("RUST_LOG").map(String::as_str), Some("info"));
    assert!(plan.policy_digest.starts_with("sha256:"));
    assert!(plan.request_digest.starts_with("sha256:"));
}

#[test]
fn policy_digest_is_independent_of_json_object_key_order_and_whitespace() {
    let sandbox = Sandbox::new();
    let value = serde_json::to_value(sandbox.policy()).unwrap();
    let compact = serde_json::to_vec(&value).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    let left = load_capability_policy_bytes(&compact).unwrap();
    let right = load_capability_policy_bytes(&pretty).unwrap();
    assert_eq!(left.policy_digest(), right.policy_digest());
}

#[test]
fn policy_file_must_not_be_a_symlink() {
    let sandbox = Sandbox::new();
    let real = sandbox.root.join("policy.json");
    let link = sandbox.root.join("policy-link.json");
    fs::write(&real, policy_json(&sandbox.policy())).unwrap();
    symlink(&real, &link).unwrap();
    assert_eq!(
        load_capability_policy_file(&link).unwrap_err().code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn executable_must_not_be_a_symlink() {
    let sandbox = Sandbox::new();
    let link = sandbox.root.join("tool-link");
    symlink(&sandbox.executable, &link).unwrap();
    let mut policy = sandbox.policy();
    policy.profiles[0].executable = link.to_string_lossy().to_string();
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&policy))
            .unwrap_err()
            .code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn executable_digest_is_verified_at_load() {
    let sandbox = Sandbox::new();
    let mut policy = sandbox.policy();
    policy.profiles[0].executable_digest = format!("sha256:{}", "0".repeat(64));
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&policy))
            .unwrap_err()
            .field
            .as_deref(),
        Some("executableDigest")
    );
}

#[test]
fn executable_identity_is_rechecked_before_plan_creation() {
    let sandbox = Sandbox::new();
    let policy = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    fs::write(&sandbox.executable, b"#!/bin/sh\nexit 7\n").unwrap();
    assert_eq!(
        evaluate_job_start(&policy, &sandbox.request(), &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn canonical_cwd_cannot_escape_through_a_symlink() {
    let sandbox = Sandbox::new();
    let outside = sandbox
        .root
        .parent()
        .unwrap()
        .join(format!("ordivon-capability-outside-{}", std::process::id()));
    fs::create_dir_all(&outside).unwrap();
    let escape = sandbox.workspace.join("escape");
    symlink(&outside, &escape).unwrap();
    let mut request = sandbox.request();
    request.cwd = escape.to_string_lossy().to_string();
    assert_eq!(
        evaluate_job_start(
            &load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap(),
            &request,
            &context()
        )
        .unwrap_err()
        .code,
        JobContractErrorCode::PathScopeDenied
    );
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn canonical_root_aliases_are_rejected_as_duplicates() {
    let sandbox = Sandbox::new();
    let alias = sandbox.root.join("workspace-alias");
    symlink(&sandbox.workspace, &alias).unwrap();
    let mut policy = sandbox.policy();
    policy.profiles[0]
        .allowed_cwd_roots
        .push(alias.to_string_lossy().to_string());
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&policy))
            .unwrap_err()
            .code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn profile_roots_must_be_inside_global_policy_roots() {
    let sandbox = Sandbox::new();
    let outside = sandbox.root.parent().unwrap().to_path_buf();
    let mut policy = sandbox.policy();
    policy.allowed_roots = vec![sandbox.workspace.to_string_lossy().to_string()];
    policy.profiles[0].allowed_cwd_roots = vec![outside.to_string_lossy().to_string()];
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&policy))
            .unwrap_err()
            .field
            .as_deref(),
        Some("allowedCwdRoots")
    );
}

#[test]
fn arguments_are_exact_not_prefix_or_subsequence_matches() {
    let sandbox = Sandbox::new();
    let policy = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    let mut request = sandbox.request();
    request.args.push("extra".to_string());
    assert_eq!(
        evaluate_job_start(&policy, &request, &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::ArgumentPolicyDenied
    );
}

#[test]
fn unlisted_and_wrong_value_environment_overrides_are_denied() {
    let sandbox = Sandbox::new();
    let policy = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    let mut unlisted = sandbox.request();
    unlisted.env_overrides = BTreeMap::from([("OTHER".to_string(), "1".to_string())]);
    assert_eq!(
        evaluate_job_start(&policy, &unlisted, &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::EnvironmentDenied
    );
    let mut wrong = sandbox.request();
    wrong
        .env_overrides
        .insert("RUST_LOG".to_string(), "debug".to_string());
    assert_eq!(
        evaluate_job_start(&policy, &wrong, &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::EnvironmentDenied
    );
}

#[test]
fn execution_shaping_environment_cannot_be_client_controlled() {
    let sandbox = Sandbox::new();
    let mut policy = sandbox.policy();
    policy.profiles[0].environment_rules.push(EnvironmentRule {
        name: "PATH".to_string(),
        allowed_values: vec!["/tmp".to_string()],
    });
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&policy))
            .unwrap_err()
            .field
            .as_deref(),
        Some("environmentRules")
    );
}

#[test]
fn loader_rejects_injection_environment_even_when_policy_authored() {
    let sandbox = Sandbox::new();
    let mut policy = sandbox.policy();
    policy.profiles[0]
        .base_environment
        .insert("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string());
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&policy))
            .unwrap_err()
            .field
            .as_deref(),
        Some("baseEnvironment")
    );
}

#[test]
fn timeout_and_output_limits_are_enforced() {
    let sandbox = Sandbox::new();
    let policy = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    let mut timeout = sandbox.request();
    timeout.timeout_ms = 5_001;
    assert_eq!(
        evaluate_job_start(&policy, &timeout, &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::TimeoutExceedsPolicy
    );
    let mut output = sandbox.request();
    output.stdout_retention_bytes = 4_097;
    assert_eq!(
        evaluate_job_start(&policy, &output, &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::OutputLimitExceedsPolicy
    );
}

#[test]
fn concurrency_denials_are_retryable() {
    let sandbox = Sandbox::new();
    let policy = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    let mut global = context();
    global.concurrency.global_running = 2;
    let error = evaluate_job_start(&policy, &sandbox.request(), &global).unwrap_err();
    assert_eq!(error.code, JobContractErrorCode::ConcurrencyLimit);
    assert!(error.retryable);
    let mut profile = context();
    profile.concurrency.profile_running = 1;
    assert!(
        evaluate_job_start(&policy, &sandbox.request(), &profile)
            .unwrap_err()
            .retryable
    );
}

#[test]
fn disabled_and_unknown_profiles_have_distinct_errors() {
    let sandbox = Sandbox::new();
    let mut disabled_policy = sandbox.policy();
    disabled_policy.profiles[0].enabled = false;
    let disabled = load_capability_policy_bytes(&policy_json(&disabled_policy)).unwrap();
    assert_eq!(
        evaluate_job_start(&disabled, &sandbox.request(), &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::ProfileDisabled
    );
    let enabled = load_capability_policy_bytes(&policy_json(&sandbox.policy())).unwrap();
    let mut unknown = sandbox.request();
    unknown.profile_id = "missing".to_string();
    assert_eq!(
        evaluate_job_start(&enabled, &unknown, &context())
            .unwrap_err()
            .code,
        JobContractErrorCode::ProfileNotFound
    );
}

#[test]
fn policy_payload_is_strictly_bounded() {
    assert_eq!(
        load_capability_policy_bytes(&vec![b' '; MAX_CAPABILITY_POLICY_BYTES as usize + 1])
            .unwrap_err()
            .code,
        JobContractErrorCode::PolicyInvalid
    );
}

#[test]
fn duplicate_json_keys_are_rejected_before_typed_deserialization() {
    let sandbox = Sandbox::new();
    let mut value = String::from_utf8(policy_json(&sandbox.policy())).unwrap();
    value = value.replacen(
        "\"policyId\": \"ordivon.execution.test.v1\",",
        "\"policyId\": \"shadow\",\n  \"policyId\": \"ordivon.execution.test.v1\",",
        1,
    );
    let error = load_capability_policy_bytes(value.as_bytes()).unwrap_err();
    assert_eq!(error.code, JobContractErrorCode::PolicyInvalid);
    assert!(error.message.contains("duplicate JSON key policyId"));
}

#[test]
fn writable_policy_files_and_executables_are_rejected() {
    let sandbox = Sandbox::new();
    let policy_path = sandbox.root.join("policy.json");
    fs::write(&policy_path, policy_json(&sandbox.policy())).unwrap();
    let mut policy_permissions = fs::metadata(&policy_path).unwrap().permissions();
    policy_permissions.set_mode(0o666);
    fs::set_permissions(&policy_path, policy_permissions).unwrap();
    assert_eq!(
        load_capability_policy_file(&policy_path)
            .unwrap_err()
            .field
            .as_deref(),
        Some("policyPath")
    );

    let mut executable_permissions = fs::metadata(&sandbox.executable).unwrap().permissions();
    executable_permissions.set_mode(0o777);
    fs::set_permissions(&sandbox.executable, executable_permissions).unwrap();
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&sandbox.policy()))
            .unwrap_err()
            .field
            .as_deref(),
        Some("executable")
    );
}

#[test]
fn runtime_and_concurrency_policy_ceilings_are_fail_closed() {
    let sandbox = Sandbox::new();
    let mut runtime = sandbox.policy();
    runtime.profiles[0].max_runtime_ms = MAX_EXECUTION_RUNTIME_MS + 1;
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&runtime))
            .unwrap_err()
            .field
            .as_deref(),
        Some("maxRuntimeMs")
    );

    let mut concurrency = sandbox.policy();
    concurrency.global_max_concurrency = MAX_EXECUTION_CONCURRENCY + 1;
    assert_eq!(
        load_capability_policy_bytes(&policy_json(&concurrency))
            .unwrap_err()
            .field
            .as_deref(),
        Some("globalMaxConcurrency")
    );
}
