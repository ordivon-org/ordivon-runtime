use super::*;

#[tool_handler]
impl ServerHandler for M4Server {
    fn get_info(&self) -> ServerInfo {
        let mut tools_task = ToolsTaskCapability::default();
        tools_task.call = Some(JsonObject::new());
        let mut requests = TaskRequestsCapability::default();
        requests.tools = Some(tools_task);
        let mut tasks = TasksCapability::default();
        tasks.requests = Some(requests);
        tasks.list = Some(JsonObject::new());
        tasks.cancel = Some(JsonObject::new());
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks_with(tasks)
                .build(),
        )
        .with_server_info(
            Implementation::new("ordivon-m4-experimental", env!("CARGO_PKG_VERSION"))
                .with_title("Ordivon Experimental MCP Adapter"),
        )
        .with_instructions(
            "Experimental localhost-only Ordivon adapter. Prefer compact workspace tools. workspace.exec supports native MCP Tasks or explicit Task handles. No production routing is authorized.",
        )
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        if request.name.as_ref() != "workspace.exec" {
            return Err(McpError::invalid_params(
                "only workspace.exec supports task-based invocation",
                None,
            ));
        }
        let run_request = parse_task_run(request.arguments)?;
        let task_id = run_request.execution.task_id.clone();
        let projection = NativeTaskProjection {
            schema_version: 1,
            task_id: task_id.clone(),
            stdout_tail_bytes: run_request.stdout_tail_bytes,
            stderr_tail_bytes: run_request.stderr_tail_bytes,
            ttl_ms: request.task.and_then(|task| task.ttl),
        };
        let config = self.state.config.executor.clone();
        let run_for_start = run_request.clone();
        let start_result = tokio::task::spawn_blocking(move || {
            start_universal_task(&config, &run_for_start.execution).map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task enqueue: {error}"), None)
        })?;
        if let Err(error) = start_result {
            return Err(mcp_error_from_m4(error));
        }
        if let Err(error) = write_native_projection(&self.state.config.executor, &projection) {
            let cancel_config = self.state.config.executor.clone();
            let cancel_id = task_id.clone();
            let _ = tokio::task::spawn_blocking(move || {
                cancel_universal_task(
                    &cancel_config,
                    &TaskCancelRequest {
                        schema_version: 1,
                        task_id: cancel_id,
                    },
                )
            })
            .await;
            return Err(mcp_error_from_m4(error));
        }
        let task = self.load_mcp_task(&task_id).await?;
        Ok(CreateTaskResult::new(task))
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        self.load_mcp_task(&request.task_id)
            .await
            .map(GetTaskResult::new)
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let protocol_started = Instant::now();
        let projection = read_native_projection(&self.state.config.executor, &request.task_id)
            .map_err(mcp_error_from_m4)?;
        let config = self.state.config.executor.clone();
        let task_id = request.task_id.clone();
        let observation = tokio::task::spawn_blocking(move || {
            await_universal_task_compact(
                &config,
                &TaskAwaitRequest {
                    schema_version: 1,
                    task_id,
                    wait_ms: 0,
                    stdout_tail_bytes: projection.stdout_tail_bytes,
                    stderr_tail_bytes: projection.stderr_tail_bytes,
                },
            )
            .map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task result load: {error}"), None)
        })?
        .map_err(mcp_error_from_m4)?;
        if observation.status == MigrationTaskStatus::Working {
            return Err(McpError::invalid_params("task result is not ready", None));
        }
        let ok = observation.status == MigrationTaskStatus::Completed;
        let error = match observation.status {
            MigrationTaskStatus::Completed => None,
            MigrationTaskStatus::Cancelled => Some(M4Error {
                code: "TASK_CANCELLED".to_string(),
                message: "task was cancelled".to_string(),
                field: Some("taskId".to_string()),
                retryable: false,
            }),
            MigrationTaskStatus::Failed => Some(M4Error {
                code: "TASK_FAILED".to_string(),
                message: observation
                    .error_summary
                    .clone()
                    .unwrap_or_else(|| "task failed".to_string()),
                field: Some("taskId".to_string()),
                retryable: false,
            }),
            _ => None,
        };
        let trace = M4TraceSummary {
            trace_id: next_trace_id("task-result"),
            core_ms: elapsed_ms(protocol_started),
            total_ms: elapsed_ms(protocol_started),
        };
        self.record_trace("tasks.result", &trace, ok);
        let outcome = M4Outcome {
            ok,
            result: Some(observation),
            error,
            trace: None,
        };
        let tool_result = outcome.into_call_tool_result()?;
        let value = serde_json::to_value(tool_result).map_err(|error| {
            McpError::internal_error(format!("cannot encode task result: {error}"), None)
        })?;
        Ok(GetTaskPayloadResult::new(value))
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let config = self.state.config.executor.clone();
        let task_id = request.task_id.clone();
        tokio::task::spawn_blocking(move || {
            cancel_universal_task(
                &config,
                &TaskCancelRequest {
                    schema_version: 1,
                    task_id,
                },
            )
            .map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task cancellation: {error}"), None)
        })?
        .map_err(mcp_error_from_m4)?;
        self.load_mcp_task(&request.task_id)
            .await
            .map(CancelTaskResult::new)
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let cursor = request.and_then(|params| params.cursor);
        let config = self.state.config.executor.clone();
        let page = tokio::task::spawn_blocking(move || list_native_task_ids(&config, cursor))
            .await
            .map_err(|error| {
                McpError::internal_error(format!("cannot join task listing: {error}"), None)
            })?
            .map_err(mcp_error_from_m4)?;
        let mut tasks = Vec::with_capacity(page.0.len());
        for task_id in page.0 {
            tasks.push(self.load_mcp_task(&task_id).await?);
        }
        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.1;
        Ok(result)
    }
}
impl M4Server {
    async fn load_mcp_task(&self, task_id: &str) -> Result<Task, McpError> {
        let config = self.state.config.executor.clone();
        let task_id_owned = task_id.to_string();
        let snapshot = tokio::task::spawn_blocking(move || {
            snapshot_universal_task(&config, &task_id_owned).map_err(M4Error::from)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task snapshot: {error}"), None)
        })?
        .map_err(mcp_error_from_m4)?;
        let ttl = read_native_projection(&self.state.config.executor, task_id)
            .map(|projection| projection.ttl_ms)
            .unwrap_or(None);
        task_from_snapshot(snapshot, ttl)
    }
}

