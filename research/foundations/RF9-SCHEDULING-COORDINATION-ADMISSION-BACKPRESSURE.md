---
schema_version: 1
id: runtime.foundations.rf9
title: RF9 — Scheduling, Coordination, Admission and Backpressure
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
summary: RF9 separates eligibility, admission, queueing, scheduling, dispatch, coordination, flow/backpressure, overload and load shedding. It establishes that current Runtime has durable admission arbitration and at-most-once dispatch mechanics but no capacity-waiting scheduler/queue; distinguishes admitted dispatch deferral from queueing; interprets CONCURRENCY_LIMIT as explicit upstream backpressure/admission refusal; and explains why adding a queue would create a new durable waiting-work ownership model rather than a convenience wrapper.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF9
related:
  - runtime.foundations.start
  - runtime.foundations.rf8
  - runtime.foundations.rf9.sources
  - runtime.foundations.rf9.continuation
---
# RF9 — Scheduling, Coordination, Admission and Backpressure

## 0. Status and research question

RF8 established that Runtime has scarce logical admission capacity but is not a scheduler.
RF9 asks:

> **When multiple Operations could run but cannot all receive service now, who decides
> what is accepted, what waits, what runs next, what is rejected, what is delayed, and how
> does overload propagate back to the producer without hiding ownership or creating
> unbounded latent work?**

RF9 is explanatory research only. It does not authorize adding a Runtime queue, priority,
fairness/aging policy, EDF/SJF scheduler, automatic retries, load shedding policy,
pressure-aware admission or background redispatch worker.

---

# 1. First deletion — “scheduler” is overloaded

Systems commonly call all of these scheduling:

```text
checking whether work may enter
placing work in a waiting queue
choosing which waiting item goes next
binding work to a worker/node
crossing a physical dispatch boundary
OS selecting runnable threads for CPU
retrying rejected work later
```

RF9 separates them.

---

# 2. Eligibility

**Eligibility** asks whether work is presently a candidate for some next transition given
its semantic/state prerequisites.

Examples:

```text
request structurally valid
Workspace exists
Operation not already terminal
Attempt is Accepted with committed bundle
provider preconditions currently match
```

Eligibility does not mean capacity is available or execution has been admitted.

Therefore:

```text
Eligible != Admitted
```

---

# 3. Admission

RF9 defines **Admission** as:

> **The decision/commit boundary at which a system accepts ownership of work under a
> declared contract and commits whatever capacity/identity obligations that acceptance
> entails.**

For current Runtime, successful admission creates durable:

```text
Job
Attempt
Operation identity
concurrency reservation
```

before physical dispatch.

---

# 4. Admission is not dispatch

RF3/RF8 already imply:

```text
Admit
→ durable commitment/reservation
→ later physical dispatch
```

Current Runtime explicitly separates these boundaries.

Thus:

```text
Admission != Dispatch
```

---

# 5. Dispatch

**Dispatch** is the transition by which already committed/eligible work is handed across
the physical execution-provider boundary for realization.

Current Runtime first commits `dispatch_issued` / Attempt `starting` and then crosses into
systemd/Runner or Windows launcher mechanics.

Therefore dispatch carries at-most-once/recovery semantics distinct from admission.

---

# 6. Dispatch order is not admission order by theorem

Two Jobs can be admitted in one order but physical dispatch/runner start may be observed in
another order because of:

```text
provider verification
thread interleaving
OS scheduling
reconciliation
platform latency
```

Unless a contract explicitly orders them:

```text
AdmissionOrder != PhysicalStartOrder
```

RF5 total-order cautions apply.

---

# 7. Queue

RF9 defines a **Queue** as more than a collection:

> **A stateful contract that owns accepted-to-wait work and provides rules for insertion,
> retention, ordering/selection, cancellation, expiry, capacity, recovery and removal.**

A real queue therefore requires:

```text
queued-item identity
owner
queue capacity
ordering/scheduling policy
lifetime/expiry
cancellation
restart durability
admission relationship
failure semantics
```

---

# 8. Queueing is not simply “not running yet”

A Job can be:

```text
admitted
reserved
Accepted
not yet dispatched for a contract-specific reason
```

without being in a capacity-waiting scheduler queue.

Therefore:

```text
NotYetRunning != Queued
```

---

# 9. Current `release.apply` proves admitted dispatch deferral exists without a capacity queue

Current Runtime Release deliberately:

```text
admits durable Accepted Job
returns admission projection
later reconciliation dispatches it
```

because immediate self-replacing release could remove the initiating MCP connection before
the durable acceptance is visible.

RF9 classifies this as:

```text
AdmittedDispatchDeferral
```

not:

```text
CapacityWaitingQueue
```

The Job already owns a reservation and Operation identity.

---

# 10. Current ordinary `workspace.exec` is admit-then-immediate-dispatch-attempt

For ordinary newly created Jobs, Runtime releases the lifecycle admission critical section
and then calls the dispatch path immediately.

Thus the default shape is:

```text
admit
→ reserve
→ release lifecycle lock
→ attempt dispatch
```

not:

```text
admit into waiting queue
→ scheduler later selects
```

---

# 11. Capacity-waiting work is currently not Runtime-owned

When current global/Workspace admission capacity is full, the new request fails before a
new Job/Attempt/reservation is committed.

Therefore there is no durable Runtime object representing:

```text
“please run this request when a slot becomes available”
```

This is a central current boundary.

---

# 12. `CONCURRENCY_LIMIT` is admission refusal, not queued acceptance

Current capacity rejection includes:

```text
scope
active count
limit
holder Job identities
holder Workspace identities
retryable=true
retryAfterMs
```

but no new Operation identity is committed.

Therefore:

```text
CONCURRENCY_LIMIT
= PreCommitAdmissionRefusal + CapacityEvidence
```

not:

```text
QueueReceipt
```

---

# 13. Retry hint is not a reservation

`retryAfterMs=1000` communicates a suggested retry interval.

