---
schema_version: 1
id: runtime.foundations.rf6
title: RF6 — Authority, Capability, Permission and Control
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
summary: RF6 separates ability, mechanism-level capability, capability-based authority, principal identity, authentication, permission, authorization, delegation, effective execution authority, policy and control. It reclassifies current Ordivon principal as a Runtime request/audit namespace bound by the MCP server rather than an OS security principal; trusted_local/contained_local and Windows limited/elevated as effective execution-authority profiles; input authorities as operator-owned namespace capabilities; and external provider credentials as independent domain authority that Runtime cannot infer from local admission.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF6
related:
  - runtime.foundations.start
  - runtime.foundations.rf5
  - runtime.foundations.rf6.sources
  - runtime.foundations.rf6.continuation
---
# RF6 — Authority, Capability, Permission and Control

## 0. Status and research question

RF0–RF5 separated world/state, action/operation, realization/effect, identity/history and
time/order. RF6 now asks:

> **Who or what can cause an Operation to be admitted and realized? What is the difference
> between being physically able to do something, possessing a technical capability,
> being permitted by policy, being authorized on behalf of a principal, being delegated
> narrow authority, and actually controlling the intervention?**

RF6 is explanatory research only. It does not authorize replacing Runtime Bearers,
introducing a new capability-token protocol, renaming `principal`, changing execution
profiles, implementing per-client RBAC, or altering Windows elevation behavior.

---

# 1. First deletion — “can” is radically overloaded

The sentence:

```text
Actor A can do X.
```

may mean:

```text
A is physically/technically able to cause X
A possesses an OS privilege sufficient for X
A holds a reference/token accepted for X
policy permits A to request X
an authority has delegated X to A
A currently controls the mechanism that can cause X
A is likely competent enough to achieve X
```

These are different relations.

RF6 therefore rejects a universal `can(A,X)` predicate.

---

# 2. Ability

RF6 uses **Ability** in the weakest engineering sense:

> **A has ability for X under environment E if there exists an available course/mechanism
> by which A can successfully realize X under the modeled physical/technical constraints.**

Notation:

```text
Able_E(A, X)
```

Ability can exist even when X is forbidden.

Example:

```text
root process can overwrite /etc/shadow
but project policy forbids doing so
```

Therefore:

```text
Ability != Permission
```

---

# 3. Permission

RF6 defines **Permission** as a normative/policy relation:

> **Under policy/rule set P, subject/principal S is allowed to perform/request action X on
> object/resource R under conditions C.**

Compactly:

```text
Permitted_P(S, X, R, C)
```

Permission can exist while the subject lacks the practical mechanism to act.

Example:

```text
policy permits service S to restart service Q
but S lacks OS privilege / API credential
```

Thus:

```text
Permission != Ability
```

---

# 4. Authority

`Authority` is also overloaded. RF6 retains a broad systems meaning:

> **Authority is recognized power, under an enforcement/recognition regime, to cause,
> approve, delegate, constrain or bind decisions/effects concerning some resource/action
> scope.**

Authority therefore requires:

```text
subject/bearer
scope/object/action
recognizing/enforcing authority regime
conditions/lifetime
```

Authority is not a mystical property of an actor independent of a system that recognizes
it.

---

# 5. Authority versus Permission

Permission is a policy proposition:

```text
S may do X.
```

Authority is stronger/operationally richer:

```text
S possesses or is recognized with power that the enforcing system will honor for X.
```

A policy document can grant permission while the mechanism has not yet provisioned the
needed credential/rights.

Conversely a root process can possess effective authority beyond application policy.

Thus:

```text
Permission != EffectiveAuthority
```

---

# 6. Authentication

**Authentication** answers:

> **What identity/credential claim has been sufficiently established under authentication
> contract A?**

It does not answer whether the authenticated principal may perform a given operation.

NIST Zero Trust explicitly treats authentication and authorization as discrete functions,
and Windows separates authenticated account/security context from object-specific access
checks.

Therefore:

```text
Authentication != Authorization
```

---

# 7. Principal

RF6 defines **Principal** conservatively as:

> **An identity recognized by an authorization/accountability namespace as a subject on
> whose behalf rights, requests or audit records can be attributed.**

A principal can represent:

```text
human
service
machine
Agent
application identity
administrative role/account
```

But:

```text
Principal != HumanActor
Principal != Intender
Principal != Executor
```

RF2's role model remains necessary.

---

# 8. Authorization

RF6 defines **Authorization** as an enforcement decision/relation:

> **Given an authenticated/recognized subject context, operation, resource, policy and
> relevant facts, the authorizer determines whether the operation may proceed under its
> authority domain.**

Conceptually:

```text
Authorize(subject_context, operation, resource, policy, world_facts)
→ allow | deny | constrained decision
```

Authorization is not the same thing as authentication, and an allow decision does not
mean the operation is physically executable or will succeed.

Thus:

```text
AuthorizationSuccess != ExecutionSuccess
```

---

# 9. Credential

A **Credential** is evidence/material used to establish identity or authority claims under
some verifier contract.

Examples:

```text
password / certificate / signed assertion      authentication credential
OAuth access token                              authorization credential/token
Windows access token                            local security context object
API key / secret                                often authentication + authority bearer
```

