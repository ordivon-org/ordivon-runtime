use super::*;

#[tool_router(vis = "pub(super)")]
impl RuntimeServer {
    #[tool(
        name = "workspace.open",
        description = "Resolve a local revision and create one detached Git Workspace. Omit workspaceId for a server-generated immutable ws-* handle; provide an explicit unique workspaceId when deterministic response-loss reconciliation matters. Repeating workspace.open is not an idempotent replay: after an uncertain response, use workspace.get on the explicit ID. This tool does not fetch remote refs.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<CompactWorkspaceOpenResult>>(),
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
        description = "Close one Workspace. By default, reject tracked or untracked changes; force=true may remove dirty files. expectedSourceStateDigest compare-and-closes only the exact committed source state and remains replayable through the closed tombstone. closureDisposition distinguishes removed, already_closed, already_absent, and recovered_missing; removed only says whether this call performed physical removal. Active or held Jobs block closure without being reconciled or dispatched by this call.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<WorkspaceCloseResult>>(),
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
        description = "Return one Workspace's canonical sourceRepo, opening/base sourceRevision, exact currentHeadRevision, detached-head mode, dirty state, complete sourceStateDigest, creation time, and active Job identities. This is a projection-only read: it does not reconcile or dispatch Jobs. Use it after reconnecting or after an uncertain workspace.open instead of reconstructing Workspace identity or state from memory.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<RuntimeWorkspaceSummary>>(),
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
        description = "List newest healthy open Workspaces using stable cursor pagination, with canonical sourceRepo, opening/base sourceRevision, and exact currentHeadRevision. Exact sourceStateDigest is omitted by default and may be requested explicitly; workspace.get remains the precise proof boundary. This is a projection-only read and does not reconcile or dispatch Jobs. Missing historical records are omitted; inventory issues are returned as global diagnostics and page-local projection failures are isolated with a machine-readable stage, while authority-wide failures still fail closed.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<RuntimeWorkspaceListResult>>(),
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
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<WorkspaceReadResult>>(),
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
        description = "Apply one atomic validated batch. mode must be exactly WRITE, APPEND, or REPLACE_EXACT; REPLACE_EXACT requires expectedText. expectedDigest is required when a target already exists and protects the complete file version. Active or held Jobs block mutation without being reconciled or dispatched by this call. This tool has no durable clientRequestId replay receipt: after an uncertain response, inspect current Workspace state before retrying. Prefer workspace.patch when response-loss reconciliation is required.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<WorkspaceMutateResult>>(),
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
        name = "workspace.patch",
        description = "Apply one digest-guarded atomic text patch under a durable clientRequestId. Active or held Jobs block mutation without being reconciled or dispatched by this call. Exact replay returns the committed receipt; changed input conflicts; uncertain mixed outcomes require reconciliation.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<DurableWorkspacePatchResult>>(),
        annotations(
            title = "Apply durable workspace patch",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_patch(
        &self,
        Parameters(request): Parameters<WorkspacePatchToolRequest>,
    ) -> ToolOutcome<DurableWorkspacePatchResult> {
        let runtime = self.state.runtime.clone();
        let request = self.state.execution.bind_patch(request);
        self.run_core("workspace.patch", move || {
            runtime
                .patch_workspace_durable(&request)
                .map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.patch.get",
        description = "Reconcile one durable Workspace Patch receipt by exact clientRequestId without applying an uncommitted patch. This call may advance Runtime receipt state from prepared to committed or unknown after inspecting physical file state.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<WorkspacePatchOperationStatus>>(),
        annotations(
            title = "Inspect durable workspace patch",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_patch_get(
        &self,
        Parameters(request): Parameters<WorkspacePatchStatusToolRequest>,
    ) -> ToolOutcome<WorkspacePatchOperationStatus> {
        let runtime = self.state.runtime.clone();
        let request = self.state.execution.bind_patch_status(request);
        self.run_core("workspace.patch.get", move || {
            runtime
                .workspace_patch_status(&request)
                .map_err(ToolError::from)
        })
        .await
    }

    #[tool(
        name = "workspace.diff",
        description = "Return a bounded Git diff plus structured changed, modified, added, deleted, renamed, and untracked paths for an isolated Workspace.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<CompactWorkspaceDiffResult>>(),
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
        description = "Run one effect-opaque command inside a workspace with the installed service user's trusted-local authority. execution.executable must be an absolute host path and execution.cwdRelative must be relative to the Workspace root. Duplicate clientRequestId admission is idempotent and current Git source state is bound. Results expose exact Attempt state, execution and delivery disposition, recovery requirement, and explicitly do not claim semantic completion or external-effect idempotency.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<TaskObservation>>(),
        annotations(
            title = "Execute transactional workspace job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
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
        description = "Run an ordered structured execution plan inside one Workspace. Steps use absolute executables and explicit args, run sequentially, stop on the first failure by default, and continue only when that step explicitly sets continueOnError. The Job exposes step progress plus exact Attempt state, execution and delivery disposition, and recovery requirement without asking the caller to infer them from output.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<TaskObservation>>(),
        annotations(
            title = "Execute fail-fast workspace plan",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
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
        description = "Observe or briefly await one exact Job and reconcile that Job before projection. If the durable Job is still accepted with desiredState=run, this call may dispatch that already-committed execution intent; it never creates a new Job. Exact Attempt state, terminal execution disposition, delivery certainty, recovery requirement, result availability, and semanticCompletionEvaluated=false are projected explicitly. Omit offsets for tail mode, or pass stdoutOffset/stderrOffset with at least 4 tail bytes to read only new retained UTF-8 text and continue from returned next offsets.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<TaskObservation>>(),
        annotations(
            title = "Observe transactional job",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
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
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<TaskObservation>>(),
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
        description = "List newest Jobs first from the current durable Registry projection with request identity, Workspace, command summary, exact Attempt state, execution and delivery disposition, recovery requirement, timestamps, duration, and Artifact count using a stable cursor. Optionally filter by exact workspaceId, clientRequestId, or their intersection so a reconnecting caller can recover historical Jobs without scanning the global ledger. This call does not reconcile or dispatch Jobs; use task.observe for targeted reconciliation. Runtime never claims Task/domain semantic completion.",
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<RuntimeJobListResult>>(),
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
        output_schema = rmcp::handler::server::tool::schema_for_output::<ToolOutcome<ArtifactReadResult>>(),
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
