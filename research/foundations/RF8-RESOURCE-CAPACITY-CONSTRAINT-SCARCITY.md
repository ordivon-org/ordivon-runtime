---
schema_version: 1
id: runtime.foundations.rf8
title: RF8 — Resource, Capacity, Constraint and Scarcity
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
summary: RF8 separates resource existence, capacity, availability, allocation, reservation, entitlement, quota, budget, limit, contention, scarcity and pressure. It classifies current Runtime concurrency reservations as durable admission-capacity claims rather than machine capacity; ExecutionBudget as per-Attempt consumption ceilings rather than reservations; time/output limits as mechanical bounds rather than resource inventory; and establishes that Runtime currently constrains selected resource dimensions without being a scheduler or complete resource manager.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF8
related:
  - runtime.foundations.start
  - runtime.foundations.rf7
  - runtime.foundations.rf8.sources
  - runtime.foundations.rf8.continuation
---
# RF8 — Resource, Capacity, Constraint and Scarcity

## 0. Status and research question

RF7 separated environmental dependencies from boundaries. RF8 now asks:

> **Which things enable realization as resources? How much of them exists, how much is
> available, who is entitled to use them, what has actually been allocated or reserved,
> what merely limits use, and when do contention/scarcity/pressure become operationally
> relevant?**

RF8 is explanatory research only. It does not authorize adding a queue, scheduler,
priority/fairness framework, resource broker, PSI controller, GPU allocator, disk quota,
rate-limit manager or new cgroup controls.

---

# 1. First deletion — Resource is not synonymous with “a numeric limit”

Current Runtime exposes quantities such as:

```text
MemoryMax
TasksMax
CPUQuota
timeoutMs
stdoutLimitBytes
stderrLimitBytes
globalLimit
```

It would be easy to call all of these “resources.”

That collapses distinct things:

```text
physical substrate
capacity
admission slots
consumption ceilings
time horizons
retention/output bounds
```

Therefore:

```text
Resource != Limit
```

---

# 2. Resource

RF8 uses **Resource** relationally:

> **A Resource is an entity, service, right or measurable substrate whose availability or
> use can enable, sustain, constrain or exclude realizations/actions for a consumer under
> some contract.**

Examples include:

```text
CPU execution time
physical/virtual memory
disk/storage space
I/O service
network bandwidth
GPU/accelerator time and memory
process/task slots
file/device handles
exclusive locks
Runtime concurrency slots
API rate-limit tokens
provider account quota
wall-clock execution opportunity
```

A resource need not be a consumable stock.

---

# 3. Resource existence versus capacity

RF7 already distinguished:

```text
GPU exists and is compatible
```

from:

```text
how much VRAM/compute is available
```

RF8 formalizes:

```text
Existence(R) != Capacity(R)
```

A resource can exist but expose zero currently usable capacity.

---

# 4. Capacity

**Capacity** is the maximum service/quantity/concurrency that a resource or allocation
domain can support under a specified model and interval.

Examples:

```text
64 GiB physical RAM
1 TiB filesystem capacity
8 concurrent Runtime admission slots
100 API requests/minute
one exclusive lock holder
GPU memory pool size
```

Capacity must name:

```text
resource
scope/domain
time interval/measurement model
units
```

---

# 5. Capacity can be physical or contractual

`64 GiB RAM` is largely physical capacity.

`8 active Runtime Jobs` is a Runtime-defined contractual admission capacity.

`100 requests/minute` is provider-defined service capacity/entitlement.

Thus:

```text
Capacity != PhysicalCapacityOnly
```

---

# 6. Availability

**Availability** is the currently usable portion/opportunity of capacity for a consumer
under relevant allocations, reservations, policy and state.

Conceptually:

```text
Available(R,t,consumer)
```

is not necessarily:

```text
Capacity - CurrentUsage
```

because:

```text
reserved-but-idle capacity
priority/protection
fragmentation
policy restrictions
external commitments
hardware topology
```

can matter.

---

# 7. Availability is observer/time relative

A resource can be:

```text
available when admission is checked
unavailable at actual allocation/use time
```

or vice versa.

Therefore:

```text
AvailabilityObservation != GuaranteedFutureCapacity
```

This is another RF5 temporal consequence.

---

# 8. Allocation

**Allocation** is assignment of actual resource quantity/service/ownership to a consumer,
usually making it usable or accounted to that consumer.

Examples:

```text
malloc returns memory
GPU allocator returns device memory
kernel schedules CPU time
filesystem allocates blocks
provider accepts one rate-limit token/request
```

Allocation is usually closer to physical/service use than admission permission.

---

# 9. Reservation

**Reservation** is a commitment that capacity/entitlement will be treated as occupied or
held for some consumer or future use, whether or not the consumer is currently consuming
the underlying resource physically.

Thus:

```text
Reservation != Usage
```

A reservation can be idle.

---

# 10. Reservation versus allocation

Examples:

```text
hotel seat reserved but person not seated
Runtime slot reserved before physical dispatch
memory address range reserved but pages not committed
GPU pool capacity retained but not actively used by kernel
```

Hence:

```text
Reservation != Allocation
```

although some systems combine them.

---

# 11. Entitlement

**Entitlement** is recognized right/claim to access some resource quantity/service under a
policy/contract.

Examples:

```text
API plan allows 1M requests/month
user quota allows 100 GiB storage
cgroup protection/weight expresses service entitlement/protection
```

Entitlement can exist without current free capacity.

Thus:

```text
Entitlement != Availability
```

---

# 12. Quota