The same word `token` can refer to very different semantics.

Therefore:

```text
Credential != Principal
Credential != Authority itself in every model
```

---

# 10. Capability is overloaded across disciplines

RF6 explicitly keeps three meanings separate.

## 10.1 Capability as ordinary ability

```text
system has capability to execute Windows workloads
```

This means `Ability/Affordance`.

## 10.2 Linux capability

Linux `CAP_NET_ADMIN`, `CAP_SYS_ADMIN`, etc. are decomposed privilege bits/sets used by
kernel permission checks.

This is a **specific OS privilege model**.

## 10.3 Object capability

In object-capability systems, possession of an unforgeable reference/handle carrying
rights provides authority to operate on a specific object, often enabling explicit
transfer/delegation and attenuation.

These meanings must not be collapsed.

---

# 11. Linux capabilities prove privilege can be decomposed from root

Linux historically distinguished root versus non-root. Modern Linux divides many root
privileges into independently enableable per-thread capability units.

Examples:

```text
CAP_NET_BIND_SERVICE
CAP_NET_ADMIN
CAP_SYS_CHROOT
CAP_SYS_BOOT
```

This proves:

```text
Privilege != UID==0 only
```

and demonstrates that effective authority can be factored into narrower mechanism-level
rights.

---

# 12. Linux effective/permitted/inheritable/bounding/ambient sets prove authority has state and transfer rules

Linux capabilities have several sets:

```text
permitted
effective
inheritable
bounding
ambient
```

The kernel uses the effective set for permission checks; bounding/inheritable/ambient
rules constrain how rights can cross `execve`.

RF6 lesson:

> **Possession, potential possession, effective activation and inheritance of authority
> are distinct states.**

Thus:

```text
PotentialPrivilege != EffectivePrivilege
```

---

# 13. Ambient authority

RF6 uses **ambient authority** for authority available to code because of its execution
context/environment rather than because the specific operation received an explicit,
operation-scoped authority reference.

Examples:

```text
service-user filesystem access
ambient environment credentials
PATH/global namespace lookup
inherited network reachability
root privileges inherited by arbitrary child code
```

Ambient authority is convenient but broadens what arbitrary target code can do.

---

# 14. Capability systems attack ambient namespace authority

Capsicum provides a particularly useful countermodel. Capability mode restricts access
to global namespaces and uses capability/refined file-descriptor handles with fine-grained
rights to interact with explicitly delegated objects.

The core lesson for RF6 is not “Ordivon should use Capsicum.” It is:

```text
Explicit object authority can be passed narrowly
instead of giving code ambient lookup authority over an entire namespace.
```

This makes authority flow more inspectable and attenuable.

---

# 15. Bearer capability/token possession

OAuth Bearer tokens show another authority style:

> possession of the token is sufficient to use the associated access rights; bearer use
does not require separate proof of a cryptographic private key by the presenter.

Therefore bearer security depends critically on preventing disclosure.

RF6 lesson:

```text
PossessionOfBearerCredential
→ exercisable authority under resource-server contract
```

which differs from an identity-only principal label.

---

# 16. Bearer possession is not semantic authorship

If Agent B receives a bearer credential originally provisioned for service A, B may be
technically able to exercise the credential's authority.

That does not make B:

```text
the original Goal Owner
the original authorizer
the resource owner
```

Therefore:

```text
CredentialBearer != AuthorityOriginator
```

and delegation/provenance should be tracked separately when it matters.

---

# 17. Delegation

RF6 defines **Delegation** as:

> **A relation/process by which an authority holder or recognized delegator enables
> another subject/mechanism to exercise some derived authority, possibly narrowed by
> scope, object, action, duration or conditions.**

Conceptually:

```text
Authority A
  --delegate subset/derived authority D-->
Subject B
```

Delegation does not require transferring every right of A.

---

# 18. Attenuation

**Attenuation** means deriving authority that is no greater than the source authority and
usually narrower.

Examples:

```text
read-only file descriptor from read-write authority
single-resource API token from account-wide credential
limited Windows token from broader administrator identity
one immutable input object from operator-owned input root
```

RF6 treats attenuation as a major design tool for Agent-first systems.

---

# 19. Delegation is not impersonation

Impersonation says:

```text
operate under another principal/security context
```

Delegation can instead say:

```text
B remains B, but B holds a derived authority granted by A
```

Windows explicitly supports thread impersonation tokens; capability systems often prefer
explicit delegated object authority.

Therefore:

```text
Delegation != Impersonation
```

---

# 20. Windows access tokens separate identity, group membership and privileges

Windows access tokens describe a process/thread security context and include:

```text
user SID
group SIDs
logon SID
privileges
restriction/impersonation information
```

Object access combines this security context with the target object's security descriptor
and access-control rules.

RF6 lesson:

```text
Principal identity alone does not determine effective access.
```

---

# 21. Restricted Windows tokens demonstrate authority reduction without identity replacement

Windows can create restricted tokens that disable/remove privileges or constrain SIDs
while preserving the same underlying user identity.

Thus:

```text
SameUserIdentity
+
DifferentEffectiveSecurityContext
→ DifferentAuthority
```

This directly supports Ordivon's distinction between identity and execution authority.

---

# 22. Windows elevation is effective execution authority, not semantic approval

Current Ordivon `windowsAuthority=elevated` selects an already-elevated provider token and
requires evidence of effective elevated/high/admin-enabled authority for the same SID.

RF6 interpretation:

> this is **effective execution-authority selection**.

It does not prove:

```text
Human semantically approved this Goal
Host endorsed the operation
external API accepts the action
```

Thus:

```text
WindowsElevation != SemanticAuthorization
```

---

# 23. Ability but forbidden — the root counterexample

Suppose Runtime runs target code as a service account/root-equivalent with broad local
power.

The target may be technically able to mutate resources that Runtime policy says it should
not touch.

Then:

```text
Able(target, X) = true
Permitted_RuntimePolicy(target/request, X) = false
```

If the execution boundary does not physically remove that authority, application policy
alone cannot prevent compromised target code from using ambient power.

This is the canonical:

```text
Ability > Permission
```

case.

---

# 24. Permitted but unable — the inverse counterexample

Suppose Runtime admission policy allows:

```text
call provider P
```

but the process lacks:

```text
network route
API credential
OS privilege
provider account permission
```

Then:

```text
Permitted = true
Able/EffectiveAuthority = false
```

The operation fails later.

Thus authorization cannot substitute for capability/effective authority verification.

---

# 25. Policy

RF6 defines **Policy** broadly as:

> **A set of normative/decision rules constraining which operations/authority transitions
> are allowed or required under declared facts and objectives.**

Policy can concern:

```text
who may act
which profiles may be requested
resource ceilings
risk constraints
approval/delegation rules
retention
revocation
```

But policy text/rules are not enforcement by themselves.

---

# 26. Policy enforcement point

An enforcement point is a mechanism where policy/authority checks actually constrain
possible system transitions.

Examples:

```text
Runtime admission
OS access check
Windows token selection
input-root resolver
remote resource server
broker/order venue
```

The same operation can cross multiple independent enforcement points.

---

# 27. Authorization is layered, not one global yes/no

A typical Ordivon flow can be:

```text
MCP ingress authentication
→ Runtime request binding
→ Runtime structural/admission policy
→ execution-profile/effective OS authority
→ target program's own policy
→ remote provider authentication/authorization
→ resource/effect-specific constraints
```

Passing one layer does not imply passing the next.

Therefore:

```text
RuntimeAdmissionAllow != ProviderAuthorizationAllow
```

---

# 28. Principal is not automatically an authenticated end-user identity in current Runtime

Current MCP behavior is important:

```text
HTTP request authenticated by local/remote Bearer or Cloudflare Access
```

then the tool/server binds a configured:

```text
ORDIVON_PRINCIPAL
(default principal:local-owner)
```

into `TaskRunRequest/Proposal`; clients do not author the Runtime principal through the
tool surface.

RF6 therefore classifies current `principal` as:

> **a server-bound Runtime request/idempotency/audit principal namespace**, not a general
> claim that Runtime authenticated a distinct external human/service identity matching
> that string.

This is a major anti-overclaim.

---

# 29. Current bearer authentication also does not create per-bearer Runtime principals

Current local and remote Bearers both authorize access to the Runtime MCP surface, while
the server uses one configured `ExecutionContext.principal` for requests.

Thus:

```text
AuthSource != RuntimePrincipalIdentity
```

at the current implementation layer.

This is valid for a single-owner trusted Runtime but should not be mistaken for multi-
principal authorization infrastructure.

---

# 30. Current Bearers are coarse authority gates

The packaged service documents that local/hosted-client Bearers remain full Runtime
authority rather than narrow per-tool/per-resource grants.

RF6 interpretation:

```text
Bearer possession
→ access to broad Runtime tool authority surface
```

subject to Runtime's internal admission constraints.

This is not object-capability-style least authority.

---

# 31. `trusted_local` is an execution-authority inheritance profile

Current docs explicitly state:

```text
trusted_local inherits the installed service user's local authority
```

RF6 therefore interprets `trusted_local` as:

> **execute target behavior while inheriting a broad ambient local authority context from
> the installed Runtime service execution environment.**

It is not merely a “trust level” label.

---

# 32. `contained_local` is authority attenuation, not hostile-code security

Current `contained_local` reduces ambient authority but explicitly does not claim
kernel-grade isolation against hostile code.

RF6 interpretation:

```text
contained_local = declared authority reduction profile
```

not:

```text
contained_local = arbitrary untrusted code sandbox
```

This distinction remains critical.

---

# 33. InputAuthority is a strong capability-like boundary

Current `InputAuthority` is operator-owned Core configuration:

```text
name → root directory handle/path authority
```

Agent requests only:

```text
a named authority
+
relative object
+
expected digest
```

and Runtime prevents `..` / symlink escape while materializing exact bytes.

RF6 interpretation:

> this is much closer to an **explicit delegated namespace authority** than ordinary
> arbitrary-path access.

The Agent does not receive authority to choose arbitrary host absolute paths inside that
mechanism.

---

