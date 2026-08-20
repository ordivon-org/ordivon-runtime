---
schema_version: 1
id: runtime.foundations.rf13
title: RF13 — Isolation, Containment and Trust
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
summary: RF13 establishes Runtime isolation foundations: isolation is dimension- and threat-model-scoped non-interference, containment is control over where effects/execution can propagate, authority reduction narrows reachable actions, and trust is an assumption about actors/components rather than an enforcement mechanism. It classifies trusted_local as owner-trusted broad local authority, contained_local as a composite partial isolation/authority-reduction profile for accidental local interference, Linux cgroups/process groups and Windows Job Objects as lifecycle/resource containment rather than hostile-code sandboxes, immutable inputs as input-integrity boundaries rather than whole-environment isolation, and hostile/multi-tenant execution as requiring a stronger external isolation owner such as AppContainer/VM/sandbox infrastructure. It rejects one-bit sandbox claims and introduces isolation vectors across process, filesystem, namespace, network, credential, resource, kernel and external-effect dimensions.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF13
related:
  - runtime.foundations.start
  - runtime.foundations.rf12
  - runtime.foundations.rf13.sources
  - runtime.foundations.rf13.continuation
---
# RF13 — Isolation, Containment and Trust

## 0. Status and research question

RF12 established that transaction boundaries follow actual participating owners. RF13 asks a
parallel question for execution safety:

> **When Runtime says an execution is trusted, contained, isolated or bounded, which effects
> are actually prevented from crossing which boundary, against which actor/threat model, and
> by which enforcement mechanism?**

RF13 is explanatory research only. It does not authorize replacing current profiles,
introducing VM/AppContainer/container orchestration, changing service privilege, adding
seccomp/Landlock/bubblewrap, controlled egress, secret brokerage, multi-tenant admission or
production API changes.

---

# 1. First deletion — “sandboxed” is not a scalar fact

A one-bit claim:

```text
sandboxed = true
```

is almost useless without answering:

```text
isolated from what?
which resource dimension?
against which actor?
under which kernel/security boundary?
what remains shared?
```

Therefore:

```text
SandboxLabel != IsolationContract
```

---

# 2. Isolation

RF13 defines **Isolation** as:

> **A contract under which actions/state changes in subject A cannot observe, influence or
> interfere with a specified set of resources/subjects B beyond explicitly permitted
> channels, under a declared threat model.**

Isolation is therefore always scoped.

---

# 3. Isolation vector

RF13 models an execution isolation contract as a vector rather than one number:

```text
I = {
  process,
  identity/authority,
  filesystem,
  mount/path view,
  IPC,
  network,
  credentials/secrets,
  devices,
  resources,
  kernel,
  external services,
  persistence,
  observation channels
}
```

Each dimension can have a different strength.

---

# 4. Non-interference

**Non-interference** is the property that one subject's behavior cannot affect or reveal a
specified protected dimension of another subject beyond permitted channels.

RF13 distinguishes:

```text
integrity non-interference
confidentiality non-interference
availability non-interference
```

because an isolation mechanism may protect one but not another.

---

# 5. Integrity isolation

Integrity isolation asks:

```text
Can A modify B's protected state?
```

Examples:

```text
write another Workspace
signal another process
replace executable bytes
mutate host credential file
```

---

# 6. Confidentiality isolation

Confidentiality isolation asks:

```text
Can A read/derive B's protected state?
```

Examples:

```text
read ~/.ssh
read another Job output
inspect process memory
observe secrets in environment
query internal network service
```

---

# 7. Availability isolation

Availability isolation asks:

```text
Can A consume/deny resources needed by B?
```

Examples:

```text
memory exhaustion
fork bomb
CPU saturation
filesystem filling
network saturation
```

Resource budgets improve this dimension but do not automatically protect confidentiality or
integrity.

---

# 8. Isolation is not atomicity

RF12 established atomicity over commit sets.

RF13 isolation asks what concurrent subjects may observe/influence.

Therefore:

```text
Isolation != Atomicity
```

---

# 9. Isolation is not immutability

A read-only input tree can be immutable to one child while the child still has broad network
or process authority.

Thus:

```text
ImmutableInput != IsolatedExecution
```

---

# 10. Isolation is not determinism

A fully isolated process can still use:

```text
clock
RNG
scheduler timing
nondeterministic algorithms
```

and produce different outputs.

Therefore:

```text
Isolation != Determinism
```

RF14 will study this directly.

---

# 11. Containment

RF13 defines **Containment** as:

> **Mechanisms that bound the propagation, lifetime or reachable effect surface of an
> execution after it has started.**

Examples:

```text
process tree ownership
cgroup membership
Job Object membership
resource limits
filesystem visibility restrictions
network namespace
```

Containment need not imply hostile-code security.

---

# 12. Containment is directional

A supervisor may be able to kill every owned child process while those children were still
able to send arbitrary network requests before termination.

Thus:

```text
LifecycleContainment != EffectContainment
```

---

# 13. Authority reduction

RF13 defines **Authority Reduction / Attenuation** as:

> **Reducing the set of actions/resources a subject can access compared with a broader
> ambient authority baseline.**

Examples:

```text
drop root UID
remove capabilities
hide host directories
private network
read-only mounts
limited Windows token
```

---

# 14. Authority reduction is not isolation by itself

A process can lose some capabilities yet still share:

```text
kernel
same UID resources
network
IPC
mutable cache
```

with other processes.

Therefore:

```text
AuthorityReduction != CompleteIsolation
```

---

# 15. Trust

RF13 defines **Trust** as an assumption that a participant/component will behave within an
expected envelope despite retaining authority that could violate it.

Trust is not an enforcement mechanism.

---

# 16. Trusted code can still be buggy

Owner-trusted code may accidentally:

```text
delete wrong files
print credentials
consume excessive resources
call unintended API
```

Therefore:

```text
Trusted != Safe
```

and accidental containment remains valuable even in trusted environments.

---

# 17. Untrusted versus hostile

RF13 separates:

```text
untrusted
  not yet relied upon / unknown behavior

hostile
  actively attempts to violate the boundary
```

A mechanism suitable for accidental/untrusted-data handling may not withstand hostile
same-host code.

---

# 18. Threat model

An isolation claim requires a declared **Threat Model**:

```text
benign bugs?
accidental path traversal?
malicious target process?
malicious same-UID sibling?
root attacker?
kernel exploit?
host administrator?
remote attacker controlling input?
```

