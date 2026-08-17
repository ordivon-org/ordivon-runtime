---
schema_version: 1
id: runtime.foundations.rf7
title: RF7 — Boundary, Environment, Context and Dependency
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
summary: RF7 separates boundary, isolation, environment, context, input, dependency and closure. It models boundaries as dimension-specific contracts rather than one shell; classifies dependencies by relevance and handling strategy; distinguishes frozen/materialized/bound/witnessed/virtualized/queried/open context; and reclassifies current immutable inputs, Host Dependencies, execution-provider snapshots, environment construction and contained_local as partial environment contracts rather than complete execution closure.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF7
related:
  - runtime.foundations.start
  - runtime.foundations.rf6
  - runtime.foundations.rf7.sources
  - runtime.foundations.rf7.continuation
---
# RF7 — Boundary, Environment, Context and Dependency

## 0. Status and research question

RF6 separated authority and control. RF7 now asks:

> **Where does an Operation actually live? Which parts of Reality form its execution
> environment, which are merely contextual observations, which facts are dependencies,
> which boundaries constrain them, and how much of that dependency world can Runtime
> truthfully close?**

RF7 is explanatory research only. It does not authorize container/VM adoption, automatic
dependency discovery, dynamic-library closure, network capture, new namespace isolation,
new environment fields, or Host Dependency behavior changes.

---

# 1. First deletion — Boundary is not one perimeter

The phrase:

```text
inside the Runtime boundary
```

is underspecified.

A system can have distinct boundaries for:

```text
ownership
address space
process identity
PID visibility
filesystem/mount namespace
network namespace
authority
resource accounting
failure containment
information flow
persistence
observation
provider responsibility
transaction commit
```

These boundaries need not coincide.

Therefore:

```text
Boundary != one universal shell
```

---

# 2. Boundary

RF7 defines a **Boundary** as:

> **A declared partition/interface across which some class of entities, rights,
> observations, resources, failures or state transitions obey different ownership,
> visibility or interaction rules.**

A boundary must name:

```text
what dimension is partitioned
what is on each side
what may cross
how crossing occurs
what evidence/enforcement establishes it
```

---

# 3. Isolation

**Isolation** is not the existence of a boundary alone.

RF7 defines isolation as:

> **A property/contract limiting influence, visibility, authority, resource interference,
> failure propagation or information flow across a named boundary.**

Thus isolation is dimension-relative:

```text
namespace isolation
authority isolation
resource isolation
failure isolation
network isolation
storage isolation
information-flow isolation
timing/scheduling isolation
```

Therefore:

```text
Isolated(X,Y) must name “isolated with respect to what?”
```

---

# 4. Process boundary is not authority boundary

Two processes have distinct address spaces and process identities.

Yet one process may still possess authority to:

```text
signal the other
ptrace it
read shared files
access same network services
read same secrets
```

Therefore:

```text
ProcessBoundary != AuthorityIsolation
```

---

# 5. Namespace boundary is not complete isolation

Linux namespaces isolate specific global-resource views. OCI exposes separate namespace
kinds including:

```text
pid
network
mount
ipc
uts
user
cgroup
time
```

Omitting a namespace means the container may inherit the Runtime's namespace for that
resource class.

Therefore:

```text
NamespaceIsolation is a vector, not a boolean.
```

---

# 6. Mount namespace gives a view boundary

Linux mount namespaces provide distinct mount hierarchies/views.

Thus the pathname:

```text
/opt/x/libfoo.so
```

is not a globally unique object reference independent of namespace.

A target in namespace N1 and an observer in N2 can resolve the same textual pathname to
different mount/object graphs.

Therefore:

```text
SamePathString != SameResolvedObject
```

unless namespace/path-resolution context is fixed.

---

# 7. Current P4 is a decisive local proof

Ordivon already physically demonstrated:

```text
Runtime host namespace path P → bytes V1
```

while a trusted-local target created a private mount namespace and bind-mounted V2 over
the same textual P in its own view.

The host path stayed V1 and no host inotify path event occurred; target consumed V2.

Therefore:

```text
HostPathWitness != TargetNamespaceViewIntegrity
```

This is one of RF7's strongest falsifiers.

---

# 8. Path is a reference interpreted in context

More generally:

```text
Resolve(path, namespace, cwd, root, symlink graph, mount graph, credentials, time)
→ object/result
```

The string `path` alone is not the resolved object.

Thus:

```text
PathIdentity != ObjectIdentity
```

and:

```text
PathCommitment != universal byte-consumption proof
```

---

# 9. Open file descriptor is a different dependency relation than pathname lookup

A file descriptor refers to an already-open file description/object under kernel rules.
A pathname is re-resolved when used.

This is why:

```text
open-then-use
```

can have stronger object continuity than:

```text
validate-path-now, reopen-path-later
```

but it can also alter target-visible semantics and transfer authority.

Therefore:

```text
ReferenceMechanism changes dependency semantics.
```

---

# 10. `execve` demonstrates inherited context channels

POSIX/Linux execution does not begin from only executable bytes and argv.

Depending on the call/flags, a new process image can inherit or receive:

```text
environment
open file descriptors without FD_CLOEXEC
process credentials/security context
signal/process attributes
working/root/filesystem namespace context
```

Therefore:

```text
ExecutableBytes + Args != CompleteExecutionEnvironment
```

---

# 11. Environment

RF7 defines **Environment** as:

> **The set of external-to-target-state conditions, structures and services whose current
> values/semantics are available to and can materially constrain or influence realization
> during a declared execution interval.**

This can include:

