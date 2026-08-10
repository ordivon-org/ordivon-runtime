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
| `release.apply` | Admit one exact Runtime self-release as a structured reconciliable effect. The caller chooses a Workspace commit and exact candidate-manifest digest; operator configuration owns source/install/Registry/environment/receipt authority. Exact clientRequestId replay is resolved before consulting current Workspace or candidate state. New admission commits a durable release effect and an Accepted Runtime Job, then returns without directly dispatching it; normal Runtime reconciliation owns later at-most-once execution. A connection loss during self-replacement is not failure evidence: reconnect with release.get using the same clientRequestId. The deployment receipt, not process exit alone, is authoritative for whether the external release effect committed. |
| `release.get` | Project one exact structured Runtime release effect by clientRequestId without reconciling, dispatching, retrying, or changing the external world. Joins durable Job/Attempt truth with the deterministic deployment receipt. Receipt truth is authoritative for deployed/not-committed/rolled-back release effects even when the generic execution channel was interrupted by Runtime self-replacement. Use this after any uncertain release.apply response; never infer release completion from transport loss or process status alone. |
| `runtime.describe` | Project the current Runtime node's execution affordances without reconciling, dispatching, selecting, or mutating work. Returns operator ceilings, allowed executable roots, named immutable-input authorities without host authority paths, and per-target configured/available state including current Runtime-owned provider identity. Windows authority availability is probed read-only; this projection never becomes admission authority, and new Jobs independently bind current provider truth at admission. |
| `task.cancel` | Persist cancellation intent, stop the cgroup-owned process tree, and reconcile the Job. |
| `task.get` | Read one exact durable Job as a projection-only Runtime inspection. This never reconciles, dispatches, cancels, or otherwise advances the Job. It returns bounded Attempt history, mechanical convergence, Artifact/episode summaries, and a bounded event timeline with event detail omitted; use artifact.read for retained stdout/stderr/results and task.observe only when targeted reconciliation or waiting is intended. |
| `task.list` | List newest Jobs first from the current durable Registry projection with request identity, Workspace, command summary, exact Attempt state, execution and delivery disposition, recovery requirement, timestamps, duration, and Artifact count using a stable cursor. Optionally filter by exact workspaceId, clientRequestId, or their intersection so a reconnecting caller can recover historical Jobs without scanning the global ledger. This call does not reconcile or dispatch Jobs; use task.observe for targeted reconciliation. Runtime never claims Task/domain semantic completion. |
| `task.observe` | Observe or briefly await one exact Job and reconcile that Job before projection. If the durable Job is still accepted with desiredState=run, this call may dispatch that already-committed execution intent; it never creates a new Job. Exact Attempt state, terminal execution disposition, delivery certainty, recovery requirement, result availability, and semanticCompletionEvaluated=false are projected explicitly. Omit offsets for tail mode, or pass stdoutOffset/stderrOffset with at least 4 tail bytes to read only new retained UTF-8 text and continue from returned next offsets. |
| `workspace.changes` | Page one exact Workspace change projection without unbounded path arrays. Entries expose atomic modified, added, deleted, and untracked changes in stable path/kind order; rename/copy interpretation stays on workspace.diff rather than adding similarity analysis to this large-change-set primitive. limit bounds entry count, maxBytes bounds encoded entry payload (not Git discovery I/O/RSS), totalEntries/remainingEntries expose traversal size, and nextCursor={changeSetDigest,afterPath,afterKind} fails closed if the projected change set changes. Use this for large change sets; workspace.diff remains the richer legacy convenience surface. |
| `workspace.close` | Close one Workspace. By default, reject tracked or untracked changes; force=true may remove dirty files. expectedSourceStateDigest compare-and-closes only the exact committed source state and remains replayable through the closed tombstone. closureDisposition distinguishes removed, already_closed, already_absent, and recovered_missing; removed only says whether this call performed physical removal. Active or held Jobs and open Workspaces whose Git authority lives under paths this close would remove block closure without reconciliation or dispatch. |
| `workspace.content` | Project one exact digest-bound Workspace image as native MCP image content. Runtime verifies the current file bytes against expectedDigest and validates PNG/JPEG signatures before transport; a changed file fails closed instead of silently changing the Agent's perceptual input. |
| `workspace.diff` | Return Git diff text with bounded Git stdout capture plus the complete legacy structured changed, modified, added, deleted, renamed, and untracked path projection. maxBytes bounds diff text only; the structured path arrays remain complete and are not response-bounded. Prefer workspace.changes for large or paged change sets. |
| `workspace.exec` | Run one effect-opaque command inside a workspace with the installed service user's trusted-local authority. execution.executable must be an absolute host path and execution.cwdRelative must be relative to the Workspace root. Duplicate clientRequestId admission is idempotent and current Git source state is bound. Omitted waitMs performs only a brief 2-second observation after durable admission; long work returns the same Job for task.observe rather than holding the MCP request open. Results expose exact Attempt state, execution and delivery disposition, recovery requirement, and explicitly do not claim semantic completion or external-effect idempotency. |
| `workspace.execBound` | Admit one contained-local execution with exact immutable inputs from operator-configured named authorities. Each input names only an authority, a relative object, an expected SHA-256 digest, and its presentation-relative path. Runtime resolves and copies those bytes only on new admission, freezes effective input commitments into the Job, presents them read-only under /run/ordivon/inputs, and replays an existing Job before consulting current authority state. Omitted waitMs performs only a brief 2-second observation after durable admission; long work returns the same Job for task.observe. This is physical execution evidence only and does not imply domain semantic completion. |
| `workspace.execPlan` | Run an ordered structured execution plan inside one Workspace. Steps use absolute executables and explicit args, run sequentially, stop on the first failure by default, and continue only when that step explicitly sets continueOnError. Omitted waitMs performs only a brief 2-second observation after durable admission; long work returns the same Job for task.observe. The Job exposes step progress plus exact Attempt state, execution and delivery disposition, and recovery requirement without asking the caller to infer them from output. |
| `workspace.get` | Return one Workspace's canonical sourceRepo, opening/base sourceRevision, exact currentHeadRevision, detached-head mode, dirty state, complete sourceStateDigest, creation time, and active Job identities. This is a projection-only read: it does not reconcile or dispatch Jobs. Use it after reconnecting or after an uncertain workspace.open instead of reconstructing Workspace identity or state from memory. |
| `workspace.list` | List newest healthy open Workspaces using stable cursor pagination, with canonical sourceRepo, opening/base sourceRevision, and exact currentHeadRevision. Exact sourceStateDigest is omitted by default and may be requested explicitly; workspace.get remains the precise proof boundary. This is a projection-only read and does not reconcile or dispatch Jobs. Inventory is derived from current physical Workspace candidates rather than historical closed tombstones; current inventory and page-local projection failures are isolated with a machine-readable stage, while authority-wide failures still fail closed. |
| `workspace.mutate` | Apply one atomic validated batch. mode must be exactly WRITE, APPEND, or REPLACE_EXACT; REPLACE_EXACT requires expectedText. expectedDigest is required when a target already exists and protects the complete file version. Active or held Jobs block mutation without being reconciled or dispatched by this call. This tool has no durable clientRequestId replay receipt: after an uncertain response, inspect current Workspace state before retrying. Prefer workspace.patch when response-loss reconciliation is required. |
| `workspace.open` | Resolve a local revision and create one detached Git Workspace. Omit workspaceId for a server-generated immutable ws-* handle; provide an explicit unique workspaceId when deterministic response-loss reconciliation matters. Repeating workspace.open is not an idempotent replay: after an uncertain response, use workspace.get on the explicit ID. This tool does not fetch remote refs. |
| `workspace.patch` | Apply one digest-guarded atomic text patch under a durable clientRequestId. Active or held Jobs block mutation without being reconciled or dispatched by this call. Exact replay returns the committed receipt; changed input conflicts; uncertain mixed outcomes require reconciliation. |
| `workspace.patch.get` | Reconcile one durable Workspace Patch receipt by exact clientRequestId without applying an uncommitted patch. This call may advance Runtime receipt state from prepared to committed or unknown after inspecting physical file state. |
| `workspace.read` | Read bounded UTF-8 content from an isolated workspace in FULL or SLICE mode. |

A Tool description is navigation, not a complete semantic contract. Runtime architecture, effect ambiguity, authority, compatibility, and recovery remain defined by the canonical documents and generated schemas.