Without this, “secure sandbox” is underspecified.

---

# 19. Trusted computing base

The **TCB** is the set of components whose correct behavior is assumed for a security claim.

For current local Runtime this can include, depending on claim:

```text
Linux kernel
systemd
Runtime service
Runner
filesystem/permissions
configured service account
Windows kernel/launcher/token machinery
```

Reducing TCB can strengthen hostile-code isolation, but usually increases complexity/cost.

---

# 20. Current `trusted_local`

Current documentation defines `trusted_local` as the canonical owner-trusted profile that
inherits the installed service user's local authority.

RF13 interpretation:

```text
trusted_local = broad owner-trusted local execution authority
```

not:

```text
sandbox
```

---

# 21. `trusted_local` trust boundary

The operator trusts:

```text
target repository/executable
its dependencies
its behavior under service-user authority
```

well enough to let it execute with broad local access.

Runtime still supervises/records it, but does not attempt hostile-code isolation.

---

# 22. Trusted-local process can intentionally mutate its world

Current source commitment does not create an immutable snapshot. A same-authority process can
write directly outside Runtime APIs, and a running command may intentionally modify its own
Workspace.

Therefore:

```text
SourceCommitment != WorkspaceIsolation
```

---

# 23. Trusted-local cross-Workspace interference remains possible under ambient authority

If two Workspaces/files are accessible to the same effective service user, a trusted-local
target can potentially access the other unless filesystem ownership/permissions or another
mechanism prevents it.

Runtime's one-Workspace reservation policy prevents **Runtime-mediated concurrent mutation**;
it is not a kernel-enforced per-Workspace security boundary.

---

# 24. Runtime reservation is coordination, not isolation

```text
one active writer per Workspace
```

prevents two admitted Runtime writers from colliding under Runtime's control plane.

It does not stop an out-of-band process with sufficient OS authority.

Thus:

```text
Reservation != SecurityIsolation
```

---

# 25. Current `contained_local`

Current contained-local composes mechanisms including:

```text
private network
capability removal
NoNewPrivileges
namespace/address-family restrictions
read-only base filesystem
hidden /root /home /run /var
exact bind-back paths
non-root payload UID/GID
fixed environment projection
Workspace-scoped cache/build paths
```

RF13 classifies this as a **composite partial isolation + authority-reduction profile**.

---

# 26. `contained_local` is stronger than mere UID drop

Dropping UID alone leaves many ambient views/resources reachable.

Contained-local additionally constrains filesystem/network/capability/environment surfaces.

Therefore:

```text
contained_local > simple privilege drop
```

for its declared dimensions.

---

# 27. But `contained_local` is explicitly not hostile-code kernel isolation

Current docs deliberately reject claims of:

```text
kernel-grade hostile-code isolation
complete filesystem confidentiality
controlled external egress semantics
disposable-machine semantics
multi-tenant sandboxing
```

RF13 strongly supports this wording.

---

# 28. Why private network is a real isolation dimension

Linux network namespaces isolate network devices, stacks, routing tables, firewall rules,
port/socket namespaces and related `/proc`/`sys` networking views.

Thus a private network namespace can create real network-view/effect separation.

But it says nothing about filesystem or kernel isolation.

---

# 29. Namespace isolation is resource-specific

Linux namespaces deliberately isolate particular global resource classes:

```text
mount
PID
network
IPC
UTS
user
cgroup view
```

Therefore:

```text
NamespaceExists != AllResourcesIsolated
```

---

# 30. Mount namespace is a view boundary

Mount namespaces give processes distinct mount hierarchies.

P4 exploited exactly this fact: same textual pathname can resolve different objects in
different mount namespaces.

Thus:

```text
PathString != GlobalObjectIdentity
```

and mount isolation can both protect and complicate evidence.

---

# 31. P4 is not merely an evidence bug

P4 also proves a security/authority fact:

> a same-authority trusted target that can create a private mount namespace can intentionally
> alter its own path-resolution world without modifying the Runtime host namespace.

This is permitted authority, not a containment escape from an advertised sandbox.

---

# 32. User namespace versus UID drop

Linux user namespaces isolate UID/GID and capability interpretation relative to a namespace.

A plain `setuid()` drop changes credentials but does not itself create a separate user
namespace/security world.

Therefore:

```text
NonRootUID != UserNamespaceIsolation
```

---

# 33. No-new-privileges

`PR_SET_NO_NEW_PRIVS` prevents `execve` from granting additional privilege through mechanisms
such as setuid/setgid file mode bits/file capabilities under Linux semantics.

RF13 interpretation:

```text
NoNewPrivileges = privilege-escalation attenuation
```

not universal sandboxing.

---

# 34. Capability removal

Removing effective/ambient Linux capabilities narrows privileged kernel operations available
to a process.

It does not remove all authority granted through ordinary UID/file/network permissions.

Thus:

```text
NoCapabilities != NoAuthority
```

---

# 35. Read-only filesystem base

Making the base filesystem read-only protects integrity of visible mounted state from normal
writes.

It does not necessarily prevent:

```text
reads of visible secrets
network exfiltration
shared-kernel attack
writes through explicitly writable bind mounts
external service mutations
```

---

# 36. Hidden path is not universal confidentiality

Hiding `/root` or `/home` removes direct pathname access through that mount view.

A secret might still be exposed through:

```text
environment
open inherited descriptor
network service
other bind mount
process memory/IPC
log/Artifact
```

So:

```text
HiddenPath != SecretIsolation
```

---

# 37. Environment filtering is authority reduction

Not inheriting the Runtime service environment prevents accidental propagation of ambient
credentials/configuration.

This reduces a major secret/authority surface.

It cannot protect a secret reachable through another channel.

---

# 38. Secret broker is a distinct abstraction

A dedicated secret-delivery mechanism can grant:

```text
specific secret
specific scope
specific lifetime
possibly non-exportable use
```

Contained-local currently does not implement this, and current docs correctly say it is not
a substitute for one.

---

# 39. Immutable inputs

`workspace.execBound` materializes exact digest-verified bytes into a Job-owned presentation,
read-only for the intended limited child contract.

This is a strong **input integrity/authority boundary**.

---

# 40. Immutable inputs do not imply isolated surroundings

The target can still depend on:

```text
runtime libraries
time
RNG
kernel
network where permitted
external files/services
```

