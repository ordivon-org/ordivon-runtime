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

The tool stages `.next` files, retains both receipt-local previous binaries and installed `.previous` binaries, stops admission, rechecks that no other active or held Job appeared, installs the candidate, waits for `active`, requires a 2026 `server/discover` lifecycle, verifies the exact tool catalog, and records the protocol lifecycle, supported versions, `toolCatalogDigest`, candidate digests, and installed digests. After a successful standard-layout deployment it keeps the current and immediately previous candidate build trees and removes older exact-commit candidate directories; rollback remains receipt-owned and does not depend on those build trees. A newly installed candidate may not pass through the legacy fallback. Recovery of an uncommitted replacement and explicit rollback probe modern discovery first but may use the previous service's 2025 `initialize` Session lifecycle, preserving old-binary rollback. If the post-stop race check fails before replacement, the original service is restarted and the receipt is `not_committed`; a failure after replacement automatically restores the receipt-local previous binaries. There is no general active-Job override in the normal deployment contract.

Rollback is explicit and receipt-bound:

```bash
receipt=/var/lib/ordivon/deployments/<deployment-receipt>

scripts/ordivon-runtime-deploy rollback \
  --receipt "$receipt" \
  --confirm-receipt "$receipt" \
  --install-dir /usr/local/libexec/ordivon \
  --env-file /etc/ordivon/ordivon-runtime.env
```

Rollback validates the receipt-bound install directory, service, environment file, binary set, and previous digests. It preserves the displaced current binaries inside the same receipt before restoring the previous set. If restoring the previous set fails, it attempts to restore the displaced current set and receipts both outcomes. Its MCP probe accepts either the modern lifecycle or the prior legacy Session lifecycle, because rollback must be able to prove a genuinely old binary rather than require it to implement the new protocol. Additive query indexes that do not change Registry semantics are maintained outside `schema_migrations`; this preserves previous-binary rollback compatibility while allowing the newer Runtime to recreate missing performance indexes idempotently.

### MCP probe module placement

`scripts/ordivon-runtime-deploy`, `scripts/ordivon-runtime-reclaim`, and `scripts/ordivon-runtime-capacity-acceptance` share `scripts/mcp_probe.py`; they no longer embed separate protocol clients. Run them from the repository as shown above, or install/copy all three files into the same directory. Installing either executable script without its sibling module is unsupported and fails before any mutation. Candidate deployment requires modern discovery; reclaim uses modern discovery first and falls back to legacy initialization only so that an installed previous Runtime can still release a Workspace through its own `workspace.close` contract.

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

The low-level reclaim command remains the only release executor. `ordivon-runtime-lifecycle` adds the installed retention policy without creating another Workspace database. It derives the retention basis from Workspace creation and the latest durable Job/Attempt activity and treats active or held Jobs as leases. The packaged policy defaults every Workspace identity—generated or readable—to `ephemeral` for 24 hours; only explicit exact/prefix rules promote selected identities to `review` or `pinned`. Naming is therefore no longer mistaken for retention intent.

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

## Shared execution caches

Runtime separates source isolation from reusable toolchain state:

```text
cache/shared/                    global package-download caches
cache/build/sources/<sha256>/    trusted-local build cache per canonical source repository
cache/build/<workspaceId>/       contained-local or legacy Workspace build cache
cache/workspaces/<workspaceId>/  Workspace generic cache and contained-local home/tooling
cache/tmp/<workspaceId>/         Workspace temporary files
```

The committed execution environment sets explicit paths for Cargo, uv, pip, npm, pnpm/Corepack, Bun, and Go. Trusted-local Jobs from different detached Workspaces of one source repository receive the same `CARGO_TARGET_DIR`; different repositories receive different directories. Package-download caches are global in trusted-local. Contained-local remains Workspace-scoped.

Inspect legacy build caches without mutation:

```bash
scripts/ordivon-runtime-cache inspect \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime --pretty
```

Migration is explicit, locked, and receipted. For each source repository it promotes the largest nonempty inactive legacy Workspace build cache with an atomic rename. Any source repository with an active or held Workspace is excluded. An absent or directory-only empty source target may be atomically replaced; a target containing cache files is never overwritten. Nonselected legacy caches are retained as fallbacks for later Workspace lifecycle cleanup.

```bash
scripts/ordivon-runtime-cache migrate \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --receipt-root /var/lib/ordivon/runtime/cache-receipts \
  --lock-file /run/ordivon-runtime-cache.lock \
  --confirm-policy MIGRATE_SHARED_EXECUTION_CACHES --pretty
```

Cache reclamation is capacity-driven rather than a blind age sweep. The packaged daily lifecycle service uses a 64 GiB high watermark and removes least-recently-modified reclaimable cache directories until 48 GiB. Open or active Workspace caches and source-build caches referenced by any open Workspace are protected. Global package-manager caches are measured but not interpreted or deleted by Runtime; mature package-manager-native pruning remains their owner.

