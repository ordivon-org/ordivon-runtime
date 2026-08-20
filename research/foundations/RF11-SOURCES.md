---
schema_version: 1
id: runtime.foundations.rf11.sources
title: RF11 Sources — Observation, Evidence, Proof and Truth Surfaces
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
summary: Primary/canonical sources used by RF11 for provenance, integrity/authenticity, snapshot consistency, negative/failure evidence, time semantics and current Runtime evidence/projection contracts.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf11
---
# RF11 Sources — Observation, Evidence, Proof and Truth Surfaces

## S1 — W3C PROV family

W3C Provenance Working Group, **PROV-Overview**, **PROV-DM / PROV-O / PROV-Constraints**.

Canonical sources:

- `https://www.w3.org/TR/prov-overview/`
- `https://www.w3.org/TR/prov-o/`
- `https://www.w3.org/TR/prov-constraints/`

RF11 use:

- provenance is information about entities, activities and agents involved in producing or
  influencing a thing;
- provenance supports assessments of quality, reliability and trustworthiness without
  itself becoming semantic truth;
- PROV explicitly models multiple descriptions/perspectives and provenance of provenance;
- supports `RecordBytes != RecordProvenance` and `GoodProvenance != SemanticTruth`.

## S2 — NIST Secure Hash Standard

NIST **FIPS 180-4 — Secure Hash Standard (SHS)**.

Canonical source: `https://csrc.nist.gov/pubs/fips/180-4/upd1/final`.

RF11 use:

- secure hash algorithms generate message digests used to detect whether message bytes have
  changed since the digest was generated;
- supports byte/content identity and integrity claims;
- does not define provenance, issuer authority or semantic correctness;
- supports `ByteIdentity != Provenance` and `DigestIntegrity != SemanticValidity`.

## S3 — NIST Digital Signature Standard

NIST **FIPS 186-5 — Digital Signature Standard (DSS)**.

Canonical source: `https://csrc.nist.gov/pubs/fips/186-5/final`.

RF11 use:

- digital signatures detect unauthorized modification and authenticate signatory identity
  under supporting key/trust infrastructure;
- supports origin/integrity evidence rather than truth of arbitrary signed content;
- supports `SignatureValidity != SemanticTruth` and explicit validation assumptions.

## S4 — W3C provenance semantics of perspective/derivation

W3C **PROV Model Primer** and **PROV Semantics**.

Canonical sources:

- `https://www.w3.org/TR/prov-primer/`
- `https://www.w3.org/TR/prov-sem/`

RF11 use:

- the same evolving thing can be described from different identified perspectives;
- generation/use/derivation/agent responsibility are separate relations;
- supports scope-qualified provenance and the principle that one representation does not
  collapse every view of Reality.

## S5 — SQLite isolation / snapshot semantics

SQLite official documentation, **Isolation In SQLite**.

Canonical source: `https://www.sqlite.org/isolation.html`.

RF11 use:

- WAL readers see an unchanging database snapshot from read-transaction start;
- supports current Runtime inspection using one query-only SQLite snapshot to avoid
  cross-query Registry inconsistency;
- does not imply Git/filesystem/process/external state is atomically frozen;
- supports `OwnerLocalSnapshot != GlobalWorldSnapshot`.

## S6 — Chandy–Lamport distributed snapshot

K. Mani Chandy and Leslie Lamport, **Distributed Snapshots: Determining Global States of a
Distributed System**, ACM Transactions on Computer Systems 3(1), 1985.

Canonical author/publication source:
`https://www.microsoft.com/en-us/research/people/lamport/publications/`.

RF11 use:

- determining meaningful global state across distributed components requires an explicit
  snapshot protocol;
- stable-property detection is a separate class of inference;
- supports the refusal to call arbitrary independent multi-owner reads one global snapshot.

## S7 — Lamport time/causality

Leslie Lamport, **Time, Clocks and the Ordering of Events in a Distributed System**, 1978.

Canonical source:
`https://www.microsoft.com/en-us/research/publication/time-clocks-ordering-events-distributed-system/`.

RF11 use:

- causal partial order and clock/timestamp order are distinct;
- supports `TimestampOrder != CausalOrder` and the requirement that time evidence be scoped
  to its clock/protocol semantics.

## S8 — Absence can carry information under a timing contract

Leslie Lamport, **Using Time Instead of Timeout for Fault-Tolerant Distributed Systems**,
1984.

Canonical source:
`https://www.microsoft.com/en-us/research/publication/using-time-instead-timeout-fault-tolerant-distributed-systems/`.

RF11 use:

- absence/silence can carry information when synchronized-time/protocol assumptions make an
  expected event mandatory by a deadline;
