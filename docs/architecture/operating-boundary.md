# Ordivon Operating Boundary

## Default mode: trusted-local

Use `trusted-local` for user-owned, Git-backed repositories where ordinary rollback is available. It does not require a dedicated worker identity, ownership transfer, mount namespace, or isolated filesystem view.

The Runtime still preserves:

- isolated worktree ownership;
- command timeout and bounded output;
- cgroup process ownership and cancellation;
- Job, Attempt, result, and Artifact truth;
- idempotency and concurrency reservations.

It must not impose document governance, wording checks, stage Receipts, or a mandatory non-root sandbox on ordinary reversible edits.

## Explicit mode: isolated

Use `isolated` for unknown repositories, downloaded code, untrusted scripts, dependency experiments, parser or fuzzer inputs, and security work. This activates the non-root `ordivon-worker`, private roots, ownership transfer, and systemd filesystem restrictions.

Isolation is selected by input risk, not applied as a universal proof of engineering quality.

## Hard-blocking boundary

A hard block is justified only when an action can:

- expose credentials or protected host state;
- create an irreversible external side effect;
- duplicate an ambiguously dispatched operation;
- violate a real concurrency reservation;
- corrupt persistent execution truth;
- cross an explicitly selected isolation boundary.

A hard block is not justified solely because code may fail, tests may be incomplete, wording may be imprecise, or a Git-backed file may need restoration.

## Network boundary

Local HTTP binds to loopback, requires a random bearer token, and bounds request bodies. Browser Host and Origin controls belong only on transports that are actually exposed through a browser or remote network.

Cloudflare Tunnel and Access protect the remote transport. They do not replace Runtime idempotency, execution bounds, or Job and Attempt truth.

## Component admission test

A new persistent component, service, Registry, state model, checker, or document class must answer all five questions:

1. Which concrete failure or missing user capability does it address?
2. Is that failure observed, structurally unavoidable, or high-loss rather than merely imaginable?
3. Does the component act at the point where the failure occurs?
4. Why can the existing Runtime, Git, tests, or operations path not solve it more simply?
5. Which current structure does it replace, or why is permanent coexistence necessary?

“Governance,” “future extensibility,” “defence in depth,” and “better visibility” are not sufficient answers without a named failure and causal mechanism.

## Deletion test

A component should leave the active surface when it has no current caller, duplicates another source of truth, preserves only a completed development stage, generates reports without changing a failure path, or exists only for a hypothetical future.

Deletion is blocked only when it would remove a current user capability, a proven security or recovery boundary, live deployment operation, or a high-cost design explanation that code cannot reveal.

Git history and dated records are the default archive. The working tree is not an archive.

## Verification selection

Default checks cover formatting, ordinary tests, script lint, and the changed path. Add strict Clippy, real systemd or cgroup tests, reboot recovery, isolated-worker matrices, dependency audit, CodeQL, full-history secret scan, or benchmark only when the affected boundary makes that evidence relevant.

Verification must reduce uncertainty about the change. A check that cannot fail on the changed boundary is process cost, not evidence.
