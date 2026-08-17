---
schema_version: 1
id: runtime.foundations.rf14
title: RF14 — Determinism, Nondeterminism and Agent Action
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
summary: RF14 separates determinism, repeatability, reproducibility, replay, re-execution and hermeticity. It establishes that determinism is always relative to a complete modeled state/input boundary; same Runtime Operation identity does not imply same physical result because current identity intentionally excludes many ambient/open dependencies including wall clock, kernel RNG, scheduling, ignored caches and arbitrary external-service state. Current exact replay returns the original durable Job/history and does not re-execute. Source/input/provider/environment commitments improve input identity and reproducibility without claiming a hermetic or deterministic world. Runtime should freeze dependencies only where contracts require it, record important nondeterministic realization evidence where useful, and leave semantic/model/external nondeterminism explicit rather than manufacturing false determinism.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF14
related:
  - runtime.foundations.start
  - runtime.foundations.rf13
  - runtime.foundations.rf14.sources
  - runtime.foundations.rf14.continuation
---
# RF14 — Determinism, Nondeterminism and Agent Action

## 0. Status and research question

RF13 established which execution dimensions are isolated, attenuated, trusted or open. RF14
asks:

> **Given the same logical Operation, why can physical realization still differ, what does
> determinism mean relative to a state boundary, what can be reproduced or replayed, and
> which nondeterminism should Runtime freeze, capture, virtualize, observe or simply leave
> open?**

RF14 is explanatory research only. It does not authorize deterministic replay machinery,
clock/RNG virtualization, a hermetic build subsystem, cache redesign, model sampling controls,
container/VM changes or production API changes.

---

# 1. First deletion — same request is not same result

Current Runtime can strongly establish:

```text
same request identity
same Operation identity
same durable Job
```

without establishing:

```text
same physical execution
same output bytes
same external Effects
```

Therefore:

```text
SameOperationIdentity != SameResult
```

---

# 2. Determinism

RF14 defines a system/process as **deterministic relative to modeled state S and input I** if:

```text
F(S,I) -> exactly one permitted next/result behavior
```

under the declared semantics.

The phrase “relative to S and I” is essential.

---

# 3. Determinism is boundary-relative

Program:

```python
print(time.time())
```

looks nondeterministic if wall clock is omitted from input.

If clock value is explicitly modeled as input:

```text
F(program, clock_value)
```

it may be deterministic relative to that larger state.

Thus:

```text
Nondeterminism often means omitted/uncontrolled state relative to a model.
```

---

# 4. True/open nondeterminism versus hidden state

RF14 separates:

```text
modeled deterministic variation
hidden/ambient state dependence
concurrency/scheduling nondeterminism
random sampling
external/open-world nondeterminism
implementation/platform variation
```

These causes demand different controls.

---

# 5. Repeatability

**Repeatability** means obtaining an equivalent result again under nominally the same setup,
often on the same system/operator/environment.

It is an empirical property of repeated trials.

It does not itself prove mathematical determinism.

---

# 6. Reproducibility

For software builds, the Reproducible Builds project defines reproducibility using the same
source code, build environment and build instructions producing bit-for-bit identical
specified artifacts by any party.

RF14 generalizes the lesson:

> reproducibility requires declaring which environment attributes belong to the reproduction
> contract and which output equivalence is expected.

---

# 7. Repeatability is weaker evidence than reproducibility proof

Running the same command twice and observing equal output demonstrates two matching trials.

It does not establish that all relevant state is controlled.

Therefore:

```text
TwoEqualRuns != ProvenDeterminism
```

---

# 8. Reproducibility is purpose-relative

Possible output equivalence relations include:

```text
byte-identical artifact
same parsed AST
same numerical result within tolerance
same semantic state transition
same business Effect identity
```

So:

```text
Reproducible(Q) requires an equivalence relation ~Q.
```

---

# 9. Hermeticity

RF14 uses **Hermeticity** for execution/build closure from undeclared ambient dependencies:

> the realized action depends only on declared/managed inputs, tools, configuration and
> permitted services/channels under the hermetic contract.

Bazel's current documentation emphasizes isolation from host-installed software and explicit
management of tools/dependencies; it lists timestamps, host binaries and source-tree writes
as common non-hermetic influences.

---

# 10. Hermeticity is not determinism

A hermetic action may deliberately call a random generator supplied inside the hermetic
boundary.

Thus:

```text
Hermetic != Deterministic
```

---

# 11. Determinism is not hermeticity

A program can deterministically depend on an ambient file whose contents happen to be fixed.

The output may be deterministic for that world while the dependency is undeclared.

Therefore:

```text
Deterministic != Hermetic
```

---

# 12. Hermeticity improves reproducibility because dependency closure becomes explicit

If all relevant tools/dependencies/inputs are identified and isolated from host drift, a
future reproducer can reconstruct the environment more reliably.

But reproducible-build systems still document timestamps and other residual nondeterminism as
separate problems.

---

# 13. Nix is a useful competing model

Nix builds strongly encode dependency identity and use sandboxed build environments, making
reproducibility easier.

Yet Nix's own reproducibility project explicitly notes that this does not eliminate timestamps
or other nondeterminism.

So:

```text
DependencyClosure + Sandbox != AutomaticReproducibility
```

---

# 14. Replay

RF4 already established:

```text
Replay != Retry != ReExecution != ReObservation != Reconciliation
```

RF14 sharpens replay into multiple kinds.

---

# 15. Request replay

Current Runtime exact replay means:

```text
same principal + clientRequestId + semantic fingerprint
-> return original durable Job/Operation identity
```

It does not create a new execution.

---

# 16. Current exact replay is historical continuity

If original Job already completed:

```text
exact replay -> same completed Job/result/evidence
```

not:

```text
run target again and compare
```

Therefore:

```text
ExactReplay != ReExecution
```

---