Therefore:

```text
ImmutableInputClosure != EnvironmentClosure
```

---

# 41. Read-only to one subject is not globally immutable

Windows immutable input presentation is protected against the committed limited child.

An elevated administrator remains outside that authority boundary.

Thus:

```text
ReadOnlyForChild != ImmutableAgainstAdministrator
```

---

# 42. Containment must name the adversary

If threat model excludes host administrator/root, a filesystem ACL can be sufficient for a
claim.

If root/admin is hostile, that same ACL does not provide the same guarantee.

So:

```text
SecurityGuarantee = Mechanism + ThreatModel
```

---

# 43. cgroup

Linux cgroups provide resource accounting/control and a durable ownership grouping useful
for process-tree supervision.

They do not by themselves isolate filesystem, network, credentials or kernel attack surface.

---

# 44. cgroup resource control improves availability isolation

Current Runtime can apply:

```text
MemoryMax
TasksMax
CPUQuota
```

These reduce one Job's ability to starve others along those dimensions.

Therefore cgroup budgets are partial availability isolation.

---

# 45. Resource quotas are not a scheduler/security sandbox

RF9 already distinguished scheduling from budgets.

RF13 adds:

```text
ResourceLimit != SecurityIsolation
```

except for the exact availability resource it limits.

---

# 46. Process group termination

Runner creates process groups and kills them on timeout when needed.

This improves cleanup/lifecycle containment for descendants in that group.

It does not retract external Effects already produced.

---

# 47. systemd cgroup supervision is stronger lifecycle ownership

The Attempt unit/cgroup gives Runtime a stronger process-tree ownership/recovery boundary than
tracking only a direct PID.

But:

```text
ProcessTreeOwnership != HostileCodeSandbox
```

---

# 48. Escaped external effects

Even if every local child is successfully terminated, earlier:

```text
HTTP request
cloud mutation
email
financial order
```

can survive outside the process tree.

Therefore:

```text
KillAllChildren != ContainAllEffects
```

---

# 49. Windows Job Object

Microsoft documents Job Objects as a way to manage groups of processes as a unit, apply
limits/accounting and terminate associated processes together.

