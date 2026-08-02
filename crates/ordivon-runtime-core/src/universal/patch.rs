use serde::{Deserialize, Serialize};

use super::{
    load_workspace_record, preflight_workspace_write_path, read_workspace_text,
    remove_workspace_file, sha256_bytes, workspace_diff_paths, write_workspace_text,
    UniversalExecError, UniversalExecErrorCode, UniversalExecutorConfig, WorkspacePatchRequest,
    WorkspacePatchResult, WorkspacePatchedFile, WorkspaceReadRequest, WorkspaceTextPosition,
    WorkspaceWriteRequest, WorkspaceWriteResult, MAX_WORKSPACE_IO_BYTES,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePatchPlanFile {
    pub relative_path: String,
    pub before_digest: Option<String>,
    pub after_digest: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePatchPlan {
    pub workspace_id: String,
    pub files: Vec<WorkspacePatchPlanFile>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspacePatchPlanState {
    Before,
    After,
    Mixed,
}

struct PreparedPatch {
    relative_path: String,
    before_content: Option<String>,
    before_digest: Option<String>,
    after_content: String,
}

struct ResolvedEdit {
    start: usize,
    end: usize,
    expected_text: String,
    replacement: String,
    source_index: usize,
}

pub fn plan_workspace_patch(
    config: &UniversalExecutorConfig,
    request: &WorkspacePatchRequest,
) -> Result<WorkspacePatchPlan, UniversalExecError> {
    let (_, prepared) = prepare_workspace_patch(config, request)?;
    Ok(plan_from_prepared(request, &prepared))
}

pub fn inspect_workspace_patch_plan(
    config: &UniversalExecutorConfig,
    plan: &WorkspacePatchPlan,
) -> Result<WorkspacePatchPlanState, UniversalExecError> {
    let record = load_workspace_record(config, &plan.workspace_id)?;
    let mut before = 0usize;
    let mut after = 0usize;
    for file in &plan.files {
        let path = preflight_workspace_write_path(&record, &file.relative_path)?;
        let current = if path.exists() {
            Some(
                read_workspace_text(
                    config,
                    &WorkspaceReadRequest {
                        schema_version: super::UNIVERSAL_EXEC_SCHEMA_VERSION,
                        workspace_id: plan.workspace_id.clone(),
                        relative_path: file.relative_path.clone(),
                        max_bytes: MAX_WORKSPACE_IO_BYTES,
                    },
                )?
                .digest,
            )
        } else {
            None
        };
        if current == file.before_digest {
            before += 1;
        } else if current.as_deref() == Some(file.after_digest.as_str()) {
            after += 1;
        } else {
            return Ok(WorkspacePatchPlanState::Mixed);
        }
    }
    if before == plan.files.len() {
        Ok(WorkspacePatchPlanState::Before)
    } else if after == plan.files.len() {
        Ok(WorkspacePatchPlanState::After)
    } else {
        Ok(WorkspacePatchPlanState::Mixed)
    }
}

pub fn result_from_workspace_patch_plan(
    config: &UniversalExecutorConfig,
    plan: &WorkspacePatchPlan,
    max_diff_bytes: u64,
) -> Result<WorkspacePatchResult, UniversalExecError> {
    if inspect_workspace_patch_plan(config, plan)? != WorkspacePatchPlanState::After {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::WorkspaceMutationIncomplete,
            "workspace files do not all match the committed patch result",
            Some("files"),
            false,
        ));
    }
    let paths = plan
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<Vec<_>>();
    let (diff, diff_truncated) =
        workspace_diff_paths(config, &plan.workspace_id, &paths, max_diff_bytes)?;
    Ok(WorkspacePatchResult {
        files: plan
            .files
            .iter()
            .map(|file| WorkspacePatchedFile {
                relative_path: file.relative_path.clone(),
                before_digest: file.before_digest.clone(),
                after_digest: file.after_digest.clone(),
                byte_length: file.byte_length,
            })
            .collect(),
        diff,
        diff_truncated,
    })
}

pub fn patch_workspace(
    config: &UniversalExecutorConfig,
    request: &WorkspacePatchRequest,
) -> Result<WorkspacePatchResult, UniversalExecError> {
    let (record, prepared) = prepare_workspace_patch(config, request)?;
    let plan = plan_from_prepared(request, &prepared);
    let mut results = Vec::with_capacity(prepared.len());
    for (index, patch) in prepared.iter().enumerate() {
        let outcome = write_workspace_text(
            config,
            &WorkspaceWriteRequest {
                schema_version: request.schema_version,
                workspace_id: request.workspace_id.clone(),
                relative_path: patch.relative_path.clone(),
                content: patch.after_content.clone(),
                expected_digest: patch.before_digest.clone(),
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
    result_from_workspace_patch_plan(config, &plan, request.max_diff_bytes)
}

fn prepare_workspace_patch(
    config: &UniversalExecutorConfig,
    request: &WorkspacePatchRequest,
) -> Result<(super::WorkspaceRecord, Vec<PreparedPatch>), UniversalExecError> {
    request.validate_shape()?;
    let record = load_workspace_record(config, &request.workspace_id)?;
    let mut prepared = Vec::with_capacity(request.files.len());

    for (file_index, file) in request.files.iter().enumerate() {
        let path = preflight_workspace_write_path(&record, &file.relative_path)?;
        let existing = if path.exists() {
            let read = read_workspace_text(
                config,
                &WorkspaceReadRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id.clone(),
                    relative_path: file.relative_path.clone(),
                    max_bytes: MAX_WORKSPACE_IO_BYTES,
                },
            )?;
            Some((read.content, read.digest))
        } else {
            None
        };
        let before_digest = existing.as_ref().map(|(_, digest)| digest.clone());
        if before_digest.is_some() && file.expected_digest.is_none() {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                format!(
                    "workspace file {} already exists; expectedDigest is required",
                    file.relative_path
                ),
                Some(&format!("files[{file_index}].expectedDigest")),
                false,
            ));
        }
        if file.expected_digest != before_digest
            && (file.expected_digest.is_some() || before_digest.is_some())
        {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::RevisionMismatch,
                format!(
                    "workspace file {} does not match expectedDigest",
                    file.relative_path
                ),
                Some(&format!("files[{file_index}].expectedDigest")),
                false,
            ));
        }

        let before_content = existing.map(|(content, _)| content);
        let original = before_content.clone().unwrap_or_default();
        let mut edits = Vec::with_capacity(file.edits.len());
        for (edit_index, edit) in file.edits.iter().enumerate() {
            let start = position_offset(
                &original,
                &edit.range.start,
                &format!("files[{file_index}].edits[{edit_index}].range.start"),
            )?;
            let end = position_offset(
                &original,
                &edit.range.end,
                &format!("files[{file_index}].edits[{edit_index}].range.end"),
            )?;
            if end < start {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::InvalidRequest,
                    "patch range end precedes start",
                    Some(&format!("files[{file_index}].edits[{edit_index}].range")),
                    false,
                ));
            }
            let actual = &original[start..end];
            if actual != edit.expected_text {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::RevisionMismatch,
                    format!(
                        "patch expectedText mismatch in {} at edit {edit_index}",
                        file.relative_path
                    ),
                    Some(&format!(
                        "files[{file_index}].edits[{edit_index}].expectedText"
                    )),
                    false,
                ));
            }
            edits.push(ResolvedEdit {
                start,
                end,
                expected_text: edit.expected_text.clone(),
                replacement: edit.replacement.clone(),
                source_index: edit_index,
            });
        }
        edits.sort_by_key(|edit| (edit.start, edit.end, edit.source_index));
        for pair in edits.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            if right.start < left.end || (left.start == left.end && right.start == left.start) {
                return Err(UniversalExecError::new(
                    UniversalExecErrorCode::InvalidRequest,
                    format!(
                        "patch edits overlap or share an insertion point in {}",
                        file.relative_path
                    ),
                    Some(&format!(
                        "files[{file_index}].edits[{}].range",
                        right.source_index
                    )),
                    false,
                ));
            }
        }
        let mut after_content = original;
        for edit in edits.iter().rev() {
            debug_assert_eq!(&after_content[edit.start..edit.end], edit.expected_text);
            after_content.replace_range(edit.start..edit.end, &edit.replacement);
        }
        if after_content.len() as u64 > MAX_WORKSPACE_IO_BYTES {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::OutputLimitExceeded,
                "patched file exceeds the workspace limit",
                Some(&format!("files[{file_index}].edits")),
                false,
            ));
        }
        prepared.push(PreparedPatch {
            relative_path: file.relative_path.clone(),
            before_content,
            before_digest,
            after_content,
        });
    }
    Ok((record, prepared))
}

