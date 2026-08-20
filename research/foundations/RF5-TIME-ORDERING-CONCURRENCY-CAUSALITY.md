---
schema_version: 1
id: runtime.foundations.rf5
title: RF5 — Time, Ordering, Concurrency and Causality
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
summary: RF5 separates physical time, clock readings, monotonic duration, logical/registry/protocol orders, happens-before, concurrency, serialization, linearization, observation order and causality. It reclassifies current event_sequence as a per-Job Registry serialization order, observed_at_ms as scoped wall-clock evidence metadata, and monotonic Instant deadlines as local waiting/control semantics rather than historical event truth.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF5
related:
  - runtime.foundations.start
  - runtime.foundations.rf4
  - runtime.foundations.rf5.sources
  - runtime.foundations.rf5.continuation
---
# RF5 — Time, Ordering, Concurrency and Causality

## 0. Status and research question

RF4 established distinct identities and histories. RF5 can finally ask a well-formed
question:

> **Given distinct entities/events/operations, what does before, after, simultaneous,
> concurrent, ordered or caused-by mean? Which relations are physical, clock-derived,
> logical, transactional, protocol-owned, observational or causal?**

RF5 is explanatory research only. It does not authorize changing the Registry event
schema, replacing wall timestamps, adding Lamport/vector clocks, modifying timeout
semantics or changing concurrency admission.

---

# 1. Starting deletion — Time is not one scalar owned by Runtime

A naive Runtime model is:

```text
time = Unix milliseconds
```

This fails because Runtime needs several different functions from “time”:

```text
name an approximate civil/real-world instant
measure elapsed duration locally
express a timeout/deadline
order Registry commits/events
order messages/protocol transitions
reason about causal influence
reason about concurrent operations
construct consistent snapshots
```

One scalar cannot provide all those semantics.

Therefore:

```text
PhysicalTime != ClockReading != LogicalOrder != CausalOrder
```

---

# 2. Physical time and clock readings are distinct

RF5 treats **physical time** weakly as the temporal dimension along which physical
processes evolve.

A clock is a physical/computational mechanism that supplies readings intended to track
some temporal quantity under a clock contract.

Thus:

```text
ClockReading = observation/model of time
```

not:

```text
ClockReading = Time itself
```

Clock resolution, drift, adjustment, synchronization and failure affect what one can
infer from readings.

---

# 3. Wall/realtime clock

A wall/realtime clock is intended to correspond to a shared civil/epoch time coordinate.

Examples:

```text
Unix epoch milliseconds
POSIX CLOCK_REALTIME
provider timestamps
```

It is useful for:

```text
human interpretation
cross-process persisted timestamps
approximate correlation with external evidence
retention/age policy when civil time is intended
```

But realtime clocks can be corrected/adjusted and cannot universally guarantee monotonic
elapsed-time behavior.

Therefore:

```text
WallClockTimestamp != DurationProof
```

---

# 4. Monotonic clock

POSIX defines `CLOCK_MONOTONIC` so its value cannot be set through `clock_settime` and it
cannot jump backwards. POSIX rationale explicitly recommends monotonic clocks for
relative/elapsed timeout behavior because realtime clock corrections can distort waits.

RF5 therefore defines:

> **Monotonic time is a local clock coordinate optimized for measuring progress/duration
> and deadlines within its validity domain, not a portable historical timestamp.**

Thus:

```text
MonotonicClock != WallClock
MonotonicValue != PortableCivilTimestamp
```

---

# 5. Current Runtime already separates monotonic waiting from durable wall timestamps

Current Core uses Rust `Instant` for:

```text
startup grace deadline
task.observe waitMs
short systemd polling deadlines
adaptive polling
```

while durable Registry/evidence fields use:

```text
created_at_ms
started_at_ms
finished_at_ms
observed_at_ms
```

based on Unix/system time or externally supplied Unix evidence time.

RF5 interpretation:

```text
Instant        local monotonic waiting/control coordinate
Unix ms fields persistent wall/observation coordinate
```

This is conceptually sound separation, although later engineering may audit where durable
wall timestamps are used to estimate elapsed durations.

---

# 6. Deadline is not a timestamp by definition

A **deadline** is a temporal constraint:

```text
some condition/action must occur before/at a temporal boundary under clock C
```

