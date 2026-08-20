---
schema_version: 1
id: runtime.foundations.rf4
title: RF4 — Identity, Sameness, Replay and History
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
summary: RF4 separates entity identity, request identity, operation identity, attempt identity, effect identity, content identity, equivalence, persistence and history. It defines replay as historical reattachment to an existing identity/commitment rather than re-execution, distinguishes retry/re-execution/re-observation/reconciliation, and reclassifies current Ordivon clientRequestId/requestDigest/operationDigest/Attempt/launch-token/receipt identities by the contracts they protect.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF4
related:
  - runtime.foundations.start
  - runtime.foundations.rf3
  - runtime.foundations.rf4.sources
  - runtime.foundations.rf4.continuation
---
# RF4 — Identity, Sameness, Replay and History

## 0. Status and research question

RF3 ended with several distinct entities:

```text
request
committed Operation
Attempt / realization generation
external Effect
Receipt
Outcome
```

RF4 asks:

> **What makes any two observations, requests, operations, attempts, effects, resources or histories the same thing rather than merely similar or equivalent? What persists while state changes? What exactly is replay, and how is it different from retry, re-execution, re-observation and reconciliation?**

RF4 is explanatory research only. It does not authorize replacing UUIDs/hashes, changing
`clientRequestId` scope, enabling generic retries, adding multi-Attempt execution, or
rewriting Registry history.

---

# 1. Identity must always be identity *of something*

The word `identity` is meaningless without a referent and contract.

These are different questions:

```text
Are these bytes the same content?
Are these two names references to the same resource?
Is this the same request instance?
Is this the same committed Operation?
Is this the same physical Attempt?
Is this the same external Effect?
Is this the same evidence object?
Is this the same semantic Task/Goal?
```

RF4 therefore freezes a first law:

```text
IdentityOf(X) must name X.
```

A digest, UUID, row key or path cannot be interpreted correctly without knowing what
entity/relationship it identifies.

---

# 2. Identity is not sameness of every property

An entity can change state while remaining the same entity.

Examples:

```text
same Job: accepted → running → terminal
same file/resource: content changes
same process: memory/register state changes over time
same remote order: open → partially filled → cancelled
same Task: unresolved → verified
```

Thus:

```text
IdentityPersistence != StateEquality
```

If identity required all properties to remain equal, every state transition would create
a new entity and lifecycle/history reasoning would collapse.

---

# 3. Sameness is relation-relative

RF4 distinguishes **identity** from several weaker sameness/equivalence relations.

## 3.1 Token/entity identity

```text
x is the same entity instance as y
```

## 3.2 Type identity

```text
x and y instantiate the same operation/resource type
```

## 3.3 Content identity

```text
content(x) == content(y)
```

## 3.4 Structural equality

```text
selected fields/structure are equal
```

## 3.5 Semantic equivalence

```text
under contract/model Q, x and y have equivalent meaning/behavior
```

## 3.6 Observational equivalence

```text
for the observer/properties considered, x and y cannot be distinguished
```

None imply universal token identity.

---

# 4. Git is the decisive content-identity counterexample

Git is explicitly content-addressable. A blob/object key is derived from the object's
content plus object header, so identical Git object content maps to the same object
identifier inside the content-addressed object model.

This is powerful but scoped:

```text
Git object identity ≈ content-addressed immutable object identity
```

It does **not** prove:

```text
all resource identity should be content-addressed
```

Two mutable resources can contain identical bytes while remaining distinct resources.
A branch ref can point to different commits over time while retaining the same ref name.

Thus:

```text
ContentIdentity != MutableEntityIdentity
ContentHash != UniversalEntityID
```

---

# 5. UUID proves uniqueness is not semantics

RFC 9562 defines UUIDs as identifiers designed for practical uniqueness and persistence
without central registration.

A UUID can identify:

```text
transaction
row
file name
machine
request
Attempt
```

but the UUID bits do not explain what the identified entity *means*.

Therefore:

```text
UniqueIdentifier != IdentityContract
```

The contract says what must remain stable under that identifier; the UUID merely supplies
an identifier value with collision/administration properties.

---

# 6. Locator is not persistent identity

SQLite provides a useful local falsifier: for ordinary rowid tables whose rowid is not
aliased by `INTEGER PRIMARY KEY`, `VACUUM` can change rowids.

So a highly efficient physical locator can still be unsuitable as persistent application
identity.

General RF4 rule:

```text
Locator != StableEntityIdentity
```

Examples:

```text
PID
memory address
database page/row location
filesystem path
network connection
```

can all be useful locators while entity identity lives elsewhere.

