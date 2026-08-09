use std::fs::File;
use std::io::Read;

use sha2::{Digest, Sha256};

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
    const CHUNK_BYTES: usize = 64 * 1024;

    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let path = super::resolve_existing_workspace_path(&record, &request.relative_path, false)?;
    let mut file = File::open(&path).map_err(|error| super::io_error(&path, "open", error))?;
    let before = file
        .metadata()
        .map_err(|error| super::io_error(&path, "inspect", error))?;
    let file_byte_length = before.len();
    if request.offset > file_byte_length {
        return Err(super::invalid("offset exceeds file length", "offset"));
    }
    let slice_end = request
        .offset
        .saturating_add(request.max_bytes)
        .min(file_byte_length);
    let mut digest = Sha256::new();
    let mut carry = Vec::with_capacity(4);
    let mut chunk = vec![0_u8; CHUNK_BYTES];
    let mut captured =
        Vec::with_capacity(usize::try_from(slice_end.saturating_sub(request.offset)).unwrap_or(0));
    let mut position = 0_u64;
    while position < file_byte_length {
        let remaining = file_byte_length - position;
        let wanted = usize::try_from(remaining.min(CHUNK_BYTES as u64)).unwrap_or(CHUNK_BYTES);
        let read = file
            .read(&mut chunk[..wanted])
            .map_err(|error| super::io_error(&path, "read", error))?;
        if read == 0 {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::WorkspaceMutationIncomplete,
                "workspace file changed while reading",
                Some("relativePath"),
                true,
            ));
        }
        let bytes = &chunk[..read];
        digest.update(bytes);

        let mut validation = Vec::with_capacity(carry.len() + read);
        validation.extend_from_slice(&carry);
        validation.extend_from_slice(bytes);
        match std::str::from_utf8(&validation) {
            Ok(_) => carry.clear(),
            Err(error) if error.error_len().is_none() => {
                let valid = error.valid_up_to();
                carry.clear();
                carry.extend_from_slice(&validation[valid..]);
                debug_assert!(carry.len() <= 3);
            }
            Err(error) => {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::ArtifactNotUtf8,
                    format!("workspace file is not UTF-8: {error}"),
                    Some("relativePath"),
                    false,
                ));
            }
        }

        let chunk_end = position.saturating_add(read as u64);
        let capture_start = request.offset.max(position);
        let capture_end = slice_end.min(chunk_end);
        if capture_start < capture_end {
            let local_start = usize::try_from(capture_start - position).unwrap_or(0);
            let local_end = usize::try_from(capture_end - position).unwrap_or(read);
            captured.extend_from_slice(&bytes[local_start..local_end]);
        }
        position = chunk_end;
    }
    if !carry.is_empty() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            "workspace file ends with an incomplete UTF-8 sequence",
            Some("relativePath"),
            false,
        ));
    }
    let after = file
        .metadata()
        .map_err(|error| super::io_error(&path, "inspect after read", error))?;
    if after.len() != before.len() || after.modified().ok() != before.modified().ok() {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceMutationIncomplete,
            "workspace file changed while reading",
            Some("relativePath"),
            true,
        ));
    }
    let content = String::from_utf8(captured).map_err(|_| {
        super::invalid("offset and maxBytes must end on UTF-8 boundaries", "offset")
    })?;
    Ok(super::WorkspaceReadSliceResult {
        workspace_id: request.workspace_id.clone(),
        relative_path: request.relative_path.clone(),
        content,
        offset: request.offset,
        next_offset: slice_end,
        eof: slice_end == file_byte_length,
        file_digest: format!("sha256:{}", hex::encode(digest.finalize())),
        file_byte_length,
    })
}
