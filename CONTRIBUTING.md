# Contributing to Ordivon

Ordivon optimizes for one understandable execution path, not a large governance platform.

## Start from current truth

Read `docs/current-state.md` before creating an Issue or implementation. Confirm the checked-out branch, exact HEAD, live topology, and only active milestone. Dated reports and old Issues are not current architecture.

## Issue contract

An implementation Issue must describe one observable state change:

```text
current state
→ one bounded execution path
→ target state
→ reproducible exit evidence
```

It must include current evidence, target behavior, non-goals, affected path, and exit commands. An Issue must not double as a product vision, knowledge base, multi-stage roadmap, or catalogue of future capabilities.

Keep only work that is ready for the active milestone open. Close obsolete or superseded Issues rather than extending them indefinitely. A new horizontal umbrella is not a substitute for choosing the next executable path.

## Change contract

Every persistent addition must name the failure or missing user capability it addresses, the current mechanism that is insufficient, and the structure it replaces. Avoid permanent parallel implementations.

Preserve unrelated user changes. Prefer an isolated worktree and small commits with ordinary Git rollback. Do not add generated governance Registries, Receipt hierarchies, evidence envelopes, wording gates, or stage-versioned current servers.

Hard controls are justified for irreversible external effects, credentials, ambiguous dispatch, concurrency ownership, persistent truth, and selected isolation boundaries.

## Verification

Run the smallest checks that can invalidate the change:

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Add strict Clippy, systemd and cgroup integration, restart recovery, isolated-worker tests, dependency audits, CodeQL, or release acceptance when the modified boundary requires them.