---

# 7. Name is not identity by itself

A name may be:

```text
stable and unique
stable but reusable
mutable alias
scoped to a namespace
human-friendly reference
```

A filesystem path can refer to different underlying content/entities over time. A Git
branch name can move. A DNS name can resolve differently.

Therefore:

```text
SameName != SameEntity
DifferentName != DifferentEntity
```

Identity requires namespace/scope/lifecycle semantics.

---

# 8. RF4 provisional account of identity

The strongest portable account is:

> **An identity is a contract for tracking one entity/instance/commitment across a
> declared scope and lifetime, specifying which changes preserve that entity and which
> differences imply another entity. An identifier is a token/reference used to enact
> that identity contract.**

Compactly:

```text
IdentityContract I_X =
  entity kind X
  + namespace/scope
  + lifetime
  + persistence conditions
  + distinction conditions
  + canonical identifier/reference rules
```

This makes identity partly ontic/operational and partly conventional: a system must name
the distinctions it promises to preserve.

---

# 9. Identity and equivalence must not collapse

Two deterministic computations can produce byte-identical results:

```text
run A → digest D
run B → digest D
```

They may be equivalent with respect to output, but they remain two execution occurrences.

Similarly:

```text
two identical market orders
same symbol/side/price/size
```

can intentionally be two distinct orders.

Therefore:

```text
EquivalentOutcome != SameOperation
SameParameters != SameRequest
SameBehavior != SameOccurrence
```

---

# 10. Request identity

RF4 defines a **Request Instance** as one communicated invocation/candidate intention to
have an operation interpreted under a receiver contract.

Request identity answers:

> **Is this message/call a retransmission/re-observation of the same caller request, or a
> new request that happens to have similar content?**

A stable caller token can represent that continuity.

But the token alone is insufficient: accidental reuse with changed payload must fail.

Thus strong request identity usually has two parts:

```text
Request Key / continuity token
+
Request Fingerprint / semantic shape
```

---

# 11. AWS/IETF idempotency models confirm key + fingerprint + owner scope

AWS EC2 documents the canonical behavior:

```text
same client token + same parameters
→ retry succeeds without further action

same client token + changed parameters
→ IdempotentParameterMismatch
```

It also scopes some idempotency regionally/zonally, showing that key sameness is not
global by definition.

The IETF HTTPAPI Idempotency-Key draft similarly says uniqueness/lifetime are defined by
the resource owner and allows a fingerprint derived from payload/selected fields/digest.

RF4 conclusion:

```text
IdempotencyKey != RequestFingerprint
IdempotencyKeyScope != universal/global scope
```

This is structurally the same distinction current Runtime makes between
`clientRequestId` and request/proposal digest.

---

# 12. Current `clientRequestId` is a request-continuity key, not Operation identity

Current Runtime uses `(principal, clientRequestId)` as the durable idempotency-key lookup
namespace, then verifies the stored request identity.

Therefore RF4 classifies:

```text
clientRequestId
≈ caller request-continuity token within principal/Runtime scope
```

It is not:

```text
Operation digest
Attempt identity
Effect identity
semantic Task identity
```

This explains why same `clientRequestId` with changed request identity produces an
idempotency conflict instead of becoming a new Job.

---

# 13. Request fingerprint/identity digest is not the same as request key

Current Runtime has explicit request/proposal identity digest forms. These encode the
caller-authored/delegated request shape and can normalize equivalent representations
while preserving distinctions that matter.

Current tests demonstrate:

```text
/usr/bin/../bin/true
and
/usr/bin/true
```

can canonicalize to the same proposal identity, while omission of a mechanical limit and
an explicitly authored limit remain different request identities.

Thus:

```text
RequestIdentityDigest = canonicalized request-semantic fingerprint
```

not a random continuity token.

---

# 14. Canonicalization defines an equivalence relation before hashing

A digest can only identify what its canonicalization chooses to distinguish.

If:

```text
canonical(x) = canonical(y)
```

then the digest intentionally treats them as equivalent under that identity contract.

This means:

```text
HashEquality alone does not define semantic identity;
CanonicalizationContract + Hash does.
```

Canonicalization is therefore an identity-policy decision, not mere serialization
cleanup.

---

# 15. Request identity can persist while world state changes

One current test is especially important:

```text
first request identity R
world/source digest = W1
→ admitted Job O(W1)

same request identity R arrives later
current world/source digest = W2
→ returns original Job O(W1)
```

This establishes:

```text
RequestIdentity persists across current-world drift
```

because replay means recovering the historical interpretation of that request, not
reinterpreting it against current Reality.