With `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, closing the final Job handle terminates associated
processes.

---

# 50. Windows Job Object is lifecycle/resource containment

Current Runtime correctly uses it for:

```text
process-tree ownership
cancel/timeout/crash cleanup
resource/accounting semantics
```

It is not, by itself:

```text
filesystem sandbox
credential sandbox
network sandbox
hostile-code security boundary
```

---

# 51. Windows limited token

Current `windowsAuthority=limited` verifies a non-elevated, no-higher-than-Medium effective
token with Administrators not enabled under the supported token selection paths.

This is **authority attenuation**.

---

# 52. Limited token is not AppContainer

A normal non-elevated desktop token can still access many resources available to that user.

Microsoft AppContainer separately introduces default-deny application identity/capability
restrictions over file, network, credential, process and other resources.

Therefore:

```text
LimitedToken != AppContainerIsolation
```

---

# 53. Windows elevated token is explicitly broader authority

Current `windowsAuthority=elevated` deliberately selects and proves an already-elevated
provider token.

RF13 interpretation:

```text
Elevation = broader authority selection
```

not “stronger isolation.”

---

# 54. Authority and isolation can move in opposite directions

An elevated process may be easier to supervise while possessing much more authority to alter
the host.

Thus:

```text
MoreControlAuthority != MoreTargetIsolation
```

---

# 55. AppContainer illustrates a stronger per-application security boundary

Microsoft describes AppContainer as isolating credentials, devices, files, network,
processes/windows and requiring explicitly granted capability access.

This is qualitatively different from simply running a process with a limited desktop token.

---

# 56. VM illustrates a different isolation boundary

Windows Sandbox uses hardware virtualization and a separate kernel to isolate untrusted
applications from the host.

NIST container guidance likewise emphasizes that containers are OS-level virtualization and
carry distinct shared-kernel security concerns.

RF13 conclusion:

```text
Namespace/ContainerBoundary != VMBoundary
```

---

# 57. VM is not automatically perfect either

A VM still depends on:

```text
hypervisor
virtual devices
shared folders
clipboard
networking
host integrations
```

and can expose intentional channels.

Therefore:

```text
VM != ZeroInteraction
```

---

# 58. Stronger isolation usually means narrower explicit channels

A robust hostile-code sandbox tends toward:

```text
default deny
small capability set
explicit file grants
explicit network policy
separate identity/process namespace
small broker interface
```

rather than broad ambient authority plus monitoring.

---

# 59. Compatibility tradeoff

Reducing authority can break workloads that implicitly depend on:

```text
package caches
home directory
toolchains
network
IPC
GPU/devices
host services
```

Therefore isolation strength and compatibility often trade off.

---

# 60. Current dual-profile design reflects this tradeoff

`trusted_local` prioritizes compatibility for owner-trusted engineering work.

`contained_local` deliberately trades some compatibility for reduced ambient authority.

This is a coherent design, not inconsistency.

---

# 61. Shared caches are an isolation dimension

A mutable shared cache can create:

```text
integrity coupling
information leakage
nondeterministic dependency
resource contention
```

between Jobs.

Therefore cache sharing should be treated as explicit shared state, not invisible performance
plumbing.

---

# 62. Current trusted-local Cargo strategy separates mutable target bytes

Current trusted-local Jobs use Workspace-private Cargo target backings while presenting a
stable compiler-visible path for cache effectiveness.

This improves cross-Workspace integrity separation for build outputs without abandoning
shared compiler/package caching where intentionally allowed.

---

# 63. Contained-local uses narrower Workspace-scoped caches

This is consistent with its authority-reduction objective: reduce cross-Workspace ambient
shared mutable state even at some performance cost.

---

# 64. Cache isolation is not complete process isolation

Even with private build caches, Jobs may still share:

```text
kernel
CPU
memory pressure
clock
network depending on profile
system libraries
```

---

# 65. Same UID process interference

On ordinary Unix-like systems, same-UID processes may have permissions to signal, inspect or
interact with one another depending on ptrace/Yama/procfs/session/kernel policy.

Therefore merely assigning all contained payloads one common non-root UID would not create
strong per-Job process isolation.

---

# 66. Per-Job identity can strengthen isolation, but is not free

Distinct UID/user namespace identities could reduce cross-Job process/file authority, but
would complicate:

```text
Workspace ownership
cache reuse
Git ownership
host tooling
cleanup
mapping
```

Current Runtime does not need this without hostile multi-tenant requirements.

---

# 67. Multi-tenant isolation is a qualitatively stronger requirement

Multi-tenant means mutually distrustful principals share infrastructure.

The threat model includes one tenant intentionally attacking another or the host.

Current Runtime explicitly does not provide this.

---

# 68. Owner-trusted single-operator versus multi-tenant changes architecture

In owner-trusted mode, broad local authority can be acceptable because principal/operator
trust collapses many adversarial boundaries.

In multi-tenant mode, those assumptions disappear and must be replaced by enforcement.

---

# 69. Root-owned Runtime is a major trust amplifier

If Runtime/service has root or equivalent authority, then target-code execution pathways must
be treated carefully: accidental privilege propagation or unsafe target execution can have
large consequences.

Current docs correctly require trusted repositories/executables for trusted-local operation.

---

# 70. Root supervisor + non-root payload is a privilege-separation pattern

Contained-local can use privileged supervisor setup to create mounts/cgroups and then execute
target code with reduced credentials.

This can be strong for accidental containment, provided supervisor/Runner paths are correct.

---

# 71. Privilege separation is not equivalent to separate kernel

A payload still relies on the same Linux kernel and any kernel attack surface reachable from
its allowed syscalls/devices/namespaces.

Thus:

```text
PrivilegeSeparation != KernelIsolation
```

---

# 72. Seccomp/Landlock/bubblewrap are mechanism families, not goals

Adding one would restrict specific syscall/filesystem/namespace surfaces.

No single mechanism should be described as “the sandbox” without the resulting isolation
vector/threat model.

---

# 73. External isolation owner

RF13 defines an **External Isolation Owner** as a subsystem outside Runtime that owns the
stronger security boundary for hostile/disposable/multi-tenant execution.

Examples can include:

```text
VM sandbox
AppContainer/LPAC
specialized container sandbox
remote disposable worker
microVM
```

Runtime can execute against/through such a provider without absorbing its entire security
implementation.

---

# 74. This preserves Runtime thin-core responsibility

Runtime should own:

```text
operation identity
provider commitment
execution evidence
resource/lifecycle control
```

while the isolation provider owns:

```text
kernel/process/file/network/credential security boundary
```

when stronger hostile-code isolation is required.

---

# 75. Provider identity matters for isolation claims

If an Operation expects execution under isolation provider P, the concrete provider contract
and implementation identity should be committed like other execution-provider facts.

Otherwise replay/recovery could silently realize the same Operation under a weaker boundary.

---

# 76. Current provider commitment is a useful precursor

Runtime already binds Linux Runner or Windows launcher contract/digest into operation
identity.

A future isolation provider could extend this pattern without changing the principle.

---

# 77. Isolation drift is conceptually distinct from input drift

Possible future class:

```text
expected sandbox/profile/provider boundary changed before dispatch
```

This would be an execution-environment authority mismatch, not source-input mismatch.

RF13 records the concept only; no implementation is authorized.

---

# 78. Network isolation versus controlled egress

Private network can mean:

```text
no normal host/external networking
```

while controlled egress means:

```text
explicitly allow only destinations/protocols/identities under policy
```

These are different capabilities.

---

# 79. No network is stronger than unrestricted network for confidentiality, but weaker for compatibility

Removing network closes a large exfiltration/external-effect channel.

But many builds/tests legitimately require package/service access.

This is another policy/profile tradeoff, not an absolute “better.”

---

# 80. Network namespace does not close non-network channels

A process without network may still leak/interfere through:

```text
filesystem
shared memory
logs
CPU timing
process signaling
```

Therefore:

```text
NoNetwork != FullIsolation
```

---

# 81. Filesystem path restrictions are only one namespace view

A hidden/read-only mount view protects against ordinary pathname access through that view.

It does not prove complete confidentiality against every same-kernel channel.

---

# 82. Device access is another independent dimension

GPU, block devices, camera/microphone or special character devices can expand attack/side
channel/effect surface.

Current contained-local should not be described as device-isolated unless its actual systemd
policy proves the relevant dimension.

---

# 83. Kernel is the common authority under namespace/container execution

Namespaces virtualize selected global resources but the same kernel interprets system calls
and enforces boundaries.

Therefore kernel compromise can invalidate multiple namespace boundaries simultaneously.

---

# 84. Container security literature treats shared kernel as a real security concern

NIST SP 800-190 explicitly treats application containers as OS virtualization and documents
container-specific threats/risks rather than equating them to full VM isolation.

RF13 uses this as competing model evidence.

---

# 85. Confidentiality and integrity can diverge

A read-only mount may prevent writes while permitting reads.

A hidden mount may prevent both via pathname but still leave other information channels.

Therefore:

```text
IntegrityIsolation != ConfidentialityIsolation
```

---

# 86. Availability isolation can diverge again

Two perfectly file-isolated processes sharing CPU can still starve each other without
resource controls.

Thus:

```text
FilesystemIsolation != AvailabilityIsolation
```

---

# 87. Isolation should be stated as a matrix

Example:

| Dimension | trusted_local | contained_local | hostile external sandbox |
|---|---|---|---|
| process lifecycle | supervised | supervised | provider-specific |
| UID authority | service user | dropped non-root payload | provider-specific/minimal |
| capabilities | ambient service model | removed | provider-specific |
| filesystem write | broad allowed authority | base RO + exact writable binds | usually explicit grants |
| host home/root visibility | normal permissions | hidden | provider-specific/default deny |
| network | ambient | private/restricted | explicit policy/provider |
| kernel | shared | shared | VM may separate kernel |
| hostile-code claim | no | no | provider-dependent yes |

RF13 treats this kind of matrix as more truthful than a scalar `sandboxed` field.

---

# 88. “Contained” should name what is contained

Current contained-local most strongly contains:

```text
ambient local authority
normal host filesystem visibility
normal network access
privilege escalation paths
Runtime-managed process/resource lifetime
```

It does not contain all semantic/external consequences.

---

# 89. “Trusted” should name who trusts whom

Current `trusted_local` means:

```text
operator trusts target/repository enough to grant installed service-user authority
```

It does not mean:

```text
Runtime verified target is benign
```

---

# 90. Trust should not leak across principals

If Runtime becomes multi-user/remote in the future, owner-trust assumptions must not silently
transfer to arbitrary authenticated callers.

Authentication and local target trust remain separate.

---

# 91. Authentication is not isolation

Bearer/Cloudflare Access can authenticate/gate API callers.

It does not sandbox the code those callers ask Runtime to execute.

Therefore:

```text
IngressAuth != ExecutionIsolation
```

---

# 92. Authorization is not isolation either

An authorization policy deciding whether an Agent may request elevated Windows execution
controls **who may ask**.

Isolation controls **what the realized process can influence/observe**.

---

# 93. Principle of least privilege

Least privilege is a design strategy to grant only needed authority.

Contained-local implements parts of this by reducing ambient authority.

Least privilege can reduce blast radius even when hostile isolation is not complete.

---

# 94. Blast radius

RF13 defines **Blast Radius** as the maximum consequence set reachable if an execution acts
incorrectly or maliciously within its effective authority.

Authority reduction primarily seeks to shrink this set.

---

# 95. Isolation quality can be evaluated by reachable consequence set

Instead of asking:

```text
is this sandbox secure?
```

ask:

```text
which protected resources remain reachable?
which channels remain open?
which actor could exploit them?
```

This gives actionable engineering questions.

---

# 96. Accidental safety and adversarial security are different optimization targets

For owner-authored builds/tests, the biggest practical hazards may be:

```text
credential leakage
wrong-path writes
fork bombs
stale dependency use
cross-Workspace cache pollution
```

A partial containment profile can deliver large value without claiming adversarial security.

---

# 97. Hostile-code isolation raises proof burden

Once hostile code is in threat model, every escape-capable channel matters:

```text
syscalls
kernel vulnerabilities
procfs
ptrace
mounts
IPC
network
devices
brokers
shared files
hypervisor interfaces
```

This is materially different engineering.

---

# 98. Current status wording is therefore appropriate

Current Runtime status says:

```text
operational for owner-trusted local engineering work
hostile multi-tenant sandboxing not provided
```

RF13 strongly supports this maturity boundary.

---

# 99. Cancellation/kill is not a security eraser

If hostile/buggy code already read a secret or sent data externally, terminating it later
does not restore confidentiality.

Thus:

```text
Kill != UndoInformationFlow
```

---

# 100. Cleanup and isolation are distinct

A system can provide excellent descendant cleanup and weak pre-execution isolation.

Current systemd/cgroup and Windows Job Object work is primarily strong cleanup/lifecycle
ownership.

---

# 101. Isolation and observation are distinct

RF11 showed observation evidence does not create authority.

Similarly:

```text
Observing forbidden access != Preventing forbidden access
```

Audit/telemetry is not isolation unless paired with enforcement.

---

# 102. Detection can still be valuable

Path drift witnesses, process identities and resource metrics can detect contract violations
or ambiguity.

They improve evidence/recovery but should not be described as prevention when the target can
still act before detection.

---

# 103. Preventive versus detective controls

RF13 distinguishes:

```text
preventive control   blocks forbidden behavior