- supports the qualified rule that absence can become evidence only under a completeness
  contract.

## S9 — Failure detector completeness/accuracy

Tushar D. Chandra and Sam Toueg, **Unreliable Failure Detectors for Reliable Distributed
Systems**, Journal of the ACM 43(2), 1996.

Canonical Cornell source:
`https://www.cs.cornell.edu/info/people/sam/FDpapers.html`.

RF11 use:

- failure detection is characterized by completeness and accuracy rather than infallible
  process-death observation;
- supports `NoObservedResponse != ProvenFailure` absent stronger assumptions;
- reinforces that negative evidence has an observation/failure model.

## S10 — Timestamp evidence semantics

IETF RFC 9921 (building on RFC 3161), **COSE Header Parameter for Timestamp Tokens**.

Canonical source: `https://www.rfc-editor.org/rfc/rfc9921.html`.

RF11 use:

- explicitly distinguishes evidence of payload existence at a prior time from evidence of
  cryptographic signature existence at a prior time;
- supports `TimeEvidence must bind the exact object/claim` and rejects unqualified timestamp
  semantics.

## S11 — NIST forensic-source plurality

NIST **SP 800-86 — Guide to Integrating Forensic Techniques into Incident Response**.

Canonical source: `https://csrc.nist.gov/pubs/sp/800/86/final`.

RF11 use:

- practical forensic reconstruction consumes heterogeneous data sources including files,
  operating systems, network traffic and applications;
- supports the architectural distinction between multiple evidence domains rather than one
  universal record source.

## S12 — Current Ordivon Runtime as empirical source

RF11 audits current Core, Registry, evidence, inspection, recovery and effect-kernel code.

### Scoped durable commitments

SQLite Registry is canonical for Runtime-owned Job, Attempt, reservation, idempotency and
state-transition commitments; rows carry local `row_version` guards and repair uses exact
fingerprints/snapshot digests/row versions before administrative mutation.

### Identity-bound terminal evidence

Terminal evidence is content-addressed and binds Job, Attempt, operation/source/provider,
process/supervisor observations, execution/delivery/process-tree dispositions and terminal
Artifact identities. Recovery can emit a later terminal-evidence Artifact containing
`supersedesArtifactId` instead of overwriting the earlier evidence history.

### Projection completeness

Runtime exposes explicit completeness metadata including `holdersTruncated`,
`capacityHoldersTruncated`, `attemptsTruncated`, `eventsTruncated`, `recentJobsTruncated`,
Artifact `truncated` and paginated cursors. Mechanical convergence is not claimed when the
Attempt projection itself is truncated.

### Registry-local snapshot versus physical observations

Read-only inspection uses one SQLite query snapshot for Registry-backed facts but treats Git
and filesystem facts as separate physical observations; it does not claim a cross-owner
transaction.

### P4 target-view falsifier

Current P4 physically proved a trusted same-authority target could create a private mount
namespace, present unchanged V1 bytes at the Runtime host pathname while consuming V2 at the
same target-visible pathname, and produce no host-inotify-visible path event. Current
continuity evidence therefore reports scope `runtime_host_namespace_path_witness`, not
immutable target-byte-consumption proof.

### Windows negative evidence

The `windows_native` launcher owns the native tree through Windows Job Object
`KILL_ON_JOB_CLOSE`. Under this target-specific contract, loss of launcher lineage proves
that the native child tree cannot still survive; Runtime can classify this more strongly than
generic Linux absence.

### Structured Effect receipts

Runtime Release binds deterministic effect identity and canonical deployment receipt;
`release.get` treats matching Effect receipt as direct deployment-domain evidence and
returns reconciliation required when receipt truth is missing/inconsistent. Workspace Patch
instead owns before/after digest state and reconciles to committed/prepared/unknown.

### Non-authoritative projections/logs

`task.get` / `task.list` are projection-only; `task.observe` may actively reconcile and even
dispatch one already-admitted Accepted Job. Trace JSONL and inspection reports remain
derived diagnostic surfaces rather than new state owners.

## Source discipline

Later rounds should preserve:

```text
observation != reality
record != truth
evidence is claim-relative
evidence != proof
truth-owner language must name fact/scope/contract
integrity != authenticity != provenance != correctness != completeness != freshness
digest proves bytes, not provenance/semantic truth
signature proves origin/integrity under trust assumptions, not semantic truth
bounded correct projection may be incomplete
owner-local snapshot != global world snapshot
newer record != stronger evidence
absence requires completeness/timing/substrate assumptions
receipt authority is domain/claim scoped
artifact integrity != artifact correctness
federated scoped authority + reconciliation > synthetic global truth database by default
```