# 34. Input authority name is not the authority itself

The string:

```text
"finance"
```

is only a reference/name resolved against operator-owned Core configuration.

The actual enforceable authority comes from Runtime's held root handle/configuration and
safe resolution mechanism.

Thus:

```text
AuthorityName != AuthorityPossession
```

---

# 35. ForeignReference is not an authority token

Current `ForeignReference` binds external identity/generation/digest facts into request or
operation identity.

It does not itself grant access to the foreign system.

Therefore:

```text
ForeignReference != Credential
ForeignReference != CapabilityToken
ForeignReference != Authorization
```

It is an identity/dependency/reference commitment coordinate.

---

# 36. HostDependency is also not authority

A committed host dependency says:

```text
this exact host path/digest is a declared prerequisite
```

It does not grant the target new rights over that path or prove the target can consume it.

Thus:

```text
DependencyCommitment != AccessAuthority
```

---

# 37. ExecutionProfile and WindowsAuthority belong to Operation identity for a reason

Current Runtime binds profile/target/elevated authority choices into request/operation
identity where semantically relevant.

RF6 gives the foundation reason:

> **Changing effective authority can change the set of reachable behaviors/effects even
> when executable/arguments are unchanged.**

Therefore:

```text
SameCommand + DifferentEffectiveAuthority
!= SameOperationalCommitment
```

for contracts where authority differs materially.

---

# 38. External provider authority remains independent

A target process may possess local authority and still be rejected remotely:

```text
HTTP 401/403
exchange permission denied
cloud IAM deny
expired API credential
resource-specific policy rejection
```

Therefore:

```text
LocalExecutionAuthority != ExternalDomainAuthority
```

RF3's Effect Owner remains the authority owner for external effects.

---

# 39. Credential possession can create external authority without Runtime understanding it

An arbitrary target may receive a secret via environment/file/agent setup and use it to
call an external API.

Runtime may know only:

```text
process executed
```

while the target internally exercises very high external authority.

Therefore:

```text
RuntimeExecutionEvidence != CompleteAuthorityInventory
```

This is another reason arbitrary execution remains effect-opaque.

---

# 40. Control

RF6 defines **Control** separately from authority:

> **Control is a relation in which a controller has effective means to select, constrain,
> stop, steer or otherwise influence behavior/transitions of a controlled system within
> some control scope.**

Examples:

```text
Runtime can start/kill its owned process tree
controller can select actuator input
provider can commit/reject resource mutation
human can revoke a credential
```

Control can exist without legitimate permission.

---

# 41. Authority is not control

A principal may be legally/policy-authorized to control a resource while currently
lacking network connectivity or mechanism access.

Conversely an attacker who steals a bearer token can exercise control/effective authority
without legitimate authorization from the original owner.

Therefore:

```text
Authority/Permission != ActualControl
```

---

# 42. Control is scoped and may be shared

Runtime may control:

```text
its process tree
```

while target code controls:

```text
application decisions
```

and external provider controls:

```text
resource commit
```

Meanwhile the OS kernel controls enforcement of process/filesystem/network rights.

There is no universal single controller.

---

# 43. Runtime cancellation demonstrates control without rollback authority

Runtime can often terminate its owned process tree.

That proves:

```text
Runtime has control over remaining local realization behavior
```

but not:

```text
Runtime has authority/control to reverse an external committed effect
```

This connects RF3:

```text
CancelExecution != RollbackEffect
```

to RF6 authority/control boundaries.

---

# 44. Admission authority versus dispatch authority

RF6 distinguishes temporal authority checkpoints.

At admission:

```text
is this operation allowed/valid to commit under Runtime contract?
```

At dispatch/start:

```text
is the committed effective execution context still realizable/provable?
```

At external effect:

```text
does the domain provider still authorize this request/effect?
```

These can differ because authority and world state may change over time.

---

# 45. Revocation

**Revocation** removes or invalidates previously recognized authority for future exercise
under a contract.

It raises a difficult temporal question:

```text
authority valid at admission
→ Operation committed
→ authority revoked
→ dispatch/effect not yet occurred
```

RF6 does not impose one answer for all systems.

Instead it requires the operation contract to specify which authority facts are:

```text
snapshotted/committed at admission
revalidated at dispatch
revalidated by effect owner at use time
```

---

# 46. Revocation cannot always undo already committed effects

If authority is revoked after:

```text
payment settled
order filled
message delivered
filesystem mutation committed
```

revocation cannot retroactively make the effect nonexistent.

Thus:

```text
AuthorityRevocation != EffectRollback
```

Revocation is primarily prospective unless the domain provides compensation/rollback.

---

# 47. Historical replay must not blindly re-adjudicate current authority

RF4 established:

```text
ReplayHistoricalTruth != ReinterpretCurrentWorld
```

RF6 extends this carefully:

> replaying/observing an already committed historical Operation can return its historical
> authority commitments without claiming that the same authority would be granted to a
> new Operation now.

Therefore:

```text
HistoricalAuthorityFact != CurrentAuthorizationDecision
```

---

# 48. But new physical re-execution may require current authority

If a future recovery design permits another Attempt after authority was revoked, the
system must explicitly decide whether:

```text
historical Operation commitment authorizes new realization
```

or:

```text
current authority must be revalidated
```

RF6 does not authorize multi-Attempt retry, but it identifies this as a mandatory RF10
question.

---

# 49. Least privilege versus least authority

`Least privilege` usually asks that subjects/processes receive no more privileges than
needed for their function.

Capability-system thinking sharpens this toward:

```text
least ambient authority
explicit narrowly delegated authority
```

RF6 retains both ideas while preferring the more operational question:

> **What is the smallest effective authority envelope needed by this exact realization,
> for this scope and lifetime?**

---

# 50. Least authority is not zero authority

A process that must write one file or submit one order needs enough authority to do it.
Over-attenuation produces unusable systems.

Thus the objective is not:

```text
minimize authority numerically at all costs
```

but:

```text
minimize unnecessary authority while preserving required capability and recoverability
```

---

# 51. Recoverability complicates authority minimization

Recovery may require authority to:

```text
inspect process state
query provider state
read receipts
terminate leaked descendants
repair bookkeeping
```

So an authority profile sufficient for forward execution may be insufficient for safe
reconciliation/recovery.

RF6 therefore introduces:

```text
ExecutionAuthority
RecoveryAuthority
ObservationAuthority
```

as potentially distinct projections.

---

# 52. Observation authority is real authority

Read-only access is still authority.

Examples:

```text
read process metadata
read secrets
query account balances
read filesystem content
inspect audit logs
```

RF6 rejects treating `read-only` as “no authority.”

Read authority may have major privacy/security consequences even when it cannot mutate.

---

# 53. Mutation authority and control authority can differ

A process may be allowed to write a file but unable to kill another process.

Runtime may be able to kill its process tree but not edit provider resources.

Thus authority should be object/action scoped, not described only by coarse labels like
`trusted` or `admin` when precision matters.

---

# 54. Authority closure matters for target code

If target code inherits:

```text
filesystem
network
credentials
process control
namespace manipulation
```

then the effective authority envelope is the closure of all available mechanisms, not
merely the declared tool call.

This explains why `trusted_local` remains effect-opaque and unsuitable for hostile code.

---

# 55. Ambient environment is part of authority surface

Environment variables can contain:

```text
credentials
proxy URLs
socket paths
configuration pointing to privileged services
```

So environment freezing/reduction is not only determinism work; it also affects authority
propagation.

However:

```text
EnvironmentDigest != CompleteAuthorityInventory
```

because kernel/process/network rights may exist independently.

---

# 56. Namespace access is authority

Ability to resolve an arbitrary pathname, service name, network endpoint or Registry key
can itself confer broad authority because it allows the target to discover/choose objects
at runtime.

Capsicum's denial of global namespace access in capability mode makes this explicit.

RF6 therefore treats namespace-resolution authority as a first-class authority dimension.

---

# 57. Current InputBinding architecture is an important least-authority success

Instead of giving Agent-authored target code arbitrary host input-path selection, Runtime
has:

```text
operator chooses named authority root
Agent chooses relative object within that authority
Runtime verifies exact digest
Runtime materializes frozen read-only presentation
```

This reduces:

```text
ambient namespace authority
TOCTOU exposure
Agent path-selection authority
```

while preserving the needed read capability.

---

# 58. Current Windows limited default is also an authority-minimization success

Windows native execution defaults to limited/non-elevated authority and requires explicit
`windowsAuthority=elevated` selection plus effective token evidence.

RF6 strongly supports the shape:

```text
narrower authority by default
broader authority explicit and identity-bound
```

while noting that current Windows execution remains `trusted_local` at the Runtime profile
layer rather than hostile-code containment.

---

# 59. Authority should be committed when it changes behavior space

If two otherwise identical Operations run under materially different effective authority,
their reachable behavior/effect spaces differ.

Therefore authority commitments can legitimately participate in Operation identity.

Current Windows elevated selection already follows this rule.

The same principle may later apply to other explicit authority profiles/credentials, but
RF6 does not mandate serializing secret material into identity.

---

# 60. Secret identity and secret value must remain distinct

A secret/API token may confer authority, but storing the raw secret in Operation identity,
logs or command args is unsafe.

A correct authority commitment may instead bind:

```text
credential reference/version/provider/account/scope
```

without recording secret bytes.

This is a future design constraint rather than current generic Runtime behavior.

---

# 61. Policy permission can be dynamic while historical commitment remains fixed

Examples of policy facts that can change:

```text
resource quota
allowed executable roots
Windows elevation availability
input authority roots
provider account permissions
```

For a new admission, current policy matters.

For an existing historical replay, RF4 says do not silently reinterpret the Operation.

Thus:

```text
NewAuthorizationDecision uses current policy
HistoricalReplay returns historical commitment
```

These are different operations.

---

# 62. Authority chain for delegated Agent execution

A useful RF6 role chain is:

```text
Goal Owner
   ↓ semantic delegation
Planner / Agent
   ↓ proposal authority
Runtime ingress/authentication
   ↓ request binding
Runtime Admitter / policy
   ↓ committed Operation authority envelope
Execution Provider / OS
   ↓ effective process authority
Target Program
   ↓ optional credential use
External Effect Owner
   ↓ domain authorization
External Effect
```

