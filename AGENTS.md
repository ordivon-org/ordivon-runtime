# Ordivon Agent Entry

## Read order

1. `docs/current-state.md`
2. `docs/ai/problem-statement.md`
3. `docs/architecture/runtime.md`
4. `docs/architecture/operating-boundary.md`
5. `docs/operations/runbook.md`
6. `docs/operations/deployment-boundary.md`

Do not derive current architecture from dated reports, retrospectives, old Issues, or the M-series tag.

## Default operating model

- Prefer direct execution over pre-emptive ceremony.
- Preserve user changes and use isolated worktrees for substantial work.
- Record exact revisions, commands, results, and terminal state.
- Recover from reversible failures instead of adding speculative gates.
- In `trusted-local`, inherit the user's host authority instead of inventing per-tool approval or allowlist systems.
- Preserve ambiguous-dispatch, idempotency, concurrency, process ownership, cancellation, and persistent-truth invariants without restricting what the Agent may operate.
- Use `isolated` only when the user or Agent explicitly chooses reduced authority for untrusted inputs.

## Active engineering surface

The current surface is one Rust MCP server, one Rust execution Runtime, SQLite persistent truth, systemd and cgroup supervision, minimal execution policy, packaging, and operational helpers.

Historical governance runtimes, Receipt systems, generated evidence trees, document Registries, and parallel M-series servers are not active architecture.

## Change discipline

Before adding a component, state the concrete failure, its causal location, the simpler alternatives considered, and the structure being replaced. Do not preserve completed development stages as permanent current surfaces.

Before creating an Issue, confirm that it advances the only active milestone in `docs/current-state.md` and describes one observable state change.

## Default verification

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Run stronger checks only when their boundary is affected. Git history and tag `m-series-closed-2026-07-22` are the archive.