detective control   observes/reports behavior/violation

corrective control   terminates/repairs afterward
```

One mechanism can serve multiple roles, but claims should stay precise.

---

# 104. Host Dependency witness is detective, not isolation

Current Attempt-lifetime path/topology witness can detect host-view drift.

P4 proves it does not prevent target namespace reinterpretation.

Therefore:

```text
Witness != IsolationBoundary
```

---

# 105. Immutable-input mount is preventive for its specific mutation dimension

A read-only, authority-separated input presentation prevents the target from mutating those
committed input bytes through the permitted presentation path.

That is a real preventive boundary for that claim.

---

# 106. Strong claim requires mechanism alignment

General RF13 law:

```text
IsolationClaimScope <= EnforcedBoundaryScope
```

This parallels RF11 `ClaimScope <= ProvenEvidenceScope` and RF12
`AtomicityScope <= TransactionParticipantScope`.

---

# 107. Shared kernel means one large common failure domain

Namespace-based contained-local can isolate selected resource views but remains dependent on
the same kernel for enforcement.

A kernel exploit can cross several logical boundaries at once.

---

# 108. Separate kernel changes the failure domain

A hardware-virtualized VM/microVM can move the untrusted guest behind a hypervisor boundary
and separate guest kernel from host kernel.

This typically strengthens hostile-code isolation at higher startup/resource/integration
cost.

---

# 109. External sandbox can still intentionally share channels

Windows Sandbox, for example, can enable networking, clipboard, mapped folders or devices.

Each shared channel enlarges its isolation vector in a controlled way.

Therefore isolation is still configuration-sensitive even with a VM boundary.

---

# 110. Disposable semantics are another independent dimension

A VM/container can persist disks/state, or be destroyed after execution.

Disposability reduces persistence of residual state but is not synonymous with runtime
isolation.

---

# 111. Persistence isolation

Questions include:

```text
what mutable state survives Job completion?
can one later Job observe it?
can malicious state poison future executions?
```

Caches and Workspaces make this a real dimension.

---

# 112. Current Runtime intentionally persists some state

Examples:

```text
Workspace files
Registry
Artifacts
caches
receipts
input materializations
```

so current local execution is deliberately not disposable.

---

# 113. Persistence can be a feature

Persistent Workspaces/caches enable incremental work, recovery and efficiency.

The correct question is whether retained state is authority-scoped and auditable, not whether
every execution is ephemeral.

---

# 114. Cross-Job state poisoning is a threat-model-dependent risk

In owner-trusted use, shared/persistent caches may be acceptable.

In mutually hostile multi-tenant use, they would require much stronger partitioning or
reconstruction.

---

# 115. Isolation owner should own policy enforcement, not just metadata

If an external provider claims:

```text
network = none
filesystem = grants only
kernel = separate
```

those guarantees must be enforced by the provider substrate, not merely stored as labels in
Runtime.

---

# 116. Runtime may commit an isolation contract without implementing it

A future provider snapshot can record:

```text
provider type/version/digest
isolation profile identity
```

while the external provider remains enforcement owner.

This mirrors current execution-provider ownership design.

---

# 117. Isolation policy drift must fail closed when contract-relevant

If an admitted Operation requires profile P and the provider is now configured with weaker
profile Q, dispatch should not silently proceed as though equivalent.

This follows existing provider-drift principles.

---

# 118. Isolation labels need versioned semantics

`contained_local` v1 means a specific vector of restrictions.

If future versions materially change authority/threat semantics, operation identity/provider
contract should make that change visible rather than silently reinterpret historical Jobs.

---

# 119. Security boundary versus convenience boundary

A **Security Boundary** is an enforcement boundary intended to resist the declared adversary.

A **Convenience Boundary** organizes behavior but is not designed to resist bypass.

Examples:

```text
Workspace directory naming       convenience/authority address
Registry reservation             coordination boundary
read-only mount                  integrity security boundary for normal writes
VM hypervisor                    hostile-code security boundary under its threat model
```

---

# 120. Policy boundary versus physical boundary

Runtime may forbid an Agent from requesting path X.

If target code with broad OS authority can still open X directly, this is an API policy
boundary, not a physical execution isolation boundary.

---

# 121. Current executable allowed roots are admission policy/authority constraints

They constrain what executable path Runtime will deliberately launch.

They do not prevent that launched process from invoking other executables/files if its OS
authority permits.

Therefore:

```text
ExecutableAllowlist != ProcessSandbox
```

---

# 122. Same distinction applies to Workspace scope

`cwdRelative` and Workspace path authority constrain Runtime's own launch/mutation APIs.

They do not automatically chroot the target.

---

# 123. Physical realization can widen semantic authority beyond API schema

A request may name one executable and one cwd, but the executable can:

```text
fork
exec other binaries
open arbitrary permitted files
call network APIs
```

This is why RF6 called authority the closure of reachable mechanisms.

---

# 124. Isolation seeks to constrain that closure physically

A true sandbox cannot rely only on the Agent promising not to use forbidden paths; it must
remove/restrict the reachable mechanisms themselves.

---

# 125. Capability systems provide one competing root model

Instead of ambient namespaces/path permissions, a capability-oriented design gives explicit
references/rights and derives reachability from possession.

Runtime's InputAuthority/immutable-input model already partially moves this direction for
inputs, without turning all execution into a capability OS.

---

# 126. Capability isolation is still domain-specific

Possessing only explicit file capabilities does not necessarily constrain network or CPU.

Again, isolation remains vector-valued.

---

# 127. Hostile sandboxing is not automatically a Runtime responsibility

If Edge/Workstation/remote worker already owns strong VM/container isolation, duplicating it
inside Runtime can create layered complexity and unclear authority.

The thin-core answer is often to bind/prove the external provider contract instead.

---

# 128. But Runtime must not overclaim external provider guarantees

Runtime may report only evidence/contract properties actually supplied and validated by that
provider.

It should not infer “secure” merely from provider name.

---

# 129. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| trusted_local writes another accessible Workspace | Workspace identity = isolation | API/workspace scope is not OS sandbox |
| reservation blocks Runtime mutate | coordination = security | out-of-band authority can still write |
| contained_local drops UID only | non-root = contained | multiple additional dimensions matter |
| private network enabled | one namespace = sandbox | network isolated, other dimensions may remain shared |
| hidden `/home` | hidden path = total confidentiality | secrets can exist in env/FD/network/process channels |
| read-only base FS | integrity = confidentiality | reads/exfiltration may remain |
| no capabilities | no privileged caps = no authority | ordinary UID permissions still exist |
| P4 mount namespace | host path witness = target view | namespace-relative resolution differs |
| same non-root UID Jobs | non-root = per-Job process isolation | signaling/ptrace/file access may remain possible |
| cgroup MemoryMax | budget = sandbox | availability bounded, file/network secrecy unchanged |
| process group killed | descendants dead = effects contained | external Effects survive |
| Windows Job Object | process ownership = sandbox | no inherent file/network/credential isolation |
| Windows limited token | non-elevated = AppContainer | normal user authority remains broader |
| immutable input tree | input frozen = whole environment frozen | ambient deps/kernel/time remain |
| elevated admin can alter Windows input tree | read-only = global immutable | guarantee is child-authority scoped |
| shared cache | cache = performance only | integrity/confidentiality/persistence coupling exists |
| root supervisor + dropped child | privilege separation = separate kernel | shared-kernel attack surface remains |
| container namespaces | container = VM | shared kernel and different failure domain |
| VM with networking+clipboard | VM = zero channels | configured sharing weakens dimensions intentionally |
| Access/Bearer auth | authenticated caller = sandboxed execution | ingress auth and target isolation are separate |
| executable allow-root | launch policy = target confinement | child can access other permitted OS resources |
| path witness alarm | detection = prevention | detective controls do not create isolation |
| external provider named “sandbox” | name = security contract | threat model/mechanisms/evidence required |

---

# 130. RF13 anti-laws

RF13-L1: `SandboxLabel != IsolationContract`.

RF13-L2: `Isolation != Atomicity`.

RF13-L3: `Isolation != Immutability`.

RF13-L4: `Isolation != Determinism`.

RF13-L5: `LifecycleContainment != EffectContainment`.

RF13-L6: `AuthorityReduction != CompleteIsolation`.

RF13-L7: `Trusted != Safe`.

RF13-L8: `Reservation != SecurityIsolation`.

RF13-L9: `SourceCommitment != WorkspaceIsolation`.

RF13-L10: `NamespaceExists != AllResourcesIsolated`.

RF13-L11: `NonRootUID != UserNamespaceIsolation`.

RF13-L12: `NoNewPrivileges != Sandbox`.

RF13-L13: `NoCapabilities != NoAuthority`.

RF13-L14: `HiddenPath != SecretIsolation`.

RF13-L15: `ImmutableInput != IsolatedExecution`.

RF13-L16: `ImmutableInputClosure != EnvironmentClosure`.

RF13-L17: `ReadOnlyForChild != ImmutableAgainstAdministrator`.

RF13-L18: `ResourceLimit != SecurityIsolation`.

RF13-L19: `ProcessTreeOwnership != HostileCodeSandbox`.

RF13-L20: `KillAllChildren != ContainAllEffects`.

RF13-L21: `LimitedToken != AppContainerIsolation`.

RF13-L22: `MoreControlAuthority != MoreTargetIsolation`.

RF13-L23: `Namespace/ContainerBoundary != VMBoundary`.

RF13-L24: `VM != ZeroInteraction`.

RF13-L25: `NoNetwork != FullIsolation`.

RF13-L26: `IntegrityIsolation != ConfidentialityIsolation`.

RF13-L27: `FilesystemIsolation != AvailabilityIsolation`.

RF13-L28: `IngressAuth != ExecutionIsolation`.

RF13-L29: `Kill != UndoInformationFlow`.

RF13-L30: `Observation != Prevention`.

RF13-L31: `Witness != IsolationBoundary`.

RF13-L32: `ExecutableAllowlist != ProcessSandbox`.

RF13-L33: `PolicyBoundary != PhysicalIsolationBoundary`.

RF13-L34: `PrivilegeSeparation != KernelIsolation`.

---

# 131. Minimum RF13 grammar

## 131.1 Isolation `Iso(A,B,D,T)`

Subject A is prevented from unpermitted observation/interference with B across dimension D
under threat model T.

## 131.2 Isolation vector `I`

Set of dimension-specific isolation properties.

## 131.3 Non-interference `NI(A→B,D)`

A cannot influence/reveal protected dimension D of B beyond allowed channels.

## 131.4 Containment `Contain(A,S)`

Execution/effect propagation from A is bounded to set S under mechanism/contract.

## 131.5 Authority `Auth(A)`

Reachable action/resource set available to A.

## 131.6 Attenuation `Auth'(A) ⊂ Auth(A)`

