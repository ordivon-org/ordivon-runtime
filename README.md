# Ordivon

Ordivon is a local execution Runtime for capable agents.

It provides one path from a Git-backed repository task to a bounded process, persistent Job and Attempt truth, observable results, and recovery after interruption.

```text
agent intent
→ exact repository state
→ bounded execution
→ owned process tree
→ persistent result
→ recovery or continuation
```

Ordivon treats ordinary repository edits as reversible through Git. Strong controls remain for credentials, irreversible external effects, ambiguous dispatch, concurrency ownership, persistent-state integrity, and untrusted execution.

## Current surface

- one `ordivon-mcp` Streamable HTTP server;
- one `ordivon-exec` Runtime;
- SQLite Job, Attempt, idempotency, and concurrency truth;
- systemd and cgroup process ownership;
- timeout, bounded output, cancellation, restart recovery, and Artifact verification;
- `trusted-local` by default and `isolated` on demand.

## Current truth

Read [docs/current-state.md](docs/current-state.md) first. It distinguishes repository `main`, the simplification candidate, and the live remote Desktop Commander bridge. These are not yet the same system.

The complete active document set is listed in [docs/README.md](docs/README.md). Dated audits, acceptance reports, closure notes, and retrospectives are records rather than current architecture.

## Development checks

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Use stronger checks when the changed boundary makes them relevant.

## Recovery point

The complete M0–M7 tree remains available through Git tag `m-series-closed-2026-07-22`. Historical code is not retained as parallel current architecture.
