---
schema_version: 1
id: runtime.foundations.rf11
title: RF11 — Observation, Evidence, Proof and Truth Surfaces
type: report
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - reader
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: RF11 establishes Runtime epistemology: Reality/facts, claims, observations, records, evidence, proof, scoped truth authority, provenance, integrity, authenticity, freshness, completeness and projection are distinct. It refines truth-owner language into fact-scoped canonical commit/reconstruction authority; shows digests prove byte identity/integrity rather than provenance or semantic truth; treats receipts as domain-scoped evidence; formalizes negative evidence as requiring observation completeness and a closed-world/termination contract; and explains why bounded projections, SQLite snapshots, process observations and external receipts must remain scope-qualified instead of becoming one synthetic global truth database.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF11
related:
  - runtime.foundations.start
  - runtime.foundations.rf10
  - runtime.foundations.rf11.sources
  - runtime.foundations.rf11.continuation
---
# RF11 — Observation, Evidence, Proof and Truth Surfaces

## 0. Status and research question

RF10 established that failure, ambiguity and retry safety depend on what can actually be
known after interruption. RF11 therefore asks:

> **What does Runtime observe, what turns an observation into evidence for a claim, what
> makes evidence conclusive enough to support a proof, who is authoritative for a fact,
> what do digests/receipts/timestamps/generations actually establish, and how can Runtime
> combine bounded independent truth surfaces without pretending that one projection is the
> whole Reality?**

RF11 is explanatory research only. It does not authorize a new evidence database,
confidence-scoring service, provenance graph, signature layer, cross-owner transaction,
unbounded projection, global event store or semantic verifier.

---

# 1. First deletion — “truth” is overloaded

Systems documentation often says:

```text
source of truth
truth owner
canonical truth
observed truth
execution truth
receipt truth
```

These phrases can hide distinct concepts:

```text
Reality
Fact
Claim
Observation
Record
Evidence
Proof
Authority
Projection
```

RF11 separates them.

---

# 2. Reality

**Reality** is the target world/system whose behavior exists independently of Runtime's
particular observation record.

Examples:

```text
actual bytes currently on a filesystem
actual process tree currently alive
actual deployment installed
actual financial order accepted
actual user Goal satisfied
```

Runtime may participate in Reality but never equates its database with Reality itself.

---

# 3. Fact

RF11 uses **Fact** for a proposition about Reality/model that is actually true under a named
boundary/time/semantics.

Examples:

```text
At t, Attempt A had a committed dispatch intent.
At t, process P with start identity X existed.
Release effect E produced canonical receipt R.
Artifact bytes had digest D when registered.
```

A fact is not automatically known merely because it is true.

---

# 4. Claim

A **Claim** is a proposition asserted or queried by an observer/system.

Examples:

```text
Claim: Attempt A succeeded.
Claim: no child process remains.
Claim: release E was deployed.
Claim: this Artifact has not changed.
Claim: this list contains every capacity holder.
```

Claims can be true, false, unsupported, stale, under-specified or outside an observer's
authority.

---

# 5. Observation

RF11 defines an **Observation** as:

> **The outcome of an observer/instrument interacting with or reading some target under a
> declared method, scope and time.**

Examples:

```text
read SQLite row
read cgroup.events
stat /proc/PID
hash file bytes
read deployment receipt
query external provider state
```

---

# 6. Observation is observer-relative

The same Reality can yield different observations because observers have different:

```text
namespaces
permissions
sampling times
instruments
caches
network paths
models
```

Therefore:

```text
Observation != Reality
```

RF1's state/observation distinction remains active.

---

# 7. An observation can be correct yet insufficient

Example:

```text
inotify observed no host-path mutation event
```

can be a perfectly correct observation while being insufficient for:

```text
target process consumed the committed bytes
```

Current P4 physically demonstrated exactly this through a private mount namespace.

Thus:

```text
CorrectObservation != SufficientEvidenceForArbitraryClaim
```

---

# 8. Record

A **Record** is a durable or transient representation of an observation, commitment,
decision or claim.

Examples:

```text
Registry row
job_event
terminal-evidence JSON
trace line
receipt file
MCP response
```

A record can survive the event it describes, but record existence does not automatically
make its content true.

---

# 9. Record provenance matters

To use a record safely, one may need to know:

```text
who/what generated it
from which inputs
under which software/version
when
under which identity
whether it was transformed
```

W3C PROV formalizes provenance around Entities, Activities and Agents and explicitly treats
provenance as information supporting assessments of quality, reliability and
trustworthiness.

RF11 conclusion:

```text
RecordBytes != RecordProvenance
```

---

# 10. Evidence

RF11 defines **Evidence** as:

> **An observation or record that, under a declared contract/model, bears on the truth of a
> specific claim.**

Evidence is therefore claim-relative.

The same record can be strong evidence for one claim and irrelevant for another.

---

# 11. Evidence is not a universal object property

Example:

```text
exitCode = 0
```

is evidence for:

```text
the local process returned success status according to its exit convention
```

but not by itself for:

```text
external payment committed
user Goal satisfied
result scientifically correct
```

Therefore:

```text
EvidenceStrength is claim-relative.
```

---

# 12. Evidence support relation

RF11 introduces notation:

```text
E ⊢[C] Q
```

meaning:

> Evidence set E supports Claim Q under Contract/assumptions C.

This notation forces the missing scope to be named.

---

# 13. Proof

RF11 uses **Proof** in a systems/contract sense:

> **A valid argument that a claim follows from accepted premises, evidence and a declared
> model/contract.**

Formally:

```text
(E, C) ⇒ Q
```

when the inference is valid inside C.

---

# 14. Evidence is not proof by itself

A SHA-256 digest, log line or receipt is an evidence object.

A proof additionally requires:

```text
what claim it addresses
why this source is authoritative/relevant
what assumptions connect evidence to claim
whether required completeness/freshness holds
```

Thus:

```text
Evidence != Proof
```

---

# 15. Proof is scoped, not metaphysical certainty

When Runtime says a process tree is terminal, the proof is under:

```text
current process/cgroup ownership model
identity binding
observation method
platform semantics
```

It is not a philosophical claim of omniscience over all Reality.

---

# 16. Truth owner — refined definition

Prior Runtime documentation uses “truth owner” productively. RF11 preserves the shorthand
but makes it exact:

> **TruthOwner(f, C) is the component/domain that has canonical commit/reconstruction
> authority for fact class f under contract C.**

This means the owner controls the authoritative record/transition semantics for that fact.

It does **not** mean the component metaphysically owns Reality.

---

# 17. Truth ownership must name the fact

Bad:

```text
SQLite is the truth.
```

Better:

```text
SQLite Registry is authoritative for committed Runtime Job/Attempt/reservation identity and
state-transition records under the Registry contract.
```

SQLite is not authoritative for:

```text
Git filesystem contents
live external process existence
payment-provider world state
semantic Goal completion
```

---

# 18. Truth authority is domain-scoped

Current Runtime is fundamentally **federated** across scoped authorities:

```text
Git/filesystem     source/worktree facts
SQLite Registry    durable Runtime commitments/identity/state
systemd/cgroup     live Linux supervision/process ownership facts
Windows Job/launcher native process ownership facts
Runner bundle      execution start/result evidence
structured receipt domain-specific Effect facts
Host/domain        semantic completion/Goal facts
```

No one owner subsumes all others.

---

# 19. “Source of truth” is therefore shorthand

RF11 allows the phrase only when expanded mentally as:

```text
canonical source of record for fact class F under scope S and contract C
```

Without F/S/C, “source of truth” encourages accidental authority expansion.

---

# 20. Authority and truth are distinct

A component may be the canonical authority for recording a fact class and still contain
corrupt/buggy data.

Therefore:

```text
AuthoritativeSource != InfallibleSource
```

Validation, invariants and corroboration still matter.

---

# 21. Provenance

W3C PROV defines provenance as information about entities, activities and agents involved
in producing/influencing/delivering a thing.

RF11 uses **Provenance** to answer:

```text
Where did this evidence/data come from?
What process produced it?
What inputs influenced it?
Who/what bears responsibility?
```

---

# 22. Provenance is not truthfulness

A record can have excellent provenance:

```text
signed by service S
produced by version V
from input I
```

and still contain a wrong conclusion because S had a bug.

Thus:

```text
GoodProvenance != SemanticTruth
```

Provenance supports assessment; it does not replace validation.

---

# 23. Integrity

**Integrity** here means evidence that bytes/state have not changed outside the declared
mutation contract since a reference point.

It does not by itself establish origin, authority or correctness.

---

# 24. Cryptographic digest

NIST FIPS 180-4 defines secure hash algorithms that generate message digests used to detect
whether messages have changed since digests were generated.

RF11 consequence:

```text
Hash verifies content identity/change relationship.
```

It does not intrinsically tell us who produced the bytes.

---

# 25. Same digest/bytes do not prove provenance

If two unrelated sources contain identical bytes B:

```text
SHA256(source1) = SHA256(source2)
```

that establishes byte equivalence under the hash assumption, not:

```text
source1 derived from source2
source1 was authorized
source1 came from intended toolchain
```

Thus:

```text
ByteIdentity != Provenance
```

---

# 26. Digest does not prove semantic correctness

A perfectly hashed wrong answer remains wrong.

Therefore:

```text
DigestIntegrity != SemanticValidity
```

Current Artifact digests are strong evidence of Artifact byte identity, not correctness of
Artifact claims.

---

# 27. Digital signature

NIST FIPS 186-5 states that digital signatures can detect unauthorized data modification
and authenticate signatory identity under the supporting key/trust infrastructure.

RF11 interpretation:

```text
Signature → origin/integrity assurance under key-binding assumptions
```

not:

```text
Signature → content is semantically true
```

---

# 28. Signature validity itself has premises

Signature validation needs:

```text
valid algorithm
valid key binding
appropriate public key
private-key possession assumptions
revocation/lifetime policy
signed bytes/context
```

So:

```text
CryptographicVerification != UnconditionalAuthority
```

---

# 29. Timestamp

A timestamp is evidence about time only under a clock/authority/protocol contract.

RF5 already established:

```text
TimestampOrder != CausalOrder
```

RF11 adds:

```text
TimestampPresence != Freshness
TimestampPresence != OccurrenceProofByItself
```

---

# 30. Timestamp semantics depend on what was timestamped

Modern RFC timestamp work explicitly distinguishes a timestamp over payload existence from
a timestamp over a cryptographic signature's existence.

RF11 lesson:

```text
TimeEvidence must bind the exact object/claim whose time is asserted.
```

---

# 31. Generation/version counters

A `generation`, `row_version` or sequence number can prove ordering/equality relationships
inside its owner contract.

It does not automatically represent:

```text
wall-clock time
causal order across unrelated owners
semantic freshness
```

---

# 32. Current `row_version` is concurrency evidence

Runtime repair uses expected row versions to prove:

```text
this Registry object has not changed since the plan observed it
```

before applying CAS-like mutation.

Thus:

```text
RowVersion = owner-local stale-write/freshness guard
```

not global time.

---

# 33. Fingerprint

Doctor/repair fingerprints bind a planned repair to the exact inspected case/report state.

If Runtime state changes afterward, repair fails with reconciliation required.

This establishes:

```text
PlanStillMatchesObservedState?
```

not whether the original observation was semantically correct about the external world.

---

# 34. Identity binding

Evidence is much stronger when it names the exact subject it purports to describe.

Useful bindings include:

```text
Job ID
Attempt ID
Operation digest
invocation ID
launch token
source revision/digest
provider digest
process start identity
Effect ID
Artifact digest
```

---

# 35. Unbound evidence can be misleading

Example:

```text
log: "process completed successfully"
```

without Attempt/invocation identity may belong to an old or unrelated process.

Therefore:

```text
PlausibleContent != IdentityBoundEvidence
```

---

# 36. Current terminal evidence is deliberately identity-rich

Terminal evidence binds at least the Runtime Job/Attempt and relevant execution plan,
source/provider/process/artifact identities.

Its value is not just “more fields”; its fields constrain which Reality trajectory the
record may legitimately describe.

---

# 37. Direct evidence versus derived evidence

RF11 distinguishes:

```text
Direct/domain evidence
  observation/receipt authored close to the fact owner

Derived evidence
  projection/inference computed from other records
```

Examples:

```text
Runner result              direct local execution evidence
Registry JobProjection     derived from durable rows
release receipt            direct Effect-domain evidence
capacity summary           derived Registry projection
```

---

# 38. “Direct” does not mean infallible

Runner Result can be malformed or identity-mismatched.

Therefore direct evidence still needs:

```text
validation
identity binding
integrity
contract checks
```

---

# 39. Projection

A **Projection** is a selected representation of a larger state/evidence set for a specific
consumer/question.

Examples:

```text
task.get
TaskObservation
task.list page
Runtime inspection report
capacity holder sample
Workspace change page
```

Projection is an epistemic interface, not a new Reality owner.

---

# 40. Projection can be exact for its declared question

A bounded projection is not inherently weak.

Example:

```text
task.get(jobId)
```

can exactly project one durable Job's selected state fields while intentionally omitting all
stdout bytes or full event history.

The important requirement is that omitted dimensions are not silently claimed complete.

---

# 41. Correctness and completeness are independent

A list can contain only correct entries yet omit valid entries.

Therefore:

```text
CorrectProjection != CompleteProjection
```

and:

```text
Incomplete != Incorrect
```

---

# 42. Truncation is epistemically meaningful

Current Runtime explicitly exposes:

```text
holdersTruncated
capacityHoldersTruncated
eventsTruncated
attemptsTruncated
recentJobsTruncated
Artifact.truncated
```

These flags prevent a bounded representation from being mistaken for a closed-world set.

---

# 43. Completeness flags constrain downstream inference

Current inspection computes mechanical convergence only if Attempts are not truncated.

That is exactly correct:

```text
All observed Attempts are terminal
```

cannot prove:

```text
All Attempts are terminal
```

when unobserved Attempts may exist.

---

# 44. Pagination is a completeness mechanism

A stable cursor/change-set digest can let bounded responses compose into a complete set
under a continuity contract.

Therefore:

```text
BoundedPage != IncompleteFinalKnowledge
```

if the consumer follows all pages and validates continuation stability.

---

# 45. A prefix is not a set proof

Returning the newest 50 events can answer:

```text
what recently happened?
```

but cannot prove:

```text
this event type never occurred historically
```

unless the API explicitly proves total count/completeness for that predicate.

---

# 46. Snapshot

A **Snapshot** is an observation designed to present a coherent state relative to a
consistency contract.

It is not automatically an instantaneous copy of all Reality.

---

# 47. SQLite snapshot isolation gives one owner-local coherent view

SQLite WAL snapshot isolation lets a read transaction continue seeing the database state as
of its snapshot while concurrent commits happen later.

Current inspection deliberately uses one query-only SQLite read snapshot so multiple
Registry queries do not fabricate cross-query combinations.

---

# 48. SQLite snapshot does not freeze Git/filesystem/process Reality

Current inspection combines:

```text
Registry snapshot
+
separate Git/filesystem observations
```

without pretending they share one atomic transaction.

Thus:

```text
OwnerLocalSnapshot != GlobalWorldSnapshot
```

---

# 49. Distributed snapshot is an explicit protocol problem

Chandy–Lamport's distributed snapshot work exists precisely because a meaningful global
state across distributed components cannot be obtained by casually reading each component
at arbitrary moments.

RF11 conclusion:

```text
CrossOwnerConsistency requires a declared snapshot/reconciliation protocol.
```

---

# 50. Freshness

**Freshness** describes how current evidence is relative to the claim's relevant time.

Evidence can be historically correct and currently stale.

Therefore:

```text
Stale != False
```

and:

```text
OldEvidence may prove a historical fact but not a current-state claim.
```

---

# 51. Observed-at time is not effective-until time

A condition with:

```text
observedAtMs = t
```

says when Runtime observed/recorded it under that method.

It does not necessarily prove the underlying fact began exactly at t.

---

# 52. Freshness must be matched to claim volatility

A source commit digest may remain valid for long periods if bytes are immutable.

A process-alive observation can become stale milliseconds later.

Thus evidence freshness has no universal TTL.

---

# 53. Evidence precedence is claim-specific

RF11 rejects one universal ranking such as:

```text
receipt > database > process > log
```

Instead:

```text
Strength(E1,E2,Q,C)
```

compares evidence for one Claim Q under Contract C.

---

# 54. Stronger evidence means fewer/stronger justified assumptions for the same claim

Example claim:

```text
Release Effect E committed.
```

A canonical identity-bound deployment receipt directly generated by the Effect owner can be
stronger than:

```text
release process exited 0
```

because process exit does not itself establish the deployment state transition.

---

# 55. For another claim, the order reverses

Claim:

```text
The release process exited with code 0.
```

Runner Result/process execution evidence is direct.

Deployment receipt may not even contain that fact.

Therefore authority is claim-dependent.

---

# 56. Last-write-wins is not evidence precedence

A newer weaker record should not automatically overwrite an older stronger fact.

Current late Runner recovery uses explicit supersession semantics rather than mere timestamp
ordering.

Thus:

```text
NewerRecord != StrongerEvidence
```

---

# 57. Supersession

**Supersession** means new evidence justifies replacing a previous conclusion for a claim
while retaining the historical evidence that produced the earlier conclusion.

Current terminal evidence supports this directly through:

```text
supersedesArtifactId
```

---

# 58. Supersession is not deletion

Orphaned evidence followed by a late identity-bound succeeded Runner Result yields two
records:

```text
E1: orphaned conclusion
E2: succeeded conclusion, supersedes E1
```

This preserves epistemic history:

```text
what was known then
what became known later
```

---

# 59. Contradiction must first be normalized by subject/time/scope

Suppose:

```text
Registry says Running at t1
Runner receipt says Succeeded at t2
```

These are not necessarily contradictory.

Before resolving a conflict, compare:

```text
same subject?
same fact?
same effective time?
same scope?
same generation?
```

---

# 60. Genuine conflicting evidence requires owner-aware reconciliation

If two authoritative sources make incompatible claims about the same subject/time/fact,
Runtime should not silently choose the latest row.

It should:

```text
validate identities
validate integrity/provenance
apply contract-specific precedence
preserve ambiguity/manual review if unresolved
```

---

# 61. Corroboration

Multiple independent evidence sources can strengthen a conclusion when they fail
differently.

Example:

```text
systemd unit identity
/proc process start identity
cgroup membership
Runner start digest
```

jointly provide stronger execution-lineage evidence than one unbound log.

---

# 62. Independence matters

Three projections derived from the same corrupted Registry row are not three independent
pieces of evidence.

Thus:

```text
EvidenceCount != IndependentCorroboration
```

---

# 63. Common-mode failure limits corroboration

If:

```text
A and B both read the same wrong file
```

agreement does not rule out corruption of that shared source.

RF11 therefore cares about **provenance/failure-domain diversity**, not superficial count.

---

# 64. Negative evidence

**Negative evidence** is evidence supporting a claim of absence/non-occurrence.

It is much harder than merely failing to observe something.

---

# 65. Absence of observation is not automatically observation of absence

Classic rule:

```text
NoObservedX != ProvenNoX
```

unless the observation contract guarantees that X would have been observed if it existed/
occurred.

---

# 66. Negative evidence requires completeness assumptions

To infer:

```text
X does not exist
```

from:

```text
observer did not see X
```

one generally needs a contract such as:

```text
observer covers the complete relevant domain
the observation interval is sufficient
visibility/permissions are complete
identity mapping is correct
instrument would detect X if present
```

---

# 67. Failure detectors formalize why absence can be uncertain

Chandra–Toueg failure detectors distinguish completeness from accuracy: an asynchronous
observer can suspect a process that is actually correct or fail to know immediately that a
process crashed.

RF11 lesson:

```text
FailureObservation has an accuracy/completeness model.
```

“Not responding” is not a universal proof of death.

---

# 68. Timing assumptions can make absence informative

Lamport's work on using time in fault-tolerant systems explicitly notes that absence of an
expected message can carry information when a synchronized timing protocol supplies the
necessary assumptions.

Thus:

```text
AbsenceCanBeEvidence
```

but only under a protocol that makes silence meaningful.

---

# 69. Windows current absence semantics are stronger because the substrate contract is stronger

Current `windows_native` launcher uses Job Object `KILL_ON_JOB_CLOSE` ownership.

When the launcher lineage is gone under that proven contract, Runtime has evidence that the
native process tree cannot remain alive.

So:

```text
LauncherGone + KillOnJobCloseContract
⊢ NativeTreeGone
```

---

# 70. Ordinary Linux process/unit absence is weaker

A missing systemd unit or unobserved PID alone may not prove historical execution outcome or
external Effect absence.

Runtime therefore combines:

```text
unit state
recorded process identity
cgroup/process facts
termination intent
Runner Result
```

before classifying.

---

# 71. Same raw observation can support different claims under different substrate contracts

Observation:

```text
supervisor lineage absent
```

can yield stronger negative execution evidence on one substrate than another.

Therefore:

```text
ObservationMeaning = Observation + Contract
```

---

# 72. P4 is the canonical “no event” falsifier

P4 proved a target with same authority could create a private mount namespace and consume V2
at the same pathname while Runtime's host namespace remained V1 and inotify observed no host
mutation.

Therefore:

```text
NoHostInotifyEvent
⊬ TargetConsumedCommittedBytes
```

The correct claim is only about the host-namespace path witness.

---

# 73. Scope-qualified evidence prevents false closure

Current Host Dependency continuity therefore explicitly reports:

```text
runtime_host_namespace_path_witness
```

instead of pretending:

```text
target-byte-consumption proof
```

This is an exemplary epistemic boundary.

---

# 74. Receipt

RF11 defines a **Receipt** as:

> **Identity-bound evidence emitted/committed by the owner of a transition/effect contract
> to attest that a named state transition or outcome occurred under that contract.**

A receipt is stronger than a generic log only if its contract actually owns the fact.

---

# 75. Receipt is not just JSON with a digest

A useful receipt requires:

```text
canonical issuer/owner
stable subject/effect identity
contract/version
outcome semantics
integrity/provenance
retention/retrieval
reconciliation behavior
```

Otherwise it may be merely a record named “receipt.”

---

# 76. Runtime Release receipt has domain authority

Current Release binds a deterministic Effect ID and canonical deployment receipt.

For claim:

```text
Release E deployed/rolled back/not committed
```

that receipt can be stronger evidence than generic Runner process state because the deployer
owns the deployment transition itself.

---

# 77. Receipt corruption/absence can reduce certainty

Current `release.get` returns reconciliation required when receipt directory/data is
unavailable, malformed or inconsistent.

Thus:

```text
ExpectedReceiptMissing != EffectDidNotOccur
```

unless the Effect contract establishes stronger negative semantics.

---

# 78. Workspace Patch uses state proof rather than only a terminal receipt

Patch recovery checks physical files against committed before/after digests.

This creates three possible epistemic outcomes:

```text
committed
prepared/not applied
unknown mixed/unexpected state
```

The adapter's strength comes from owning a closed local state model.

---

# 79. State query can be stronger than historical process evidence for current Effect state

For a structured Effect, asking the canonical owner:

```text
What state is Effect E now?
```

can answer the effect claim even if the initiating process history is incomplete.

This motivates reconciliation rather than retry-first recovery.

---

# 80. Artifact

An Artifact is retained evidence/data produced by an operation.

Current Runtime binds Artifact identity, kind, digest, byte length and truncation metadata.

Artifact presence proves only what the Artifact contract supports.

---

# 81. Artifact presence is not correctness proof

A model can generate an incorrect report and Runtime can faithfully retain it.

Thus:

```text
ArtifactExists != ClaimCorrect
```

and:

```text
ArtifactDigestValid != SemanticValidity
```

---

# 82. Artifact truncation changes admissible claims

A truncated stdout Artifact can support claims about retained prefix/tail and truncation
metadata.

It cannot support:

```text
this text is the complete program output
```

unless completeness is established elsewhere.

---

# 83. Logs and traces

Logs/traces are observations/history records useful for diagnosis.

They become authoritative only for fact classes their emission/storage contract owns.

Current Runtime intentionally keeps trace JSONL outside canonical Job/Attempt truth.

---

# 84. Missing trace is usually weak negative evidence

Trace rotation, truncation or bounded scans can make absence expected.

Current compatibility logic correctly treats insufficient trace evidence as blocking a
deletion conclusion rather than proving no live consumer exists.

---

# 85. Forensic evidence versus operational evidence

RF11 distinguishes purposes:

```text
Operational evidence
  sufficient/current enough to make a safe control transition now

Forensic evidence
  richer historical material for later reconstruction/audit
```

One record may serve both, but requirements differ.

---

# 86. Control evidence should be minimal but conclusive for the transition

Example:

```text
expected row version + state + reservation state
```

can be sufficient to reject a stale repair without requiring a full forensic event export.

---

# 87. Forensic richness should not create a second control plane

Current inspection/Doctor derive reports from canonical Registry/Artifacts rather than
creating another trajectory database.

RF11 supports this architecture.

---

# 88. Current truth and historical truth differ

Example:

```text
At t1 Attempt was Orphaned.
At t2 stronger evidence proves execution Succeeded.
```

Both historical propositions can be true:

```text
Runtime's supported conclusion at t1 was Orphaned.
Later evidence at t2 supported Succeeded.
```

The current best conclusion differs from historical epistemic state.

---

# 89. Append-oriented evidence preserves auditability

By retaining prior evidence and linking supersession, Runtime can explain:

```text
why did we believe X?
what changed?
which evidence strengthened/revised the conclusion?
```

This is superior to mutable “current status only” for recovery.

---

# 90. Confidence

**Confidence** is an observer's degree of belief/calibrated probability about a claim.

It is not identical to:

```text
truth
correctness
completeness
authority
```

---

