---
schema_version: 1
id: runtime.operations
title: Runtime Operations
type: operations
profile: engineering
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - operator
  - builder
  - agent
updated: 2026-08-05
summary: Canonical deployment, health, capacity, Workspace lifecycle, reclaim, rollback, and operational verification contract.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.model
  - runtime.recovery
  - runtime.authority
---
<!-- cspell:words aria2c hyperfine libexec nocapture nonselected rclone rsync toplevel -->

# Runtime Operations

## Scope

This document owns the operational path for local deployment, health inspection, capacity acceptance, Workspace lifecycle, reclaim, cache hygiene, rollback, and contained-local acceptance.

Runtime operations stop at physical execution and Workspace state. They do not assess Host Task completion, resume a Harness Run, invoke a Provider, or decide whether domain evidence is sufficient. Use Host operations for Journal/CAS and Task reconciliation, and Harness operations for Assignment/Run recovery.

## Normal operation

Operate through the receipted service, bounded health path, configured lifecycle timers, explicit Workspace ownership, and the canonical deployment scripts described below. Frequent automation should use the fast health path rather than maintenance scans.

## Failure detection

Treat unhealthy service state, unresolved Registry conditions, held reservations, orphan directories, dirty or active Workspaces, digest mismatches, failed cache cleanup, and deployment-receipt mismatch as explicit operational signals rather than inferred success.

## Recovery

Recover through Runtime reconciliation, documented repair or quarantine paths, explicit Workspace close/reclaim operations, and receipt-bound Git rollback. Never delete live state or redispatch ambiguous work as a substitute for diagnosis.

## Verification

Use service status, Runtime doctor and inspect commands, capacity acceptance, lifecycle receipts, contained-local acceptance, deployment manifests, and the repository test suite. The architecture is defined in [`runtime.md`](runtime.md), focused repair guidance is in [`recovery.md`](recovery.md), and exact command sequences and acceptance conditions follow in the detailed sections.

Operational tools are narrow wrappers around existing truth owners. They do not create a second deployment database, scheduler, or Workspace lifecycle service.

## Prefer mature host utilities

Do not recreate mature file transfer, structured-data filtering, archive, media, PDF, database, GitHub, or benchmarking behavior in one-off Python or shell scripts. Use the installed host utility through `workspace.exec` or `workspace.execPlan` with an absolute executable and explicit arguments. Runtime owns admission, execution, observation, cancellation, evidence, and process state; the utility remains responsible for its own operation semantics.

The canonical Arch WSL workstation currently provisions the following additional utilities:

```bash
pacman -S --needed aria2 rsync yq hyperfine rclone
```

These packages are host capabilities, not mandatory Runtime server dependencies and not additional MCP Tools. Query their live paths and versions before a version-sensitive operation rather than copying version claims into durable documentation.

| Need | Preferred utility | Operational rule |
| --- | --- | --- |
| repository and text search | `/usr/bin/rg`, `/usr/bin/fd` | Prefer them over recursive `grep` and complex `find` expressions. |
| JSON and YAML inspection | `/usr/bin/jq`, `/usr/bin/yq` | Use jq-style queries. Do not rewrite comment- or formatting-sensitive YAML with `yq`; use a parser or an exact text patch. |
| ordinary API calls and small probes | `/usr/bin/curl` | Keep API requests and bounded health probes on `curl`; do not substitute a download manager for protocol-aware application calls. |
| large or interruption-prone HTTP downloads | `/usr/bin/aria2c` | Enable continuation and write to an explicit destination. Prefer this over custom retry loops and partial-file scripts. |
| local or SSH directory copying | `/usr/bin/rsync` | Preserve trailing-slash intent. Use `--dry-run` before any deletion-capable invocation; never add `--delete` by default. |
| remote or object-storage transfer | `/usr/bin/rclone` | Start with `copy` and verify with `check`. Treat `sync` as deletion-capable and require explicit target semantics. Installation does not configure a remote. |
| archive inspection and creation | `/usr/bin/7z`, `/usr/bin/bsdtar`, `/usr/bin/zstd` | Prefer mature format support over new `zipfile` or `tarfile` scripts unless product code genuinely requires a library API. |
| repeatable command benchmarks | `/usr/bin/hyperfine` | Use warmups and multiple runs; use `--shell=none` when shell behavior is not part of the measurement. |
| media inspection | `/usr/bin/ffprobe` | Request JSON output instead of parsing human-oriented FFmpeg logs. |
| PDF inspection and text extraction | `/usr/bin/qpdf`, `/usr/bin/pdfinfo`, `/usr/bin/pdftotext` | Use `qpdf --check` for structural validity and the Poppler tools for metadata or text. |
| SQLite and GitHub operations | `/usr/bin/sqlite3`, `/usr/bin/gh` | Prefer direct read-only queries and official CLI operations over temporary Python or REST wrappers. |

