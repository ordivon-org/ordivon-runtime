# Agent-facing execution UX

Ordivon Runtime keeps a small universal execution surface in the core and leaves product-specific workflows in Host or external developer tools.

## Core primitives

- `workspace.mutate`: one digest-guarded atomic batch for writes, appends, and exact replacements. Existing targets require the complete-file digest.
- `workspace.execPlan`: ordered native process steps. Steps stop on the first failure by default and continue only with explicit `continueOnError`. No shell parsing or injected `set -e` is involved.
- `foreignReferences`: bounded opaque correlation inputs on execution requests. A Host Harness path uses namespace `ordivon.host` and exact `task`, `task_attempt`, `assignment`, and `harness_run` identities. They participate in request identity and terminal evidence but do not transfer Host ownership into Runtime.
- `task.observe`: projects elapsed time, retained output byte counts, last output time, progress revision, completed and total steps, current step, and first failed step. `waitUntil=change_or_terminal` waits until output, progress, or terminal state changes.
- typed operation errors: MCP errors carry `origin`, `retryClass`, `commitState`, `traceId`, and, when known, `operationId`. `commitState=unknown` requires reconciliation rather than a new operation identity.

## Correlated Job recovery

Reissue an exact committed request only with the same `clientRequestId`, execution request, and foreign references; Runtime then returns the original Job. Do not reuse that identity after changing a Host Assignment generation or digest. When delivery may have committed, filter `task.list` by the exact `clientRequestId`, require one consistent Job identity, then inspect `task.observe` and the terminal-evidence Artifact rather than dispatching new work.

A successful Runtime Job or Runtime Attempt is not a Host completion decision. Required Artifacts, unresolved Effects or `UNKNOWN` state, and semantic acceptance remain above Runtime.

## Workspace recovery

`workspace.open` accepts a caller-provided compatibility ID or generates an immutable `ws-*` handle when `workspaceId` is omitted. Use an explicit ID when response-loss replay identity matters; generated handles are a low-friction convenience for interactive opens. `workspace.list` and `workspace.get` are the reconnection entry points and expose exact source revision, detached-head mode, dirty state, complete `sourceStateDigest`, creation time, and active Jobs. A caller that verifies one exact source state can pass that digest to `workspace.close.expectedSourceStateDigest`; Runtime compares it under the Workspace lifecycle lock, persists it in the closed tombstone, and permits exact replay after response loss.

Compiler-report normalization, Git publication, GitHub orchestration, coding-agent evaluation, and benchmark harnesses are not Runtime state owners. They remain outside the Runtime repository unless a concrete execution contract requires them.
