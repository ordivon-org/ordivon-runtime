# Ordivon Operating Boundary

## Default mode: trusted-local

Use for user-owned repositories with an exact Git revision and ordinary rollback. The runtime should preserve:

- isolated worktree ownership;
- command timeout;
- bounded output;
- cgroup process ownership and cancellation;
- Job, Attempt, result, and Artifact truth;
- idempotency and concurrency reservations.

It should not impose document governance, wording checks, stage receipts, or a mandatory non-root sandbox on ordinary reversible edits.

## Explicit mode: isolated

Use strong isolation for unknown repositories, downloaded code, untrusted scripts, dependency investigation, or security work. The retained worker identity and systemd filesystem layout are the basis for this mode.

## Hard-blocking boundary

A hard block is justified when an action can expose credentials, create an irreversible external side effect, duplicate an ambiguously dispatched operation, violate a real concurrency reservation, or corrupt persistent execution truth.

A hard block is not justified solely because code may fail, tests may be incomplete, wording may be imprecise, or a file can be restored from Git.

## Network boundary

Local HTTP must bind to loopback, require a random bearer token, and bound request bodies. Browser-specific Host and Origin policy should be enabled only when that transport is actually exposed through a browser or remote network.

## On-demand verification

Run strict Clippy, systemd integration, reboot recovery, dependency audit, CodeQL, and isolated-worker matrices only when the changed boundary makes the result relevant.