pub(super) fn task_from_snapshot(
    snapshot: DurableTaskSnapshot,
    ttl_ms: Option<u64>,
) -> Result<Task, McpError> {
    let mut task = Task::new(
        snapshot.task_id,
        mcp_task_status(snapshot.status),
        format_unix_ms(snapshot.created_unix_ms)?,
        format_unix_ms(snapshot.updated_unix_ms)?,
    )
    .with_status_message(snapshot.status_message);
    task.ttl = ttl_ms;
    if let Some(poll) = snapshot.poll_after_ms {
        task = task.with_poll_interval(poll);
    }
    Ok(task)
}

fn mcp_task_status(status: MigrationTaskStatus) -> TaskStatus {
    match status {
        MigrationTaskStatus::Working => TaskStatus::Working,
        MigrationTaskStatus::InputRequired => TaskStatus::InputRequired,
        MigrationTaskStatus::Completed => TaskStatus::Completed,
        MigrationTaskStatus::Failed => TaskStatus::Failed,
        MigrationTaskStatus::Cancelled => TaskStatus::Cancelled,
    }
}
fn format_unix_ms(value: u128) -> Result<String, McpError> {
    let nanos = value
        .checked_mul(1_000_000)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or_else(|| McpError::internal_error("task timestamp is out of range", None))?;
    let timestamp = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|error| McpError::internal_error(format!("invalid task time: {error}"), None))?;
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| {
            McpError::internal_error(format!("cannot format task time: {error}"), None)
        })
}

fn mcp_error_from_m4(error: M4Error) -> McpError {
    let data = serde_json::to_value(&error).ok();
    if error.code == "TASK_NOT_FOUND" || error.code == "INVALID_REQUEST" {
        McpError::invalid_params(error.message, data)
    } else {
        McpError::internal_error(error.message, data)
    }
}

pub(super) fn list_native_task_ids(
    config: &UniversalExecutorConfig,
    cursor: Option<String>,
) -> Result<(Vec<String>, Option<String>), M4Error> {
    let root = native_projection_root(config);
    let mut ids = Vec::new();
    for entry in fs::read_dir(&root)
        .map_err(|error| M4Error::internal(format!("cannot list task projections: {error}")))?
    {
        let entry = entry.map_err(|error| {
            M4Error::internal(format!("cannot read task projection entry: {error}"))
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
            if cursor.as_ref().is_none_or(|cursor| stem > cursor.as_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    ids.sort();
    let next_cursor = (ids.len() > 100).then(|| ids[99].clone());
    ids.truncate(100);
    Ok((ids, next_cursor))
}