RF8 uses **Quota** as a policy-accounting allowance limiting how much a subject/group may
consume over a scope or interval.

A quota usually says:

```text
consumer may not exceed Q
```

It does not necessarily reserve Q physically.

Therefore:

```text
Quota != Reservation
```

---

# 13. Limit

A **Limit** is an enforced or declared upper bound on a measured quantity/rate/count.

Examples:

```text
MemoryMax
pids.max / TasksMax
cpu.max / CPUQuota
output retention bytes
max runtime
```

A limit constrains a consumer but does not prove that the bounded quantity is actually
available.

Thus:

```text
Limit != GuaranteedCapacity
```

---

# 14. Linux cgroup v2 directly proves limit overcommit

Linux cgroup v2 explicitly permits limits to be overcommitted: the sum of child limits can
exceed resources available to the parent.

Therefore if two groups each have:

```text
MemoryMax = 8 GiB
```

that does not prove 16 GiB has been reserved.

Hence:

```text
ConfiguredLimit != ReservedCapacity
```

This is a decisive RF8 falsifier.

---

# 15. Protection/guarantee is another resource relation

cgroup v2 separately models **protections** from limits. Protections may be hard or soft
and can themselves be overcommitted, in which case only available parent capacity can be
protected among children.

This proves resource control has distinct axes:

```text
maximum allowed consumption
minimum/protected service
relative distribution preference
```

They are not one scalar budget.

---

# 16. Budget

RF8 uses **Budget** as an operation/consumer-scoped allowance/envelope governing maximum
resource expenditure or physical consumption for a realization.

Budget can include multiple dimensions:

```text
memory
CPU rate
tasks/process count
time
money
tokens
output bytes
```

Budget is a planning/constraint concept, not automatically a resource reservation.

---

# 17. Current `ExecutionBudget` is specifically a consumption ceiling envelope

Current Runtime defines:

```text
memoryMaxBytes
tasksMax
cpuQuotaPercent
```

and applies them through Linux systemd/cgroup controls or target-native Windows Job
controls.

RF8 classifies this as:

> **per-Attempt committed physical-consumption ceilings**.

Not:

```text
pre-reserved RAM/CPU/process capacity
```

---

# 18. `MemoryMax` is not a promise of memory

`MemoryMax=4GiB` means the cgroup must not successfully sustain consumption beyond the
configured memory-control boundary under kernel semantics.

It does not mean:

```text
Runtime reserved 4 GiB of physical RAM before dispatch
```

and does not prove the Job can ever allocate 4 GiB under machine pressure.

Thus:

```text
MemoryMax != MemoryReservation
```

---

# 19. `TasksMax` is a count ceiling, not process capacity allocation

`TasksMax=N` constrains how many tasks/processes the unit may have according to systemd/
cgroup PID accounting.

It is not:

```text
reservation of N schedulable CPUs or process structures
```

Thus:

```text
TaskCountLimit != ComputeCapacityReservation
```

---

# 20. `CPUQuota` is bandwidth/rate ceiling, not CPU ownership

Linux cgroup `cpu.max` is a maximum CPU bandwidth amount over a period. Runtime's
`CPUQuota=` maps to such target-specific CPU-rate control.

Therefore:

```text
CPUQuota != ExclusiveCPUReservation
```

A Job with `100%` quota can still contend with others and can receive less service because
CPU capacity is shared and scheduled.

---

# 21. Windows budget controls are semantically aligned dimensions, not identical mechanisms

Current Windows launcher applies:

```text
JobMemoryLimit
ActiveProcessLimit
CPU rate control
```

and explicitly refuses to claim byte-for-byte semantic equivalence with Linux
`MemoryMax/TasksMax`.

RF8 strongly supports this distinction:

```text
ResourceDimensionContract != IdenticalPlatformAccounting
```

---

# 22. Time limit is not stored resource capacity

A timeout constrains how long a realization may continue under RF5 clock semantics.

It does not correspond to a stock of `milliseconds` physically stored by Runtime.

Thus:

```text
TimeBudget != CapacityInventory
```

Time is still economically/resource-like because opportunities expire and one realization
can occupy scarce slots while time passes, but its mechanics differ from memory/disk.

---

# 23. Output limit is a retention/observation bound, not total output resource use

Current `stdoutLimitBytes/stderrLimitBytes` bound retained/captured output.

A process may emit more bytes than Runtime retains under truncation semantics.

Therefore:

```text
RetainedOutputLimit != TotalPhysicalIOUsage
```

This is another example of a mechanical bound that should not be mislabeled machine
capacity.

---

# 24. Current `global_limit` is admission capacity, not machine capacity

Current admission counts durable reservations in states:

```text
active
held_orphaned
```

and rejects a new Job if count reaches `global_limit`.

This does not inspect:

```text
free RAM
CPU idle percentage
load average
GPU VRAM
network bandwidth
```

Therefore:

```text
global_limit = Runtime admission concurrency capacity
```

not:

```text
global machine physical capacity
```

---

# 25. Per-Workspace limit is another logical/contractual capacity

Current Runtime allows at most one active/held execution per Workspace.

This protects Workspace operation/source/execution semantics rather than expressing how
many physical CPU cores the Workspace could consume.

Thus:

```text
WorkspaceConcurrencyCapacity != HardwareCapacity
```

---

# 26. Current concurrency reservation is a real reservation

Admission transaction:

```text
checks holder count
→ creates Job/Attempt
→ inserts concurrency_reservation state=active
→ commits
→ physical dispatch happens later
```