# 17. Physical re-execution

A **Re-execution** creates a new physical realization of an action, normally with a new
Attempt/Operation lineage depending on contract.

Even with identical source and explicit request parameters, ambient/open state can differ.

---

# 18. Result reproduction

A **Result Reproduction** is a new realization intended to produce output equivalent to a
previous result under a declared relation.

It is not historical replay.

---

# 19. Deterministic replay in systems research is a stronger concept

A deterministic replay system attempts to reproduce a prior execution by recording or
controlling nondeterministic inputs/events such as scheduling, signals, system-call results,
external input or timing-sensitive order.

Current Runtime does not provide this.

---

# 20. Operation identity is not world-state identity

Current identity binds important explicit/runtime-owned semantics including:

```text
principal
Workspace ID
executable path
args
cwd
env
limits
steps
budget
execution profile/target/Windows authority
source-state fingerprint
provider commitment
explicit bound inputs where applicable
```

but not every state variable that target code can observe.

---

# 21. Current sourceStateDigest intentionally excludes ignored caches/build outputs

This is good source identity semantics:

```text
ignored compiler.cache changes
-> sourceStateDigest unchanged
```

But it directly proves:

```text
SourceIdentity != CompleteExecutionStateIdentity
```

---

# 22. Ignored mutable state can still influence execution

If target code intentionally reads an ignored cache/config artifact, two executions with the
same sourceStateDigest can behave differently.

Thus:

```text
IgnoredBySourceDigest != CausallyIrrelevant
```

---

# 23. Current documentation already states this boundary

Ignored files, wall-clock time, network state and ambient external services are not included
in source-state commitment.

RF14 strongly supports retaining this explicit limitation.

---

# 24. Executable/provider binding closes only selected toolchain state

Runtime can bind:

```text
target executable digest
Linux Runner digest
Windows launcher digest
explicit Host Dependency digests
```

This reduces tool/provider drift.

It does not automatically bind every dynamically loaded library, compiler subtool, kernel,
CPU feature or remote service.

---

# 25. Exact executable identity is not toolchain closure

A compiler executable can load:

```text
shared libraries
plugins
configuration
linker
assembler
SDK
system headers
```

Therefore:

```text
ExecutableDigest != CompleteToolchainIdentity
```

---

# 26. Host Dependencies can close selected ambient prerequisites

Explicit path+digest commitments improve reproducibility for dependencies intentionally named
by the caller.

But RF7/P4 showed these are neither automatic closure nor target-view immutability.

---

# 27. Immutable inputs close another selected dependency class

Materialized input bytes are much stronger than mutable paths for input identity.

But they still do not close:

```text
clock
RNG
scheduler
kernel
CPU
undeclared filesystem dependencies
external services
```

---

# 28. Environment freezing is valuable but partial

Current Runtime deliberately constructs/fixes important execution environment values and
binds explicit request environment rather than treating arbitrary service ambient variables
as semantic input.

This reduces one large source of accidental variation.

---

# 29. Environment map is not total environment

RF7 already established:

```text
ProcessEnvMap != CompleteExecutionEnvironment
```

Kernel state, filesystem, hardware, scheduler and network remain outside `env`.

---

# 30. Locale can be a nondeterministic-looking dependency

Sorting, formatting, parsing and tool output can vary with locale.

Fixing locale is therefore a reproducibility technique.

It does not make the rest of the execution deterministic.

---

# 31. Wall clock is an ambient input

Programs can call system clock APIs independently of Runtime request identity.

Example:

```text
same Operation parameters
run at t1 -> output A
run at t2 -> output B
```

So:

```text
SameOperationIdentity + OpenClock != SameOutput
```

---

# 32. Runtime timestamps are evidence, not clock virtualization

Runner records start/finish times.

That observes when execution occurred.

It does not force the target to observe a fixed clock.

Therefore:

```text
TimestampEvidence != DeterministicTime
```

---

# 33. Monotonic deadlines do not make workload time deterministic

Runtime uses monotonic `Instant` for timeout control.

This stabilizes deadline mechanics against wall-clock jumps.

It does not virtualize target clock APIs.

---

# 34. Kernel RNG is an explicit nondeterminism source

Linux `getrandom()` returns bytes from the kernel random source, backed by a CSPRNG seeded by
entropy/environmental noise.

Two otherwise identical executions can therefore receive different random bytes.

---

# 35. Isolation does not remove RNG nondeterminism

Contained-local can block network and hide host state while `/dev/urandom`/`getrandom()`
semantics remain available depending on exact sandbox policy.

Therefore:

```text
Contained != Deterministic
```

---

# 36. Seeded PRNG narrows one source of variation

If algorithm/version/seed/call sequence are fixed, a pseudorandom generator can produce a
repeatable stream.

But:

```text
seed alone
```

does not control scheduling, floating-point reduction order, library/version changes or other
random sources.

---

# 37. Seed is part of modeled state

When stochastic algorithms are intentionally used, a seed should be interpreted as one
explicit state/input dimension, not magic “deterministic=true”.

---

# 38. Concurrency creates schedule-dependent behavior

Two threads racing on shared state may produce different histories even from identical
initial bytes.

The scheduler/interleaving becomes a hidden or nondeterministic input.

---

# 39. Data race nondeterminism is qualitatively different from RNG

RNG variation may be intentionally sampled.

Race variation may reflect under-specified synchronization/undefined behavior.

The same umbrella word “nondeterminism” should not erase cause.

---

# 40. Stable scheduling order is not generally part of current Operation identity

Runtime commits Job-level orchestration order for execPlan, but not thread scheduling inside
an arbitrary target process.

Therefore:

```text
DeterministicStepOrder != DeterministicInternalExecution
```

---

# 41. execPlan ordering controls only step order

Current Runner deterministically selects:

```text
step1 -> step2 -> ...
```

