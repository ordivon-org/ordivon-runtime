---
schema_version: 1
id: runtime.tools
title: Runtime Tool Reference
type: reference
profile: provider
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
evidence_status: generated
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.quickstart
  - runtime.model
---
# Runtime Tool Reference

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
| `task.list` | List newest Jobs first with semantic identity, Workspace, command summary, timestamps, duration, and Artifact count using a stable cursor. Optionally filter by exact clientRequestId. |
| `task.observe` | Observe or briefly await one Job. Omit offsets for tail mode, or pass stdoutOffset/stderrOffset with at least 4 tail bytes to read only new retained UTF-8 text and continue from returned next offsets. |
| `workspace.close` | Close one Workspace. By default, reject tracked or untracked changes; force=true may remove dirty files. expectedSourceStateDigest compare-and-closes only the exact committed source state and remains replayable through the closed tombstone. Active or held Jobs always block closure. |
| `workspace.diff` | Return a bounded Git diff plus structured changed, modified, added, deleted, renamed, and untracked paths for an isolated Workspace. |
| `workspace.exec` | Run one effect-opaque command inside a workspace with the installed service user's trusted-local authority. execution.executable must be an absolute host path and execution.cwdRelative must be relative to the Workspace root. The server makes duplicate clientRequestId admission idempotent and binds current Git source state, but it does not claim the command's external effects are idempotent. |
| `workspace.execPlan` | Run an ordered structured execution plan inside one Workspace. Steps use absolute executables and explicit args, run sequentially, stop on the first failure by default, and continue only when that step explicitly sets continueOnError. The Job exposes current and failed step progress without parsing shell text. |
| `workspace.get` | Return one Workspace's exact source commit, detached-head mode, dirty state, complete sourceStateDigest, creation time, and active Job identities. Use this after reconnecting instead of reconstructing Workspace state from memory. |
| `workspace.list` | List newest healthy open Workspaces using a lightweight Git status probe. Exact sourceStateDigest is omitted by default and may be requested explicitly; workspace.get remains the precise proof boundary. Missing historical records are omitted and unusable Workspaces are isolated in issues. |
| `workspace.mutate` | Apply one atomic validated batch. mode must be exactly WRITE, APPEND, or REPLACE_EXACT; REPLACE_EXACT requires expectedText. expectedDigest is required when a target already exists and protects the complete file version. |
| `workspace.open` | Resolve a local revision and create one detached Git Workspace. Omit workspaceId to receive a server-generated immutable ws-* handle; explicit IDs remain supported for compatibility. This tool does not fetch remote refs. |
| `workspace.patch` | Apply one digest-guarded atomic text patch under a durable clientRequestId. Exact replay returns the committed receipt; changed input conflicts; uncertain mixed outcomes require reconciliation. |
| `workspace.patch.get` | Read and reconcile one durable Workspace Patch receipt by exact clientRequestId without applying an uncommitted patch. |
| `workspace.read` | Read bounded UTF-8 content from an isolated workspace in FULL or SLICE mode. |

A Tool description is navigation, not a complete semantic contract. Runtime architecture, effect ambiguity, authority, compatibility, and recovery remain defined by the canonical documents and generated schemas.
