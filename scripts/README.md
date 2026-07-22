# Operational scripts

These scripts are external utilities, not Runtime Core protocols.

- `smoke.py` performs one bounded MCP `initialize` request against a running loopback server.
- `cleanup.py` is dry-run by default and only targets stale `.*.staging-*` and `.*.tmp-*` entries. It never deletes Job, Attempt, workspace, result, or Artifact directories by name.
- `backup.py` uses Python's SQLite Backup API and optionally copies regular files from the control root into a new atomic snapshot directory. It rejects symlinks and writes SHA-256 metadata.

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