```text
process environment variables
filesystem tree and namespaces
working directory
executable/interpreter/runtime
shared libraries/modules
kernel ABI/semantics
CPU architecture/features
GPU/device/driver state
clock/randomness
locale/timezone
network topology/DNS/services
credentials/authority
external databases/providers
resource availability/scheduling
```

---

# 12. Environment is not State

RF1 defined State as a modeled projection of Reality for some observer/purpose.

Environment is a **role** facts play relative to an execution.

The same fact can be:

```text
State for observer A
Environment for execution B
Effect for operation C
```

Example:

```text
GPU driver version
```

is observed machine State, while simultaneously an Environment dependency of a CUDA
execution.

Thus:

```text
Environment != State
```

---

# 13. Context

RF7 uses **Context** even more broadly:

> **Facts that frame interpretation, observation or decision for an entity/operation,
> whether or not they are causally/materially consumed during realization.**

Examples:

```text
caller identity
Task/Goal
current branch
human explanation
trace correlation ID
location/time metadata
policy version
external market regime
```

Context may matter semantically without being a physical execution dependency.

Therefore:

```text
Context != ExecutionEnvironment
```

---

# 14. Input

An **Input** is something intentionally supplied/selected as an operand to the operation or
execution contract.

Examples:

```text
argv
stdin
explicit file/object binding
request payload
model prompt
```

But not every dependency is an explicit input.

Thus:

```text
Input ⊂ possible Dependencies
```

---

# 15. Dependency

RF7 defines a **Dependency** relative to a claim/outcome:

> **A fact/entity/service D is a dependency of realization/result/effect claim Q when
> changing/removing D within the modeled counterfactual space can change whether Q occurs,
> its behavior/result, or the validity of the claim.**

Compactly:

```text
D ∈ Dep(Q)
```

This is claim-relative.

A dependency for byte-identical output may be different from a dependency for security,
performance or external Effect success.

---

# 16. Dependencies have roles

RF7 distinguishes:

```text
Explicit Input Dependency
Declared Physical Dependency
Transitive Runtime Dependency
Ambient Dependency
Authority Dependency
Service/Network Dependency
Hardware/Kernel Dependency
Temporal/Random Dependency
Observation/Recovery Dependency
```

No one representation fits all.

---

# 17. Explicit versus implicit dependency

**Explicit dependency** is named in the operation/execution contract.

**Implicit dependency** affects realization but is not named there.

Examples:

```text
argv file      explicit
libc loaded by ELF loader   often implicit
DNS resolver configuration implicit
GPU driver     often implicit
wall clock     implicit unless deliberately controlled
```

Thus:

```text
Undeclared != Nonexistent
```

---

# 18. Transitive dependency

If:

```text
A depends on B
B depends on C
```

then C can matter to A's realization even if A never directly names C.

Dynamic package/module graphs are common examples.

Thus dependency closure is not necessarily equal to direct dependency list.

---

# 19. Dynamic linker is a decisive dependency-closure falsifier

Linux dynamic linker/loader resolves shared objects needed by a program using ELF
metadata plus search paths/environment/cache/runtime rules.

Changing a shared library can change behavior while the main executable's bytes remain
identical.

Current Ordivon P2 physically demonstrated exactly this:

```text
same target executable digest
shared library V1 → ENV_V1
shared library V2 → ENV_V2
```

Therefore:

```text
ExecutableDigest != ExecutionClosureDigest
```

---

# 20. Dynamic loading defeats purely static direct-dependency enumeration

Programs can load modules/libraries after startup:

```text
dlopen
Python import
Node module resolution
plugins
JIT/compiler toolchains
```

and may derive names from data/configuration.

Therefore a complete dependency graph cannot always be inferred from executable metadata
before execution.

---

# 21. HostDependency is deliberately a declared dependency, not closure discovery

Current Ordivon `HostDependencyBinding` explicitly documents that Runtime does not infer
undeclared dynamic dependencies or claim complete environment closure.

RF7 classifies it as:

```text
Agent/domain-declared physical filesystem prerequisite
+
identity-bound expected digest
+
Runtime-host-namespace continuity witness
```

This is a partial dependency contract.

---

# 22. Freeze, Materialize, Bind, Witness, Virtualize, Query, Leave Open

RF7 introduces seven major dependency-handling strategies.

## 22.1 Freeze

Fix a fact/value as part of the Operation commitment.

Example:

```text
Execution Plan field
sourceStateDigest
provider snapshot identity
```

## 22.2 Materialize

Create/own concrete bytes/state corresponding to the committed dependency.

Example:

```text
immutable input materialization
```

## 22.3 Bind

Commit an identity/version/digest/reference to a dependency without necessarily owning its
physical realization.

Example:

```text
HostDependency path+digest
ForeignReference generation/digest
```

## 22.4 Witness

Observe continuity/invariants across time without making the dependency immutable.

Example:

```text
runtime_host_namespace_path_witness
```

## 22.5 Virtualize

Interpose/provide a controlled substitute/environment layer.

Example:

```text
namespace
VM device
virtual clock/network
```

## 22.6 Query/Reconcile

Ask an external owner for current/historical state.

Example:

```text
provider receipt/status query
```

## 22.7 Leave Open

Explicitly acknowledge that the dependency remains ambient/open and outside the claimed
closure.

Example:

```text
arbitrary network response
wall clock
undeclared kernel behavior
```

---

# 23. These strategies have different strength

A witness is not a freeze.

A bind is not a materialization.

A materialization of bytes is not virtualization of kernel semantics.

A query is not local ownership.

Therefore:

```text
Binding != Ownership
Witness != Immutability
MaterializedBytes != CompleteEnvironmentClosure
```

---

# 24. Immutable inputs are one of current Runtime's strongest closed subsets