# 91. Runtime should not invent generic confidence scores

A value like:

```text
confidence = 0.93
```

is meaningless without a calibrated statistical model linking evidence to truth.

Current Runtime mostly uses explicit dispositions/unknown/reconciliation states instead,
which is preferable for hard mechanical contracts.

---

# 92. Confidence can be useful in probabilistic domains but must remain model-owned

A perception system or external classifier may legitimately produce calibrated confidence.

Runtime should preserve that as domain evidence, not reinterpret it as mechanical execution
certainty.

---

# 93. Evidence quality is multidimensional

RF11 rejects a single scalar “evidence strength.” Useful dimensions include:

```text
relevance to claim
identity binding
owner authority
provenance
authenticity
integrity
freshness
completeness
observation coverage
independence/corroboration
model assumptions
```

---

# 94. Correctness versus authenticity versus integrity versus provenance

These axes differ:

```text
Correctness   claim matches Reality/model
Authenticity  source/issuer identity is genuine
Integrity     bytes/state not altered outside contract
Provenance    production/derivation history is known
Completeness  all required members/observations are represented
Freshness     evidence is timely enough for the claim
```

No one axis implies all others.

---

# 95. Same bytes, wrong provenance is a real failure mode

A binary copied from an unauthorized source can have the exact expected digest if it is the
same bytes.

If the requirement is:

```text
this binary was built by pipeline P from source S
```

the digest alone is insufficient; provenance/attestation is required.

---

# 96. Conversely provenance can exist with changed bytes

A provenance record can truthfully say:

```text
file F was produced by pipeline P
```

while F was corrupted afterward.

Integrity evidence is then separately required.

---

# 97. Evidence tuple

RF11 proposes a minimum evidence description:

```text
Evidence E = {
  subject,
  claim_class,
  observer_or_issuer,
  owner_domain,
  method,
  identity_binding,
  observed_or_effective_time,
  scope,
  integrity,
  provenance,
  completeness,
  freshness,
  assumptions
}
```

Not every surface must serialize every field, but the semantics should be reconstructable.

---

# 98. Claim scope should be no larger than evidence scope

Core epistemic law:

```text
ClaimScope ⊆ ProvenEvidenceScope
```

or the claim is an overreach.

P4 violated the stronger target-byte claim but preserved the narrower host-path-witness
claim.

---

# 99. Current Runtime already follows this law in several places

Examples:

```text
semanticCompletionEvaluated=false
runtime_host_namespace_path_witness
holdersTruncated=true
processTreeDisposition=unknown
release ReconciliationRequired
Workspace Patch unknown
```

All are explicit refusals to inflate evidence scope.

---

# 100. Truth surfaces form a graph, not one hierarchy

A Runtime Operation may relate:

```text
Registry commitment
  ↕
Execution Plan / provider binding
  ↕
Runner start/result
  ↕
systemd/cgroup/process observation
  ↕
Artifacts
  ↕
structured Effect receipt
  ↕
Host/domain outcome
```

No single linear ranking is correct for every claim.

---

# 101. Reconciliation is claim-directed graph traversal

RF10 said reconciliation compares surviving truth owners.

RF11 sharpens it:

> Reconciliation selects the evidence surfaces relevant to a specific unresolved claim,
> validates identity/provenance/integrity/scope, then derives the strongest conclusion the
> contract permits.

---

# 102. Reconciliation must not create authority it does not possess

Combining:

```text
process exit 0
+ stdout "done"
+ no error log
```

still cannot prove an external financial order committed if Runtime has no venue receipt or
state query.

More weak evidence does not cross an authority boundary.

---

# 103. Observation can itself mutate the observed system

`task.observe` is intentionally not a pure read: it may reconcile or dispatch an already
admitted Accepted Job.

Therefore:

```text
ObservationSurface != NecessarilyPassiveRead
```

RF1's passive observation / active probing / reconciliation distinction remains necessary.

---

# 104. `task.get` and `task.list` are different epistemic surfaces

They are projection-only Registry reads.

They answer:

```text
what does durable Runtime state currently record/project?
```

without probing physical realization.

---

# 105. `task.observe` can produce fresher physical knowledge

Because it may inspect/reconcile the exact Job against physical/evidence surfaces,
`task.observe` can advance Runtime's durable conclusion.

Thus:

```text
task.get result at t
```

and later:

```text
task.observe result at t+1
```

need not match, even without corruption.

---

# 106. Observation side effects must be part of API semantics

A caller choosing a fresh reconciliation surface should know it can change durable state or
dispatch already-committed work.

Calling such an operation “read” would be misleading.

---

# 107. Global truth database is not justified

One tempting response to multiple owners is:

```text
copy every fact into one universal evidence DB
```

But this creates:

```text
new staleness
new synchronization protocol
new provenance questions
new conflict semantics
new authority ambiguity
```

and still cannot make copied records more authoritative than their source domains.

---

# 108. Better principle: federated authority + identity-bound evidence + reconciliation

Current Runtime architecture is closer to:

```text
keep fact owner where physical authority lives
persist only non-reconstructable Runtime commitments/evidence
bind identities across boundaries
reconcile when needed
project bounded views
```

RF11 strongly supports this.

---

# 109. A digest-bound copy can preserve bytes without inheriting authority

If Runtime copies an external receipt into an Artifact and hashes it, Runtime can prove:

```text
these are the bytes it retained
```

but the semantic authority still comes from the original receipt issuer/contract.

Thus:

```text
EvidenceCustodian != EvidenceIssuer
```

---

# 110. Custody is another provenance role

Runtime can become custodian of evidence:

```text
retain bytes
bind digest
bind Job/Attempt
prevent silent mutation
```

without claiming it authored the external fact.

This is a useful responsibility separation.

---

# 111. Negative claims are especially scope-sensitive

Claims like:

```text
no process remains
no external effect happened
no consumer uses protocol version V
no capacity holder exists
```

require stronger completeness than positive existence claims.

Finding one process proves existence; failing to find one proves absence only under complete
search semantics.

---

# 112. Positive/negative evidence asymmetry

In many domains:

```text
one identity-valid witness of X → strong evidence X exists/occurred
```

while:

```text
one observer did not see X → weak evidence of absence
```

unless coverage is closed and complete.

---

# 113. Stable properties can strengthen negative/finality proofs

Chandy–Lamport discusses stable properties such as termination/deadlock that, once true,
remain true.

If Runtime's contract proves a stable terminal property, later observation need not keep
re-proving a reversible condition.

This differs from volatile claims like “PID is alive now.”

---

# 114. Immutability can convert freshness problems into identity problems

