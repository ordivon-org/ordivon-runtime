use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::validation::{
    ensure_unique, invalid, is_valid_environment_name, policy_error, policy_invalid,
    validate_absolute_path, validate_argument_vector, validate_environment, validate_identifier,
    validate_retention, validate_sha256_digest,
};
use super::{
    JobContractError, JobContractErrorCode, JOB_CONTRACT_SCHEMA_VERSION, MAX_JOB_ENV_VALUE_BYTES,
    MAX_JOB_OUTPUT_RETENTION_BYTES,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobStartRequest {
    pub profile_id: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    pub timeout_ms: u64,
    #[serde(default)]
    pub env_overrides: BTreeMap<String, String>,
    pub stdout_retention_bytes: u64,
    pub stderr_retention_bytes: u64,
    pub client_request_id: String,
}

impl JobStartRequest {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        validate_identifier(&self.profile_id, "profileId")?;
        validate_identifier(&self.client_request_id, "clientRequestId")?;
        validate_absolute_path(&self.cwd, "cwd", JobContractErrorCode::InvalidCwd)?;
        validate_argument_vector(&self.args, "args")?;
        if self.timeout_ms == 0 {
            return Err(invalid("timeoutMs must be greater than zero", "timeoutMs"));
        }
        validate_retention(self.stdout_retention_bytes, "stdoutRetentionBytes")?;
        validate_retention(self.stderr_retention_bytes, "stderrRetentionBytes")?;
        validate_environment(&self.env_overrides)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentRule {
    pub name: String,
    pub allowed_values: Vec<String>,
}

impl EnvironmentRule {
    fn validate_shape(&self) -> Result<(), JobContractError> {
        if !is_valid_environment_name(&self.name) {
            return Err(JobContractError::new(
                JobContractErrorCode::PolicyInvalid,
                format!("invalid environment variable name {}", self.name),
                Some("environmentRules"),
                false,
            ));
        }
        if self.allowed_values.is_empty() {
            return Err(policy_invalid(
                "environment rule must allow at least one exact value",
                "environmentRules",
            ));
        }
        if self
            .allowed_values
            .iter()
            .any(|value| value.as_bytes().contains(&0) || value.len() > MAX_JOB_ENV_VALUE_BYTES)
        {
            return Err(policy_invalid(
                "environment rule contains an invalid value",
                "environmentRules",
            ));
        }
        let unique: BTreeSet<_> = self.allowed_values.iter().collect();
        if unique.len() != self.allowed_values.len() {
            return Err(policy_invalid(
                "environment allowed values must be unique",
                "environmentRules",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionProfile {
    pub profile_id: String,
    pub enabled: bool,
    pub executable: String,
    pub executable_digest: String,
    #[serde(default)]
    pub fixed_args: Vec<String>,
    pub allowed_argument_vectors: Vec<Vec<String>>,
    pub allowed_cwd_roots: Vec<String>,
    #[serde(default)]
    pub base_environment: BTreeMap<String, String>,
    #[serde(default)]
    pub environment_rules: Vec<EnvironmentRule>,
    pub max_runtime_ms: u64,
    pub max_stdout_bytes: u64,
    pub max_stderr_bytes: u64,
    pub max_concurrency: u32,
    pub terminate_on_output_limit: bool,
}

impl ExecutionProfile {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        validate_identifier(&self.profile_id, "profileId")
            .map_err(|error| policy_error(error, "profiles"))?;
        validate_absolute_path(
            &self.executable,
            "executable",
            JobContractErrorCode::PolicyInvalid,
        )?;
        validate_sha256_digest(&self.executable_digest, "executableDigest")
            .map_err(|error| policy_error(error, "executableDigest"))?;
        validate_argument_vector(&self.fixed_args, "fixedArgs")
            .map_err(|error| policy_error(error, "profiles"))?;
        if self.allowed_argument_vectors.is_empty() {
            return Err(policy_invalid(
                "profile must declare at least one exact allowed argument vector",
                "allowedArgumentVectors",
            ));
        }
        let mut argument_vectors = BTreeSet::new();
        for args in &self.allowed_argument_vectors {
            validate_argument_vector(args, "allowedArgumentVectors")
                .map_err(|error| policy_error(error, "profiles"))?;
            if !argument_vectors.insert(args) {
                return Err(policy_invalid(
                    "allowed argument vectors must be unique",
                    "allowedArgumentVectors",
                ));
            }
        }
        if self.allowed_cwd_roots.is_empty() {
            return Err(policy_invalid(
                "profile must declare at least one allowed cwd root",
                "allowedCwdRoots",
            ));
        }
        for root in &self.allowed_cwd_roots {
            validate_absolute_path(root, "allowedCwdRoots", JobContractErrorCode::PolicyInvalid)?;
        }
        ensure_unique(&self.allowed_cwd_roots, "allowedCwdRoots")?;
        validate_environment(&self.base_environment)
            .map_err(|error| policy_error(error, "baseEnvironment"))?;
        let mut environment_names = BTreeSet::new();
        for rule in &self.environment_rules {
            rule.validate_shape()?;
            if self.base_environment.contains_key(&rule.name) {
                return Err(policy_invalid(
                    "client environment rules cannot override base environment keys",
                    "environmentRules",
                ));
            }
            if !environment_names.insert(rule.name.as_str()) {
                return Err(policy_invalid(
                    "environment rule names must be unique",
                    "environmentRules",
                ));
            }
        }
        if self.max_runtime_ms == 0
            || self.max_stdout_bytes == 0
            || self.max_stderr_bytes == 0
            || self.max_concurrency == 0
        {
            return Err(policy_invalid(
                "runtime, output, and concurrency limits must be positive",
                "profiles",
            ));
        }
        if self.max_stdout_bytes > MAX_JOB_OUTPUT_RETENTION_BYTES
            || self.max_stderr_bytes > MAX_JOB_OUTPUT_RETENTION_BYTES
        {
            return Err(policy_invalid(
                "profile output limit exceeds the contract ceiling",
                "profiles",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub policy_version: String,
    pub allowed_roots: Vec<String>,
    pub global_max_concurrency: u32,
    pub profiles: Vec<ExecutionProfile>,
}

impl CapabilityPolicy {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        if self.schema_version != JOB_CONTRACT_SCHEMA_VERSION {
            return Err(policy_invalid(
                "unsupported capability policy schema version",
                "schemaVersion",
            ));
        }
        validate_identifier(&self.policy_id, "policyId")
            .map_err(|error| policy_error(error, "policyId"))?;
        validate_identifier(&self.policy_version, "policyVersion")
            .map_err(|error| policy_error(error, "policyVersion"))?;
        if self.allowed_roots.is_empty() {
            return Err(policy_invalid(
                "policy must declare at least one allowed root",
                "allowedRoots",
            ));
        }
        for root in &self.allowed_roots {
            validate_absolute_path(root, "allowedRoots", JobContractErrorCode::PolicyInvalid)?;
        }
        ensure_unique(&self.allowed_roots, "allowedRoots")?;
        if self.global_max_concurrency == 0 {
            return Err(policy_invalid(
                "globalMaxConcurrency must be positive",
                "globalMaxConcurrency",
            ));
        }
        if self.profiles.is_empty() {
            return Err(policy_invalid(
                "policy must contain at least one execution profile",
                "profiles",
            ));
        }
        let mut profile_ids = BTreeSet::new();
        for profile in &self.profiles {
            profile.validate_shape()?;
            if !profile_ids.insert(profile.profile_id.as_str()) {
                return Err(policy_invalid("profile IDs must be unique", "profiles"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionPlan {
    pub schema_version: u32,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_digest: String,
    pub profile_id: String,
    pub executable: String,
    pub executable_digest: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub stdout_retention_bytes: u64,
    pub stderr_retention_bytes: u64,
    pub terminate_on_output_limit: bool,
    pub client_request_id: String,
    pub request_digest: String,
    pub principal: String,
    pub authority_ref: String,
}

impl ExecutionPlan {
    pub fn validate_shape(&self) -> Result<(), JobContractError> {
        if self.schema_version != JOB_CONTRACT_SCHEMA_VERSION {
            return Err(invalid(
                "unsupported execution plan schema version",
                "schemaVersion",
            ));
        }
        validate_identifier(&self.policy_id, "policyId")?;
        validate_identifier(&self.policy_version, "policyVersion")?;
        validate_identifier(&self.profile_id, "profileId")?;
        validate_identifier(&self.client_request_id, "clientRequestId")?;
        validate_identifier(&self.principal, "principal")?;
        validate_identifier(&self.authority_ref, "authorityRef")?;
        validate_absolute_path(
            &self.executable,
            "executable",
            JobContractErrorCode::InvalidRequest,
        )?;
        validate_absolute_path(&self.cwd, "cwd", JobContractErrorCode::InvalidCwd)?;
        validate_sha256_digest(&self.policy_digest, "policyDigest")?;
        validate_sha256_digest(&self.executable_digest, "executableDigest")
            .map_err(|error| policy_error(error, "executableDigest"))?;
        validate_sha256_digest(&self.request_digest, "requestDigest")?;
        validate_argument_vector(&self.argv, "argv")?;
        validate_environment(&self.env)?;
        if self.timeout_ms == 0 {
            return Err(invalid("timeoutMs must be greater than zero", "timeoutMs"));
        }
        validate_retention(self.stdout_retention_bytes, "stdoutRetentionBytes")?;
        validate_retention(self.stderr_retention_bytes, "stderrRetentionBytes")?;
        Ok(())
    }
}