subject to failure/continue conditions.

But each step can independently observe open time/RNG/filesystem/external state.

---

# 42. A prior step can intentionally create input for a later step

Example:

```text
step1 writes current timestamp to file
step2 reads file
```

The plan ordering is deterministic while result varies across plan realizations.

Thus:

```text
DeterministicControlFlow != DeterministicDataflowValues
```

---

# 43. Directory enumeration can expose filesystem/order variation

Programs that rely on unspecified directory iteration order can obtain different ordering
across filesystem/platform/runtime states.

Explicit sorting converts that ambient order into deterministic application semantics.

---

# 44. Filesystem metadata can be hidden input

Examples:

```text
mtime
inode number
permissions
ownership
extended attributes
filesystem allocation/order
```

A byte digest does not necessarily capture all metadata a target may inspect.

---

# 45. Path itself can affect output

Compilers/debug info/build IDs may embed absolute build paths.

Current stable Cargo target presentation emerged from a performance/cache concern, but it
also demonstrates that pathname is a semantically observable environment attribute.

---

# 46. Stable path does not imply stable bytes

Two executions can observe the same pathname while backing contents differ.

P4 already proved the stronger namespace-relative version of this principle.

---

# 47. Cache is both optimization and state

A cache can alter:

```text
performance
which computation is executed
failure exposure
occasionally outputs if cache correctness is broken
```

A correct content-addressed cache should preserve semantic result equivalence, but cache
presence itself is still an execution-environment state.

---

# 48. Cache hit/miss difference is not necessarily semantic nondeterminism

If both paths produce byte-identical valid outputs, the realization history differs while
result equivalence holds.

Therefore:

```text
DifferentExecutionTrace != DifferentResult
```

---

# 49. Conversely, shared mutable cache can leak nondeterminism

A broken/incompletely keyed cache can make result depend on prior Jobs.

RF13's persistence/coupling analysis therefore feeds directly into RF14.

---

# 50. Hermetic build systems try to make cache keys meaningful

Bazel's hermeticity model treats tools/dependencies as managed inputs and isolates actions
from host differences so that action identities/cache decisions can more safely correspond
to result identity.

This is a useful competing model for future structured build effects, not a mandate for
generic Runtime exec.

---

# 51. Generic Runtime execution is intentionally not a hermetic build system

`workspace.exec` must run arbitrary installed host executables and ordinary engineering
workloads.

Making all execution hermetic would greatly change compatibility/authority/dependency
semantics.

No current consumer evidence justifies that change.

---

# 52. Network responses are open-world inputs

Even if request URL/body are identical:

```text
service data changes
server version changes
rate limit changes
DNS changes
clock-based behavior changes
```

can change response.

Thus:

```text
SameNetworkRequest != SameNetworkResponse
```

---

# 53. External service identity/version can matter

For strong reproduction, one may need to bind:

```text
service endpoint
API version
model version
resource version
response artifact/receipt
```

But arbitrary open APIs rarely provide full deterministic replay semantics.

---

# 54. Recording a response can convert open input into frozen historical input

If a future reproduction consumes the exact recorded response bytes instead of calling live
service, one nondeterministic boundary has been replaced by immutable input.

This is **record/replay virtualization** of that dependency.

---

# 55. Recording everything is not automatically desirable

Full record/replay can incur:

```text
storage cost
secret/privacy risk
huge traces
kernel/platform coupling
complexity
semantic distortion
```

Runtime should not pursue universal deterministic replay without a concrete use case.

---

# 56. Agent/model action adds another nondeterminism layer

An Agent decision may depend on:

```text
model weights/version
prompt/context
sampling parameters
randomness
provider serving implementation
tool observations
external world state
```

Therefore “same Agent request” does not automatically imply same decision/action.

---

# 57. Agent nondeterminism is upstream of Runtime action identity

If an Agent chooses command A in one run and B in another, Runtime correctly sees two
different proposals/Operations.

Runtime should not collapse them merely because the higher-level user Goal was identical.

---

# 58. Runtime can make Agent action mechanically explicit without making Agent policy deterministic

Once proposal P is submitted, Runtime can bind its exact executable/args/env/steps/profile/etc.

This freezes **the delegated mechanical proposal**, not the reasoning process that selected
it.

---

# 59. Deterministic Agent policy would still face nondeterministic tools/world

Even a pure policy function over observations can receive different observations from:

```text
clock
finance market
network API
filesystem mutation
other actors
```

and therefore choose differently.

---

# 60. Nondeterministic Agent can still produce reproducible mechanical evidence

Runtime can durably record:

```text
proposal identity
Operation
Attempt
outputs
receipts
Artifacts
```

so a stochastic decision remains auditable even when it is not reproducible.

---

# 61. This suggests an important Runtime role

Runtime should often **capture nondeterministic realization evidence** rather than trying to
eliminate nondeterminism.

Example:

```text
actual response/result
actual provider receipt
actual timestamps
actual process identity
actual output digest
```

---

# 62. Determinism claim and evidence preservation are orthogonal

A nondeterministic execution can be extremely well evidenced.

A deterministic algorithm can be poorly evidenced.

Therefore:

```text
Determinism != Auditability
```

---

# 63. Reproducibility claim and semantic correctness are orthogonal

A bug can reproducibly generate the exact same wrong artifact.

Thus:

```text
Reproducible != Correct
```

---

# 64. Deterministic does not mean correct

Likewise:

```text
F(x) always returns wrong answer
```

is deterministic.

---

# 65. Nondeterministic does not mean unreliable

A cryptographic nonce generator should be nondeterministic/unpredictable by design.

A load balancer may legitimately choose among equivalent workers.

Randomized algorithms can be correct probabilistically.

---

# 66. Randomness can be a resource/requirement, not noise

RF14 therefore refuses a general objective of “eliminate all randomness.”

The goal is to **classify and control material nondeterminism relative to contract**.

