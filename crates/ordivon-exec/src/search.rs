use crate::{ExecError, ExecErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

pub const MAX_SEARCH_RESULTS: usize = 10_000;
pub const MAX_SEARCH_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_SEARCH_GLOBS: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchPatternMode {
    Regex,
    Fixed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTextRequest {
    pub root: String,
    pub pattern: String,
    pub mode: SearchPatternMode,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub case_sensitive: bool,
    pub hidden: bool,
    pub max_results: usize,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSubmatch {
    pub start_byte: u64,
    pub end_byte: u64,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub line_number: u64,
    pub text: String,
    pub submatches: Vec<SearchSubmatch>,
}

#[derive(Clone, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTextResult {
    pub root: String,
    pub pattern: String,
    pub hits: Vec<SearchHit>,
    pub returned_bytes: u64,
    pub truncated: bool,
    pub tool: String,
}

pub fn search_text(request: &SearchTextRequest) -> Result<SearchTextResult, ExecError> {
    validate_search_request(request)?;
    let root = fs::canonicalize(&request.root).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ExecErrorCode::PathNotFound
        } else {
            ExecErrorCode::IoError
        };
        ExecError::new(code, error.to_string(), Some(request.root.clone()), false)
    })?;
    if !root.is_dir() {
        return Err(ExecError::new(
            ExecErrorCode::PathNotDirectory,
            "search root is not a directory",
            Some(root.display().to_string()),
            false,
        ));
    }

    let mut command = Command::new("rg");
    command.current_dir(&root).args([
        "--json",
        "--line-number",
        "--column",
        "--no-heading",
        "--color",
        "never",
        "--sort",
        "path",
        "--max-columns",
        "4096",
        "--max-columns-preview",
    ]);
    if matches!(request.mode, SearchPatternMode::Fixed) {
        command.arg("--fixed-strings");
    }
    command.arg(if request.case_sensitive {
        "--case-sensitive"
    } else {
        "--ignore-case"
    });
    if request.hidden {
        command.arg("--hidden");
    }
    command.args(["--glob", "!.git/**"]);
    for pattern in &request.include {
        command.args(["--glob", pattern]);
    }
    for pattern in &request.exclude {
        command.args(["--glob", &format!("!{pattern}")]);
    }
    command
        .arg("--")
        .arg(&request.pattern)
        .arg(".")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ExecErrorCode::ToolUnavailable
        } else {
            ExecErrorCode::IoError
        };
        ExecError::new(code, error.to_string(), None, false)
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        ExecError::new(
            ExecErrorCode::InvalidToolOutput,
            "ripgrep stdout was not captured",
            None,
            false,
        )
    })?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut hits = Vec::new();
    let mut returned_bytes = 0_u64;
    let mut truncated = false;

    loop {
        line.clear();
        let count = reader.read_line(&mut line).map_err(|error| {
            ExecError::new(ExecErrorCode::IoError, error.to_string(), None, true)
        })?;
        if count == 0 {
            break;
        }
        let Some(hit) = parse_match_event(&line)? else {
            continue;
        };
        let hit_bytes = hit.path.len() as u64
            + hit.text.len() as u64
            + hit
                .submatches
                .iter()
                .map(|item| item.text.len() as u64)
                .sum::<u64>();
        if hits.len() >= request.max_results
            || returned_bytes.saturating_add(hit_bytes) > request.max_bytes
        {
            truncated = true;
            let _ = child.kill();
            break;
        }
        returned_bytes = returned_bytes.saturating_add(hit_bytes);
        hits.push(hit);
    }

    let status = child
        .wait()
        .map_err(|error| ExecError::new(ExecErrorCode::IoError, error.to_string(), None, true))?;
    let mut stderr = String::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_string(&mut stderr);
    }
    if !truncated && !matches!(status.code(), Some(0 | 1)) {
        return Err(ExecError::new(
            ExecErrorCode::ToolFailed,
            if stderr.trim().is_empty() {
                format!("ripgrep exited with status {status}")
            } else {
                stderr.trim().to_string()
            },
            Some(root.display().to_string()),
            false,
        ));
    }

    Ok(SearchTextResult {
        root: root.display().to_string(),
        pattern: request.pattern.clone(),
        hits,
        returned_bytes,
        truncated,
        tool: "ripgrep".to_string(),
    })
}

