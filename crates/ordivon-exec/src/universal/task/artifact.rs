use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};

use super::super::{
    invalid, sha256_file, ArtifactReadRequest, ArtifactReadResult, UniversalExecError,
    UniversalExecErrorCode, UniversalExecutorConfig,
};
use super::status::load_task_metadata;
use super::{RESULT_FILE, STDERR_FILE, STDOUT_FILE};

pub fn read_task_artifact(
    config: &UniversalExecutorConfig,
    request: &ArtifactReadRequest,
) -> Result<ArtifactReadResult, UniversalExecError> {
    request.validate_shape()?;
    let task_dir = config.task_path(&request.task_id);
    let _metadata = load_task_metadata(&task_dir, &request.task_id)?;
    let expected = [
        (format!("{}.stdout", request.task_id), STDOUT_FILE),
        (format!("{}.stderr", request.task_id), STDERR_FILE),
        (format!("{}.result", request.task_id), RESULT_FILE),
    ];
    let file_name = expected
        .iter()
        .find_map(|(artifact_id, file_name)| {
            (artifact_id == &request.artifact_id).then_some(*file_name)
        })
        .ok_or_else(|| {
            UniversalExecError::new(
                UniversalExecErrorCode::ArtifactNotFound,
                "artifact ID is not defined for this task",
                Some("artifactId"),
                false,
            )
        })?;
    let path = task_dir.join(file_name);
    let metadata = fs::metadata(&path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotFound,
            format!("artifact is not available: {error}"),
            Some("artifactId"),
            false,
        )
    })?;
    if request.offset > metadata.len() {
        return Err(invalid("offset exceeds artifact length", "offset"));
    }
    let mut file = File::open(&path).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot open artifact: {error}"),
            Some("artifactId"),
            false,
        )
    })?;
    file.seek(SeekFrom::Start(request.offset))
        .map_err(|error| {
            UniversalExecError::new(
                UniversalExecErrorCode::IoError,
                format!("cannot seek artifact: {error}"),
                Some("offset"),
                false,
            )
        })?;
    let remaining = metadata.len() - request.offset;
    let to_read = remaining.min(request.max_bytes) as usize;
    let mut bytes = vec![0_u8; to_read];
    file.read_exact(&mut bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::IoError,
            format!("cannot read artifact: {error}"),
            Some("artifactId"),
            false,
        )
    })?;
    let content = String::from_utf8(bytes).map_err(|error| {
        UniversalExecError::new(
            UniversalExecErrorCode::ArtifactNotUtf8,
            format!("artifact is not UTF-8: {error}"),
            Some("artifactId"),
            false,
        )
    })?;
    let next_offset = request.offset + to_read as u64;
    Ok(ArtifactReadResult {
        task_id: request.task_id.clone(),
        artifact_id: request.artifact_id.clone(),
        content,
        offset: request.offset,
        next_offset,
        eof: next_offset == metadata.len(),
        digest: sha256_file(&path)?,
    })
}