Typical invocations are intentionally thin:

```bash
/usr/bin/aria2c --continue=true --dir "$destination" --out "$name" "$url"
/usr/bin/rsync -aH --partial --info=progress2 "$source/" "$destination/"
/usr/bin/yq -r '.runtime.preferred' configuration.yaml
/usr/bin/hyperfine --warmup 3 --runs 10 --shell=none '/absolute/command'
/usr/bin/rclone copy "$source" remote:path
/usr/bin/rclone check "$source" remote:path
```

Do not add a Runtime wrapper or MCP projection merely to rename one of these commands. Add a narrow adapter only after repeated real failures show that callers cannot safely express the operation through explicit arguments, or when structured effect semantics must be enforced rather than merely documented.

## Local deployment

`scripts/ordivon-runtime-deploy` replaces the ad hoc deployment shell used during development. It has four commands:

```text
prepare   materialize one exact Commit and write a digest-bound release manifest
plan      read-only eligibility and installed-artifact report
apply     lock, retain previous artifacts, install, restart, probe, receipt
rollback  restore the exact previous artifact set from one receipt
```

A normal production deployment is:

```bash
repo=$(git rev-parse --show-toplevel)
commit=$(git -C "$repo" rev-parse HEAD)
candidate="$repo/target/ordivon-release-candidates/$commit/release"
manifest="$candidate/ordivon-deployment-manifest.json"
cargo=$(command -v cargo)
test -x "$cargo"

scripts/ordivon-runtime-deploy prepare \
  --source-repo "$repo" \
  --commit "$commit" \
  --candidate-dir "$candidate" \
  --candidate-manifest "$manifest" \
  --cargo "$cargo"

scripts/ordivon-runtime-deploy plan \
  --source-repo "$repo" \
  --commit "$commit" \
  --candidate-dir "$candidate" \
  --candidate-manifest "$manifest" \
  --install-dir /usr/local/libexec/ordivon \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/deployments \
  --expected-tool-count 15 \
  --pretty

scripts/ordivon-runtime-deploy apply \
  --source-repo "$repo" \
  --commit "$commit" \
  --confirm-commit "$commit" \
  --candidate-dir "$candidate" \
  --candidate-manifest "$manifest" \
  --install-dir /usr/local/libexec/ordivon \
  --database /var/lib/ordivon/registry/registry.sqlite3 \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --receipt-root /var/lib/ordivon/deployments \
  --expected-tool-count 15
```

The default production release has one artifact authority rather than separate binary and operator-script installations. It contains five Rust binaries, the six installed Runtime operator executables (`deploy`, `lifecycle`, `reclaim`, `cache`, `status`, and `capacity-acceptance`), and their shared `mcp_probe.py` support module. Every artifact is bound by name, kind, byte length, SHA-256 digest, and canonical mode (`0755` for executable artifacts and `0644` for `mcp_probe.py`). Passing explicit `--binary` values intentionally selects a binary-only subset for focused testing or exceptional maintenance; it is not the canonical production release set.

Eligibility requires:

- the exact requested Commit can be materialized from the source repository;
- `prepare` constructs a temporary detached Git checkout of that exact Commit and both builds binaries and stages operator/support artifacts from that checkout, so mutable checkout state, including Git index flags such as `assume-unchanged`, cannot enter the candidate;
- the detached release source remains clean after the build;
- the explicit required ref (by default `origin/main`) resolves to the requested Commit; local `HEAD` and dirty state remain visible in the plan as diagnostics but do not become release authority;
- the candidate manifest binds the exact source repository, Commit, `sourceMaterialization=detached_git_checkout`, candidate directory, complete release artifact set, modes, sizes, and digests;
- the candidate manifest binds the Cargo/rustc invocation paths, their resolved launcher SHA-256 digests, version output, and Rust host target; proxy invocation paths such as rustup's `cargo`/`rustc` symlinks are preserved during the build;
- `plan` verifies the complete candidate-manifest digest before installation eligibility is reported;
- every candidate artifact and currently installed artifact is a regular file and every executable-mode artifact is executable;
- no active or held Job exists, except the deployment Job itself when the tool can prove its own Attempt identity from cgroup and Registry state;
- the confirmation commit exactly matches the requested commit.