Therefore the slot is held before the underlying execution necessarily begins.

This satisfies the core RF8 definition of reservation.

---

# 27. Reservation ownership survives process/daemon reconstruction

Reservations are stored durably in SQLite rather than an in-memory semaphore.

A Runtime restart therefore reconstructs holders from Registry truth.

This proves the reservation is a durable Operation/Attempt coordination commitment, not
merely a transient scheduling counter.

---

# 28. Orphaned work holding capacity is intentional conservative reservation

Current `held_orphaned` reservation state preserves slot occupation when Runtime cannot yet
prove it is safe to release capacity.

This means:

```text
KnownPhysicalUsage may be uncertain
while
ReservedAdmissionCapacity remains held
```

Therefore reservation tracks **safety commitment**, not measured utilization.

---

# 29. Reservation release is a semantic/state transition, not resource sampling

Current reservation is released atomically with conclusive terminal/reconciliation
transitions.

It is not released because:

```text
CPU usage reached zero for 500ms
```

This confirms again:

```text
AdmissionSlotReservation != PhysicalUsageMeasurement
```

---

# 30. Capacity admission remains immediate, not queued

Current Runtime does not turn capacity exhaustion into a hidden waiting queue.

A request exceeding current admission capacity receives `CONCURRENCY_LIMIT` and may retry
later.

RF8 interpretation:

```text
capacity check + reservation
```

exists, but:

```text
queue + scheduling policy
```

does not.

---

# 31. Scarcity

RF8 defines **Scarcity** relationally:

> **Resource demand/claims can exceed or conflict with available service/capacity over the
> relevant scope/time, requiring exclusion, delay, degradation, prioritization or tradeoff.**

Scarcity is not synonymous with small absolute quantity.

A huge resource can still be scarce relative to demand.

---

# 32. Scarcity can be physical, contractual or policy-created

Examples:

```text
physical RAM exhausted
one exclusive lock
Runtime global limit=8 despite idle machine
provider API rate limit
per-user quota
```

All create scarcity at different layers.

Thus:

```text
Scarcity != PhysicalExhaustionOnly
```

---

# 33. Contention

**Contention** exists when multiple consumers concurrently compete for the same resource
or mutually constraining service.

Examples:

```text
CPU runnable tasks
memory reclaim
I/O queue
network bandwidth
one file lock
one Runtime slot
GPU memory allocator
```

Contention may exist well before hard exhaustion.

---

# 34. Contention is not dependency

RF7 dependency says a resource/service is needed for a claim.

RF8 contention says multiple consumers interfere/compete over it.

Thus:

```text
Dependency != Contention
```

A Job can depend on CPU while facing no meaningful CPU contention.

---

# 35. Pressure

RF8 uses **Pressure** for measured degradation/stall/reclaim/interference caused by resource
contention/scarcity, rather than raw utilization alone.

Linux PSI is an important concrete model: it tracks time workloads are stalled due to CPU,
memory and I/O contention and distinguishes partial versus full stalls.

Thus:

```text
Pressure != Utilization
```

---

# 36. High utilization does not necessarily mean harmful pressure

A CPU at near 100% can be productively executing one batch workload with little waiting.

Conversely moderate utilization can coexist with severe I/O or memory stalls.

Therefore:

```text
UsageRatio != WorkloadPressure
```

This is why utilization-only scheduling signals can mislead.

---

# 37. Pressure can be dynamic evidence for future control, not resource truth itself

PSI-style stall evidence can support decisions such as:

```text
load shedding
migration
pausing low-priority work
capacity planning
```

But pressure observation does not itself allocate capacity or establish entitlement.

Thus:

```text
PressureMetric != ResourceManager
```

---

# 38. Resource classes differ fundamentally

RF8 rejects one universal scalar resource model.

## 38.1 Shareable renewable service

```text
CPU time
network bandwidth
I/O throughput
```

capacity renews over time and is schedulable.

## 38.2 Stateful allocated stock

```text
memory pages
VRAM
disk blocks
```

capacity remains occupied until released/reclaimed.

## 38.3 Discrete cardinality

```text
process/task count
file descriptors
connection slots
Runtime concurrency slots
```

## 38.4 Exclusive/non-divisible resource

```text
mutex/file lock
exclusive device
single-writer lease
```

## 38.5 Rate/token resource

```text
API requests/minute
provider credits
message quota
```

## 38.6 Opportunity/deadline resource

```text
wall-clock window
lease lifetime
execution deadline
```

These have different allocation/recovery semantics.

---

# 39. CPU is renewable service, memory is stateful occupancy

CPU service consumed in one interval does not remain permanently occupied; new CPU time
becomes available in future intervals.

Memory allocation persists until freed/reclaimed.

Therefore:

```text
CPUCapacitySemantics != MemoryCapacitySemantics
```

This alone defeats a generic “resourceAmount” abstraction as a full model.

---

# 40. Disk is both stock and service

Storage has at least:

```text
capacity bytes/inodes
I/O bandwidth/IOPS/latency
```

A filesystem can have plenty of free space but saturated I/O, or fast I/O with no free
space.

Thus:

```text
StorageCapacity != StorageServiceCapacity
```

---

# 41. Network similarly has reachability and capacity dimensions

RF7 established endpoint/reachability as environment/dependency facts.

RF8 adds:

```text
bandwidth
latency under load
connection slots
rate limits
```

A network may be reachable yet operationally scarce/congested.

Therefore:

```text
Reachable != SufficientNetworkCapacity
```

---

# 42. GPU existence versus GPU allocation