Current input-bound execution performs:

```text
operator authority resolution
→ exact object digest verification
→ Job-scoped copy/materialization
→ committed file inventory/closure
→ read-only presentation
→ target receives presentation root
```

This gives a much stronger dependency contract for those exact bytes than ordinary host
paths.

RF7 classifies them as:

```text
Explicit Input
+ Frozen Identity
+ Runtime-owned Materialization
+ Bounded Presentation
```

---

# 25. But immutable input closure is local to those inputs

A target with exact immutable input X can still depend on:

```text
libc
kernel
network API
DNS
clock
GPU
secret
external DB
randomness
```

Therefore:

```text
ImmutableInputClosure != ExecutionEnvironmentClosure
```

---

# 26. Source commitment is also only one projection

`sourceStateDigest` freezes a declared tracked source projection.

It does not automatically freeze:

```text
ignored files
build outputs
system libraries
host tools
network modules
external services
clock
```

Thus:

```text
SourceClosure != RuntimeClosure
```

---

# 27. Execution-provider snapshot freezes Runtime-owned dispatch machinery, not target world

Current provider snapshot binds:

```text
Linux Runner contract + executable digest
```

or:

```text
Windows launcher contract + executable digest + WSL distribution
```

and revalidates before dispatch.

RF7 classifies this as:

```text
Runtime-owned provider dependency freeze/bind
```

not:

```text
complete target runtime environment snapshot
```

---

# 28. Environment variables are only one environment projection

Current Runtime deliberately constructs a small environment and does not inherit the
whole host environment.

For Linux it fixes values such as:

```text
PATH
HOME
LANG=C.UTF-8
LC_ALL=C.UTF-8
TMPDIR
XDG_CACHE_HOME
package/build cache paths
```

This meaningfully reduces accidental ambient context.

But environment variables are not equivalent to environment closure.

---

# 29. Locale is a real behavior dependency

POSIX defines `LANG`, `LC_ALL` and category-specific `LC_*` values as affecting character
interpretation, collation, messages, numeric/time formatting and other locale-sensitive
behavior.

Therefore fixing:

```text
LANG=C.UTF-8
LC_ALL=C.UTF-8
```

is not cosmetic; it removes one important nondeterministic/contextual dimension.

But:

```text
LocaleFreeze != WholeEnvironmentFreeze
```

---

# 30. Timezone/clock remain separate dependencies

Even with locale fixed, programs can observe:

```text
wall clock
monotonic clock
timezone database/system configuration
```

and behavior can depend on them.

RF5 already showed clocks have different semantics.

Unless virtualized/frozen by contract, time remains open context.

---

# 31. Randomness is another open dependency

Programs may consume:

```text
kernel RNG
language PRNG seeds
hardware randomness
provider-generated IDs
```

If exact reproducibility is the claim, random source/state may need control.

If only safe execution is the claim, it may remain open.

Thus dependency closure depends on the claim being made.

---

# 32. Network endpoint names are references, not frozen services

A hostname/service name can resolve differently over time/context.

Even if IP is fixed, the remote service may deploy new code/state.

Therefore:

```text
EndpointName != RemoteServiceIdentity
IP != RemoteSemanticState
```

Network dependencies are usually open unless a stronger provider/content protocol binds
them.

---

# 33. DNS/service discovery is contextual resolution

Like filesystem paths, network names are resolved under a resolver/service-discovery
context.

Thus:

```text
Resolve(name, resolver_state, cache, namespace, time)
→ endpoint(s)
```

This creates a close analogy:

```text
filesystem pathname resolution
network/service name resolution
```

Both are contextual reference interpretation.

---

# 34. Network namespace isolates network view, not external service semantics

A Linux network namespace can isolate devices, routes, ports and network stack state.

But if it permits egress to an external API, the remote service remains an external open
world.

Therefore:

```text
NetworkNamespaceIsolation != NetworkDeterminism
```

---

# 35. Container is a composition of boundaries, not a new universe

OCI Linux containers configure combinations of:

```text
root filesystem
mounts
namespaces
cgroups
capabilities
LSM/filesystem restrictions
process environment
```

and any namespace omitted may be inherited from Runtime/host context.

Thus:

```text
ContainerBoundary != CompleteHostIndependence
```

A container remains dependent on host kernel semantics and configured external resources.

---

# 36. Same container image does not imply same kernel environment

Linux containers use host kernel facilities rather than bundling an independent guest
kernel in the ordinary process-container model.

OCI itself defines Linux configuration in terms of host kernel namespaces, cgroups,
capabilities and filesystem mechanisms.

Therefore:

```text
SameContainerImage != SameKernelSemantics
```

---

# 37. VM is a stronger/different boundary, not complete closure

KVM creates a VM abstraction with virtual CPUs, memory and devices controlled through the
host KVM subsystem.

A VM can run its own guest kernel, therefore separating guest-kernel state from ordinary
host process-container semantics more strongly.

But it still depends on:

```text
hypervisor/KVM
host CPU/features
virtual devices
backing storage
network
host scheduling
external services
```

Therefore:

```text
VM != ClosedUniverse
```

---

# 38. Sandbox is a contract, not one mechanism

The word sandbox can mean any deliberately restricted execution environment.

It may use:

```text
process credentials
namespaces
seccomp
LSM
containers
VMs
language isolation
remote/disposable machine
```

RF7 therefore refuses:

```text
Sandbox = Container
Sandbox = Namespace
Sandbox = VM
```

A sandbox claim must state isolation dimensions and threat model.

---

# 39. `contained_local` is therefore an isolation vector

Current contained-local uses mechanisms including:

```text
private network
capability removal
no-new-privileges
namespace/address-family restrictions
read-only base filesystem
hidden host state hierarchies
explicit bind-back of required paths
fixed environment allowlist
```

RF7 interprets this as a **composite partial isolation profile**.

It reduces several environment/authority surfaces, but deliberately does not claim:

```text
hostile-code kernel isolation
complete confidentiality
controlled external egress semantics
disposable host semantics
complete dependency closure
```

---

# 40. Isolation dimensions can conflict with compatibility

P3's sealed-memfd executable experiment is instructive.

Executing exact sealed bytes strengthened byte identity but changed legitimate observable
semantics:

```text
shebang __file__
sys.argv[0]
/proc/self/exe
```

Thus strengthening one boundary can alter the target's environment/meaning.

Therefore:

```text
StrongerIsolation != SemanticallyTransparentIsolation
```

---

# 41. Boundary mechanism can become part of behavior

Programs can observe:

```text
pathnames
mounts
hostnames
PID namespace
CPU features
clock source
filesystem type
container/VM artifacts
```

so the boundary itself can influence behavior.

Therefore isolation cannot always be added “underneath” without changing Operation
semantics.

---

# 42. Hardware is environment

Executable behavior can depend on:

```text
CPU ISA/features
memory model
GPU device
accelerator availability
NUMA/topology
storage/device behavior
```

Whether these matter depends on the claim.

For deterministic numerical computation, hardware details may even affect floating-point
results/performance paths.

---

# 43. CUDA gives a concrete hardware/software closure example

NVIDIA documents that running CUDA applications requires a compatible GPU/driver/runtime
relationship, and dynamic-linked CUDA libraries must also be present in compatible
versions.

Therefore:

```text
ApplicationBytes != GPUExecutionClosure
```

A GPU workload's dependency set includes at least device/driver/runtime compatibility
facts.

---

# 44. Device availability is not merely resource quantity

RF8 will study resources/capacity. RF7 distinguishes first:

```text
“GPU exists and exposes compatible semantics”
```

as an environment/dependency fact from:

```text
“GPU has 8 GiB free memory”
```

as a capacity/resource fact.

The first is prerequisite/environment; the second is resource quantity/state.

---

# 45. Kernel semantics are hidden dependencies for ordinary processes

Syscalls, filesystem behavior, scheduling primitives, cgroups, namespaces and security
mechanisms all rely on the kernel.

Ordinary `workspace.exec` does not freeze a kernel image/ABI/behavioral model into each
Operation.

Thus kernel semantics remain largely ambient platform dependency.

---

# 46. Complete dependency closure is claim-relative

For claim:

```text
“these input bytes were exactly supplied”
```

closure may be small.

For claim:

```text
“this computation is bit-for-bit reproducible”
```

closure may need:

```text
program
libraries
kernel/runtime
CPU/FPU behavior
locale/time/randomness
data
```

For claim:

```text
“external business outcome will be identical”
```

complete closure may be impossible because external world/humans/providers remain open.

Thus:

```text
DependencyClosure(Q) is relative to claim Q.
```

---

# 47. Open-world execution may have no finite complete closure

Arbitrary programs can:

```text
read current time
query network
load generated plugin names
scan filesystem
contact humans/services
consume randomness
branch on hardware state
```

The future dependency set may not be statically enumerable.

Therefore:

```text
ArbitraryExecutionCompleteClosure
```

is generally not something Runtime should claim without a deliberately closed execution
contract.

---

# 48. Closure can be engineered by restricting the world

Instead of discovering every possible dependency, a system can reduce accessible Reality:

```text
no network
fixed mounted filesystem
fixed environment
no ambient credentials
controlled clock/randomness
fixed runtime image
isolated guest kernel
```

This turns dependency discovery partly into **environment construction**.

But every restriction must be measured against semantic compatibility and cost.

---

# 49. Hermeticity

RF7 uses **Hermeticity** as a strong execution-environment property:

> execution consumes only dependencies admitted by the declared closure and cannot
> silently reach undeclared external dependencies in the relevant dimensions.

Hermeticity is not the same as determinism.

A hermetic program can still be nondeterministic due to intentionally supplied randomness
or concurrency.

Thus:

```text
Hermetic != Deterministic
```

---

# 50. Reproducibility

**Reproducibility** is a claim about repeated realizations/results under specified
conditions.

It requires enough dependency/environment control relative to the desired equality
relation.

Thus:

```text
Reproducibility != Isolation
Reproducibility != ImmutableInputsOnly
```

---

# 51. Determinism

**Determinism** concerns whether given the relevant state/inputs the behavior/result is
uniquely determined under the model.

A deterministic function executed in two uncontrolled environments may yield different
results because the actual dependency state differs.

Therefore:

```text
ProgramDeterminism != DeploymentReproducibility
```

---

# 52. Context should not all enter Operation identity

If every contextual fact enters identity:

```text
trace timestamp
current free memory
incidental hostname
unrelated process state
```

then almost every request becomes unique and replay impossible.

Operation identity should bind facts that are contractually material to the admitted
commitment, not all observable Reality.

Thus:

```text
Context != IdentityMaterial by default
```

---

# 53. Dependency materiality determines commitment strength

A useful RF7 rule:

```text
if drift of D invalidates the Operation's promised realization contract,
D needs an explicit commitment/handling strategy.
```

Possible strategies:

```text
freeze/materialize
bind + revalidate
witness
virtualize
external owner receipt/query
or weaken the claim and declare D open
```

---

# 54. Not every dependency should be frozen

Some dependencies are intentionally live:

```text
current market data
current database
current web API
current filesystem state
```

Freezing them would change the semantics of the operation.

Therefore:

```text
Dependency != MustFreeze
```