It does not promise:

```text
a slot will exist in 1000ms
caller has priority then
original request is remembered by Runtime
```

Thus:

```text
RetryAfter != Reservation
RetryAfter != ScheduledWakeupGuarantee
```

---

# 14. Retry is not queueing

If an upper layer receives `CONCURRENCY_LIMIT`, waits and submits again later:

```text
request attempt 1 rejected pre-commit
upper layer waits
request attempt 2 later re-evaluated
```

that is **retry/re-admission**, not Runtime queueing.

Therefore:

```text
Retry != Queue
```

RF10 will study retry semantics in depth.

---

# 15. Scheduling

RF9 defines **Scheduling** as:

> **Selecting when/where/which eligible competing work receives service or dispatch under
> a policy, subject to resource and coordination constraints.**

A scheduler therefore needs at least:

```text
candidate set
eligibility state
resource/capacity facts
selection policy
ordering/preemption semantics
```

---

# 16. Scheduler is not admission controller

Admission asks:

```text
may/should we accept this work into our owned responsibility set now?
```

Scheduling asks:

```text
among owned eligible contenders, who receives service now/next?
```

Some systems combine the mechanisms, but the concepts differ.

Thus:

```text
AdmissionControl != Scheduling
```

---

# 17. Scheduler is not dispatcher

Scheduler can decide:

```text
run Job B next
```

while dispatcher executes the physical mechanism that launches/binds B.

Therefore:

```text
SchedulingDecision != DispatchEffect
```

This separation is crucial for at-most-once dispatch recovery.

---

# 18. Coordination

RF9 uses **Coordination** for mechanisms/rules that constrain multiple actors/Operations so
joint invariants remain valid.

Examples:

```text
one active execution per Workspace
one reservation transaction per capacity slot
at-most-once dispatch
lifecycle lock around Workspace changes/admission
```

Coordination need not optimize an ordering objective.

---

# 19. Coordination is not scheduling

The one-Workspace-at-a-time invariant still applies on an otherwise idle 64-core machine.

That means its purpose is not CPU scheduling efficiency.

It is a coordination/serialization invariant.

Thus:

```text
WorkspaceSerialization != ResourceSchedulerPolicy
```

---

# 20. Mutual exclusion can intentionally leave physical capacity idle

If one Workspace holds its cardinality-one execution lease, another Operation in the same
Workspace is rejected even when global physical resources remain abundant.

Therefore:

```text
IdlePhysicalCapacity != SchedulingBug
```

when a stronger coordination invariant requires exclusion.

---

# 21. Work-conserving scheduling

A scheduler is **work-conserving** for a resource when it does not leave relevant capacity
idle while eligible work that could use that capacity is waiting.

Linux fair scheduling approximates work-conserving sharing among runnable tasks; cgroup
bandwidth documentation also explicitly discusses over-subscription enabling work-
conserving semantics within a hierarchy.

But Runtime may intentionally be non-work-conserving with respect to raw hardware because
of Workspace, authority, recovery or safety constraints.

---

# 22. Work conservation is claim-relative

A system may be work-conserving for:

```text
Runtime admission slots
```

while intentionally not work-conserving for:

```text
raw CPU cores
```

because slots abstract coordination, not CPU packing.

Therefore:

```text
WorkConserving must name the resource/eligibility domain.
```

---

# 23. FIFO

A FIFO queue schedules by insertion/admission order.

Its advantages include:

```text
simple explainability
low metadata burden
stable deterministic order
```

but FIFO can produce head-of-line blocking: one long/blocked item delays later short/urgent
work.

Thus:

```text
FIFO != NeutralPolicy
```

Ordering by arrival is itself a policy.

---

# 24. Arrival time is not value or urgency

RF5 already rejects wall-clock order as semantic causality.

RF9 adds:

```text
ArrivedEarlier != MoreImportant
ArrivedEarlier != Shorter
ArrivedEarlier != EarlierDeadline
```

Therefore FIFO cannot be justified as “no scheduling policy.”

---

# 25. Shortest-job-first / shortest-remaining-time

Policies favoring shorter predicted jobs can reduce mean waiting/response time under useful
models.

But they require knowledge or prediction of service demand.

For arbitrary Runtime execution:

```text
JobDurationPrediction
```

is usually weak or unavailable.

Therefore such a scheduler would import a prediction model and its errors into Runtime
semantics.

---

# 26. Prediction error is scheduler state, not execution truth

If Runtime predicts:

```text
Job A = 2s
Job B = 20s
```

and chooses A first, that is policy evidence—not a fact about eventual execution duration.

Thus:

```text
PredictedCost != ActualCost
```

and scheduling decisions should not be rewritten as execution truth.

---

# 27. Priority scheduling

Priority provides an explicit ordering/preemption preference among eligible contenders.

But priority introduces a fundamental question:

```text
who assigns priority and according to whose value model?
```

A low-level Runtime has no generic basis for deciding that Finance > Game > maintenance or
interactive > batch without an upper policy owner.

---

# 28. Priority can cause starvation

If high-priority work keeps arriving, low-priority work may wait indefinitely.

Therefore:

```text
Priority != Fairness
```

and:

```text
PriorityScheduling can violate liveness for low classes.
```

---

# 29. Aging is a policy response, not a theorem

Aging gradually raises effective priority of long-waiting work to reduce starvation.

But this changes the value/ordering policy:

```text
waiting time becomes a source of priority
```

That may be desirable, but Runtime cannot infer it from resource facts alone.

---

# 30. Fairness

RF9 defines **Fairness** only relationally:

> **A declared rule/equity objective for distributing waiting time, service, opportunity or
> resource shares among competing subjects/classes.**

Possible meanings include:

```text
equal CPU share
weighted share
bounded waiting
per-user fairness
per-project fairness
max-min fairness
proportional fairness
```

Therefore:

```text
Fairness != OneUniversalMetric
```

---

