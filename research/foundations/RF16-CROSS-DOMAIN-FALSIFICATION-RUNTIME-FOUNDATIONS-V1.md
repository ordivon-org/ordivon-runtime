---
schema_version: 1
id: runtime.foundations.rf16
title: RF16 — Cross-Domain Falsification and Runtime Foundations v1 Reconstruction
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
summary: RF16 attacks RF0-RF15 as one theory across deterministic, concurrent, adversarial, cross-platform, transactional, failure, and open-system cases. No prior foundation round requires reopening. Runtime Foundations v1 is frozen around a minimal ontology in which Runtime is a bounded authority-and-evidence system for durable operational commitments: a Proposal may be admitted into a Committed Operation; one Operation may have zero or more Attempt/Realization episodes under explicitly bound authority, dependencies and provider semantics; Effects and observations may cross into owner-specific domains; evidence and reconciliation refine knowledge without collapsing truth scopes; semantic Goal completion remains outside generic Runtime. Job, Workspace, systemd/cgroup, Windows Job Object, SQLite, Git, and current profile names are useful implementation forms rather than ontological roots. The RF11-RF15 scope laws unify into a Scope Conservation Law: no claim may exceed the scope of the mechanism, evidence, authority, participant set, controlled state or truth owner that actually supports it.
evidence_status: verified
readiness: FROZEN_V1
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF16
related:
  - runtime.foundations.start
  - runtime.foundations.rf15
  - runtime.foundations.v1
  - runtime.foundations.rf16.continuation
---
# RF16 — Cross-Domain Falsification and Runtime Foundations v1 Reconstruction

## 0. Purpose

RF16 does not add another conceptual layer. It attacks RF0-RF15 as one system and asks:

> Which distinctions survive all tested domains strongly enough to freeze as Runtime
> Foundations v1, which current structures are contingent implementation choices, and what
> precise conditions would justify reopening the Foundations later?

No production implementation change is authorized by RF16 itself.

---

# 1. Final falsification result

The cross-domain attack did **not** find a contradiction requiring RF0-RF15 to reopen.

Instead, it found that the rounds are mutually reinforcing when their scopes are preserved.

The main source of apparent contradiction was repeatedly the same pattern:

```text
one layer's useful representation
was accidentally treated as another layer's universal ontology
```

Examples:

```text
Process -> Runtime root
Job -> Operation ontology
Workspace -> security boundary
AttemptState -> complete world state
exit code -> external effect truth
receipt -> semantic Goal truth
source digest -> world digest
sandbox label -> complete isolation
request replay -> physical replay
```

All fail under cross-domain attack.

---

# 2. The deepest root is not Operation

RF1 remains correct: below Runtime-specific concepts lies a more general world of:

```text
state / behavior / change / event / transition / dynamics / observation
```

Runtime does not create these categories.

Therefore:

```text
Operation != universal ontological root
```

---

# 3. Operation is nevertheless the correct Runtime root commitment entity

RF2-RF4 survive every attack if Operation is scoped correctly.

RF16 freezes:

> **A Runtime Operation is a durable, identity-bearing operational commitment to attempt a
> bounded realization under an admitted contract.**

This is the central Runtime entity because it is the point where:

```text
proposal
+ principal/authority
+ preconditions/context
+ realization/provider contract
+ selected dependencies
+ identity
```

become a durable system responsibility.

---

# 4. Why Process cannot replace Operation

Pure local execution falsifier:

```text
Operation admitted
Attempt not dispatched yet
```

There is already a valid Runtime commitment with no process.

Structured Patch falsifier:

```text
Runtime effect exists
no generic target process required
```

External asynchronous effect falsifier:

```text
local process exits
provider operation continues
```

Thus:

```text
Process != Runtime root entity
```

---

# 5. Why Job cannot replace Operation ontologically

Current `RuntimeJobRecord` is an excellent durable record/handle for one admitted execution
lineage.

But `Job` is a product/data-model form combining:

```text
Operation identity
request continuity
current desired state
resolution summary
Attempt lineage
Workspace references
```

A future implementation could rename/restructure the durable record while preserving the
same Operation semantics.

Therefore:

```text
Job = current durable operational record/handle
Job != foundational ontological root
```

---

# 6. Attempt survives as a foundational realization concept

RF3/RF4/RF10 survive strongly:

> **Attempt is one physical realization episode/generation of a committed Operation.**

