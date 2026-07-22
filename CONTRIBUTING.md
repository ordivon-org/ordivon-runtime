# Contributing to Ordivon

Ordivon optimizes for a small, understandable execution runtime rather than a large governance platform.

## Before committing

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Use additional checks only when relevant:

- strict Clippy for final Rust runtime changes;
- systemd and cgroup integration for supervisor changes;
- reboot recovery for startup or reconciliation semantics;
- dependency and CodeQL audits for dependency or release work;
- isolated-worker tests for untrusted execution paths.

## Change boundaries

Preserve unrelated user changes. Prefer an isolated worktree and small commits with ordinary Git rollback. Do not add generated governance registries, receipts, evidence envelopes, or wording gates.

Hard controls are justified for irreversible external effects, credentials, ambiguous dispatch, concurrency ownership, and persistent state that cannot be reconstructed.

## Documentation

Keep the current document index in `docs/README.md`. Historical material belongs in Git history rather than a permanent archive directory.
