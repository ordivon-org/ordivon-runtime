use super::*;

impl ServerHandler for RuntimeServer {
    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<InitializeResult, McpError>> + Send + '_ {
        self.record_protocol_observation("initialize", None, &context);
        context.peer.set_peer_info(request.clone());
        let mut info = self.get_info();
        info.protocol_version =
            if ProtocolVersion::KNOWN_VERSIONS.contains(&request.protocol_version) {
                request.protocol_version.clone()
            } else {
                info.protocol_version
            };
        std::future::ready(Ok(info))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[
            ProtocolVersion::V_2026_07_28,
            ProtocolVersion::V_2025_11_25,
            ProtocolVersion::V_2025_06_18,
        ])
    }

    fn discover(
        &self,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<DiscoverResult, McpError>> + Send + '_ {
        self.record_protocol_observation("server/discover", None, &context);
        std::future::ready(Ok(self.discovery_result()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool = request.name.to_string();
        let protocol_version = context
            .protocol_version()
            .map(|version| version.to_string());
        let client = context.client_info().map(|implementation| {
            json!({
                "name": implementation.name,
                "version": implementation.version,
                "title": implementation.title,
            })
        });
        self.record_protocol_observation("tools/call", Some(tool.as_str()), &context);
        let started = Instant::now();
        let call = ToolCallContext::new(self, request, context);
        let result = self.tool_router.call(call).await;
        self.record_tool_call_outcome(
            tool.as_str(),
            protocol_version,
            client,
            elapsed_ms(started),
            &result,
        );
        result
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.record_protocol_observation("tools/list", None, &context);
        Ok(ListToolsResult::with_all_items(self.tool_router.list_all()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("ordivon-runtime-mcp", env!("CARGO_PKG_VERSION"))
                    .with_title("Ordivon Runtime"),
            )
            .with_instructions(
                "Local transactional Ordivon Runtime. MCP adapts protocol lifecycle only; durable Workspace, Job, Attempt, cancellation, recovery, and Artifact truth live in Runtime Core. workspace.exec uses trusted_local by default and supports explicit contained_local authority reduction. task.get/task.observe/task.list/task.cancel are ordinary Ordivon Tools, not MCP Tasks extension methods.",
            )
    }
}

impl RuntimeServer {
    fn record_protocol_observation(
        &self,
        method: &str,
        tool: Option<&str>,
        context: &RequestContext<RoleServer>,
    ) {
        let Some(path) = &self.state.trace_path else {
            return;
        };
        let _guard = match GLOBAL_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("trace lock poisoned: {error}");
                return;
            }
        };
        let client = context.client_info();
        let record = json!({
            "traceId": next_trace_id("protocol"),
            "kind": "mcp_protocol_observation",
            "method": method,
            "tool": tool,
            "protocolVersion": context.protocol_version().map(|version| version.to_string()),
            "client": client.map(|implementation| json!({
                "name": implementation.name,
                "version": implementation.version,
                "title": implementation.title,
            })),
            "clientCapabilities": context
                .client_capabilities()
                .and_then(|capabilities| serde_json::to_value(capabilities).ok()),
            "observedUnixMs": unix_ms(),
        });
        let write_result =
            crate::append_rotating_jsonl(path, &record, crate::DEFAULT_TRACE_ROTATION_BYTES);
        if let Err(error) = write_result {
            tracing::warn!("cannot append protocol trace {}: {error}", path.display());
        }
    }
}

impl RuntimeServer {
    fn record_tool_call_outcome(
        &self,
        tool: &str,
        protocol_version: Option<String>,
        client: Option<serde_json::Value>,
        total_ms: u64,
        result: &Result<CallToolResponse, McpError>,
    ) {
        let Some(path) = &self.state.trace_path else {
            return;
        };
        let (outcome, ok, error) = match result {
            Ok(CallToolResponse::Complete(value)) if value.is_error == Some(true) => (
                "tool_error",
                false,
                value
                    .structured_content
                    .as_ref()
                    .and_then(|content| content.get("error")),
            ),
            Ok(_) => ("success", true, None),
            Err(_) => ("protocol_error", false, None),
        };
        let _guard = match GLOBAL_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(lock_error) => {
                tracing::warn!("trace lock poisoned: {lock_error}");
                return;
            }
        };
        let record = json!({
            "traceId": next_trace_id("tool-call"),
            "kind": "mcp_tool_call_outcome",
            "tool": tool,
            "protocolVersion": protocol_version,
            "client": client,
            "outcome": outcome,
            "ok": ok,
            "totalMs": total_ms,
            "errorCode": error
                .and_then(|value| value.get("code"))
                .and_then(serde_json::Value::as_str),
            "errorOrigin": error
                .and_then(|value| value.get("origin"))
                .and_then(serde_json::Value::as_str),
            "retryClass": error
                .and_then(|value| value.get("retryClass"))
                .and_then(serde_json::Value::as_str),
            "commitState": error
                .and_then(|value| value.get("commitState"))
                .and_then(serde_json::Value::as_str),
            "field": error
                .and_then(|value| value.get("field"))
                .and_then(serde_json::Value::as_str),
            "observedUnixMs": unix_ms(),
        });
        let write_result =
            crate::append_rotating_jsonl(path, &record, crate::DEFAULT_TRACE_ROTATION_BYTES);
        if let Err(write_error) = write_result {
            tracing::warn!(
                "cannot append tool outcome trace {}: {write_error}",
                path.display()
            );
        }
    }
}
