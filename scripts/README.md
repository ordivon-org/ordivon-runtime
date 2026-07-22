# Operational scripts

These scripts are external utilities, not Runtime Core protocols.

- `smoke.py` performs one bounded MCP `initialize` request against a running loopback server.
- `mcp_e2e.py` launches the current server and proves all nine tools, cancellation, restart observation, and Artifact digest verification.
- `acceptance.py` runs layered `fast`, `merge`, or explicit `release` acceptance and writes an exact-head JSON manifest.
- `legacy_reference_scan.py` rejects active tracked references to removed runtime entry points.
- `tracked_secret_scan.py` exports and scans the exact tracked Git tree without build artifacts or old-history noise.
- `benchmark.py` records a bounded exact-head release-build and loopback-initialize snapshot without extrapolating to execution workloads.
- `cleanup.py` is dry-run by default and only targets stale `.*.staging-*` and `.*.tmp-*` entries. It never deletes Job, Attempt, workspace, result, or Artifact directories by name.
- `backup.py` uses Python's SQLite Backup API and optionally copies regular files from the control root into a new atomic snapshot directory. It rejects symlinks and writes SHA-256 metadata.
- `restore.py` verifies every snapshot record and restores into a new atomically committed directory; it never overwrites a live root.

Examples:

```bash
python scripts/smoke.py --token "$ORDIVON_BEARER_TOKEN"
python scripts/cleanup.py --root /var/lib/ordivon/registry
python scripts/cleanup.py --root /var/lib/ordivon/registry --apply
python scripts/backup.py \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --control-root /var/lib/ordivon/registry \
  --destination /var/backups/ordivon/2026-07-22
```

Stop new dispatches before taking an operational backup. Git remains the recovery mechanism for repository code and documentation.

Acceptance examples:

```bash
python scripts/acceptance.py fast
python scripts/acceptance.py merge
python scripts/acceptance.py release --release-opt-in
python scripts/mcp_e2e.py --repo "$PWD"
python scripts/benchmark.py --repo "$PWD"
python scripts/restore.py --snapshot /var/backups/ordivon/<snapshot> --destination /var/lib/ordivon-restored
```
