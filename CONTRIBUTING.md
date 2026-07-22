# Contributing to Ordivon

Ordivon optimizes for one understandable execution path, minimal active context, and rapid recovery.

## Start from current truth

Read `AGENTS.md` and `docs/current-state.md`. Confirm the branch, exact HEAD, relevant source and tests, and live topology. Do not load historical reports by default.

## Issue contract

An implementation Issue describes one observable state change:

```text
current state
→ one bounded change
→ target state
→ reproducible exit evidence
```

Keep only work that is ready for the active milestone open. Do not use Issues as product encyclopedias, capability catalogues, or permanent roadmaps.

## Change contract

Every persistent addition must name:

1. the observed failure or missing capability;
2. why the current code, Git, tests, Runtime, or an existing CLI cannot solve it;
3. the structure it replaces.

Preserve unrelated user changes. Prefer isolated worktrees and ordinary Git rollback. Do not add parallel execution paths, document Registries, Receipt hierarchies, evidence envelopes, wording gates, per-tool approval systems, or speculative Adapters.

`trusted-local` inherits the service user's authority. `isolated` is explicit. Preserve Runtime invariants such as idempotency, ambiguous-dispatch handling, cgroup ownership, cancellation, result identity, and reconciliation without treating them as restrictions on Agent capability.

## Verification

Run the smallest checks that can invalidate the changed boundary:

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Use stronger integration or Release checks when systemd, cgroups, persistence, transport, isolation, or deployment behavior changes.