It may be represented as:

```text
absolute realtime instant
absolute monotonic instant
relative duration converted to monotonic deadline
protocol lease expiry
```

Therefore:

```text
Deadline != WallTimestamp
```

The clock contract is part of deadline semantics.

---

# 7. Timeout is an observation/control decision, not a fact about remote Reality

A timeout means that under a waiting/control contract, a deadline was reached before the
waiter observed the awaited condition.

It does not prove:

```text
remote work stopped
remote Effect failed
message was never delivered
another observer did not observe completion
```

RF3 already established `Timeout != EffectFailure`; RF5 adds:

```text
TimeoutOccurrence = clock/observer/control-relative event
```

---

# 8. Timestamp order is not event identity or causal order

RF1 already established:

```text
Timestamp != EventIdentity
```

RF5 extends:

```text
timestamp(A) < timestamp(B)
```

can support a clock-relative observed precedence claim if the clock contract is valid,
but does not by itself prove:

```text
A caused B
A was visible to B
A synchronized-with B
A belongs to the same protocol history
```

Thus:

```text
TimestampOrder != CausalOrder
```

---

# 9. Equal timestamps do not imply simultaneity

Finite clock resolution means two distinct ordered events can share the same clock value.
RF1's hybrid/superdense-time falsifier already showed that multiple logically ordered
discrete occurrences can share one real-time coordinate.

Therefore:

```text
SameTimestamp != SameEvent
SameTimestamp != NoOrder
SameTimestamp != PhysicalSimultaneityProof
```

---

# 10. Observation time is not occurrence time

Suppose an external Effect commits at `t_effect`, but Runtime learns about it later at
`t_observe`.

```text
t_effect < t_observe
```

The receipt/log insertion time records when evidence was observed or persisted, not when
the target-world event necessarily occurred.

Thus:

```text
OccurrenceTime != ObservationTime != RecordCommitTime
```

This distinction is central to recovery/reconciliation.

---

# 11. Lamport's happened-before relation

Lamport's distributed-system relation gives RF5 its most important ordering primitive.

For events in a distributed computation, define `→` as the smallest transitive relation
such that:

```text
1. if a and b occur in the same process and a precedes b, then a → b
2. if a is sending a message and b is reception of that message, then a → b
3. transitivity: a → b and b → c imply a → c
```

The relation is a **partial order**.

If neither:

```text
a → b
```

nor:

```text
b → a
```

then the events are concurrent under this model.

---

# 12. Happens-before is not wall-clock-before

Two events can have clock readings that suggest one order while no relevant distributed
information/synchronization path relates them.

Conversely message communication establishes happened-before even if poorly synchronized
wall clocks produce confusing timestamps.

Therefore:

```text
HappensBefore != WallClockBefore
```

This is why logical order can be more trustworthy than timestamp comparison for
coordination semantics.

---

# 13. Concurrent means incomparable under a chosen partial order

RF5 defines concurrency carefully:

> **Two events/operations are concurrent relative to an ordering relation when neither is
> ordered before the other by that relation.**

For Lamport happened-before:

```text
Concurrent(a,b) ⇔ ¬(a→b) ∧ ¬(b→a)
```

This is different from:

```text
“their wall-clock intervals overlap”
```

although interval overlap can be a useful physical/operational indicator.

Thus:

```text
Concurrency != CloseTimestamps
Concurrency != necessarily SimultaneousPhysicalExecution
```

---

# 14. Concurrency is relation-relative

Two operations can be:

```text
concurrent under causal/happens-before order
```

while nevertheless:

```text
serialized by one database writer lock
```

or:

```text
given a deterministic total display order by UUID/timestamp tie-break
```

Therefore the phrase `concurrent operations` must name the relevant semantics when it
affects correctness.

---

# 15. Logical clock

A logical clock assigns values that respect a chosen event order, especially
happened-before:

```text
a → b  =>  C(a) < C(b)
```

The converse does not necessarily hold for scalar Lamport clocks.

Thus:

```text
LogicalClockOrder can extend causal order
```

but:

```text
C(a) < C(b) does not prove a → b
```

unless the clock/metadata scheme has stronger semantics.

---

# 16. Total order can be constructed without discovering total causality

Lamport showed that a total ordering can be obtained by extending logical clock order
with a deterministic tie-break.

