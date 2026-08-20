---
schema_version: 1
id: runtime.foundations.rf7.continuation
title: RF7 Continuation — Enter RF8
type: continuation
profile: research
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - agent
updated: 2026-08-17
summary: Cross-conversation continuation checkpoint after RF7, preserving boundary/environment/dependency distinctions and the exact RF8 frontier on Resource, Capacity, Constraint and Scarcity.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf7
  - runtime.foundations.rf7.sources
---
# RF7 Continuation — Enter RF8

## Completed

RF7 — Boundary, Environment, Context and Dependency.

Primary record:

`research/foundations/RF7-BOUNDARY-ENVIRONMENT-CONTEXT-DEPENDENCY.md`

Source map:

`research/foundations/RF7-SOURCES.md`

## Durable results to carry forward

1. Boundary is dimension-specific; no universal Runtime perimeter exists.
2. Isolation is a vector of restrictions over namespace, authority, resource, failure,
   information, provider and other dimensions rather than a boolean.
3. Process/address-space, mount/PID/network namespace, authority, resource and provider
   boundaries are distinct.
4. Path/object resolution is context/namespace dependent; same path text does not prove
   same target object.
5. Current P4 physically proves `runtime_host_namespace_path_witness` does not establish
   target mount-namespace view integrity or byte-consumption truth.
6. Environment is the materially available external condition/service set for realization;
   State is an observer/model projection; Context is broader interpretation/decision
   framing.
7. Explicit Input is only one subset of Dependencies.
8. Dependency is claim-relative: a fact/service/entity whose counterfactual drift/removal
   can alter realization/result/effect or invalidate claim Q.
9. Keep direct, transitive, ambient, authority, kernel/hardware, network/service,
   temporal/random and recovery dependencies distinct where relevant.
10. Dependency handling has different strengths: `freeze`, `materialize`, `bind`,
    `witness`, `virtualize`, `query/reconcile`, `leave-open`.
11. Dynamic linker/P2/P3 prove `ExecutableDigest != ExecutionClosureDigest` and generic
    static dependency closure is not available for arbitrary execution.
12. Immutable inputs provide strong Runtime-owned byte/materialization closure for the
    declared explicit inputs only.
13. Host Dependencies remain Agent/domain-declared path+digest prerequisites plus an
    Attempt-lifetime Runtime-host-namespace continuity witness, not automatic discovery or
    target-view immutability.
14. ExecutionProviderSnapshot freezes/binds Runtime-owned dispatch-provider identity only.
15. Runtime's explicit process environment and fixed locale reduce ambient variability but
    do not close kernel, toolchain, time, randomness, device or network dependencies.
16. Containers are compositions of host-kernel namespace/cgroup/capability/filesystem
    boundaries; same image does not imply same kernel semantics.
17. VMs add a guest-kernel/virtual-machine boundary but remain dependent on host KVM/
    hardware/devices/network and are not closed universes.
18. Sandbox is a contract/threat-model term, not synonymous with container, namespace or VM.
19. Hermeticity, determinism, isolation and reproducibility are distinct properties.
20. Dependency closure is relative to the claim and may be impossible to enumerate for
    arbitrary open-world execution; authority restriction can sometimes engineer closure
    more reliably than dependency enumeration.
21. Stronger isolation may alter target semantics; P3 sealed-memfd falsifier proves byte
    closure and semantic transparency can conflict.
22. Current Ordivon should preserve strong narrow closure claims and explicit open-world
    boundaries instead of inventing a generic “fully captured environment” abstraction.

## RF7 compact grammar

```text
B_d          dimension-specific boundary
Iso_d        restricted interaction across B_d
Env(O,t)     materially relevant available external conditions during realization
Ctx(O)       broader interpretation/decision context
I            explicit operand/input
D∈Dep(Q)     claim-relative dependency
Closure(Q)   dependency set sufficient for claim/equivalence Q
H(D)         freeze | materialize | bind | witness | virtualize | query | leave-open
HEnv         hermetic envelope for the declared relevant dimensions
B⃗           vector of boundary/isolation dimensions
```

## Current implementation interpretation

```text
sourceStateDigest              frozen tracked-source projection
Execution Plan                 frozen operation/environment projection
immutable input set            Runtime-owned materialized explicit-input closure
InputAuthority                 source namespace/authority boundary
HostDependency                 declared bound filesystem prerequisite
path drift watcher             runtime-host-namespace continuity witness
ExecutionProviderSnapshot      Runtime-owned dispatch-provider dependency commitment
minimal env + fixed locale     constructed process-environment projection
contained_local                composite partial isolation/authority/environment profile
trusted_local                  broad/open ambient local context
Windows limited input tree     authority-scoped read-only materialized input boundary
ForeignReference               external identity/dependency bind, not closure
network/kernel/GPU/time        generally open unless separately contracted
```

## RF8 exact frontier

Enter:

# RF8 — Resource, Capacity, Constraint and Scarcity

Core questions:

```text
What is a resource for Runtime: consumable quantity, reusable capability, substrate,
exclusive object, time budget, concurrency slot or several classes?
What is capacity versus availability versus allocation versus reservation?
What is scarcity: finite physical quantity, admission policy, contention or opportunity
cost?
What is a budget versus quota versus limit versus reservation versus entitlement?
How do CPU, memory, process count, disk, I/O, network, GPU, locks and concurrency slots
differ as resources?
What is pressure and how should Runtime observe it?
What is overcommit and when is it safe?
What is contention versus dependency?
How should per-Job limits relate to global capacity and scheduler decisions?
What does fairness/priority mean before RF9 scheduling/policy?
What resources are owned, borrowed, leased, shared or externally allocated?
How do resource exhaustion and ordinary execution failure differ?
```

Mandatory falsifiers:

```text
CPU is reusable/shareable while disk bytes remain allocated
memory overcommit/OOM
cgroup MemoryMax/TasksMax/CPUQuota
one global_limit concurrency slot with many queued callers
GPU exists but VRAM is exhausted
network endpoint reachable but bandwidth congested
file lock/exclusive resource versus divisible CPU time
external API rate limit
filesystem free-space exhaustion
time budget expiration despite abundant CPU
reservation that is never consumed
quota that permits usage without reserving capacity
resource leak after process crash
shared cache improves throughput while increasing contention/coupling
```

Do not enter RF9 Scheduling, Coordination and Admission until RF8 separates resource
existence, capacity, availability, entitlement, allocation/reservation and constraint well
enough that a scheduler is not mistaken for the resource itself.