fn validate_search_request(request: &SearchTextRequest) -> Result<(), ExecError> {
    if request.pattern.is_empty() {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            "pattern must not be empty",
            None,
            false,
        ));
    }
    if request.max_results == 0 || request.max_results > MAX_SEARCH_RESULTS {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("maxResults must be in 1..={MAX_SEARCH_RESULTS}"),
            None,
            false,
        ));
    }
    if request.max_bytes == 0 || request.max_bytes > MAX_SEARCH_BYTES {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("maxBytes must be in 1..={MAX_SEARCH_BYTES}"),
            None,
            false,
        ));
    }
    if request.include.len() > MAX_SEARCH_GLOBS || request.exclude.len() > MAX_SEARCH_GLOBS {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            format!("include and exclude support at most {MAX_SEARCH_GLOBS} entries each"),
            None,
            false,
        ));
    }
    if request
        .include
        .iter()
        .chain(request.exclude.iter())
        .any(|pattern| pattern.is_empty())
    {
        return Err(ExecError::new(
            ExecErrorCode::InvalidRequest,
            "glob patterns must not be empty",
            None,
            false,
        ));
    }
    Ok(())
}

fn parse_match_event(line: &str) -> Result<Option<SearchHit>, ExecError> {
    let value: Value = serde_json::from_str(line).map_err(|error| {
        ExecError::new(
            ExecErrorCode::InvalidToolOutput,
            format!("invalid ripgrep JSON: {error}"),
            None,
            false,
        )
    })?;
    if value.get("type").and_then(Value::as_str) != Some("match") {
        return Ok(None);
    }
    let data = value
        .get("data")
        .ok_or_else(|| invalid_rg("missing data"))?;
    let path = data
        .pointer("/path/text")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_rg("missing path.text"))?
        .strip_prefix("./")
        .unwrap_or_else(|| {
            data.pointer("/path/text")
                .and_then(Value::as_str)
                .unwrap_or_default()
        })
        .to_string();
    let line_number = data
        .get("line_number")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_rg("missing line_number"))?;
    let text = data
        .pointer("/lines/text")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_rg("missing lines.text"))?
        .to_string();
    let submatches = data
        .get("submatches")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_rg("missing submatches"))?
        .iter()
        .map(|item| {
            Ok(SearchSubmatch {
                start_byte: item
                    .get("start")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| invalid_rg("missing submatch.start"))?,
                end_byte: item
                    .get("end")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| invalid_rg("missing submatch.end"))?,
                text: item
                    .pointer("/match/text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid_rg("missing submatch.match.text"))?
                    .to_string(),
            })
        })
        .collect::<Result<Vec<_>, ExecError>>()?;
    Ok(Some(SearchHit {
        path,
        line_number,
        text,
        submatches,
    }))
}

fn invalid_rg(message: &str) -> ExecError {
    ExecError::new(
        ExecErrorCode::InvalidToolOutput,
        format!("invalid ripgrep JSON: {message}"),
        None,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_ripgrep_match_without_exposing_machine_protocol() {
        let line = r#"{"type":"match","data":{"path":{"text":"./src/lib.rs"},"lines":{"text":"pub fn read_text() {}\n"},"line_number":7,"absolute_offset":12,"submatches":[{"match":{"text":"read_text"},"start":7,"end":16}]}}"#;
        let hit = parse_match_event(line).unwrap().unwrap();
        assert_eq!(hit.path, "src/lib.rs");
        assert_eq!(hit.line_number, 7);
        assert_eq!(hit.submatches[0].start_byte, 7);
    }

    #[test]
    fn rejects_unbounded_search_requests() {
        let request = SearchTextRequest {
            root: ".".to_string(),
            pattern: "x".to_string(),
            mode: SearchPatternMode::Fixed,
            include: Vec::new(),
            exclude: Vec::new(),
            case_sensitive: true,
            hidden: false,
            max_results: MAX_SEARCH_RESULTS + 1,
            max_bytes: 10,
        };
        assert_eq!(
            validate_search_request(&request).unwrap_err().code,
            ExecErrorCode::InvalidRequest
        );
    }
}