Reduced authority envelope.

## 131.7 Trust `Trust(X,T)`

Assumption X behaves within envelope under threat model T.

## 131.8 Threat model `T`

Actors, capabilities, attacks and excluded assumptions relevant to claim.

## 131.9 Security boundary

Enforcement boundary intended to resist T.

## 131.10 Blast radius `BR(A)`

Maximum reachable consequence set under effective authority.

## 131.11 Preventive / detective / corrective control

Mechanisms that block, observe or remediate behavior respectively.

---

# 132. Current Runtime isolation map

| Current mechanism | RF13 interpretation |
|---|---|
| `trusted_local` | broad owner-trusted service-user authority; not sandbox |
| `contained_local` | composite partial isolation + ambient-authority reduction for local engineering workloads |
| Workspace reservation | Runtime coordination/single-writer invariant, not OS security boundary |
| sourceStateDigest/pre-spawn check | source identity/precondition evidence, not immutable Workspace isolation |
| host dependency path witness | detective continuity evidence in Runtime host namespace, not prevention |
| immutable input materialization | strong scoped input-integrity/read-only boundary |
| Linux non-root payload UID/GID | authority reduction |
| `PR_SET_NO_NEW_PRIVS` | privilege-escalation attenuation |
| capability removal | privileged-operation attenuation |
| private network | network-view/effect isolation dimension |
| read-only/hidden mounts | filesystem integrity/visibility restrictions for that mount view |
| filtered environment | ambient credential/configuration reduction |
| cgroup MemoryMax/TasksMax/CPUQuota | availability/resource containment |
| systemd Attempt cgroup | lifecycle/process-tree ownership + resource control |
| per-step process group | local descendant timeout cleanup |
| Windows Job Object | native lifecycle/process-tree/resource containment |
| Windows limited token | native authority attenuation, not AppContainer |
| Windows elevated token | explicit broader native authority |
| protected Windows immutable inputs | child-scoped read-only input authority boundary |
| Agent executable roots | Runtime launch policy, not target process confinement |
| MCP Bearer/Access | ingress authentication/gating, not execution isolation |
| external Edge/VM/AppContainer | potential stronger hostile-code isolation owner outside current Runtime |

