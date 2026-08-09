# Agent-facing execution UX

Ordivon Runtime keeps a small universal execution surface in the core and leaves product-specific workflows in Host or external developer tools.

## Core primitives

- `workspace.mutate`: one digest-guarded atomic batch for writes, appends, and exact replacements. Existing targets require the complete-file digest. It remains a low-level synchronous mutation without durable replay identity.
- `workspace.patch`: one multi-file, multi-hunk text patch under a stable `clientRequestId`. Every existing file binds its complete before digest; every edit binds an exact one-based line, zero-based Unicode-column range and `expectedText`. Exact replay returns the committed receipt instead of writing again.
- `workspace.patch.get`: reconcile one Patch receipt. It does not apply an uncommitted Patch, but it may advance Runtime receipt state from `prepared` to `committed` or `unknown` after inspecting physical file state.
- `workspace.execPlan`: ordered native process steps. Steps stop on the first failure by default and continue only with explicit `continueOnError`. Job-wide and step-local mechanical timeouts may be delegated to Runtime; explicit timeouts remain exact Agent constraints. No shell parsing or injected `set -e` is involved.
- `foreignReferences`: opaque immutable correlation inputs on execution requests. A Host Harness path uses namespace `ordivon.host` and exact `task`, `task_attempt`, `assignment`, and `harness_run` identities. Runtime does not impose a representation-count ceiling; the references participate in request identity and terminal evidence but do not transfer Host ownership into Runtime.
- `workspace.execBound`: public immutable-input execution. A Runtime instance owns operator-configured named `InputAuthority` roots; the caller may reference only an authority name, a normal relative object path, an expected SHA-256 digest, and a presentation-relative path. The Tool structurally selects `contained_local` rather than exposing a caller-selectable profile. New admission resolves the object beneath the configured authority with fail-closed path rules, copies and verifies exact bytes into a Job-owned Runtime input set, freezes `inputSetId` plus `effectiveInputs`, and presents that set read-only at `/run/ordivon/inputs`. Durable plan/evidence does not contain the authority host root or materialization host path. Exact replay finds the existing Job before consulting current policy, authority configuration, or source bytes.
- `task.observe`: targeted Job observation plus reconciliation. It may dispatch the exact requested Job when that Job is durably `accepted` with `desiredState=run`; it never creates a new Job or reconciles unrelated Jobs. It projects the durable `effectiveLimits`, elapsed time, retained output byte counts, last output time, progress revision, completed and total steps, current step, and first failed step. `waitUntil=change_or_terminal` waits until output, progress, or terminal state changes.
- typed operation errors: MCP errors carry `origin`, `retryClass`, `commitState`, `traceId`, and, when known, `operationId`. `commitState=unknown` requires reconciliation rather than a new operation identity.

## Bound ownership

Runtime does not reject a request merely because a collection has crossed a convenient round number. A bound must protect a concrete physical or semantic invariant: transport size, retained bytes, durable identity, atomicity, isolation, platform representability, or a response projection with an explicit continuation/truncation contract. Public paginated lists may keep a page-size ceiling; chunked reads may keep a per-read ceiling; those limits are acceptable because the Agent can continue without losing truth.

Execution argument and environment collection counts, `execPlan` step counts, foreign-reference counts, mutation counts, Patch file/edit counts, and cgroup budget maxima are therefore not Runtime policy. Individual argv/environment strings and the combined argv/environment payload remain bounded only by the host Linux `execve` physical limits (`MAX_ARG_STRLEN` and `_SC_ARG_MAX`). Runtime/output server ceilings are operator configuration for new admission, not a reason to reinterpret historical Jobs. Supported mechanical limits may be omitted: Runtime forms proposal identity before resolution, fills only omitted values from current policy on first admission, freezes the concrete plan, and preserves explicit values without clamping. Existing replay is resolved before current policy. `workspace.diff` may truncate patch text when requested, but its structured changed/untracked path projection is complete.

Runtime law is intentionally narrower than Runtime policy. A **hard Runtime law** must protect either physical representability or a Runtime-owned truth invariant such as durable identity, causal ownership, atomicity, reconciliation, or explicit authority boundaries. An **operator policy** chooses resource or retention budgets for one installation and may be changed without redefining Runtime semantics. A **warning or bounded projection** informs an Agent about risk or incomplete evidence and must make incompleteness machine-visible. A CLI confirmation phrase is only an operator anti-mistake affordance; copying a magic string does not prove understanding or grant new Runtime authority, and this pattern must not become an Agent-facing authorization model.

The governing question for every denial is therefore: **what concrete physical or truth invariant would become false if this request were allowed?** If the answer is only a convenient round number, historical implementation simplification, or a generic statement that an operation is risky, the rule belongs outside the hard Runtime contract. Risk should be made legible through identities, consequences, retry semantics, and evidence before it is promoted to prohibition.

## Proposal and effective-plan boundary

The Agent must decide the action; it does not have to restate every installation-owned mechanical ceiling on every call. `workspace.exec`, `workspace.execPlan`, and `workspace.execBound` therefore distinguish a proposal from the concrete Execution Plan. Executable, arguments, working directory, environment, profile, references, and explicit budgets remain concrete action input. Job timeout, retained stdout/stderr limits, and step-local timeout may be omitted when the Agent wants Runtime to choose the current mechanical bound.

