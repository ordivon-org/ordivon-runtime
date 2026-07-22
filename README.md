# Ordivon

Ordivon is a local execution runtime for capable agents.

Its default engineering model is:

```text
agent acts freely
→ runtime records what happened
→ failures remain recoverable
→ hard blocking is reserved for irreversible boundaries
```

Code and repository files are treated as reversible through Git. Strong controls remain for credentials, external side effects, ambiguous dispatch, concurrency ownership, and other state that cannot be reconstructed by reasoning alone.

## Current direction

The active simplification removes historical governance machinery and converges on:

- one Rust execution runtime;
- one MCP surface;
- SQLite-backed Job and Attempt truth;
- idempotency and concurrency reservation;
- cgroup ownership, timeout, output bounds, cancellation, and recovery;
- trusted-local execution by default, with isolation available on demand.

See [docs/README.md](docs/README.md) for the small current document set.

## Development checks

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Stronger isolation, Clippy, systemd matrices, reboot testing, dependency audits, and CodeQL are run when the changed boundary requires them.

## Recovery point

The complete M0–M7 tree remains available through Git tag `m-series-closed-2026-07-22`.
