---
schema_version: 1
id: runtime.releases
title: Runtime Releases and Versioning
type: release
profile: engineering
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - user
  - maintainer
  - builder
  - operator
updated: 2026-08-04
summary: Version identities, change classification, compatibility obligations, release evidence, and deprecation rules.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.status
  - runtime.compatibility
  - runtime.operations
  - runtime.data-privacy
---
<!-- cspell:words Clippy -->
# Runtime Releases and Versioning

## Release identity

Runtime does not compress every compatibility question into one SemVer number.

| Identity | Meaning |
| --- | --- |
| repository release version | public source and product change set |
| MCP protocol version | transport lifecycle understood by clients |
| Tool catalog digest | exact generated Tool names, annotations, and schemas |
| Runtime schema version | public request and response structure |
| Registry migration version and checksums | persisted execution-state interpretation |
| deployment receipt | exact source commit, toolchain, candidate/installed Runtime artifacts including target-bound systemd units, any configured repository-owned Windows provider transition, protocol, and catalog |

A release version helps users discuss change. It does not replace the stronger identities required for replay, recovery, migration, or rollback.

## Current version stage

The repository is pre-1.0. Version `0.1.0` represents the first operational baseline, not a promise that every public field is permanently stable.

Pre-1.0 changes still require:

- an entry in [`../CHANGELOG.md`](../CHANGELOG.md);
- explicit compatibility impact;
- a migration or major-cutover path for persisted state;
- a deletion trigger for retained compatibility code;
- updated generated Tool reference when the catalog changes;
- portable and real-system evidence appropriate to the boundary.

## Changes

### Patch

A patch release fixes behavior without intentionally changing the supported Tool, protocol, or persisted-state contract. It may improve diagnostics, tests, documentation, or operational safety.

### Minor

A minor release may add Tools, fields, authority profiles, operational capabilities, or additive persisted state. Existing supported callers and rollback dependencies must remain valid or receive an explicit migration.

`schemaVersion` names a compatible request/response family, not every exact generated shape. Additive response fields may remain in the same schema family when existing supported callers can ignore them safely. Additive optional request fields or newly optional mechanical fields may also remain in-family when the previous fully explicit shape retains its semantics and request identity, while use of the new shape has an explicit identity/migration rule. `toolCatalogDigest` binds the exact generated Tool definitions and schemas for deployment, discovery, and acceptance. Removing, reinterpreting, or making a previously optional field mandatory requires an explicit compatibility decision rather than relying on the unchanged schema number.

Error codes are control semantics, not diagnostic prose. Reclassifying an error is compatible only when named live consumers either already understand the precise code or are migrated first with a bounded old/new compatibility branch. A semantic correction must not silently route infrastructure corruption into a caller-correctable branch, or vice versa.

### Major

A major release may intentionally remove or reinterpret a supported contract. It requires a cutover plan that names:

- affected clients and persisted objects;
- export or archival requirements;
- rollback boundary;
- acceptance evidence;
- the point after which the old contract is no longer supported.

## Verification

A releasable commit must have:

1. clean portable CI: formatting, Clippy, Rust tests, Python tests, documentation contract, secret scanning, dependency policy, and advisory checks;
2. a successful `scripts/local-acceptance run` receipt from the supported systemd/cgroup environment when execution or supervision behavior changed;
3. an updated Changelog entry;
4. generated Tool reference with no diff;
5. a clean source tree and fixed Rust toolchain;
6. a deployment candidate manifest binding toolchain identity plus every release artifact's kind, constrained destination target, mode, size, and digest; the default release includes all three repository-owned systemd units, and on a Windows-configured node the manifest also includes the exact Windows launcher contract, source/compiler identity, and candidate digest;
7. a successful deployment plan;
8. after deployment, a receipt binding the complete installed release-artifact targets, digests, and modes; the selected systemd unit directory and manager reload are part of the cutover/rollback contract, alongside any Windows launcher path/digest transition plus environment-file before/after digests, protocol lifecycle, supported versions, and Tool catalog digest;
9. for a release that changes the structured self-release contract, an exact `release.apply` → Runtime ingress replacement → reconnect → `release.get` acceptance proving the same effect identity/receipt is reconciled without a second physical deployment;
10. a verified previous-artifact-set rollback path—including receipt-bound systemd units when present—while that rollback window remains supported.

Documentation-only changes do not require redeploying identical binaries, but public canonical documents must pass the documentation contract and identify when production behavior remains on an earlier code-equivalent commit.

## Compatibility

A release must state Tool, protocol, persisted-state, client, and rollback compatibility explicitly. Version numbers do not replace schema, migration, catalog, or deployment identities.

## Changelog policy

`CHANGELOG.md` is the human-facing record of user-visible change. Record:

- Added;
- Changed;
- Deprecated;
- Removed;
- Fixed;
- Security;
- Migration notes.

Do not copy every commit. Include changes that affect use, operation, compatibility, safety, recovery, or understanding.

## Rollback

The receipt-bound previous artifact set remains the supported rollback boundary while its migrations and compatibility storage remain valid. A release that removes that path requires an explicit major cutover and archival decision.

## Deprecation and deletion

Compatibility code is removed only when:

- every current production and rollback consumer is named;
- the explicit observation window has completed;
- no live client is observed;
- no retained Registry object requires the contract for replay or reconstruction;
- a reviewed source change removes it;
- deployment and rollback acceptance remain valid.

The current inventory and deletion rules are [`compatibility.md`](compatibility.md).

## Publication

The Rust crates are currently repository-internal and set `publish = false`. Public distribution is through source, tagged releases, and receipted installed binaries until a separate crates.io publication contract is deliberately adopted.