```bash
scripts/ordivon-runtime-cache prune \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --runtime-store-root /var/lib/ordivon/runtime \
  --receipt-root /var/lib/ordivon/runtime/cache-receipts \
  --lock-file /run/ordivon-runtime-cache.lock \
  --high-watermark-bytes 68719476736 \
  --low-watermark-bytes 51539607552 \
  --confirm-policy PRUNE_EXECUTION_CACHES --pretty
```

## Capacity acceptance

`scripts/ordivon-runtime-capacity-acceptance` is the repeatable public-surface proof for global admission. It opens `N+1` isolated Workspaces, admits exactly `N` bounded sleep Jobs, requires the next Job to fail with `CONCURRENCY_LIMIT`, verifies the complete holder Job and Workspace sets, waits for every admitted Job to succeed, and closes every acceptance Workspace. It emits a digest-bound JSON receipt and never reads the Registry directly.

Run it from a host shell or independent systemd unit, not from a Runtime Job: a Runtime Job would itself consume one of the slots being measured.

```bash
set -a
source /etc/ordivon/ordivon-mcp.env
set +a

scripts/ordivon-runtime-capacity-acceptance \
  --source-repo /root/projects/ordivon-runtime \
  --source-revision "$(git -C /root/projects/ordivon-runtime rev-parse HEAD)" \
  --limit 8 \
  --receipt /var/lib/ordivon/runtime/evidence/runtime-capacity-live.json \
  --pretty
```

The command is parameterized rather than hard-coded to eight. Exact holder-set validation is bounded to `1..=16`, matching the bounded capacity error envelope.

## Secret-free status

`scripts/ordivon-runtime-status` separates operational health from bounded maintenance diagnosis. The default and `--health` path verifies the latest successful deployment receipt, installed binary digests, systemd state, allowlisted numeric Runtime configuration, and Registry/recovery consistency. It deliberately skips Git Workspace scans, recursive storage measurement, lifecycle receipts, and protocol-history analysis. `--diagnose` adds the receipted MCP lifecycle, selected and supported protocol versions, `toolCatalogDigest`, bounded current-plus-rotated protocol observations, the immediately previous rollback protocol, Workspace consistency, cache/Registry/deployment/candidate byte measurements, stale dirty Workspace counts, and the latest lifecycle receipt. It never opens the MCP endpoint and never reads or emits the bearer token.

```bash
# Fast health path; this is the default and is suitable for frequent automation.
scripts/ordivon-runtime-status
scripts/ordivon-runtime-status --health --json

# Explicit maintenance and compatibility diagnosis; this may scan large cache trees.
scripts/ordivon-runtime-status --diagnose --json
scripts/ordivon-runtime-status --diagnose --json \
  --expected-commit "$(git rev-parse HEAD)"
```

The JSON schema reports separate `health` and `maintenance` states. Exit code `1` is reserved for operational health failures such as service, deployment, Registry, recovery, or exact-binary inconsistency. Maintenance findings such as a stale dirty Workspace produce top-level `status=attention` but retain exit code `0`; automation must inspect `maintenanceAction` when maintenance policy should gate a workflow. Compatibility observations remain advisory deletion evidence: an unreadable or truncated trace, unknown rollback protocol, or incomplete observation window blocks deletion but does not create a health incident. Exit code `2` remains reserved for an invalid invocation or unreadable mandatory input.
## Contained-local acceptance

The contained profile has a root/systemd integration fixture that uses the real Runner and cgroup v2. It proves that an unmounted secret below host `/var` is invisible, network socket creation or connection is blocked, the Workspace remains writable, inherited credential variables are absent, the systemd/Runner cgroup identity remains consistent, and `terminal_evidence` reports a clean process tree with the supplied foreign reference.

```sh
cargo build -p ordivon-runtime-core --bin ordivon-runtime-runner
ORDIVON_RUN_INTEGRATION=1 \
ORDIVON_RUNNER_PATH="$CARGO_TARGET_DIR/debug/ordivon-runtime-runner" \
  cargo test -p ordivon-runtime-core --test transactional_runtime \
  contained_local_hides_unmounted_state_blocks_egress_and_preserves_evidence \
  -- --ignored --nocapture
```

A failed namespace setup is terminal `RUNNER_START_FAILED`; contained execution does not retry as trusted-local. `ProtectControlGroups=yes` is intentional: it keeps the host cgroup path readable but immutable so Runner-start identity, cancellation, restart recovery, and recursive residual checks refer to the same supervisor object. The stronger `strict` cgroup namespace is not used because it rewrites the Runner-visible path to `/` and destroys that identity invariant.