This is a deep principle, not just an idempotency trick.

---

# 16. Operation identity

RF4 defines a **Committed Operation identity** as:

> the identity of one admitted operational commitment after relevant request choices,
> Runtime-resolved mechanical facts, bound world/precondition state, authority/provider
> commitments and effect-side truth have been concretized under the operation contract.

Current `operationDigest` is a content/fact-derived commitment fingerprint for this
identity, while `jobId` is the durable entity handle/record identity.

Thus current Runtime has two useful coordinates:

```text
jobId             entity/record handle
operationDigest   committed operation-content/contract fingerprint
```

They should not be collapsed.

---

# 17. Job identity is not Operation fingerprint equality

Even if two separately admitted Operations somehow produce equal operation fingerprints
under a future contract, whether they should be one Job depends on the request/operation
identity rules.

Conversely one Job persists while mutable projections change:

```text
status
current Attempt state
resolution
observations
```

Therefore:

```text
JobIdentity != CurrentJobState
JobID != OperationDigest semantic role
```

Current design uses both because they answer different questions.

---

# 18. Attempt identity

An Attempt is one physical realization generation of an Operation.

Attempt identity must distinguish:

```text
this exact dispatch/realization generation
```

from another generation, even if:

```text
same Operation
same executable
same inputs
same result
```

Therefore:

```text
AttemptIdentity != OperationIdentity
```

and:

```text
EquivalentExecution != SameAttempt
```

---

# 19. PID/process identity is weaker/narrower than Attempt identity

A Runtime Attempt can span:

```text
systemd unit
Runner PID
child process tree
Windows launcher
Windows Job Object
```

A process restart/new PID is not automatically a new higher Operation; one physical
Attempt can also involve multiple descendant processes.

Thus:

```text
PID != AttemptIdentity
```

Current Runtime uses stronger start identities, invocation IDs, cgroup/unit identities,
Windows process creation time/image and launch-token binding to avoid PID reuse/aliasing
errors.

---

# 20. Launch token is a lineage-binding credential/nonce, not Operation identity

Current Runtime derives a launch token from:

```text
AttemptId + OperationDigest
```

and stores/compares its digest across bundle/start/result evidence.

RF4 interpretation:

> The launch token binds one concrete realization lineage to both the Attempt and its
> parent Operation commitment.

It is not itself the Operation entity or Attempt entity.

Thus:

```text
LaunchToken != AttemptID != OperationID
```

It is an integrity/binding coordinate between identities.

---

# 21. Effect identity

An external Effect can have an identity distinct from both local Operation and Attempt.

Examples:

```text
provider order ID
payment transaction ID
deployment effect ID
message/resource creation ID
```

One Operation can create multiple Effect events; one Effect can also be realized by
multiple local executions/protocol participants.

Therefore:

```text
EffectIdentity != OperationIdentity
EffectIdentity != AttemptIdentity
```

A structured adapter must define the relation explicitly.

---

# 22. Resource identity is not Effect identity

Creating resource R can be one Effect, but R can continue to exist and change after the
creation Effect is complete.

Similarly deleting R is another Effect concerning the same resource identity.

Thus:

```text
ResourceIdentity != EffectIdentity
```

and:

```text
SameResource can participate in many Effects over time.
```

---

# 23. Delete/recreate falsifies name-based resource sameness

Suppose:

```text
name = "foo"
resource R1 created
R1 deleted
later resource R2 created with same name/content
```

Unless the domain contract explicitly defines name as persistent identity:

```text
R1 != R2
```

This matters for late idempotent retries. A stale token must not accidentally target a
new incarnation merely because the human-readable name is the same.

Therefore idempotency retention/lifetime is part of the effect-owner identity contract.

---

# 24. Receipt/evidence identity

A receipt is an evidence entity and may itself have identity/version lineage.

Current Runtime demonstrates recovered terminal evidence that **supersedes** earlier
orphaned evidence without overwriting history.

Thus:

```text
EvidenceIdentity != FactIdentity
```

Two evidence objects can make claims about the same Attempt/Effect while differing in
quality/time/state.

Preserving both can be important for auditability.

---

# 25. Supersession is not identity replacement

If evidence E2 supersedes E1:

```text
E1 remains a historical evidence object
E2 becomes the stronger/current claim
```

It does not mean:

```text
E1 never existed
```

RF4 therefore separates:

```text
entity identity
current authoritative projection
history of prior claims
```

This becomes important for reconciliation and repair.

---

# 26. Identity persistence requires a lifetime boundary

Every identity contract needs a statement of lifetime:

```text
forever
until explicit deletion
until tombstone expiry
for N hours/days
within process lifetime
within one provider region/cluster
within one Runtime Registry
```

Without lifetime, a late message cannot be correctly classified as:

```text
replay of old request
or
new request reusing an expired key
```

Idempotency keys therefore need retention/expiry semantics owned by the receiver/effect
owner.

---

# 27. Tombstones preserve negative identity history

Deletion does not necessarily mean identity memory should vanish immediately.

A tombstone can preserve:

```text
this identity existed
it is now closed/deleted
this exact close/delete request already committed
```

Current Workspace close uses a tombstone so the same close request can replay after
response loss.

This shows:

```text
Entity no longer physically active
!=
Identity/history no longer relevant
```

---

# 28. What is Replay?

`Replay` is badly overloaded. RF4 rejects the notion that it universally means “run it
again.”

For recoverable Runtime semantics, the strongest definition is:

> **Replay is re-presentation of an already-recognized request identity to the owning
> system, which returns/continues the historical committed interpretation associated with
> that identity instead of creating/reinterpreting a new operation.**

Thus:

```text
Replay = historical identity reattachment
```

not:

```text
Replay = repeat physical behavior
```

---

# 29. Current exact request replay is historical, not reconstructive

Current Runtime deliberately looks up the existing request before inspecting current
provider/world state for a new admission.

Therefore exact replay can return:

```text
original Job
original Execution Plan
original world/source bindings
original provider contract
original operationDigest
```

although current Reality has changed.

This is RF4's central law:

```text
ReplayHistoricalTruth != ReinterpretCurrentWorld
```

---

# 30. Replay is not Retry

RF4 defines **Retry** as:

> **a renewed attempt to achieve an intended operation/effect after a prior attempt/request
> failed, was rejected, timed out or remained incomplete, where the retry contract may
> or may not reuse higher-level identity.**

Retry can lead to new physical work.

Replay by default should not.

Therefore:

```text
Replay != Retry
```

This distinction is more fundamental than whether the API call happens twice.

---

# 31. Retryable error does not mean a committed Operation exists

Current Runtime distinguishes errors with commit state.

Example:

```text
capacity/deployment fence rejects before admission
commitState = not_started
retryable = true
```

The caller may later retry the **same request identity** because no Operation committed.

Contrast:

```text
operation/dispatch outcome unknown or already committed
```

where blind creation of a new identity is unsafe.

Thus:

```text
Retryability != PermissionToCreateNewOperation
```

Commit state matters.

---

# 32. Re-execution is not Replay

**Re-execution** means physically executing the same or equivalent executable
specification again.

Example:

```text
run SHA-256 again
run same test suite again
```

This creates another execution occurrence/Attempt even if deterministic output matches.

Therefore:

```text
ReExecution != Replay
```

A replay may return a retained result with zero new execution.

---

# 33. Retry is not necessarily re-execution of the same Attempt

If a committed Operation later permits another Attempt:

```text
Operation O
  ├─ Attempt A1
  └─ Attempt A2
```

A2 must be a distinct Attempt identity even if it reuses the same Execution Plan.

Therefore:

```text
RetryAttempt != SameAttempt
```

Whether A2 is allowed depends on effect/idempotency/recovery semantics, not identity
similarity.

RF4 does not authorize this feature in current Runtime.

---

# 34. Re-observation is not replay or retry

**Re-observation** means acquiring new evidence about the same entity/history without
repeating the target operation.

Examples:

```text
query systemd again
read provider order status
inspect Workspace files
read deployment receipt
```

Thus:

```text
ReObservation != Replay != Retry
```

A re-observation can reveal that old execution/effect already completed.

---

# 35. Reconciliation is state/history convergence, not repeat work

RF0–RF3 established reconciliation. RF4 sharpens it:

> **Reconciliation uses identity-bound historical commitments plus fresh observations to
> update the system's current justified projection without silently changing which
> historical entity/operation is being discussed.**

So:

```text
Historical identity
+ new evidence
→ revised current claim
```

without:

```text
new Operation by default
```

---

# 36. Result replay is not behavior reproduction

A system may retain a historical Result/Receipt and return it when the request is
replayed.

That gives:

```text
same result object / same historical receipt
```

without reproducing:

```text
same timing
same process path
same nondeterministic choices
same external environment
```

Therefore:

```text
ResultReplay != BehaviorReplay
```

and true behavior reproduction may be impossible.

---

# 37. Deterministic recomputation is equivalence, not historical identity

For pure function:

```text
f(x) = y
```

