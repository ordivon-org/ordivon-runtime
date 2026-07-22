# ordivon-exec

`ordivon-exec` is the local execution core used by `ordivon-mcp`.

## Current surface

The default crate feature is `transactional-runtime`. It provides:

- exact Git workspaces and digest-bound file mutation;
- bounded read, search, diff, stdout, stderr, and Artifact access;
- a SQLite Job/Attempt Registry;
- idempotent admission and concurrency reservations;
- systemd/cgroup process-tree ownership;
- timeout, cancellation, terminal result persistence, and reconciliation;
- fail-closed handling for ambiguous dispatch, lost identity, and orphaned work.

The task runner is built as:

```bash
cargo build -p ordivon-exec --bin ordivon-task-runner
```

## Execution modes

`trusted-local` is selected by the MCP layer by leaving `IsolationConfig` absent. It runs user-owned Git-backed work as the current user while retaining cgroup ownership, timeout, output bounds, durable Registry truth, and recovery.

The optional `isolated-execution` feature adds the non-root `ordivon-worker`, isolated filesystem views, ownership checks, and minimal orphan remediation for untrusted code. `ordivon-mcp` compiles this capability but activates it only when `ORDIVON_EXECUTION_MODE=isolated`.

## On-demand OS tests

Tests that require root, systemd, and cgroup v2 are ignored unless explicitly enabled:

```bash
ORDIVON_RUN_INTEGRATION=1 cargo test -p ordivon-exec --test transactional_runtime -- --ignored
cargo test -p ordivon-exec --features systemd-supervisor --test systemd_supervisor -- --ignored
```

## Deliberate exclusions

This crate does not contain the historical migration comparator, filesystem Task registry, M-series CLI, lifecycle quotas, GC/tombstones, backup protocol, document governance, or a second execution server. Historical implementations remain available through Git tag `m-series-closed-2026-07-22`.