---

# 67. Material nondeterminism

A nondeterministic dimension is material when variation can change a claim/output/effect the
consumer cares about.

Examples:

```text
build artifact bytes
financial order decision
model answer
security test result
```

---

# 68. Irrelevant nondeterminism need not be frozen

Scheduler timing can differ without changing final artifact bytes.

Capturing/fixing it may have zero consumer value.

Therefore:

```text
MoreFrozenState != AlwaysBetterSystem
```

---

# 69. Freezing has cost

Freezing more environment state can increase:

```text
materialization/storage
compatibility burden
provider complexity
staleness
loss of access to live services
performance cost
```

This mirrors RF7 closure tradeoffs.

---

# 70. Freeze only what the contract needs

General design principle:

```text
Freeze/Bind D when variation in D invalidates a required sameness/replay/reproduction claim.
```

Do not freeze merely because D exists.

---

# 71. Current source binding is therefore intentionally selective

It captures semantic source state while excluding ignored caches/build outputs for practical
engineering compatibility/performance.

RF14 sees this as a deliberate equivalence choice, not incomplete hashing by accident.

---

# 72. Current input materialization is stronger because consumer semantics demanded exact bytes

For `execBound`, the explicit foreign input contract justified the higher materialization and
read-only presentation cost.

This is exactly the “freeze when claim requires it” principle.

---

# 73. Provider commitment similarly freezes Runtime-owned realization machinery

Runner/launcher drift could materially change execution/recovery semantics, so Runtime binds
that provider identity.

It still does not freeze the entire OS.

---

# 74. Windows baseline environment freezing is another scoped choice

Windows admission records a bounded baseline environment and effective authority state.

This reduces WSL/ambient drift while deliberately not claiming full Windows machine snapshot.

---

# 75. CPU/hardware can matter

Floating-point behavior, vector instruction availability, concurrency timing and device
behavior may vary across hardware.

Strong cross-host reproducibility may require architecture/feature/toolchain constraints.

Generic Runtime currently does not bind complete CPU semantics.

---

# 76. Kernel version/semantics can matter

System-call behavior, scheduling, filesystems, namespace details and device drivers can vary
across kernels.

Current provider commitment is not a full kernel identity/reproducibility contract.

---

# 77. Container/VM can freeze more environment dimensions but not automatically determinism

A VM image can stabilize OS/toolchain state while target time/RNG/network/concurrency remain
variable.

So RF13 isolation strengthening does not solve RF14 by itself.

---

# 78. Hermeticity is closure, not necessarily snapshotting

A hermetic action can consume explicitly declared dynamic inputs if the contract identifies
them.

What matters is absence of undeclared ambient influence, not that every byte is old/static.

---

# 79. Dynamic but versioned services can be explicit dependencies

An external service can participate in a reproducibility contract if it provides a stable
version/snapshot/query semantics.

Example conceptual form:

```text
DatasetService(snapshotId=123)
```

is stronger than:

```text
GET /latest
```

---

# 80. “Latest” is inherently time-relative

Any input specified as:

```text
latest
current
today
now
```

contains an implicit time/world dependency unless resolved to a durable identity at
admission.

---

# 81. Resolution versus use-time matters

Resolving a dependency at admission does not ensure the same object is consumed later unless
identity/continuity/materialization guarantees bridge the interval.

RF7/P4 already proved this for paths.

---

# 82. Reproduction requires knowing which historical world to reproduce

To reproduce a prior execution, a system may need:

```text
source/input identities
exact tool versions
platform identity
explicit env
external response snapshots
seed/random trace
scheduling/event trace for races
```

The required set depends on the program and output equivalence claim.

---

# 83. No universal finite dependency list is guaranteed for arbitrary programs

A Turing-complete target can dynamically choose dependencies based on prior observations.

Therefore generic Runtime cannot cheaply precompute complete causal closure for every
execution.

---

# 84. Restriction can engineer closure

As RF7 noted, instead of predicting every dependency, a sandbox/hermetic executor may expose
only declared resources.

Then undeclared access fails.

This is one path to stronger reproducibility, at compatibility cost.

---

# 85. Observation can discover dependencies but not prove universal closure

One syscall/file/network trace shows what one run used.

Another branch/input/schedule may use different dependencies.

Thus:

```text
ObservedDependenciesOneRun != CompletePossibleDependencySet
```

---

# 86. Deterministic replay often records nondeterministic events instead of eliminating them

This is a crucial competing strategy:

```text
record actual nondeterministic choices/events
then force/replay those choices later
```

instead of trying to make underlying world deterministic.

---

# 87. This strategy changes semantics and cost

Replay environment must recreate enough low-level state/order that recorded events remain
valid.

For arbitrary external Effects, replaying calls can also be unsafe unless virtualization
intercepts them.

---

# 88. Runtime exact replay wisely avoids this problem

Current replay returns the historical Job rather than re-triggering physical effects.

This makes idempotent request continuity strong without pretending physical deterministic
replay exists.

---

# 89. Re-run must require a new semantic decision/identity

If a user/Host wants “run the same command again”, that is a new physical action and can
produce new Effects.

It should not be smuggled through exact replay semantics.

---

# 90. Same source + same command can legitimately create a new Operation

Because physical time/world has changed, re-execution is a new historical event even if the
proposal fingerprint is equivalent.

RF4 identity and RF14 determinism align here.

---

# 91. Result equality does not merge histories

Two independent builds may produce identical SHA-256 outputs.

They remain distinct execution histories.

Therefore:

```text
SameResult != SameExecution
```

---

# 92. Different result does not necessarily invalidate request identity

If a contract permits stochastic/open-world outcomes, one logical action type can have many
valid results.

Request/Operation identity is about historical continuity, not a pure-function key.

---

# 93. This is why content-addressing cannot replace Operation identity

