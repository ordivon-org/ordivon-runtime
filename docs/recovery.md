# Ordivon Runtime Recovery

This protocol is for Registry inconsistency, ambiguous terminal state, backup, administrative repair, or restore. Ordinary task failure should be corrected through normal source, tests, Git, and Runtime operation.

## Recovery sequence

1. **Control new dispatches.** Prevent new work from changing the Registry state being diagnosed.
2. **Inspect without writing.** Run `ordivon-runtime-doctor inspect` and preserve its complete report and fingerprint.
3. **Create a fresh verified snapshot.** Back up the Registry and permitted control files before any administrative mutation.
4. **Classify every case from evidence.** A validated runner Result may support recovery. A missing Result does not prove success or failure.
5. **Apply an exact repair.** `ordivon-runtime-repair apply` must use the Doctor fingerprint, the verified snapshot, an explicit principal, and explicit selection of every manual Lost conclusion. Invocation without `--apply` must remain non-mutating.
6. **Verify after mutation.** Run the Doctor again, check SQLite integrity and remaining invariants, then restart or inspect the service as required by the incident.
7. **Preserve recovery evidence.** Keep the before report, snapshot identity, repair report, after report, and relevant Runtime Artifacts outside the active repository tree.

Use the executable local acceptance contract after changes that affect systemd, cgroups, dispatch, cancellation, recovery, or MCP execution:

```text
scripts/local-acceptance check
scripts/local-acceptance run
```

`check` is portable and verifies that the repository still contains the local-only acceptance entry points. `run` requires the trusted local root/systemd/cgroup-v2 environment and executes the real systemd suites plus MCP end-to-end acceptance.

Inspect Workspace lifecycle without mutation with:

```text
ordivon-runtime-reclaim inspect --database <registry> --runtime-store-root <runtime> --measure-bytes --pretty
```

The report identifies `workspace.close` as the release mechanism. Do not delete Workspace directories directly or add a second reclaim apply path.

Use each command's `--help` as the current argument contract:

```text
ordivon-runtime-doctor --help
ordivon-runtime-repair --help
python scripts/backup.py --help
python scripts/restore.py --help
```

## Non-negotiable rules

- Repair does not implicitly migrate the Registry schema.
- A stale fingerprint, changed row version, invalid snapshot, Digest mismatch, or unselected manual case must abort the repair.
- The repair batch must be atomic; a late conflict must not leave partial Registry mutation.
- Missing runner evidence remains an explicit Lost conclusion rather than an inferred process outcome.
- Restore writes into a new destination and never overwrites live state.
- Git restores code and documentation; Runtime snapshots restore durable execution truth.
