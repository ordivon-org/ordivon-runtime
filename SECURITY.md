# Security Policy

## Reporting a vulnerability

Do not open a public issue. Contact the repository owner with `[SECURITY]` in the subject and include the affected component, reproduction steps, and impact.

## Supported version

Only the current `main` branch is supported. There are no backports or LTS branches.

## Active security boundary

Ordivon treats ordinary code and Git-backed file changes as reversible. Strong controls are reserved for boundaries that cannot be reconstructed safely:

- credentials and bearer tokens;
- irreversible external side effects;
- ambiguous dispatch after a Core or transport failure;
- idempotency and concurrent reservation ownership;
- process-tree ownership, timeout, cancellation, and bounded output;
- persistent Job, Attempt, result, and artifact truth.

The local HTTP surface must bind to loopback, use a random bearer token, and bound request bodies. Strong non-root isolation is available for untrusted repositories and scripts rather than imposed on every trusted-local task.

## Supply chain

- Gitleaks runs on pull requests and pushes to `main`.
- Dependabot covers Cargo and GitHub Actions weekly.
- CodeQL, `cargo audit`, and `cargo deny` are on-demand checks for dependency, security, or release work.
- The default CI does not publish packages, containers, or SBOMs.

## Recovery

The complete pre-simplification tree is recoverable from Git tag `m-series-closed-2026-07-22`.