A compatible GPU may exist while:

```text
VRAM unavailable
memory fragmented/pool constrained
compute streams saturated
another process holds allocations
```

CUDA memory allocation APIs can fail when allocation cannot be satisfied; newer allocator
models also expose reusable memory pools and asynchronous allocation lifetimes.

RF8 uses this to separate:

```text
DeviceCapability
Capacity
Allocation
Availability
```

---

# 43. Memory pool illustrates reserved/reusable allocator state

CUDA stream-ordered memory pools may retain/cachе allocations for reuse, reducing future
allocation overhead while increasing retained footprint.

Thus a resource manager can intentionally keep capacity unavailable to general allocation
because it is retained in a pool.

Again:

```text
FreePhysicalMemory != AllocatorAvailableMemory
```

---

# 44. Overcommit

**Overcommit** means total promised/configured potential demand exceeds underlying
simultaneously realizable physical capacity, based on the expectation that peak claims do
not occur together or can be reclaimed/throttled/failed.

Examples:

```text
sum of cgroup limits > parent capacity
virtual memory promises > physical RAM
many Jobs each allowed 4 GiB on a 16 GiB host
```

Overcommit can improve utilization but increases tail/failure risk.

---

# 45. Overcommit is not necessarily an error

Linux cgroup explicitly permits limit overcommit.

Managed-memory systems can also oversubscribe/migrate memory depending on platform
support.

Thus:

```text
Overcommit != InvalidConfiguration
```

The question is whether the enforcement/reclaim/failure semantics are acceptable.

---

# 46. Reservation is stronger than overcommitted limit for admission guarantees

If Runtime wants to guarantee one of N discrete execution slots, a durable reservation can
exclude new claimants.

A mere `limit=N` attached independently to each Job would not reserve a shared slot.

Thus:

```text
SharedCapacitySafety requires shared arbitration/reservation,
not only per-consumer limits.
```

This explains current concurrency design.

---

# 47. `global_limit` is currently caller/request-carried capacity policy, not discovered capacity

Current low-level `SubmitRequest` carries `global_limit`, while production MCP/server
configuration resolves the production global admission limit.

Request identity intentionally excludes this capacity/observation policy.

RF8 interpretation:

```text
global_limit is an admission-policy ceiling used by Registry arbitration
```

not a physical property of the Job or machine.

---

# 48. Capacity policy should not enter historical request identity by default

RF4 replay semantics already preserve old Jobs across current capacity-policy change.

This makes sense because:

```text
same intended Operation
```

can be admitted under different current global capacity ceilings without changing the
Operation itself.

Thus:

```text
CapacityAdmissionPolicy != OperationIdentity
```

unless a domain contract explicitly makes resource entitlement semantic.

---

# 49. Per-Attempt resource budget does enter Operation identity

In contrast, `ExecutionBudget` is bound into the Execution Plan/operation digest.

Why?

Because:

```text
same program under MemoryMax=1GiB
```

and:

```text
same program under MemoryMax=16GiB
```

can have materially different possible realization behavior/outcomes.

Therefore:

```text
CommittedConsumptionEnvelope can be Operation-semantic.
```

---

# 50. Budget versus admission capacity therefore have different identity roles

Current Runtime exposes a useful distinction:

```text
ExecutionBudget          part of committed physical realization contract
global/workspace limit   current shared admission policy
```

They should not be merged into one `resourceConfig` identity field.

---

# 51. Resource exhaustion

**Resource exhaustion** occurs when a required allocation/service cannot be provided under
current capacity, policy and contention state.

Examples:

```text
ENOMEM/OOM
ENOSPC
process limit reached
GPU allocation failure
API 429/rate limit
Runtime CONCURRENCY_LIMIT
```

But these failures arise from different owners/contracts.

---

# 52. Resource exhaustion is not ordinary semantic failure

A compiler returning “syntax error” is not resource exhaustion.

A process killed because memory limit is exceeded is resource-governance/realization
failure.

A provider returning rate-limit exhaustion is external resource denial.

Thus:

```text
ResourceFailure != DomainSemanticFailure
```

This classification matters for retry/recovery policy later.

---

# 53. Resource exhaustion is also not always infrastructure failure

If a Job deliberately requests too little memory and exceeds its committed `MemoryMax`,
that is a consequence of the requested realization envelope.

It should not automatically be blamed on Runtime infrastructure.

Therefore:

```text
ResourceLimitHit can be OperationOutcome,
not necessarily RuntimeInfrastructureFailure.
```

---

# 54. Global capacity rejection is pre-commit admission failure

Current `CONCURRENCY_LIMIT` occurs before a new Job/Attempt reservation is committed.

The response therefore has retryable/not-started semantics.

This is different from:

```text
Job admitted and later OOMs
```

which is post-commit realization failure.

Thus:

```text
AdmissionCapacityExhaustion != ExecutionResourceExhaustion
```

---

# 55. Resource leak

A **resource leak** occurs when resource allocation/reservation remains held beyond the
lifetime justified by the owning consumer/contract.

Examples:

```text
unreleased memory
open file descriptor
stale lock
stale concurrency reservation
GPU allocation retained after failure
```

Leak detection requires ownership/lifetime semantics from RF3/RF4.

---

# 56. `held_orphaned` is not automatically a leak

An orphaned Attempt deliberately holding its concurrency reservation can be correct while
reconciliation remains unresolved.

It becomes a leak only if the reservation is no longer justified yet fails to release.

Thus:

```text
LongHeldResource != ResourceLeak by duration alone
```