If Operation ID were derived from result bytes, identity would not exist before execution and
nondeterministic valid outcomes would fragment incorrectly.

RF4 already rejected this; RF14 provides another reason.

---

# 94. Deterministic structured Effect adapters can be stronger than opaque exec

A structured adapter may resolve:

```text
stable effect identity
versioned inputs
provider snapshot
canonical receipt
```

and thus expose tighter replay/reconciliation semantics.

Still, external provider behavior may remain nondeterministic where contract permits.

---

# 95. Runtime Release has some deterministic identity without deterministic deployment world

Release effect identity can be deterministically derived/bound.

Actual deploy/probe outcome can still vary because host/service state may differ.

Thus:

```text
DeterministicEffectIdentity != DeterministicEffectOutcome
```

---

# 96. Workspace Patch has stronger deterministic planning

Given same before-state and patch specification, expected after-digests can be determined.

Physical mutation may still encounter races/IO failures and reconcile differently.

So:

```text
DeterministicPlan != DeterministicRealization
```

---

# 97. Failure itself can be nondeterministic

Timeout, disk-full, network reset, competing process, transient provider error and race
conditions can vary across otherwise similar executions.

Therefore reproducibility must include failure outcomes if the consumer cares about them.

---

# 98. Timeouts introduce schedule/load dependence

A program that normally finishes near its deadline may succeed under one CPU load and time
out under another.

Same input bytes do not guarantee same terminal disposition.

---

# 99. Resource budgets can reduce but also expose nondeterminism

Fixed CPU/memory limits make policy more stable, but scheduling contention within those
limits can still alter timing.

Budgets are constraints, not deterministic execution schedules.

---

# 100. Output capture concurrency can affect interleaving

stdout/stderr from concurrent target threads/processes can be ordered according to actual
write/interleaving timing.

Runtime faithfully capturing output does not make application output ordering deterministic.

---

# 101. Truncation can alter observed result equivalence

Two executions may produce different full outputs but identical retained prefixes because
both exceed limits.

Therefore output equivalence claims must include completeness/truncation metadata from RF11.

---

# 102. Artifact digest gives content identity, not reproduction explanation

If two Artifacts match by digest, we know bytes match under hash assumptions.

We do not thereby know which controlled inputs caused reproducibility.

---

# 103. Proven reproducibility needs independent realization evidence

For a build claim, stronger evidence is:

```text
independent rebuild under declared environment
-> same artifact digest
```

rather than simply copying the original artifact and hashing it again.

---

# 104. Reproducibility can strengthen supply-chain confidence

Independent identical rebuilds can provide evidence that artifact corresponds to declared
source/build environment rather than only one potentially compromised builder.

This is one reason reproducible builds are valuable.

---

# 105. But reproducibility does not prove source is benign

Again:

```text
ReproducibleMalware is still malware.
```

So provenance/reproducibility/security correctness remain separate axes.

---

# 106. Runtime is not currently a reproduction-verification authority

It can execute builds and preserve Artifacts/evidence.

Host/domain tooling can compare independent outputs and decide whether a reproducibility claim
is satisfied.

---

# 107. Host can own reproduction experiments

Example:

```text
Host creates Workspace A/B
Runtime executes same declared build under chosen environments
Host compares Artifact digests
```

This does not require a generic Runtime “reproducible=true” primitive.

---

# 108. Agent action makes experiment design important

An Agent can intentionally vary one factor:

```text
toolchain version
cache state
CPU target
network access
seed
```

and use Runtime evidence to identify which dimensions materially affect outputs.

---

# 109. This is falsification rather than assumption

Instead of declaring an action deterministic, test:

```text
repeated execution under controlled perturbations
```

and observe output/effect invariance.

---

# 110. Statistical nondeterminism requires distributions, not one result

For randomized/ML workloads, the relevant object may be:

```text
output distribution
failure probability
performance distribution
```

rather than one expected byte-identical result.

Runtime does not need to own the statistical semantic evaluation.

---

# 111. Agent/model outputs often need semantic equivalence, not byte equivalence

Two valid model answers can differ in wording while satisfying the same Goal.

Thus reproducibility of Agent behavior must name its equivalence relation.

---

# 112. Seeded model generation may still not be fully reproducible

Even with a nominal seed, differences in model/provider implementation, batching, hardware or
serving revision can change behavior unless the provider contract explicitly guarantees
otherwise.

RF14 records this concept without relying on any particular vendor guarantee.

---

# 113. Model version is a dependency

If an Agent's decision materially depends on model behavior, exact model/revision/provider
semantics belong to the higher-level reproducibility contract.

Runtime does not own model identity unless a concrete tool/provider surfaces it as input.

---

# 114. Tool observations are also Agent inputs

Even a fixed model/prompt can receive different:

```text
web search
market price
filesystem state
Runtime observation
```

and legitimately act differently.

---

# 115. Deterministic Agent action can be undesirable in open-world control

A controller should respond to changed Reality.

Forcing identical action despite changed observations would be incorrect.

Therefore:

```text
BehavioralSameness != AlwaysDesiredCorrectness
```

---

# 116. Reproducibility and adaptivity trade off by design

Open-world Agents need to incorporate fresh state.

Research/reproduction workflows need to freeze or snapshot enough observations to rerun prior
reasoning.

These are different modes.

---

# 117. Historical observation capture can enable reasoning reproduction

A higher layer can persist:

```text
prompt/context
model identity/config
exact tool observations
```

then rerun reasoning against that frozen transcript if desired.

This belongs primarily to Harness/Host, not generic Runtime.

---

# 118. Runtime's role is lower-level realization evidence

Runtime should preserve:

```text
what mechanical proposal was committed
what exact provider/executable/input identities were bound
what Attempt actually happened
what outputs/evidence resulted
```

and not claim that the Agent's cognitive policy was deterministic.

---

