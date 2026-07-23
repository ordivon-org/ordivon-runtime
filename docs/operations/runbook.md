# Ordivon Runbook

## Build and verify

```bash
cargo build --workspace
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Run strict Clippy, real systemd/cgroup tests, MCP E2E, or full-history secret scanning only when the changed boundary requires them.

## Install and start

Build both binaries:

```bash
cargo build -p ordivon-exec --bin ordivon-task-runner
cargo build -p ordivon-mcp
```

Required configuration:

```text
ORDIVON_BIND
ORDIVON_BEARER_TOKEN
ORDIVON_STORE_ROOT
ORDIVON_REGISTRY_ROOT
ORDIVON_RUNNER_PATH
```

Executable roots default to `/`; the service user may run any accessible executable. The server must remain loopback-bound. Local callers use the fixed Bearer token. Set `ORDIVON_TRUST_CF_ACCESS=true` only when Cloudflare Access protects the remote hostname.

## Live topology

```text
mcp.ordivon.com
→ Cloudflare Access and Tunnel
→ 127.0.0.1:8897
→ ordivon-mcp
→ ordivon-exec
```

There is no Ordivon rollback bridge on port 8811. The independent Docker MCP listener on `127.0.0.1:18812` is outside Ordivon.

## Running tasks

Use `task.list`, `task.observe`, and `task.cancel`. Restarting the MCP reconciles nonterminal Attempts and held orphan reservations from SQLite, result bundles, and systemd/cgroup state.

Execution capacity is shared across all clients. Different Workspaces may run concurrently. A second `workspace.exec` against the same Workspace receives a retryable `CONCURRENCY_LIMIT`; do not create a second Registry or server to bypass it.

## Backup and restore

Use ordinary Git for repository recovery. The pre-simplification implementation remains at tag `m-series-closed-2026-07-22`.

Stop new dispatches before a coordinated Runtime snapshot:

```bash
python scripts/backup.py \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --control-root /var/lib/ordivon/registry \
  --destination /var/backups/ordivon/<snapshot>

python scripts/restore.py \
  --snapshot /var/backups/ordivon/<snapshot> \
  --destination /var/lib/ordivon-restored
```

Cleanup is dry-run unless `--apply` is supplied.

## Inspect exact state

```bash
git rev-parse HEAD
git status --short --branch
systemctl status ordivon-mcp.service
ss -ltnp | grep 8897
```