---

# 57. Recovery authority and resource release interact

RF6 introduced recovery authority.

RF8 adds:

```text
safe recovery often includes proving resources can be released
```

For example Runtime refuses to release ambiguous capacity too early because doing so could
admit additional work while the old execution may still exist.

This is a safety-vs-utilization tradeoff.

---

# 58. Conservative reservation reduces utilization to preserve safety

`held_orphaned` can lower apparent liveness/capacity utilization.

But premature release could violate the global concurrency invariant.

Thus:

```text
MaxUtilization != MaxCorrectness
```

and resource control must include uncertainty semantics.

---

# 59. Resource accounting

**Accounting** measures/attributes resource usage, reservation or entitlement to owners.

Examples:

```text
cgroup memory.current
CPU usage counters
filesystem blocks
rate-limit counters
Runtime reservation rows
```

Accounting is evidence; it does not itself impose a policy.

Thus:

```text
Accounting != Limiting != Scheduling
```

---

# 60. Resource controller

A **resource controller** is a mechanism that enforces distribution/limits/protections over
resource use.

Linux cgroup controllers are canonical examples.

Runtime currently delegates selected physical resource enforcement to systemd/cgroup or
Windows Job controls rather than reimplementing low-level resource accounting.

This remains sound ownership.

---

# 61. Scheduler

RF8 intentionally gives only a preliminary definition before RF9:

> **A scheduler chooses when/where/which eligible work receives service from scarce
> resources according to an ordering/allocation policy.**

Current Runtime does not maintain an internal waiting queue or choose among queued Jobs.

Therefore:

```text
ResourceLimits + AdmissionGate != Scheduler
```

---

# 62. Resource manager

A broad **resource manager** may discover capacity, arbitrate allocations/reservations,
enforce quotas/limits, reclaim resources and coordinate scheduling across resource types.

Current Runtime implements only selected pieces:

```text
admission-slot reservation
per-Attempt limit commitment
delegated OS enforcement
capacity-holder observation/reconciliation
```

It does not own complete machine/resource allocation.

Thus:

```text
Current Runtime != General Resource Manager
```

---

# 63. Resource availability and scheduler policy must remain distinct

Suppose one CPU is idle but a high-priority Job is intentionally delayed due to policy.

Resource exists and is available physically, yet scheduler chooses not to allocate it.

Conversely scheduler may want to run work but no capacity exists.

Therefore:

```text
SchedulingDecision != AvailabilityFact
```

RF9 will build on this.

---

# 64. Priority/fairness are distribution rules, not resources

A priority number or fair-share weight is metadata/policy controlling how resource service
is distributed.

It is not capacity itself.

Thus:

```text
Priority != Resource
FairnessPolicy != Capacity
```

---

# 65. Locks are resources of cardinality one

An exclusive lock has capacity:

```text
1 holder
```

but unlike CPU bandwidth, it is typically indivisible/exclusive.

Waiting consumers experience contention despite essentially zero “utilization percentage”
concept.

This shows why resource models need classes.

---

# 66. Runtime Workspace limit resembles an exclusive logical lease more than CPU quota

One active execution per Workspace creates a cardinality-one coordination resource:

```text
Workspace execution lease/slot
```

This prevents conflicting concurrent realization under current Workspace semantics.

It is conceptually closer to a lock/lease than to RAM allocation.

---

# 67. External API rate limits are resources outside Runtime ownership

A provider may grant:

```text
N requests per interval
```

and return 429/rejection when exhausted.

Runtime generic execution does not own that counter, capacity or reset schedule.

Therefore:

```text
ExternalRateCapacity != RuntimeCapacity
```

unless a structured provider adapter explicitly imports that contract.

---

# 68. Money/credits/tokens can be resources too, but belong to domain owners

Some operations consume:

```text
cloud credits
API tokens
transaction fees
monetary budget
```

RF8's resource ontology can represent them, but generic Runtime should not become the
semantic owner of Finance/economic resource policy.

This is a boundary lesson from RF6/RF7.

---

# 69. Scarcity implies opportunity cost only relative to alternatives

When one scarce slot is reserved for Job A, Job B may be delayed/rejected.

That creates opportunity cost at the coordination layer.

But RF8 does not yet define value/utility-based scheduling.

It only establishes that scarcity makes competing allocation alternatives mutually
relevant.

---

# 70. Resource pressure can justify dynamic admission—but does not currently do so

In principle Runtime could use PSI/free-memory/GPU/network observations to alter admission.

Current Runtime intentionally does not.

Its global concurrency capacity is configured and durable, not dynamically inferred from
machine pressure.

RF8 does not authorize changing this.

---

# 71. Why fixed admission capacity can be preferable

A configured fixed limit has useful properties:

```text
simple contract
predictable concurrency ceiling
durable reservation semantics
no noisy utilization feedback loop
no false precision about heterogeneous resource demand
```

Dynamic pressure admission could improve utilization but would add another control loop,
new evidence semantics and new failure modes.

It requires a demonstrated consumer.

---

# 72. Per-Job limits can coexist with global overcommit

Suppose:

```text
8 admitted Jobs
MemoryMax=4 GiB each
physical RAM=16 GiB
```

Potential maximums sum to 32 GiB.

This can still be a valid configuration because the per-Job maxima are ceilings, not
reservations.

Whether the workload behaves acceptably depends on actual demand/reclaim/pressure.

---

# 73. Therefore admission slots and physical budgets are orthogonal dimensions

A single low-memory Job can occupy one slot.