# 119. Determinism budget

RF14 introduces a useful design concept: **Determinism Budget**.

Every consumer can decide how much variation must be removed/captured to satisfy its claim.

Examples:

```text
unit test: fixed inputs/env/seed may be enough
release build: toolchain/deps/timestamps/path must be tighter
financial execution: live market is intentionally open
Agent exploration: stochasticity may be useful
```

---

# 120. Strongest boundary is consumer-relative

There is no universal Runtime optimum of “maximum determinism.”

The correct amount is:

```text
minimum closure/control needed for the required outcome/evidence contract
```

while preserving needed adaptivity/compatibility.

---

# 121. Determinism can be engineered by substitution

Open dependency:

```text
current time
```

can become explicit:

```text
TIME=1730000000
```

Live API can become:

```text
recorded response artifact
```

RNG can become:

```text
explicit seed/trace
```

This transforms ambient state into declared input.

---

# 122. Virtualization can provide controlled nondeterministic interfaces

A sandbox/replay system may virtualize:

```text
clock
RNG
network
filesystem metadata
```

so target sees controlled values.

Current Runtime deliberately does not provide this generic layer.

---

# 123. Deterministic interfaces can still wrap open Reality

A provider might expose a snapshot query:

```text
market snapshot at timestamp/version X
```

which is reproducible historically even though the underlying market was dynamic.

This reinforces fact-owner/versioned-state design from RF11.

---

# 124. Nondeterminism provenance matters

When results differ, useful diagnosis asks:

```text
which uncontrolled dimension changed?
```

Potential provenance classes:

```text
input drift
tool/provider drift
environment drift
clock/randomness
scheduler/race
cache/persistence
external service/world
hardware/kernel
Agent/model policy
```

---

# 125. Runtime evidence already closes some diagnosis classes

Current source/provider/executable/input identities can rule out or identify several forms of
drift.

This is valuable even without full determinism.

---

# 126. Missing evidence should remain unknown, not “nondeterministic”

If we do not know whether toolchain library changed, we should not conclude randomness caused
the difference.

RF11 evidence discipline still governs RF14 diagnosis.

---

# 127. Nondeterministic does not mean unexplained

A process can intentionally record:

```text
seed=42
sampled choice=7
provider response digest=D
```

making the realized path explainable even if alternatives were possible.

---

# 128. Explainability is distinct again

```text
Deterministic != Explainable
Nondeterministic != Unexplainable
```

---

# 129. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| same source + `date` | source identity = result identity | wall clock is ambient input |
| same source + `getrandom()` | isolation/input freeze = determinism | kernel RNG remains variable |
| same seed + thread race | seed = deterministic execution | scheduling/interleaving remains open |
| same request exact replay | replay = rerun | current Runtime returns historical Job |
| re-run same command tomorrow | same proposal = same historical action | re-execution is new event/world |
| ignored cache changes | source digest = world digest | ignored state may still influence target |
| same executable digest + changed shared library | executable identity = toolchain closure | dynamic dependencies remain |
| immutable inputs + live clock | frozen inputs = hermetic | environment remains open |
| contained_local no network + RNG | isolation = determinism | randomness/time remain |
| same network request tomorrow | request sameness = response sameness | service/world changes |
| fixed VM image + live API | VM snapshot = reproducibility | external open dependency remains |
| Nix derivation with timestamp leak | sandbox/dependency closure = reproducible | residual nondeterminism survives |
| Bazel action embeds timestamp | hermetic intent = deterministic bytes | time must be controlled |
| execPlan timestamp step1→step2 | fixed step order = fixed values | deterministic control flow, variable data |
| equal artifact digests from two builds | equal result = same execution | histories remain distinct |
| release effect ID stable, deploy varies | deterministic identity = deterministic outcome | host state affects realization |
| Patch expected after-state fixed, I/O race | deterministic plan = deterministic realization | physical failure can vary |
| timeout near deadline | same bytes = same terminal state | scheduling/load affects outcome |
| stochastic Agent, same Goal | same Goal = same proposal | policy sampling/context can differ |
| deterministic Agent, fresh market observation | deterministic policy = same action | changed input yields changed action |

---

# 130. RF14 anti-laws

RF14-L1: `SameOperationIdentity != SameResult`.

RF14-L2: `TwoEqualRuns != ProvenDeterminism`.

RF14-L3: `Repeatability != Determinism`.

RF14-L4: `Reproducibility != Determinism`.

RF14-L5: `Hermetic != Deterministic`.

RF14-L6: `Deterministic != Hermetic`.

RF14-L7: `DependencyClosure + Sandbox != AutomaticReproducibility`.

RF14-L8: `ExactReplay != ReExecution`.

RF14-L9: `OperationIdentity != WorldStateIdentity`.

RF14-L10: `SourceIdentity != CompleteExecutionStateIdentity`.

RF14-L11: `IgnoredBySourceDigest != CausallyIrrelevant`.

RF14-L12: `ExecutableDigest != CompleteToolchainIdentity`.

RF14-L13: `TimestampEvidence != DeterministicTime`.

RF14-L14: `Contained != Deterministic`.

RF14-L15: `Seed != DeterministicExecution`.

RF14-L16: `DeterministicStepOrder != DeterministicInternalExecution`.

RF14-L17: `DeterministicControlFlow != DeterministicDataflowValues`.

RF14-L18: `DifferentExecutionTrace != DifferentResult`.

RF14-L19: `SameNetworkRequest != SameNetworkResponse`.

RF14-L20: `Determinism != Auditability`.

RF14-L21: `Reproducible != Correct`.

RF14-L22: `Nondeterministic != Unreliable`.

RF14-L23: `MoreFrozenState != AlwaysBetterSystem`.

RF14-L24: `SameResult != SameExecution`.

RF14-L25: `DeterministicEffectIdentity != DeterministicEffectOutcome`.