# 31. Linux CFS/EEVDF demonstrates fairness is an explicit objective

Linux CFS historically modeled an ideal equal-share multitasking CPU using virtual runtime.
Current Linux EEVDF selects eligible entities using lag and virtual deadlines, preserving a
fair-share objective while improving latency sensitivity. citeturn227548search0turn227548search1

RF9 lesson:

> scheduling policy requires an explicit objective and accounting state; “pick next” is
> not mechanically neutral.

---

# 32. Kernel CPU scheduling and Runtime Job scheduling live at different layers

Linux scheduler chooses among runnable tasks already admitted into the OS runnable set.

A hypothetical Runtime scheduler would choose among higher-level Operations before or at
physical dispatch.

Therefore:

```text
KernelScheduler != RuntimeOperationScheduler
```

Runtime should delegate CPU task scheduling to the kernel rather than duplicate it.

---

# 33. Deadline scheduling

Deadline-aware schedulers choose service based on deadlines rather than arrival order or
fair share.

Classical real-time scheduling theory, including Liu–Layland's analysis, shows that deadline
scheduling requires assumptions about execution demand, periods/deadlines and feasibility;
it is not merely attaching a timestamp to a Job.

Thus:

```text
DeadlineField != SchedulabilityProof
```

---

# 34. EDF under overload still requires a miss policy

Even a theoretically useful Earliest Deadline First policy cannot make an infeasible
workload feasible.

If aggregate demand exceeds capacity, the system still needs decisions such as:

```text
which deadline may miss
which work to reject
which work to drop/degrade
whether to reclaim capacity
```

Therefore:

```text
SchedulingPolicy != CapacityCreation
```

---

# 35. Deadline is not timeout

RF5 timeout is an observer/control deadline on execution duration.

A scheduling deadline is typically a service-completion requirement/value constraint.

They can coincide by contract but are not synonymous.

Thus:

```text
ExecutionTimeout != SchedulingDeadline
```

---

# 36. Queue capacity is itself a resource limit

A queue consumes:

```text
memory
metadata
persistence
human/system attention
future execution obligations
```

Therefore a queue must have a bounded capacity/lifetime policy unless unlimited latent work
is deliberately accepted.

---

# 37. Unbounded queue converts overload into latency and storage pressure

Suppose arrival rate exceeds sustainable service rate.

Immediate rejection exposes overload quickly.

An unbounded queue can instead produce:

```text
increasing latency
memory/storage growth
stale work
cancellation burden
restart/recovery burden
```

without increasing service capacity.

Thus:

```text
Queue != OverloadSolution
```

---

# 38. Bounded queue moves overload boundary, it does not remove it

A bounded queue can absorb short bursts.

Once full, the system must still:

```text
block producer
reject
shed
or overwrite/drop according to policy
```

Therefore:

```text
BoundedQueue = finite buffering + later overload signal
```

not infinite capacity.

---

# 39. Backpressure

RF9 defines **Backpressure** broadly as:

> **A downstream capacity/processing constraint being communicated or mechanically
> propagated upstream so producers reduce, delay or stop creating additional outstanding
> work.**

Backpressure can appear as:

```text
flow-control credit
blocking send
bounded queue full
retryable refusal
rate signal
capacity error with holders
```

---

# 40. Backpressure is not necessarily queueing

HTTP/2 flow control protects constrained receivers by limiting how much data peers can have
outstanding, and receiver-advertised windows throttle senders without requiring an
application-level waiting-job queue. citeturn742919search1turn742919search4

Thus:

```text
Backpressure != Queue
```

---

# 41. Backpressure is also not congestion control

Transport literature distinguishes receiver flow control from network congestion control:
flow control protects endpoint receive capacity, while congestion control responds to path/
network capacity. citeturn742919search2turn742919search6

RF9 lesson:

```text
Backpressure must name which downstream constraint is being propagated.
```

---

# 42. Current `CONCURRENCY_LIMIT` is a form of explicit admission backpressure

Runtime says:

```text
I will not own another Operation now
here is the capacity scope/limit/current holders
this condition is retryable
```

This pushes the strategy decision upward.

RF9 interprets that as:

```text
ExplicitRefusalBackpressure
```

rather than failed scheduling.

---

# 43. Holder identity improves backpressure quality

Returning actual blocking holder Job/Workspace identities means upper logic can choose:

```text
wait
observe holders
cancel obsolete work
switch Workspace
reduce parallelism
retry later
```

rather than blind polling.

This is information-rich backpressure without Runtime choosing value policy.

---

# 44. Backpressure requires producer response to be effective

If every Agent immediately retries every millisecond despite `retryAfterMs`, refusal does
not reduce offered load effectively.

Therefore:

```text
BackpressureSignal != BackpressureCompliance
```

Upper producer strategy matters.

---

# 45. Retry storm is an upper-layer scheduling pathology

Many rejected callers retrying in synchronized loops can create:

```text
thundering herd
Registry pressure
unfair races
```

without Runtime owning a queue.

Mitigations such as:

```text
jitter
exponential backoff
holder observation
central upper-layer coordinator
```

belong to retry/backpressure strategy and will connect to RF10.

---

# 46. Flow control shows credit is not scheduling priority

HTTP/2/QUIC flow control grants bytes/credit to prevent resource overrun; it does not by
itself decide application semantic priority among all pending work. QUIC also highlights a
tradeoff between advertised credit, receiver resource commitment and throughput. citeturn742919search8turn742919search11

Thus:

```text
CapacityCredit != SchedulingPriority
```

---

# 47. Load shedding

RF9 defines **Load shedding** as:

> **Intentionally refusing, dropping, degrading or terminating some work under overload to
> preserve more valuable/system-critical service for remaining work.**

Unlike plain capacity rejection, load shedding requires a selection/value policy over work.

---

# 48. Capacity rejection is not load shedding by default

Current Runtime rejects whichever new request encounters the full capacity boundary.

It does not inspect:

```text
business value
priority
age
cost
importance
```

Therefore:

```text
Current CONCURRENCY_LIMIT != ValueAwareLoadShedding
```

---

# 49. Load shedding requires value ownership

To say:

```text
drop Game task, preserve Finance task
```

someone must own that cross-domain value function.

Generic Runtime does not.

That belongs more naturally to Host/Agent/domain policy unless a specific Runtime-level
class is justified.

---

# 50. Cancellation of queued work creates a new lifecycle

If Runtime had capacity-waiting queued items, cancellation would need to distinguish:

```text
cancel before admission
cancel queued-but-owned work
cancel admitted-but-not-dispatched
cancel dispatched/running
```

Current Runtime already supports admitted `Accepted` cancellation before dispatch, but it
does not own a pre-admission queued item state.

---

# 51. Queued work needs identity before execution identity is decided

A queue must answer:

```text
Does queued work already have a Job ID?
Does it already have Operation identity?
Does it hold a capacity reservation?
Can policy/environment change while it waits?
Is replay returning the same queued item or re-admitting now?
```

These are RF4/RF6/RF7 problems, not data-structure trivia.

---

# 52. Pre-admission queue and post-admission queue are different architectures

## 52.1 Pre-admission queue

```text
proposal received
→ waiting item retained
→ later admission/policy/current-world checks
→ Job created
```

The waiting item is not yet an admitted Operation.

## 52.2 Post-admission queue

```text
Job/Operation admitted
→ reservation/identity committed
→ waits for scheduled dispatch
```

The system already owns execution responsibility.

These have very different replay/revocation/capacity semantics.

---

# 53. Current release deferral is a specialized post-admission wait, not generic scheduling queue

Runtime Release is already admitted and capacity-reserved; reconciliation merely owns the
later at-most-once dispatch transition.

There is no competition-policy selection among multiple waiting release Jobs for a scarce
queue service contract.

Thus it should remain described as dispatch deferral/reconciliation, not evidence that
Runtime “already has a scheduler.”

---

# 54. Queue persistence creates recovery obligations

If a queue survives Runtime restart, it needs durable truth for:

```text
which items were waiting
which were selected
which were cancelled
whether selection occurred before crash
whether dispatch crossed the boundary
```

This immediately intersects RF10 exactly-once/recovery semantics.

A queue cannot be bolted on as an in-memory list without deciding whether lost waiting work
is acceptable.

---

# 55. Queue persistence also creates staleness semantics

While waiting:

```text
source changes
credential revoked
provider state changes
input authority changes
policy changes
user intent changes
```

A queue must specify which facts are frozen at enqueue, admission or dispatch and which
must be revalidated.

RF4/RF6/RF7 all become active.

---

# 56. Long queues create semantic obsolescence

Even if a queued request remains technically executable, it may no longer be desirable.

Examples:

```text
old market quote analysis
superseded build
obsolete repair
user changed Goal
```

Therefore queueing is not merely latency—it changes the probability that work remains
valuable when service arrives.

---

# 57. Deadline/expiry can bound semantic staleness but adds policy

A queued item might have:

```text
not-after
deadline
TTL
```

Then the queue must define:

```text
expire silently?
return failure?
reconcile?
release reservation?
```

Again, a queue becomes a durable workflow system.

---

# 58. Backpressure can preserve freshness by refusing ownership

Immediate capacity rejection lets the producer decide later whether the Operation is still
worth submitting.

This can be superior to automatically running stale work from a hidden queue.

Thus:

```text
Refusal can preserve strategic optionality.
```

---

# 59. Upper-layer strategy has richer information than Runtime

Host/Agent/domain logic may know:

```text
Goal priority
whether work is obsolete
alternative tools/routes
whether another Workspace can be used
whether to cancel holder
whether to spend more resources
```

Generic Runtime often knows only mechanical capacity and execution state.

Therefore pushing overload strategy upward can preserve the correct value owner.

---

# 60. Lowest layer should own mechanical invariants; upper layer should own value tradeoffs

RF9 derives a provisional responsibility rule:

```text
Runtime:
  capacity truth it actually owns
  admission reservation
  coordination invariants
  dispatch/recovery mechanics
  explicit backpressure evidence

Upper Host/Agent/domain:
  priority/value
  whether to wait/retry/cancel/substitute
  cross-Goal fairness
  stale-work strategy
  load shedding by importance
```

A future repeated workload may justify moving some policy downward, but not by default.

---

# 61. Scheduler placement follows truth ownership

A scheduler can only make sound decisions if it owns or reliably observes:

```text
candidate set
eligibility
capacity
cost/demand
value/priority/deadline
```

No single current Runtime component owns all of these.

Therefore a generic optimal scheduler would be epistemically overconfident.

---

# 62. Selection policy can be deterministic without being justified

A simple queue could say:

```text
sort by Job ID
```

which is deterministic.

But determinism does not imply semantic quality/fairness/value.

Thus:

```text
DeterministicScheduling != GoodScheduling
```

---

# 63. Fair scheduling can conflict with latency optimization

Linux EEVDF itself illustrates explicit tradeoffs: it preserves fairness accounting while
using virtual deadlines and request slices to improve latency-sensitive behavior. citeturn227548search0

Likewise at Runtime level:

```text
mean latency
p99 latency
fair share
throughput
deadline satisfaction
value
```

can conflict.

Therefore there is no policy-free “best scheduler.”

---

# 64. Scheduling can also conflict with cache/locality efficiency

Batching similar work may improve:

```text
cache locality
GPU model residency
container/tool startup amortization
```

while worsening fairness or deadline response.

This reinforces that scheduling objectives are workload-specific.

---

# 65. Preemption is a separate axis

A scheduler can be:

```text
non-preemptive
preemptive
cooperative
```

Preempting a Runtime Job is much harder than preempting a CPU thread because arbitrary
execution may have external Effects.

Stopping local realization does not roll back effects (RF3/RF6).