A content-addressed immutable Artifact with digest D does not need continuous re-observation
of bytes if storage immutability/integrity is trusted.

The question becomes:

```text
is this the same Artifact identity D?
```

rather than “are current mutable bytes still equal?”

---

# 115. Current source commitment is not full immutability

Runtime sourceStateDigest strongly identifies a scoped source-world projection, but direct
same-authority writes can still change mutable Workspace Reality after some checks.

Therefore source digest evidence must retain its observation/commit timing and scope.

---

# 116. Proof obligations should be attached to transitions

Before Runtime performs a control mutation, it should know what evidence proves the
precondition.

Examples:

```text
terminal commit requires identity-bound result/control evidence
repair requires matching fingerprint/row versions
workspace close compare-and-close requires expected sourceStateDigest
release reconciliation requires matching Effect receipt
```

This is more useful than collecting telemetry without a decision question.

---

# 117. Evidence collection should follow claim/falsifier needs

A new telemetry source is justified when it can distinguish outcomes that current evidence
cannot distinguish and that matter to a transition/claim.

Otherwise it is observability volume, not increased proof power.

---

# 118. Evidence redundancy versus epistemic redundancy

Two sensors with independent failure modes can improve confidence/diagnosis.

Two aliases of the same underlying source usually do not.

Thus:

```text
DataRedundancy != EpistemicIndependence
```

---

# 119. Evidence completeness can itself be a first-class fact

Examples:

```text
holdersTruncated=false
eventsTruncated=false
nextCursor=null
Artifact.truncated=false
```

These metadata are not presentation trivia. They are evidence about whether closed-set
inferences are allowed.

---

# 120. Missing completeness metadata forces conservative interpretation

If an API returns 10 items and says nothing about truncation/pagination/totality, the safe
interpretation is:

```text
these 10 were observed
```

not:

```text
there are exactly 10
```

unless the API contract guarantees totality.

---

# 121. Error responses also carry epistemic semantics

Current Tool error envelope includes concepts such as:

```text
origin
retryClass
commitState
```

`commitState=not_started` versus `unknown` changes what a caller can infer and safely do.

Thus errors are evidence surfaces, not merely strings.

---

# 122. Commit-state evidence is stronger than generic retryability

For response/retry reasoning:

```text
not_started
committed
unknown
```

answers a more relevant claim than a generic:

```text
retryable=true
```

RF10/RF11 together imply callers should reason from commitment/effect evidence first.

---

# 123. Truth-owner conflict should fail closed at authority boundaries

If Registry says a Runtime Release Job succeeded but canonical Effect receipt is malformed
or mismatched, Runtime should not elevate local process success into deployment success.

Current Release does this by returning reconciliation required.

---

# 124. Truth surfaces can legitimately disagree because they answer different questions

Example:

```text
Registry: Attempt failed
Release receipt: deployment rolled back
```

These may both be true.

The first is local execution classification; the second is Effect outcome.

Do not “resolve” non-conflicting multi-axis facts into one status.

---

# 125. One coarse status is epistemically lossy

Compatibility `status=working/succeeded/...` can remain useful UI, but control logic should
consume explicit dimensions:

```text
attemptState
executionDisposition
deliveryDisposition
recoveryRequired
structured Effect disposition
```

RF11 explains why.

---

# 126. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| process exits 0 but Goal fails | execution evidence = semantic proof | exit status has local scope |
| release receipt says deployed while process history lost | process state owns Effect truth | Effect-owner receipt can be stronger for deployment claim |
| P4 no inotify event but target reads V2 | no observation = no change | observation scope is host namespace only |
| Windows launcher gone with KILL_ON_JOB_CLOSE | absence never proves anything | substrate contract can make absence conclusive |
| Linux unit absent without result | absence = terminal success/failure | may remain lost/ambiguous |
| 50-holder Doctor list marked truncated | every returned item correct = complete set | completeness independent of correctness |
| Attempts truncated but all shown terminal | shown terminals = convergence | unobserved Attempts block proof |
| SQLite read snapshot + live Git read | two coherent reads = one global snapshot | owners are not atomically synchronized |
| same SHA-256 bytes from two origins | byte identity = provenance | provenance is separate |
| valid signature on wrong statement | authenticity = semantic correctness | signer origin/integrity != truth |
| stale Doctor report with correct digest | integrity = freshness | row versions/fingerprint reject stale mutation |
| log says succeeded without Attempt ID | plausible message = evidence | identity binding required |
| Runner Result wrong invocation ID | result file exists = valid terminal evidence | identity contract rejects mismatch |
| Orphaned then late Runner Result | terminal evidence immutable conclusion | stronger evidence may supersede conclusion, not history |
| missing result file before protocol closure | absence = non-occurrence | incomplete observation only |
| missing trace entry after rotation | not logged = never happened | bounded trace cannot prove negative historical claim |
| Artifact digest valid but content wrong | integrity = correctness | byte proof != semantic proof |
| three views derived from same Registry row | evidence count = corroboration | independence/provenance matters |
| old process-alive observation | true once = true now | freshness is claim-relative |
| timestamp later than another | later timestamp = stronger/causal | time labels do not imply authority/causality |

---

# 127. RF11 anti-laws

RF11-L1: `Observation != Reality`.

RF11-L2: `CorrectObservation != SufficientEvidenceForArbitraryClaim`.

RF11-L3: `Record != Truth`.

RF11-L4: `RecordBytes != RecordProvenance`.

RF11-L5: `Evidence != Proof`.

RF11-L6: `AuthoritativeSource != InfallibleSource`.

RF11-L7: `GoodProvenance != SemanticTruth`.

RF11-L8: `ByteIdentity != Provenance`.

RF11-L9: `DigestIntegrity != SemanticValidity`.

RF11-L10: `SignatureValidity != SemanticTruth`.

RF11-L11: `TimestampOrder != CausalOrder`.

RF11-L12: `TimestampPresence != Freshness`.

RF11-L13: `RowVersion != GlobalTime`.

RF11-L14: `PlausibleContent != IdentityBoundEvidence`.

RF11-L15: `DirectEvidence != InfallibleEvidence`.

RF11-L16: `Projection != TruthOwner`.

RF11-L17: `CorrectProjection != CompleteProjection`.

RF11-L18: `Incomplete != Incorrect`.

RF11-L19: `AllObserved != AllExisting when projection is incomplete`.

RF11-L20: `OwnerLocalSnapshot != GlobalWorldSnapshot`.

RF11-L21: `Stale != False`.

RF11-L22: `ObservedAt != FactBeganAt`.

RF11-L23: `NewerRecord != StrongerEvidence`.

RF11-L24: `Supersession != Deletion`.