RF14-L26: `DeterministicPlan != DeterministicRealization`.

RF14-L27: `BehavioralSameness != AlwaysDesiredCorrectness`.

RF14-L28: `ObservedDependenciesOneRun != CompletePossibleDependencySet`.

RF14-L29: `Deterministic != Explainable`.

RF14-L30: `Nondeterministic != Unexplainable`.

---

# 131. Minimum RF14 grammar

## 131.1 Deterministic mapping

```text
F_C(S,I) -> one permitted outcome under contract C
```

## 131.2 Nondeterministic mapping

```text
F_C(S,I) -> {O1, O2, ...}
```

more than one permitted realization/outcome.

## 131.3 Repeatability `Repeat_C(E)`

Repeated trials under nominally same conditions produce equivalent result.

## 131.4 Reproducibility `Reproduce_C(E,~Q)`

Independent realization under declared source/environment/instruction contract produces
output equivalent under relation `~Q`.

## 131.5 Hermeticity `Hermetic_C(E)`

Execution influence is restricted to declared/managed dependency channels in C.

## 131.6 Request Replay

Re-observe/retrieve same historical committed Operation by request continuity identity.

## 131.7 Re-execution

Create a new physical realization/historical action from equivalent/similar proposal.

## 131.8 Result Reproduction

New realization intended to match prior output under an equivalence contract.

## 131.9 Nondeterministic input

Any variable/event/choice not fixed by the modeled state/input boundary that can alter a
material outcome.

## 131.10 Determinism Budget

Consumer-specific set of state dimensions that must be fixed/recorded/virtualized to satisfy
the required sameness/reproduction claim.

---

# 132. Current Runtime determinism map

| Current mechanism | RF14 interpretation |
|---|---|
| `(principal, clientRequestId)` + semantic fingerprint | request continuity, not result key |
| exact replay | return same durable Job/history, no re-execution |
| sourceStateDigest | scoped source identity; excludes ignored caches/build outputs |
| executable digest | exact primary executable bytes, not full toolchain closure |
| provider commitment | Runtime-owned Runner/launcher identity, not OS/world snapshot |
| explicit env | bound process configuration dimension |
| fixed locale/temp/cache projections | reduced ambient variation |
| Host Dependencies | selected explicit ambient dependency identities/witnesses |
| immutable inputs | strong exact input-byte closure for named inputs |
| trusted-local cache | persistent/shared performance state outside source identity |
| contained-local cache | narrower Workspace-scoped state, still persistent |
| wall clock | open target-observable input |
| kernel RNG | open nondeterministic input |
| internal thread scheduling | open target-internal nondeterminism |
| filesystem metadata/order | partly open depending on workload |
| kernel/CPU/toolchain transitive state | not generally fully bound |
| arbitrary network/external API | open-world nondeterministic dependency |
| execPlan | deterministic step order/control proposal; step behavior remains open |
| Runtime timestamps | realization evidence, not time virtualization |
| Runtime Release effect identity | deterministic structured identity, outcome may vary |
| Workspace Patch plan | strongly determined expected state transition, realization may fail/race |
| Agent/model policy | outside Runtime determinism authority |

---

# 133. What RF14 strongly supports in current design

## 133.1 Exact replay must remain historical/idempotent rather than rerun

This is essential for effect safety and keeps request continuity independent from physical
reproducibility.

## 133.2 Source identity should remain selective

Ignoring build caches/output state is a deliberate source equivalence relation. Do not bloat
it into a false world-state digest.

## 133.3 Freeze explicit high-value dependencies where contracts require sameness

Immutable inputs, provider identity, executable identity and bounded environment are good
examples.

## 133.4 Preserve nondeterministic realization evidence

Output digests, timestamps, receipts and identity-bound results improve audit/recovery even
when deterministic reproduction is impossible.

## 133.5 Keep generic exec non-hermetic

A universal hermetic executor would sacrifice compatibility and duplicate specialized build/
sandbox systems without current consumer evidence.

---

# 134. What RF14 reopens

## 134.1 Structured build/reproduction adapters may eventually justify stronger closure

If repeated consumers require reproducible release builds, a dedicated structured build
Effect could bind toolchain/dependencies/time/path semantics more strongly than generic exec.

No implementation is authorized here.

## 134.2 Determinism metadata could become useful only if tied to real claims

A future adapter might declare/capture seed, snapshot ID or deterministic output contract.

Avoid generic `deterministic=true` flags.

## 134.3 Reproduction experiments belong naturally above Runtime today

Host can orchestrate independent runs and compare Artifacts while Runtime provides exact
mechanical evidence.

## 134.4 External response capture can be a targeted reproducibility technique

Only where privacy/storage/effect semantics justify converting live responses into frozen
inputs.

---

# 135. Research constitution added by RF14

1. Never infer same result from same Operation identity.
2. Define determinism only relative to a declared modeled state/input/semantic boundary.
3. Keep determinism, repeatability, reproducibility, replay and hermeticity distinct.
4. Treat output equivalence as purpose-relative rather than always byte equality.
5. Preserve exact replay as same historical Job/Operation retrieval, not re-execution.
6. Treat re-execution as a new historical action that may observe a changed world.
7. Keep sourceStateDigest scoped to source semantics; ignored state may still be causally
   relevant to target behavior.
8. Do not treat executable/provider digests as complete toolchain/OS closure.
9. Treat wall clock and kernel RNG as open inputs unless explicitly virtualized/seeded.
10. Treat a seed as one state dimension, not proof of deterministic execution.
11. Keep internal scheduling/race behavior separate from RNG variation.
12. Treat execPlan order as deterministic control structure only; constituent steps remain
    ordinary open executions.
13. Treat caches/persistence as execution-world state and explicit coupling dimensions.
14. Keep network/external-service responses open unless a version/snapshot/recording contract
    closes them.
