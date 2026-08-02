use rusqlite::{params, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::universal::{
    sha256_bytes, WorkspacePatchPlan, WorkspacePatchRequest, WorkspacePatchResult,
};

use super::{Registry, RuntimeError, RuntimeErrorCode, RuntimeResult, RUNTIME_SCHEMA_VERSION};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurableWorkspacePatchRequest {
    pub schema_version: u32,
    pub principal: String,
    pub client_request_id: String,
    pub patch: WorkspacePatchRequest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePatchOperationState {
    Prepared,
    Committed,
    Unknown,
}

impl WorkspacePatchOperationState {
    fn parse(value: &str) -> RuntimeResult<Self> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "committed" => Ok(Self::Committed),
            "unknown" => Ok(Self::Unknown),
            _ => Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("stored Workspace Patch state is invalid: {value}"),
                Some("state"),
                false,
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurableWorkspacePatchResult {
    pub operation_id: String,
    pub client_request_id: String,
    pub request_digest: String,
    pub replayed: bool,
    pub patch: WorkspacePatchResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePatchStatusRequest {
    pub schema_version: u32,
    pub principal: String,
    pub client_request_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePatchOperationStatus {
    pub operation_id: String,
    pub client_request_id: String,
    pub request_digest: String,
    pub workspace_id: String,
    pub state: WorkspacePatchOperationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<WorkspacePatchResult>,
}

#[derive(Clone, Debug)]
pub(crate) struct StoredWorkspacePatchOperation {
    pub operation_id: String,
    pub client_request_id: String,
    pub request_digest: String,
    pub workspace_id: String,
    pub plan: WorkspacePatchPlan,
    pub max_diff_bytes: u64,
    pub state: WorkspacePatchOperationState,
    pub result: Option<WorkspacePatchResult>,
}

pub(crate) fn validate_durable_patch_request(
    request: &DurableWorkspacePatchRequest,
) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION
        || request.patch.schema_version != crate::UNIVERSAL_EXEC_SCHEMA_VERSION
    {
        return Err(RuntimeError::invalid(
            "unsupported Workspace Patch schema version",
            "schemaVersion",
        ));
    }
    validate_identity(&request.principal, "principal")?;
    validate_identity(&request.client_request_id, "clientRequestId")?;
    request.patch.validate_shape().map_err(map_patch_error)
}

pub(crate) fn validate_patch_status_request(
    request: &WorkspacePatchStatusRequest,
) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION {
        return Err(RuntimeError::invalid(
            "unsupported Workspace Patch status schema version",
            "schemaVersion",
        ));
    }
    validate_identity(&request.principal, "principal")?;
    validate_identity(&request.client_request_id, "clientRequestId")
}

pub(crate) fn durable_patch_request_digest(
    request: &DurableWorkspacePatchRequest,
) -> RuntimeResult<String> {
    let bytes = serde_json::to_vec(request).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::InvalidRequest,
            format!("cannot serialize Workspace Patch request: {error}"),
            None,
            false,
        )
    })?;
    Ok(sha256_bytes(&bytes))
}