An Operation can exist with:

```text
0 realized Attempts
1 Attempt
potentially >1 Attempts under a future explicit retry/resume contract
```

Current Runtime mostly implements one physical dispatch lineage; the concept is broader than
the current cardinality policy.

---

# 7. Realization survives below process implementation

A realization may use:

```text
Linux process + systemd unit
Windows process + Job Object
future VM
future remote worker
structured local mutation
```

Hence:

```text
Realization != Process
Realization != systemd unit
Realization != Windows Job Object
```

Those are provider/substrate forms.

---

# 8. Effect survives, but only as boundary-relative

RF1/RF3/RF10/RF12/RF15 jointly support:

> **An Effect is a change/obligation/state transition considered relative to an explicitly
> named world/resource/domain boundary and owner.**

One Operation may realize:

```text
zero effects
one effect
many effects
partial effects
opaque unknown effects
```

Therefore:

```text
Operation != Effect
Attempt != Effect
ProcessExit != Effect
```

---

# 9. Evidence survives as a separate foundational axis

RF11 is essential under every falsifier.

Reality and our knowledge of Reality are distinct.

Thus Runtime Foundations v1 freezes:

```text
Occurrence/State != Observation != Evidence != Claim
```

Evidence must retain:

```text
source/owner
identity scope
observation time/freshness when relevant
completeness/truncation
supersession relation where applicable
```

---

# 10. Authority survives as a root cross-cutting concept

Every realization is meaningful only relative to what the actor/provider is permitted and
able to reach.

RF6/RF7/RF13/RF15 jointly establish:

```text
Authority = reachable action/consequence set under a boundary
```

Authority is not merely Unix UID or IAM role.

It can include:

```text
filesystem
process
network
credentials
provider APIs
resource identities
input authorities
external write permissions
```

---

# 11. Resource survives, but not as one concrete class

Runtime acts with/on resources:

```text
source trees
files
CPU/memory/process slots
executables
input objects
provider resources
external orders
cloud objects
```

`Resource` is therefore foundational as a generic relation target, but concrete resource
ontologies remain owner/domain-specific.

---

# 12. Observation survives, but is not automatically authoritative

An observation is acquired information about a world/resource.

Authority and freshness determine what claims it can support.

Therefore:

```text
Observation != Truth
Observation != Effect
Observation != CurrentStateWithoutFreshnessContract
```

---

# 13. Goal and Semantic Outcome are deliberately outside Runtime Core ontology

RF2/RF3/RF11/RF15 all survive only if Runtime refuses to own generic Goal semantics.

Runtime can know:

```text
what Operation was committed
what Attempt happened
what mechanical evidence exists
what structured Effect owner reports
```

It cannot generically know:

```text
whether the user's broader Goal was wise
whether business objective succeeded
whether human understood
whether trading strategy was good
```

Thus:

```text
RuntimeSemanticAuthority < Host/DomainSemanticAuthority
```

for generic operations.

---

# 14. Minimal frozen ontology

RF16 freezes the following as the smallest Runtime-specific conceptual set:

```text
Proposal
Operation
Attempt / Realization
Authority
Resource / Dependency
Effect
Observation / Evidence
Claim / Projection
Reconciliation
```

with external Goal/Semantic Outcome explicitly adjacent but not generic Runtime-owned.

---

# 15. Why Proposal is foundational

There must be a state before commitment where a candidate action can be:

```text
validated
rejected
resolved
bounded
```

without already existing as a durable Operation.

So:

```text
Proposal != Operation
```

is foundational, not an API accident.

---

# 16. Why Reconciliation is foundational

Failures, delayed evidence, external owners and partial observations make post-hoc truth
convergence unavoidable.

Reconciliation is not merely a repair command; it is the epistemic/control operation:

```text
surviving local state
+ provider/process/resource evidence
-> strongest justified current claim
```

without blindly repeating the original Effect.

---

# 17. State machine is not the ontology

Current `AttemptState` is useful operational compression.

But RF1, RF10 and RF15 show multiple independent axes:

```text
realization phase
execution outcome
delivery certainty
recovery requirement
external effect state
semantic completion
```

Therefore Foundations v1 freezes:

```text
OneEnum != CompleteSystemStateOntology
```

---

# 18. Workspace is contingent, not foundational

Current Workspace provides:

```text
source context
physical working tree
source-state identity
mutation boundary
coordination/reservation scope
```

