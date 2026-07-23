# ordivon-exec

`ordivon-exec` is the trusted-local execution core used by `ordivon-mcp`.

It provides:

- exact Git workspaces and digest-bound file mutation;
- bounded read, search, diff, stdout, stderr, and Artifact access;
- SQLite Job and Attempt truth;
- idempotent admission and transactional concurrency reservations;
- one active command tree per Workspace;
- systemd/cgroup process-tree ownership;
- timeout, cancellation, terminal result persistence, and reconciliation;
- conservative handling of ambiguous dispatch, lost identity, and orphaned work.

The Runtime inherits the installed service user's host authority. It adds ownership, bounded execution, durable task truth, cancellation, and recovery without imposing an internal sandbox. Untrusted workloads require an external isolation boundary.

Build the runner with:

```bash
cargo build -p ordivon-exec --bin ordivon-task-runner
```

Real systemd/cgroup tests are opt-in:

```bash
ORDIVON_RUN_INTEGRATION=1 \
ORDIVON_RUNNER_PATH="$PWD/target/debug/ordivon-task-runner" \
cargo test -p ordivon-exec --test transactional_runtime -- --ignored --test-threads=1
```

Historical governance, lifecycle, migration, and alternate execution implementations remain available through Git tag `m-series-closed-2026-07-22`.