RF11-L25: `EvidenceCount != IndependentCorroboration`.

RF11-L26: `NoObservedX != ProvenNoX`.

RF11-L27: `MissingResponse/Receipt != NonOccurrence by default`.

RF11-L28: `ObservationMeaning != ObservationAlone`; contract matters.

RF11-L29: `ReceiptName != ReceiptAuthority`.

RF11-L30: `ArtifactExists != ClaimCorrect`.

RF11-L31: `ArtifactDigestValid != SemanticValidity`.

RF11-L32: `Confidence != Truth/Correctness/Completeness`.

RF11-L33: `DataRedundancy != EpistemicIndependence`.

RF11-L34: `EvidenceCustodian != EvidenceIssuer`.

RF11-L35: `OneCoarseStatus != CompleteOutcomeTruth`.

---

# 128. Minimum RF11 grammar

## 128.1 Reality `R`

Target world/system behavior independent of one record.

## 128.2 Fact `F(R,B,t)`

True proposition about Reality/model under boundary B and time t.

## 128.3 Claim `Q`

Proposition asserted/queried by a participant.

## 128.4 Observation `O(observer,target,method,scope,t)`

Measured/read outcome.

## 128.5 Record `Rec(O|commit|claim)`

Representation retaining an observation/commit/claim.

## 128.6 Evidence `E(Q,C)`

Observation/record relevant to Claim Q under Contract C.

## 128.7 Proof `Proof_C(E⇒Q)`

Valid inference from evidence/premises to Q under C.

## 128.8 Truth authority `Owner(F,C)`

Canonical commit/reconstruction authority for fact class F under C.

## 128.9 Provenance `Prov(E)`

Origins/activities/agents/derivation history of evidence/data.

## 128.10 Integrity `Int(E)`

Assurance evidence bytes/state have not changed outside declared contract.

## 128.11 Completeness `Complete(E,D)`

Evidence covers all members/events required by domain D for the claim.

## 128.12 Freshness `Fresh(E,Q,t)`

Evidence remains current enough for Claim Q at time t.

## 128.13 Projection `π_Q(S)`

Question/consumer-relative selected representation of larger state/evidence S.

## 128.14 Receipt `Receipt(owner,effect,outcome)`

Identity-bound domain-owner evidence of a named transition/outcome.

## 128.15 Supersession `E2 >_Q E1`

E2 supports a stronger current conclusion for Q while E1 remains historical evidence.

---

# 129. Current Runtime truth/evidence map

| Current surface | RF11 interpretation / strongest owned claim |
|---|---|
| Git + Workspace filesystem | source/worktree state facts within Git/filesystem scope |
| `sourceStateDigest` | digest of declared source projection at observation/commit point |
| SQLite Registry | canonical Runtime Job/Attempt/reservation/commitment record |
| Registry row version | owner-local CAS/freshness guard |
| Job event history | append-oriented Runtime history evidence, not universal causality |
| systemd/cgroup | Linux live supervision/process-tree facts under systemd/kernel scope |
| Windows Job/launcher | native process ownership/termination facts under Job Object contract |
| Runner start identity | identity-bound realization-start evidence |
| Runner Result | identity-bound local execution terminal evidence |
| terminal-evidence Artifact | durable synthesized execution evidence with provenance/bindings |
| `supersedesArtifactId` | explicit epistemic correction lineage |
| Artifact digest | retained byte identity/integrity evidence |
| Artifact `truncated` | completeness boundary for retained bytes |
| deployment receipt | Runtime Release Effect-domain outcome evidence |
| Patch before/after digests | structured Workspace Patch state-reconciliation evidence |
| `task.get` | projection-only durable Registry view |
| `task.list` | bounded/paginated projection-only discovery view |
| `task.observe` | active targeted observation/reconciliation surface; may mutate durable conclusion/dispatch admitted work |
| inspection SQLite snapshot | coherent Registry-local read view |
| inspection Git/filesystem read | separate physical observation; not same cross-owner transaction |
| `holdersTruncated` etc. | explicit projection completeness evidence |
| Doctor fingerprint/snapshot | repair-plan identity/freshness evidence |
| trace JSONL | bounded diagnostic/compatibility evidence; not canonical execution truth |
| Host TaskOutcome/domain receipt | semantic/domain authority outside Runtime |

---

# 130. What RF11 strongly supports in current design

## 130.1 Multiple scoped truth owners

Keep Git, Registry, process supervisors, Runner evidence and structured Effect receipts as
separate authorities rather than copying them into one universal truth store.

## 130.2 Explicit negative boundaries

Fields such as:

```text
semanticCompletionEvaluated=false
processTreeDisposition=unknown
holdersTruncated=true
ReconciliationRequired
```

correctly prevent overclaiming.

## 130.3 Identity-rich terminal evidence

Attempt/Job/source/provider/process/Artifact bindings turn generic logs into claim-specific
execution evidence.

## 130.4 Supersession without history deletion

Late stronger evidence can update the current conclusion while preserving why the previous
conclusion existed.

## 130.5 Bounded projection metadata

Truncation/cursor/completeness semantics are part of proof safety, not UI decoration.

## 130.6 Snapshot scope honesty

Registry read snapshots remain Registry-local; Git/filesystem/process observations remain
separate.

## 130.7 Effect receipts remain adapter-scoped

Release/Patch evidence does not inflate arbitrary command execution into a domain Effect
proof.

---

# 131. What RF11 reopens

## 131.1 “Truth owner” language should remain fact-scoped in future docs

Existing prose is mostly correct but future writing should avoid unqualified statements like
“X is truth” when the owned fact class is narrower.

No production rename is authorized.

## 131.2 Evidence provenance could become more explicit for future external adapters

If Runtime later consumes remote provider receipts, signatures/issuer identity, retrieval
method and retention semantics may need stronger explicit fields. Current two local
structured Effects do not justify a generic provenance schema.

## 131.3 Projection completeness metadata should remain mandatory where bounded sets support control decisions

Any future bounded list/report should expose truncation/cursor/totality semantics sufficient
to prevent closed-world inference from a prefix.

## 131.4 Confidence scores remain unjustified for mechanical Runtime claims

If probabilistic observers are introduced later, their calibration/owner contract must come
with them rather than Runtime inventing generic confidence.

---

# 132. Research constitution added by RF11

1. Separate Reality, Fact, Claim, Observation, Record, Evidence, Proof and Projection.
2. Use “truth owner” only as fact-scoped canonical commit/reconstruction authority under a
   named contract.
3. Never let authority scope expand from Runtime execution facts into external-domain or
   semantic truth without an owner contract.
