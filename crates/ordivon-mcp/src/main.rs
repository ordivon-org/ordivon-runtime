use ordivon_exec::{
    read_many, read_text, repo_snapshot, search_text, ExecError, ReadManyRequest, ReadManyResult,
    ReadTextRequest, ReadTextResult, RepoSnapshotRequest, RepoSnapshotResult, SearchTextRequest,
    SearchTextResult,
};
use rmcp::{
    handler::server::{tool::IntoCallToolResult, wrapper::Parameters},
    model::CallToolResult,
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData, ServerHandler, ServiceExt,
};
use serde::Serialize;

#[derive(Clone, Default)]
struct OrdivonMcp;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ToolOutcome<T> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ExecError>,
}

impl<T> ToolOutcome<T> {
    fn from_result(result: Result<T, ExecError>) -> Self {
        match result {
            Ok(value) => Self {
                ok: true,
                result: Some(value),
                error: None,
            },
            Err(error) => Self {
                ok: false,
                result: None,
                error: Some(error),
            },
        }
    }
}

impl<T> IntoCallToolResult for ToolOutcome<T>
where
    T: Serialize + JsonSchema + Send + 'static,
{
    fn into_call_tool_result(self) -> Result<CallToolResult, ErrorData> {
        let ok = self.ok;
        let value = serde_json::to_value(self).map_err(|error| {
            ErrorData::internal_error(format!("failed to serialize tool result: {error}"), None)
        })?;
        Ok(if ok {
            CallToolResult::structured(value)
        } else {
            CallToolResult::structured_error(value)
        })
    }
}

#[tool_router]
impl OrdivonMcp {
    #[tool(
        description = "Read a bounded UTF-8 line range with a strong SHA-256 file revision and explicit continuation metadata. Use this instead of shell cat, sed, or head.",
        annotations(
            title = "Read bounded text",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn read_text(
        &self,
        Parameters(request): Parameters<ReadTextRequest>,
    ) -> ToolOutcome<ReadTextResult> {
        ToolOutcome::from_result(read_text(&request))
    }

    #[tool(
        description = "Read several bounded UTF-8 file ranges in one call under one total byte budget. Each item succeeds or fails independently.",
        annotations(
            title = "Read many files",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn read_many(
        &self,
        Parameters(request): Parameters<ReadManyRequest>,
    ) -> ToolOutcome<ReadManyResult> {
        ToolOutcome::from_result(read_many(&request))
    }

    #[tool(
        description = "Search text through a bounded structured ripgrep invocation. Supply semantic search parameters, not a shell command.",
        annotations(
            title = "Search text",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn search_text(
        &self,
        Parameters(request): Parameters<SearchTextRequest>,
    ) -> ToolOutcome<SearchTextResult> {
        ToolOutcome::from_result(search_text(&request))
    }

    #[tool(
        description = "Return exact Git branch, HEAD, upstream distance, and dirty-state counts from one porcelain-v2 status call.",
        annotations(
            title = "Repository snapshot",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn repo_snapshot(
        &self,
        Parameters(request): Parameters<RepoSnapshotRequest>,
    ) -> ToolOutcome<RepoSnapshotResult> {
        ToolOutcome::from_result(repo_snapshot(&request))
    }
}

#[tool_handler(
    name = "ordivon-structured-exec",
    version = "0.1.0",
    instructions = "Read-only structured local execution tools. Prefer read_many over repeated reads. Tool errors are structured and caller-visible."
)]
impl ServerHandler for OrdivonMcp {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = OrdivonMcp.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_failures_become_structured_tool_errors() {
        let outcome = ToolOutcome::<ReadTextResult>::from_result(Err(ExecError {
            code: ordivon_exec::ExecErrorCode::PathNotFound,
            message: "missing".to_string(),
            path: Some("/missing".to_string()),
            retryable: false,
        }));
        let result = outcome.into_call_tool_result().unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result
                .structured_content
                .as_ref()
                .and_then(|value| value.get("ok"))
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
    }
}
