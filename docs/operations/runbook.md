# Ordivon Local Operations

Ordivon no longer requires PostgreSQL, NATS, Temporal, OpenFGA, OPA, Redis, DuckDB, or a Python governance service.

## Build and verify

```bash
cargo build --workspace
cargo fmt --check
cargo test --workspace
```

## Run the MCP server

Build the task runner and start the single current MCP entry:

```bash
cargo build -p ordivon-exec --bin ordivon-task-runner --features universal-executor-m1
cargo run -p ordivon-mcp
```

Required environment variables are listed in `.env.example`. The server binds to loopback, requires a random bearer token, bounds request bodies, and exposes `/mcp`.

## Execution modes

The runtime defaults to `trusted-local`. Only the common store, Registry, runner, executable roots, loopback bind, and bearer token are required.

Set `ORDIVON_EXECUTION_MODE=isolated` for untrusted repositories or scripts. The static `ordivon-worker` identity and systemd directory definitions remain under `packaging/systemd/`; worker UID/GID and isolated roots are required only in this mode.

## Inspect repository state

```bash
git status --short --branch
git rev-parse HEAD
git diff --stat
```

## Recover code or documentation

Use ordinary Git recovery. The complete pre-simplification M-series tree is tagged as:

```text
m-series-closed-2026-07-22
```

## Persistent runtime state

SQLite Registry and result files are the execution truth. Before manual maintenance, stop new dispatches and copy the control root with the SQLite backup API or `sqlite3 .backup`. Backup is an external operation, not a Runtime Core protocol.