No layer's allow decision subsumes all later layers.

---

# 63. Principal/Actor/Authority roles can diverge

A single operation may have:

```text
Human             Goal Owner
Agent             Planner/Proposer
Runtime principal principal:local-owner
Runtime service   local OS authority holder
Windows token     effective target security context
API credential    remote account authority
broker             final Effect authorizer
```

Asking “who authorized it?” has no one-word answer unless the authority layer is named.

---

# 64. Accountability is not authorization

Recording:

```text
principal = P
```

supports attribution/audit/idempotency namespace.

It does not prove P had proper authority unless P's identity and authorization context are
mechanically bound by an authorization contract.

Thus:

```text
AuditAttribution != AuthorizationProof
```

This is particularly important for current Runtime principal semantics.

---

# 65. Authentication source is also not semantic Goal ownership

An MCP Bearer authenticates/authorizes access to the Runtime surface.

It does not prove the caller is the original Human Goal Owner or that every Agent decision
inside the authenticated session was explicitly approved by that human.

Therefore:

```text
AuthenticatedCaller != GoalOwner
```

RF2's delegated-action roles remain orthogonal.

---

# 66. Control plane versus data/effect plane authority

Runtime control-plane authority includes:

```text
admit
start
cancel
observe
reconcile
```

Target/effect-plane authority includes:

```text
filesystem mutation
network/API actions
provider domain effects
```

These can be held by different mechanisms.

This distinction helps explain why Runtime can cancel a Job yet remain unable to undo a
provider Effect.

---

# 67. Capability discovery/affordance is not authority grant

Current `runtime.describe` can expose capabilities/affordances such as supported targets,
profiles and authority classes.

This means:

```text
Runtime can currently support/admit this category under configuration
```

not:

```text
the caller has been granted authority to every advertised operation
```

Therefore:

```text
CapabilityDiscovery != AuthorizationGrant
```

---

# 68. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| root can mutate forbidden file | ability = permission | technically able but policy forbidden |
| policy allows restart but user lacks privilege | permission = ability | allowed yet mechanically incapable |
| Linux CAP_NET_ADMIN | privilege = root boolean | mechanism authority can be decomposed |
| Linux permitted vs effective capability sets | possession = active authority | potential/effective privilege differ |
| ambient Linux capability across exec | explicit operation grant = inherited authority | authority can propagate ambiently |
| Capsicum FD capability | identity principal = all authorization | explicit object reference can carry scoped authority |
| OAuth bearer stolen | legitimate principal = exercisable authority | bearer possession may be enough to exercise grant |
| Windows restricted token | same identity = same authority | same SID can run with reduced effective authority |
| Windows elevated token | elevation = semantic approval | execution authority differs from Goal authorization |
| MCP local/remote Bearer | auth source = Runtime principal | current principal is server-configured binding |
| same principal under different execution profile | principal = effective authority | process authority envelope can differ |
| input authority name | name = authority | enforcement comes from operator root + resolver |
| ForeignReference | external reference = credential | identity commitment does not grant access |
| HostDependency | prerequisite = permission | dependency truth does not confer access authority |
| provider 403 after local success | Runtime admission = domain authorization | downstream authority is independent |
| credential in target environment | Runtime knows process = Runtime knows authority | target may exercise opaque external authority |
| cancel local process after order fill | local control = rollback authority | process control does not imply effect reversal |
| authority revoked after admission | committed operation = perpetual grant | revocation/revalidation semantics are contract-specific |
| replay old Job after policy change | current deny rewrites history | historical authority commitment and new authorization differ |
| two equal commands limited/elevated | command = operation | authority changes behavior space and operation meaning |

---

# 69. RF6 anti-laws

RF6-L1: `Ability != Permission`.

RF6-L2: `Permission != EffectiveAuthority`.

RF6-L3: `Authentication != Authorization`.

RF6-L4: `Principal != HumanActor`.

RF6-L5: `Principal != Intender`.

RF6-L6: `Principal != Executor`.

RF6-L7: `AuthorizationSuccess != ExecutionSuccess`.

RF6-L8: `Credential != Principal`.

RF6-L9: `Capability(ordinary ability) != LinuxCapability != ObjectCapability`.

RF6-L10: `PotentialPrivilege != EffectivePrivilege`.

RF6-L11: `AmbientAuthority != ExplicitDelegatedAuthority`.

RF6-L12: `CredentialBearer != AuthorityOriginator`.

RF6-L13: `Delegation != Impersonation`.

RF6-L14: `SameUserIdentity != SameEffectiveAuthority`.

RF6-L15: `WindowsElevation != SemanticAuthorization`.

RF6-L16: `Policy != Enforcement`.

RF6-L17: `RuntimeAdmissionAllow != ProviderAuthorizationAllow`.

RF6-L18: `AuthSource != RuntimePrincipalIdentity` in current Runtime.

RF6-L19: `RuntimePrincipal != authenticated external end-user identity by default`.

RF6-L20: `trusted_local != hostile-code containment`.

RF6-L21: `contained_local != kernel-grade hostile-code sandbox`.

