mod environment;
mod filesystem;
mod json;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use environment::{is_forbidden_base_environment, is_forbidden_client_override};
use filesystem::{
    canonical_directory, canonical_roots, inspect_executable, path_to_string, same_file,
    FileIdentity,
};
use json::{canonical_digest, reject_duplicate_json_keys};

use super::validation::validate_identifier;
use super::{
    CapabilityPolicy, ExecutionPlan, ExecutionProfile, JobContractError, JobContractErrorCode,
    JobStartRequest, JOB_CONTRACT_SCHEMA_VERSION, MAX_JOB_ARGS, MAX_JOB_ENV_VARS,
};

pub const MAX_CAPABILITY_POLICY_BYTES: u64 = 1024 * 1024;
pub const MAX_CAPABILITY_PROFILES: usize = 64;
pub const MAX_CAPABILITY_ROOTS: usize = 64;
pub const MAX_ALLOWED_ARGUMENT_VECTORS: usize = 64;
pub const MAX_EXECUTION_RUNTIME_MS: u64 = 24 * 60 * 60 * 1000;
pub const MAX_EXECUTION_CONCURRENCY: u32 = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcurrencySnapshot {
    pub global_running: u32,
    pub profile_running: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityEvaluationContext {
    pub principal: String,
    pub authority_ref: String,
    pub concurrency: ConcurrencySnapshot,
}

#[derive(Clone, Debug)]
pub struct ResolvedCapabilityPolicy {
    policy: CapabilityPolicy,
    policy_digest: String,
    canonical_allowed_roots: Vec<PathBuf>,
    profiles: BTreeMap<String, ResolvedExecutionProfile>,
}

#[derive(Clone, Debug)]
struct ResolvedExecutionProfile {
    source: ExecutionProfile,
    canonical_executable: PathBuf,
    executable_identity: FileIdentity,
    canonical_cwd_roots: Vec<PathBuf>,
}

impl ResolvedCapabilityPolicy {
    pub fn policy(&self) -> &CapabilityPolicy {
        &self.policy
    }

    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }

    pub fn canonical_allowed_roots(&self) -> &[PathBuf] {
        &self.canonical_allowed_roots
    }

    pub fn profile_ids(&self) -> impl Iterator<Item = &str> {
        self.profiles.keys().map(String::as_str)
    }
}

pub fn load_capability_policy_file(
    path: impl AsRef<Path>,
) -> Result<ResolvedCapabilityPolicy, JobContractError> {
    let path = path.as_ref();
    let before = fs::symlink_metadata(path)
        .map_err(|error| policy_io_error("policyPath", "cannot inspect policy file", error))?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(policy_error(
            "policy file must be a non-symlink regular file",
            "policyPath",
        ));
    }
    if before.permissions().mode() & 0o022 != 0 {
        return Err(policy_error(
            "policy file must not be group- or world-writable",
            "policyPath",
        ));
    }
    if before.len() == 0 || before.len() > MAX_CAPABILITY_POLICY_BYTES {
        return Err(policy_error(
            format!("policy file size must be in 1..={MAX_CAPABILITY_POLICY_BYTES} bytes"),
            "policyPath",
        ));
    }

    let file = File::open(path)
        .map_err(|error| policy_io_error("policyPath", "cannot open policy file", error))?;
    let opened = file
        .metadata()
        .map_err(|error| policy_io_error("policyPath", "cannot inspect opened policy", error))?;
    if !same_file(&before, &opened) {
        return Err(policy_error(
            "policy file identity changed while it was opened",
            "policyPath",
        ));
    }

    let mut bytes = Vec::with_capacity(before.len() as usize);
    file.take(MAX_CAPABILITY_POLICY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| policy_io_error("policyPath", "cannot read policy file", error))?;
    load_capability_policy_bytes(&bytes)
}

pub fn load_capability_policy_bytes(
    bytes: &[u8],
) -> Result<ResolvedCapabilityPolicy, JobContractError> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_CAPABILITY_POLICY_BYTES {
        return Err(policy_error(
            format!("policy payload size must be in 1..={MAX_CAPABILITY_POLICY_BYTES} bytes"),
            "policy",
        ));
    }
    reject_duplicate_json_keys(bytes)?;
    let policy: CapabilityPolicy = serde_json::from_slice(bytes).map_err(|error| {
        policy_error(format!("invalid capability policy JSON: {error}"), "policy")
    })?;
    resolve_policy(policy)
}