---

# 133. What RF13 strongly supports in current design

## 133.1 Keep `trusted_local` honest and broad

It should remain explicitly owner-trusted and effect-opaque rather than hiding broad authority
behind a sandbox label.

## 133.2 Keep `contained_local` narrow and dimension-specific

Its current private-network, capability, no-new-privileges, filesystem-view, UID and
environment restrictions deliver substantial accidental-safety value without hostile-code
claims.

## 133.3 Keep lifecycle containment separate from hostile security

systemd/cgroup and Windows Job Object are excellent process-tree ownership/control substrates;
that does not need to be exaggerated into file/network/credential isolation.

## 133.4 Keep immutable-input claims input-scoped

They are strong precisely because they do not claim whole-world immutability.

## 133.5 Use external isolation owners for hostile/multi-tenant workloads

Binding/proving a VM/AppContainer/sandbox provider is more consistent with thin-core Runtime
than recreating every security mechanism inside Core.

---

# 134. What RF13 reopens

## 134.1 Profile semantics should eventually be machine-legible as dimensions

A future `runtime.describe` could project explicit proven dimensions rather than relying only
on names such as `contained_local`, if real consumers need automated isolation selection.

RF13 does not authorize this implementation.

## 134.2 Same-UID cross-Job interference deserves measurement before hostile workloads

Current profile is not intended for malicious tenants, but real experiments could quantify
signal/ptrace/proc/cache/file reachability to document accidental-blast-radius boundaries more
precisely.

## 134.3 Controlled egress remains a separate future profile/provider concern

Private/no network and unrestricted network are not the same as destination-scoped egress.

## 134.4 External isolation provider commitment may deserve a structured contract

If Runtime later dispatches to VM/AppContainer/remote sandbox providers, provider identity,
isolation profile/version and evidence should participate in Operation identity/recovery.

---

# 135. Research constitution added by RF13

1. Never use one-bit sandbox claims; name isolation dimensions and threat model.
2. Treat isolation as non-interference over specified resources/channels, not a vague feeling
   of separation.
3. Keep integrity, confidentiality and availability isolation distinct.
4. Keep isolation separate from atomicity, immutability and determinism.
5. Distinguish containment of process lifetime/resources from containment of external Effects.
6. Treat authority attenuation as valuable even when it does not reach hostile-code
   isolation.
7. Treat trust as an assumption, not an enforcement mechanism.
8. Keep `trusted_local` explicitly owner-trusted broad authority.
9. Keep `contained_local` explicitly partial/dimension-scoped and non-hostile.
10. Do not reinterpret Workspace reservation or API path scopes as physical security
    isolation.
11. Treat namespaces as resource-specific isolation mechanisms, not universal sandboxes.
12. Keep mount/path evidence namespace-relative.
13. Treat UID drop, no-new-privileges and capability removal as distinct authority controls.
14. Do not equate hidden/read-only filesystem views with complete confidentiality.
15. Treat environment filtering as ambient secret/authority reduction, not secret brokerage.
16. Keep immutable inputs scoped to committed input integrity/read-only semantics.
17. Scope read-only claims to the actor/authority they actually constrain.
18. Treat cgroup/process-group/Job Object controls as lifecycle/resource containment unless a
    separate security mechanism proves more.
