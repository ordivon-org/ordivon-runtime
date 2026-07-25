# Ordivon Runtime Agent Entry

## Start from reality

Do not preload the documentation tree. Confirm the exact Git state, then inspect the relevant source, tests, and live process state.

Read [`docs/runtime.md`](docs/runtime.md) only when changing or diagnosing cross-component execution semantics: Workspace, request identity, Job, Attempt, reservation, runner bundle, process ownership, terminal result, Artifact, or reconciliation.

Read [`docs/recovery.md`](docs/recovery.md) only when operating on Registry integrity, backups, administrative repair, or restore. Read [`docs/effect-kernel.md`](docs/effect-kernel.md) before adding an operation identity field, retry path, external receipt, authority contract, world precondition, or structured effect adapter.

## Truth and responsibility

- The user owns goals, preferences, risk posture, and final commitments.
- The Agent owns implementation, testing, ordinary repair, and operation.
- Git owns code history, diff, rollback, and Workspace identity.
- SQLite owns durable request, Job, Attempt, reservation, cancellation, and resolution state.
- systemd and cgroups own observation and termination of live process trees.
- Result and Artifact files own their Digest-bound bytes.
- Current tools, configuration, deployment, and health must be queried from the live system rather than copied into prose.

## Component boundary

This repository implements **Ordivon Runtime**, not the complete Ordivon system. Cognition, planning, memory, coordination, and product interfaces remain outside this repository unless a concrete Runtime contract requires them.

## Change rule

Every persistent addition must solve an observed failure or repeatedly missing operation and state what existing path it replaces or extends. Effect semantics must be enforceable by a structured adapter; never trust a caller-declared effect class for arbitrary execution. Prefer mature host CLIs through `workspace.exec` over bespoke adapters, while treating arbitrary execution as opaque. Preserve unrelated user changes, use isolated Git Workspaces for substantial work, and keep one execution path.

Ordivon Runtime is trusted-local. Put untrusted code behind an external isolation boundary rather than adding a second policy or sandbox Runtime inside it.

## Verification

Run the smallest checks that can invalidate the changed boundary. CI is the machine-owned default verification contract.
