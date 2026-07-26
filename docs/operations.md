# Runtime Operations

Operational tools are narrow wrappers around existing truth owners. They do not create a second deployment database, scheduler, or Workspace lifecycle service.

## Local deployment

`scripts/ordivon-runtime-deploy` replaces the ad hoc deployment shell used during development. It has three commands:

```text
plan      read-only eligibility and digest report
apply     lock, retain previous binaries, install, restart, probe, receipt
rollback  restore the exact previous binaries from one receipt
```

A normal production deployment is:

```bash
commit=$(git rev-parse HEAD)
manifest="$PWD/target/release/ordivon-deployment-manifest.json"

scripts/ordivon-runtime-deploy prepare \
  --source-repo "$PWD" \
  --commit "$commit" \
  --candidate-dir "$repo/target/release" \
  --candidate-manifest "$manifest" \
  --cargo /root/.cargo/bin/cargo

scripts/ordivon-runtime-deploy plan \
  --source-repo "$repo" \
  --commit "$commit" \
  --candidate-dir "$repo/target/release" \
  --candidate-manifest "$manifest" \
  --install-dir /usr/local/libexec/ordivon \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/runtime/deployments \
  --expected-tool-count 14 \
  --pretty

scripts/ordivon-runtime-deploy apply \
  --source-repo "$repo" \
  --commit "$commit" \
  --confirm-commit "$commit" \
  --candidate-dir "$repo/target/release" \
  --candidate-manifest "$manifest" \
  --install-dir /usr/local/libexec/ordivon \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/runtime/deployments \
  --expected-tool-count 14
```

Eligibility requires:

- the source repository `HEAD` and `origin/main` equal the exact requested commit;
- the source repository is clean;
- the candidate directory is inside the exact verified source worktree, and every candidate and currently installed binary is a regular executable file;
- no active or held Job exists, except the deployment Job itself when the tool can prove its own Attempt identity from cgroup and Registry state;
- the confirmation commit exactly matches the requested commit.

The tool stages `.next` files, retains both receipt-local previous binaries and installed `.previous` binaries, restarts the service, waits for `active`, initializes MCP, verifies the tool catalog, and records candidate and installed digests. A failed service or MCP probe automatically restores the receipt-local previous binaries. There is no general active-Job override in the normal deployment contract.

Rollback is explicit and receipt-bound:

```bash
receipt=/var/lib/ordivon/runtime/deployments/<deployment-receipt>

scripts/ordivon-runtime-deploy rollback \
  --receipt "$receipt" \
  --confirm-receipt "$receipt" \
  --install-dir /usr/local/libexec/ordivon \
  --env-file /etc/ordivon/ordivon-runtime.env
```

Rollback validates the receipt-bound install directory, service, environment file, binary set, and previous digests. It preserves the displaced current binaries inside the same receipt before restoring the previous set. If restoring the previous set fails, it attempts to restore the displaced current set and receipts both outcomes.

## Workspace lifecycle and reclaim

Workspace lifecycle has separate states and release rules:

| Classification | Meaning | Automatic action |
| --- | --- | --- |
| `blocked_active` | an unresolved Job or active/held reservation exists | never |
| `blocked_dirty` | tracked, staged, deleted, or untracked state exists | never |
| `unknown` | identity, metadata, or Git health cannot be proven | never |
| `orphan_directory` | directory exists without an identity record | never |
| `stale_record` | open record exists but the physical Workspace is absent | old record may be archived and deleted |
| `closable` | Workspace is healthy, clean, and has no active Job | old Workspace may be released through `workspace.close` |

Inspection remains read-only:

```bash
scripts/ordivon-runtime-reclaim inspect \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --measure-bytes \
  --pretty
```

Conservative apply uses a minimum age, an exact policy confirmation, a process lock, and receipts:

```bash
scripts/ordivon-runtime-reclaim apply \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/runtime/reclaim-receipts \
  --lock-file /run/ordivon-runtime-reclaim.lock \
  --minimum-age-hours 168 \
  --classification stale_record \
  --classification closable \
  --confirm-policy RECLAIM_ELIGIBLE_WORKSPACES \
  --pretty
```

The default policy is seven days and includes only `stale_record` and `closable`. A `stale_record` candidate carries a digest from inspection; apply requires the record to remain a regular file with the same digest, rechecks Workspace absence and active Jobs, copies it into the receipt, and only then deletes it. `closable` Workspaces are never removed directly; the tool calls the Runtime's `workspace.close` contract with `force=false`, preserving active-Job exclusion, dirty-state refusal, rescue refs, tombstones, and idempotency.

The apply command is a policy executor, not a timer. Scheduling is intentionally separate because retention age and cadence are user policy. Failed items are recorded independently and produce a partial result instead of hiding successful actions or deleting a broader set.