15. Distinguish hermetic dependency closure from deterministic computation.
16. Do not expect sandboxing/VM isolation by itself to produce deterministic results.
17. Preserve nondeterministic evidence rather than attempting universal elimination.
18. Do not equate reproducibility with correctness, safety or semantic success.
19. Do not equate nondeterminism with unreliability or error.
20. Freeze only dependencies whose variation invalidates a required consumer claim.
21. Prefer targeted substitution/recording/versioning over universal world snapshots.
22. Treat Agent/model policy nondeterminism as higher-layer behavior unless Runtime executes a
    concrete provider adapter with explicit semantics.
23. Let Host orchestrate current reproduction/falsification experiments using multiple Runtime
    Operations.
24. Avoid generic deterministic/sandbox/reproducible scalar flags without a contract.
25. Governing law: `DeterminismClaimScope <= ControlledStateBoundaryScope`.
26. Do not enter RF15 until open-world dependency/effect semantics can be studied without
    assuming replay/reproduction of external Reality.

---

# 136. RF14 result — conceptual compression

```text
Operation identity says which historical logical action this is.
Replay returns that same history.
Re-execution creates another physical event.
Determinism says a declared complete state/input admits one outcome.
Repeatability says repeated trials happened to/effectively match under a setup.
Reproducibility says independent realization recreates equivalent specified outputs under a
declared source/environment/instruction contract.
Hermeticity says undeclared ambient dependencies are excluded/managed.
None of these concepts imply the others automatically.
```

Strongest Runtime-specific compression:

```text
Runtime freezes:
  request/proposal semantics
  selected source state
  explicit env/limits/profile/target
  primary executable/provider identity
  named immutable inputs/dependencies where contracted

Runtime does NOT generally freeze:
  wall clock
  kernel RNG
  internal scheduling
  ignored caches
  complete transitive toolchain
  kernel/CPU semantics
  arbitrary filesystem metadata
  external services/world state
  Agent/model policy

Exact replay:
  same durable Job/history
  NOT rerun
```

The central theorem is:

```text
DeterminismClaimScope <= ControlledStateBoundaryScope
```

and the current generic Runtime wisely makes very few determinism claims.

---

# 137. RF14 closeout

RF14 closes with thirty durable results:

1. **Same Operation identity does not imply same result or same physical realization.**
2. Determinism is always relative to a modeled state/input/semantic boundary.
3. Repeatability, reproducibility, deterministic mapping, request replay, re-execution and
   result reproduction are distinct.
4. Reproducibility requires an explicit input/environment/instruction contract and an output
   equivalence relation.
5. Hermeticity is dependency closure/isolation from undeclared ambient influence; it is
   neither necessary nor sufficient by itself for deterministic computation.
6. Reproducible-build and Nix/Bazel models demonstrate that tool/dependency closure greatly
   improves reproduction while timestamps and other nondeterminism can still survive.
7. **Current exact replay returns the original durable Job/history and deliberately does not
   re-execute.**
8. Physical re-execution is a new historical action and may legitimately observe a different
   world.
9. Current Operation identity binds important explicit proposal, source, environment,
   executable/provider and optional input semantics but is not a complete world-state key.
10. `sourceStateDigest` deliberately excludes ignored caches/build outputs; source identity
    is not execution-state identity.
11. Ignored state can still be causally relevant if target code chooses to read it.
12. Executable/provider binding closes selected realization machinery but not transitive
    toolchain, kernel, CPU or all dynamic dependencies.
13. Host Dependencies and immutable inputs provide increasingly strong closure for named
    dependency classes without becoming universal environment closure.
14. Environment filtering/fixing reduces ambient variation but process env is only one
    execution-state dimension.
15. Wall clock remains an open target-observable input; Runtime timestamps are evidence and
    monotonic timeout mechanics, not clock virtualization.
16. Kernel RNG provides deliberate nondeterministic input; containment/isolation does not
    imply deterministic randomness.
17. A fixed PRNG seed controls one randomness dimension but not scheduling, races, platform or
    other randomness sources.
18. Internal concurrency/scheduling can make identical explicit inputs realize different
    histories/results.
19. `execPlan` fixes step/control order but does not make step internals or produced values
    deterministic.
20. Filesystem enumeration, metadata, pathnames and persistent/shared caches can all be
    material hidden state depending on workload.
21. Network and arbitrary external-service responses are open-world inputs unless a version,
    snapshot or record/replay contract closes them.
22. Recording an external response can convert one live dependency into immutable historical
    input, but universal deterministic replay would carry major cost and effect-safety risks.
23. Agent/model decisions add a higher-layer nondeterminism source; Runtime freezes the
    submitted mechanical proposal rather than the reasoning policy that selected it.
24. Deterministic policy can still adapt to changed observations, while stochastic policy can
    still produce auditable identity-bound Runtime evidence.
25. **Runtime should often preserve nondeterministic realization evidence instead of trying
    to eliminate all nondeterminism.**
26. Determinism/reproducibility are orthogonal to correctness, semantic success and
    auditability.
27. Nondeterminism can be intentional and valuable; materiality to the consumer contract is
    what matters.
28. Freezing/virtualizing more state has compatibility, freshness, storage and complexity
    costs; use a consumer-specific determinism budget.
29. Current Host+Runtime split already supports reproduction/falsification experiments:
    Host orchestrates independent executions and semantic comparison, Runtime supplies exact
    mechanical evidence.
30. **The governing RF14 law is `DeterminismClaimScope <= ControlledStateBoundaryScope`.**

The next frontier is:

> **Runtime now understands that many dependencies remain deliberately open. What exactly is
> an external/open-system Effect, where does Runtime authority stop, how can actions interact
> with a changing Reality, and what evidence/reconciliation is possible when the world is not
> transactionally owned or reproducible?**

That is RF15 — External World and Open-System Effects.