RF6-L22: `AuthorityName != AuthorityPossession`.

RF6-L23: `ForeignReference != Credential/Authorization`.

RF6-L24: `DependencyCommitment != AccessAuthority`.

RF6-L25: `LocalExecutionAuthority != ExternalDomainAuthority`.

RF6-L26: `RuntimeExecutionEvidence != CompleteAuthorityInventory`.

RF6-L27: `Authority/Permission != ActualControl`.

RF6-L28: `RuntimeProcessControl != ExternalEffectRollbackAuthority`.

RF6-L29: `AuthorityRevocation != EffectRollback`.

RF6-L30: `HistoricalAuthorityFact != CurrentAuthorizationDecision`.

RF6-L31: `ReadOnly != NoAuthority`.

RF6-L32: `EnvironmentDigest != CompleteAuthorityInventory`.

RF6-L33: `AuditAttribution != AuthorizationProof`.

RF6-L34: `AuthenticatedCaller != GoalOwner`.

RF6-L35: `CapabilityDiscovery != AuthorizationGrant`.

---

# 70. Minimum RF6 grammar

## 70.1 Ability `Able_E(A,X)`

Technical/physical possibility under environment E.

## 70.2 Principal `P`

Identity in an authorization/accountability namespace.

## 70.3 Authentication Context `AuthN(P,evidence)`

Established identity/security claim under an authenticator contract.

## 70.4 Permission `Permitted_Policy(P,X,R,C)`

Normative allow relation under policy.

## 70.5 Authorization Decision `AuthZ`

Enforcement decision combining subject context, operation/resource, policy and facts.

## 70.6 Effective Authority `A_eff`

Authority actually recognized/enforceable by the mechanism/resource at the point of use.

## 70.7 Credential / Capability Token `cred`

Evidence/reference whose possession/presentation may establish identity or authority under
its verifier contract.

## 70.8 Delegation `D(A→B,scope)`

Derived authority transfer/enablement relation.

## 70.9 Attenuation `Att(A,scope')`

Derive narrower authority.

## 70.10 Ambient Authority `A_ambient`

Authority inherited/available from execution context or global namespaces rather than
explicit operation-scoped grant.

## 70.11 Control `Ctrl(A,S,scope)`

Effective means to steer/select/constrain transitions of system S.

## 70.12 Policy `Π`

Normative/decision rules used at enforcement points.

## 70.13 Revocation `Revoke(authority,lifetime)`

Prospective invalidation/removal under contract.

---

# 71. Current Runtime authority map

| Current mechanism | RF6 interpretation |
|---|---|
| HTTP local/remote Bearer | coarse MCP surface authentication/authority credential |
| Cloudflare Access assertion | optional ingress authentication gate |
| configured `ORDIVON_PRINCIPAL` | server-bound Runtime request/idempotency/audit principal |
| `principal` in Job/request | attribution + request namespace, not OS effective user authority |
| `trusted_local` | broad inherited service-user local execution authority profile |
| `contained_local` | reduced ambient local authority profile, not hostile-code isolation |
| `windowsAuthority=limited` | reduced effective Windows token authority |
| `windowsAuthority=elevated` | explicit elevated effective Windows token authority |
| Windows token SID/class evidence | execution security-context evidence |
| executable allowed roots | Runtime admission namespace/policy constraint |
| InputAuthority | operator-owned explicit input namespace authority |
| InputBindingRequest | domain/Agent request for one object inside named authority |
| immutable read-only presentation | attenuated target read authority over materialized bytes |
| HostDependency | prerequisite identity/continuity commitment, not permission |
| ForeignReference | foreign identity/generation commitment, not credential |
| target environment/secrets | possible opaque authority propagation surface |
| Runtime process-tree ownership | local realization control authority |
| provider/API credential | external-domain authority outside generic Runtime knowledge |
| provider receipt/decision | external Effect-owner authorization/effect evidence |

---

# 72. What RF6 strongly supports in current design

## 72.1 Server binds principal rather than trusting Agent-authored principal

The MCP tool does not permit callers to arbitrarily choose the durable Runtime principal.
This prevents an obvious audit/idempotency namespace spoofing class within the current
single-owner model.

## 72.2 Narrow authority defaults

Windows limited by default and explicit elevated selection are well-founded.

## 72.3 Execution authority is part of operation semantics

Authority can change reachable behavior, so binding explicit authority choices into
identity is correct.

## 72.4 Input authorities are operator-owned

Agent requests one object inside a named operator root rather than declaring host root
authority itself.

## 72.5 Containment claims stay narrow

Current docs correctly avoid claiming hostile-code isolation from `contained_local`.

## 72.6 External authorization remains domain-owned

Runtime does not treat local execution/admission as proof that a remote provider will
honor an action.

---

# 73. What RF6 reopens

## 73.1 Current Bearers are broad

A hosted/local Bearer grants broad Runtime surface access. This is acceptable for current
single-owner/trusted-client deployment but is not a least-authority multi-principal model.

A future multi-agent/multi-user Runtime would need explicit principal/authentication/
authorization/delegation semantics rather than merely more Bearers.

## 73.2 `principal` naming may over-suggest security semantics

