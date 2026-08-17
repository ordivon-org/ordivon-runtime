---
schema_version: 1
id: runtime.foundations.rf13.continuation
title: RF13 Continuation — Enter RF14
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
summary: Cross-conversation checkpoint after RF13, preserving isolation/containment/trust distinctions and the exact RF14 frontier on Determinism, Nondeterminism and Agent Action.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf13
  - runtime.foundations.rf13.sources
---
# RF13 Continuation — Enter RF14

## Completed

RF13 — Isolation, Containment and Trust.

Primary record:

`research/foundations/RF13-ISOLATION-CONTAINMENT-TRUST.md`

Source map:

`research/foundations/RF13-SOURCES.md`

## Durable results to carry forward

1. Isolation, containment, authority reduction, trust and sandbox are distinct.
2. Isolation is dimension- and threat-model-scoped non-interference, not a one-bit property.
3. Integrity, confidentiality and availability isolation are independent axes.
4. Isolation is distinct from atomicity, immutability and determinism.
5. Containment bounds process/effect propagation, lifetime or resources; lifecycle
   containment does not imply containment/undo of external Effects.
6. Authority attenuation shrinks the reachable consequence set but does not automatically
   create complete hostile-code isolation.
7. Trust is a behavior assumption under retained authority, not an enforcement mechanism.
8. Current `trusted_local` is broad owner-trusted installed-service-user authority and must
   not be called a hostile sandbox.
9. Workspace reservations/source commitments/API path restrictions are coordination,
   precondition and policy boundaries rather than per-Workspace kernel isolation.
10. Current `contained_local` is a composite partial isolation/authority-reduction profile:
    private network, capability removal, no-new-privileges, filesystem visibility/write
    restrictions, non-root payload identity, environment reduction and narrower cache paths.
11. Current contained-local correctly refuses kernel-grade hostile-code, multi-tenant,
    complete confidentiality, controlled-egress and disposable-machine claims.
12. Linux namespaces isolate specific resource classes; a namespace is not universal
    isolation.
13. Mount namespaces make path resolution namespace-relative; P4 is both evidence and
    isolation/authority falsification.
14. Non-root UID, user namespace, no-new-privileges and capability removal are distinct
    controls.
15. Read-only/hidden mount views constrain particular filesystem channels and do not imply
    complete secret isolation.
16. Environment filtering reduces ambient credential/configuration authority but is not a
    secret broker.
17. Immutable inputs are a strong scoped input-integrity/read-only boundary rather than an
    environment-closure/isolation claim.
18. Read-only/security guarantees are actor-scoped; root/elevated administrators may remain
    outside the boundary.
19. cgroup resource limits improve availability isolation; systemd cgroups/process groups and
    Windows Job Objects primarily provide lifecycle/resource containment.
20. Killing owned descendants does not undo information flow or external Effects already
    emitted.
21. Windows limited token is authority attenuation; it is materially different from
    AppContainer/LPAC isolation. Elevated token is broader authority, not stronger isolation.
22. AppContainer demonstrates default-restricted per-application file/network/process/
    credential isolation; VM/Windows Sandbox demonstrates a hardware-virtualized separate-
    kernel boundary.
23. Namespace/container shared-kernel and VM separate-kernel boundaries have different TCBs
    and failure domains.
24. Shared caches/persistent state are explicit cross-Job coupling dimensions affecting
    integrity, confidentiality, availability and later determinism.
25. Current owner-trusted architecture is qualitatively different from mutually hostile
    multi-tenant execution; the latter is explicitly unsupported.
26. Root/elevated supervision plus non-root child is useful privilege separation but remains
    same-kernel.
27. Detective controls/witnesses are not preventive isolation.
28. MCP ingress authentication/authorization and target execution isolation are distinct.
29. Strong hostile/disposable workloads should normally use an external isolation owner whose
    provider/profile identity is bound/proven by Runtime rather than silently widening current
    profile claims.
30. Governing law: `IsolationClaimScope <= EnforcedBoundaryScope`.

## RF13 compact grammar

```text
Iso(A,B,D,T)       dimension/threat-scoped isolation
I                  isolation vector
NI(A→B,D)          non-interference
Contain(A,S)       propagation/lifetime/resource containment
Auth(A)            reachable authority set
Auth'(A)⊂Auth(A)   authority attenuation
Trust(X,T)         behavior assumption under threat model
TCB                trusted computing base
BR(A)              reachable blast radius
```

## Current implementation interpretation

```text
trusted_local               broad owner-trusted authority
contained_local             composite partial authority reduction/isolation
Workspace reservation       coordination, not security isolation
sourceStateDigest            source precondition/evidence, not snapshot isolation
host-dependency witness      detective continuity evidence
immutable input              scoped input-integrity boundary
non-root payload             authority attenuation
NoNewPrivileges              escalation attenuation
capability removal           privileged-operation attenuation
private network              network isolation dimension
RO/hidden mounts             filesystem-view integrity/visibility restrictions
filtered environment         ambient secret/authority reduction
cgroup budgets               resource/availability containment
systemd Attempt cgroup       lifecycle/process ownership
process group                local descendant cleanup
Windows Job Object           native lifecycle/resource containment
Windows limited token        authority attenuation
Windows elevated token       explicit broader authority
MCP auth                     ingress gating, not target sandbox
external VM/AppContainer     candidate stronger hostile isolation owner
```

## RF14 exact frontier

Enter:

# RF14 — Determinism, Nondeterminism and Agent Action

Core questions:

```text
What is determinism versus reproducibility versus repeatability?
Deterministic relative to which complete state/input boundary?
What is hidden/ambient nondeterministic state?
How do time, RNG, scheduling, filesystem ordering, ASLR, network responses and external
services enter execution behavior?
What does source/input freezing actually make reproducible?
Can identical Operation identity legitimately realize different outputs?
What must be captured to replay an execution?
What is replay of request versus replay of physical realization versus reproduction of
result?
What does seeded randomness guarantee and what remains uncontrolled?
What role do concurrency/races play?
Can Agent decisions be deterministic while tools/environment are not?
When should Runtime preserve nondeterministic evidence rather than trying to eliminate it?
What is hermetic execution and how does it differ from deterministic execution?
```

Mandatory falsifiers:

```text
same source + `date`
same source + `/dev/urandom`
same source + random seed but different thread schedule
same binary + network API whose response changed
same input + directory enumeration order
same test + race condition
same Agent prompt + stochastic model output
same model seed but provider/model version changes
immutable inputs + live package registry
contained_local with private network but wall clock/RNG still open
rebuild same commit on different toolchain/kernel/CPU
execPlan whose first step produces a timestamp consumed by second
same Workspace identity after mutable ignored cache changes
```

Do not enter RF15 External World/Open-System Effects until RF14 separates deterministic
mapping, reproducibility, hermeticity, replay and irreducible/open nondeterminism well enough
that “same execution” is not assumed to imply “same result.”
