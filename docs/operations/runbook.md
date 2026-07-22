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
cargo build -p ordivon-exec --bin ordivon-task-runner
cargo run -p ordivon-mcp
```

Required environment variables are listed in `.env.example`. The server binds to loopback, requires a random bearer token, bounds request bodies, and exposes `/mcp`.
Registry and Store roots must be persistent paths visible inside the systemd service boundary; do not place them under `/tmp` when the Runner uses `PrivateTmp`.

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

## External maintenance helpers

```bash
python scripts/smoke.py --token "$ORDIVON_BEARER_TOKEN"
python scripts/cleanup.py --root /var/lib/ordivon/registry
python scripts/backup.py \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --control-root /var/lib/ordivon/registry \
  --destination /var/backups/ordivon/<snapshot>
```

Cleanup is dry-run unless `--apply` is supplied and only removes temporary staging names. Backup uses SQLite's online backup API; stop new dispatches first when a complete control-root snapshot is required.
