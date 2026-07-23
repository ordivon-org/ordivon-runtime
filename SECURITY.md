# Security Policy

## Reporting

Do not open a public vulnerability issue. Contact the repository owner with `[SECURITY]`, the affected component, reproduction steps, and impact.

## Supported version

Only current `main` is supported. There are no backports or LTS branches.

## Boundary

Ordivon is a trusted-local execution Runtime. It intentionally inherits the installed service user's authority. Do not submit untrusted repositories or scripts to this Runtime; use a separate VM, container host, or other external isolation boundary.

Strong controls remain where reality cannot be reconstructed safely:

- bearer credentials and Cloudflare Access authentication;
- loopback-only origin and bounded request bodies;
- idempotent admission and transactional capacity ownership;
- ambiguous dispatch, process-tree ownership, timeout, and cancellation;
- persistent Job, Attempt, result, and Artifact identity.

## Supply chain

- Gitleaks runs on pull requests and pushes to `main`.
- Dependabot covers Cargo and GitHub Actions.
- CodeQL and release checks are on demand.
- Default CI does not publish packages, containers, or SBOMs.

The pre-simplification tree remains recoverable from Git tag `m-series-closed-2026-07-22`.