running `f(x)` tomorrow may produce byte-identical `y`.

This does not make tomorrow's execution the same historical execution occurrence.

Thus:

```text
DeterministicReproduction != SameOccurrence
```

History requires token/event/lineage identity, not only output equality.

---

# 38. Human “do it again” is usually a new request, not replay

If a human intentionally asks:

```text
send the same message again
place the same order again
run the benchmark again
```

the surface content may match the old request but the practical intention is a new
operation occurrence.

Therefore:

```text
SameSemanticWording != Replay
```

The caller should mint a new request identity when the intention is genuinely “another
one.”

This is why idempotency keys encode caller intent about sameness rather than deriving
solely from payload hashes.

---

# 39. Two identical market orders are the decisive intent/identity falsifier

Two orders can have identical:

```text
instrument
side
price
quantity
```

and still intentionally represent two distinct economic operations.

If payload hash defined Operation identity, the second order would be incorrectly
collapsed into the first.

Therefore:

```text
PayloadEquality != OperationIdentity
```

External Finance order identity must preserve intent/request occurrence and venue
contract.

---

# 40. Idempotency is a relation over repeated application, not universal identity

Mathematically/semantically, idempotency concerns whether repeated application changes
the relevant result/effect beyond the first.

But systems idempotency usually combines:

```text
request identity
scope/lifetime
fingerprint matching
effect-owner memory
semantic response behavior
```

Thus:

```text
IdempotentOperation != SameOperationByPayload
```

and:

```text
IdempotencyKey != EffectID by default
```

Some domains may intentionally bind them; that must be explicit.

---

# 41. History is not just a log table

RF4 identifies several distinct histories.

## 41.1 Entity state history

```text
state of entity X over time
```

## 41.2 Event/evidence history

```text
records that particular modeled occurrences/observations were captured
```

## 41.3 Lineage history

```text
request → Operation → Attempt → provider/effect → receipts
```

## 41.4 Causal history

```text
which events/conditions caused/enabled others
```

## 41.5 Version/content history

```text
Git commits / object versions / source state
```

## 41.6 Semantic Task history

```text
plans, decisions, failures, verification/outcome changes
```

These histories overlap but cannot be treated as one sequence.

---

# 42. Append-only history is evidence strategy, not ontology

Current Runtime keeps append-only `job_events`. This is useful because prior observations
and transitions remain auditable.

But RF4 rejects:

```text
History = job_events rows
```

Physical process state, immutable Artifacts, Workspace/Git history, provider receipts and
Registry entity rows can all be stronger truth owners for different claims.

Thus:

```text
EventLog != CompleteHistory
```

---

# 43. History can contain superseded claims without contradiction

Example:

```text
t1: evidence says orphaned
later authoritative recovery:
t2: evidence says succeeded, superseding earlier evidence
```

Both historical facts are true:

```text
At t1 Runtime justifiably classified it orphaned.
At t2 stronger evidence changed the justified projection.
```

So history must distinguish:

```text
what Reality did
what was known when
what claim was current at each point
```

This is a direct continuation of RF0's ontic/epistemic split.

---

# 44. Current state should not erase historical identity

If an Attempt moves:

```text
running → orphaned → recovered succeeded
```

we do not create three Attempts merely because classifications changed.

The Attempt identity persists while the epistemic/operational state evolves.

Thus:

```text
StateCorrection != NewEntity
```

unless the domain contract says the correction discovered an identity mistake rather
than a state mistake.

---

# 45. History and replay require immutable past commitments

A replayable system must retain enough historical truth that a past request does not
silently acquire current semantics.

Examples of required frozen facts may include:

```text
request fingerprint
Operation plan
world/source binding
provider binding
authority/environment
input digests
effect contract
```

If these are recomputed from mutable current configuration during replay, the system is
not replaying historical identity—it is creating a new interpretation under an old key.

RF4 forbids that collapse.

---

# 46. Provider/runtime upgrades must not rewrite old Operation identity

Suppose Runtime/provider bytes change after Job admission.

Exact historical replay must still refer to:

```text
original committed provider contract
```

not the newly installed provider.

Current Runtime explicitly replays existing Jobs before inspecting current provider
state, while new admission binds current provider identity.

This is a strong example of:

```text
HistoricalIdentity != CurrentImplementation
```

---

# 47. Source/Workspace drift must not rewrite old Operation identity

Similarly, if Workspace/source changes after an Operation commits:

```text
old request replay
```

must return the old Operation bound to old source state.

A genuinely new request should receive a new identity and be admitted against the new
world.

Thus:

```text
Replay protects historical interpretation.
New request interprets current world.
```