19. Do not infer external-effect containment from child-process cleanup.
20. Distinguish Windows limited token from AppContainer/LPAC security isolation.
21. Distinguish namespace/container shared-kernel isolation from separate-kernel VM
    isolation.
22. Treat shared caches/persistence as explicit cross-execution coupling dimensions.
23. Do not claim multi-tenant isolation under current owner-trusted Runtime.
24. Treat root/elevated supervisor authority as part of the TCB and blast-radius analysis.
25. Use preventive/detective/corrective terminology so witnesses are not mistaken for
    isolation.
26. Keep ingress authentication/authorization separate from realized execution isolation.
27. Prefer stronger external isolation owners for hostile/disposable workloads rather than
    bloating Runtime Core without a proven requirement.
28. Bind isolation-provider identity/version when a future Operation materially depends on
    that boundary.
29. Governing law: `IsolationClaimScope <= EnforcedBoundaryScope`.
30. Do not enter RF14 until it is clear which execution-world dimensions are frozen,
    restricted, shared or open.

---

# 136. RF13 result — conceptual compression

```text
Trust says who/what we assume will behave.
Authority says what a subject can reach/do.
Authority reduction shrinks that set.
Isolation prevents specified cross-boundary influence/observation under a threat model.
Containment bounds propagation/lifetime/resources but may not block already-created external
Effects.
A sandbox is a bundle of these mechanisms and must be described as an isolation vector, not
a boolean.
```

Strongest Runtime-specific compression:

```text
trusted_local:
  owner-trusted broad local authority
  supervision/evidence, not hostile sandboxing

contained_local:
  partial authority reduction + network/filesystem/privilege/environment restrictions
  useful against accidental local interference
  NOT kernel-grade hostile-code/multi-tenant isolation

systemd/cgroup + Windows Job Object:
  strong process-tree/lifecycle/resource containment
  NOT generic file/network/credential sandbox

immutable inputs:
  strong input-integrity boundary
  NOT whole-environment isolation

hostile/disposable/multi-tenant execution:
  requires stronger external isolation owner
```

The central theorem is:

```text
IsolationClaimScope <= EnforcedBoundaryScope
```

This is the security analogue of RF11 evidence scope and RF12 atomicity scope.

---

# 137. RF13 closeout

RF13 closes with thirty durable results:

1. **Isolation, containment, authority reduction, trust and sandbox are distinct concepts.**
2. Isolation is dimension- and threat-model-scoped non-interference, not a scalar boolean.
3. Integrity, confidentiality and availability isolation are independent axes.
4. Isolation is distinct from atomicity, immutability and determinism.
5. Containment bounds propagation/lifetime/resources; lifecycle containment does not imply
   external-effect containment.
6. Authority attenuation shrinks reachable consequences but does not automatically create
   complete isolation.
7. Trust is an assumption about behavior under retained authority, not an enforcement
   mechanism; trusted code can still fail accidentally.
8. Current `trusted_local` is correctly modeled as broad owner-trusted service-user local
   authority and must not be called hostile-code sandboxing.
9. Workspace reservation/source commitment/API path scopes provide coordination/precondition
   boundaries, not per-Workspace kernel security isolation.
10. Current `contained_local` is a composite partial isolation + authority-reduction profile
    using private network, capability removal, no-new-privileges, filesystem restrictions,
    payload credential drop and environment reduction.
11. **Current contained-local wording correctly rejects kernel-grade hostile-code,
    multi-tenant, complete-confidentiality, controlled-egress and disposable-machine claims.**
12. Linux namespaces isolate specific global resource views; presence of one namespace does
    not imply complete isolation.
13. Mount namespaces make pathname resolution namespace-relative; P4 is both an epistemic and
    authority-boundary falsifier.
14. Non-root UID, user namespace, no-new-privileges and capability removal are distinct
    controls and must not be collapsed.
15. Read-only/hidden filesystem views improve integrity/visibility only for their specific
    mount/path channels and do not imply secret isolation.
16. Environment filtering reduces ambient credential/configuration exposure but is not a
    secret broker.
17. Immutable-input materialization is a strong scoped input-integrity/read-only boundary and
    deliberately not an environment-closure claim.
18. Read-only guarantees are actor-scoped; elevated/root administrators may remain outside
    the constrained authority set.
19. cgroup budgets improve availability/resource isolation while systemd Attempt cgroups and
    process groups provide process-tree lifecycle ownership/cleanup.
20. **Killing all owned descendants does not undo or contain external Effects already
    emitted.**
21. Windows Job Objects provide group process management, limits/accounting and
    KILL_ON_JOB_CLOSE lifecycle containment but not generic file/network/credential
    isolation.
22. Windows limited token is authority attenuation and is materially weaker/different from
    AppContainer isolation; elevated token is broader authority, not stronger isolation.
23. AppContainer demonstrates capability/default-deny per-application file/network/process/
    credential isolation, while VM/Windows Sandbox demonstrates a separate-kernel
    hardware-virtualized boundary.
24. Namespace/container and VM boundaries have different TCB/failure domains; neither should
    be reduced to one generic “sandbox” category.
25. Shared caches and persistent state are explicit cross-Job coupling dimensions affecting
    integrity, confidentiality, nondeterminism and availability.
26. Current owner-trusted single-operator architecture is qualitatively different from
    mutually hostile multi-tenant execution; current Runtime explicitly does not support the
    latter.
27. Root/elevated supervision combined with non-root payload execution is useful privilege
    separation but still shares a kernel/TCB.
28. Detective controls such as path witnesses improve evidence/recovery but are not
    preventive isolation boundaries.
29. Strong hostile/disposable execution should normally be delegated to an external
    isolation owner whose provider/profile identity can be bound into Runtime Operations,
    rather than silently expanding current profile claims.
30. **The governing RF13 law is `IsolationClaimScope <= EnforcedBoundaryScope`.**

The next frontier is:

> **If authority, inputs and isolation boundaries are now explicit, why can repeated
> executions still differ? What is determinism versus reproducibility, what counts as
> nondeterministic dependency, and what can Runtime actually freeze or replay?**

That is RF14 — Determinism, Nondeterminism and Agent Action.
