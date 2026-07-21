use super::*;

#[tool_router(vis = "pub(super)")]
impl M4Server {
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
    ) -> M4Outcome<CompactWorkspaceOpenResult> {
        if let Some(policy) = &self.state.config.dogfood_policy {
            if let Err(error) = policy.authorize(&request) {
                return M4Outcome::Error(error);
            }
        }
        let config = self.state.config.executor.clone();
        self.run_core("workspace.open", move || {
            create_git_workspace_compact(&config, &request).map_err(M4Error::from)
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
    ) -> M4Outcome<M4WorkspaceReadResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.read", move || match request.mode {
            M4ReadMode::Full => {
                if request.offset != 0 {
                    return Err(M4Error::invalid(
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
                .map_err(M4Error::from)?;
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
                .map_err(M4Error::from)?;
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
    ) -> M4Outcome<WorkspaceMutateResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.mutate", move || {
            mutate_workspace(&config, &request).map_err(M4Error::from)
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
    ) -> M4Outcome<CompactWorkspaceDiffResult> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.diff", move || {
            workspace_diff_compact(
                &config,
                &WorkspaceDiffRequest {
                    schema_version: request.schema_version,
                    workspace_id: request.workspace_id,
                    max_bytes: request.max_bytes,
                },
            )
            .map_err(M4Error::from)
        })
        .await
    }
    #[tool(
        name = "workspace.exec",
        description = "Run an absolute executable plus argv as a durable sandboxed Task. Small results return compactly; long work remains recoverable by Task ID.",
        execution(task_support = "optional"),
        annotations(
            title = "Execute durable workspace task",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_exec(
        &self,
        Parameters(request): Parameters<TaskRunRequest>,
    ) -> M4Outcome<CompactTaskObservation> {
        let config = self.state.config.executor.clone();
        self.run_core("workspace.exec", move || {
            run_universal_task_compact(&config, &request).map_err(M4Error::from)
        })
        .await
    }

    #[tool(
        name = "task.observe",
        description = "Observe or briefly await one durable Ordivon Task by Task ID.",
        annotations(
            title = "Observe durable task",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_observe(
        &self,
        Parameters(request): Parameters<TaskAwaitRequest>,
    ) -> M4Outcome<CompactTaskObservation> {
        let config = self.state.config.executor.clone();
        self.run_core("task.observe", move || {
            await_universal_task_compact(&config, &request).map_err(M4Error::from)
        })
        .await
    }
    #[tool(
        name = "task.cancel",
        description = "Cancel one durable Ordivon Task and stop its entire cgroup-owned process tree.",
        annotations(
            title = "Cancel durable task",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn task_cancel(
        &self,
        Parameters(request): Parameters<TaskCancelRequest>,
    ) -> M4Outcome<MigrationTaskHandle> {
        let config = self.state.config.executor.clone();
        self.run_core("task.cancel", move || {
            cancel_universal_task(&config, &request).map_err(M4Error::from)
        })
        .await
    }

    #[tool(
        name = "artifact.read",
        description = "Read a bounded UTF-8 range from stdout, stderr, or result Artifacts by stable Task and Artifact IDs.",
        annotations(
            title = "Read task artifact",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn artifact_read(
        &self,
        Parameters(request): Parameters<ArtifactReadRequest>,
    ) -> M4Outcome<ArtifactReadResult> {
        let config = self.state.config.executor.clone();
        self.run_core("artifact.read", move || {
            read_task_artifact(&config, &request).map_err(M4Error::from)
        })
        .await
    }
}