---

# 48. Same bytes can belong to different resource histories

Suppose two files/resources independently contain identical bytes.

A content digest can truthfully say:

```text
content equivalent
```

while resource identities/history differ:

```text
resource A created by operation O1
resource B created by operation O2
```

Thus content-addressing is excellent for immutable materialization/evidence bytes but
insufficient for mutable resource lineage.

This supports current Runtime's use of digests and UUID-like handles together.

---

# 49. Identity graphs are more appropriate than one universal ID

For a real Runtime operation, the useful relation is:

```text
principal + clientRequestId
        ↓ request-continuity relation
requestDigest
        ↓ admission/binding relation
jobId + operationDigest
        ↓ realization relation
attemptId + launchToken
        ↓ execution evidence
Artifacts / terminal evidence
        ↓ optional effect mapping
external effectId / resourceId / receiptId
```

No one ID should replace all of them because each protects a different equivalence and
lifetime boundary.

---

# 50. Current Ordivon identity stack reclassified

| Current coordinate | RF4 role |
|---|---|
| `principal` | namespace/authority participant in request identity |
| `clientRequestId` | caller request-continuity token |
| `requestDigest` / proposal identity | canonical request-semantic fingerprint |
| `jobId` | durable Runtime committed-Operation record/entity handle |
| `operationDigest` | committed operation contract/world/provider fingerprint |
| `attemptId` | one realization-generation entity identity |
| `attempt_number` | ordinal relation inside one Job history |
| `launchToken` / digest | Attempt↔Operation realization-lineage binding |
| unit/invocation/process-start identity | physical execution-substrate identities |
| Artifact ID/digest | evidence object identity + content integrity |
| runtime release `effectId` | structured deployment-effect identity |
| Workspace Patch `operationId` | direct structured mutation operation identity |
| Workspace ID | mutable working-world entity identity |
| sourceStateDigest | content/world-state fingerprint, not Workspace entity identity |
| Git commit/object digest | immutable source/content identity under Git contract |

---

# 51. What current design now appears strongly justified

## 51.1 Keep `clientRequestId` separate from request digest

One is continuity token; one verifies the semantic request shape.

## 51.2 Keep Job ID separate from operation digest

One identifies the durable entity/record; one fingerprints its committed operational
facts.

## 51.3 Keep Attempt ID separate from Job/Operation

Physical realization occurrences must not collapse into the operation commitment.

## 51.4 Keep launch token as a cross-identity binding

It binds evidence to the right Operation/Attempt lineage rather than trying to serve as
all-purpose identity.

## 51.5 Replay existing identity before current-world checks

This is now a foundation rule for historical fidelity.

## 51.6 Preserve tombstones/superseded evidence

Deletion/recovery should not destroy history required to interpret late retries or prior
claims.

---

# 52. What RF4 reopens

## 52.1 Operation identity may have two notions

Current `jobId` and `operationDigest` already expose a useful distinction between entity
identity and committed-content fingerprint. Later foundations should avoid casually
calling both `Operation identity` without qualification.

## 52.2 Idempotency lifetime needs explicit domain scope

Current Runtime retains durable Registry identities effectively indefinitely within its
active history. Structured external effects may require their own retention/expiry
policies, especially if provider keys expire or resource names can be reused.

## 52.3 Multi-Attempt Operations need a future explicit contract

The ontology permits:

```text
one Operation → multiple Attempts
```

but current ordinary Runtime intentionally does not auto-retry opaque effects. Later RF10
must define when another Attempt preserves Operation identity versus constitutes a new
Operation.

## 52.4 History projection needs layer names

`job_events` remains useful, but future docs should distinguish execution-event history,
evidence history, resource/source history and semantic Task history when ambiguity
matters.

---

