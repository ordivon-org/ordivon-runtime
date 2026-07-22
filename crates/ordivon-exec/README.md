# ordivon-exec

`ordivon-exec` is the local execution core used by `ordivon-mcp`.

## Current surface

The default crate feature is `transactional-runtime`. It provides:

- exact Git workspaces and digest-bound file mutation;
- bounded read, search, diff, stdout, stderr, and Artifact access;
- a SQLite Job/Attempt Registry;
- idempotent admission and transactional global concurrency reservations;
- one active command tree per Workspace, with independent Workspaces allowed to run concurrently;
- systemd/cgroup process-tree ownership;
- timeout, cancellation, terminal result persistence, and reconciliation;
- fail-closed handling for ambiguous dispatch, lost identity, and orphaned work;
- recovery of a late identity-bound Runner result only after the original process tree is proven gone, followed by atomic terminal correction and reservation release.

The task runner is built as:

```bash
cargo build -p ordivon-exec --bin ordivon-task-runner
```

## Execution modes

`trusted-local` is selected by leaving `IsolationConfig` absent. The transient unit inherits the host user's environment and authority: network, credentials, host filesystem, Git remotes, Docker, systemd, cloud CLIs, and accessible executables remain available. The Runtime adds cgroup ownership, timeout, bounded output, durable Registry truth, cancellation, and recovery without imposing a sandbox.

The optional `isolated-execution` feature adds the non-root `ordivon-worker`, hidden host control paths, private network/filesystem views, ownership checks, and orphan remediation. `ordivon-mcp` activates it only when `ORDIVON_EXECUTION_MODE=isolated`.

## On-demand OS tests

Tests that require root, systemd, and cgroup v2 are ignored unless explicitly enabled:

```bash
ORDIVON_RUN_INTEGRATION=1 \
ORDIVON_RUNNER_PATH="$PWD/target/debug/ordivon-task-runner" \
cargo test -p ordivon-exec --all-features --test transactional_runtime -- --ignored --test-threads=1

ORDIVON_RUN_SYSTEMD_SPIKE=1 \
cargo test -p ordivon-exec --all-features --test systemd_supervisor -- --ignored --test-threads=1
```

## Deliberate exclusions

This crate does not contain the historical migration comparator, filesystem Task registry, M-series CLI, lifecycle quotas, GC/tombstones, backup protocol, document governance, or a second execution server. Historical implementations remain available through Git tag `m-series-closed-2026-07-22`.