The correct contract may be:

```text
bind query definition + observe current state at use time
```

rather than snapshot data.

---

# 55. Live dependency semantics require explicit temporal interpretation

If an operation intentionally means:

```text
“fetch current quote when executed”
```

then current quote is an open/live dependency by design.

RF4 replay must still preserve the historical Operation contract, but RF5/RF7 must say
whether re-execution later is expected to observe a new current value.

Thus environment semantics and replay semantics interact.

---

# 56. External databases create mixed closed/open operations

An operation can have:

```text
frozen code
frozen model/input file
```

while reading a mutable database.

Then some parts of realization are closed and others live.

RF7 prefers an explicit mixed dependency map over a fake binary label:

```text
hermetic = false
```

without explanation.

---

# 57. Boundary ownership and observation ownership differ

Runtime may own:

```text
Job record
Attempt bundle
input materialization
Runner process tree
```

while merely observing:

```text
host dependency path
systemd state
provider receipt
```

and not controlling:

```text
remote provider implementation
network
kernel scheduler
external database
```

Therefore:

```text
ObservedByRuntime != OwnedByRuntime
```

---

# 58. Boundary and authority interact but remain distinct

RF6 established authority dimensions.

A mount namespace boundary may prevent visibility of host paths, while user authority
still allows creating/reconfiguring mounts inside the namespace.

A VM may separate guest kernel while a host administrator retains total machine control.

Thus:

```text
Boundary != Authority
```

although boundaries are often mechanisms for enforcing authority/isolation.

---

# 59. Boundary and resource control also remain distinct

A process can be in a separate namespace but consume unbounded CPU/memory unless cgroup/
resource mechanisms constrain it.

Conversely two processes can share namespaces while having different resource budgets.

Therefore:

```text
NamespaceIsolation != ResourceIsolation
```

RF8 will continue this.

---

# 60. Failure isolation is another dimension

A target crash may be contained to one process tree.

But:

```text
kernel panic
filesystem corruption
external destructive API call
resource exhaustion
```

can cross that boundary.

Therefore process-tree ownership is not universal failure containment.

---

# 61. Information-flow isolation is stronger than access denial

Even if direct filesystem access is denied, information may cross through:

```text
network
shared cache
timing
resource contention
logs
explicit outputs
```

RF7 does not attempt a complete noninterference model, but rejects equating hidden paths
with complete information isolation.

---

# 62. Current contained-local claims are appropriately narrow

Current docs say contained-local protects against accidental credential/host-state access
for local engineering workloads, while refusing claims of kernel-grade hostile-code
isolation, complete confidentiality or controlled egress.

RF7 strongly supports this wording.

It accurately names the isolation dimensions actually enforced.

---

# 63. Current Windows immutable input is a different boundary composition

Windows input-bound execution:

```text
materializes exact bytes
→ native provider copy
→ verifies inventory/digests
→ ACL-protected parent/tree
→ limited child ReadAndExecute
```

This creates a limited-child read-only input boundary.

It is deliberately not immutable against separate elevated administrator authority.

Thus:

```text
ReadOnlyForCommittedChild != GloballyImmutable
```

---

# 64. Same logical contract can require different boundary mechanisms by platform

Linux contained immutable input and Windows limited-child immutable input use different
physical mechanisms.

Yet both implement a shared higher contract:

```text
exact committed bytes
presented to target
without target write authority under declared target authority
```

Therefore:

```text
BoundaryContract != OnePhysicalMechanism
```

---

# 65. Provider boundary

Runtime's execution provider boundary is where responsibility passes to:

```text
Linux Runner/systemd substrate
or Windows launcher/native Job
```

The provider contract covers certain launch/evidence mechanics.

It does not include arbitrary downstream provider semantics used by target code.

Thus:

```text
ExecutionProviderBoundary != ExternalEffectProviderBoundary
```

---

# 66. Environment source matters

Current Windows runtime context records:

```text
environment_source = windows_user_machine_profile_allowlist_v1
```

because the baseline is obtained under a particular token/security context and allowlist
contract.

This is stronger than simply serializing key/value pairs: it records how the environment
projection was produced.

RF7 generalizes:

```text
EnvironmentValue + Provenance/ConstructionContract
```

can be more meaningful than value alone.

---

# 67. Hidden inherited file descriptors are environment/authority channels

POSIX specifies that open descriptors generally remain open across exec unless marked
close-on-exec.

Therefore a process can inherit access to resources that are absent from argv/env/path
configuration.

This is a decisive falsifier of:

```text
Environment = environment-variable map
```

---

# 68. Caches can be dependencies without being semantic inputs

Build/package caches can affect:

```text
performance
which downloaded artifact is reused
failure mode
occasionally behavior if cache correctness is weak
```

Current Runtime gives them explicit locations and separates shared versus contained
profiles.

They should be classified according to the claim, not assumed to be semantically inert.

---

# 69. Cache identity is not source identity

A clean source commitment does not imply a clean build cache.

Likewise a build cache may contain artifacts produced under different toolchain/context.

Therefore:

```text
SourceDigest != BuildArtifactProvenance
```

unless the build system provides stronger content-addressed/reproducible semantics.

---

# 70. Toolchain closure is distinct from source closure

Compiler/interpreter/package-manager versions can materially change behavior.

Therefore reproducible builds/executions may need explicit toolchain dependencies.

Current Host Dependencies can express some declared files, but do not automatically
capture complete toolchain/module/kernel closure.

---

# 71. Dependency graph can change during execution

A program can branch and discover different dependencies based on earlier observations.

Therefore:

```text
Dep(execution)
```

can be dynamically realized rather than fully known at admission.