The current field is useful but should be documented as a server-bound Runtime principal
namespace, not necessarily an authenticated OS/domain principal.

RF6 does not authorize renaming it.

## 73.3 Secret delivery remains an authority gap

Current docs already warn that `contained_local` is not a secret broker. Future structured
credential delivery should preserve least authority, lifetime and non-leakage semantics.

## 73.4 Revocation semantics are not generic

Runtime currently freezes historical Operation identity, but external credentials/policy
can change. Any future redispatch/retry model must state exactly which authority facts are
historical commitments and which must be revalidated.

---

# 74. Research constitution added by RF6

1. Never use `can` without distinguishing ability, permission or effective authority when
   correctness depends on it.
2. Keep authentication and authorization separate.
3. Treat principal as an identity namespace role, not automatically Actor/Intender/
   Executor or OS privilege.
4. Name the enforcement/recognition domain for every authority claim.
5. Distinguish ordinary “capability,” Linux capability privilege bits and object-
   capability authority.
6. Prefer explicit/attenuated authority over unnecessary ambient authority.
7. Treat bearer credentials as exercisable authority material and protect them accordingly.
8. Track delegation separately from Goal ownership and impersonation.
9. Distinguish policy from enforcement mechanism.
10. Keep Runtime admission authority, OS execution authority and external provider
    authorization separate.
11. Treat read/observation access as real authority.
12. Treat process control as local realization control, not external effect rollback.
13. State revocation/revalidation timing explicitly across admission/dispatch/effect use.
14. Historical replay may report past authority commitments without making a new current
    authorization decision.
15. Capability discovery/affordance surfaces must not be confused with grants to a caller.
16. Do not claim complete authority closure for arbitrary target code unless filesystem,
    namespaces, credentials, network and downstream providers are all bounded by contract.

---

# 75. RF6 result — conceptual compression

RF6 reduces the old ambiguous question:

```text
Can this Agent do X?
```

into:

```text
Is X technically achievable under environment E?               Ability
Who is the recognized request/accountability subject?           Principal
How was that identity/security context established?             Authentication
Does policy allow X under current conditions?                   Permission
Did the enforcing system approve X?                             Authorization
What effective rights/credentials does execution actually hold? Effective Authority
Was that authority explicitly delegated/attenuated or ambient?  Delegation/Authority Flow
Who can steer/stop/commit the relevant transitions?             Control
Will the external Effect Owner independently accept it?         Domain Authorization
```

The strongest compression is:

> **Ability describes reachable behavior. Permission describes normative allowance.
> Authority describes recognized power under an enforcement regime. Authentication
> establishes identity claims; authorization decides access. Capabilities/credentials can
> carry exercisable authority. Delegation moves/attenuates authority. Control is actual
> influence over transitions. None of these alone substitutes for the others.**

---

# 76. RF6 closeout

RF6 closes with eighteen durable results:

1. **Ability, Permission, Authority and Control are distinct relations.**
2. Authentication establishes identity/security claims; Authorization evaluates whether
   an operation/resource access may proceed.
3. Principal is an authorization/accountability identity role and is not necessarily the
   Human Actor, Intender or Executor.
4. Capability is overloaded: ordinary ability, Linux privilege capabilities and object-
   capability authority must remain separate.
5. Linux capabilities demonstrate decomposed, stateful effective privilege below the
   root/non-root binary model.
6. Capability-system designs demonstrate the value of explicit object authority and
   reducing ambient global-namespace authority.
7. Bearer credentials demonstrate that possession itself can create exercisable authority
   under the verifier/resource-server contract.
8. Delegation and attenuation allow authority to move/narrow without transferring all Goal
   ownership or identity.
9. Same principal/user identity can execute under different effective authority contexts.
10. **Current Ordivon `principal` is a server-bound Runtime request/idempotency/audit
    principal, not a generic authenticated OS/end-user principal.**
11. Current MCP Bearer/Cloudflare authentication source is distinct from the configured
    Runtime principal; broad Bearers currently gate a single-owner Runtime authority
    surface.
12. `trusted_local` and `contained_local` are effective execution-authority profiles;
    contained-local is attenuation, not hostile-code isolation.
13. Windows limited/elevated is explicit effective execution-authority selection and
    correctly participates in operation identity where material.
14. `InputAuthority` is a strong operator-owned explicit namespace-authority mechanism;
    its name, ForeignReferences and HostDependencies are not themselves credentials.
15. Runtime admission, OS execution and external provider authorization are independent
    enforcement layers.
16. Process-tree control/cancellation does not grant external Effect rollback authority.
17. Revocation and replay require temporal semantics: historical authority facts can remain
    true while current authorization changes; new physical work may need fresh authority
    checks depending on contract.
18. Ordivon should minimize unnecessary ambient authority and prefer explicit, scoped,
    attenuated authority while retaining enough observation/recovery authority for safe
    reconciliation.

The next frontier is:

> **Where does an Operation actually live? Which facts belong to the boundary,
> environment, context and dependency closure of execution, and when does an omitted
> dependency make a result non-reproducible or an authority boundary porous?**

That is RF7 — Boundary, Environment, Context and Dependency.
