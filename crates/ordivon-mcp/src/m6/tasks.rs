use super::*;

#[tool_handler]
impl ServerHandler for M6Server {
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
            Implementation::new("ordivon-m6-experimental", env!("CARGO_PKG_VERSION"))
                .with_title("Ordivon M6 Transactional MCP Adapter"),
        )
        .with_instructions(
            "Experimental localhost-only transactional Ordivon adapter. workspace.exec creates a server-generated Job ID. Native MCP Tasks project the SQLite Job truth. No production routing or external side effects are authorized.",
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
                McpError::internal_error(format!("cannot join M6 task enqueue: {error}"), None)
            })?
            .map_err(M6McpError::from)
            .map_err(mcp_error_from_m6)?;
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
            runtime.observe_task(&M6TaskObserveRequest {
                schema_version: ordivon_exec::M6_SCHEMA_VERSION,
                job_id,
                wait_ms: 0,
                stdout_tail_bytes: 4096,
                stderr_tail_bytes: 4096,
            })
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join M6 task result load: {error}"), None)
        })?
        .map_err(M6McpError::from)
        .map_err(mcp_error_from_m6)?;
        if matches!(observation.status.as_str(), "queued" | "working") {
            return Err(McpError::invalid_params("task result is not ready", None));
        }
        let ok = observation.status == "succeeded";
        let outcome = if ok {
            M6Outcome::Success(observation)
        } else {
            M6Outcome::Error(terminal_task_error(&observation))
        };
        let tool_result = outcome.into_call_tool_result()?;
        let value = serde_json::to_value(tool_result).map_err(|error| {
            McpError::internal_error(format!("cannot encode M6 task result: {error}"), None)
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
            runtime.cancel_task(&M6TaskCancelRequest {
                schema_version: ordivon_exec::M6_SCHEMA_VERSION,
                job_id,
            })
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join M6 task cancellation: {error}"), None)
        })?
        .map_err(M6McpError::from)
        .map_err(mcp_error_from_m6)?;
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
            runtime.list_jobs(&JobListRequestM6 { limit: 100, cursor })
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join M6 task listing: {error}"), None)
        })?
        .map_err(M6McpError::from)
        .map_err(mcp_error_from_m6)?;
        let mut tasks = Vec::with_capacity(page.jobs.len());
        for projection in page.jobs {
            tasks.push(self.load_mcp_task(&projection.job_id).await?);
        }
        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.next_cursor.as_ref().map(encode_cursor);
        Ok(result)
    }
}

impl M6Server {
    async fn load_mcp_task(&self, job_id: &str) -> Result<Task, McpError> {
        let runtime = self.state.runtime.clone();
        let job_id_owned = job_id.to_string();
        let (job, attempt, projection) = tokio::task::spawn_blocking(move || {
            runtime.observe_task(&M6TaskObserveRequest {
                schema_version: ordivon_exec::M6_SCHEMA_VERSION,
                job_id: job_id_owned.clone(),
                wait_ms: 0,
                stdout_tail_bytes: 0,
                stderr_tail_bytes: 0,
            })?;
            let job = runtime.registry().get_job(&job_id_owned)?;
            let attempt = runtime.registry().get_latest_attempt(&job_id_owned)?;
            let projection = runtime.registry().project_job(&job_id_owned)?;
            Ok::<_, M6Error>((job, attempt, projection))
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("cannot join M6 task snapshot: {error}"), None)
        })?
        .map_err(M6McpError::from)
        .map_err(mcp_error_from_m6)?;
        task_from_job(job, attempt.as_ref(), projection)
    }
}

pub(super) fn task_from_job(
    job: ordivon_exec::JobRecordM6,
    attempt: Option<&ordivon_exec::AttemptRecordM6>,
    projection: ordivon_exec::JobProjectionM6,
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
                format!("unknown M6 Job projection status {unknown}"),
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
    .with_status_message(format!("M6 Job is {}.", projection.status));
    if let Some(poll) = projection.poll_after_ms {
        task = task.with_poll_interval(poll);
    }
    Ok(task)
}

fn terminal_task_error(observation: &M6TaskObservation) -> M6McpError {
    let (code, default_message) = match observation.status.as_str() {
        "cancelled" => ("TASK_CANCELLED", "task was cancelled"),
        "timed_out" => ("TASK_TIMED_OUT", "task exceeded its runtime limit"),
        "lost" => ("TASK_LOST", "task execution identity was lost"),
        "orphaned" => ("TASK_ORPHANED", "task execution identity is orphaned"),
        _ => ("TASK_FAILED", "task failed"),
    };
    M6McpError {
        code: code.to_string(),
        message: observation
            .error_summary
            .clone()
            .unwrap_or_else(|| default_message.to_string()),
        field: Some("taskId".to_string()),
        retryable: false,
    }
}