pub fn evaluate_job_start(
    policy: &ResolvedCapabilityPolicy,
    request: &JobStartRequest,
    context: &CapabilityEvaluationContext,
) -> Result<ExecutionPlan, JobContractError> {
    request.validate_shape()?;
    validate_identifier(&context.principal, "principal")?;
    validate_identifier(&context.authority_ref, "authorityRef")?;

    let profile = policy.profiles.get(&request.profile_id).ok_or_else(|| {
        JobContractError::new(
            JobContractErrorCode::ProfileNotFound,
            format!("unknown execution profile {}", request.profile_id),
            Some("profileId"),
            false,
        )
    })?;
    if !profile.source.enabled {
        return Err(JobContractError::new(
            JobContractErrorCode::ProfileDisabled,
            format!("execution profile {} is disabled", request.profile_id),
            Some("profileId"),
            false,
        ));
    }

    if context.concurrency.global_running >= policy.policy.global_max_concurrency {
        return Err(JobContractError::new(
            JobContractErrorCode::ConcurrencyLimit,
            "global execution concurrency limit reached",
            Some("concurrency"),
            true,
        ));
    }
    if context.concurrency.profile_running >= profile.source.max_concurrency {
        return Err(JobContractError::new(
            JobContractErrorCode::ConcurrencyLimit,
            format!("profile {} concurrency limit reached", request.profile_id),
            Some("concurrency"),
            true,
        ));
    }

    if !profile
        .source
        .allowed_argument_vectors
        .iter()
        .any(|allowed| allowed == &request.args)
    {
        return Err(JobContractError::new(
            JobContractErrorCode::ArgumentPolicyDenied,
            "argument vector is not exactly allowlisted",
            Some("args"),
            false,
        ));
    }
    if request.timeout_ms > profile.source.max_runtime_ms {
        return Err(JobContractError::new(
            JobContractErrorCode::TimeoutExceedsPolicy,
            "requested timeout exceeds profile maximum",
            Some("timeoutMs"),
            false,
        ));
    }
    if request.stdout_retention_bytes > profile.source.max_stdout_bytes
        || request.stderr_retention_bytes > profile.source.max_stderr_bytes
    {
        return Err(JobContractError::new(
            JobContractErrorCode::OutputLimitExceedsPolicy,
            "requested output retention exceeds profile maximum",
            Some("stdoutRetentionBytes"),
            false,
        ));
    }

    let cwd = canonical_directory(&request.cwd, "cwd", JobContractErrorCode::InvalidCwd)?;
    if !profile
        .canonical_cwd_roots
        .iter()
        .any(|root| cwd.starts_with(root))
    {
        return Err(JobContractError::new(
            JobContractErrorCode::PathScopeDenied,
            "canonical cwd is outside the profile capability scope",
            Some("cwd"),
            false,
        ));
    }

    let current_executable = inspect_executable(&profile.canonical_executable)?;
    if current_executable != profile.executable_identity {
        return Err(policy_error(
            "executable identity changed after policy resolution; reload policy",
            "executable",
        ));
    }

    let mut environment = profile.source.base_environment.clone();
    for (name, value) in &request.env_overrides {
        if is_forbidden_client_override(name) {
            return Err(JobContractError::new(
                JobContractErrorCode::EnvironmentDenied,
                format!("environment override {name} is reserved for the profile"),
                Some("envOverrides"),
                false,
            ));
        }
        let rule = profile
            .source
            .environment_rules
            .iter()
            .find(|rule| rule.name == *name)
            .ok_or_else(|| {
                JobContractError::new(
                    JobContractErrorCode::EnvironmentDenied,
                    format!("environment override {name} is not allowlisted"),
                    Some("envOverrides"),
                    false,
                )
            })?;
        if !rule.allowed_values.iter().any(|allowed| allowed == value) {
            return Err(JobContractError::new(
                JobContractErrorCode::EnvironmentDenied,
                format!("environment override {name} has a denied value"),
                Some("envOverrides"),
                false,
            ));
        }
        environment.insert(name.clone(), value.clone());
    }

    let mut argv = profile.source.fixed_args.clone();
    argv.extend(request.args.iter().cloned());
    let executable = path_to_string(&profile.canonical_executable, "executable")?;
    let cwd = path_to_string(&cwd, "cwd")?;
    let plan = ExecutionPlan {
        schema_version: JOB_CONTRACT_SCHEMA_VERSION,
        policy_id: policy.policy.policy_id.clone(),
        policy_version: policy.policy.policy_version.clone(),
        policy_digest: policy.policy_digest.clone(),
        profile_id: profile.source.profile_id.clone(),
        executable,
        executable_digest: profile.executable_identity.digest.clone(),
        argv,
        cwd,
        env: environment,
        timeout_ms: request.timeout_ms,
        stdout_retention_bytes: request.stdout_retention_bytes,
        stderr_retention_bytes: request.stderr_retention_bytes,
        terminate_on_output_limit: profile.source.terminate_on_output_limit,
        client_request_id: request.client_request_id.clone(),
        request_digest: canonical_digest(request)?,
        principal: context.principal.clone(),
        authority_ref: context.authority_ref.clone(),
    };
    plan.validate_shape()?;
    Ok(plan)
}