But Runtime could operate on:

```text
immutable object trees
remote CAS snapshots
container images
database resources
non-Git source packages
```

without changing the core Operation model.

Thus:

```text
Workspace = current source/resource context abstraction
Workspace != Runtime ontology root
```

---

# 19. Git is contingent

Git currently gives Runtime powerful source identity/diff/workspace semantics.

But Foundations v1 only requires:

```text
resource/context identity
precondition/equivalence model
```

not Git specifically.

---

# 20. SQLite is contingent

RF12 validates current Registry transactions because SQLite actually owns those local
participants atomically.

But the Foundation is:

```text
Durable atomic commitment within the Registry owner domain
```

not:

```text
SQLite forever
```

---

# 21. systemd/cgroup is contingent

Current Linux realization substrate provides excellent:

```text
process ownership
lifecycle
resource control
recovery evidence
```

but RF13 proves these are one provider implementation.

A VM/remote worker could satisfy equivalent higher-level contracts differently.

---

# 22. Windows Job Object is equally contingent

It is a native realization/lifecycle substrate, not the definition of Attempt or containment.

---

# 23. `trusted_local` and `contained_local` are contingent profiles

Foundations v1 freezes the concepts:

```text
authority profile
isolation dimensions
trust/threat model
```

not these exact profile names or mechanisms.

---

# 24. Current input materialization is a strong implementation of a deeper law

The deeper concept is:

> named dependency/input commitments may be frozen/materialized when the contract requires
> stronger identity/integrity continuity.

`/run/ordivon/inputs` itself is contingent.

---

# 25. Current provider commitment is foundational in shape, contingent in representation

The durable idea:

```text
if realization semantics materially depend on provider P,
Operation identity must bind enough of P's contract/identity to prevent silent substitution
```

The exact `ExecutionProviderSnapshot` struct is current representation.

---

# 26. Falsifier: pure deterministic local computation

Case:

```text
immutable input
pure deterministic program
no external effects
```

Survives model:

```text
Proposal -> Operation -> Attempt -> Result Evidence
```

No Effect abstraction is required for the output unless a chosen world boundary interprets
artifact creation as an Effect.

Conclusion:

```text
Effect is optional/boundary-relative, not mandatory one-per-Operation.
```

---

# 27. Falsifier: multithreaded nondeterministic computation

Same Operation can realize varying traces/results because scheduler state is outside the
controlled determinism boundary.

No contradiction:

```text
Operation identity = historical operational commitment
not result-content identity
```

RF14 survives.

---

# 28. Falsifier: immutable input + mutable host dependency

One dependency is frozen; another is witnessed/mutable.

No contradiction if claims stay per dependency class.

This directly supports scope conservation rather than a scalar “reproducible execution” flag.

---

# 29. Falsifier: trusted-local private namespace reinterpretation

P4 target constructs another mount view.

No contradiction:

```text
Runtime host-namespace witness claim remains true
Target-consumed-byte proof was never claimed
trusted_local retains broad authority
```

RF7/RF11/RF13 survive together.

---

# 30. Falsifier: contained-local hostile kernel assumption

If target exploits same kernel, current contained-local may fail hostile-code containment.

This does not falsify Foundations because current contract explicitly excludes kernel-grade
hostile isolation.

It instead confirms:

```text
IsolationClaimScope <= EnforcedBoundaryScope
```

---

# 31. Falsifier: Windows limited versus elevated

Same operation shape under different Windows authority can have different reachable Effects.

Therefore authority is material operation semantics when it changes capability/blast radius.

Current identity rules are directionally supported.

---

# 32. Falsifier: Registry COMMIT ambiguity

Local commit can succeed while response/connection reports failure/uncertainty.

Registry readback/reconciliation resolves within its owner domain.

This supports:

```text
commit reality != caller knowledge
```

at the local database level, foreshadowing RF15 external cases.

---

# 33. Falsifier: HTTP POST commits then response is lost

No contradiction:

```text
Runtime local Attempt may be terminal/failed/unknown
External Effect may already be committed
```

The model explicitly allows divergent truth axes.

---

# 34. Falsifier: provider idempotency expires before Runtime replay identity

Runtime correctly returns/references the historical Operation.

It must not infer that resubmitting to provider today is still deduplicated.

This confirms independent identity/retention domains.

---

# 35. Falsifier: finance partial fill + cancel race

