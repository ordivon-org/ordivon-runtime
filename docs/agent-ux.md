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

## Machine-discoverable Tool results

All public Runtime MCP Tools publish an `outputSchema` for their `structuredContent`. The schema is a two-branch contract: one branch is the Tool's exact success result and the other is the standard `{ "error": ToolError }` envelope. A caller can therefore discover execution fields, Workspace closure state, Artifact metadata, and error-control semantics before the first Tool call instead of learning result shape from trial execution.

The error envelope keeps `code` as the stable machine reason while constraining the control vocabulary that changes safe Agent behavior: `origin` is one of `mcp_adapter`, `runtime_core`, or `workspace_executor`; `retryClass` is one of `never`, `safe_same_request`, or `reconcile_first`; and `commitState` is one of `not_started`, `not_committed`, or `unknown`. The wire values are unchanged from the pre-schema envelope. `toolCatalogDigest` binds these generated output schemas together with Tool names, descriptions, annotations, and input schemas, so a discovery refresh can detect an exact contract change without creating another Runtime authority.

## Correlated Patch recovery

A durable Patch records its Intent and before/after file digest plan before physical mutation. On response loss, reissue the exact same `workspace.patch` request or call `workspace.patch.get` with the same `clientRequestId`. Runtime applies the Patch only when every file still matches the complete before state. If every file already matches the after state, Runtime commits the missing receipt. Any mixed state is isolated as `unknown`; Runtime never guesses whether a partial external write should be completed or rolled back.

Patch receipt storage is an additive table outside the Registry migration version. Previous Runtime binaries ignore it, preserving receipt-bound rollback compatibility; the current binary recreates the table and index idempotently when absent.

## Correlated Job recovery

Reissue an exact committed request only with the same `clientRequestId`, execution request, and foreign references; Runtime then returns the original Job. Do not reuse that identity after changing a Host Assignment generation or digest. When delivery may have committed, filter `task.list` by the exact `clientRequestId`, require one consistent Job identity, then inspect `task.observe` and the terminal-evidence Artifact rather than dispatching new work.

A successful Runtime Job or Runtime Attempt is not a Host completion decision. Required Artifacts, unresolved Effects or `UNKNOWN` state, and semantic acceptance remain above Runtime.

## Workspace recovery

`workspace.open` accepts a caller-provided ID or generates an immutable `ws-*` handle when `workspaceId` is omitted. Use an explicit unique ID when deterministic response-loss reconciliation matters; `workspace.open` itself is not an idempotent replay operation, so an uncertain response should be reconciled with `workspace.get` rather than blindly repeated. Generated handles are a low-friction convenience when response-loss identity is not required. `workspace.list` and `workspace.get` are the reconnection entry points and expose canonical `sourceRepo`, exact source revision, detached-head mode, dirty state, complete `sourceStateDigest`, creation time, and active Jobs. A caller that verifies one exact source state can pass that digest to `workspace.close.expectedSourceStateDigest`; Runtime compares it under the Workspace lifecycle lock, persists it in the closed tombstone, and permits exact replay after response loss. `workspace.diff` returns the bounded patch together with sorted `changedPaths`, `modifiedPaths`, `addedPaths`, `deletedPaths`, structured `renamedPaths`, and `untrackedPaths`; callers do not need to infer the exact changed set from patch headers.

Compiler-report normalization, Git publication, GitHub orchestration, coding-agent evaluation, and benchmark harnesses are not Runtime state owners. They remain outside the Runtime repository unless a concrete execution contract requires them.

## Execution semantic projection

Runtime preserves state distinctions that change a caller's safe next action. `status` remains a coarse compatibility summary (`queued`, `working`, or a terminal resolution) and must not be used as the sole control signal. `workspace.exec`, `workspace.execPlan`, `task.observe`, and `task.list` project the following explicit execution semantics:

