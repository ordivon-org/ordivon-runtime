# Ordivon Agent Entry

## Minimal read path

Read only:

1. `docs/current-state.md`
2. `docs/architecture/runtime.md`
3. `docs/operations/runbook.md`

Then inspect the relevant source, tests, Git state, and live process state. Do not load dated reports, retrospectives, closed Issues, or the M-series tag unless a specific unresolved question requires them.

## Three engineering defaults

### Context over documentation

Code, tests, Git, and observed deployment are primary truth. Documents exist to compile the smallest useful context for an Agent, not to preserve every historical explanation.

### Recovery over prevention

Use Git for code recovery. Preserve only execution facts that reasoning cannot reconstruct: dispatch identity, running process ownership, cancellation intent, terminal result, and Artifact identity. Do not add parallel Receipts, evidence Registries, wording gates, or speculative approval systems.

### Intent over implementation scarcity

The user owns goals, preferences, risk posture, and final commitments. The Agent owns implementation, testing, repair, and ordinary operation. The Runtime owns durable execution truth and recovery.

## Operating model

- Prefer direct execution and rapid correction over ceremony.
- Preserve unrelated user changes and use isolated worktrees for substantial work.
- In `trusted-local`, inherit the service user's host authority, including network, credentials, Git, Docker, systemd, cloud CLIs, and accessible files and executables.
- Use `isolated` only when the user or Agent explicitly chooses reduced authority for untrusted inputs.
- Preserve idempotency, ambiguous-dispatch handling, process ownership, cancellation, terminal identity, and restart recovery without restricting the Agent's ordinary capabilities.

## Change discipline

Every persistent addition must name the observed failure it solves and the current structure it replaces. Model-generated “best practices” inherited from human-era software engineering are hypotheses, not defaults. Prefer existing CLIs through `workspace.exec` over bespoke Adapters. Keep one active milestone in `docs/current-state.md`; do not create horizontal capability catalogues.

## Default verification

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Add stronger checks only when the changed boundary can invalidate them. Git history is the archive.