This creates a choice:

```text
restrict discovery to declared closure
trace/record dynamic dependencies
or accept open dependencies
```

depending on contract.

---

# 72. Dynamic tracing is evidence, not automatic semantic closure

Even if Runtime traced every file/network syscall in one run, that would show:

```text
dependencies observed in that realization
```

not necessarily:

```text
all possible dependencies for all future executions
```

because branches/input/timing can differ.

Thus:

```text
ObservedDependencySet != CompletePossibleDependencySet
```

---

# 73. Dependency closure also has an authority dimension

If a target has authority to open arbitrary filesystem/network objects, its future
dependency set can expand dynamically.

RF6's least-authority approach therefore assists RF7 closure:

```text
reduce accessible authority
→ reduce possible dependency surface
```

This is a deep link between authority and hermeticity.

---

# 74. Closure through authority restriction can be stronger than dependency enumeration

Instead of predicting every file a hostile/open program might request, a sandbox may make
only a declared set reachable.

Then undeclared dependencies fail rather than silently enter realization.

This is often a stronger closure strategy, but may reduce compatibility.

---

# 75. World closure is never “all Reality”

Even a highly controlled VM execution remains embedded in:

```text
physical hardware
host hypervisor
power/failure domain
operator actions
network/environment
```

A Runtime should claim only the closure relevant to its contract.

Thus:

```text
Closure != OntologicalIsolationFromUniverse
```

---

# 76. RF7 dependency-envelope model

For one Operation O and claim Q:

```text
DependencyEnvelope(O,Q) =
  ExplicitInputs
+ FrozenFacts
+ MaterializedDependencies
+ BoundDependencies
+ WitnessedDependencies
+ VirtualizedDependencies
+ QueriedExternalDependencies
+ DeclaredOpenDependencies
```

with each item carrying:

```text
identity
owner
scope
lifetime
handling strategy
evidence
failure semantics
```

---

# 77. Boundary vector model

RF7 models boundary/isolation as a vector:

```text
B(O) = {
  process,
  pid,
  mount,
  network,
  user/authority,
  filesystem,
  resource,
  failure,
  information,
  provider,
  persistence,
  observation
}
```

Each coordinate states:

```text
mechanism
scope
what crosses
what remains shared
evidence/threat model
```

This is more truthful than:

```text
sandboxed=true
```

---

# 78. Current Runtime dependency/boundary map

| Current mechanism | RF7 interpretation |
|---|---|
| Workspace source digest | frozen tracked-source projection |
| Execution Plan | frozen operation/environment projection |
| immutable input set | Runtime-owned materialized explicit-input closure |
| InputAuthority | authority/namespace boundary for sourcing input bytes |
| HostDependency | declared bound filesystem dependency |
| path-drift watch | Runtime-host-namespace continuity witness |
| executable digest/watch | target path/provider continuity evidence |
| ExecutionProviderSnapshot | frozen Runtime-owned dispatch-provider dependency |
| explicit minimal env | constructed process-environment projection |
| fixed `LANG/LC_ALL` | locale dependency reduction |
| contained_local | composite partial isolation/authority/environment profile |
| trusted_local | broad/open ambient host dependency/authority profile |
| Windows limited input tree | target-authority-scoped read-only materialization boundary |
| ForeignReference | external identity/dependency binding, not realization closure |
| provider receipt/query | external dependency/effect-owner observation |
| network/kernel/GPU/clock | generally open ambient dependencies unless separately contracted |

---

# 79. What current Runtime gets especially right

## 79.1 It names partial proof scopes

`runtime_host_namespace_path_witness` is exactly the right kind of wording: it says which
observer/namespace/invariant was witnessed.

## 79.2 It refuses complete environment-closure claims

Current docs explicitly list ELF loader edges, `dlopen`, modules, GPU, kernel, network and
other ambient prerequisites as outside automatic closure.

## 79.3 It materializes inputs when strong byte closure is required

This is substantially stronger than validating a mutable path.

## 79.4 It does not silently change trusted-local semantics to satisfy evidence wishes

P4 correctly refused to convert a trusted authority profile into a hidden sandbox.

## 79.5 It binds Runtime-owned provider machinery separately

Provider snapshot closes a specific dispatch dependency without pretending to close the
target world.

---

# 80. What RF7 reopens

## 80.1 Environment terminology should become more precise

Current code/docs sometimes use `environment` for environment-variable maps while the
foundations term is broader. Future docs should distinguish:

```text
process environment map
execution environment/dependency envelope
```

No field/API rename is authorized here.

## 80.2 Toolchain/library dependency contracts remain workload-specific

We now know the problem, but there is no evidence that generic automatic closure belongs
in Runtime Core. A consumer with a real reproducibility need should supply the cheapest
falsifier and concrete dependency contract.

## 80.3 Host Dependency support is deliberately trusted-local only

Contained/Windows have different boundary mechanisms. Extending HostDependency should
not occur merely for symmetry.

## 80.4 Network dependency semantics remain largely open

A future consumer requiring reproducible or controlled network behavior needs a dedicated
network/provider contract rather than generic Runtime timestamp/digest tricks.

## 80.5 Cross-platform “same environment” claims need normalization criteria

Linux and Windows can realize similar contracts with different kernels/path models/token
environments. Semantic equivalence should be claim-specific, not byte-identical config
assumption.

---

