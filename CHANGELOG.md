# Changelog

All user-visible changes to Ordivon Runtime are recorded here. The repository follows the change classes and evidence requirements in `docs/releases.md`.

## Unreleased

### Added

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

### Changed

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

### Fixed

- Python operational and test helpers explicitly close temporary SQLite connections instead of emitting resource warnings;
- `artifactsAvailable` now reflects registered Artifact existence rather than merely the presence of an Attempt result digest;
- recovery-bearing states such as `orphaned` explicitly project reconciliation requirements instead of appearing mechanically complete;
- multi-field Job projections are assembled from one Registry read snapshot so execution, recovery, Artifact, and terminal-reason facts cannot drift across concurrent reads;
- post-admission execution errors now carry the durable Job identity and report `commitState=committed` with `retryClass=reconcile_first` instead of incorrectly claiming that a Runtime operation was not committed;
- Doctor capacity-holder projection now reports `capacityHoldersTruncated` instead of silently presenting the first 50 holders as complete, and terminal control evidence no longer silently drops error detail after 4096 characters.
- `workspace.close` now preserves cross-Workspace Git authority: while holding the existing lifecycle lock it rejects closure with `WORKSPACE_BUSY` when another open Workspace's Git common directory lives under the Workspace or disposable cache paths that would be removed. This prevents a parent Workspace from deleting the Git authority of an already-open child Workspace without adding a dependency database.

## 0.1.0 — Operational baseline

### Added

- Git Workspace lifecycle and digest-bound source commitment;
- durable Job and Attempt Registry with idempotent request admission;
- systemd/cgroup process-tree ownership, cancellation, reconciliation, and recovery;
- bounded Results, Artifacts, terminal supervision evidence, backup, restore, doctor, and repair;
- receipted deployment, rollback, capacity acceptance, lifecycle, reclaim, cache management, and protocol compatibility inventory;
- MCP Tool surface for Workspace, Job, and Artifact operations.
