use super::*;

#[tool_router(vis = "pub(super)")]
impl RuntimeServer {
    #[tool(
        name = "workspace.open",
        description = "Resolve a revision already present in the local source repository, create one detached Git workspace at that exact commit, and return the resolved commit SHA. This tool does not fetch or update remote refs and does not isolate host authority.",
        annotations(
            title = "Open isolated workspace",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_open(
        &self,
        Parameters(request): Parameters<GitWorkspaceCreateRequest>,
    ) -> ToolOutcome<CompactWorkspaceOpenResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("workspace.open", move || {
            runtime.open_workspace(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.close",
        description = "Close one Workspace. By default, reject tracked or untracked changes; force=true may remove dirty files. Active or held Jobs always block closure.",
        annotations(
            title = "Close workspace",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_close(
        &self,
        Parameters(request): Parameters<WorkspaceCloseRequest>,
    ) -> ToolOutcome<WorkspaceCloseResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("workspace.close", move || {
            runtime.close_workspace(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.read",
        description = "Read bounded UTF-8 content from an isolated workspace in FULL or SLICE mode.",
        annotations(
            title = "Read workspace content",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_read(
        &self,
        Parameters(request): Parameters<WorkspaceReadRequest>,
    ) -> ToolOutcome<WorkspaceReadResult> {
        let config = self.state.executor.clone();
        self.run_core("workspace.read", move || match request.mode {
            WorkspaceReadMode::Full => {
                if request.offset != 0 {
                    return Err(ToolError::invalid(
                        "offset must be zero in FULL mode",
                        "offset",
                    ));
                }
                let result = read_workspace_text_compact(
                    &config,
                    &ExecWorkspaceReadRequest {
                        schema_version: request.schema_version,
                        workspace_id: request.workspace_id,
                        relative_path: request.relative_path,
                        max_bytes: request.max_bytes,
                    },
                )
                .map_err(ToolError::from)?;
                Ok(WorkspaceReadResult {
                    content: result.content,
                    digest: result.digest,
                    file_byte_length: None,
                    eof: None,
                })
            }
            WorkspaceReadMode::Slice => {
                let result = read_workspace_slice_compact(
                    &config,
                    &WorkspaceReadSliceRequest {
                        schema_version: request.schema_version,
                        workspace_id: request.workspace_id,
                        relative_path: request.relative_path,
                        offset: request.offset,
                        max_bytes: request.max_bytes,
                    },
                )
                .map_err(ToolError::from)?;
                Ok(WorkspaceReadResult {
                    content: result.content,
                    digest: result.file_digest,
                    file_byte_length: Some(result.file_byte_length),
                    eof: Some(result.eof),
                })
            }
        })
        .await
    }

    #[tool(
        name = "workspace.mutate",
        description = "Apply one atomic validated batch. mode must be exactly WRITE, APPEND, or REPLACE_EXACT; REPLACE_EXACT requires expectedText. expectedDigest is required when a target already exists and protects the complete file version.",
        annotations(
            title = "Mutate workspace files",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_mutate(
        &self,
        Parameters(request): Parameters<WorkspaceMutateRequest>,
    ) -> ToolOutcome<WorkspaceMutateResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("workspace.mutate", move || {
            runtime.mutate_workspace(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.diff",
        description = "Return a bounded compact Git diff and untracked paths for an isolated workspace.",
        annotations(
            title = "Inspect workspace diff",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_diff(
        &self,
        Parameters(request): Parameters<WorkspaceDiffRequest>,
    ) -> ToolOutcome<CompactWorkspaceDiffResult> {
        let config = self.state.executor.clone();
        self.run_core("workspace.diff", move || {
            workspace_diff_compact(
                &config,
                &ExecWorkspaceDiffRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id,
                    max_bytes: request.max_bytes,
                },
            )
            .map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.exec",
        description = "Run one command inside a workspace with the installed service user's trusted-local authority. execution.executable must be an absolute host path and execution.cwdRelative must be relative to the Workspace root. The server owns identity, concurrency, Job IDs, and Attempt IDs; duplicate clientRequestId values are idempotent.",
        execution(task_support = "optional"),
        annotations(
            title = "Execute transactional workspace job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_exec(
        &self,
        Parameters(request): Parameters<WorkspaceExecRequest>,
    ) -> ToolOutcome<TaskObservation> {
        let runtime = self.state.runtime.clone();
        let request = self.state.execution.bind(request);
        self.run_core("workspace.exec", move || {
            runtime.run_task(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "task.observe",
        description = "Observe or briefly await one Job. Omit offsets for tail mode, or pass stdoutOffset/stderrOffset with at least 4 tail bytes to read only new retained UTF-8 text and continue from returned next offsets.",
        annotations(
            title = "Observe transactional job",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_observe(
        &self,
        Parameters(request): Parameters<TaskObserveRequest>,
    ) -> ToolOutcome<TaskObservation> {
        let runtime = self.state.runtime.clone();
        self.run_core("task.observe", move || {
            runtime.observe_task(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "task.cancel",
        description = "Persist cancellation intent, stop the cgroup-owned process tree, and reconcile the Job.",
        annotations(
            title = "Cancel transactional job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_cancel(
        &self,
        Parameters(request): Parameters<TaskCancelRequest>,
    ) -> ToolOutcome<TaskObservation> {
        let runtime = self.state.runtime.clone();
        self.run_core("task.cancel", move || {
            runtime.cancel_task(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "task.list",
        description = "List newest Jobs first with semantic identity, Workspace, command summary, timestamps, duration, and Artifact count using a stable cursor.",
        annotations(
            title = "List transactional jobs",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_list(
        &self,
        Parameters(request): Parameters<RuntimeJobListRequest>,
    ) -> ToolOutcome<RuntimeJobListResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("task.list", move || {
            runtime.list_jobs(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "artifact.read",
        description = "Read a bounded verified range from one Job Artifact by Job and Artifact identity.",
        annotations(
            title = "Read transactional job artifact",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn artifact_read(
        &self,
        Parameters(request): Parameters<ArtifactReadRequest>,
    ) -> ToolOutcome<ArtifactReadResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("artifact.read", move || {
            runtime.read_artifact(&request).map_err(ToolError::from)
        })
        .await
    }
}