# 81. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| same executable, changed `.so` | executable = execution closure | runtime library is a dependency |
| delayed `dlopen` after validation | pre-spawn digest = use-time bytes | dependency can be consumed later |
| same host path, target private mount | host witness = target view | namespace-qualified resolution matters |
| open FD across exec | env map = all inherited context | descriptor capabilities cross exec |
| same source, different compiler | source closure = build closure | toolchain is independent dependency |
| same container image, different kernel | image = environment | container shares host-kernel semantics |
| VM with different host CPU/KVM | VM = closed universe | hypervisor/hardware remain dependencies |
| same env vars, different locale database | env map = realized behavior | values reference implementation data |
| same hostname, new DNS target | name = service identity | resolution context/time matters |
| same IP, provider redeployed | endpoint = semantic service state | remote state remains open |
| network namespace + egress | namespace isolation = determinism | external service still changes |
| immutable input + live DB | frozen input = hermetic execution | operation can mix closed/open dependencies |
| GPU app, incompatible driver | app bytes = executable capability | device/driver/runtime compatibility matters |
| fixed source + wall clock | code/input = deterministic result | time can be dependency |
| fixed code + RNG | exact input = deterministic realization | randomness state can alter behavior |
| contained_local | one sandbox flag = full isolation | multiple dimensions remain shared/open |
| sealed memfd changes `/proc/self/exe` | stronger byte isolation = transparent | boundary mechanism changes semantics |
| read-only child input tree | read-only = global immutable | authority scope determines guarantee |
| syscall trace from one run | observed deps = all possible deps | dynamic paths may differ in another run |
| no network + fixed mounts | enumerate every dependency required | restriction can engineer closure |

---

# 82. RF7 anti-laws

RF7-L1: `Boundary != UniversalShell`.

RF7-L2: `Isolation != Boolean`.

RF7-L3: `ProcessBoundary != AuthorityIsolation`.

RF7-L4: `NamespaceIsolation != CompleteIsolation`.

RF7-L5: `SamePathString != SameResolvedObject`.

RF7-L6: `HostPathWitness != TargetNamespaceViewIntegrity`.

RF7-L7: `PathCommitment != ByteConsumptionProof`.

RF7-L8: `ExecutableBytes + Args != CompleteExecutionEnvironment`.

RF7-L9: `Environment != State`.

RF7-L10: `Context != ExecutionEnvironment`.

RF7-L11: `Input != CompleteDependencySet`.

RF7-L12: `UndeclaredDependency != NonexistentDependency`.

RF7-L13: `DirectDependencies != TransitiveClosure`.

RF7-L14: `ExecutableDigest != ExecutionClosureDigest`.

RF7-L15: `Binding != Ownership`.

RF7-L16: `Witness != Immutability`.

RF7-L17: `MaterializedBytes != CompleteEnvironmentClosure`.

RF7-L18: `ImmutableInputClosure != ExecutionEnvironmentClosure`.

RF7-L19: `SourceClosure != RuntimeClosure`.

RF7-L20: `ProviderSnapshot != CompleteTargetEnvironment`.

RF7-L21: `EnvironmentVariableMap != ExecutionEnvironment`.

RF7-L22: `LocaleFreeze != WholeEnvironmentFreeze`.

RF7-L23: `EndpointName != RemoteServiceIdentity`.

RF7-L24: `NetworkNamespaceIsolation != NetworkDeterminism`.

RF7-L25: `ContainerBoundary != CompleteHostIndependence`.

RF7-L26: `SameContainerImage != SameKernelSemantics`.

RF7-L27: `VM != ClosedUniverse`.

RF7-L28: `Sandbox != Container/Namespace/VM by definition`.

RF7-L29: `StrongerIsolation != SemanticallyTransparentIsolation`.

RF7-L30: `ApplicationBytes != GPUExecutionClosure`.

RF7-L31: `Hermetic != Deterministic`.

RF7-L32: `Reproducibility != Isolation`.

RF7-L33: `ProgramDeterminism != DeploymentReproducibility`.

RF7-L34: `Context != IdentityMaterial by default`.

RF7-L35: `Dependency != MustFreeze`.

RF7-L36: `ObservedByRuntime != OwnedByRuntime`.

RF7-L37: `Boundary != Authority`.

RF7-L38: `NamespaceIsolation != ResourceIsolation`.

RF7-L39: `ReadOnlyForCommittedChild != GloballyImmutable`.

RF7-L40: `BoundaryContract != OnePhysicalMechanism`.

RF7-L41: `ExecutionProviderBoundary != ExternalEffectProviderBoundary`.

RF7-L42: `ObservedDependencySet != CompletePossibleDependencySet`.

RF7-L43: `Closure != OntologicalIsolationFromUniverse`.

---

# 83. Minimum RF7 grammar

## 83.1 Boundary `B_d`

Partition/interface for dimension d with crossing/enforcement rules.

## 83.2 Isolation `Iso_d`

Restriction of influence/visibility/authority/resource/failure/information across B_d.

## 83.3 Environment `Env(O,t)`

External conditions/structures/services available to and materially relevant to
realization during interval t.

## 83.4 Context `Ctx(O)`

Broader interpretation/decision facts, not all physically consumed.

## 83.5 Input `I`

Explicitly supplied operand under the Operation contract.

## 83.6 Dependency `D ∈ Dep(Q)`

Fact/entity/service whose counterfactual change can affect claim Q.

## 83.7 Dependency Closure `Closure(Q)`

The dependency set sufficient for the declared claim/equivalence relation.

## 83.8 Handling Strategy `H(D)`

One of:

```text
freeze
materialize
bind
witness
virtualize
query/reconcile
leave-open
```

## 83.9 Hermetic Envelope `HEnv`

Environment in which relevant undeclared dependencies cannot silently enter realization.

## 83.10 Boundary Vector `B⃗`

Per-dimension map of process/namespace/authority/resource/failure/information/provider
boundaries.

---

# 84. RF7 reconstructed Runtime envelope