fn plan_from_prepared(
    request: &WorkspacePatchRequest,
    prepared: &[PreparedPatch],
) -> WorkspacePatchPlan {
    WorkspacePatchPlan {
        workspace_id: request.workspace_id.clone(),
        files: prepared
            .iter()
            .map(|patch| WorkspacePatchPlanFile {
                relative_path: patch.relative_path.clone(),
                before_digest: patch.before_digest.clone(),
                after_digest: sha256_bytes(patch.after_content.as_bytes()),
                byte_length: patch.after_content.len() as u64,
            })
            .collect(),
    }
}

fn position_offset(
    content: &str,
    position: &WorkspaceTextPosition,
    field: &str,
) -> Result<usize, UniversalExecError> {
    let line_index = usize::try_from(position.line - 1).unwrap_or(usize::MAX);
    let mut line_start = 0usize;
    let mut found = None;
    for (index, line) in content.split('\n').enumerate() {
        if index == line_index {
            found = Some((line_start, line));
            break;
        }
        line_start = line_start.saturating_add(line.len()).saturating_add(1);
    }
    let (line_start, line) = found.ok_or_else(|| {
        UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            "patch line exceeds file length",
            Some(field),
            false,
        )
    })?;
    let column = usize::try_from(position.column).unwrap_or(usize::MAX);
    let line_char_count = line.chars().count();
    if column > line_char_count {
        return Err(UniversalExecError::new(
            UniversalExecErrorCode::InvalidRequest,
            "patch column exceeds line length",
            Some(field),
            false,
        ));
    }
    let column_bytes = if column == line_char_count {
        line.len()
    } else {
        line.char_indices()
            .nth(column)
            .map(|(offset, _)| offset)
            .unwrap_or(line.len())
    };
    Ok(line_start + column_bytes)
}

