---
schema_version: 1
id: runtime.foundations.rf6.sources
title: RF6 Sources — Authority, Capability, Permission and Control
type: reference
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: Primary/canonical sources used by RF6 to separate authentication, authorization, principal/security context, Linux privilege capabilities, object capabilities, bearer authority, delegation/attenuation and Runtime effective authority.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf6
---
# RF6 Sources — Authority, Capability, Permission and Control

## S1 — Linux capabilities

Linux man-pages project, **capabilities(7)**.

Canonical upstream: Linux man-pages / kernel.org; current public rendering: man7.org.

RF6 use:

- Linux decomposes privileges traditionally associated with root into independently
  enabled/disabled per-thread capability units;
- effective, permitted, inheritable, bounding and ambient sets demonstrate that potential,
  effective and inherited privilege are distinct;
- ambient capability inheritance across `execve` is a concrete ambient-authority model;
- supports `Privilege != UID==0 only`, `PotentialPrivilege != EffectivePrivilege` and the
  need to model authority propagation.

## S2 — Microsoft Windows access tokens and access-control model

Microsoft Learn, **Access Tokens** and **Parts of the Access Control Model**.

Canonical source: Microsoft Learn Win32 Authorization documentation.

RF6 use:

- an access token describes a process/thread security context and includes user SID,
  groups, logon identity, privileges and restriction/impersonation information;
- securable-object access is evaluated using token/security-context facts together with
  the object's security descriptor;
- authentication creates security context, but effective object access remains an
  authorization/access-check question;
- supports `PrincipalIdentity != EffectiveAccess`.

## S3 — Microsoft restricted tokens

Microsoft Learn, **Restricted Tokens** / Windows Security Model.

Canonical source: Microsoft Learn.

RF6 use:

- restricted tokens can remove privileges or restrict SIDs while preserving the same
  underlying user identity;
- directly supports `SameUserIdentity != SameEffectiveAuthority` and authority attenuation
  without identity replacement.

## S4 — RFC 6750 bearer-token authority

M. Jones and D. Hardt, **RFC 6750 — The OAuth 2.0 Authorization Framework: Bearer Token
Usage**, IETF, 2012.

Canonical source: IETF Datatracker / RFC Editor.

RF6 use:

- possession of a bearer token is sufficient to exercise its associated protected-resource
  access without proving possession of separate cryptographic key material;
- access tokens represent authorization grants/scope and must be protected from
  disclosure;
- supports `BearerPossession → exercisable authority under resource-server contract` and
  the distinction between credential bearer and authority originator.

## S5 — Capsicum capability model

Robert N. M. Watson, Jonathan Anderson, Ben Laurie and Kris Kennaway,
**Capsicum: Practical Capabilities for UNIX**, USENIX Security 2010.

Canonical sources: USENIX proceedings and University of Cambridge Computer Laboratory
Capsicum project documentation.

RF6 use:

- capabilities are refined file descriptors with fine-grained rights;
- capability mode denies access to global namespaces and encourages explicit delegated
  object authority;
- demonstrates least/explicit authority and attenuation as alternatives to ambient global
  namespace authority;
- RF6 uses Capsicum as a competing authority model, not a mandate for Ordivon.

## S6 — NIST Zero Trust Architecture

NIST SP 800-207, **Zero Trust Architecture**, 2020; NIST SP 800-207A, 2023.

Canonical source: NIST CSRC.

RF6 use:

- authentication and authorization are discrete functions;
- access decisions should be resource/identity/policy oriented rather than inferred from
  network location alone;
- supports layered enforcement and the anti-collapse `Authentication != Authorization`.

## S7 — Current Ordivon Runtime as empirical source

RF6 audits current Core/MCP source and canonical docs.

### MCP authentication and principal binding

HTTP middleware recognizes:

```text
local Bearer
remote Bearer
optional Cloudflare Access assertion
```

as ingress authentication sources. Independently, ServerConfig constructs one
`ExecutionContext.principal` from `ORDIVON_PRINCIPAL` (default
`principal:local-owner`) and binds that server-side value into TaskRunRequest/Proposal and
Patch operations. The MCP client does not author `principal` on tool requests.

RF6 therefore interprets current `principal` as server-bound Runtime request/idempotency/
audit attribution, not as a generic authenticated external-end-user identity.

### Execution authority profiles

Canonical docs state:

```text
trusted_local   inherits installed service-user local authority
contained_local reduces ambient authority but is not hostile-code isolation
```

Windows native adds explicit `limited` versus `elevated` effective token classes, with
evidence checks on elevation/integrity/admin-group state.

### Input authority

`InputAuthority` is explicitly documented in Core as operator-owned configuration,
not Agent-authored path authority. Agent/domain input requests select one exact relative
object inside a named authority and an expected digest; Runtime performs bounded safe
resolution and materialization.

### Non-authority references

`ForeignReference` and `HostDependencyBinding` participate in identity/dependency
commitments but contain no credential or permission semantics.

### Current broad Bearers

`docs/data-and-privacy.md` states the hosted-client Bearer, when configured, is separate
from the trusted-local Bearer but remains full Runtime authority. RF6 therefore treats
current Bearers as coarse single-owner surface gates rather than per-resource least-
authority grants.

## Source discipline

Later rounds should preserve:

```text
ordinary capability != Linux privilege capability != object capability
authentication != authorization
principal identity != effective authority
permission != mechanism ability
authority label/name != enforceable authority possession
bearer credential possession may itself convey exercisable authority
ambient authority != explicit delegated authority
Runtime admission != external provider authorization
current Runtime principal remains a server-bound attribution/request namespace
```
