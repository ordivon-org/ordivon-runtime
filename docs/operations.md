# Runtime Operations

Operational tools are narrow wrappers around existing truth owners. They do not create a second deployment database, scheduler, or Workspace lifecycle service.

## Local deployment

`scripts/ordivon-runtime-deploy` replaces the ad hoc deployment shell used during development. It has four commands:

```text
prepare   build one exact clean Commit and write a digest-bound candidate manifest
plan      read-only eligibility and digest report
apply     lock, retain previous binaries, install, restart, probe, receipt
rollback  restore the exact previous binaries from one receipt
```

A normal production deployment is:

```bash
repo=$(git rev-parse --show-toplevel)
commit=$(git -C "$repo" rev-parse HEAD)
manifest="$repo/target/release/ordivon-deployment-manifest.json"
cargo=$(command -v cargo)
test -x "$cargo"

scripts/ordivon-runtime-deploy prepare \
  --source-repo "$repo" \
  --commit "$commit" \
  --candidate-dir "$repo/target/release" \
  --candidate-manifest "$manifest" \
  --cargo "$cargo"

scripts/ordivon-runtime-deploy plan \
  --source-repo "$repo" \
  --commit "$commit" \
  --candidate-dir "$repo/target/release" \
  --candidate-manifest "$manifest" \
  --install-dir /usr/local/libexec/ordivon \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/deployments \
  --expected-tool-count 13 \
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
  --receipt-root /var/lib/ordivon/deployments \
  --expected-tool-count 13
```

Eligibility requires:

- the source repository `HEAD` and `origin/main` equal the exact requested commit;
- the source repository is clean;
- `prepare` proves the source is clean both before and after the build;
- the candidate manifest binds the exact source repository, Commit, candidate directory, binary set, sizes, and digests;
- every candidate and currently installed binary is a regular executable file;
- no active or held Job exists, except the deployment Job itself when the tool can prove its own Attempt identity from cgroup and Registry state;
- the confirmation commit exactly matches the requested commit.

The tool stages `.next` files, retains both receipt-local previous binaries and installed `.previous` binaries, stops admission, rechecks that no other active or held Job appeared, installs the candidate, waits for `active`, initializes MCP, verifies the tool catalog, and records candidate and installed digests. If the post-stop race check fails before replacement, the original service is restarted and the receipt is `not_committed`; a failure after replacement automatically restores the receipt-local previous binaries. There is no general active-Job override in the normal deployment contract.

Rollback is explicit and receipt-bound:

```bash
receipt=/var/lib/ordivon/deployments/<deployment-receipt>

scripts/ordivon-runtime-deploy rollback \
  --receipt "$receipt" \
  --confirm-receipt "$receipt" \
  --install-dir /usr/local/libexec/ordivon \
  --env-file /etc/ordivon/ordivon-runtime.env
```

Rollback validates the receipt-bound install directory, service, environment file, binary set, and previous digests. It preserves the displaced current binaries inside the same receipt before restoring the previous set. If restoring the previous set fails, it attempts to restore the displaced current set and receipts both outcomes. Additive query indexes that do not change Registry semantics are maintained outside `schema_migrations`; this preserves previous-binary rollback compatibility while allowing the newer Runtime to recreate missing performance indexes idempotently.

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


### Policy-driven lifecycle

The low-level reclaim command remains the only release executor. `ordivon-runtime-lifecycle` adds the installed retention policy without creating another Workspace database. It derives the retention basis from Workspace creation and the latest durable Job/Attempt activity, treats active or held Jobs as leases, and classifies server-generated `ws-*` handles as `ephemeral`, explicit handles as `review`, or configured identities as `pinned`.

```bash
scripts/ordivon-runtime-lifecycle inspect \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --policy-file /etc/ordivon/workspace-retention.json \
  --measure-bytes --pretty

scripts/ordivon-runtime-lifecycle sweep \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --policy-file /etc/ordivon/workspace-retention.json \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/runtime/lifecycle-receipts \
  --lock-file /run/ordivon-runtime-lifecycle.lock \
  --confirm-policy APPLY_WORKSPACE_RETENTION_POLICY --pretty
```

The packaged timer runs this sweep daily with a randomized delay. It can only select policy-expired `closable` and `stale_record` entries; dirty, active, pinned, unknown, and orphan-directory cases remain excluded. The subordinate reclaim receipt is linked from the lifecycle receipt.

Repository renames can leave a healthy worktree registered in the new Git repository while its `.git` file and Runtime record still name the old path. The repair command accepts exact `sourceRepoAliases`, verifies the recorded commit in the mapped repository, runs `git worktree repair`, rechecks the exact HEAD, and updates only the record's source repository. Unrepairable data may be moved atomically to a quarantine directory and replaced by a valid closed tombstone only with a second explicit confirmation; bytes are preserved rather than deleted.

```bash
scripts/ordivon-runtime-lifecycle repair \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --policy-file /etc/ordivon/workspace-retention.json \
  --receipt-root /var/lib/ordivon/runtime/lifecycle-receipts \
  --lock-file /run/ordivon-runtime-lifecycle.lock \
  --confirm-policy REPAIR_WORKSPACE_IDENTITIES --pretty
```

### Installation hygiene

The one-time pre-release installation hygiene pass is complete and remains available through Git history and its receipts rather than as a permanent Runtime subsystem. Target commands do not inherit the Runtime service environment. `ORDIVON_EXEC_PATH` and `ORDIVON_EXEC_HOME` explicitly retain trusted root toolchains, while request `env` values remain the only per-operation overrides. The trusted-local root authority model remains unchanged.

## Secret-free status

`scripts/ordivon-runtime-status` combines the latest successful deployment receipt, installed binary digests, systemd state, allowlisted numeric Runtime configuration, Registry summaries, Workspace consistency, and the latest lifecycle receipt. It never opens the MCP endpoint and never reads or emits the bearer token.

```bash
scripts/ordivon-runtime-status
scripts/ordivon-runtime-status --json
scripts/ordivon-runtime-status --json --expected-commit "$(git rev-parse HEAD)"
```

The command returns `0` for `healthy`, `1` when bounded facts require operator attention, and `2` for an invalid invocation or unreadable mandatory input. Output contains receipt identifiers and binary names, but not source paths, Workspace paths, command arguments, arbitrary environment values, or secret material. The deployment receipt remains the build-identity source of truth; the status command does not introduce another database or marker.
