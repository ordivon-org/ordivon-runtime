use std::fs;

use super::{
    load_workspace_record, preflight_workspace_write_path, read_workspace_text,
    remove_workspace_file, write_workspace_text, UniversalExecError, UniversalExecErrorCode,
    UniversalExecutorConfig, WorkspaceMutateRequest, WorkspaceMutateResult, WorkspaceMutationMode,
    WorkspaceMutationResult, WorkspaceReadRequest, WorkspaceWriteRequest, WorkspaceWriteResult,
    MAX_WORKSPACE_IO_BYTES,
};

struct PreparedMutation {
    relative_path: String,
    before_content: Option<String>,
    before_digest: Option<String>,
    after_content: String,
}

pub fn mutate_workspace(
    config: &UniversalExecutorConfig,
    request: &WorkspaceMutateRequest,
) -> Result<WorkspaceMutateResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let mut prepared = Vec::with_capacity(request.mutations.len());
    for (mutation_index, mutation) in request.mutations.iter().enumerate() {
        let path = preflight_workspace_write_path(&record, &mutation.relative_path)?;
        let existing = if path.exists() {
            let read = read_workspace_text(
                config,
                &WorkspaceReadRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id.clone(),
                    relative_path: mutation.relative_path.clone(),
                    max_bytes: MAX_WORKSPACE_IO_BYTES,
                },
            )?;
            Some((read.content, read.digest))
        } else {
            None
        };
        let before_digest = existing.as_ref().map(|(_, digest)| digest.clone());
        if mutation.expected_digest.is_none() && before_digest.is_some() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                format!(
                    "workspace file {} already exists; expectedDigest is required",
                    mutation.relative_path
                ),
                Some(&format!("mutations[{mutation_index}].expectedDigest")),
                false,
            ));
        }
        if mutation.expected_digest != before_digest
            && (mutation.expected_digest.is_some() || before_digest.is_some())
        {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                format!(
                    "workspace file {} does not match expectedDigest",
                    mutation.relative_path
                ),
                Some(&format!("mutations[{mutation_index}].expectedDigest")),
                false,
            ));
        }
        let before_content = existing.map(|(content, _)| content);
        let after_content = match mutation.mode {
            WorkspaceMutationMode::Write => mutation.content.clone(),
            WorkspaceMutationMode::Append => {
                let mut content = before_content.clone().unwrap_or_default();
                content.push_str(&mutation.content);
                content
            }
            WorkspaceMutationMode::ReplaceExact => {
                let content = before_content.as_ref().ok_or_else(|| {
                    UniversalExecError::new(
                        UniversalExecErrorCode::WorkspacePathNotFound,
                        format!(
                            "REPLACE_EXACT target does not exist: {}",
                            mutation.relative_path
                        ),
                        Some(&format!("mutations[{mutation_index}].relativePath")),
                        false,
                    )
                })?;
                let expected = mutation.expected_text.as_ref().expect("validated");
                let occurrences = content.matches(expected).count();
                if occurrences != 1 {
                    return Err(UniversalExecError::new(
                        UniversalExecErrorCode::RevisionMismatch,
                        format!(
                            "REPLACE_EXACT expected one match in {}, found {occurrences}",
                            mutation.relative_path
                        ),
                        Some(&format!("mutations[{mutation_index}].expectedText")),
                        false,
                    ));
                }
                content.replacen(expected, &mutation.content, 1)
            }
        };
        if after_content.len() as u64 > MAX_WORKSPACE_IO_BYTES {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::OutputLimitExceeded,
                "mutated file exceeds the workspace limit",
                Some(&format!("mutations[{mutation_index}].content")),
                false,
            ));
        }
        prepared.push(PreparedMutation {
            relative_path: mutation.relative_path.clone(),
            before_content,
            before_digest,
            after_content,
        });
    }
    let mut results = Vec::with_capacity(prepared.len());
    for (index, mutation) in prepared.iter().enumerate() {
        let outcome = write_workspace_text(
            config,
            &WorkspaceWriteRequest {
                schema_version: request.schema_version,
                workspace_id: request.workspace_id.clone(),
                relative_path: mutation.relative_path.clone(),
                content: mutation.after_content.clone(),
                expected_digest: mutation.before_digest.clone(),
            },
        );
        match outcome {
            Ok(result) => results.push(result),
            Err(error) => {
                rollback(config, request, &record, &prepared[..index], &results)?;
                return Err(error);
            }
        }
    }
    Ok(WorkspaceMutateResult {
        mutations: results
            .into_iter()
            .map(|result| WorkspaceMutationResult {
                relative_path: result.relative_path,
                after_digest: result.after_digest,
                byte_length: result.byte_length,
            })
            .collect(),
    })
}
fn rollback(
    config: &UniversalExecutorConfig,
    request: &WorkspaceMutateRequest,
    record: &super::WorkspaceRecord,
    applied: &[PreparedMutation],
    results: &[WorkspaceWriteResult],
) -> Result<(), UniversalExecError> {
    for (mutation, result) in applied.iter().zip(results).rev() {
        let restored = if let Some(content) = &mutation.before_content {
            write_workspace_text(
                config,
                &WorkspaceWriteRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id.clone(),
                    relative_path: mutation.relative_path.clone(),
                    content: content.clone(),
                    expected_digest: Some(result.after_digest.clone()),
                },
            )
            .map(|_| ())
        } else {
            remove_workspace_file(record, &mutation.relative_path)
        };
        if let Err(error) = restored {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::WorkspaceMutationIncomplete,
                format!(
                    "batch mutation failed and rollback of {} also failed: {error}",
                    mutation.relative_path
                ),
                Some("mutations"),
                false,
            ));
        }
    }
    Ok(())
}

pub fn read_workspace_slice(
    config: &UniversalExecutorConfig,
    request: &super::WorkspaceReadSliceRequest,
) -> Result<super::WorkspaceReadSliceResult, UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let path = super::resolve_existing_workspace_path(&record, &request.relative_path, false)?;
    let bytes = fs::read(&path).map_err(|error| super::io_error(&path, "read", error))?;
    if request.offset > bytes.len() as u64 {
        return Err(super::invalid("offset exceeds file length", "offset"));
    }
    let content = std::str::from_utf8(&bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("workspace file is not UTF-8: {error}"),
            Some("relativePath"),
            false,
        )
    })?;
    let start = request.offset as usize;
    let end = (start + request.max_bytes as usize).min(bytes.len());
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return Err(super::invalid(
            "offset and maxBytes must end on UTF-8 boundaries",
            "offset",
        ));
    }
    Ok(super::WorkspaceReadSliceResult {
        workspace_id: request.workspace_id.clone(),
        relative_path: request.relative_path.clone(),
        content: content[start..end].to_string(),
        offset: request.offset,
        next_offset: end as u64,
        eof: end == bytes.len(),
        file_digest: super::sha256_bytes(&bytes),
        file_byte_length: bytes.len() as u64,
    })
}