A single Order resource has multiple real Fill Effects and a cancel operation can race them.

No contradiction because:

```text
Operation != one Effect
Cancel != rollback
Partial Effect is first-class
```

RF3/RF10/RF12/RF15 reinforce one another.

---

# 36. Falsifier: async cloud create + stale read

Provider operation is IN_PROGRESS or SUCCESS while a read replica may lag.

No contradiction when:

```text
provider operation state
resource state
observation freshness
```

remain separate.

---

# 37. Falsifier: SMTP accepted + bounce + human nonresponse

Three layers can be:

```text
responsibility accepted
technical delivery/failure later
semantic human outcome unknown
```

This is exactly what the three-authority model predicts.

---

# 38. Falsifier: execPlan with partial external effects

Step1 can commit an external Effect; Step2 can fail.

`stopOnError` prevents future steps but cannot rollback Step1.

No contradiction because:

```text
Sequence != Transaction
ControlFlow != Atomicity
```

---

# 39. Falsifier: Runtime self-release during response loss

Generic execution evidence can be interrupted by self-replacement while the deployment receipt
still proves Effect state.

This validates:

```text
Effect-owner receipt can outrank generic process observation for that Effect fact
```

without making receipt globally authoritative.

---

# 40. Falsifier: Workspace Patch mixed state

Some files match before-state, others after-state.

Correct result remains:

```text
unknown
```

rather than pretending transactional success/failure.

RF11/RF12 survive.

---

# 41. Falsifier: Host Goal continues after Runtime terminality

Runtime process succeeds, but higher-level Goal remains unresolved.

No contradiction because:

```text
Runtime terminality != semantic completion
```

This validates current `semanticCompletionEvaluated=false` as foundational boundary, though
that exact field is implementation representation.

---

# 42. Falsifier: multiple truth owners disagree

Example:

```text
Runtime receipt at t1 says deployed
external health observer at t2 says unhealthy
```

No contradiction if claims are scoped:

```text
receipt -> deployment frontier/history fact
health observer -> later service-health fact
```

Different facts/times can coexist.

---

# 43. No global truth table is required

The theory does not need one master state that flattens all owner facts into one enum.

Instead:

```text
owner-scoped facts
+ causal/identity relations
+ observation/evidence scopes
+ reconciliation
```

compose a partial world model.

---

# 44. The Scope Conservation Law

RF11-RF15 appear different but are instances of one deeper theorem.

RF16 freezes:

> **A system claim must not exceed the scope actually established by the mechanism, owner,
> participant set, controlled state, authority boundary or evidence that supports it.**

Formal schematic:

```text
ClaimScope(C) <= SupportScope(M,E,A,P,S,O)
```

where support may include:

```text
M mechanism
E evidence
A authority/enforcement
P participant/transaction set
S controlled state
O owner/truth domain
```

---

# 45. RF11 as Scope Conservation

```text
ClaimScope <= ProvenEvidenceScope
```

A claim cannot outrun what evidence proves.

---

# 46. RF12 as Scope Conservation

```text
AtomicityScope <= TransactionParticipantScope
```

Atomicity cannot outrun the commit owner/participants.

---

# 47. RF13 as Scope Conservation

```text
IsolationClaimScope <= EnforcedBoundaryScope
```

Isolation cannot outrun enforcement.

---

# 48. RF14 as Scope Conservation

```text
DeterminismClaimScope <= ControlledStateBoundaryScope
```

Determinism cannot outrun controlled/modelled inputs/state.

---

# 49. RF15 as Scope Conservation

```text
ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope
```

External-world claims cannot outrun the owner's API/receipt/query/control semantics.

---

# 50. Unified theorem does not erase specific laws

The specific laws remain useful because their support scopes differ operationally.

Scope Conservation is the meta-law; RF11-RF15 laws are domain-specialized corollaries.

---

# 51. The second deep law — Identity Conservation

Cross-domain falsification reveals another recurring invariant:

> **Identity must remain attached to the object/event/owner whose continuity is actually being
> claimed.**

Thus:

```text
Request ID != Operation ID
Operation ID != Attempt ID
Attempt ID != Process ID
Operation ID != Effect ID
Effect ID != Resource ID
Provider operation ID != Resource ID
Receipt digest != Effect identity
Workspace ID != source-state digest
```

---

# 52. Identity links are relations, not forced unification

A robust Runtime should preserve mappings:

