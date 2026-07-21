use super::*;

#[tool_router(vis = "pub(super)")]
impl M6Server {
    #[tool(
        name = "workspace.open",
        description = "Create one detached isolated Git workspace at an exact revision.",
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
    ) -> M6Outcome<CompactWorkspaceOpenResult> {
        if let Some(policy) = &self.state.dogfood_policy {
            if let Err(error) = policy.authorize(&request) {
                return M6Outcome::Error(M6McpError {
                    code: error.code,
                    message: error.message,
                    field: error.field,
                    retryable: error.retryable,
                });
            }
        }
        let config = self.state.executor.clone();
        self.run_core("workspace.open", move || {
            create_git_workspace_compact(&config, &request).map_err(M6McpError::from)
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
        Parameters(request): Parameters<M4WorkspaceReadRequest>,
    ) -> M6Outcome<M4WorkspaceReadResult> {
        let config = self.state.executor.clone();
        self.run_core("workspace.read", move || match request.mode {
            M4ReadMode::Full => {
                if request.offset != 0 {
                    return Err(M6McpError::invalid(
                        "offset must be zero in FULL mode",
                        "offset",
                    ));
                }
                let result = read_workspace_text_compact(
                    &config,
                    &WorkspaceReadRequest {
                        schema_version: request.schema_version,
                        workspace_id: request.workspace_id,
                        relative_path: request.relative_path,
                        max_bytes: request.max_bytes,
                    },
                )
                .map_err(M6McpError::from)?;
                Ok(M4WorkspaceReadResult {
                    content: result.content,
                    digest: result.digest,
                    file_byte_length: None,
                    eof: None,
                })
            }
            M4ReadMode::Slice => {
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
                .map_err(M6McpError::from)?;
                Ok(M4WorkspaceReadResult {
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
        description = "Apply one validated batch of WRITE, APPEND, or exact replacement mutations.",
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
    ) -> M6Outcome<WorkspaceMutateResult> {
        let config = self.state.executor.clone();
        self.run_core("workspace.mutate", move || {
            mutate_workspace(&config, &request).map_err(M6McpError::from)
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
        Parameters(request): Parameters<M4WorkspaceDiffRequest>,
    ) -> M6Outcome<CompactWorkspaceDiffResult> {
        let config = self.state.executor.clone();
        self.run_core("workspace.diff", move || {
            workspace_diff_compact(
                &config,
                &WorkspaceDiffRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id,
                    max_bytes: request.max_bytes,
                },
            )
            .map_err(M6McpError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.exec",
        description = "Submit one transactional Job. The server generates the durable Job and Attempt IDs; duplicate clientRequestId values are idempotent.",
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
        Parameters(request): Parameters<M6TaskRunRequest>,
    ) -> M6Outcome<M6TaskObservation> {
        let runtime = self.state.runtime.clone();
        self.run_core("workspace.exec", move || {
            runtime.run_task(&request).map_err(M6McpError::from)
        })
        .await
    }

    #[tool(
        name = "task.observe",
        description = "Observe or briefly await one transactional Job by server-generated Job ID.",
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
        Parameters(request): Parameters<M6TaskObserveRequest>,
    ) -> M6Outcome<M6TaskObservation> {
        let runtime = self.state.runtime.clone();
        self.run_core("task.observe", move || {
            runtime.observe_task(&request).map_err(M6McpError::from)
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
        Parameters(request): Parameters<M6TaskCancelRequest>,
    ) -> M6Outcome<M6TaskObservation> {
        let runtime = self.state.runtime.clone();
        self.run_core("task.cancel", move || {
            runtime.cancel_task(&request).map_err(M6McpError::from)
        })
        .await
    }

    #[tool(
        name = "task.list",
        description = "List a bounded page of transactional Jobs using a stable database cursor.",
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
        Parameters(request): Parameters<JobListRequestM6>,
    ) -> M6Outcome<JobListResultM6> {
        let runtime = self.state.runtime.clone();
        self.run_core("task.list", move || {
            runtime.list_jobs(&request).map_err(M6McpError::from)
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
        Parameters(request): Parameters<M6ArtifactReadRequest>,
    ) -> M6Outcome<M6ArtifactReadResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("artifact.read", move || {
            runtime.read_artifact(&request).map_err(M6McpError::from)
        })
        .await
    }
}
