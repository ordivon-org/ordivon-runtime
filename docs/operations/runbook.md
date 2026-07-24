# Ordivon Runbook

## Build and verify

```bash
cargo build --workspace
cargo fmt --check
PROPTEST_CASES=512 cargo test --workspace
ruff check scripts/
```

Run strict Clippy, real systemd/cgroup tests, MCP E2E, or full-history secret scanning only when the changed boundary requires them.

## Install and start

Build the service binaries and the local-only administrative binaries:

```bash
cargo build -p ordivon-exec \
  --bin ordivon-task-runner \
  --bin ordivon-runtime-doctor \
  --bin ordivon-runtime-repair
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

## Local Runtime acceptance

Run these only when process ownership, cancellation, persistence, transport, or deployment changes:

```bash
cargo build -p ordivon-exec \
  --bin ordivon-task-runner \
  --bin ordivon-runtime-doctor \
  --bin ordivon-runtime-repair
ORDIVON_RUN_SYSTEMD_SPIKE=1 \
  cargo test -p ordivon-exec --features systemd-supervisor \
  --test systemd_supervisor -- --ignored --test-threads=1
ORDIVON_RUN_INTEGRATION=1 \
ORDIVON_RUNNER_PATH="$PWD/target/debug/ordivon-task-runner" \
  cargo test -p ordivon-exec --test transactional_runtime \
  -- --ignored --test-threads=1
python scripts/mcp_e2e.py --repo "$PWD"
```

Before a release, also run the official MCP Inspector against an isolated candidate server and use a short temporary `cargo-fuzz` harness for public request deserialization and validation. Keep product fixes and regression cases; remove the temporary fuzz project instead of adding another permanent test subsystem.

Temporary filesystem roots are valid in trusted-local mode because the Runner does not use `PrivateTmp`.

## Running tasks

Use newest-first `task.list`, `task.observe`, and `task.cancel`. For repeated observation, pass the returned stdout/stderr next offsets to receive only newly retained text; omit offsets to keep tail mode. Restarting the MCP reconciles nonterminal Attempts and held orphan reservations from SQLite, result bundles, and systemd/cgroup state.

`workspace.close` rejects tracked or untracked changes by default. Use `force=true` only after the work is committed, adopted, backed up, or deliberately discarded. Active or held Jobs always block closure.

Execution capacity is shared across all clients. Different Workspaces may run concurrently. A second `workspace.exec` against the same Workspace receives a retryable `CONCURRENCY_LIMIT`; do not create a second Registry or server to bypass it.

## Runtime Doctor

Inspect Registry integrity and cross-table Runtime invariants without applying migrations or writing state:

```bash
ordivon-runtime-doctor inspect \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --store-root /var/lib/ordivon/registry \
  --pretty
```

The report fingerprints the exact Job, Attempt, reservation, and validated Bundle evidence used by a repair plan. `recoverRunnerResult` means the existing Runner result passed the same identity and Artifact checks used by ordinary reconciliation. `manualReview` is deliberately non-actionable until additional evidence is supplied. Use `--fail-on-violation` in diagnostics that should exit with status `2` after printing the report.

### Applying an audited Runtime repair

Runtime repair is a separate administrative binary and is never exposed as an MCP tool. Create and verify a fresh backup first, then use the exact Doctor fingerprint. Every manual Lost conclusion must be named explicitly:

```bash
ordivon-runtime-repair apply \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --store-root /var/lib/ordivon/registry \
  --expected-fingerprint sha256:... \
  --snapshot /var/backups/ordivon/<snapshot> \
  --principal runtime-admin:<operator> \
  --finalize-lost attempt-... \
  --apply --pretty
```

The command refuses stale fingerprints, invalid backup manifests, Digest mismatch, changed row versions, unselected manual-review cases, and invocation without `--apply`. A verified Runner result may correct only `Lost` or `Orphaned` into a Runner terminal state. A missing Runner result remains `Lost` and receives an explicit administrative receipt rather than an inferred process outcome.

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