```text
Proposal/Request
   -> Operation
      -> Attempt(s)
         -> provider/process realization IDs
      -> Effect correlations
         -> external provider IDs
      -> Evidence/Artifacts
```

without collapsing namespaces.

---

# 53. The third deep law — Epistemic Separation

Cross-domain cases repeatedly require:

```text
Reality != Observation != Knowledge/Projection
```

Therefore:

> Runtime must preserve unknown/partial/stale/superseded evidence states rather than convert
> lack of evidence into fabricated reality claims.

---

# 54. The fourth deep law — Authority Locality

An actor/system may only safely own claims/actions within the authority it actually has.

Examples:

```text
Runtime owns process execution truth
provider owns external resource truth
Host/domain owns Goal semantics
```

Authority can be delegated, attenuated and evidenced, but not inferred from correlation.

---

# 55. The fifth deep law — Open-World Non-Closure

Generic Runtime execution is not a closed world.

Unless a contract explicitly closes dependencies/effects:

```text
time
RNG
scheduler
filesystem
network
humans
external providers
other actors
```

may continue influencing outcomes.

Therefore generic Runtime must prefer bounded claims + evidence over fantasies of complete
closure.

---

# 56. The sixth deep law — Recovery by Truth Convergence

When failures create uncertainty:

```text
recover by consulting surviving authoritative identities/evidence
```

not by assuming absence and repeating effects.

This unifies:

```text
Registry commit readback
Runner late evidence
Patch reconciliation
Release receipt query
external provider query
```

---

# 57. The seventh deep law — Composition Does Not Upgrade Guarantees

Putting mechanisms in sequence does not automatically strengthen them.

Examples:

```text
atomic steps != atomic workflow
isolated process != isolated external effect
idempotent admission != idempotent external API
immutable input != deterministic execution
trusted tool != safe result
```

Guarantees compose only when their owner/participant/identity contracts genuinely compose.

---

# 58. Runtime Foundations v1 definition

RF16 freezes the following high-level definition:

> **Ordivon Runtime is a bounded authority, realization and evidence system that turns an
> admitted operational proposal into a durable Operation commitment; realizes that Operation
> through one or more explicitly governed Attempt episodes; preserves identity-bound evidence
> about what it controlled and observed; and supports conservative reconciliation across
> failure and external boundaries without claiming semantic, transactional, deterministic,
> isolation or effect guarantees beyond the scopes it actually owns.**

---

# 59. Compact Runtime grammar v1

```text
Goal / semantic objective                 [external to generic Runtime]
   ↓ delegation
Proposal P
   ↓ validate + resolve + authority/precondition binding
Committed Operation O
   ↓ realization policy
Attempt / Realization A_i
   ↓ interacts with
Resources / Dependencies R
   ↓ under
Authority / Isolation / Provider Contract K
   ↓ produces or correlates
Effects E_j
   ↓ observed through
Observations / Evidence V_k
   ↓ interpreted as scoped
Claims / Projections C
   ↓ after ambiguity
Reconciliation
```

---

# 60. What Runtime fundamentally owns

Runtime Foundations v1 says Runtime may fundamentally own:

```text
admission/commit identity for Runtime Operations
Attempt/realization identity it creates
its own durable Registry facts
its own dispatch/control protocol
its own Artifacts/evidence validation
its own configured authority/provider commitments
structured Effects whose resource/equivalence/receipt contract Runtime truly owns
```

---

# 61. What Runtime fundamentally does not own generically

```text
user Goal correctness
Agent psychological/planning intent
arbitrary target process semantics
complete environment state
complete dependency closure
arbitrary external Effect state
provider resource truth it merely calls
human semantic outcome
cross-owner atomicity
universal hostile-code isolation
universal deterministic replay
```

---

# 62. Current implementation mapping