This is operationally useful for replicated state-machine style coordination.

But:

```text
TotalOrderExtension != TotalCausalOrder
```

For concurrent events, the chosen total order may be a coordination convention rather
than a discovered physical causal fact.

---

# 17. Current `event_sequence` is a per-Job Registry serialization order

Current `append_event` computes:

```text
MAX(event_sequence) + 1
WHERE job_id = ?
```

inside the SQLite transaction and persists that sequence with the event.

RF5 therefore classifies:

> **`event_sequence` is a Runtime-owned, per-Job total serialization/order of durable
> `job_events` records as appended through Registry transactions.**

It strongly answers:

```text
Which Runtime event record for this Job is before another in Registry history?
```

It does not automatically answer:

```text
Which physical event occurred first in the world?
Which external Effect caused another?
Which event from another Job came first?
```

---

# 18. `event_sequence` is not global

Because sequence allocation is scoped by `job_id`:

```text
Job A event_sequence = 5
Job B event_sequence = 5
```

have no direct ordering relation.

Therefore:

```text
JobEventSequence != RuntimeGlobalEventOrder
```

Cross-Job ordering requires another relation/observation if one is needed at all.

---

# 19. `event_sequence` and `observed_at_ms` encode different orders

An event row contains both:

```text
event_sequence
observed_at_ms
```

`event_sequence` is allocated at durable Registry append.

`observed_at_ms` can represent the observation/evidence timestamp supplied to the
Registry, including Runner/provider-derived evidence times.

Thus the following is semantically possible:

```text
event_sequence(E1) < event_sequence(E2)
while
observed_at_ms(E1) > observed_at_ms(E2)
```

for late-arriving/recovered evidence.

This is not necessarily corruption. It means:

```text
record order != claimed/observed occurrence time order
```

---

# 20. History needs multiple order projections

RF4 established multiple histories. RF5 now gives them different order relations:

```text
Registry append order
Attempt state-transition order
process/program order
message send→receive order
provider protocol order
external effect order
observation order
semantic decision order
```

There is no requirement that all collapse into one universal sequence.

---

# 21. Serialization order

A **serialization order** is an order under which concurrent operations are required or
shown to have semantics equivalent to some sequential execution.

Database serializability is the canonical example.

PostgreSQL's SERIALIZABLE isolation allows transactions to physically overlap while only
allowing committed outcomes that can be explained by some serial one-at-a-time order;
transactions that would violate that condition can abort with serialization failure.

Therefore:

```text
Serializable != PhysicallySequential
```

and:

```text
SerializationOrder != WallClockStartOrder
```

---

# 22. SQLite write serialization is one concrete mechanism, not concurrency ontology

SQLite ordinarily serializes writers so only one database writer proceeds at a time.
This is a physical/mechanical implementation strategy that helps provide serializable
transaction behavior for its database contract.

It does not mean all higher Runtime operations are non-concurrent.

For example:

```text
two callers invoke Registry concurrently
```

and SQLite transaction admission determines which durable commit wins/appears first.

Thus:

```text
DBWriterSerialization != GlobalOperationSerialization
```

---

# 23. Current same-key race demonstrates transactional serialization of admission

Current test launches two simultaneous callers with the same request identity.

The durable result is:

```text
one Created Job
one Existing replay
one active reservation
```

The callers race physically, but the Registry transaction/idempotency constraint gives a
single committed admission result.

RF5 interpretation:

```text
Concurrent invocations
→ serialized durable admission decision
```

without needing to claim one caller was “really first” in wall-clock Reality before the
transaction boundary decides it.

---

# 24. Linearizability

Herlihy/Wing linearizability provides a stronger concurrent-object abstraction.

A concurrent history is linearizable when operations can be placed in a legal sequential
history while preserving each process's operation order and the real-time precedence of
non-overlapping operations.

The useful RF5 structure is:

```text
invocation ───────── response
             ↑
   abstract linearization point
```

The linearization point is a reasoning abstraction between invocation and response; RF3
already prevents treating it as necessarily one physical instruction.

---

# 25. Linearization order is not arbitrary serialization order

For operations that do not overlap in real time:

```text
response(A) occurs before invocation(B)
```

linearizability requires:

```text
A before B
```

in the abstract sequential history.