Therefore:

```text
RuntimePreemption != KernelTaskPreemption
```

---

# 66. “Pause” is not a generic safe scheduler primitive

SIGSTOP/freezing a process tree may retain:

```text
locks
memory
network leases
external transactions
provider deadlines
```

and can alter external behavior.

Thus a hypothetical Runtime scheduler cannot assume suspend/resume is semantically neutral.

---

# 67. Non-preemptive scheduling may be safer for effect-opaque work

For arbitrary `workspace.exec`, once physical execution crosses the dispatch boundary,
letting the Attempt run/cancel according to its own contract is easier to reason about than
generic scheduler preemption.

Current Runtime accordingly does not implement preemptive Job scheduling.

---

# 68. Head-of-line blocking is multidimensional

A FIFO queue can block because head work is:

```text
long-running
waiting for scarce GPU
waiting on external provider
Workspace-conflicting
not currently eligible
```

A sophisticated scheduler might skip blocked work, but then queue order and eligibility
become dynamic policy state.

---

# 69. Queue length is not backlog value

Ten tiny Jobs may be less backlog than one 6-hour Job.

Likewise one critical Job may matter more than 100 background Jobs.

Therefore:

```text
QueueLength != WorkAmount != ValueBacklog
```

This mirrors RF8's `one slot != equal physical demand`.

---

# 70. Little-style queue metrics require a stable queueing system model

Queueing-theoretic latency/throughput relationships can be useful only when arrival rate,
service rate, stability and class assumptions are meaningful.

Arbitrary Agent-driven work can be bursty/nonstationary and strategically cancelled or
superseded.

RF9 therefore does not infer scheduler design from queue length alone.

---

# 71. Overload

RF9 defines **Overload** operationally:

> **Offered eligible work exceeds the system's sustainable service/admission capability
> over a relevant interval such that backlog, rejection, latency, pressure or failure must
> grow without adaptation.**

Overload is temporal; a short burst may be absorbed by bounded buffering while persistent
overload cannot.

---

# 72. Burst absorption and sustainable throughput are different

A queue of 100 can absorb 100 burst items.

If arrival remains 20/s and service is 10/s, the queue only postpones overload exposure.

Therefore:

```text
BurstCapacity != SustainableServiceCapacity
```

---

# 73. Load shedding is one overload response among several

Possible responses include:

```text
backpressure producer
reject new work
queue bounded burst
shed low-value work
scale capacity
reduce work quality
route elsewhere
cancel obsolete work
```

Which one is correct depends on ownership/value/cost.

---

# 74. Current Runtime's response is deliberately narrow

For admission capacity:

```text
full
→ reject new commitment
→ expose holders/scope/retry hint
```

It does not:

```text
kill an existing Job
reorder Goals
queue silently
raise machine resource limits
choose alternative provider
```

This is a strong narrow contract.

---

# 75. `DEPLOYMENT_IN_PROGRESS` is another backpressure/refusal boundary

Current new-admission fence can reject new work while Runtime deployment owns a critical
transition.

Exact replay still succeeds across that fence.

RF9 interpretation:

```text
coordination-state refusal/backpressure
```

not resource scheduler queueing.

---

# 76. `WORKSPACE_BUSY` likewise expresses coordination backpressure

Workspace lifecycle operations can fail because active/held work or Git dependency
relationships make closure unsafe.

That is not CPU capacity exhaustion.

This reinforces:

```text
Backpressure can originate from coordination invariants, not only resources.
```

---

# 77. Different refusal causes should remain distinguishable

Examples:

```text
CONCURRENCY_LIMIT        capacity
DEPLOYMENT_IN_PROGRESS   deployment coordination fence
WORKSPACE_BUSY           workspace lifecycle coordination
REGISTRY_BUSY            storage/transaction contention
```

Flattening all into “queue later” would destroy decision information.

---

# 78. Backpressure should carry reason/owner/scope when possible

A good refusal should let the upper layer answer:

```text
what blocked?
who owns the blocker?
is condition transient?
what evidence identifies holders?
what future action can change it?
```

Current capacity errors already move in this direction.

---

# 79. Flow-control credit demonstrates bounded outstanding work

HTTP/2 and QUIC flow control bound how much data can remain outstanding based on receiver
credit; QUIC explicitly discusses balancing credit size against receiver resource
commitment and throughput. citeturn742919search4turn742919search11

RF9 generalization:

> good backpressure bounds **outstanding responsibility**, not merely instantaneous CPU
> utilization.

This aligns with Runtime's refusal to own unbounded waiting Operations.

---

# 80. Outstanding responsibility is itself a system resource

Every accepted waiting Job creates future obligations:

```text
persist it
explain it
cancel it
revalidate it
recover it
possibly execute it
```

Thus queue length consumes governance/recovery capacity even before CPU dispatch.

This is one reason queue capacity cannot be treated as free.

---

# 81. Current pre-commit refusal preserves a clean ownership boundary

On `CONCURRENCY_LIMIT`:

```text
Runtime has not accepted a new Job
caller still owns whether/when to try again
```

This makes responsibility explicit.

A queue would move ownership into Runtime and therefore needs a much stronger contract.

---

# 82. Scheduler queue can become workflow orchestration

Once Runtime owns:

```text
waiting work
priority
retry/revalidation
expiry
dependencies
cancellation
restart
```

it begins to overlap with durable workflow/Host concerns.

RF9 therefore treats queue introduction as an architectural boundary change, not UX
polish.

---

# 83. Current accepted-state reconciliation is not a hidden scheduler

`reconcile_all` may encounter an `Accepted` Attempt and call the existing dispatch path.

But there is no policy deciding among multiple capacity-waiting contenders because each
Accepted Attempt already owns its reservation.

This is recovery/dispatch convergence, not resource scheduling.

---

# 84. Dispatch at-most-once boundary remains independent of scheduling policy

If a future scheduler chooses Job J, Runtime must still:

```text
commit dispatch intent
cross physical provider boundary at most once
recover ambiguous outcomes
```

Therefore RF3/RF10 dispatch correctness is downstream from RF9 selection policy.

---

# 85. Scheduling policy should not rewrite historical admission truth

A Job admitted under policy P remains historically admitted even if current scheduling
policy changes.

Future selection can use current policy only where the contract permits.

Thus:

```text
HistoricalAdmission != CurrentSchedulingPreference
```

---

# 86. A queue also needs fairness across identities/scopes

If one client can enqueue unlimited work first, FIFO can monopolize future service.

Possible remedies:

```text
per-principal limits
per-Goal queues
weighted fair scheduling
round robin
admission quotas
```

but each requires a principal/value/accounting policy from RF6/RF8.

---

# 87. Queue admission itself requires admission control

A queue does not eliminate admission control; it creates two admissions:

```text
accept into queue
accept into execution capacity
```

Each needs capacity and ownership semantics.

Therefore:

```text
QueueingSystem still needs admission control.
```

---

# 88. Backpressure can be hop-by-hop

HTTP/2 explicitly defines flow control as hop-by-hop; an intermediary's throttling can
indirectly propagate upstream rather than representing one global end-to-end capacity
truth. citeturn742919search9

RF9 lesson:

```text
BackpressureSignalOwner != GlobalWorldCapacityOwner
```

Runtime's capacity signal is only about Runtime-owned admission scope.

---

# 89. Upper Host can aggregate multiple backpressure domains

An upper coordinator may receive:

```text
Runtime CONCURRENCY_LIMIT
provider 429
network flow control
finance venue throttling
human approval delay
```

and decide strategy across them.

This is another reason generic Runtime should not prematurely own cross-domain scheduling.

---

# 90. RF9 queue-introduction gate

A Runtime-owned waiting queue should require evidence that all of these are answered:

```text
concrete repeated workload
why upstream retry/refusal is insufficient
queue location: pre- or post-admission
waiting-item identity
capacity and persistence
ordering/selection objective
fairness/starvation behavior
cancellation/expiry
policy/env revalidation
restart/recovery semantics
dispatch at-most-once integration
overload/backpressure when queue fills
```

Without these, `add a queue` is under-specified.

---

# 91. Current queue-less model has real benefits

```text
no hidden latency backlog
no stale work accumulation
no durable pre-admission item state
no generic priority/value policy
no scheduler recovery surface
clear ownership on rejection
simple capacity invariant
upper layer retains strategy optionality
```

Its cost is that callers must handle transient refusal explicitly.

---

# 92. Queue-less does not mean caller inconvenience is irrelevant

Agent-first surfaces should make rejection easy to act on by exposing:

```text
scope
holders
retryability
retry hint
blocking Workspace identities
```

The right improvement may be better information/upper coordination rather than lower-level
queue ownership.

---

# 93. FoundationReopenCondition for scheduling

RF9 would reopen current queue-less design if repeated measured workloads show that:

```text
capacity bursts are common and short-lived
upper-layer retry creates measurable herd/friction
work remains valid while waiting
one stable scheduling objective exists
waiting-item ownership can be made explicit
queue durability/cancellation/recovery costs are justified
```

or if a concrete product requires hard deadlines/fairness guarantees Runtime itself must
own.

---

# 94. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| full Runtime slot → immediate error | capacity shortage = queue | admission may refuse ownership |
| retry 1s later | retry = queue | producer resubmits; Runtime retained nothing |
| release Job Accepted then later dispatch | not-running = queued | admitted dispatch deferral differs |
| one Workspace serializes on idle 64-core host | idle resource = scheduler bug | coordination invariant can dominate utilization |
| FIFO long job blocks urgent short job | FIFO = neutral | arrival order is policy |
| SJF requires unknown runtime estimate | scheduler can know cost | prediction dependency appears |
| high-priority stream never stops | priority = liveness | starvation possible |
| aging rescues old work | fairness emerges automatically | waiting time becomes explicit policy |
| EEVDF fair-share/latency tradeoff | scheduler = mechanism only | scheduler encodes objective/accounting |
| EDF infeasible workload | scheduler = capacity creation | overload still needs reject/miss policy |
| bounded queue fills | queue = overload solution | backpressure boundary only moved |
| unbounded queue under λ>μ | queue = safety | latency/storage diverge |
| HTTP/2 flow-control window | backpressure = queue | credit can throttle sender without job queue |
| flow control vs congestion control | backpressure has one meaning | endpoint vs network constraints differ |
| capacity error with holders | rejection = opaque failure | refusal can carry actionable evidence |
| queued item revoked before run | enqueue = eternal authorization | waiting needs revalidation semantics |
| restart with queued work | queue = transient list | durability/recovery contract required |
| cancel accepted-before-dispatch | cancellation = running only | admitted work can cancel pre-dispatch |
| pre-admission queued cancel | current Job lifecycle covers it | would require new waiting-item lifecycle |
| pause arbitrary process with external API lease | preemption = neutral | scheduler action can alter external semantics |

---

# 95. RF9 anti-laws

RF9-L1: `Eligible != Admitted`.

RF9-L2: `Admission != Dispatch`.

RF9-L3: `AdmissionOrder != PhysicalStartOrder`.

RF9-L4: `NotYetRunning != Queued`.

RF9-L5: `AdmittedDispatchDeferral != CapacityWaitingQueue`.

RF9-L6: `CONCURRENCY_LIMIT != QueueReceipt`.

RF9-L7: `RetryAfter != Reservation`.

RF9-L8: `Retry != Queue`.

RF9-L9: `AdmissionControl != Scheduling`.

RF9-L10: `SchedulingDecision != DispatchEffect`.

RF9-L11: `Coordination != Scheduling`.

RF9-L12: `WorkspaceSerialization != ResourceSchedulerPolicy`.

RF9-L13: `IdlePhysicalCapacity != SchedulingBug by itself`.