A huge-memory Job also occupies one slot.

Current concurrency capacity therefore does not attempt weighted packing by memory/CPU.

Thus:

```text
1 ReservationSlot != EqualPhysicalDemand
```

This is an explicit limitation of current admission semantics, not necessarily a bug.

---

# 74. Multi-resource packing is a different problem

A generalized resource scheduler might need vectors:

```text
CPU
memory
GPU
VRAM
network
I/O
special devices
```

and choose feasible placements/allocations.

Current Runtime does not solve this and should not be described as if `global_limit` did.

---

# 75. Fragmentation/topology can make arithmetic availability misleading

Examples:

```text
GPU memory fragmentation
NUMA locality
filesystem inode exhaustion despite free bytes
port range exhaustion
contiguous hugepage requirements
```

Therefore:

```text
SumFreeCapacity >= Request
```

may still not imply allocatability.

Availability can be structure/topology dependent.

---

# 76. Resource ownership/lifetime classes

RF8 distinguishes:

```text
Owned        Runtime creates/controls lifecycle
Borrowed     Runtime temporarily uses external resource
Reserved     capacity held for consumer
Leased       time-bounded revocable/expiring right
Shared       concurrently usable with contention
Exclusive    one/few holders
External     provider-owned capacity
```

This vocabulary matters for cleanup/recovery.

---

# 77. Power Request is a lease-like supporting resource/control mechanism

Current Windows launcher acquires an Attempt-scoped `SystemRequired` Power Request while
physical work is active and releases it with launcher lifetime/terminal completion.

RF8 interprets it as:

```text
Attempt-lifetime system power-policy lease/control request
```

not as CPU/memory capacity.

This is another resource-like relation that does not fit simple byte/count models.

---

# 78. Shared caches are reusable resources with coupling tradeoffs

Current trusted-local package caches survive Workspace closure because they are reusable
infrastructure.

They can improve throughput and reduce network/storage work, while introducing shared
state/capacity contention and broader dependency coupling.

Thus:

```text
ResourceReuse can improve efficiency while reducing isolation.
```

RF7/RF8 intersect here.

---

# 79. Reclaim is resource recovery, not identity erasure

Runtime reclaim/lifecycle may release disposable cache/storage/Workspace resources while
preserving durable tombstones/history.

RF4 ensures historical identity can survive physical resource release.

Thus:

```text
ReleaseResource != EraseHistory
```

---

# 80. Scarcity observation and capacity truth have different owners

Runtime can authoritatively know:

```text
its own active reservation count
its configured global/workspace limit
its committed per-Attempt budgets
```

It cannot authoritatively infer all current host/provider capacity without explicit probes
and contracts.

Therefore:

```text
RuntimeCapacityTruth is scoped.
```

---

# 81. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| cgroup child limits sum > parent | limit = reservation | limits may be overcommitted |
| Runtime slot reserved before dispatch | reservation = usage | capacity held before physical execution |
| held orphan occupies slot | reservation = measured activity | safety commitment survives uncertainty |
| MemoryMax=4GiB on pressured host | limit = available memory | ceiling does not guarantee allocation |
| CPUQuota=100% under contention | quota = CPU ownership | rate ceiling shares scheduler capacity |
| TasksMax=100 | task limit = CPU capacity | cardinality bound is not compute reservation |
| output truncation | output limit = physical I/O use | retained bytes differ from emitted bytes |
| timeout | time budget = stored capacity | deadline bound has different mechanics |
| CPU 100% productive | utilization = pressure | high usage can have low harmful stall |
| PSI memory stall | utilization = scarcity impact | pressure measures lost/stalled work |
| disk free but IOPS saturated | storage capacity = storage service | stock and throughput differ |
| endpoint reachable but bandwidth congested | dependency = sufficient capacity | availability/service quality differ |
| GPU present but cuda allocation fails | resource existence = available capacity | device and allocation state differ |
| CUDA pool retains memory | physical free = allocator available | allocator reservation/cache changes availability |
| API quota permits 1000 calls | quota = reserved service | entitlement does not reserve provider capacity |
| file lock | all resources divisible | exclusive cardinality-one resource |
| fixed global limit with idle host | scarcity = physical shortage | policy-created scarcity exists |
| eight 4GiB Jobs on 16GiB host | per-Job max = physical reservation | global overcommit possible |
| one slot for tiny/huge Job | slot = equal resource demand | admission units abstract heterogeneous demand |
| leaked reservation | old consumer = safe release | ownership/lifetime evidence required |

---

# 82. RF8 anti-laws

RF8-L1: `Resource != Limit`.

RF8-L2: `ResourceExistence != Capacity`.

RF8-L3: `Capacity != Availability`.

RF8-L4: `AvailabilityObservation != FutureCapacityGuarantee`.

RF8-L5: `Reservation != Usage`.

RF8-L6: `Reservation != Allocation`.

RF8-L7: `Entitlement != Availability`.

RF8-L8: `Quota != Reservation`.

RF8-L9: `Limit != GuaranteedCapacity`.

RF8-L10: `ConfiguredLimit != ReservedCapacity`.

RF8-L11: `Budget != Reservation`.

RF8-L12: `MemoryMax != MemoryReservation`.

RF8-L13: `TasksMax != ComputeCapacityReservation`.

RF8-L14: `CPUQuota != ExclusiveCPUReservation`.

RF8-L15: `TimeBudget != CapacityInventory`.

RF8-L16: `RetainedOutputLimit != TotalPhysicalIOUsage`.

RF8-L17: `GlobalLimit != MachineCapacity`.

