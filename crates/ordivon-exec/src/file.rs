use crate::{ExecError, ExecErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub const MAX_READ_LINES: u64 = 10_000;
pub const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_BATCH_FILES: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextRequest {
    pub path: String,
    pub start_line: u64,
    pub max_lines: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextResult {
    pub path: String,
    pub digest: String,
    pub size_bytes: u64,
    pub total_lines: u64,
    pub returned_bytes: u64,
    pub start_line: u64,
    pub end_line: Option<u64>,
    pub next_line: Option<u64>,
    pub eof: bool,
    pub truncated: bool,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadManyRequest {
    pub requests: Vec<ReadTextRequest>,
    pub max_total_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadManyItem {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ReadTextResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ExecError>,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadManyResult {
    pub items: Vec<ReadManyItem>,
    pub total_bytes: u64,
    pub budget_exhausted: bool,
}

pub fn read_text(request: &ReadTextRequest) -> Result<ReadTextResult, ExecError> {
    validate_read_request(request)?;

    let requested_path = Path::new(&request.path);
    let metadata = fs::metadata(requested_path).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ExecErrorCode::PathNotFound
        } else {
            ExecErrorCode::IoError
        };
        ExecError::new(
            code,
            error.to_string(),
            Some(request.path.clone()),
            error.kind() == std::io::ErrorKind::Interrupted,
        )
    })?;
    if !metadata.is_file() {
        return Err(ExecError::new(
            ExecErrorCode::PathNotFile,
            "path is not a regular file",
            Some(request.path.clone()),
            false,
        ));
    }

    let canonical_path = fs::canonicalize(requested_path).map_err(|error| {
        ExecError::new(
            ExecErrorCode::IoError,
            error.to_string(),
            Some(request.path.clone()),
            error.kind() == std::io::ErrorKind::Interrupted,
        )
    })?;
    let file = fs::File::open(&canonical_path).map_err(|error| {
        ExecError::new(
            ExecErrorCode::IoError,
            error.to_string(),
            Some(canonical_path.display().to_string()),
            error.kind() == std::io::ErrorKind::Interrupted,
        )
    })?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut content = String::new();
    let mut buffer = Vec::new();
    let mut current_line = 0_u64;
    let mut lines_read = 0_u64;
    let mut next_line = None;

    loop {
        buffer.clear();
        let count = reader.read_until(b'\n', &mut buffer).map_err(|error| {
            ExecError::new(
                ExecErrorCode::IoError,
                error.to_string(),
                Some(canonical_path.display().to_string()),
                error.kind() == std::io::ErrorKind::Interrupted,
            )
        })?;
        if count == 0 {
            break;
        }

        hasher.update(&buffer);
        current_line += 1;
        let line = std::str::from_utf8(&buffer).map_err(|error| {
            ExecError::new(
                ExecErrorCode::UnsupportedEncoding,
                format!("v0 accepts UTF-8 text only: {error}"),
                Some(canonical_path.display().to_string()),
                false,
            )
        })?;

        if current_line < request.start_line || next_line.is_some() {
            continue;
        }
        if lines_read >= request.max_lines {
            next_line = Some(current_line);
            continue;
        }
        if content.len().saturating_add(line.len()) > request.max_bytes as usize {
            if content.is_empty() {
                return Err(ExecError::new(
                    ExecErrorCode::LineExceedsByteBudget,
                    format!(
                        "line {} requires {} bytes but maxBytes is {}",
                        current_line,
                        line.len(),
                        request.max_bytes
                    ),
                    Some(canonical_path.display().to_string()),
                    true,
                ));
            }
            next_line = Some(current_line);
            continue;
        }

        content.push_str(line);
        lines_read += 1;
    }

    if request.start_line > current_line.saturating_add(1) {
        return Err(ExecError::new(
            ExecErrorCode::StartLineOutOfRange,
            format!(
                "startLine {} is beyond the continuation position {}",
                request.start_line,
                current_line + 1
            ),
            Some(canonical_path.display().to_string()),
            false,
        ));
    }

    let digest = format!("sha256:{}", hex::encode(hasher.finalize()));
    let eof = next_line.is_none();
    let end_line = (lines_read > 0).then_some(request.start_line + lines_read - 1);

    Ok(ReadTextResult {
        path: canonical_path.display().to_string(),
        digest,
        size_bytes: metadata.len(),
        total_lines: current_line,
        returned_bytes: content.len() as u64,
        start_line: request.start_line,
        end_line,
        next_line,
        eof,
        truncated: !eof,
        content,
    })
}

pub fn read_many(request: &ReadManyRequest) -> Result<ReadManyResult, ExecError> {
    if request.requests.is_empty() || request.requests.len() > MAX_BATCH_FILES {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("requests must contain 1..={MAX_BATCH_FILES} items"),
            None,
            false,
        ));
    }
    if request.max_total_bytes == 0 || request.max_total_bytes > MAX_READ_BYTES {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("maxTotalBytes must be in 1..={MAX_READ_BYTES}"),
            None,
            false,
        ));
    }

    let mut items = Vec::with_capacity(request.requests.len());
    let mut total_bytes = 0_u64;
    let mut budget_exhausted = false;

    for item_request in &request.requests {
        let remaining = request.max_total_bytes.saturating_sub(total_bytes);
        if remaining == 0 {
            budget_exhausted = true;
            items.push(ReadManyItem {
                path: item_request.path.clone(),
                result: None,
                error: Some(ExecError::new(
                    ExecErrorCode::BatchBudgetExhausted,
                    "batch byte budget is exhausted",
                    Some(item_request.path.clone()),
                    true,
                )),
            });
            continue;
        }

        let mut bounded = item_request.clone();
        bounded.max_bytes = bounded.max_bytes.min(remaining);
        match read_text(&bounded) {
            Ok(result) => {
                total_bytes = total_bytes.saturating_add(result.content.len() as u64);
                items.push(ReadManyItem {
                    path: item_request.path.clone(),
                    result: Some(result),
                    error: None,
                });
            }
            Err(error) => items.push(ReadManyItem {
                path: item_request.path.clone(),
                result: None,
                error: Some(error),
            }),
        }
    }

    Ok(ReadManyResult {
        items,
        total_bytes,
        budget_exhausted,
    })
}

