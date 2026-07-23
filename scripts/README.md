# Operational scripts

These are direct utilities, not Runtime protocols:

- `smoke.py` initializes a running loopback MCP server.
- `mcp_e2e.py` proves all ten tools, cancellation, restart observation, and Artifact Digests.
- `cleanup.py` removes only stale staging and temporary entries; dry-run is the default.
- `backup.py` takes an atomic SQLite snapshot and copies permitted regular control files.
- `restore.py` verifies every Digest and restores into a new destination; it never overwrites live state.

```bash
python scripts/smoke.py --token "$ORDIVON_BEARER_TOKEN"
python scripts/mcp_e2e.py --repo "$PWD"
python scripts/cleanup.py --root /var/lib/ordivon/registry
python scripts/backup.py \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --control-root /var/lib/ordivon/registry \
  --destination /var/backups/ordivon/<snapshot>
python scripts/restore.py \
  --snapshot /var/backups/ordivon/<snapshot> \
  --destination /var/lib/ordivon-restored
```

Git remains the recovery mechanism for code and documentation.