RF8-L18: `WorkspaceConcurrencyCapacity != HardwareCapacity`.

RF8-L19: `AdmissionSlotReservation != PhysicalUsageMeasurement`.

RF8-L20: `Scarcity != PhysicalExhaustionOnly`.

RF8-L21: `Dependency != Contention`.

RF8-L22: `Pressure != Utilization`.

RF8-L23: `PressureMetric != ResourceManager`.

RF8-L24: `CPUCapacitySemantics != MemoryCapacitySemantics`.

RF8-L25: `StorageCapacity != StorageServiceCapacity`.

RF8-L26: `Reachable != SufficientNetworkCapacity`.

RF8-L27: `Overcommit != InvalidConfiguration`.

RF8-L28: `CapacityAdmissionPolicy != OperationIdentity`.

RF8-L29: `AdmissionCapacityExhaustion != ExecutionResourceExhaustion`.

RF8-L30: `ResourceFailure != DomainSemanticFailure`.

RF8-L31: `LongHeldResource != LeakByDurationAlone`.

RF8-L32: `Accounting != Limiting != Scheduling`.

RF8-L33: `ResourceLimits+AdmissionGate != Scheduler`.

RF8-L34: `CurrentRuntime != GeneralResourceManager`.

RF8-L35: `SchedulingDecision != AvailabilityFact`.

RF8-L36: `Priority != Resource`.

RF8-L37: `OneReservationSlot != EqualPhysicalDemand`.

RF8-L38: `ReleaseResource != EraseHistory`.

---

# 83. Minimum RF8 grammar

## 83.1 Resource `R`

Entity/service/right/substrate enabling or constraining realization.

## 83.2 Capacity `Cap(R,scope,t)`

Maximum service/quantity/concurrency under model and interval.

## 83.3 Availability `Avail(R,consumer,t)`

Currently usable service/quantity after current allocation/reservation/policy/topology.

## 83.4 Allocation `Alloc(R,C,q)`

Actual assignment/use of resource quantity/service q to consumer C.

## 83.5 Reservation `Resv(R,C,q,lifetime)`

Capacity/entitlement held for C independent of immediate physical usage.

## 83.6 Entitlement `Ent(C,R,q)`

Recognized right/claim to resource service.

## 83.7 Quota `Quota(C,R,scope,interval)`

Policy/accounting maximum allowance, usually not physical reservation.

## 83.8 Limit `Limit(C,R,q/rate)`

Enforced upper consumption/rate/count bound.

## 83.9 Budget `Budget(O,{R_i:q_i})`

Committed multi-dimensional resource expenditure/consumption envelope.

## 83.10 Usage `Use(C,R,t)`

Observed/measured resource consumption.

## 83.11 Contention `Contend({C_i},R,t)`

Multiple consumers competing/interfering over R.

## 83.12 Scarcity `Scarce(R,scope,t)`

Demand/claims conflict with available service/capacity, requiring tradeoff.

## 83.13 Pressure `Pressure(R,scope,t)`

Measured stall/degradation resulting from contention/scarcity.

## 83.14 Overcommit

Potential/configured aggregate claims exceed simultaneous underlying physical capacity.

---

# 84. Current Runtime resource map

| Current mechanism | RF8 interpretation |
|---|---|
| `global_limit` | configured Runtime-global discrete admission-capacity ceiling |
| Workspace execution limit | cardinality-one logical/coordination capacity |
| `concurrency_reservations` | durable admission-slot reservations |
| `active` reservation | currently held committed admission capacity |
| `held_orphaned` | conservative held capacity under unresolved execution truth |
| `released` | capacity claim relinquished |
| `MemoryMax` | per-Attempt memory consumption ceiling |
| `TasksMax` | per-Attempt task-count ceiling |
| `CPUQuota` | per-Attempt CPU service-rate ceiling |
| Windows Job memory/process/CPU controls | target-native enforcement of same resource dimensions |
| Job/step timeout | temporal execution bound |
| stdout/stderr retention limits | bounded retained observation/output storage |
| cgroup ownership | OS-owned accounting/enforcement substrate |
| Runtime capacity-holder projection | admission-reservation accounting/evidence |
| Power Request | Attempt-lifetime Windows power-policy lease/control request |
| caches | shared/reusable storage/network-efficiency resources |
| reclaim/lifecycle | resource release/recovery mechanisms |

---

# 85. What RF8 strongly supports in current design

## 85.1 Durable reservations for discrete concurrency safety

Current SQLite reservation model correctly separates admission commitment from dispatch and
survives Runtime reconstruction.

## 85.2 Immediate rejection rather than hidden queue

Until a scheduler/queue contract exists, `CONCURRENCY_LIMIT` is more truthful than silently
waiting/reordering work.

## 85.3 Delegating low-level enforcement to OS primitives

Runtime should continue using systemd/cgroup and Windows Job controls rather than
reimplementing resource accounting.

## 85.4 Keeping per-Attempt budgets distinct from global admission capacity

They protect different invariants and have different identity semantics.

## 85.5 Holding orphaned reservations conservatively

Capacity can remain blocked when safe release is uncertain; utilization is secondary to
not violating concurrency guarantees.

---

# 86. What RF8 reopens

## 86.1 `global_limit` is not workload-aware

Every active Attempt consumes one slot regardless of actual CPU/memory/GPU demand.

This is intentionally simple but may become inefficient for heterogeneous heavy workloads.
A concrete consumer is required before adding vector packing/scheduling.

## 86.2 Runtime does not model host pressure