fn resolve_policy(policy: CapabilityPolicy) -> Result<ResolvedCapabilityPolicy, JobContractError> {
    policy.validate_shape()?;
    if policy.allowed_roots.len() > MAX_CAPABILITY_ROOTS {
        return Err(policy_error(
            format!("policy supports at most {MAX_CAPABILITY_ROOTS} allowed roots"),
            "allowedRoots",
        ));
    }
    if policy.profiles.len() > MAX_CAPABILITY_PROFILES {
        return Err(policy_error(
            format!("policy supports at most {MAX_CAPABILITY_PROFILES} profiles"),
            "profiles",
        ));
    }
    if policy.global_max_concurrency > MAX_EXECUTION_CONCURRENCY {
        return Err(policy_error(
            format!("global concurrency exceeds ceiling {MAX_EXECUTION_CONCURRENCY}"),
            "globalMaxConcurrency",
        ));
    }

    let canonical_allowed_roots = canonical_roots(&policy.allowed_roots, "allowedRoots")?;
    let mut profiles = BTreeMap::new();
    for source in &policy.profiles {
        if source.allowed_argument_vectors.len() > MAX_ALLOWED_ARGUMENT_VECTORS {
            return Err(policy_error(
                format!("profile supports at most {MAX_ALLOWED_ARGUMENT_VECTORS} argument vectors"),
                "allowedArgumentVectors",
            ));
        }
        if source.max_runtime_ms > MAX_EXECUTION_RUNTIME_MS {
            return Err(policy_error(
                format!("runtime exceeds ceiling {MAX_EXECUTION_RUNTIME_MS} ms"),
                "maxRuntimeMs",
            ));
        }
        if source.max_concurrency > MAX_EXECUTION_CONCURRENCY {
            return Err(policy_error(
                format!("profile concurrency exceeds ceiling {MAX_EXECUTION_CONCURRENCY}"),
                "maxConcurrency",
            ));
        }
        if source
            .allowed_argument_vectors
            .iter()
            .any(|args| source.fixed_args.len() + args.len() > MAX_JOB_ARGS)
        {
            return Err(policy_error(
                "fixed arguments plus an allowed vector exceed argv ceiling",
                "allowedArgumentVectors",
            ));
        }
        if source.base_environment.len() + source.environment_rules.len() > MAX_JOB_ENV_VARS {
            return Err(policy_error(
                "base environment plus client rules exceed environment ceiling",
                "environmentRules",
            ));
        }
        for name in source.base_environment.keys() {
            if is_forbidden_base_environment(name) {
                return Err(policy_error(
                    format!("base environment variable {name} is prohibited"),
                    "baseEnvironment",
                ));
            }
        }
        for rule in &source.environment_rules {
            if is_forbidden_client_override(&rule.name) {
                return Err(policy_error(
                    format!(
                        "environment variable {} cannot be client-controlled",
                        rule.name
                    ),
                    "environmentRules",
                ));
            }
        }

        let canonical_cwd_roots = canonical_roots(&source.allowed_cwd_roots, "allowedCwdRoots")?;
        if canonical_cwd_roots.iter().any(|root| {
            !canonical_allowed_roots
                .iter()
                .any(|global_root| root.starts_with(global_root))
        }) {
            return Err(policy_error(
                "profile cwd root is outside policy allowed roots",
                "allowedCwdRoots",
            ));
        }

        let declared = Path::new(&source.executable);
        let metadata = fs::symlink_metadata(declared).map_err(|error| {
            policy_io_error("executable", "cannot inspect profile executable", error)
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(policy_error(
                "profile executable must be a non-symlink regular file",
                "executable",
            ));
        }
        let canonical_executable = fs::canonicalize(declared).map_err(|error| {
            policy_io_error(
                "executable",
                "cannot canonicalize profile executable",
                error,
            )
        })?;
        let executable_identity = inspect_executable(&canonical_executable)?;
        if executable_identity.digest != source.executable_digest {
            return Err(policy_error(
                "profile executable digest does not match the file on disk",
                "executableDigest",
            ));
        }

        profiles.insert(
            source.profile_id.clone(),
            ResolvedExecutionProfile {
                source: source.clone(),
                canonical_executable,
                executable_identity,
                canonical_cwd_roots,
            },
        );
    }

    Ok(ResolvedCapabilityPolicy {
        policy_digest: canonical_digest(&policy)?,
        policy,
        canonical_allowed_roots,
        profiles,
    })
}

pub(super) fn policy_error(message: impl Into<String>, field: &str) -> JobContractError {
    JobContractError::new(
        JobContractErrorCode::PolicyInvalid,
        message,
        Some(field),
        false,
    )
}

pub(super) fn policy_io_error(
    field: &str,
    message: &str,
    error: impl std::fmt::Display,
) -> JobContractError {
    policy_error(format!("{message}: {error}"), field)
}

#[cfg(test)]
mod tests;