| Current construct | Foundations v1 status |
|---|---|
| `ExecutionProposal` | strong representation of Proposal |
| `RuntimeExecutionPlan` | concrete resolved realization contract representation |
| `operationDigest` | current Operation identity commitment representation |
| `RuntimeJobRecord` | durable Operation record/handle; not ontology root |
| `AttemptRecord` | direct representation of foundational Attempt concept |
| `Workspace` | current source/resource context abstraction |
| `sourceStateDigest` | scoped resource/source precondition identity |
| `ExecutionProviderSnapshot` | provider commitment representation |
| systemd unit/cgroup | Linux realization/lifecycle substrate |
| Windows Job Object | Windows realization/lifecycle substrate |
| `trusted_local` / `contained_local` | current authority/isolation profiles |
| Runner/launcher | current realization providers/components |
| SQLite Registry | current durable local truth/transaction owner |
| Git | current source identity/workspace substrate |
| `Artifact` | durable evidence/content object |
| `deliveryDisposition` | mechanical result certainty projection |
| `semanticCompletionEvaluated=false` | representation of semantic boundary |
| Release | Runtime-owned structured Effect contract |
| Workspace Patch | Runtime-owned structured local Effect contract |
| ForeignReference | correlation/link to foreign owner identities |

---

# 63. What is foundational versus contingent

## Foundational

```text
proposal before commitment
stable Operation identity
Attempt/realization distinction
owner-scoped authority
resource/dependency identity
boundary-relative Effects
observation/evidence separation
explicit uncertainty
reconciliation
scope conservation
identity conservation
epistemic separation
```

## Contingent/current

```text
Git worktree
Workspace directory
SQLite
systemd
cgroups
Windows Job Objects
/proc/self/fd presentation
/run/ordivon/inputs
current profile names
current exact schema fields
one in-flight writer policy
current service-user model
current MCP surface
```

---

# 64. Where current Runtime is overfit

RF16 finds no harmful foundational overfit, but identifies implementation-local assumptions:

```text
Git-centric source context
local filesystem Workspace paths
Linux systemd/cgroup lifecycle provider
Windows native launcher/Job Object provider
service-user local trust model
current cache topology
```

These are acceptable because the code does not generally elevate them into universal theory.

---

# 65. Where current Runtime engineering intuition was already unusually sound

Before explicit Foundations research, implementation already encoded several principles that
survived the theory attack:

```text
durable admission before dispatch
idempotent same-request replay
no speculative redispatch after ambiguous dispatch
Operation/Attempt separation
source-state commitment
provider commitment
exact executable paths
explicit authority profiles
immutable-input materialization
explicit unknown/Lost/Orphaned states
separate delivery disposition
semanticCompletionEvaluated=false
receipt-first Release reconciliation
Patch mixed-state unknown
Host ownership of semantic Task continuity
```

RF16 interprets these not as accidental complexity but as empirical discoveries of the deeper
laws now made explicit.

---

# 66. Where research changes our interpretation more than implementation

Large portions of RF0-RF15 should **not** immediately create new code.

Their primary value is to prevent future category errors:

```text
calling contained_local a hostile sandbox
calling execPlan a transaction
calling exact replay a rerun
calling sourceStateDigest a world snapshot
calling exit zero semantic success
calling Runtime delivery external effect commitment
putting Finance order states into Runtime Core
```

---

# 67. Immediate implementation changes authorized by RF16

None are mandatory merely because Foundations v1 is frozen.

This is deliberate.

A foundation should explain and constrain engineering; it should not manufacture work when the
current implementation already satisfies the discovered laws.

---

# 68. Candidate future engineering consequences, only on consumer pressure

Potentially justified later:

```text
more explicit isolation-dimension projection
structured build/reproduction adapter
stronger external isolation provider commitment
provider-specific Effect adapters in domain layers
explicit observation freshness/version fields in domain projections
additional exact provider reconciliation surfaces
```

Each requires concrete consumer evidence.

---

# 69. FoundationReopenConditions

Runtime Foundations v1 must reopen only when a concrete observation falsifies a frozen
principle or reveals a missing root category.

---

# 70. FRC-1 — Operation root commitment fails

Reopen RF2/RF4/RF16 if a required Runtime responsibility cannot be coherently represented as a
durable Operation commitment plus realization/evidence lineage without severe distortion.

Example candidate:

```text
continuous autonomous controller whose identity cannot be decomposed into bounded operations
```

Current Runtime scope intentionally excludes this as its root workload form.

---

# 71. FRC-2 — Attempt concept fails

Reopen RF3/RF4 if a real realization model cannot be represented as one or more identity-
bearing realization episodes without losing essential continuity.

---

# 72. FRC-3 — Scope Conservation fails

Reopen the relevant round if a mechanism can validly support a claim beyond the scope of its
participants/evidence/enforcement/control/owner under a precise counterexample.

This would be a deep theoretical falsification.

---

# 73. FRC-4 — Reality/evidence separation fails

