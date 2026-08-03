# Security Policy

## Reporting a vulnerability

Do not open a public Issue, Discussion, or pull request for a suspected vulnerability.

Use GitHub's private vulnerability reporting channel:

- `https://github.com/zycxfyh/ordivon-runtime/security/advisories/new`

Include:

- affected commit, release, binary, or deployment receipt;
- affected Tool, script, service, or persisted-state path;
- reproduction steps or a minimal proof of concept;
- expected and observed authority boundaries;
- impact, including whether credentials, source, Artifacts, process trees, or external effects are involved;
- any public disclosure deadline known to you.

The maintainer aims to acknowledge a complete report within three business days. Validation, remediation, release, and disclosure timing depend on severity and the need to preserve recovery or compatibility evidence. Coordinate public disclosure through the private advisory until a fix or an explicit disclosure decision is ready.

If GitHub private reporting is unavailable, open a public Issue containing no vulnerability details and ask the repository owner to establish a private channel. Do not send secrets or exploit material through a public Issue.

## Supported version

Only current `main` and the exact receipt-bound previous production binary are supported. There are no LTS or backport branches. A report affecting older retained state is still relevant when that state participates in current replay, recovery, migration, or rollback.

## Security boundary

Ordivon Runtime is a trusted-local execution subsystem. The canonical `trusted_local` profile intentionally inherits the installed service user's filesystem, process, network, credential, Git, Docker, systemd, cloud-CLI, and executable authority.

Do not submit untrusted repositories, commands, or scripts. Put hostile or high-consequence untrusted work behind a disposable VM, container host, or equivalent external isolation boundary owned outside Runtime.

`contained_local` is an explicit no-fallback authority-reduction profile. It removes inherited credentials and capabilities, blocks network access, hides common host-state directories, and exposes only declared Runtime paths. It does not claim kernel-grade isolation, controlled egress, multi-tenant confidentiality, or protection from hostile same-host code.

## Network boundary

The MCP origin must remain loopback-bound. Direct local clients require the configured Bearer token. Remote access must reach the loopback origin through an authenticated tunnel boundary configured and owned by the operator.

Do not expose the Runtime service directly on a public or shared interface. Rotate the Bearer token and tunnel credentials after suspected disclosure.

## Sensitive data

Runtime may retain source, command arguments, explicit target-process environment values, stdout, stderr, Artifacts, paths, client identities, foreign references, traces, and operational receipts. It does not automatically redact sensitive content.

Read [`docs/data-and-privacy.md`](docs/data-and-privacy.md) before collecting or sharing diagnostics. Never publish an environment file, Bearer token, raw Registry, Attempt bundle, backup, or quarantine data.

## Security process

Security is enforced through:

- fail-closed request validation and source-state commitment;
- explicit unknown, lost, and orphaned outcomes;
- at-most-once physical dispatch per Attempt;
- process-tree ownership through systemd and cgroups;
- bounded execution resources and output;
- private state permissions and restrictive umasks;
- secret scanning, dependency policy, advisory checks, and CodeQL;
- portable tests plus explicit real-system acceptance;
- digest-bound deployment and rollback receipts;
- documented recovery instead of speculative redispatch.

A passing scan or test is not a security guarantee. Reassess the threat model whenever authority, network exposure, persisted data, Tool contracts, or external effects change.