CPU/memory/I/O PSI, free disk, GPU memory and network congestion are not current admission
inputs. RF8 recognizes them as possible future observations but does not recommend adding
them without a falsified fixed-capacity workload.

## 86.3 Resource failure classification could become more explicit

Future experience may justify distinguishing capacity admission failure, cgroup-limit hit,
OOM, disk exhaustion, provider rate-limit and domain failures. Existing reason codes/source
truth should be audited before adding abstractions.

## 86.4 Output/time “limits” should not be described as the same class as cgroup budgets

They are all mechanical bounds, but target different resource/observation dimensions.
Documentation terminology can become more precise without API change.

---

# 87. Research constitution added by RF8

1. Never infer reserved/available capacity from a configured per-consumer limit.
2. Name the resource class, scope, units and interval for capacity claims.
3. Keep resource existence, capacity, availability, allocation and reservation separate.
4. Treat quota/entitlement as policy/accounting rights, not physical reservations by
   default.
5. Treat budgets as committed consumption/expenditure envelopes unless a reservation
   mechanism explicitly exists.
6. Distinguish renewable service resources, stateful stock, cardinality resources,
   exclusives, rate/token resources and temporal opportunities.
7. Keep utilization separate from pressure; pressure is about degraded/stalled work.
8. Keep dependencies separate from contention over dependencies.
9. Do not call `global_limit` machine capacity; it is Runtime admission capacity.
10. Do not call one reservation slot equal physical demand.
11. Preserve durable conservative reservations under ambiguous ownership/execution state.
12. Separate admission-capacity exhaustion from post-admission physical resource failure.
13. Delegate physical enforcement/accounting to OS/provider owners when they already own
    the resource semantics.
14. Do not add queue/scheduler/fairness semantics merely because resources are scarce.
15. Do not add dynamic pressure-based admission until a concrete workload proves fixed
    admission capacity inadequate.
16. Treat resource release, cleanup and historical identity as separate lifecycle concerns.

---

# 88. RF8 result — conceptual compression

```text
Resource says what service/substrate/right can enable realization.
Capacity says how much service can be supported under a scope/model.
Availability says what is usable now.
Allocation says what is actually assigned/used.
Reservation says what capacity is held.
Entitlement/quota say what policy permits/allocates as an allowance.
Limit says what may not be exceeded.
Budget says what one Operation commits as a consumption envelope.
Contention says consumers compete.
Scarcity says claims exceed/conflict with available service.
Pressure says contention/scarcity is degrading progress.
```

The strongest Runtime-specific compression is:

```text
current concurrency reservation
  = durable logical admission-capacity claim
  != CPU/RAM reservation

current ExecutionBudget
  = per-Attempt consumption ceilings
  != shared-capacity reservation

current Runtime
  = admission + selected limit enforcement + recovery/accounting
  != scheduler
  != complete resource manager
```

---

# 89. RF8 closeout

RF8 closes with twenty-one durable results:

1. **Resource, Capacity, Availability, Allocation and Reservation are distinct.**
2. Resource capacity can be physical, contractual or provider-defined.
3. Availability is current consumer-relative usable capacity and is not guaranteed by a
   prior observation.
4. Entitlement/quota/limit govern allowed use but do not imply physical reservation.
5. Linux cgroup limit overcommit directly proves configured limits are not reservations.
6. **Current `ExecutionBudget` represents per-Attempt consumption ceilings, not pre-
   reserved physical resources.**
7. `MemoryMax`, `TasksMax` and `CPUQuota` govern different resource dimensions and do not
   mean memory/CPU/process capacity is reserved.
8. Windows native resource controls implement corresponding requested dimensions under
   target-specific accounting, not identical Linux semantics.
9. Time/output limits are mechanical bounds but differ from CPU/memory capacity inventory.
10. **Current `global_limit` is a configured Runtime admission-capacity ceiling, not host
    machine capacity.**
11. Current `concurrency_reservations` are genuine durable discrete-capacity reservations
    committed before physical dispatch.
12. `held_orphaned` intentionally preserves capacity under unresolved execution truth;
    reservation tracks safety commitment rather than measured utilization.
13. Current per-Workspace capacity is a cardinality-one coordination/lease resource rather
    than hardware capacity.
14. Scarcity may be physical, contractual or policy-created; contention can arise before
    exhaustion.
15. Pressure is degradation/stall from resource contention/scarcity and is distinct from
    utilization; Linux PSI supplies a mature concrete model.
16. CPU, memory, storage, network, locks, GPU memory, API rate tokens and time have
    materially different resource semantics; no universal scalar model is sufficient.
17. Overcommit can be valid and useful; whether it is safe depends on enforcement,
    reclaim, pressure and failure semantics.
18. Admission-capacity exhaustion is pre-commit and distinct from post-admission resource
    exhaustion/limit hits.
19. Resource accounting, limiting, admission, scheduling and general resource management
    are separate responsibilities.
20. **Current Ordivon Runtime constrains and reserves selected resource dimensions but is
    neither an internal scheduler nor a complete resource manager.**
21. Dynamic pressure-aware admission, multi-resource packing, priorities and fairness
    remain unjustified until concrete workloads falsify the current immediate fixed-
    capacity model.

The next frontier is:

> **Given scarce resources and concurrent eligible Operations, who should be admitted,
> delayed, ordered, coordinated or rejected? What is scheduling versus admission, queueing,
> arbitration, fairness, priority and backpressure?**

That is RF9 — Scheduling, Coordination, Admission and Backpressure.
