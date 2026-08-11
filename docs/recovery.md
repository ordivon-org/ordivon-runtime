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
scripts/ordivon-runtime-status --health --json
scripts/ordivon-runtime-status --diagnose --json
```

`check` is portable and verifies that the repository still contains the local-only acceptance entry points. `run` requires the trusted local root/systemd/cgroup-v2 environment and executes the real systemd suites plus MCP end-to-end acceptance.

Inspect one durable Job or summarize Runtime-owned experience without mutation:

```text
ordivon-runtime-inspect job --database <registry> --job-id <job> --pretty
ordivon-runtime-inspect workspace --database <registry> --store-root <runtime-store> --workspace-id <workspace> --pretty
ordivon-runtime-inspect summary --database <registry> --since-ms <unix-ms> --pretty
```

The inspection binary reads the current Registry schema through the shared Rust Core. It never writes recovery decisions, creates a second trajectory database, or evaluates semantic task completion. Inspection is a bounded projection, not a full Registry-health ceremony: it uses one query-only snapshot, checks the supported migration version, and surfaces SQLite/query/decoding failures it encounters, while global `PRAGMA integrity_check` remains owned by Doctor and explicit repair/backup/restore validation.

Registry-medium failure is not evidence that a logical operation definitely did or did not commit. Admission, cancellation, and terminal write paths therefore classify the transaction outcome from durable readback after a COMMIT error. If the exact durable mutation is present, Runtime returns/reconciles the committed identity. If rollback/not-committed state can be proven, the operation remains retryable with the **same** request identity. `commitState=unknown` is reserved for the harder case where COMMIT outcome and subsequent rollback/readback cannot establish either side; the caller must reconcile before creating any new operation identity. The same fail-closed rule applies when the admission fence or Registry medium becomes read-only, full, or otherwise unavailable: no successful error response is allowed to imply a speculative second Job.

A Registry fault experiment must isolate the Registry/WAL/fence medium from the Workspace, source checkout, Attempt bundle, and trace storage. Making the entire Runtime state tree read-only or full is not a valid Registry-only experiment because failures in those other owners can mimic Registry behavior. After restoring the medium, exact request replay and `ordivon-runtime-doctor inspect --fail-on-violation` are the required convergence checks.

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
- A missing supervisor unit is interpreted together with persisted termination intent and recorded process identity. If the recorded process is gone, committed stop intent converges to `cancelled`, committed deadline intent converges to `timed_out`, and natural disappearance retains the generic lost/failure path. Concurrent cancel and generic reconciliation must not create two control terminal identities; the short control-result/evidence/terminal-commit section is serialized and Registry terminal identity remains the final arbiter.
- Restore writes into a new destination and never overwrites live state.
- Git restores code and documentation; Runtime snapshots restore durable execution truth.