# 53. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| same Git blob content | content equality = all entity identity | content-addressed immutable object identity is scoped |
| same branch name after move | same name = same content/version | reference identity can persist while target changes |
| SQLite rowid changed by VACUUM | locator = persistent identity | physical locator may be unstable |
| two UUIDs for same content | content = identifier | surrogate entity identity can differ from content |
| same client token + changed params | key alone = request identity | key + fingerprint/contract required |
| same payload + new client token | payload equality = replay | new request identity can intentionally mean another operation |
| Runtime request replay after source drift | current world defines request | historical request identity returns original committed interpretation |
| same proposal, policy limits changed later | replay recomputes current policy | replay returns original effective plan |
| same command under new principal | command content = operation identity | namespace/authority changes operational identity |
| same Operation after process restart | PID = Operation | process locator can change below Operation identity |
| two Attempts with same plan | same execution spec = same occurrence | each physical generation has separate Attempt identity |
| deterministic recomputation same result | equal output = same execution | equivalence is not occurrence identity |
| two identical market orders | parameters = same operation | caller intent/request identity distinguishes occurrences |
| resource delete/recreate same name | name = persistent resource identity | incarnation/lifetime matters |
| late idempotent retry after deletion | old key can safely map to new resource | key retention/lifetime belongs to effect owner |
| recovered receipt | new evidence = new Effect | evidence can change without repeating Effect |
| superseding terminal evidence | stronger claim erases history | evidence objects/history remain distinct |
| Workspace close replay via tombstone | deleted entity = forgotten identity | tombstone preserves closed identity/history |
| provider upgrade after admission | current provider = historical operation | committed Operation binds historical provider facts |
| same bytes in two Workspace files | digest = resource identity | content identity differs from mutable resource identity |
| “do it again” from Human | same wording = replay | new intention should mint new request identity |

---

# 54. RF4 anti-laws

RF4-L1: `Identity != EqualityOfAllState`.

RF4-L2: `Identity != TypeEquality`.

RF4-L3: `Identity != ContentEquality`.

RF4-L4: `Identity != StructuralEquality`.

RF4-L5: `Identity != ObservationalEquivalence`.

RF4-L6: `Identifier != IdentityContract`.

RF4-L7: `UniqueIdentifier != SemanticMeaning`.

RF4-L8: `Locator != StableEntityIdentity`.

RF4-L9: `SameName != SameEntity`.

RF4-L10: `DifferentName != DifferentEntity`.

RF4-L11: `ContentHash != UniversalEntityID`.

RF4-L12: `PayloadEquality != RequestIdentity`.

RF4-L13: `clientRequestId != RequestFingerprint`.

RF4-L14: `RequestIdentity != OperationIdentity`.

RF4-L15: `RequestIdentity != AttemptIdentity`.

RF4-L16: `JobID != OperationDigest semantic role`.

RF4-L17: `AttemptIdentity != OperationIdentity`.

RF4-L18: `PID != AttemptIdentity`.

RF4-L19: `LaunchToken != AttemptID`.

RF4-L20: `LaunchToken != OperationID`.

RF4-L21: `EffectIdentity != OperationIdentity`.

RF4-L22: `ResourceIdentity != EffectIdentity`.

RF4-L23: `EvidenceIdentity != FactIdentity`.

RF4-L24: `Supersession != HistoricalErasure`.

RF4-L25: `StateCorrection != NewEntity`.

RF4-L26: `Replay != Retry`.

RF4-L27: `Replay != ReExecution`.

RF4-L28: `Replay != ReObservation`.

RF4-L29: `Replay != Reconciliation`.

RF4-L30: `ResultReplay != BehaviorReplay`.

RF4-L31: `DeterministicReproduction != SameOccurrence`.

RF4-L32: `Retryability != PermissionToCreateNewOperation`.

RF4-L33: `RetryAttempt != SameAttempt`.

RF4-L34: `ReplayHistoricalTruth != ReinterpretCurrentWorld`.

RF4-L35: `HistoricalIdentity != CurrentImplementation`.

RF4-L36: `EventLog != CompleteHistory`.

RF4-L37: `CurrentProjection != CompleteHistoricalTruth`.

RF4-L38: `IdempotencyKey != EffectID by default`.

RF4-L39: `IdempotencyKeyScope != UniversalScope`.

RF4-L40: `SameSemanticWording != Replay`.

---

# 55. Minimum RF4 grammar

RF4 retains these coordinates.

## 55.1 Entity Kind `K`

What is being identified?

```text
request
operation
attempt
resource
effect
receipt/evidence
semantic task
```

## 55.2 Identity Contract `I_K`

Defines scope, lifetime, persistence/distinction conditions and canonical reference rules.

## 55.3 Identifier / Handle `id_K`

Token used to refer to an entity under `I_K`.

## 55.4 Fingerprint `fp_K`

Digest/structural representation used to verify content/contract equality for selected
facts.

## 55.5 Equivalence Relation `~_Q`

Purpose-specific sameness weaker than token identity.

## 55.6 Lineage Relation `L`

Connects related but distinct entities:

```text
Request → Operation → Attempt → Effect → Receipt
```

## 55.7 Lifetime / Tombstone `T_K`

Defines when identity remains reserved/remembered after inactivity/deletion.

## 55.8 Historical Record `H`

Versioned/evidentiary facts preserving prior state/claims/lineage.

## 55.9 Replay `Replay(id_request)`

