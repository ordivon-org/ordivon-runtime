---
schema_version: 1
id: runtime.foundations.rf6.continuation
title: RF6 Continuation — Enter RF7
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
summary: Cross-conversation continuation checkpoint after RF6, preserving authority/capability/permission/control distinctions and the exact RF7 frontier on Boundary, Environment, Context and Dependency.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf6
  - runtime.foundations.rf6.sources
---
# RF6 Continuation — Enter RF7

## Completed

RF6 — Authority, Capability, Permission and Control.

Primary record:

`research/foundations/RF6-AUTHORITY-CAPABILITY-PERMISSION-CONTROL.md`

Source map:

`research/foundations/RF6-SOURCES.md`

## Durable results to carry forward

1. `Ability != Permission != EffectiveAuthority != Control`.
2. Authentication establishes identity/security claims; Authorization decides whether a
   concrete access/operation may proceed under an enforcement domain.
3. Principal is an authorization/accountability namespace identity, not necessarily RF2
   Human Actor, Goal Owner, Intender, Proposer or Executor.
4. Keep ordinary capability/affordance, Linux privilege capabilities and object-capability
   authority separate.
5. Effective authority can differ from potential/inheritable/ambient authority.
6. Ambient authority is authority inherited from execution context/global namespaces;
   explicit capability-like delegation can narrow object/action scope.
7. Bearer credential possession can confer exercisable authority under the verifier/
   resource-server contract without establishing semantic authorship.
8. Delegation and attenuation transfer/narrow authority without requiring identity or
   Goal-ownership transfer; Delegation != Impersonation.
9. Same user/principal identity can operate under different effective authority contexts.
10. Current Ordivon `principal` is server-bound from `ORDIVON_PRINCIPAL` into Runtime
    requests and functions as request/idempotency/audit attribution; it is not a generic
    authenticated OS/end-user principal.
11. Current MCP authentication source (local Bearer, remote Bearer, Cloudflare Access) is
    separate from current configured Runtime principal.
12. Current Bearers are coarse broad Runtime authority gates suitable to the current
    single-owner/trusted-client model, not per-resource least-authority grants.
13. `trusted_local` is broad inherited service-user local execution authority;
    `contained_local` is ambient-authority attenuation, not hostile-code isolation.
14. Windows limited/elevated is effective target execution-authority selection and
    materially participates in Operation identity.
15. `InputAuthority` is operator-owned explicit namespace authority; Agent selects an
    object within it but does not author the root authority.
16. Input authority names, ForeignReferences and HostDependencies are not credentials or
    authorization grants.
17. Runtime admission, OS execution authority and external provider/domain authorization
    are separate enforcement layers.
18. Runtime process-tree control does not imply external Effect rollback/control.
19. Revocation is primarily prospective; historical authority facts and current
    authorization decisions are distinct.
20. Least authority must preserve enough execution, observation and recovery authority for
    safe realization/reconciliation.

## RF6 compact grammar

```text
Able_E(A,X)                    technical/physical ability
Principal P                    authorization/accountability subject identity
AuthN(P,evidence)              authenticated security/identity context
Permitted_Π(P,X,R,C)           normative policy relation
AuthZ                          enforcement decision
A_eff                          effective authority recognized at point of use
Credential / Capability Token  material/reference carrying identity/authority semantics
Delegation / Attenuation       derived/narrow authority flow
A_ambient                      inherited/global-context authority
Control                        effective power to steer/constrain transitions
Revocation                     prospective invalidation under authority contract
```

## Current implementation interpretation

```text
MCP Bearer / Access assertion  ingress auth + broad surface authority gate
ORDIVON_PRINCIPAL              configured Runtime attribution/request principal
principal field                request/idempotency/audit namespace coordinate
trusted_local                  inherited service-user execution authority
contained_local                reduced ambient authority profile
Windows limited/elevated       effective Windows token authority class
InputAuthority                 operator-owned explicit input namespace authority
InputBindingRequest            request for one object inside that authority
ForeignReference               identity/dependency reference, not credential
HostDependency                 prerequisite commitment, not access authority
process-tree ownership         Runtime local realization control
provider credential            external-domain authority outside generic Runtime truth
```

## RF7 exact frontier

Enter:

# RF7 — Boundary, Environment, Context and Dependency

Core questions:

```text
What is a Runtime boundary: ownership boundary, authority boundary, observation boundary,
address-space/process boundary, filesystem namespace boundary, provider boundary or many
orthogonal boundaries?
What is Environment versus State versus Context?
Which facts are inputs and which are dependencies?
What makes a dependency explicit, implicit, ambient, transitive or hidden?
What is dependency closure and can an open-world execution ever have complete closure?
What is an execution environment: bytes, OS/kernel semantics, dynamic libraries,
filesystem namespace, locale, clock, network, credentials, device/GPU state?
What does isolation actually isolate: namespace, authority, resources, information flow,
failure, scheduling, determinism?
What is a sandbox versus container versus VM versus process boundary?
How do paths/references cross namespaces and become different objects?
What belongs in Operation identity versus evidence versus external current context?
What can be frozen, witnessed, materialized, virtualized or only observed?
Where does Runtime's ownership end and Host/provider/world context begin?
```

Mandatory falsifiers:

```text
same executable and source under different shared-library version
same host pathname but target mount namespace resolves different bytes
same environment variables but different kernel behavior
same container image but different host kernel
same network endpoint name resolving to different remote service
locale/timezone changes output
clock/network/randomness changes behavior
GPU driver/device availability changes execution
secret/credential availability changes reachable effects
contained process shares host kernel
VM isolates kernel but still depends on hypervisor/hardware/network
immutable input bytes with mutable external database
undeclared transitive dependency
symlink/path race versus already-open file descriptor
DNS/cache/service discovery drift
process boundary with inherited file descriptors
```

Do not enter RF8 Resource/Capacity/Constraint until RF7 separates environment/context,
boundary/isolation and dependency closure sufficiently that “resource” is not confused
with arbitrary environmental fact or authority boundary.