Omission is itself durable request intent. It receives a proposal request identity distinct from any explicit numeric value. On a new admission Runtime resolves only the omitted fields, applies current operator policy, and stores the concrete values in the Execution Plan. Later replay with the same `clientRequestId` finds the existing Job before policy evaluation, so a policy change cannot silently alter or invalidate the old action. A new request after the same policy change receives the new policy values. `effectiveLimits` is the Agent-facing projection of the frozen plan; it is evidence of what governed this Job, not a report of whatever the server is configured to use now.

For compatibility, a fully explicit legacy `workspace.exec` request keeps its v1 request identity. `workspace.execPlan` also keeps its historical v1 shape when all step timeouts and both output limits are explicit and no new Job-wide timeout is supplied; in that legacy shape the adapter derives the historical overall timeout from the explicit step sum. Once optional proposal semantics are used, Core owns the v2 proposal identity and resolution. New plan semantics do not impose the old `sum(step.timeoutMs) <= execution.timeoutMs` rule: the Job timeout is a shared overall deadline and each step timeout is an independent local upper bound.

## Machine-discoverable Tool results

All public Runtime MCP Tools publish an `outputSchema` for their `structuredContent`. The schema is a two-branch contract: one branch is the Tool's exact success result and the other is the standard `{ "error": ToolError }` envelope. A caller can therefore discover execution fields, Workspace closure state, Artifact metadata, and error-control semantics before the first Tool call instead of learning result shape from trial execution.

The error envelope keeps `code` as the stable machine reason while constraining the control vocabulary that changes safe Agent behavior: `origin` is one of `mcp_adapter`, `runtime_core`, or `workspace_executor`; `retryClass` is one of `never`, `safe_same_request`, or `reconcile_first`; and `commitState` is one of `not_started`, `not_committed`, `committed`, or `unknown`. `committed` means Runtime can prove that a durable operation identity already exists; the error includes `operationId`, and the caller must reattach/reconcile that identity rather than create new work. `unknown` remains reserved for cases where commitment itself cannot be proven. `toolCatalogDigest` binds these generated output schemas together with Tool names, descriptions, annotations, and input schemas, so a discovery refresh can detect an exact contract change without creating another Runtime authority.

## Observation and reconciliation boundary

Observation does not implicitly authorize execution. `workspace.get`, `workspace.list`, and `task.list` are projection-only reads over current Workspace/Registry truth: they do not reconcile, dispatch, cancel, or otherwise advance Jobs. Workspace mutation, Patch, and close operations likewise use durable active/held reservations as guards and do not dispatch an accepted Job merely to decide that the Workspace is busy. A projection can therefore temporarily show an unsettled state even when newer physical evidence exists but has not yet been committed by reconciliation.

Reconciliation has explicit owners. The service performs startup and bounded background maintenance reconciliation; execution admission may reconcile the same Workspace before admitting new work; and `task.observe(jobId)` performs targeted reconciliation of exactly that Job. If the requested Job is durably `accepted` with `desiredState=run`, `task.observe` may dispatch that already-committed execution intent, so its MCP contract is deliberately not read-only and is open-world because the committed command may have external effects. `workspace.patch.get` is also not read-only because it may advance a durable Patch receipt after inspecting physical file state, though it never applies an uncommitted Patch.

## Workspace-bounded Job reattachment

`task.list` accepts an exact `workspaceId` in addition to the existing exact `clientRequestId`. The filters may be used independently or together. This is a projection-only read over the existing `jobs.workspace_id` relation: it creates no new recovery state, does not make Workspace metadata a Job authority, and does not reconcile or dispatch Jobs. A reconnecting Agent that retained a Workspace identity can therefore enumerate that Workspace's historical terminal and nonterminal Jobs with the normal newest-first cursor instead of paging through the global Job ledger. When fresher physical state must be reconciled, target the exact Job with `task.observe`. Runtime maintains a derived `(workspace_id, created_at_ms, job_id)` query index alongside the existing client-request lookup index; both are recreatable query accelerators outside `schema_migrations`.

## Correlated Patch recovery

A durable Patch records its Intent and before/after file digest plan before physical mutation. On response loss, reissue the exact same `workspace.patch` request or call `workspace.patch.get` with the same `clientRequestId`. Runtime applies the Patch only when every file still matches the complete before state. If every file already matches the after state, Runtime commits the missing receipt. Any mixed state is isolated as `unknown`; Runtime never guesses whether a partial external write should be completed or rolled back.

Patch receipt storage is an additive table outside the Registry migration version. Previous Runtime binaries ignore it, preserving receipt-bound rollback compatibility; the current binary recreates the table and index idempotently when absent.

## Correlated Job recovery

Reissue an exact committed request only with the same `clientRequestId`, execution request, and foreign references; Runtime then returns the original Job. Do not reuse that identity after changing a Host Assignment generation or digest. When delivery may have committed, filter `task.list` by the exact `clientRequestId`, require one consistent Job identity, then inspect `task.observe` and the terminal-evidence Artifact rather than dispatching new work.

