# Ordivon Runbook

## Build and verify

```bash
cargo build --workspace
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Run strict Clippy, real systemd/cgroup tests, full-history secret scanning, and Release acceptance only when the changed boundary requires them.

## Start the Rust MCP

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

`trusted-local` is the default. Executable roots default to `/`, so the service user may run any accessible executable. Set `ORDIVON_EXECUTION_MODE=isolated` only for an explicitly reduced-authority task and provide the isolated worker roots and UID/GID.

The installable unit and environment example are under `packaging/systemd/`. The server remains loopback-bound and bearer-authenticated; Cloudflare Access and Tunnel authenticate and transport remote calls.

## Current live topology

```text
mcp.ordivon.com
→ Cloudflare Tunnel
→ localhost:8811
→ Supergateway
→ Desktop Commander
```

This live route is not the Rust candidate.

## Local adoption sequence

1. install `ordivon-mcp` and `ordivon-task-runner` at fixed paths;
2. start Rust MCP on `127.0.0.1:8897` without changing 8811;
3. run initialize, full MCP E2E, cancellation, restart observation, and backup/restore;
4. use it on real repositories and record only repeated missing capabilities;
5. expose a separate Cloudflare test route;
6. switch production only after authenticated full-authority operation is proven;
7. retain the old bridge until rollback is no longer needed.

## Recovery

### Code

Use ordinary Git status, diff, branches, worktrees, tags, and remote Push. The complete pre-simplification tree remains at tag `m-series-closed-2026-07-22`.

### Running tasks

Use `task.list`, `task.observe`, and `task.cancel`. Restarting the MCP should reconcile nonterminal Attempts from SQLite, result bundles, and systemd state.

### Persistent Runtime state

Stop new dispatches before a coordinated snapshot. Use:

```bash
python scripts/backup.py \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --control-root /var/lib/ordivon/registry \
  --destination /var/backups/ordivon/<snapshot>

python scripts/restore.py \
  --snapshot /var/backups/ordivon/<snapshot> \
  --destination /var/lib/ordivon-restored
```

Cleanup is dry-run unless `--apply` is supplied. Do not create a second recovery Registry around Git, SQLite, or systemd.

## Inspect exact state

```bash
git rev-parse HEAD
git status --short --branch
systemctl status ordivon-mcp.service
systemctl status ordivon-mcp-bridge.service
ss -ltnp
```
