# Agent-facing execution UX

Ordivon Runtime keeps a small universal execution surface in the core and leaves product-specific workflows in Host or external developer tools.

## Core primitives

- `workspace.mutate`: one digest-guarded atomic batch for writes, appends, and exact replacements. Existing targets require the complete-file digest.
- `workspace.execPlan`: ordered native process steps. Steps stop on the first failure by default and continue only with explicit `continueOnError`. No shell parsing or injected `set -e` is involved.
- `task.observe`: projects elapsed time, retained output byte counts, last output time, progress revision, completed and total steps, current step, and first failed step. `waitUntil=change_or_terminal` waits until output, progress, or terminal state changes.
- typed operation errors: MCP errors carry `origin`, `retryClass`, `commitState`, `traceId`, and, when known, `operationId`. `commitState=unknown` requires reconciliation rather than a new operation identity.

## Workspace recovery

`workspace.open` accepts a caller-provided compatibility ID or generates an immutable `ws-*` handle when `workspaceId` is omitted. Use an explicit ID when response-loss replay identity matters; generated handles are a low-friction convenience for interactive opens. `workspace.list` and `workspace.get` are the reconnection entry points and expose exact source revision, detached-head mode, dirty state, creation time, and active Jobs.

Compiler-report normalization, Git publication, GitHub orchestration, coding-agent evaluation, and benchmark harnesses are not Runtime state owners. They remain outside the Runtime repository unless a concrete execution contract requires them.
