---
schema_version: 1
id: runtime.foundations.rf4.sources
title: RF4 Sources — Identity, Sameness, Replay and History
type: reference
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: Primary/canonical sources used by RF4 to separate content identity, surrogate identifiers, persistent entity identity, request idempotency keys/fingerprints, replay and history.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf4
---
# RF4 Sources — Identity, Sameness, Replay and History

## S1 — Git: content-addressed immutable object identity

Git project, **Git Internals — Git Objects**.

Canonical source: git-scm.com.

RF4 use:

- Git is explicitly a content-addressable filesystem/object store;
- object keys derive from object type/header plus content;
- demonstrates a powerful scoped identity contract for immutable content objects;
- falsifies the inverse overgeneralization that every mutable resource/request/operation
  should use content identity.

## S2 — Git: references are distinct from object identity

Git project, **Git Internals — Git References** and revision-selection documentation.

Canonical source: git-scm.com.

RF4 use:

- refs provide names/pointers to Git object identifiers and may move over time;
- supports `Reference/Name != ImmutableObjectIdentity`.

## S3 — RFC 9562: UUIDs

K. Davis, B. Peabody and P. Leach, **RFC 9562 — Universally Unique IDentifiers
(UUIDs)**, IETF, 2024.

Canonical source: RFC Editor.

RF4 use:

- UUIDs supply inexpensive practically unique identifier values across many application
  entity kinds;
- uniqueness/persistence properties do not define the application semantics of the
  identified entity;
- RFC guidance itself warns against assuming natural names never change;
- supports `UniqueIdentifier != IdentityContract` and separation of surrogate identifier
  from mutable natural attributes.

## S4 — SQLite: rowid persistence limits

SQLite project, **Rowid Tables**.

Canonical source: sqlite.org.

RF4 use:

- rowid is a highly optimized locator/key;
- without aliasing to `INTEGER PRIMARY KEY`, rowid is not persistent and may change after
  `VACUUM`;
- directly supports `Locator != StableEntityIdentity`.

## S5 — AWS EC2 idempotency

Amazon EC2, **Ensuring idempotency in Amazon EC2 API requests**.

Canonical source: docs.aws.amazon.com.

RF4 use:

- client token + same parameters yields idempotent retry without further action;
- same client token + changed parameters yields `IdempotentParameterMismatch`;
- idempotency scope can be regional or zonal rather than globally universal;
- supports `Request continuity key != Request semantic fingerprint` and explicit scope.

## S6 — IETF HTTPAPI Idempotency-Key draft

J. Jena and S. Dalal, **The Idempotency-Key HTTP Header Field**,
`draft-ietf-httpapi-idempotency-key-header-07`, HTTPAPI WG, 2025.

Canonical source: IETF Datatracker.

Status note: Internet-Draft, not a final RFC at RF4 time.

RF4 use:

- key is a client-generated value used by a resource to recognize retries of the same
  request;
- key uniqueness/lifetime policy belongs to the resource owner;
- fingerprint can be derived from payload fields/checksum/signature;
- clients cannot assume an arbitrary key will be honored without server contract;
- supports key/fingerprint/scope/lifetime separation.

## S7 — Current Ordivon Runtime as empirical source

RF4 audits the current Runtime source/test suite.

### Request continuity versus world binding

`request_identity_replays_across_world_change_but_operation_identity_binds_world` proves:

```text
same request identity + changed current world
→ same historical Job / original operationDigest
```

while changed request identity under the same key conflicts.

### Proposal canonicalization

`proposal_identity_preserves_omission_and_normalizes_equivalent_paths` proves the request
fingerprint intentionally defines an equivalence relation over normalized paths while
preserving semantic differences such as omitted versus explicitly authored limits.

### Historical effective-plan replay

`proposal_identity_replays_original_effective_plan` proves replay does not recompute
current policy/effective values.

### Request-key conflict

`idempotent_replay_returns_one_job_and_conflict_rejects_change` proves exact replay returns
one Job while same request key with changed committed request shape conflicts.

### Layered identity

Current source separately stores/derives:

```text
principal + clientRequestId
requestDigest
jobId
operationDigest
attemptId
launchTokenDigest
process/unit/invocation identities
Artifact IDs/digests
structured release effectId
Workspace Patch operationId
```

RF4 interprets these as separate identity/fingerprint/lineage coordinates rather than
redundant aliases.

### Superseding evidence and tombstones

Tests/docs preserve superseded terminal evidence and Workspace-close tombstones, proving
that corrected/current projections need not erase prior evidence/history.

## Source discipline

Later rounds should preserve:

```text
identifier value != identity semantics
content address != universal mutable entity identity
name/reference != target/content identity
request key != request fingerprint
idempotency scope/lifetime belongs to receiver/effect owner
historical replay != current-world reinterpretation
current projection != complete history
local Runtime IDs remain contract-specific coordinates, not ontology primitives
```
