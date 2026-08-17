---
schema_version: 1
id: runtime.foundations.rf13.sources
title: RF13 Sources — Isolation, Containment and Trust
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
summary: Primary/canonical sources used by RF13 for Linux namespace semantics, Windows Job Objects and AppContainer/Windows Sandbox isolation, container-versus-VM threat models, and current Runtime profile/containment contracts.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf13
---
# RF13 Sources — Isolation, Containment and Trust

## S1 — Linux namespaces overview

Linux man-pages, **namespaces(7)**.

Canonical upstream: Linux man-pages project; public rendering:
`https://man7.org/linux/man-pages/man7/namespaces.7.html`.

RF13 use:

- namespaces wrap particular global resources so processes see isolated instances;
- namespace kinds isolate distinct resource classes rather than one universal security
  domain;
- supports `NamespaceExists != AllResourcesIsolated` and dimension-specific isolation.

## S2 — Linux mount namespaces

Linux man-pages, **mount_namespaces(7)**.

Canonical public rendering:
`https://man7.org/linux/man-pages/man7/mount_namespaces.7.html`.

RF13 use:

- mount namespaces provide distinct mount hierarchies/views;
- supports namespace-relative path/object resolution and the P4 falsifier;
- reinforces `SamePathString != SameResolvedObject`.

## S3 — Linux network namespaces

Linux man-pages, **network_namespaces(7)**.

Canonical public rendering:
`https://man7.org/linux/man-pages/man7/network_namespaces.7.html`.

RF13 use:

- network namespaces isolate devices, protocol stacks, routes/firewall/socket/port views and
  related network resources;
- supports private-network as a real isolation dimension while rejecting universal sandbox
  inference.

## S4 — Linux user namespaces

Linux man-pages, **user_namespaces(7)**.

Canonical public rendering:
`https://man7.org/linux/man-pages/man7/user_namespaces.7.html`.

RF13 use:

- user namespaces isolate UID/GID and capability-related security attributes;
- supports `NonRootUID != UserNamespaceIsolation` and the distinction between credential
  drop and namespace security semantics.

## S5 — Microsoft Windows Job Objects

Microsoft Learn, **Job Objects**.

Canonical source:
`https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects`.

RF13 use:

- Job Objects manage groups of processes as a unit;
- support resource limits/accounting and process-tree termination;
- `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` terminates associated processes when the last Job
  handle closes;
- supports classification of current Windows Job Object use as lifecycle/resource
  containment rather than generic filesystem/network/credential isolation.

## S6 — Microsoft AppContainer isolation

Microsoft Learn, **AppContainer isolation** and **Launch an AppContainer**.

Canonical sources:

- `https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation`
- `https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer`

RF13 use:

- AppContainer is an explicit per-application isolation/security boundary using low
  integrity, SIDs/capabilities and default-restricted access;
- Microsoft documents credential, device, file, network, process and window isolation;
- supports `LimitedToken != AppContainerIsolation` and least-privilege/default-deny models.

## S7 — Microsoft Windows Sandbox

Microsoft Learn, **Windows Sandbox** and **Windows Sandbox architecture/configuration**.

Canonical sources:

- `https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/`
- `https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-architecture`
- `https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file`

RF13 use:

- Windows Sandbox uses hardware-based virtualization and a separate kernel/hypervisor boundary
  for untrusted applications;
- networking, clipboard, mapped resources and devices are configurable sharing channels;
- supports `Namespace/ContainerBoundary != VMBoundary` and `VM != ZeroInteraction`.

## S8 — NIST application container security

NIST SP 800-190, **Application Container Security Guide**.

Canonical source: `https://csrc.nist.gov/pubs/sp/800/190/final`.

RF13 use:

- containers are OS-level virtualization/application packaging and have distinct security
  threats/risks;
- shared-kernel/container isolation should not be collapsed with VM isolation;
- supports explicit threat-model/security-boundary analysis for containerized execution.

## S9 — Current Ordivon Runtime as empirical source

### `trusted_local`

Current canonical docs state `trusted_local` inherits installed service-user local authority
and is intended for owner-trusted local engineering repositories/executables. Source
commitment/pre-spawn digest checking is not a filesystem snapshot, and same-authority target
code can intentionally mutate its world.

### `contained_local`

Current profile uses the same systemd transient-unit supervisor while reducing ambient
authority through private network, effective/ambient capability removal, no-new-privileges,
namespace/address-family restrictions, read-only base filesystem, hidden host state
hierarchies, exact bind-backs, reduced payload identity, fixed Runner environment and
Workspace-scoped cache paths. Canonical docs explicitly reject hostile-code/kernel-grade,
complete-confidentiality, controlled-egress and disposable-machine claims.

### Linux payload privilege separation

Runner validates non-root payload UID/GID, clears supplementary groups, calls setgid/setuid,
enables `PR_SET_NO_NEW_PRIVS`, applies restrictive umask and executes under the prepared
contained view. This is authority attenuation/privilege separation rather than separate-kernel
isolation.

### Resource/lifecycle containment

Current Attempt supervision uses systemd/cgroup identity and optional MemoryMax/TasksMax/
CPUQuota budgets. Per-step process groups support descendant timeout cleanup. These are
resource/lifecycle controls and do not retract external Effects.

### Immutable inputs

Input-bound execution materializes exact digest-verified Job-owned bytes and provides a
read-only presentation for the committed target authority. The Linux input-bound path uses
contained-local; Windows input-bound execution requires `windowsAuthority=limited`. Current
docs explicitly scope Windows read-only semantics to the limited child, not elevated
administrators.

### Windows authority/Job Object

Current Windows target records token SID/type/elevation/integrity/restriction/admin-group
evidence, distinguishes limited from explicitly elevated authority, and uses a launcher-owned
Job Object with kill-on-close process-tree semantics. Current Windows profile remains
`trusted_local`; limited token does not imply AppContainer isolation.

### P4

P4 physically demonstrated a trusted same-authority target creating a private mount namespace
and consuming alternate bytes at the same textual pathname without host-view drift. This is
both a target-view evidence falsifier and an authority/isolation boundary demonstration.

## Source discipline

Later rounds should preserve:

```text
sandbox label != isolation contract
isolation is dimension + threat-model scoped
integrity != confidentiality != availability isolation
containment != authority attenuation != trust
trusted_local = broad owner-trusted authority
contained_local = partial authority reduction/isolation, not hostile sandbox
namespace kinds isolate specific resources
mount/path identity is namespace-relative
non-root UID != user namespace
NoNewPrivileges/capability removal are narrower authority controls
resource cgroups/Job Objects = lifecycle/availability containment, not generic sandbox
immutable input != isolated environment
limited Windows token != AppContainer
container/shared-kernel boundary != VM/separate-kernel boundary
authentication/authorization != execution isolation
witness/detection != prevention
IsolationClaimScope <= EnforcedBoundaryScope
```