- `desiredState` preserves the persisted Job intent (`run` or `cancelled`) across reconnects;
- `attemptState` is the exact current or latest Runtime Attempt state, including `starting`, `stopping`, and `recovering`;
- `terminationIntent` distinguishes natural execution from persisted stop or deadline intent;
- `executionTerminal` says only whether the Runtime Job has a terminal physical execution resolution;
- `executionDisposition` is absent while unresolved and otherwise carries the terminal Runtime resolution such as `succeeded`, `failed`, `timed_out`, `cancelled`, `lost`, or `orphaned`;
- `executionReasonCode` carries the stable machine reason for that terminal resolution when the append-only event history records one;
- `deliveryDisposition` separates `in_progress`, `committed`, `reconciliation_required`, and `unknown` result certainty;
- `recoveryRequired` is a derived Runtime fact: it is true for active recovery conditions and recovery-bearing states such as `recovering` or `orphaned`, rather than exposing an internal condition-table implementation detail;
- `resultAvailable` means a durable Runtime terminal result exists, not that the result is semantically correct;
- `artifactsAvailable` means at least one registered Runtime Artifact actually exists; it is not inferred from the presence of a result digest;
- `semanticCompletionEvaluated` is always `false`. Runtime does not decide Task, Goal, Assignment, or domain completion.

`resultAvailable=true` is therefore compatible with `deliveryDisposition=reconciliation_required` or `deliveryDisposition=unknown`. A caller must not translate terminality into certainty, certainty into semantic correctness, or `lost` into permission to redispatch an effect-opaque operation. `pollAfterMs` is emitted for `in_progress` and `reconciliation_required`; a terminal `unknown` disposition intentionally carries no implication that repeated polling will make the external effect safe to repeat.

The projection rule is: **if two Runtime-owned states imply different safe Agent behavior, the Agent-facing contract must preserve that distinction through a machine-readable field.** Runtime may spend additional bounded read work to preserve this semantic certainty; later mechanical optimization must not collapse it again. `task.list` and single-Job projections construct their multi-field semantic view from one bounded Registry read snapshot, so a caller does not receive a state vector assembled across different concurrent moments.

Workspace mutation follows the same rule. `workspace.mutate` is atomic and digest-guarded but does not own a durable request identity or replay receipt; if its response is uncertain, the caller must re-read/reconcile the affected Workspace before deciding whether to retry. `workspace.patch` is the preferred text-mutation primitive when response-loss safety matters because `clientRequestId` and `workspace.patch.get` make committed, uncommitted, and unknown outcomes distinguishable without guessing.

Workspace closure also separates state from action. `WorkspaceCloseResult.removed` is a compatibility fact about whether the current call physically removed the directory; `closureDisposition` says how the closed state was established (`removed`, `already_closed`, `already_absent`, or `recovered_missing`). A false `removed` value therefore does not mean close failed.

## RT0 Agent-path measurement

Agent-first Runtime work measures the complete Agent path without creating another execution ledger.

Use the existing owners for separate facts:

- `ordivon-runtime-inspect summary` derives physical execution, dispatch, recovery, reservation, Artifact, and mechanical-latency facts from the Runtime Registry. These remain execution-authoritative.
- `runtime-trace.jsonl` is a bounded diagnostic projection of MCP and adapter behavior. It is not Job, Attempt, Workspace, or semantic-completion truth.
- `scripts/agent_ux_report.py` derives Tool-call volume, outcome coverage, failure rate, latency, and structured failure classes from that trace. It stores no new state.
- `scripts/observation_export.py` continues to export read-only Registry metadata into the experimental cross-owner Observation Plane. The exporter does not reinterpret Runtime state and the Observation Plane does not become Runtime authority.

The MCP adapter emits `mcp_tool_call_outcome` records at the Tool boundary. They contain Tool identity, client identity, protocol version, duration, success/failure class, and bounded structured error metadata (`errorCode`, `errorOrigin`, `retryClass`, `commitState`, and `field`). They deliberately exclude request arguments, Tool result content, error messages, Artifact bytes, prompts, and semantic judgments.

This distinction matters during RT0. A Runtime-visible Tool error can be attributed from the trace. A caller, connector, or transport failure that never produces a Runtime Tool outcome is an external Agent-path friction signal; Runtime must not fabricate an internal failure for it. Recovery through `clientRequestId`, `task.list`, `task.observe`, or Artifacts may prove what physically happened, but the extra recovery calls still count as Agent UX friction at the caller layer.

A bounded local comparison can therefore use both projections:

```bash
ordivon-runtime-inspect summary \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --since-ms <unix-ms> \
  --pretty

python3 scripts/agent_ux_report.py \
  --trace /var/lib/ordivon/registry/runtime-trace.jsonl \
  --since-ms <unix-ms> \
  --pretty
```

The first report answers whether physical execution remained correct and recoverable. The second answers how much protocol and Tool friction an Agent paid to use it. Neither report evaluates semantic completion, model quality, or user satisfaction.