The tool stages `.next` files for the complete release artifact set, retains both receipt-local previous artifacts and installed `.previous` artifacts with their observed modes, stops admission, rechecks that no other active or held Job appeared, atomically replaces the set, waits for `active`, requires a 2026 `server/discover` lifecycle, verifies the exact tool catalog, and records the candidate-manifest digest, build toolchain identity, protocol lifecycle, supported versions, `toolCatalogDigest`, and candidate/installed artifact digests and modes. After a successful standard-layout deployment it keeps the current and immediately previous candidate build trees and removes older exact-commit candidate directories; rollback remains receipt-owned and does not depend on those build trees. A newly installed candidate may not pass through the legacy fallback. Recovery of an uncommitted replacement and explicit rollback probe modern discovery first but may use the previous service's 2025 `initialize` Session lifecycle. If the post-stop race check fails before replacement, the original service is restarted and the receipt is `not_committed`; a failure after replacement automatically restores the receipt-local previous artifact set. There is no general active-Job override in the normal deployment contract.

Rollback is explicit and receipt-bound:

```bash
receipt=/var/lib/ordivon/deployments/<deployment-receipt>

scripts/ordivon-runtime-deploy rollback \
  --receipt "$receipt" \
  --confirm-receipt "$receipt" \
  --install-dir /usr/local/libexec/ordivon \
  --env-file /etc/ordivon/ordivon-runtime.env
```

Rollback validates the receipt-bound install directory, service, environment file, artifact set, previous digests, and recorded modes. It preserves the displaced current artifacts inside the same receipt before restoring the previous set. If restoring the previous set fails, it attempts to restore the displaced current set and receipts both outcomes. A successful `rollback-result.json` is itself a later release-state event: `ordivon-runtime-status` verifies the restored artifact set against it instead of continuing to compare physical state with the superseded forward deployment. At apply time the deployer records `previousCommit` only when the complete pre-deployment artifact fingerprint exactly matches an earlier receipted release event. A later rollback may therefore recover that exact commit; when the restored bytes predate unified release authority, their artifact truth remains exact but `commitKnown=false`, and status reports `DEPLOYMENT_COMMIT_UNKNOWN` rather than inventing revision identity. Release schema v2 owns the complete artifact set; the deployer deliberately retains rollback support for schema-v1 binary-only receipts, whose historical `0755` install mode is explicit compatibility knowledge. Its MCP probe accepts either the modern lifecycle or the prior legacy Session lifecycle, because rollback must be able to prove a genuinely old Runtime rather than require it to implement the new protocol. Additive query indexes and the isolated Workspace Patch receipt table are maintained outside `schema_migrations`; neither changes existing Job, Attempt, or repair semantics.

### MCP probe module placement

`ordivon-runtime-deploy`, `ordivon-runtime-reclaim`, and `ordivon-runtime-capacity-acceptance` share `mcp_probe.py`; they do not embed separate protocol clients. Repository execution remains useful during development and bootstrap, but canonical production installation is the receipt-bound release artifact set above: the three consumers and `mcp_probe.py` are installed and rolled back together with the Runtime binaries. A manually copied executable or support module is therefore outside canonical deployment truth until a normal release brings its digest and mode under the latest receipt. Candidate deployment requires modern discovery; reclaim uses modern discovery first and falls back to legacy initialization only so that an installed previous Runtime can still release a Workspace through its own `workspace.close` contract.

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

The `--confirm-policy` / `--confirm-quarantine` phrases on these root-operated maintenance CLIs are human/operator anti-mistake ceremony, not semantic authority. The actual protection comes from classification, exact Workspace identity, active-Job checks, locks, before/after receipts, and preserved bytes. Future Agent-facing control surfaces should express deliberate intent and affected identities structurally rather than asking an Agent to echo a magic phrase.

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

