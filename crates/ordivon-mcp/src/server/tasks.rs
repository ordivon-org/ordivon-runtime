use super::*;

#[tool_handler]
impl ServerHandler for OrdivonServer {
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
            Implementation::new("ordivon-mcp", env!("CARGO_PKG_VERSION"))
                .with_title("Ordivon MCP"),
        )
        .with_instructions(
            "Local transactional Ordivon runtime. workspace.exec creates a durable server-generated Job and Attempt; MCP Tasks project SQLite truth. Reversible repository work is permitted, while irreversible external effects require explicit authority.",
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
        let mut run_request = parse_task_run(request.arguments)?;
        run_request.wait_ms = 0;
        let runtime = self.state.runtime.clone();
        let observation = tokio::task::spawn_blocking(move || runtime.run_task(&run_request))
            .await
            .map_err(|error| {
                McpError::internal_error(format!("cannot join task enqueue: {error}"), None)
            })?
            .map_err(ToolError::from)
            .map_err(mcp_error)?;
        let task = self.load_mcp_task(&observation.job_id).await?;
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
        let runtime = self.state.runtime.clone();
        let job_id = request.task_id.clone();
        let observation = tokio::task::spawn_blocking(move || {
            runtime.observe_task(&TaskObserveRequest {
                schema_version: ordivon_exec::RUNTIME_SCHEMA_VERSION,
                job_id,
                wait_ms: 0,
                stdout_tail_bytes: 4096,
                stderr_tail_bytes: 4096,
            })
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task result load: {error}"), None)
        })?
        .map_err(ToolError::from)
        .map_err(mcp_error)?;
        if matches!(observation.status.as_str(), "queued" | "working") {
            return Err(McpError::invalid_params("task result is not ready", None));
        }
        let ok = observation.status == "succeeded";
        let outcome = if ok {
            ToolOutcome::Success(observation)
        } else {
            ToolOutcome::Error(terminal_task_error(&observation))
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
        let runtime = self.state.runtime.clone();
        let job_id = request.task_id.clone();
        tokio::task::spawn_blocking(move || {
            runtime.cancel_task(&TaskCancelRequest {
                schema_version: ordivon_exec::RUNTIME_SCHEMA_VERSION,
                job_id,
            })
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task cancellation: {error}"), None)
        })?
        .map_err(ToolError::from)
        .map_err(mcp_error)?;
        self.load_mcp_task(&request.task_id)
            .await
            .map(CancelTaskResult::new)
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let cursor = decode_cursor(request.and_then(|params| params.cursor))?;
        let runtime = self.state.runtime.clone();
        let page = tokio::task::spawn_blocking(move || {
            runtime.list_jobs(&RuntimeJobListRequest { limit: 100, cursor })
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task listing: {error}"), None)
        })?
        .map_err(ToolError::from)
        .map_err(mcp_error)?;
        let mut tasks = Vec::with_capacity(page.jobs.len());
        for projection in page.jobs {
            tasks.push(self.load_mcp_task(&projection.job_id).await?);
        }
        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.next_cursor.as_ref().map(encode_cursor);
        Ok(result)
    }
}

impl OrdivonServer {
    async fn load_mcp_task(&self, job_id: &str) -> Result<Task, McpError> {
        let runtime = self.state.runtime.clone();
        let job_id_owned = job_id.to_string();
        let (job, attempt, projection) = tokio::task::spawn_blocking(move || {
            runtime.observe_task(&TaskObserveRequest {
                schema_version: ordivon_exec::RUNTIME_SCHEMA_VERSION,
                job_id: job_id_owned.clone(),
                wait_ms: 0,
                stdout_tail_bytes: 0,
                stderr_tail_bytes: 0,
            })?;
            let job = runtime.registry().get_job(&job_id_owned)?;
            let attempt = runtime.registry().get_latest_attempt(&job_id_owned)?;
            let projection = runtime.registry().project_job(&job_id_owned)?;
            Ok::<_, RuntimeError>((job, attempt, projection))
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join task snapshot: {error}"), None)
        })?
        .map_err(ToolError::from)
        .map_err(mcp_error)?;
        task_from_job(job, attempt.as_ref(), projection)
    }
}

pub(super) fn task_from_job(
    job: ordivon_exec::RuntimeJobRecord,
    attempt: Option<&ordivon_exec::AttemptRecord>,
    projection: ordivon_exec::JobProjection,
) -> Result<Task, McpError> {
    let updated_at_ms = attempt
        .and_then(|attempt| attempt.finished_at_ms.or(attempt.started_at_ms))
        .or_else(|| attempt.map(|attempt| attempt.created_at_ms))
        .unwrap_or(job.created_at_ms);
    let status = match projection.status.as_str() {
        "queued" | "working" => TaskStatus::Working,
        "succeeded" => TaskStatus::Completed,
        "cancelled" => TaskStatus::Cancelled,
        "failed" | "timed_out" | "lost" | "orphaned" => TaskStatus::Failed,
        unknown => {
            return Err(McpError::internal_error(
                format!("unknown Job projection status {unknown}"),
                None,
            ));
        }
    };
    let mut task = Task::new(
        job.job_id,
        status,
        format_unix_ms(job.created_at_ms)?,
        format_unix_ms(updated_at_ms)?,
    )
    .with_status_message(format!("Job is {}.", projection.status));
    if let Some(poll) = projection.poll_after_ms {
        task = task.with_poll_interval(poll);
    }
    Ok(task)
}

fn terminal_task_error(observation: &TaskObservation) -> ToolError {
    let (code, default_message) = match observation.status.as_str() {
        "cancelled" => ("TASK_CANCELLED", "task was cancelled"),
        "timed_out" => ("TASK_TIMED_OUT", "task exceeded its runtime limit"),
        "lost" => ("TASK_LOST", "task execution identity was lost"),
        "orphaned" => ("TASK_ORPHANED", "task execution identity is orphaned"),
        _ => ("TASK_FAILED", "task failed"),
    };
    ToolError {
        code: code.to_string(),
        message: observation
            .error_summary
            .clone()
            .unwrap_or_else(|| default_message.to_string()),
        field: Some("taskId".to_string()),
        retryable: false,
    }
}