Historical reattachment to the prior committed interpretation of the same request.

## 55.10 Retry `Retry(goal/request context)`

Renewed attempt that may create new physical realization and requires explicit identity/effect
rules.

## 55.11 Re-execution

Another physical execution occurrence of same/equivalent specification.

## 55.12 Re-observation

Acquire new evidence about an existing entity/history.

## 55.13 Reconciliation

Converge current justified projection using historical identity + new observations.

---

# 56. RF4 reconstructed identity graph

```text
Caller / Principal
       │
       │ request namespace
       ▼
clientRequestId ── verifies ── requestFingerprint
       │
       │ replay/reattachment
       ▼
Request Instance
       │ admission/binding
       ▼
Committed Operation
   ┌───────────────┐
   │               │
 jobId         operationFingerprint
   │
   │ realization lineage
   ▼
Attempt A1 ── launch binding ── executor/process identities
   │
   ├─ artifacts/evidence identities
   │
   └─ optional external effect mapping
                  │
                  ▼
             Effect identity
                  │
                  ▼
          resource/receipt identities
```

Arrows mean relations between identities, not aliases.

---

# 57. Replay/Retry decision grammar

A useful RF4 decision skeleton is:

```text
Incoming request with key K
        │
        ├─ no prior identity exists
        │      ↓
        │   new admission candidate
        │
        └─ prior K exists
               │
        fingerprint matches?
          ┌────┴────┐
         no         yes
         ↓           ↓
   identity conflict  return/observe historical entity
                      │
                      ├─ terminal → replay result/history
                      ├─ active   → reattach/observe
                      └─ uncertain→ reconcile same identity
```

A **new retry Operation** is a different branch and requires proof/contract that new
physical work is valid.

---

# 58. RF4 result — conceptual compression

The strongest RF4 compression is:

```text
Identity is not sameness of state/content.
Identity is a persistence/distinction contract for a named entity kind across a scope and lifetime.
Identifiers enact that contract; fingerprints verify selected facts; equivalence is weaker and purpose-relative.

Request identity says “this is the same caller request.”
Operation identity says “this is the same admitted operational commitment.”
Attempt identity says “this is the same physical realization generation.”
Effect/resource/receipt identities belong to their own owners/contracts.

Replay means historical reattachment to the existing request/operation identity.
Retry may create new physical work.
Re-execution repeats execution.
Re-observation gathers evidence.
Reconciliation updates justified state about an existing identity/history.
```

---

# 59. RF4 closeout

RF4 closes with eighteen durable results:

1. **Identity must always name the entity kind, scope and lifetime being identified.**
2. Identity persists across selected state changes and is not equality of all properties.
3. Token identity, content equality, structural equality, semantic equivalence and
   observational equivalence are distinct relations.
4. Content-addressed identity is powerful for immutable objects but is not a universal
   mutable-resource identity model.
5. Identifier values (UUID/hash/name/row locator) do not themselves define the semantic
   identity contract.
6. Request identity is best modeled as stable continuity key + semantic fingerprint under
   receiver-owned scope/lifetime.
7. Current `clientRequestId` and request/proposal digest correctly occupy these two
   different roles.
8. **Replay preserves historical request interpretation across current-world/source/policy/provider drift.**
9. Current Job/entity handle and operation digest are distinct and useful: entity identity
   versus committed operational fingerprint.
10. Attempt identity is one physical realization generation and is strictly below
    Operation identity.
11. Launch token is an integrity/lineage binding between Attempt and Operation rather than
    either entity's identity.
12. External Effect, resource and receipt identities are separate from local Runtime
    Operation/Attempt identity and require explicit mappings.
13. Identity lifetime/tombstones matter for late messages, delete/recreate and idempotency
    safety.
14. **Replay != Retry != ReExecution != ReObservation != Reconciliation.**
15. Deterministic reproduction/equal result does not make two execution occurrences the
    same historical occurrence.
16. History is multi-projection: entity-state, evidence/event, lineage, content/version,
    causal and semantic histories are distinct.
17. Superseding evidence/state correction should preserve prior evidence/history rather
    than rewriting the past.
18. Current Ordivon Runtime's layered identity stack is substantially supported: no one
    universal ID should replace request, Operation, Attempt, execution-lineage and effect
    identities.

The next frontier is now safe to enter:

> **Given distinct entities and histories, what does it mean for events/operations to be
> before, after, concurrent, causally related or mutually exclusive? Which clocks and
> orders are physical, logical, protocol-owned or observer-imposed?**

That is RF5 — Time, Ordering, Concurrency and Causality.