fn rollback(
    config: &UniversalExecutorConfig,
    request: &WorkspacePatchRequest,
    record: &super::WorkspaceRecord,
    applied: &[PreparedPatch],
    results: &[WorkspaceWriteResult],
) -> Result<(), UniversalExecError> {
    for (patch, result) in applied.iter().zip(results).rev() {
        let restored = if let Some(content) = &patch.before_content {
            write_workspace_text(
                config,
                &WorkspaceWriteRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id.clone(),
                    relative_path: patch.relative_path.clone(),
                    content: content.clone(),
                    expected_digest: Some(result.after_digest.clone()),
                },
            )
            .map(|_| ())
        } else {
            remove_workspace_file(record, &patch.relative_path)
        };
        if let Err(error) = restored {
            return Err(UniversalExecError::new(
                UniversalExecErrorCode::WorkspaceMutationIncomplete,
                format!(
                    "batch patch failed and rollback of {} also failed: {error}",
                    patch.relative_path
                ),
                Some("files"),
                false,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_uses_one_based_lines_and_unicode_columns() {
        let text = "alpha\nβeta\n";
        assert_eq!(
            position_offset(
                text,
                &WorkspaceTextPosition { line: 2, column: 1 },
                "position"
            )
            .unwrap(),
            8
        );
        assert_eq!(
            position_offset(
                text,
                &WorkspaceTextPosition { line: 3, column: 0 },
                "position"
            )
            .unwrap(),
            text.len()
        );
    }
}