Reopen RF1/RF11 if a future architecture proves a domain where observation/evidence and ontic
state can safely be identified without hidden epistemic assumptions.

No current domain supports that collapse.

---

# 74. FRC-5 — Generic semantic completion becomes Runtime-owned

Reopen RF2/RF3/RF15/RF16 if Runtime is intentionally redesigned to own the user/domain Goal
model rather than only operational realization.

That would be a product/architecture boundary shift, not a small feature.

---

# 75. FRC-6 — Runtime becomes hostile multi-tenant execution authority

Reopen RF6/RF7/RF13 if threat model changes from owner-trusted local engineering to mutually
hostile tenants/arbitrary untrusted code.

Security ontology remains valid, but current profile interpretations/TCB assumptions require
major reopening.

---

# 76. FRC-7 — Generic multi-provider transaction becomes real

Reopen RF10/RF12/RF15 if actual providers participate in one shared prepare/commit/abort or
equivalent transaction protocol that Runtime owns/co-owns.

Do not reopen for mere orchestration or Saga compensation.

---

# 77. FRC-8 — Deterministic physical replay becomes a product requirement

Reopen RF14 if Runtime must recreate prior low-level execution traces/results rather than only
historically replay durable identities.

This would require explicit time/RNG/scheduling/external-I/O capture/control contracts.

---

# 78. FRC-9 — Workspace ceases to be sufficient current context abstraction

This is an implementation reopening, not Foundation reopening, unless it reveals a missing
resource/context root category.

A move from Git worktrees to CAS snapshots alone does not reopen Foundations.

---

# 79. FRC-10 — External Effect ownership changes

Reopen RF15 only if Runtime itself becomes authoritative owner of a previously external domain
state machine, such that current provider/domain boundary assumptions materially change.

---

# 80. FRC-11 — New physical substrate

A remote worker, microVM, WASM runtime, AppContainer or accelerator does **not** automatically
reopen Foundations.

It should map into:

```text
provider + authority + realization + evidence
```

unless it falsifies those categories.

---

# 81. FRC-12 — Long-running reactive systems

If Runtime expands from bounded operations to indefinitely reactive control loops as first-
class primitives, reopen RF0/RF1/RF2/RF16 to determine whether Operation remains the right root
commitment granularity.

---

# 82. Runtime Foundations v1 constitution

1. Runtime is not a process manager ontology; process is one realization substrate.
2. Runtime is not a workflow/Goal engine ontology; semantic Goal completion stays outside
   generic Core.
3. Proposal and committed Operation are distinct.
4. Operation is the root Runtime **commitment** entity, not the root of Reality.
5. Attempt/Realization is distinct from Operation and from process/provider identity.
6. Operation, Attempt, Effect, Resource and provider identifiers retain separate identities.
7. Effects are boundary/owner-relative and may be zero, one, many, partial or opaque per
   Operation.
8. State, observation, evidence and claim remain distinct.
9. Authority is explicit and owner/boundary scoped.
10. Dependencies/resources are bound only to the strength required by their contract.
11. Isolation, atomicity, determinism, idempotency and effect finality are never scalar global
    properties without scope.
12. Unknown is a legitimate state when evidence cannot decide.
13. Reconciliation converges surviving truth; it is distinct from retry/re-execution.
14. Exact replay means historical Operation continuity unless a stronger replay contract is
    explicitly introduced.
15. External Reality remains open unless a provider contract closes a dimension.
16. Runtime owns mechanical/operational truth only where it has actual authority/evidence.
17. Provider/domain owners remain authoritative for their resource/effect state.
18. Host/domain owns semantic multi-Operation convergence and Goal judgment.
19. Stronger guarantees require stronger owner/participant/mechanism contracts, not labels.
20. Composition does not automatically upgrade guarantees.
21. Preserve identity across failures before attempting recovery.
22. Bind provider/authority/dependency identity when substitution would change Operation
    semantics.
23. Prefer conservative ambiguity over fabricated certainty.
24. Keep implementation substrate replaceable beneath the Foundation grammar.
25. Do not generalize from current Git/systemd/SQLite/Windows mechanisms into universal
    ontology.
26. Add generic Core abstractions only when multiple real consumers demonstrate shared
    semantics, not merely shared words.
