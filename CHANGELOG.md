# Changelog

All user-visible changes to Ordivon Runtime are recorded here. The repository follows the change classes and evidence requirements in `docs/releases.md`.

## Unreleased

- `workspace.open` now classifies a missing Git revision as `REVISION_NOT_FOUND` on `sourceRevision` instead of collapsing the Git rejection into a generic `TOOL_FAILED` error.

- concurrent observers now converge when the exact same Runner identity was already durably bound by another Runtime instance; divergent Runner identity still fails closed and duplicate observation does not append another `RUNNER_BOUND` event.

- added projection-only `task.get` over the existing read-only Runtime Job inspection path, separating ordinary Job reads from `task.observe` reconciliation/dispatch semantics; public timeline detail remains omitted and forensic detail stays operator-local.

### Added

- orthogonal `executionTarget=windows_native` admission for configured WSL/Windows nodes, backed by the repository-owned Windows Job Object launcher while preserving the existing Job/Attempt/result/replay model; R-W2 proves exact executable/image binding, durable explicit non-inherited request environment, bounded output, timeout and explicit-cancel descendant termination, native Job memory/process/CPU controls, Windows start evidence as a durable Artifact, and exact replay without turning Windows into an `ExecutionProfile`;
- public Quick Start, status, data/privacy, release, and generated Tool-reference documents;
- executable documentation ownership and local-link validation;
- scheduled CodeQL, dependency-policy, advisory, and real-system acceptance workflows;
- fixed Rust toolchain and build-toolchain identity in deployment candidate manifests;
- structured GitHub issue forms;
- explicit Agent-facing execution semantics for Runtime Jobs and Attempts, including persisted Job intent, exact Attempt state, termination intent, terminal execution disposition and reason, delivery certainty, recovery requirement, result availability, and an explicit `semanticCompletionEvaluated=false` boundary;
- Workspace source-repository identity, closure disposition, and machine-readable Workspace inventory issue stages;
- machine-discoverable `outputSchema` contracts for all public MCP Tools, covering both exact success results and the standard structured error envelope with finite origin, retry, and commit-state vocabularies;
- Workspace-bounded Runtime Job reattachment through `task.list(workspaceId=...)`, backed by a recreatable query index rather than a new state owner;
- stable cursor pagination for `workspace.list`, so bounded Workspace inventory no longer silently stops at the first page;
- exact `currentHeadRevision` on Workspace projections and a projection-only `ordivon-runtime-inspect workspace` view over Git state, active Jobs, recent Attempts, progress, output activity, and Artifact counts.
- typed Agent-authored execution proposals for `workspace.exec` and `workspace.execPlan`: Job/output/step mechanical limits may be omitted, Core preserves omission in v2 request identity, resolves only omitted values at first admission, and projects the frozen concrete plan as `effectiveLimits`.
- Core immutable input binding: operator-owned named authorities resolve only normal relative regular files, new admission copies and verifies expected SHA-256 bytes into a Runtime-owned input set, `contained_local` presents the frozen set read-only at a fixed payload path, durable plans and terminal evidence preserve `inputSetId`/effective bindings without leaking host authority/materialization paths, and exact replay resolves the existing Job before current authority/source access. The public MCP execution schema is unchanged.

### Changed