RF9-L14: `FIFO != NeutralPolicy`.

RF9-L15: `ArrivedEarlier != MoreImportant/Urgent/Shorter`.

RF9-L16: `PredictedCost != ActualCost`.

RF9-L17: `Priority != Fairness`.

RF9-L18: `Fairness != OneUniversalMetric`.

RF9-L19: `KernelScheduler != RuntimeOperationScheduler`.

RF9-L20: `DeadlineField != SchedulabilityProof`.

RF9-L21: `SchedulingPolicy != CapacityCreation`.

RF9-L22: `ExecutionTimeout != SchedulingDeadline`.

RF9-L23: `Queue != OverloadSolution`.

RF9-L24: `Backpressure != Queue`.

RF9-L25: `Backpressure != CongestionControl`.

RF9-L26: `BackpressureSignal != BackpressureCompliance`.

RF9-L27: `CapacityCredit != SchedulingPriority`.

RF9-L28: `CapacityRejection != ValueAwareLoadShedding`.

RF9-L29: `RuntimePreemption != KernelTaskPreemption`.

RF9-L30: `QueueLength != WorkAmount != ValueBacklog`.

RF9-L31: `BurstCapacity != SustainableServiceCapacity`.

RF9-L32: `HistoricalAdmission != CurrentSchedulingPreference`.

RF9-L33: `Queueing != EliminationOfAdmissionControl`.

RF9-L34: `BackpressureSignalOwner != GlobalCapacityOwner`.

RF9-L35: `DeterministicScheduling != GoodScheduling`.

---

# 96. Minimum RF9 grammar

## 96.1 Eligibility `Elig(W,t)`

Work W satisfies current prerequisites to contend for a next transition.

## 96.2 Admission `Admit(W)`

System accepts ownership and commits identity/capacity obligations.

## 96.3 Queue `Q`

Owned waiting-work state with insertion, capacity, ordering, lifetime, cancellation and
recovery semantics.

## 96.4 Scheduling `Sched(Candidates,Resources,Policy)`

Select service/dispatch ordering among eligible owned contenders.

## 96.5 Dispatch `Dispatch(W)`

Cross physical execution-provider boundary for an admitted work item.

## 96.6 Coordination `Coord(I)`

Mechanisms preserving joint invariant I across concurrent actors/work.

## 96.7 Backpressure `BP(downstream→upstream,constraint)`

Propagation/communication of downstream capacity constraints to reduce offered work.

## 96.8 Load Shedding `Shed(W,Policy)`

Reject/drop/degrade selected work under overload according to value/survival policy.

## 96.9 Overload

Offered demand persistently exceeds sustainable service/admission capacity over an interval.

## 96.10 Work Conservation

Do not leave named resource capacity idle while eligible compatible waiting work exists,
subject to declared higher invariants.

---

# 97. Current Runtime scheduling/coordination map

| Current mechanism | RF9 interpretation |
|---|---|
| request validation | eligibility/admission precondition |
| Registry capacity transaction | admission arbitration |
| `concurrency_reservation` | durable admission ownership/capacity commitment |
| `CONCURRENCY_LIMIT` | pre-commit admission refusal + explicit backpressure |
| `retryAfterMs` | retry hint, not reservation/schedule |
| holder IDs | blocking-capacity evidence for upper strategy |
| one Workspace active | coordination/serialization invariant |
| lifecycle/admission fence | short coordination critical section |
| ordinary new Job dispatch | admit then immediate dispatch attempt |
| Runtime Release Accepted wait | specialized admitted dispatch deferral |
| `dispatch_issued` | durable at-most-once dispatch intent |
| systemd/Windows launcher | physical dispatcher/provider substrate |
| reconciliation of Accepted | dispatch convergence/recovery, not capacity scheduler |
| cancellation of Accepted | admitted-before-dispatch cancellation |
| kernel EEVDF/CFS | lower-level CPU task scheduler outside Runtime ownership |
| no waiting-item table | no Runtime-owned capacity queue |
| no priority/fairness fields | no Runtime scheduling value policy |

---

# 98. What RF9 strongly supports in current design

## 98.1 Pre-commit capacity refusal

Runtime does not create hidden durable waiting obligations when it cannot honor admission
capacity.

## 98.2 Actionable backpressure

Capacity errors expose holder identities and scope rather than only `busy`.

## 98.3 Admission/dispatch separation

Capacity ownership is committed before dispatch; physical at-most-once dispatch remains a
separate recoverable boundary.

## 98.4 Coordination stays distinct from optimization

One Workspace at a time is preserved even when physical hardware is idle because it guards
Workspace semantics rather than CPU efficiency.

## 98.5 Kernel owns CPU scheduling

Runtime does not duplicate OS runnable-task scheduling/fairness machinery.

## 98.6 Upper layer owns strategic waiting/value policy

Current refusal preserves Agent/Host ability to choose wait, retry, cancel, substitute or
reframe work using broader Goal information.

---

# 99. What RF9 reopens

## 99.1 Static retry hint can create herd behavior

`retryAfterMs=1000` is simple but synchronized callers can retry together. Future measured
friction might justify jitter/adaptive hints or a higher-level coordinator before a Runtime
queue.

## 99.2 Capacity-holder evidence is mechanical, not strategy

Upper agents still need a reusable policy for observing holders and deciding when to retry.
This may belong in Harness/Host rather than Core.

## 99.3 Release dispatch deferral should remain explicitly named

It should not evolve accidentally into a generic Accepted backlog with implicit scheduling.
If multiple deferred structured effects require ordering, that must become an explicit
contract.

## 99.4 A future scheduler requires workload evidence

Potential triggers include measured burst friction, hard deadline ownership, persistent
multi-tenant fairness requirements, or resource-aware heterogeneous packing that cannot be
handled upstream.

---

# 100. Research constitution added by RF9

