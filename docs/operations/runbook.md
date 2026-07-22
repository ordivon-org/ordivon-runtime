# Ordivon Local Operations

Ordivon no longer requires PostgreSQL, NATS, Temporal, OpenFGA, OPA, Redis, DuckDB, or a Python governance service.

## Build and verify

```bash
cargo build --workspace
cargo fmt --check
cargo test --workspace
```

## Installable systemd entry

`packaging/systemd/ordivon-mcp.service` and `ordivon-mcp.env.example` define a loopback-only service without changing the currently active Desktop Commander bridge. Install and switch remote routing only during a separate rollback-capable maintenance window.

## Run the MCP server

Build the task runner and start the single current MCP entry:

```bash
cargo build -p ordivon-exec --bin ordivon-task-runner
cargo run -p ordivon-mcp
```

Required environment variables are listed in `.env.example`. The server binds to loopback, requires a random bearer token, bounds request bodies, and exposes `/mcp`.
Registry and Store roots must be persistent paths visible to systemd transient units. Do not place them under a private or ephemeral `/tmp` boundary.

## Execution modes

The Runtime defaults to `trusted-local`. It inherits the service user's host authority and environment. `ORDIVON_ALLOWED_EXECUTABLE_ROOTS` is optional; when omitted, trusted mode accepts any canonical executable path. `ORDIVON_PRINCIPAL` and `ORDIVON_GLOBAL_MAX_CONCURRENCY` configure server-owned execution identity and capacity.

Set `ORDIVON_EXECUTION_MODE=isolated` only when reduced authority is desired. The static `ordivon-worker` identity and isolated roots are required only in that mode.

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
python scripts/mcp_e2e.py --repo "$PWD"
python scripts/acceptance.py merge
python scripts/cleanup.py --root /var/lib/ordivon/registry
python scripts/backup.py \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --control-root /var/lib/ordivon/registry \
  --destination /var/backups/ordivon/<snapshot>
python scripts/restore.py \
  --snapshot /var/backups/ordivon/<snapshot> \
  --destination /var/lib/ordivon-restored
```

Cleanup is dry-run unless `--apply` is supplied and only removes temporary staging names. Backup uses SQLite's online backup API; stop new dispatches first when a complete control-root snapshot is required.