For overlapping operations, their legal linearization order can be chosen according to
the object's semantics.

Therefore:

```text
Linearizability = sequential semantics + real-time precedence constraint
```

which is stronger in this respect than generic serializability.

---

# 26. Invocation/response order is another distinct relation

RF5 separates:

```text
invocation order
commit/linearization order
response order
```

A slow request can be invoked first but linearize/commit after a faster concurrent
request.

Thus:

```text
InvocationOrder != CommitOrder != ResponseOrder
```

This is directly relevant to concurrent Runtime admissions and external providers.

---

# 27. Commit order is domain/protocol-owned

Different domains define commit/finality differently:

```text
SQLite transaction COMMIT
provider resource acceptance
transaction coordinator decision
filesystem rename visibility
market fill
Runtime Job admission
```

The fact that Runtime observed a response at time `t` does not define the provider's
actual commit order unless the provider contract says it does.

Therefore:

```text
ObservationOrder != DomainCommitOrder
```

---

# 28. Causality is not temporal precedence

The weakest causal anti-law is:

```text
A before B does not imply A caused B
```

Many unrelated events are temporally ordered by a clock/observer.

Thus:

```text
TemporalPrecedence != Causation
```

Likewise correlation/common ordering does not establish causation.

RF5 intentionally does not attempt a full metaphysical theory of causality.

---

# 29. Happens-before is a distributed causal/information relation, not complete physical causality

Lamport motivates happened-before as expressing potential causal influence in a
distributed computation.

RF5 uses that interpretation narrowly:

```text
a → b
```

means the distributed computation contains program/message/transitive structure through
which `a` can be relevant to/influence `b` under the model.

It does not prove:

```text
a was the sole sufficient physical cause of b
```

or enumerate environmental causal channels omitted from the distributed model.

Therefore:

```text
HappensBefore != CompletePhysicalCausation
```

---

# 30. Lineage is not causality

RF3 introduced realization lineage:

```text
Operation → Attempt → Runner → request/effect evidence
```

This is an identity/provenance relation.

It supports causal investigation but does not by itself prove every lineage edge is a
sufficient/necessary cause of every downstream event.

Thus:

```text
Lineage != Causality
```

---

# 31. Synchronization creates ordering information

Synchronization primitives/protocol steps can establish trustworthy order relations:

```text
mutex unlock → later successful lock acquisition
message send → matching receive
transaction commit → visibility under isolation contract
process start evidence → later identity-bound result
```

The exact semantics depend on the synchronization mechanism.

RF5 therefore treats synchronization as **an operation/protocol that constrains possible
histories and creates ordering/visibility guarantees**.

---

# 32. Locks serialize access but do not mean “cause”

If transaction A releases a lock before B acquires it, there is a synchronization/order
relation.

That does not mean every state change in B is causally attributable to A.

Therefore:

```text
SynchronizationOrder != DomainCausation
```

---

# 33. Memory-model happens-before is a specialization, not universal Runtime order

Programming-language/hardware memory models also use `happens-before`-style terms to
specify visibility/order of memory operations.

RF5 retains Lamport's distributed-order ancestry while warning:

```text
same term across memory model / distributed system
!= identical contract
```

Runtime foundations must name the relation's domain.

---

# 34. Consistent global state is not “read every node at the same timestamp”

Chandy–Lamport distributed snapshots show that a meaningful global state of a
distributed system can be captured without stopping the whole system and without
requiring a perfectly synchronized global clock.

The snapshot includes:

```text
process/local states
channel/message state
```

in a way corresponding to a consistent cut of the distributed computation.

RF5 conclusion:

```text
ConsistentGlobalSnapshot != SimultaneousWallClockRead
```

---

# 35. Consistent cut

A cut partitions events into “included past” and “excluded future.”

A cut is causally consistent when it does not include an event while excluding a causal
predecessor required by the modeled relation.

Intuitively:

```text
if receive is in snapshot,
relevant prior send cannot be absent from history/state accounting
```

This gives RF5 a stronger snapshot concept than timestamp equality.

---

# 36. Current Runtime does not own a universal distributed snapshot

`task.get`, Registry snapshots, systemd observations, Workspace state and provider receipts
are acquired through different mechanisms/times.

Therefore a returned Runtime projection should not be interpreted as:

```text
atomic snapshot of all Reality
```