4. Evidence is claim-relative; name what proposition a record can actually support.
5. Keep byte integrity, authenticity, provenance, correctness, completeness and freshness
   separate.
6. A digest proves byte identity/change relationships under hash assumptions, not origin or
   semantic truth.
7. A signature proves signer-origin/integrity properties under key/trust assumptions, not
   semantic correctness.
8. Timestamps/generations/row versions prove only the ordering/freshness relationships their
   owner contracts define.
9. Prefer identity-bound evidence over plausible but unbound logs/observations.
10. Never infer complete-set properties from truncated/bounded projections without a
    completeness contract.
11. Treat truncation/cursor/totality metadata as epistemically significant.
12. Keep owner-local snapshots distinct from global-world snapshots; cross-owner consistency
    requires an explicit protocol.
13. Compare evidence strength per claim; do not use universal last-write-wins precedence.
14. Preserve stronger-evidence supersession without deleting prior epistemic history.
15. Require coverage/closed-world assumptions before turning absence of observation into
    proof of absence.
16. Let substrate semantics determine how strong negative evidence can be.
17. Treat receipts as authoritative only when their issuer owns the claimed effect/transition
    and identity/integrity semantics are validated.
18. Keep Artifact existence/digest separate from correctness of Artifact content.
19. Distinguish operational evidence sufficient for safe control from forensic evidence for
    reconstruction; do not create a second control database merely for observability.
20. Do not invent scalar confidence for deterministic mechanical claims without a calibrated
    probabilistic model.
21. Prefer federated scoped authority + identity-bound evidence + reconciliation over one
    copied global truth database.
22. Collect new telemetry only when it closes a real proof gap/falsifier or improves a
    concrete decision boundary.
23. A custodian of evidence does not automatically become issuer/authority for the fact.
24. Do not enter RF12 until composition/transaction claims can state exactly which evidence
    and authority boundaries they cross.

---

# 133. RF11 result — conceptual compression

```text
Reality contains what actually happened/is.
A Fact is a true scoped proposition about Reality/model.
A Claim asserts one proposition.
An Observation is what one instrument saw under a method/scope/time.
A Record preserves an observation/commit/claim.
Evidence is a record/observation relevant to one claim under a contract.
Proof is the valid inference from evidence+premises to that claim.
A truth owner is the canonical commit/reconstruction authority for one fact class—not owner
of Reality itself.
A Projection is a bounded/question-relative view and must expose completeness/freshness
limits.
A Receipt is domain-owner evidence of a named effect/transition.
Provenance tells how evidence came to be; integrity says bytes/state were not altered;
neither alone says the semantic claim is true.
```

Strongest Runtime-specific compression:

```text
Ordivon Runtime does not need one global truth database.
It needs:
  scoped fact owners
  identity-bound durable commitments/evidence
  explicit provenance/integrity/completeness boundaries
  claim-specific reconciliation
  bounded projections that never imply more than they contain
  explicit unknown when evidence cannot close the claim
```

---

# 134. RF11 closeout

RF11 closes with twenty-seven durable results:

1. **Reality, Fact, Claim, Observation, Record, Evidence, Proof and Projection are distinct.**
2. Observation is method/scope/time/observer-relative and does not equal Reality.
3. Evidence is claim-relative; a correct observation can be insufficient for a broader
   claim.
4. Proof requires evidence plus a declared contract/inference, not merely a record.
5. **“Truth owner” is refined to fact-scoped canonical commit/reconstruction authority under
   a contract, not metaphysical ownership of Reality.**
6. Current Runtime therefore correctly remains federated across Git/filesystem, SQLite,
   systemd/cgroup, Windows native ownership, Runner evidence, structured Effect receipts and
   Host/domain authorities.
7. Provenance describes origins/activities/agents and supports trust assessment; it does not
   itself prove semantic correctness.
8. SHA-family digests establish byte identity/change detection under cryptographic
   assumptions but not provenance, authority or semantic validity.
9. Digital signatures add origin/integrity assurance under key/trust assumptions, not truth
   of the signed assertion.
10. Timestamps, generations and row versions prove only time/order/freshness relations their
    owner contracts explicitly define.
11. Identity binding is central evidence strength; unbound logs or results can describe the
    wrong Operation/process.
12. Direct/domain evidence and derived projections are distinct, though direct evidence
    still requires validation.
13. **Correctness and completeness are independent.** A bounded list may contain only correct
    entries while remaining incomplete.
14. Current truncation flags/cursors are therefore epistemically significant; closed-set
    inference must stop when completeness is not proven.
15. SQLite read snapshots give coherent Registry-local views, not global snapshots across
    Git/filesystem/process/receipt owners.
16. Chandy–Lamport-style snapshot theory reinforces that cross-owner global state requires
    an explicit consistency protocol rather than arbitrary multi-source reads.
17. Freshness is claim-relative; stale evidence can remain historically correct while being
    unusable for a current-state claim.
18. Evidence precedence is claim-specific; newer evidence is not automatically stronger.
19. Current `supersedesArtifactId` correctly models stronger late evidence replacing a
    current conclusion without erasing prior evidence history.
20. Independent corroboration can strengthen conclusions, but multiple projections from one
    underlying source do not constitute independent evidence.
21. **Absence of observation is not proof of absence unless coverage/timing/substrate
    contracts make the observation complete.**
22. Windows `KILL_ON_JOB_CLOSE` gives stronger negative process evidence than generic Linux
    unit/PID absence; identical raw observations can mean different things under different
    substrate contracts.
23. P4 proves no host inotify event/path drift cannot establish target-byte consumption;
    evidence scope must remain `runtime_host_namespace_path_witness`.
24. Effect receipts are strongest only for claims their issuer/owner actually commits;
    Release receipt may outrank process status for deployment Effect while remaining
    irrelevant to other claims.
25. Artifact existence/digest/truncation proves retained evidence bytes and completeness
    limits, not semantic correctness.
26. Generic scalar confidence is unjustified for deterministic Runtime mechanical claims;
    explicit unknown/reconciliation states are stronger semantics absent a calibrated model.
27. **The preferred Runtime epistemic architecture is federated scoped authority +
    identity-bound evidence + explicit completeness/provenance/integrity boundaries +
    claim-directed reconciliation, not a synthetic global truth store.**

The next frontier is:

> **How do multiple Operations compose into one larger effect? What is a transaction across
> independently owned boundaries, when can several commits be atomic, what is merely a Saga
> or compensation chain, and how should Runtime avoid pretending that local atomicity
> extends across external systems?**

That is RF12 — Composition, Transactions and Cross-Operation Atomicity.