1. Keep Eligibility, Admission, Queueing, Scheduling and Dispatch separate.
2. Do not call every not-yet-running Job queued; distinguish admitted dispatch deferral.
3. A queue is an ownership/lifecycle contract, not a data structure.
4. Capacity rejection before Job creation means Runtime did not accept future execution
   responsibility.
5. Retry hints are not capacity reservations or future-start guarantees.
6. Keep upper-layer retry separate from lower-layer queueing.
7. Name the scheduling objective; FIFO/priority/fairness/deadline are all policies.
8. Do not import duration/cost predictions as execution truth.
9. Keep coordination invariants distinct from throughput/utilization optimization.
10. Do not treat idle hardware as proof a coordination capacity limit is wrong.
11. Keep kernel CPU scheduling and Runtime Operation scheduling at their proper ownership
    layers.
12. Backpressure can be explicit refusal/credit/blocking and does not require a queue.
13. Distinguish endpoint/Runtime backpressure from network congestion control and from
    value-aware load shedding.
14. Bound waiting responsibility; unbounded queues hide rather than solve persistent
    overload.
15. Queue introduction requires identity, capacity, persistence, cancellation, expiry,
    revalidation, fairness and recovery semantics.
16. Generic Runtime should not assign cross-domain value priority without an explicit
    policy owner.
17. Preempting effect-opaque work is not semantically equivalent to kernel thread
    preemption.
18. Preserve actionable refusal evidence so higher layers can choose strategy with richer
    Goal/domain information.
19. Do not enter RF10 while retry, queueing and re-dispatch remain conceptually collapsed.

---

# 101. RF9 result — conceptual compression

```text
Eligibility says work can currently contend.
Admission says Runtime accepts ownership and commits obligations.
Queueing says accepted-to-wait work is durably owned under ordering/lifetime rules.
Scheduling chooses which eligible owned contender gets service next.
Dispatch crosses the physical provider boundary.
Coordination preserves invariants independent of optimization.
Backpressure pushes downstream constraints upstream so producers reduce outstanding work.
Load shedding deliberately sacrifices selected work according to value/survival policy.
```

The strongest Runtime-specific compression is:

```text
current Runtime:
  durable admission arbitration
  durable capacity reservation
  immediate refusal when capacity unavailable
  actionable backpressure evidence
  ordinary admit→immediate-dispatch
  specialized admitted dispatch deferral for release/reconciliation
  at-most-once physical dispatch

current Runtime does NOT have:
  a capacity-waiting queue
  waiting-work selection policy
  priority/fairness/deadline scheduler
  generic preemption
  value-aware load shedding
```

---

# 102. RF9 closeout

RF9 closes with twenty-two durable results:

1. **Eligibility, Admission, Queueing, Scheduling and Dispatch are distinct transitions/
   responsibilities.**
2. Admission commits ownership/identity/capacity obligations; dispatch later crosses the
   physical execution-provider boundary.
3. A queue is a durable/stateful waiting-work ownership contract, not merely “things not
   running yet.”
4. **Current Runtime has no capacity-waiting Job queue.** Capacity exhaustion rejects new
   work before a Job/Attempt/reservation is created.
5. `CONCURRENCY_LIMIT` is explicit pre-commit admission refusal/backpressure with scoped
   capacity/holder evidence, not a queue receipt.
6. `retryAfterMs` is a hint rather than a reservation, priority guarantee or scheduled
   wakeup.
7. Upper-layer retry/re-admission is not Runtime queueing.
8. Current Runtime does permit specialized **admitted dispatch deferral**: Runtime Release
   commits an Accepted reserved Job and lets reconciliation perform later at-most-once
   dispatch. This must not be confused with a capacity-waiting queue.
9. Ordinary new Jobs are admitted/reserved then immediately sent toward dispatch after the
   lifecycle critical section is released.
10. Scheduling chooses among eligible owned contenders; admission control and physical
    dispatch are separate mechanisms.
11. One-Workspace-at-a-time is a coordination/serialization invariant rather than a
    hardware scheduler policy, so idle hardware does not invalidate it.
12. FIFO is an ordering policy, not neutral behavior; arrival order does not imply urgency,
    value or duration.
13. SJF/SRT-like policies depend on service-time prediction, which generic Runtime does not
    possess as execution truth.
14. Priority can starve; aging/fairness add explicit value/equity objectives rather than
    emerging mechanically.
15. Linux CFS/EEVDF shows scheduler fairness/latency are explicit objectives backed by
    scheduler accounting state, and kernel CPU scheduling remains outside Runtime's
    Operation layer. citeturn227548search0turn227548search1
16. Deadline scheduling requires demand/feasibility assumptions; a deadline field cannot
    create capacity or prove schedulability.
17. Unbounded queues convert persistent overload into growing latency/storage/staleness;
    bounded queues merely move the overload/backpressure boundary.
18. **Backpressure does not require queueing.** Receiver credit/flow-control systems show
    resource constraints can directly throttle upstream producers. citeturn742919search1turn742919search8
19. Capacity rejection differs from value-aware load shedding; generic Runtime lacks the
    cross-domain value owner needed for shedding priorities.
20. A Runtime-owned queue would introduce new durable semantics for waiting-item identity,
    persistence, cancellation, expiry, policy/environment revalidation, restart recovery,
    fairness and at-most-once dispatch integration.
21. Current queue-less refusal model preserves explicit ownership: rejected work remains an
    upper-layer strategic choice rather than hidden Runtime future obligation.
22. **A scheduler/queue remains unjustified until a concrete repeated workload proves that
    explicit refusal + upper-layer strategy is materially inadequate and supplies a stable
    scheduling objective.**

The next frontier is:

> **When realization or communication fails ambiguously, what does it mean to retry,
> recover, resume, reconcile or compensate? Can an Operation be exactly-once, or only its
> admission/dispatch/effect subcontracts? What must survive crash and what may be repeated?**

That is RF10 — Failure, Recovery, Retry, Idempotency and Exactly-Once.