unless a specific operation provides such a contract.

This continues RF1's `State != Reality` and RF4's multi-history model.

---

# 37. “Current” is observer/projection-relative

A Runtime response assembled at time `t` can contain:

```text
Registry state observed now
Runner evidence written earlier
provider receipt fetched now about an earlier Effect
Workspace digest computed during admission
```

Thus `current` often means:

> **the newest justified projection assembled under this observation protocol**

not:

> **every component describes one physical instant.**

---

# 38. Cancel versus natural completion is a race between control and realization

Suppose:

```text
cancel request issued
```

while target execution is naturally completing.

Possible outcomes depend on which state/commit boundary wins and what evidence exists.

Thus:

```text
CancelInvocationOrder != TerminalDisposition by itself
```

Runtime's Registry transition/row-version/terminal evidence semantics arbitrate the
accepted durable history.

This is a concurrency problem, not a timestamp-comparison problem.

---

# 39. Response versus timeout is another race

Suppose:

```text
provider response becomes available
```

near a local deadline.

One observer/thread may hit timeout before seeing the response even though provider work
already completed.

Thus:

```text
LocalTimeoutOrder != ProviderEffectOrder
```

Re-observation/reconciliation is the correct follow-up when identity permits it.

---

# 40. Event record insertion can lag physical occurrence

Runner can physically terminate and write result evidence before Runtime later reconciles
and appends `JOB_TERMINAL`.

Therefore:

```text
PhysicalOccurrence
→ evidence exists
→ Runtime observes evidence
→ Registry event appended
```

can span nontrivial time.

`event_sequence` orders the final Registry step, not the original physical event.

---

# 41. External Effect can precede local terminal evidence

RF3 established:

```text
remote effect commits
→ local process later crashes
→ Runtime observes terminal/ambiguity later
```

So there is no universal chain:

```text
RunnerTerminal → Effect
```

or:

```text
Effect → RunnerTerminal
```

Effect/execution order depends on the operation/effect contract.

---

# 42. Causal order can be partial while audit history is total

For audit/readability, Runtime may choose a total append sequence for Job events.

Reality/protocol can still contain concurrent/incomparable activities.

Thus a correct audit system can have:

```text
partial causal/physical order
        ↓ deterministic recording/serialization
per-Job total event_sequence
```

without claiming the totalization was present in Reality.

---

# 43. Total order is often a coordination resource

A total order can be valuable for:

```text
state-machine replication
transaction serialization
audit presentation
conflict resolution
leader log application
```

But total order has costs and semantic consequences.

RF5 rejects the instinct to add a global total order unless the consumer actually needs
that contract.

---

# 44. Partial order preserves more concurrency information

If two events are truly unordered under the chosen semantics, forcing:

```text
A < B
```

can hide that concurrency and lead later reasoning to infer dependencies that were never
established.

Thus:

```text
More ordering information is not always more truthful information.
```

A partial order can be the more faithful model.

---

# 45. Duration is a difference under one compatible clock contract

For duration reasoning, the safest conceptual form is:

```text
Δt = C(end) - C(start)
```

where both readings come from a compatible monotonic/progress clock or a clock contract
whose adjustment behavior is acceptable.

Subtracting arbitrary wall timestamps is a measurement approximation, not universal
physical duration truth.

---

# 46. Current persisted Runtime latency metrics are diagnostic projections

Current inspection derives distributions such as:

```text
admission → dispatch
dispatch → Runner binding
Runner binding → terminal
cancel → terminal
```

from persisted `observed_at_ms` event records.

RF5 interpretation:

> these are mechanically useful **wall-clock/evidence-timeline diagnostics**, not a
> foundations claim of exact monotonic elapsed duration or causal latency.

The current code uses saturating subtraction, which prevents negative values if evidence
wall times regress but can hide clock-adjustment artifacts.

This is a legitimate future engineering-audit target, not an RF5-authorized refactor.

---

# 47. Age/retention time has different semantics from execution duration

Using wall time for:

```text
“created more than N days ago”
```

can be appropriate because the policy is civil/persistent-time oriented.

Using monotonic time for:

```text
“wait at most 5 seconds now”
```

is appropriate because it is elapsed-control oriented.

Therefore clock choice follows semantic purpose.

---