```text
Historical / semantic context
        │
        ▼
Committed Operation
  ├─ source projection                 [freeze]
  ├─ Execution Plan                    [freeze]
  ├─ execution provider                [bind + revalidate]
  ├─ immutable inputs                  [materialize + verify]
  ├─ Host Dependencies                 [bind + host-namespace witness]
  ├─ process env map                   [construct/freeze]
  ├─ execution profile                 [boundary/authority construction]
  ├─ OS/kernel/toolchain/libraries      [partially bound or open]
  ├─ clocks/randomness/devices          [generally open]
  ├─ network/external services          [generally open/query by domain]
  └─ external provider effects          [effect-owner contract]
        │
        ▼
Physical Realization
```

The correct question is not:

```text
“Is this execution closed?”
```

but:

```text
“For claim Q, which dependencies are closed by what mechanisms, and which remain open?”
```

---

# 85. Research constitution added by RF7

1. Never say `isolated` without naming the isolated dimension and threat/interference
   model when correctness depends on it.
2. Never treat process/container/VM as synonymous isolation strengths; state the actual
   boundary vector.
3. Interpret paths/names only with their namespace/resolution context.
4. Distinguish explicit inputs from total dependency sets.
5. Define dependency relative to the claim whose validity/reproducibility is being
   protected.
6. Use `freeze/materialize/bind/witness/virtualize/query/leave-open` to name dependency
   handling strength.
7. Do not promote witnesses into immutability claims.
8. Do not promote exact input bytes/source/provider snapshots into complete execution
   closure.
9. Treat dynamic libraries/modules, kernel, network, time, randomness, devices and
   credentials as potential dependencies even when absent from argv/env.
10. Prefer truthful partial closure over a fake hermetic flag.
11. Restrict accessible Reality when a consumer truly needs hermeticity rather than
    assuming all dependencies can be statically discovered.
12. Preserve semantic compatibility when strengthening boundaries; stronger mechanism is
    not automatically better if it changes target meaning.
13. Keep ownership, observation and control separate for environmental facts.
14. Put only materially committed environmental facts into Operation identity; context is
    larger than identity material.
15. Leave live dependencies live when “current state at use time” is the desired
    semantics; freeze the contract, not necessarily the world.
16. Do not enter resource/capacity modeling until environment/dependency facts are no
    longer conflated with resource quantities.

---

# 86. RF7 result — conceptual compression

The strongest RF7 compression is:

```text
A Runtime does not execute an Operation in “an environment” as one opaque blob.
It realizes the Operation inside multiple overlapping boundaries while depending on a
mixture of frozen, materialized, bound, witnessed, virtualized, queried and open facts.

Boundary says where interaction rules change.
Isolation says which interactions are restricted across a named boundary dimension.
Environment is the materially available external condition set for realization.
Context is broader interpretation/decision framing.
Inputs are explicit operands.
Dependencies are claim-relative counterfactual prerequisites/influences.
Closure is also claim-relative and need not be complete for open-world execution.
```

---

# 87. RF7 closeout

RF7 closes with twenty durable results:

1. **Boundary is dimension-specific; there is no universal Runtime perimeter.**
2. Isolation is a vector of restrictions over namespace, authority, resource, failure,
   information and other dimensions rather than a boolean property.
3. Process/address-space, namespace, authority, resource and provider boundaries remain
   distinct.
4. Path/object resolution is context/namespace dependent; same textual path does not
   guarantee same object.
5. Current P4 physically proves Runtime-host-namespace path witness does not establish
   target mount-namespace view integrity or byte consumption.
6. Execution environment is broader than argv/environment variables and can include
   descriptors, filesystem, libraries, kernel, devices, network, time, randomness,
   credentials and external services.
7. Environment, State and Context are different roles/projections.
8. Explicit inputs are only a subset of possible dependencies.
9. Dependencies are claim-relative and can be direct, transitive, ambient, authority,
   hardware/kernel, network/service or temporal/random.
10. **Dependency handling must distinguish freeze, materialize, bind, witness, virtualize,
    query/reconcile and leave-open.**
11. Dynamic linking/P2 proves executable digest is not execution-closure digest; dynamic
    loading prevents generic static complete closure.
12. Current immutable inputs form a strong Runtime-owned byte/materialization closure for
    declared inputs but not a complete execution-environment closure.
13. Current Host Dependencies are explicitly declared/bound filesystem prerequisites plus
    Runtime-host-namespace continuity witnesses, not automatic dependency discovery.
14. Current ExecutionProviderSnapshot closes Runtime-owned dispatch-provider identity only.
15. Fixed process environment/locale reduces ambient variability but does not close kernel,
    time, randomness, device or network dependencies.
16. Containers compose host-kernel boundaries; VMs add a guest-kernel/virtual-machine
    boundary but still remain host/hardware/provider dependent. Neither implies a closed
    universe.
17. Hermeticity, determinism, isolation and reproducibility are distinct properties.
18. Dependency closure is relative to the claim being protected and may be impossible to
    enumerate completely for arbitrary open-world execution.
19. Authority reduction can reduce possible dependency surface; closure can sometimes be
    engineered more reliably by restricting reachable Reality than by enumerating it.
20. **Current Ordivon Runtime should continue making strong narrow closure claims and
    explicit open-world disclaimers rather than introducing a generic “fully captured
    environment” abstraction without a concrete consumer.**

The next frontier is:

> **Once environment and dependencies are separated from boundaries, what exactly are
> resources, capacity, scarcity, budgets, reservations, quotas, contention and pressure?
> Which constraints merely limit execution and which resources actually enable it?**

That is RF8 — Resource, Capacity, Constraint and Scarcity.