A successful Runtime Job or Runtime Attempt is not a Host completion decision. Required Artifacts, unresolved Effects or `UNKNOWN` state, and semantic acceptance remain above Runtime.

## Workspace recovery

`workspace.open` accepts a caller-provided ID or generates an immutable `ws-*` handle when `workspaceId` is omitted. Use an explicit unique ID when deterministic response-loss reconciliation matters; `workspace.open` itself is not an idempotent replay operation, so an uncertain response should be checked with projection-only `workspace.get` rather than blindly repeated. Generated handles are a low-friction convenience when response-loss identity is not required. `workspace.list` and `workspace.get` are projection-only reconnection entry points and expose canonical `sourceRepo`, the immutable opening/base `sourceRevision`, exact live `currentHeadRevision`, detached-head mode, dirty state, creation time, and complete active-Job identities without dispatching those Jobs. `workspace.list` uses stable newest-first cursor pagination and omits the expensive `sourceStateDigest` by default; `workspace.get` remains the exact single-Workspace proof boundary. A caller that verifies one exact source state can pass that digest to `workspace.close.expectedSourceStateDigest`; Runtime compares it under the Workspace lifecycle lock, persists it in the closed tombstone, and permits exact replay after response loss. `workspace.diff` returns the bounded patch together with complete sorted `changedPaths`, `modifiedPaths`, `addedPaths`, `deletedPaths`, structured `renamedPaths`, and `untrackedPaths`; callers do not need to infer the exact changed set from patch headers.

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
- `effectiveLimits` reports the concrete Job timeout, retained stdout/stderr limits, and step-local timeouts frozen into the durable Execution Plan; it does not reread current operator policy;
- `recoveryRequired` is a derived Runtime fact: it is true for active recovery conditions and recovery-bearing states such as `recovering` or `orphaned`, rather than exposing an internal condition-table implementation detail;
- `resultAvailable` means a durable Runtime terminal result exists, not that the result is semantically correct;
- `artifactsAvailable` means at least one registered Runtime Artifact actually exists; it is not inferred from the presence of a result digest;
- `semanticCompletionEvaluated` is always `false`. Runtime does not decide Task, Goal, Assignment, or domain completion.

`resultAvailable=true` is therefore compatible with `deliveryDisposition=reconciliation_required` or `deliveryDisposition=unknown`. A caller must not translate terminality into certainty, certainty into semantic correctness, or `lost` into permission to redispatch an effect-opaque operation. `pollAfterMs` is emitted for `in_progress` and `reconciliation_required`; a terminal `unknown` disposition intentionally carries no implication that repeated polling will make the external effect safe to repeat.

The projection rule is: **if two Runtime-owned states imply different safe Agent behavior, the Agent-facing contract must preserve that distinction through a machine-readable field.** Runtime may spend additional bounded read work to preserve this semantic certainty; later mechanical optimization must not collapse it again. `task.list` and single-Job projections construct their multi-field semantic view from one bounded Registry read snapshot, so a caller does not receive a state vector assembled across different concurrent moments.

Workspace mutation follows the same rule. Active or held Jobs block `workspace.mutate`, `workspace.patch`, and `workspace.close` directly from durable reservation state; these operations do not dispatch accepted work merely to decide that the Workspace is busy. A close may also return `WORKSPACE_BUSY` with dependent Workspace identities when its physical removal would delete Git authority still required by another open Workspace. The Agent should close or otherwise resolve those dependent Workspaces first rather than forcing the parent and corrupting their physical identity. `workspace.mutate` is atomic and digest-guarded but does not own a durable request identity or replay receipt; if its response is uncertain, the caller must re-read the affected Workspace and, when needed, reconcile the exact Job separately before deciding whether to retry. `workspace.patch` is the preferred text-mutation primitive when response-loss safety matters because `clientRequestId` and `workspace.patch.get` make committed, uncommitted, and unknown outcomes distinguishable without guessing.

Workspace closure also separates state from action. `WorkspaceCloseResult.removed` is a compatibility fact about whether the current call physically removed the directory; `closureDisposition` says how the closed state was established (`removed`, `already_closed`, `already_absent`, or `recovered_missing`). A false `removed` value therefore does not mean close failed.

## RT0 Agent-path measurement

Agent-first Runtime work measures the complete Agent path without creating another execution ledger.

Use the existing owners for separate facts:

- `ordivon-runtime-status --dashboard` is the fast Bash cockpit above the per-Workspace microscope. It projects installed release/MCP identity, capacity/recovery counts, Workspace inventory, and bounded active/recent Job freshness directly from deployment receipts, systemd, Registry, Workspace records, and existing Attempt files. It never calls `task.observe`, reads output contents, or runs the expensive maintenance probes used by `--diagnose`.
- `ordivon-runtime-inspect workspace` combines read-only Registry projection with current Git HEAD/status and existing Runner progress/output metadata for one Workspace. It does not call `task.observe`, reconcile, dispatch, or create a second state owner.
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