27. Delegate domain-specific Effect ontologies to the layer that owns their truth semantics.
28. Scope every claim to its supporting truth/mechanism boundary.
29. Freeze Foundations by falsification, reopen only on explicit FoundationReopenCondition.
30. Governing meta-law: `ClaimScope <= ActualSupportScope`.

---

# 83. The final meta-law

RF16 compresses the entire late Foundations series into:

```text
ClaimScope <= ActualSupportScope
```

where `ActualSupportScope` can be established by one or more of:

```text
evidence
owner authority
enforcement boundary
transaction participant set
controlled/modelled state
provider contract
identity retention
observation freshness
```

A claim that exceeds support scope is not “optimistic”; it is a category error.

---

# 84. Runtime Foundations v1 frozen statement

```text
Ordivon Runtime
is a bounded operational commitment / realization / evidence system.

It accepts candidate Proposals,
commits admitted Operations,
realizes them through governed Attempts,
binds material authority/provider/dependency semantics,
preserves identity-bound evidence,
and reconciles uncertainty conservatively.

It can correlate and sometimes own Effects,
but it does not generically own external Reality or semantic Goals.

Its guarantees are always scope-qualified.
```

---

# 85. Final answer to “why is current Runtime constructed this way?”

The research began because current Runtime appeared to be a collection of practical
mechanisms:

```text
Workspace
Job
Attempt
Registry
systemd
cgroup
Runner
provider digests
source digests
immutable inputs
Lost/Orphaned
Release/Patch receipts
```

RF0-RF16 now shows a deeper explanation.

These mechanisms repeatedly solve four invariant problems:

```text
1. What exactly did we commit to do?
2. Under what authority/world/dependency contract may it be realized?
3. What actually happened, and what evidence supports that claim?
4. After interruption or an open-world boundary, how do we recover without inventing truth or
   duplicating consequences?
```

That is the real Runtime problem space.

Current implementation is one strong realization of it, not its definition.

---

# 86. RF16 closeout

RF16 freezes Runtime Foundations v1 with these durable conclusions:

1. No RF0-RF15 contradiction requires reopening a prior round.
2. State/behavior/change are deeper than Runtime-specific entities; Operation is not a
   universal ontology root.
3. **Operation survives as the correct root Runtime commitment entity.**
4. Proposal, Operation, Attempt/Realization, Authority, Resource/Dependency, Effect,
   Observation/Evidence, Claim/Projection and Reconciliation form the minimal Runtime-specific
   ontology.
5. Goal/Semantic Outcome remains adjacent but outside generic Runtime authority.
6. Process, systemd unit/cgroup and Windows Job Object are realization substrates, not roots.
7. Job is a current durable Operation record/handle, not foundational ontology.
8. Workspace/Git are current resource/source-context abstractions, not universal foundations.
9. SQLite is the current Registry transaction owner, not a foundational requirement.
10. `trusted_local`/`contained_local` are current profiles implementing deeper authority/trust/
    isolation concepts.
11. One Operation may have zero/many/partial/opaque Effects; Operation and Effect identities
    must remain distinct.
12. Reality, Observation, Evidence and Claim remain distinct across every tested domain.
13. Authority is a foundational reachable-consequence relation, broader than OS credentials.
14. Reconciliation is foundational because both local failures and external/open systems
    produce durable epistemic ambiguity.
15. `semanticCompletionEvaluated=false` represents a genuine foundational semantic boundary,
    though the field itself is contingent.
16. Cross-domain falsifiers for nondeterminism, namespace reinterpretation, hostile assumptions,
    Windows authority, COMMIT ambiguity, response loss, finance partial fills, cloud async
    state, SMTP/human outcome, execPlan partial effects, Release/Patch ambiguity and truth-owner
    disagreement all preserve the theory when scopes remain explicit.
17. RF11-RF15 laws unify under **Scope Conservation**.
18. Identity Conservation, Epistemic Separation, Authority Locality, Open-World Non-Closure,
    Recovery by Truth Convergence and Non-Upgrading Composition are additional durable
    cross-round laws.
19. Current Runtime engineering already embodies many of these laws; the research primarily
    explains and constrains future evolution rather than demanding immediate refactoring.
20. No engineering work is automatically created by freezing Foundations v1.
21. Twelve explicit FoundationReopenConditions prevent future conceptual drift while allowing
    substrate evolution without unnecessary foundational reopening.
22. **Runtime Foundations v1 is frozen.**
23. **Governing meta-law: `ClaimScope <= ActualSupportScope`.**
