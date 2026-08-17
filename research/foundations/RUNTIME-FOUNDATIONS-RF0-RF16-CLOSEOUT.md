---
schema_version: 1
id: runtime.foundations.series.rf0-rf16.closeout
title: Runtime Foundations RF0–RF16 — Complete Research Closeout
type: closeout
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
summary: Complete closeout of the first Runtime Foundations research series. Compresses RF0-RF16 into one world-model transformation record, frozen ontology, meta-laws, implementation interpretation, rejected collapses, engineering consumption contract and continuation handoff.
evidence_status: verified
readiness: CLOSED
related:
  - runtime.foundations.v1
  - runtime.foundations.rf16
---
# Runtime Foundations RF0–RF16 — Complete Research Closeout

## 1. Why this research existed

The first Runtime implementation was built pragmatically around concrete engineering needs:

```text
Workspace
Job
Attempt
Registry
Runner
systemd/cgroup
Windows execution
source-state digests
provider commitments
immutable inputs
Lost/Orphaned recovery
Release/Patch receipts
```

Those mechanisms worked, but the world model behind them was largely implicit. The research
question was therefore not “how should we add more Runtime features?” but:

> What is Runtime actually a system *of*? Which distinctions are fundamental, which are
> implementation accidents, and why did the practical architecture converge on its present
> shape?

RF0–RF16 reconstructed that answer from first principles, mature systems theory and current
code behavior, then attacked the result across local, concurrent, adversarial, transactional,
nondeterministic and open-world cases.

---

## 2. The research arc

```text
RF0   Root problem-space reconstruction
RF1   State / change / event / behavior / dynamics
RF2   Action / intent / proposal / command / operation
RF3   Execution / realization / effect / consequence
RF4   Identity / sameness / replay
RF5   Time / order / causality / concurrency
RF6   Authority / delegation
RF7   Environment / dependency / closure
RF8   Resource / capacity / scheduling / backpressure
RF9   Lifecycle / cancellation / termination
RF10  Failure / recovery / retry / idempotency / exactly-once
RF11  Observation / evidence / proof / truth surfaces
RF12  Composition / transactions / cross-operation atomicity
RF13  Isolation / containment / trust
RF14  Determinism / nondeterminism / reproducibility
RF15  External world / open-system effects
RF16  Cross-domain falsification / Runtime Foundations v1 freeze
```

The sequence was cumulative: later rounds repeatedly falsified naïve collapses introduced by
implementation-shaped thinking.

---

## 3. The largest world-model shift

Before the research, Runtime could easily be described informally as some mixture of:

```text
process manager
job runner
workspace executor
durable task engine
sandbox
transactional executor
```

After RF16, the frozen interpretation is:

> **Ordivon Runtime is a bounded operational commitment, realization and evidence system.**

Its fundamental responsibility is not “run processes.” It is to answer four questions
reliably:

```text
1. What exactly did we commit to do?
2. Under which authority, provider, dependency and resource contract may it be realized?
3. What actually happened, and what evidence supports that claim?
4. After failure or an open-world boundary, how can knowledge/control converge without
   inventing certainty or blindly duplicating consequences?
```

This is the core Runtime problem space.

---

## 4. Frozen Runtime-specific ontology

Runtime Foundations v1 freezes the following minimal conceptual set:

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

`Goal / Semantic Outcome` remains adjacent but outside generic Runtime authority.

### 4.1 Proposal

A candidate operational specification that may still be rejected, resolved or bounded.

```text
Proposal != Operation
```

### 4.2 Operation

The root Runtime **commitment** entity:

> a durable, identity-bearing operational commitment to attempt a bounded realization under
> an admitted contract.

Operation is not the root of Reality; state/behavior/change are deeper concepts. But Operation
is where Runtime begins durable responsibility.

### 4.3 Attempt / Realization

One physical realization episode/generation of an Operation.

```text
Operation != Attempt
Attempt != Process
```

### 4.4 Authority

The reachable consequence/action set actually available under the active boundary. OS token,
filesystem reachability, network access, provider credentials and external write permissions
are all projections of authority.

### 4.5 Resource / Dependency

The things realization consumes, observes or changes. Concrete resource ontologies remain
owner/domain-specific.

### 4.6 Effect

A boundary-relative change, obligation or state transition. One Operation may produce zero,
one, many, partial or opaque Effects.

### 4.7 Observation / Evidence

Information and durable material about what occurred. These are never identical to Reality by
default.

### 4.8 Claim / Projection

A system interpretation supported by scoped evidence/authority. Claims must remain weaker than
or equal to their actual support.

### 4.9 Reconciliation

Evidence-based convergence of local knowledge/control after ambiguity, without assuming that
repeating the original action is safe.

---

## 5. The deepest frozen meta-law: Scope Conservation

RF11–RF15 independently derived five laws:

```text
ClaimScope <= ProvenEvidenceScope
AtomicityScope <= TransactionParticipantScope
IsolationClaimScope <= EnforcedBoundaryScope
DeterminismClaimScope <= ControlledStateBoundaryScope
ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope
```

RF16 showed these are instances of one deeper theorem:

```text
ClaimScope <= ActualSupportScope
```

where `ActualSupportScope` can be established by:

```text
evidence
truth owner
authority
enforcement boundary
transaction participant set
controlled/modelled state
provider contract
identity/retention semantics
observation freshness
```

This is the primary guardrail against category error.

---

## 6. Additional cross-round laws

### 6.1 Identity Conservation

Identity stays attached to the entity whose continuity is actually being claimed.

```text
Request ID != Operation ID
Operation ID != Attempt ID
Attempt ID != Process/provider realization ID
Operation ID != Effect ID
Effect ID != Resource ID
Provider operation ID != Resource ID
Receipt digest != Effect identity
Workspace ID != source-state digest
```

Relations should be preserved rather than namespaces collapsed.

### 6.2 Epistemic Separation

```text
Reality != Observation != Evidence != Projection
```

Unknown, stale, partial and superseded knowledge are legitimate states.

### 6.3 Authority Locality

Systems only safely own actions/claims within actual delegated/enforced authority and truth
ownership.

### 6.4 Open-World Non-Closure

Generic Runtime execution remains affected by uncontrolled world dimensions unless a specific
contract closes them.

### 6.5 Recovery by Truth Convergence

After ambiguity, consult surviving authoritative identity/evidence before repeating
consequential work.

### 6.6 Non-Upgrading Composition

Combining weaker mechanisms does not automatically create stronger guarantees.

```text
atomic steps != atomic workflow
immutable input != deterministic execution
isolated process != isolated external effect
idempotent admission != idempotent provider effect
```

---

## 7. The most important anti-collapses

The series permanently rejects the following shortcuts:

```text
Process = Runtime root
Job = Operation ontology
Workspace = security boundary
State enum = complete world state
Command = Action = Operation = Effect
Operation = Attempt
Attempt = Process
Process exit = semantic completion
Replay = re-execution
Same Operation identity = same result
Source digest = complete execution-world identity
Immutable input = deterministic execution
Sandbox label = complete isolation
Containment = hostile-code isolation
Atomic steps = atomic workflow
Compensation = rollback/time reversal
Idempotent request admission = exactly-once external effect
Receipt = semantic Goal truth
No response = external effect did not occur
Provider acceptance = external effect complete
External effect success = user Goal success
Current external state = complete causal history
Notification/webhook = underlying effect
```

These rejections are as important as the positive ontology because they protect future design
from overclaiming.

---

## 8. What current Runtime implementation now means

| Current construct | Foundations interpretation |
|---|---|
| `ExecutionProposal` | Proposal representation |
| `RuntimeExecutionPlan` | resolved realization contract |
| `operationDigest` | committed Operation identity representation |
| `RuntimeJobRecord` | current durable Operation record/handle |
| `AttemptRecord` | foundational Attempt representation |
| `Workspace` | current source/resource context abstraction |
| Git/worktree | current source/context substrate |
| SQLite Registry | current local durable truth/transaction owner |
| systemd/cgroup | Linux realization/lifecycle/resource substrate |
| Windows Job Object | Windows realization/lifecycle substrate |
| Runner/launcher | realization provider components |
| `trusted_local` | current broad owner-trusted authority profile |
| `contained_local` | current partial isolation/authority-reduction profile |
| source-state digest | scoped source/precondition identity, not world identity |
| immutable inputs | strong named input-integrity/read-only closure |
| Host Dependencies | selected mutable dependency commitment/evidence |
| provider commitment | binding of material realization provider semantics |
| Artifact | durable evidence/content object |
| `deliveryDisposition` | Runtime physical-result certainty only |
| `semanticCompletionEvaluated=false` | explicit generic semantic boundary |
| Runtime Release | Runtime-owned structured Effect contract |
| Workspace Patch | Runtime-owned structured filesystem Effect contract |
| ForeignReference | correlation with foreign identity, not foreign truth ownership |

---

## 9. What is foundational and what is replaceable

### Foundational

```text
Proposal before commitment
stable Operation identity
Attempt/realization distinction
explicit authority
resource/dependency semantics
boundary-relative Effects
observation/evidence separation
explicit uncertainty
reconciliation
scope conservation
identity conservation
epistemic separation
```

### Replaceable implementation

```text
Job schema
Workspace directory abstraction
Git
SQLite
systemd
cgroups
Windows Job Object
Runner executable
current cache topology
/proc/self/fd presentation
/run/ordivon/inputs
current profile names
current MCP schema
one-in-flight policy
service-user model
```

This separation is one of the largest practical benefits of the research: implementation can
evolve aggressively without confusing substrate replacement with ontology failure.

---

## 10. What the research says about current Runtime quality

RF0–RF16 did **not** reveal a foundational architecture failure requiring redesign.

Instead, many previously pragmatic choices survived first-principles analysis:

```text
durable admission before dispatch
request replay without redispatch
Operation/Attempt separation
no speculative redispatch after ambiguous physical dispatch
source-state binding
provider commitment
exact executable identity
explicit authority profiles
immutable-input materialization
Lost/Orphaned/Unknown preservation
separate execution and delivery disposition
semanticCompletionEvaluated=false
receipt-first Release truth
Patch reconciliation with mixed-state unknown
Host ownership of semantic Task continuity
```