impl Registry {
    pub(crate) fn find_workspace_patch_operation(
        &self,
        principal: &str,
        client_request_id: &str,
    ) -> RuntimeResult<Option<StoredWorkspacePatchOperation>> {
        let connection = self.open_connection()?;
        let row = connection
            .query_row(
                "SELECT operation_id,principal,client_request_id,request_digest,workspace_id,plan_json,max_diff_bytes,state,result_json FROM workspace_patch_operations WHERE principal=?1 AND client_request_id=?2",
                params![principal, client_request_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot read Workspace Patch operation"))?;
        row.map(decode_operation).transpose()
    }

    pub(crate) fn prepare_workspace_patch_operation(
        &self,
        request: &DurableWorkspacePatchRequest,
        request_digest: &str,
        plan: &WorkspacePatchPlan,
    ) -> RuntimeResult<(StoredWorkspacePatchOperation, bool)> {
        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| {
                RuntimeError::from_sql(error, "cannot begin Workspace Patch admission")
            })?;
        let existing = transaction
            .query_row(
                "SELECT operation_id,principal,client_request_id,request_digest,workspace_id,plan_json,max_diff_bytes,state,result_json FROM workspace_patch_operations WHERE principal=?1 AND client_request_id=?2",
                params![request.principal, request.client_request_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot inspect Workspace Patch admission"))?;
        if let Some(row) = existing {
            let operation = decode_operation(row)?;
            if operation.request_digest != request_digest {
                return Err(idempotency_conflict());
            }
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot close Workspace Patch replay admission")
            })?;
            return Ok((operation, true));
        }

        let operation_id = format!("patch-{}", Uuid::now_v7());
        let plan_json = serde_json::to_string(plan).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::InvalidRequest,
                format!("cannot serialize Workspace Patch plan: {error}"),
                Some("patch"),
                false,
            )
        })?;
        let created_at_ms = now_ms()?;
        transaction
            .execute(
                "INSERT INTO workspace_patch_operations(operation_id,principal,client_request_id,request_digest,workspace_id,plan_json,max_diff_bytes,state,result_json,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,'prepared',NULL,?8,?8)",
                params![
                    operation_id,
                    request.principal,
                    request.client_request_id,
                    request_digest,
                    request.patch.workspace_id,
                    plan_json,
                    i64::try_from(request.patch.max_diff_bytes).map_err(|_| RuntimeError::invalid("maxDiffBytes exceeds SQLite range", "patch.maxDiffBytes"))?,
                    i64::try_from(created_at_ms).map_err(|_| RuntimeError::new(RuntimeErrorCode::IoError, "clock exceeds SQLite range", None, false))?,
                ],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot record Workspace Patch intent"))?;
        transaction.commit().map_err(|error| {
            RuntimeError::from_sql(error, "cannot commit Workspace Patch intent")
        })?;
        Ok((
            StoredWorkspacePatchOperation {
                operation_id,
                client_request_id: request.client_request_id.clone(),
                request_digest: request_digest.to_string(),
                workspace_id: request.patch.workspace_id.clone(),
                plan: plan.clone(),
                max_diff_bytes: request.patch.max_diff_bytes,
                state: WorkspacePatchOperationState::Prepared,
                result: None,
            },
            false,
        ))
    }

    pub(crate) fn commit_workspace_patch_operation(
        &self,
        operation_id: &str,
        result: &WorkspacePatchResult,
    ) -> RuntimeResult<()> {
        let result_json = serde_json::to_string(result).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("cannot serialize Workspace Patch result: {error}"),
                Some("result"),
                false,
            )
        })?;
        let connection = self.open_connection()?;
        let changed = connection
            .execute(
                "UPDATE workspace_patch_operations SET state='committed',result_json=?2,updated_at_ms=?3 WHERE operation_id=?1 AND state IN ('prepared','committed')",
                params![
                    operation_id,
                    result_json,
                    i64::try_from(now_ms()?).map_err(|_| RuntimeError::new(RuntimeErrorCode::IoError, "clock exceeds SQLite range", None, false))?,
                ],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit Workspace Patch receipt"))?;
        if changed != 1 {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReconciliationRequired,
                "Workspace Patch operation is not committable",
                Some("operationId"),
                false,
            ));
        }
        Ok(())
    }

    pub(crate) fn mark_workspace_patch_unknown(&self, operation_id: &str) -> RuntimeResult<()> {
        let connection = self.open_connection()?;
        connection
            .execute(
                "UPDATE workspace_patch_operations SET state='unknown',updated_at_ms=?2 WHERE operation_id=?1 AND state='prepared'",
                params![
                    operation_id,
                    i64::try_from(now_ms()?).map_err(|_| RuntimeError::new(RuntimeErrorCode::IoError, "clock exceeds SQLite range", None, false))?,
                ],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot mark Workspace Patch outcome unknown"))?;
        Ok(())
    }
}

fn decode_operation(
    row: (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
    ),
) -> RuntimeResult<StoredWorkspacePatchOperation> {
    let (
        operation_id,
        client_request_id,
        request_digest,
        workspace_id,
        plan_json,
        max_diff_bytes,
        state,
        result_json,
    ) = row;
    let plan = serde_json::from_str(&plan_json).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryCorrupt,
            format!("stored Workspace Patch plan is invalid: {error}"),
            Some("plan"),
            false,
        )
    })?;
    let result = result_json
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    format!("stored Workspace Patch result is invalid: {error}"),
                    Some("result"),
                    false,
                )
            })
        })
        .transpose()?;
    Ok(StoredWorkspacePatchOperation {
        operation_id,
        client_request_id,
        request_digest,
        workspace_id,
        plan,
        max_diff_bytes: u64::try_from(max_diff_bytes).map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "stored Workspace Patch maxDiffBytes is invalid",
                Some("maxDiffBytes"),
                false,
            )
        })?,
        state: WorkspacePatchOperationState::parse(&state)?,
        result,
    })
}

fn validate_identity(value: &str, field: &str) -> RuntimeResult<()> {
    if value.is_empty() || value != value.trim() || value.len() > 1024 {
        return Err(RuntimeError::invalid(
            format!("{field} must be non-empty, trimmed, and at most 1024 bytes"),
            field,
        ));
    }
    Ok(())
}

fn idempotency_conflict() -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IdempotencyConflict,
        "clientRequestId is already bound to a different Workspace Patch request",
        Some("clientRequestId"),
        false,
    )
}

fn map_patch_error(error: crate::UniversalExecError) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::InvalidRequest,
        error.message,
        error.field.as_deref(),
        error.retryable,
    )
}

fn now_ms() -> RuntimeResult<u64> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::IoError,
                format!("system clock precedes Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis();
    u64::try_from(value).map_err(|_| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            "system time exceeds Runtime range",
            None,
            false,
        )
    })
}
