---
schema_version: 1
id: runtime.tools
title: Runtime Tool Reference
type: reference
profile: engineering
lifecycle: active
source_role: generated
visibility: public
owners:
  - ordivon-runtime
audience:
  - user
  - builder
  - agent
updated: 2026-08-04
summary: Generated names and descriptions for the current public Runtime MCP Tool surface.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.quickstart
  - runtime.model
---
# Runtime Tool Reference

## Scope

This reference lists the current generated public MCP Tool names and descriptions. Exact request and result schemas remain machine-owned by discovery.

## Contract

The checked-in table must be reproducible from the Tool registry and must change whenever the public generated catalog changes.

## Errors

Invalid Tool names, arguments, versions, identities, or bounded-output requests fail through the generated schemas and Runtime error contracts; this page does not redefine them.

## Compatibility

Compatibility is bound by protocol lifecycle, Runtime schema, and Tool catalog digest. See the canonical Runtime model and release policy for migration obligations.

This file is generated from `crates/ordivon-runtime-mcp/src/server/tools.rs`. Do not edit it by hand. Exact argument and result schemas remain machine-owned by `server/discover` and `tools/list`.

Regenerate and verify:

```bash
python3 scripts/check_docs.py --write
python3 scripts/check_docs.py
```

| Tool | Current description |
| --- | --- |
| `artifact.read` | Read a bounded verified range from one Job Artifact by Job and Artifact identity. |
| `task.cancel` | Persist cancellation intent, stop the cgroup-owned process tree, and reconcile the Job. |
| `task.list` | List newest Jobs first with request identity, Workspace, command summary, exact Attempt state, execution and delivery disposition, recovery requirement, timestamps, duration, and Artifact count using a stable cursor. Optionally filter by exact clientRequestId; Runtime never claims Task/domain semantic completion. |
| `task.observe` | Observe or briefly await one Job. Exact Attempt state, terminal execution disposition, delivery certainty, recovery requirement, result availability, and semanticCompletionEvaluated=false are projected explicitly. Omit offsets for tail mode, or pass stdoutOffset/stderrOffset with at least 4 tail bytes to read only new retained UTF-8 text and continue from returned next offsets. |
| `workspace.close` | Close one Workspace. By default, reject tracked or untracked changes; force=true may remove dirty files. expectedSourceStateDigest compare-and-closes only the exact committed source state and remains replayable through the closed tombstone. closureDisposition distinguishes removed, already_closed, already_absent, and recovered_missing; removed only says whether this call performed physical removal. Active or held Jobs always block closure. |
| `workspace.diff` | Return a bounded Git diff plus structured changed, modified, added, deleted, renamed, and untracked paths for an isolated Workspace. |
| `workspace.exec` | Run one effect-opaque command inside a workspace with the installed service user's trusted-local authority. execution.executable must be an absolute host path and execution.cwdRelative must be relative to the Workspace root. Duplicate clientRequestId admission is idempotent and current Git source state is bound. Results expose exact Attempt state, execution and delivery disposition, recovery requirement, and explicitly do not claim semantic completion or external-effect idempotency. |
| `workspace.execPlan` | Run an ordered structured execution plan inside one Workspace. Steps use absolute executables and explicit args, run sequentially, stop on the first failure by default, and continue only when that step explicitly sets continueOnError. The Job exposes step progress plus exact Attempt state, execution and delivery disposition, and recovery requirement without asking the caller to infer them from output. |
| `workspace.get` | Return one Workspace's canonical sourceRepo, exact source commit, detached-head mode, dirty state, complete sourceStateDigest, creation time, and active Job identities. Use this after reconnecting or after an uncertain workspace.open instead of reconstructing Workspace identity or state from memory. |
| `workspace.list` | List newest healthy open Workspaces with canonical sourceRepo and exact source revision. Exact sourceStateDigest is omitted by default and may be requested explicitly; workspace.get remains the precise proof boundary. Missing historical records are omitted; Workspace-local inventory/reconciliation/projection failures are isolated in issues with a machine-readable stage, while authority-wide failures still fail closed. |
| `workspace.mutate` | Apply one atomic validated batch. mode must be exactly WRITE, APPEND, or REPLACE_EXACT; REPLACE_EXACT requires expectedText. expectedDigest is required when a target already exists and protects the complete file version. This tool has no durable clientRequestId replay receipt: after an uncertain response, inspect current Workspace state before retrying. Prefer workspace.patch when response-loss reconciliation is required. |
| `workspace.open` | Resolve a local revision and create one detached Git Workspace. Omit workspaceId for a server-generated immutable ws-* handle; provide an explicit unique workspaceId when deterministic response-loss reconciliation matters. Repeating workspace.open is not an idempotent replay: after an uncertain response, use workspace.get on the explicit ID. This tool does not fetch remote refs. |
| `workspace.patch` | Apply one digest-guarded atomic text patch under a durable clientRequestId. Exact replay returns the committed receipt; changed input conflicts; uncertain mixed outcomes require reconciliation. |
| `workspace.patch.get` | Read and reconcile one durable Workspace Patch receipt by exact clientRequestId without applying an uncommitted patch. |
| `workspace.read` | Read bounded UTF-8 content from an isolated workspace in FULL or SLICE mode. |

A Tool description is navigation, not a complete semantic contract. Runtime architecture, effect ambiguity, authority, compatibility, and recovery remain defined by the canonical documents and generated schemas.
