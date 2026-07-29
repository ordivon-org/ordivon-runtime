use super::*;

#[tool_router(vis = "pub(super)")]
impl RuntimeServer {
    #[tool(
        name = "workspace.open",
        description = "Resolve a local revision and create one detached Git Workspace. Omit workspaceId to receive a server-generated immutable ws-* handle; explicit IDs remain supported for compatibility. This tool does not fetch remote refs.",
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
        Parameters(request): Parameters<WorkspaceOpenRequest>,
    ) -> ToolOutcome<CompactWorkspaceOpenResult> {
        let runtime = self.state.runtime.clone();
        let request = request.bind();
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
        name = "workspace.get",
        description = "Return one Workspace's exact source commit, detached-head mode, dirty state, creation time, and active Job identities. Use this after reconnecting instead of reconstructing Workspace state from memory.",
        annotations(
            title = "Get workspace state",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_get(
        &self,
        Parameters(request): Parameters<RuntimeWorkspaceGetRequest>,
    ) -> ToolOutcome<RuntimeWorkspaceSummary> {
        let runtime = self.state.runtime.clone();
        self.run_core("workspace.get", move || {
            runtime.get_workspace(&request).map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.list",
        description = "List newest healthy open Workspaces with exact source commits, dirty state, detached-head mode, and active Jobs. Missing historical records are omitted; existing but unusable Workspaces are isolated in issues instead of failing the whole recovery list.",
        annotations(
            title = "List open workspaces",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_list(
        &self,
        Parameters(request): Parameters<RuntimeWorkspaceListRequest>,
    ) -> ToolOutcome<RuntimeWorkspaceListResult> {
        let runtime = self.state.runtime.clone();
        self.run_core("workspace.list", move || {
            runtime.list_workspaces(&request).map_err(ToolError::from)
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
        description = "Run one effect-opaque command inside a workspace with the installed service user's trusted-local authority. execution.executable must be an absolute host path and execution.cwdRelative must be relative to the Workspace root. The server makes duplicate clientRequestId admission idempotent and binds current Git source state, but it does not claim the command's external effects are idempotent.",
        annotations(
            title = "Execute transactional workspace job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
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
        name = "workspace.execPlan",
        description = "Run an ordered structured execution plan inside one Workspace. Steps use absolute executables and explicit args, run sequentially, stop on the first failure by default, and continue only when that step explicitly sets continueOnError. The Job exposes current and failed step progress without parsing shell text.",
        annotations(
            title = "Execute fail-fast workspace plan",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_exec_plan(
        &self,
        Parameters(request): Parameters<WorkspaceExecPlanRequest>,
    ) -> ToolOutcome<TaskObservation> {
        let runtime = self.state.runtime.clone();
        let request = match self.state.execution.bind_plan(request) {
            Ok(request) => request,
            Err(error) => return ToolOutcome::Error(error),
        };
        self.run_core("workspace.execPlan", move || {
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
        description = "List newest Jobs first with semantic identity, Workspace, command summary, timestamps, duration, and Artifact count using a stable cursor. Optionally filter by exact clientRequestId.",
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