# 48. Ordering guarantees are scoped contracts

A system can truthfully guarantee:

```text
per-Job event_sequence is total
per-Attempt state transitions obey guards
SQLite writes serialize
message receive follows its send
provider receipt sequence is authoritative for provider order
```

without providing:

```text
global total order of all Runtime/world events
```

RF5 recommends explicitly naming the scope of every ordering claim.

---

# 49. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| realtime clock corrected backward/forward | timestamp = duration | elapsed time needs suitable monotonic clock semantics |
| same timestamp microsteps | equal time = same/no order | finite/superdense semantics allow distinct ordered events |
| two independent machines | wall timestamp order = causal order | unsynchronized/independent events may be causally incomparable |
| message send/receive | only clocks create order | communication establishes happened-before |
| Lamport clock total tie-break | total order = total causality | totalization can order concurrent events conventionally |
| two overlapping DB transactions | physical overlap = non-serializable | concurrent execution may be equivalent to serial order |
| PostgreSQL serialization failure | timestamp decides winner | dependency/serialization contract decides admissible history |
| SQLite one-writer behavior | DB serial writes = global serial Runtime | substrate serialization is scoped |
| same-key simultaneous Runtime calls | caller arrival order = committed request truth | transactional admission serializes one durable identity |
| linearizable object | serialization ignores real time | non-overlapping real-time precedence must be preserved |
| Chandy–Lamport snapshot | global state = simultaneous reads | consistent cut can exist without common clock stop |
| external effect before local crash | process terminal order = effect order | effect/execution timelines may cross |
| late receipt | record time = effect time | observation/recording can lag occurrence |
| cancel vs natural finish | first wall timestamp decides terminal meaning | guarded/commit/evidence race determines durable state |
| response vs timeout | timeout = remote failure | local deadline race is observer-relative |
| per-Job event_sequence | event log = global chronology | Registry total order is local to one Job history |
| event sequence vs observed timestamp | all order coordinates agree | append order and evidence time can disagree legitimately |
| duration from Unix timestamps | wall-clock subtraction = exact elapsed | durable diagnostic metric differs from monotonic duration |

---

# 50. RF5 anti-laws

RF5-L1: `PhysicalTime != ClockReading`.

RF5-L2: `WallClock != MonotonicClock`.

RF5-L3: `WallTimestamp != DurationProof`.

RF5-L4: `Deadline != WallTimestamp`.

RF5-L5: `Timeout != RemoteFailure`.

RF5-L6: `TimestampOrder != CausalOrder`.

RF5-L7: `SameTimestamp != SameEvent`.

RF5-L8: `SameTimestamp != PhysicalSimultaneityProof`.

RF5-L9: `OccurrenceTime != ObservationTime`.

RF5-L10: `ObservationTime != RecordCommitTime`.

RF5-L11: `HappensBefore != WallClockBefore`.

RF5-L12: `Concurrency != CloseTimestamps`.

RF5-L13: `Concurrency != necessarily PhysicalSimultaneity`.

RF5-L14: `LogicalClockOrder != proof of reverse causal implication`.

RF5-L15: `TotalOrderExtension != TotalCausalOrder`.

RF5-L16: `JobEventSequence != RuntimeGlobalEventOrder`.

RF5-L17: `JobEventSequence != PhysicalOccurrenceOrder`.

RF5-L18: `RecordOrder != EvidenceOccurrenceTimeOrder`.

RF5-L19: `Serializable != PhysicallySequential`.

RF5-L20: `SerializationOrder != WallClockStartOrder`.

RF5-L21: `DBWriterSerialization != GlobalOperationSerialization`.

RF5-L22: `InvocationOrder != CommitOrder`.

RF5-L23: `CommitOrder != ResponseOrder`.

RF5-L24: `ObservationOrder != DomainCommitOrder`.

RF5-L25: `TemporalPrecedence != Causation`.

RF5-L26: `HappensBefore != CompletePhysicalCausation`.

RF5-L27: `Lineage != Causality`.

RF5-L28: `SynchronizationOrder != DomainCausation`.

RF5-L29: `ConsistentGlobalSnapshot != SimultaneousWallClockRead`.

RF5-L30: `CurrentProjection != OnePhysicalInstant`.

RF5-L31: `CancelInvocationOrder != TerminalDisposition`.

