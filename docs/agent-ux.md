# Agent-facing execution UX

Ordivon Runtime keeps a small universal execution surface in the core and leaves product-specific workflows in Host or external developer tools.

## Core primitives

- `workspace.mutate`: one digest-guarded atomic batch for writes, appends, and exact replacements. Existing targets require the complete-file digest. It remains a low-level synchronous mutation without durable replay identity.
- `workspace.patch`: one multi-file, multi-hunk text patch under a stable `clientRequestId`. Every existing file binds its complete before digest; every edit binds an exact one-based line, zero-based Unicode-column range and `expectedText`. Exact replay returns the committed receipt instead of writing again.
- `workspace.patch.get`: read and reconcile one Patch receipt. A prepared record whose files all match their after digests is committed without redispatch; a mixed or externally changed state becomes `unknown` and requires manual reconciliation.
- `workspace.execPlan`: ordered native process steps. Steps stop on the first failure by default and continue only with explicit `continueOnError`. No shell parsing or injected `set -e` is involved.
- `foreignReferences`: bounded opaque correlation inputs on execution requests. A Host Harness path uses namespace `ordivon.host` and exact `task`, `task_attempt`, `assignment`, and `harness_run` identities. They participate in request identity and terminal evidence but do not transfer Host ownership into Runtime.
- `task.observe`: projects elapsed time, retained output byte counts, last output time, progress revision, completed and total steps, current step, and first failed step. `waitUntil=change_or_terminal` waits until output, progress, or terminal state changes.
- typed operation errors: MCP errors carry `origin`, `retryClass`, `commitState`, `traceId`, and, when known, `operationId`. `commitState=unknown` requires reconciliation rather than a new operation identity.

## Correlated Patch recovery

A durable Patch records its Intent and before/after file digest plan before physical mutation. On response loss, reissue the exact same `workspace.patch` request or call `workspace.patch.get` with the same `clientRequestId`. Runtime applies the Patch only when every file still matches the complete before state. If every file already matches the after state, Runtime commits the missing receipt. Any mixed state is isolated as `unknown`; Runtime never guesses whether a partial external write should be completed or rolled back.

Patch receipt storage is an additive table outside the Registry migration version. Previous Runtime binaries ignore it, preserving receipt-bound rollback compatibility; the current binary recreates the table and index idempotently when absent.

## Correlated Job recovery

Reissue an exact committed request only with the same `clientRequestId`, execution request, and foreign references; Runtime then returns the original Job. Do not reuse that identity after changing a Host Assignment generation or digest. When delivery may have committed, filter `task.list` by the exact `clientRequestId`, require one consistent Job identity, then inspect `task.observe` and the terminal-evidence Artifact rather than dispatching new work.

A successful Runtime Job or Runtime Attempt is not a Host completion decision. Required Artifacts, unresolved Effects or `UNKNOWN` state, and semantic acceptance remain above Runtime.

## Workspace recovery

`workspace.open` accepts a caller-provided compatibility ID or generates an immutable `ws-*` handle when `workspaceId` is omitted. Use an explicit ID when response-loss replay identity matters; generated handles are a low-friction convenience for interactive opens. `workspace.list` and `workspace.get` are the reconnection entry points and expose exact source revision, detached-head mode, dirty state, complete `sourceStateDigest`, creation time, and active Jobs. A caller that verifies one exact source state can pass that digest to `workspace.close.expectedSourceStateDigest`; Runtime compares it under the Workspace lifecycle lock, persists it in the closed tombstone, and permits exact replay after response loss. `workspace.diff` returns the bounded patch together with sorted `changedPaths`, `modifiedPaths`, `addedPaths`, `deletedPaths`, structured `renamedPaths`, and `untrackedPaths`; callers do not need to infer the exact changed set from patch headers.

Compiler-report normalization, Git publication, GitHub orchestration, coding-agent evaluation, and benchmark harnesses are not Runtime state owners. They remain outside the Runtime repository unless a concrete execution contract requires them.
