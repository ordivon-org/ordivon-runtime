# Ordivon Agent Entry

## Minimal read path

Read only:

1. `docs/current-state.md`
2. `docs/architecture/runtime.md`
3. `docs/operations/runbook.md`

Then inspect relevant source, tests, Git state, and live process state. Historical implementations and reports belong to Git history and the `m-series-closed-2026-07-22` tag; do not reconstruct them without a concrete unresolved failure.

## Engineering defaults

### Context over documentation

Code, tests, Git, and observed deployment are primary truth. Documents compile the smallest useful context; Git is the archive.

### Recovery over prevention

Use Git for code recovery. Preserve only dispatch identity, process ownership, cancellation intent, terminal result, and Artifact identity. Do not add parallel Receipts, evidence Registries, wording gates, or speculative approval systems.

### Intent over implementation scarcity

The user owns goals, preferences, risk posture, and final commitments. The Agent owns implementation, testing, repair, and ordinary operation. The Runtime owns durable execution truth and recovery.

## Operating model

- Prefer direct execution and rapid correction over ceremony.
- Preserve unrelated user changes and use isolated Git workspaces for substantial work.
- `trusted-local` inherits the service user's host authority, including network, credentials, Git, Docker, systemd, cloud CLIs, files, and executables.
- Put untrusted code in an external VM or container; do not rebuild a policy platform inside Ordivon.
- Preserve idempotency, ambiguous-dispatch handling, process ownership, cancellation, terminal identity, and restart recovery without restricting ordinary Agent capability.

## Change discipline

Every persistent addition must name the observed failure it solves and the current structure it replaces. Prefer existing CLIs through `workspace.exec` over bespoke Adapters. Keep one execution path and no permanent roadmap catalogue.

## Default verification

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Add stronger checks only when the changed boundary can invalidate them.