RF5-L32: `LocalTimeoutOrder != ProviderEffectOrder`.

RF5-L33: `RegistryAppendTime != OriginalPhysicalOccurrenceTime`.

RF5-L34: `MoreTotalOrder != MoreTruth`.

RF5-L35: `WallClockDelta != ExactMonotonicDuration by definition`.

---

# 51. Minimum RF5 grammar

## 51.1 Physical temporal evolution `T_phys`

The underlying temporal dimension/process evolution being modeled.

## 51.2 Wall/Realtime Clock `C_wall`

Persistable/civil-time coordinate; may be adjusted under its clock contract.

## 51.3 Monotonic Clock `C_mono`

Local progress clock suitable for elapsed waits/deadlines under its validity domain.

## 51.4 Observation Time `t_obs`

Clock reading associated with when an observer/evidence source observed/reported a fact.

## 51.5 Record/Commit Order `≺_reg`

Durable storage/Registry serialization order.

## 51.6 Program/Process Order `≺_proc`

Order of events within one modeled sequential actor/process.

## 51.7 Message/Communication Edge `send → receive`

Cross-actor synchronization/information edge.

## 51.8 Happens-before `→`

Transitive partial order generated from process order + communication/synchronization
relations.

## 51.9 Concurrency `||`

Incomparability under a declared partial order.

## 51.10 Serialization Order `≺_ser`

A sequential-equivalence order for a concurrency-control contract.

## 51.11 Linearization Order `≺_lin`

A legal sequential object order respecting required real-time precedence.

## 51.12 Domain Commit Order `≺_commit`

Effect-owner/protocol order in which domain operations become committed/visible/final
under that contract.

## 51.13 Causal Relation `⇒_cause`

A broader modeled causal relation not reducible to clock order; RF5 keeps it intentionally
under-specified beyond anti-collapse laws.

## 51.14 Consistent Cut `Cut(H)`

A causally consistent projection of distributed history/global state.

---

# 52. Current Runtime temporal/order map

| Current mechanism | RF5 role |
|---|---|
| `created_at_ms` | durable wall-clock creation/observation metadata |
| `started_at_ms` | wall/evidence timestamp for bound start projection |
| `finished_at_ms` | wall/evidence timestamp for terminal projection |
| `observed_at_ms` | scoped observation/evidence timestamp carried by event/condition |
| Rust `Instant` | local monotonic wait/deadline coordinate |
| `event_sequence` | per-Job Registry append/serialization total order |
| Attempt guarded transitions | lifecycle/state-machine order constraint |
| SQLite transaction/row version | durable serialization/concurrency arbitration |
| idempotency unique key | concurrent same-request admission convergence |
| systemd/Windows start evidence | synchronization/realization evidence, not global clock |
| Runner result | later identity-bound execution evidence |
| provider/domain receipt | domain-owned occurrence/commit evidence |
| inspection latency | diagnostic wall/evidence timeline projection |

---

# 53. What RF5 strongly supports in current design

## 53.1 Keep monotonic `Instant` for local waits

Startup grace and observe waits should not depend on civil clock adjustments.

## 53.2 Keep `event_sequence` as an explicit Registry order

It gives deterministic per-Job history independent of timestamp ties/regressions.

## 53.3 Do not promote `event_sequence` to causal/global chronology

Its current scope is correctly narrow.

## 53.4 Keep evidence timestamps alongside sequence

Sequence and time answer different questions.

## 53.5 Let SQLite transactions arbitrate concurrent admission atomically

Same-key races should converge through transactional identity/commit semantics rather than
arrival-time guesses.

## 53.6 Keep provider/effect ordering with effect owners

External domain commit order cannot be inferred from Runtime wall time alone.

---

# 54. What RF5 reopens

## 54.1 Persisted latency metrics use wall/evidence timestamps

They are useful diagnostics but can be distorted by clock correction or evidence arrival
latency. Future engineering may choose to record monotonic intervals where persistence
across restart is not required, or explicitly label current metrics as wall/evidence
latency.

No change is authorized here.

## 54.2 `started_at_ms` names observation/binding semantics, not universal target-start time

RF3 already found nested start boundaries. RF5 reinforces that these persisted timestamps
need semantic labels rather than a generic physical-time interpretation.