`scripts/ordivon-runtime-capacity-acceptance` is the repeatable public-surface proof for global admission. It opens `N+1` isolated Workspaces, admits exactly `N` bounded sleep Jobs, requires the next Job to fail with `CONCURRENCY_LIMIT`, verifies exact `active`/`limit` truth plus an explicit `holdersTruncated` completeness signal, checks that the holder identities are either complete or an honest bounded subset, waits for every admitted Job to succeed, and closes every acceptance Workspace. It emits a digest-bound JSON receipt and never reads the Registry directly.

Run it from a host shell or independent systemd unit, not from a Runtime Job: a Runtime Job would itself consume one of the slots being measured.

```bash
scripts/ordivon-runtime-capacity-acceptance \
  --env-file /etc/ordivon/ordivon-runtime.env \
  --source-repo /root/projects/ordivon-runtime \
  --source-revision "$(git -C /root/projects/ordivon-runtime rev-parse HEAD)" \
  --limit 8 \
  --receipt /var/lib/ordivon/runtime/evidence/runtime-capacity-live.json \
  --pretty
```

The command resolves the same private Runtime credential source as deploy/reclaim through the shared `mcp_probe.py`; `--token-env` is retained only as an explicit legacy override and is not the production default. The command is parameterized rather than hard-coded to eight. The Runtime capacity value follows its positive `u32` representation; the holder-identity projection may remain bounded, but incompleteness is explicit through `holdersTruncated` rather than being silently presented as the full active set.

## Secret-free status

`scripts/ordivon-runtime-status` separates operational health from bounded maintenance diagnosis. The default and `--health` path verifies the latest successful deployment receipt, the complete installed release-artifact digests and modes, systemd state, allowlisted numeric Runtime configuration, and Registry/recovery consistency. It deliberately skips Git Workspace scans, recursive storage measurement, lifecycle receipts, and protocol-history analysis. `--diagnose` adds the receipted MCP lifecycle, selected and supported protocol versions, `toolCatalogDigest`, bounded current-plus-rotated protocol observations, the immediately previous rollback protocol, Workspace consistency, cache/Registry/deployment/candidate byte measurements, stale dirty Workspace counts, and the latest lifecycle receipt. It never opens the MCP endpoint and never reads or emits the bearer token.

```bash
# Fast health path; this is the default and is suitable for frequent automation.
scripts/ordivon-runtime-status
scripts/ordivon-runtime-status --health --json

# Explicit maintenance and compatibility diagnosis; this may scan large cache trees.
scripts/ordivon-runtime-status --diagnose --json
scripts/ordivon-runtime-status --diagnose --json \
  --expected-commit "$(git rev-parse HEAD)"
```

The JSON schema reports separate `health` and `maintenance` states. The deployment projection names the current `releaseEvent` and `releaseDisposition`; a successful explicit rollback supersedes the forward deployment for installed-artifact truth. Exit code `1` is reserved for operational health failures such as service, deployment, Registry, recovery, exact release-artifact inconsistency, or a rollback whose restored artifact set is known but whose coherent source Commit cannot be proven. Maintenance findings such as a stale dirty Workspace produce top-level `status=attention` but retain exit code `0`; automation must inspect `maintenanceAction` when maintenance policy should gate a workflow. Compatibility observations remain advisory deletion evidence: an unreadable or truncated trace, unknown rollback protocol, or incomplete observation window blocks deletion but does not create a health incident. Exit code `2` remains reserved for an invalid invocation or unreadable mandatory input.

## Real-system release acceptance

Portable CI proves source, schema, Registry, protocol, operational-script, documentation, dependency, and secret-scanning contracts. It cannot prove the production systemd/cgroup path.

The real-system gate is:

```bash
export ORDIVON_ACCEPTANCE_OUTPUT=target/acceptance/runtime-system-acceptance.json
scripts/local-acceptance run
```

This executes all ignored systemd/cgroup fixtures serially and then performs the complete public MCP journey against a temporary loopback Runtime. The optional output path receives a JSON receipt binding the tested source commit, candidate binary digests, Tool catalog digest, and every asserted journey check.

`.github/workflows/system-acceptance.yml` runs this path only on an operator-owned self-hosted runner labeled `ordivon-runtime-systemd`; ordinary hosted CI is not represented as equivalent evidence. A release that changes dispatch, Runner behavior, supervision, cancellation, authority profiles, resource controls, or recovery must retain a successful receipt for the exact candidate commit.

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
