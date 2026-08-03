# Changelog

All user-visible changes to Ordivon Runtime are recorded here. The repository follows the change classes and evidence requirements in `docs/releases.md`.

## Unreleased

### Added

- public Quick Start, status, data/privacy, release, and generated Tool-reference documents;
- executable documentation ownership and local-link validation;
- scheduled CodeQL, dependency-policy, advisory, and real-system acceptance workflows;
- fixed Rust toolchain and build-toolchain identity in deployment candidate manifests;
- structured GitHub issue forms.

### Changed

- README now provides a public product entry, current capability boundary, requirements, first workflow, security warning, and documentation map;
- security reporting now uses a private, actionable channel and names response and disclosure expectations;
- dependency automation and CI actions are explicitly enabled and pinned;
- repository-internal crates now declare package metadata and are not publishable accidentally;
- systemd/cgroup launch and observation helpers are physically separated from `runtime/engine.rs` without adding a new state owner, API, or execution path.

### Fixed

- Python operational and test helpers explicitly close temporary SQLite connections instead of emitting resource warnings.

## 0.1.0 — Operational baseline

### Added

- Git Workspace lifecycle and digest-bound source commitment;
- durable Job and Attempt Registry with idempotent request admission;
- systemd/cgroup process-tree ownership, cancellation, reconciliation, and recovery;
- bounded Results, Artifacts, terminal supervision evidence, backup, restore, doctor, and repair;
- receipted deployment, rollback, capacity acceptance, lifecycle, reclaim, cache management, and protocol compatibility inventory;
- MCP Tool surface for Workspace, Job, and Artifact operations.
