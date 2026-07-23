# Contributing to Ordivon

Ordivon optimizes for one understandable execution path, minimal active context, and rapid recovery.

## Start from current truth

Read `AGENTS.md` and `docs/current-state.md`. Confirm the branch, exact HEAD, relevant source and tests, and live topology. Do not load historical implementations by default.

## Change contract

One change should express:

```text
observed failure or missing operation
→ bounded implementation
→ reproducible result
```

Every persistent addition must explain why current code, Git, tests, Runtime, or an existing CLI cannot solve the problem, and what existing structure it replaces. Preserve unrelated user changes. Prefer isolated Git workspaces and ordinary rollback. Do not add parallel execution paths, document Registries, Receipt hierarchies, wording gates, per-tool approval systems, or speculative Adapters.

Ordivon is a trusted-local Runtime. Untrusted workloads require an external isolation boundary. Preserve idempotency, process ownership, cancellation, result identity, and reconciliation without turning them into governance ceremony.

## Verification

Run the smallest checks that can invalidate the changed boundary:

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Use stronger integration or release checks only when systemd, cgroups, persistence, transport, or deployment behavior changes.