- public `workspace.exec`, `workspace.execPlan`, and `workspace.execBound` now default omitted `waitMs` to a brief 2-second post-admission observation while preserving explicit waits up to the existing 30-second Core bound; durable long-running Jobs are returned for `task.observe` instead of keeping the MCP request open by default;
- `workspace.list` derives its common projection from current physical Workspace candidates rather than scanning historical closed tombstones, uses one Git porcelain-v2 probe for HEAD plus dirty state, and leaves historical record/physical reconciliation to status, doctor, lifecycle, and reclaim surfaces;
- systemd dispatch and cancellation submit their already-durable intents with `--no-block`; Runtime launch evidence, terminal evidence, and reconciliation remain the authority for what physically happened rather than synchronous systemd CLI completion;
- executable admission preserves the exact absolute invocation path while requiring both invocation location and canonical target to remain inside executable authority, then digest-validating the target, and Runner restores that invocation as `argv[0]`, preserving rustup-style multicall/proxy semantics without trusting a retargeted symlink;
- `workspace.diff` keeps its compatibility contract of complete structured path arrays, but Git diff text is now captured with a real streaming `maxBytes` work bound instead of materializing unbounded stdout before truncation;
- new `workspace.changes` provides the response-bounded structured continuation surface for large change sets: atomic modified/added/deleted/untracked entries, explicit count and encoded-entry byte budgets, an order-independent `changeSetDigest`, and human-readable path/kind continuation cursors that fail closed when change membership/kind changes.
- README now provides a public product entry, current capability boundary, requirements, first workflow, security warning, and documentation map;
- security reporting now uses a private, actionable channel and names response and disclosure expectations;
- dependency automation and CI actions are explicitly enabled and pinned;
- repository-internal crates now declare package metadata and are not publishable accidentally;
- systemd/cgroup launch and observation helpers are physically separated from `runtime/engine.rs` without adding a new state owner, API, or execution path;
- Universal execution failures preserve precise Runtime error categories instead of collapsing Workspace identity, revision, metadata, Artifact, output, and reconciliation failures into `INVALID_REQUEST`;
- `workspace.list` isolates Workspace-local inventory/projection failures while authority-wide failures remain fail-closed;
- Workspace and mutation Tool guidance now distinguishes replay-safe reconciliation from operations that require re-reading state after an uncertain response;
- projection-only Workspace/Job reads no longer reconcile or dispatch accepted work, Workspace mutation/close guards block directly on durable reservations, `task.observe` remains exact-Job reconciliation, and MCP effect annotations now match those semantics including open-world opaque execution;
- execution and mutation contracts no longer reject work because of bootstrap-era argv, environment, step, foreign-reference, mutation, Patch-file, Patch-edit, or cgroup-budget cardinality ceilings; Linux `execve` string and aggregate argv/environment representability, durable identity, atomicity, isolation, and configured resource ceilings remain enforced;
- runtime duration, retained output, reconciliation cadence/batch size, startup grace, and SQLite busy timeout are operator policy rather than hard-coded product maxima; `ORDIVON_MAX_OUTPUT_BYTES` now makes the retained-output ceiling explicit alongside `ORDIVON_MAX_RUNTIME_MS`;
- Workspace diff path classification and active-Job identity projections remain complete instead of imposing hidden path/identity count caps; response-sized projections retain explicit continuation or truncation signals.
- Agent-first law ownership is explicit: physical/truth invariants remain hard Runtime rules, resource and retention choices remain operator policy, and bounded projections must expose completeness instead of turning transport budgets into hidden facts;
- deployment wait policy, lifecycle/reclaim retention inputs, diagnostic scan/retention inputs, cache watermarks, and capacity-acceptance parameters no longer impose legacy round-number maxima when the underlying platform or Runtime type can represent the requested value;
- explicit Workspace UID/GID `0` is no longer rejected as a Core value judgment; ownership transfer succeeds or fails according to the operator's actual OS authority, while UID/GID pair consistency remains enforced.
- Runtime deployment now has one release authority for five Rust binaries, six installed operator executables, and `mcp_probe.py`; candidate and deployment receipts bind each artifact's kind, bytes, digest, and mode, status verifies the same installed set, and rollback restores the complete previous set while remaining compatible with v1 binary-only receipts;
- release preparation materializes the requested Git Commit in a temporary detached checkout before building or staging operator artifacts, so mutable checkout dirt and `assume-unchanged`/similar index flags cannot contaminate a candidate that claims another source revision; local checkout HEAD and dirt are diagnostics, while the explicit required ref remains the release-selection gate.
- successful explicit rollback is now a first-class release-state event for status and protocol compatibility; the restored artifact set is verified from `rollback-result.json`, and the prior Commit is preserved only when the complete pre-deployment artifact fingerprint proves it, otherwise revision identity remains explicitly unknown.
- idempotent execution replay now resolves an existing Job before applying current runtime/output policy, so operator-policy drift cannot invalidate or recompute an already committed request; new admissions still use current policy, and explicit Agent limits are rejected rather than silently clamped when they exceed it.
- fully explicit legacy execution requests retain the v1 request-identity path, while requests that use optional proposal semantics enter the v2 identity path owned by Runtime Core rather than MCP transport logic.

### Fixed

- Doctor, local Runtime inspection, and secret-free status invariant reads now use one SQLite read snapshot instead of combining independently timed Registry queries;
- `systemctl show` and terminal `reset-failed` observation paths have a one-second local CLI deadline, so `task.observe(waitMs=0)` cannot be held indefinitely by a stuck observation command;
- Workspace FULL reads now enforce `maxBytes` against bytes actually read, SLICE reads preserve whole-file UTF-8/digest semantics with bounded memory and a single file pass, and output tails preserve valid text around binary bytes while hard-bounding raw bytes read;
- `workspace.close` now advertises the idempotence already guaranteed by closed-tombstone replay, while remaining correctly marked destructive;
- executable/cwd physical-validation failures now point to the exact top-level or `execution.steps[i]` field that failed;
- the shared `mcp_probe.py` uses a stable machine User-Agent instead of Python urllib's default browser signature, preventing Cloudflare Browser Integrity Check from turning a healthy public route into a false probe failure.
- Python operational and test helpers explicitly close temporary SQLite connections instead of emitting resource warnings;
- `artifactsAvailable` now reflects registered Artifact existence rather than merely the presence of an Attempt result digest;
- recovery-bearing states such as `orphaned` explicitly project reconciliation requirements instead of appearing mechanically complete;
- multi-field Job projections are assembled from one Registry read snapshot so execution, recovery, Artifact, and terminal-reason facts cannot drift across concurrent reads;
- the bootstrap rule requiring `sum(step.timeoutMs) <= execution.timeoutMs` has been removed: the Job timeout is a shared overall deadline and step timeouts are independent local upper bounds, matching Runner behavior;
- post-admission execution errors now carry the durable Job identity and report `commitState=committed` with `retryClass=reconcile_first` instead of incorrectly claiming that a Runtime operation was not committed;
- Doctor capacity-holder projection now reports `capacityHoldersTruncated` instead of silently presenting the first 50 holders as complete, and terminal control evidence no longer silently drops error detail after 4096 characters.
- Runtime `CONCURRENCY_LIMIT` errors now report `holdersTruncated`, so a bounded 16-identity holder projection cannot be mistaken for the complete active set when global capacity is configured above that projection size.
- `workspace.close` now preserves cross-Workspace Git authority: while holding the existing lifecycle lock it rejects closure with `WORKSPACE_BUSY` when another open Workspace's Git common directory lives under the Workspace or disposable cache paths that would be removed. This prevents a parent Workspace from deleting the Git authority of an already-open child Workspace without adding a dependency database.

## 0.1.0 — Operational baseline

### Added

- Git Workspace lifecycle and digest-bound source commitment;
- durable Job and Attempt Registry with idempotent request admission;
- systemd/cgroup process-tree ownership, cancellation, reconciliation, and recovery;
- bounded Results, Artifacts, terminal supervision evidence, backup, restore, doctor, and repair;
- receipted deployment, rollback, capacity acceptance, lifecycle, reclaim, cache management, and protocol compatibility inventory;
- MCP Tool surface for Workspace, Job, and Artifact operations.