The new understanding is that much of the apparent defensive complexity is not accidental
bureaucracy; it encodes real distinctions about identity, authority, evidence and recovery.

---

## 11. What the research does *not* justify

Freezing Foundations v1 does not automatically create engineering work.

In particular, it does not justify adding:

```text
a generic Effect kernel
universal deterministic replay
universal hermetic execution
hostile multi-tenant sandboxing
cross-provider transaction machinery
workflow semantics in Runtime Core
domain Goal evaluation
finance/cloud/email ontologies in Runtime Core
automatic retries after ambiguous external effects
```

Any such work requires concrete consumer pressure and a contract that genuinely owns the
stronger semantics.

---

## 12. The boundary with Host

Runtime now has a much sharper boundary with Host:

```text
Runtime:
  What operational commitment exists?
  What Attempt/realization occurred?
  Under which authority/provider/dependencies?
  What mechanical/effect evidence exists?
  What requires reconciliation?

Host/domain:
  Why are we doing this?
  How do multiple Operations compose toward a Goal?
  Should another Operation be attempted?
  Has the semantic objective been satisfied?
```

This boundary should strongly influence later Host Foundations research.

---

## 13. The boundary with Harness/Agent

Runtime owns the submitted mechanical commitment, not the cognitive process that selected it.

```text
Agent/Harness:
  perceive/reason/choose/propose

Runtime:
  validate/resolve/commit/realize/evidence/reconcile
```

Agent/model nondeterminism is therefore not generic Runtime determinism authority.

---

## 14. The boundary with external domains

For Finance/cloud/email/human-world actions:

```text
Runtime owns mechanical execution evidence
Provider owns provider resource/effect truth
Domain/Host owns semantic interpretation
```

Stronger domain effects require provider/domain-specific IDs, receipts, state machines,
retention semantics and reconciliation surfaces.

---

## 15. FoundationReopenConditions

Runtime Foundations v1 should remain frozen unless a concrete falsifier triggers one of these:

1. A required Runtime responsibility cannot be represented coherently as a durable Operation
   plus realization/evidence lineage.
2. Attempt/Realization cannot model required physical realization continuity.
3. Scope Conservation is directly falsified.
4. Reality/evidence separation becomes invalid under a new explicit truth model.
5. Runtime intentionally becomes owner of generic semantic Goals.
6. Threat model changes to hostile multi-tenancy/arbitrary untrusted-code ownership.
7. Runtime genuinely owns/co-owns a shared cross-provider transaction protocol.
8. Deterministic physical replay becomes a first-class requirement.
9. A new context/resource model reveals a genuinely missing root category, rather than merely
   replacing Workspace/Git.
10. Runtime becomes authoritative owner of a formerly external Effect domain.
11. A new execution substrate cannot map into provider + authority + realization + evidence.
12. Indefinitely reactive controllers become first-class Runtime primitives and bounded
    Operation granularity fails.

Substrate replacement alone is not a reopen condition.

---

## 16. Research outcome versus engineering outcome

The research outcome is complete and frozen.

The engineering outcome should be a separate phase with a different question:

> Given Runtime Foundations v1, where does current Runtime implementation **misrepresent,
> overclaim, hide or unnecessarily complicate** the frozen concepts for real consumers?

That future phase should classify findings into:

```text
A. already correct — preserve
B. correct but poorly exposed — improve interface/documentation
C. contingent complexity with no current value — simplify
D. real mismatch with Foundations — repair
E. research concept with no consumer need — do nothing
```

This prevents theory-driven churn.

---

## 17. Recommended next-conversation entry

Use this exact continuation:

```text
继续 Ordivon Runtime。Runtime Foundations RF0–RF16 已完成并冻结为 v1。
读取 research/foundations/RUNTIME-FOUNDATIONS-V1.md、
RUNTIME-FOUNDATIONS-RF0-RF16-CLOSEOUT.md 和当前 Runtime 实现，
开始 Foundations → Engineering Consumption。
不要重新研究 RF0–RF16，也不要因为理论结论自动重构；先建立 current implementation ×
frozen foundations 的映射，分类 already-correct / poorly-exposed / contingent-complexity /
real-mismatch / no-consumer-need，再决定哪些真正值得改进。
```

---

## 18. Final compression

The full first research series can be compressed to:

```text
Runtime is not “the thing that runs a process.”

Runtime is the system that turns a candidate operational action into a durable, bounded,
identity-bearing commitment; realizes it under explicit authority/provider/dependency
semantics; records what it can actually prove; and conservatively reconciles ambiguity across
failure and open-world boundaries.

Its guarantees are never global by default.
Every strong claim must remain inside the scope of the mechanism and truth owner that actually
support it.
```

Final meta-law:

```text
ClaimScope <= ActualSupportScope
```

Final status:

```text
RF0-RF16                 COMPLETE
Runtime Foundations v1   FROZEN
First research series    CLOSED
RF17                      NOT PLANNED
Next mode                 Foundations -> Engineering Consumption
```