## 54.3 Cross-Job causal/order queries have no universal current primitive

This is likely correct. A global logical clock should only be added if a real consumer
requires one and can define the synchronization semantics.

## 54.4 External receipt timestamps require owner-clock semantics

Comparing Runtime and provider timestamps without clock uncertainty/contract can only
support approximate correlation, not exact causal order.

---

# 55. Research constitution added by RF5

1. Every time claim must name the clock/order relation when correctness depends on it.
2. Use monotonic clocks for local elapsed waits/deadlines; use wall time for persistent
   civil/history coordinates only under its adjustment semantics.
3. Never infer causality from timestamp precedence alone.
4. Treat happened-before as a partial synchronization/information order, not complete
   physical causality.
5. Define concurrency relative to a named order relation.
6. Keep serialization/linearization order separate from physical execution overlap.
7. Treat `event_sequence` as per-Job Registry history order only.
8. Keep occurrence, observation and record-commit time distinct.
9. Do not infer external domain commit order from Runtime response/record time.
10. Prefer consistent-cut/snapshot contracts over “same timestamp” when reasoning about
    distributed global state.
11. Preserve partial order when no consumer needs a total order.
12. Totalize only when coordination/application semantics require it, and record that the
    order is conventional/contractual where appropriate.
13. Do not use timeout/cancellation ordering as proof of remote Effect absence/rollback.
14. Keep diagnostic latency metrics weaker than causal/performance truth unless their
    clock/evidence contract supports the stronger claim.

---

# 56. RF5 result — conceptual compression

The strongest RF5 compression is:

```text
Reality evolves in time.
Clocks provide scoped observations/coordinates of time.

Wall time supports persistent/civil correlation.
Monotonic time supports local duration/deadline control.

Events can be ordered by many distinct relations:
  process order
  message/synchronization order
  happened-before partial order
  Registry append order
  transaction serialization order
  linearization order
  provider commit order
  observation/record order

Concurrency is incomparability under a declared partial order.
A total order may be a useful extension/coordination convention without being total causality.
Temporal precedence, logical ordering, lineage and causality remain distinct.
```

---

# 57. RF5 closeout

RF5 closes with seventeen durable results:

1. **Runtime has no single universal Time coordinate; physical time, wall-clock,
   monotonic duration, logical order and causal order are distinct.**
2. Wall/realtime timestamps are persistent/civil observation coordinates, not universal
   duration proofs.
3. Monotonic clocks are the proper foundation for local elapsed waits/deadlines; current
   Rust `Instant` use fits this role.
4. `OccurrenceTime`, `ObservationTime` and `RecordCommitTime` are distinct.
5. Lamport happened-before provides a partial program/message/synchronization order that
   is not equivalent to wall-clock order or complete physical causality.
6. Concurrency means incomparability under a declared order relation, not merely close or
   overlapping timestamps.
7. Scalar logical/total order can extend causal order without proving reverse causality.
8. **Current `event_sequence` is a per-Job Registry serialization/history order, not a
   Runtime-global or physical-causal chronology.**
9. `event_sequence` and `observed_at_ms` intentionally encode different order/time
   projections and may disagree without inconsistency.
10. Serializable concurrent operations can physically overlap while admitting a serial
    semantic explanation; serialization order is not wall-clock start order.
11. Linearizability additionally constrains abstract sequential order by non-overlapping
    real-time precedence.
12. Current same-key races correctly rely on transactional admission/identity arbitration
    rather than caller-arrival timestamp ordering.
13. **Temporal precedence, synchronization, lineage and causality are distinct
    relations.**
14. Chandy–Lamport snapshots show consistent global state does not require simultaneous
    wall-clock observation.
15. External Effect/receipt order remains effect-owner/protocol semantics; Runtime wall
    times provide at most scoped correlation unless stronger contracts exist.
16. Current persisted latency metrics are useful wall/evidence-timeline diagnostics, not
    exact monotonic/causal duration truth.
17. Ordivon should preserve partial/scoped orders by default and only introduce wider
    total orders when an explicit consumer requires them.

The next frontier is now:

> **Who is allowed or able to cause/control an Operation? What are authority, capability,
> permission, delegation and control, and why can possession of a capability differ from
> policy permission or semantic authorization?**

That is RF6 — Authority, Capability, Permission and Control.