fn validate_read_request(request: &ReadTextRequest) -> Result<(), ExecError> {
    if request.path.trim().is_empty() {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            "path must not be empty",
            None,
            false,
        ));
    }
    if request.start_line == 0 {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            "startLine is 1-based and must be at least 1",
            Some(request.path.clone()),
            false,
        ));
    }
    if request.max_lines == 0 || request.max_lines > MAX_READ_LINES {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("maxLines must be in 1..={MAX_READ_LINES}"),
            Some(request.path.clone()),
            false,
        ));
    }
    if request.max_bytes == 0 || request.max_bytes > MAX_READ_BYTES {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("maxBytes must be in 1..={MAX_READ_BYTES}"),
            Some(request.path.clone()),
            false,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("ordivon-exec-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, name: &str, bytes: &[u8]) -> String {
            let path = self.path.join(name);
            fs::write(&path, bytes).unwrap();
            path.display().to_string()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn request(path: String) -> ReadTextRequest {
        ReadTextRequest {
            path,
            start_line: 1,
            max_lines: 200,
            max_bytes: 64 * 1024,
        }
    }

    #[test]
    fn reads_bounded_line_range_with_digest_and_continuation() {
        let dir = TestDir::new();
        let path = dir.write("sample.txt", b"alpha\nbeta\ngamma\n");
        let result = read_text(&ReadTextRequest {
            path,
            start_line: 2,
            max_lines: 1,
            max_bytes: 1024,
        })
        .unwrap();

        assert_eq!(result.content, "beta\n");
        assert_eq!(result.start_line, 2);
        assert_eq!(result.end_line, Some(2));
        assert_eq!(result.next_line, Some(3));
        assert!(!result.eof);
        assert!(result.truncated);
        assert!(result.digest.starts_with("sha256:"));
        assert_eq!(result.size_bytes, 17);
        assert_eq!(result.total_lines, 3);
        assert_eq!(result.returned_bytes, 5);
    }

    #[test]
    fn permits_the_continuation_position_after_the_last_line() {
        let dir = TestDir::new();
        let path = dir.write("sample.txt", b"alpha\nbeta\n");
        let result = read_text(&ReadTextRequest {
            path,
            start_line: 3,
            max_lines: 10,
            max_bytes: 1024,
        })
        .unwrap();

        assert_eq!(result.content, "");
        assert_eq!(result.end_line, None);
        assert_eq!(result.next_line, None);
        assert!(result.eof);
    }

    #[test]
    fn rejects_non_utf8_without_guessing() {
        let dir = TestDir::new();
        let path = dir.write("binary.txt", &[0xff, 0xfe, 0xfd]);
        let error = read_text(&request(path)).unwrap_err();
        assert_eq!(error.code, ExecErrorCode::UnsupportedEncoding);
    }

    #[test]
    fn rejects_a_line_that_cannot_fit_in_the_byte_budget() {
        let dir = TestDir::new();
        let path = dir.write("long.txt", b"abcdefghij\n");
        let error = read_text(&ReadTextRequest {
            path,
            start_line: 1,
            max_lines: 10,
            max_bytes: 5,
        })
        .unwrap_err();
        assert_eq!(error.code, ExecErrorCode::LineExceedsByteBudget);
        assert!(error.retryable);
    }

    #[test]
    fn read_many_preserves_independent_item_failures() {
        let dir = TestDir::new();
        let good = dir.write("good.txt", b"good\n");
        let missing = dir.path.join("missing.txt").display().to_string();
        let result = read_many(&ReadManyRequest {
            requests: vec![request(good), request(missing)],
            max_total_bytes: 1024,
        })
        .unwrap();

        assert!(result.items[0].result.is_some());
        assert!(result.items[0].error.is_none());
        assert!(result.items[1].result.is_none());
        assert_eq!(
            result.items[1].error.as_ref().unwrap().code,
            ExecErrorCode::PathNotFound
        );
    }

    #[test]
    fn read_many_enforces_one_total_byte_budget() {
        let dir = TestDir::new();
        let first = dir.write("first.txt", b"1234\n");
        let second = dir.write("second.txt", b"5678\n");
        let result = read_many(&ReadManyRequest {
            requests: vec![request(first), request(second)],
            max_total_bytes: 5,
        })
        .unwrap();

        assert_eq!(result.total_bytes, 5);
        assert!(result.budget_exhausted);
        assert_eq!(
            result.items[1].error.as_ref().unwrap().code,
            ExecErrorCode::BatchBudgetExhausted
        );
    }
}
