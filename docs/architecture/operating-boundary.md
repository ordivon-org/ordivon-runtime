# Ordivon Operating Boundary

## Default mode: trusted-local

`trusted-local` runs with the same host identity and authority as the Ordivon service. It inherits the host environment and may use any repository, executable, network route, credential store, Git remote, Docker socket, systemd interface, cloud CLI, or filesystem path available to that user.

The Runtime does not create per-repository, per-executable, per-credential, or per-tool approval systems. It preserves:

- exact Git workspaces and digest-guarded mutation;
- Job, Attempt, result, and Artifact truth;
- idempotent admission and concurrency reservations;
- systemd/cgroup process ownership;
- timeout, bounded output, cancellation, and restart recovery;
- fail-closed treatment of ambiguous dispatch.

The MCP client supplies the command and resource bounds. The server owns Principal, authority identity, policy metadata, and concurrency ceilings; an Agent does not self-certify those fields.

## Explicit mode: isolated

`isolated` is an opt-in reduced-authority mode for unknown repositories, downloaded code, hostile samples, parser/fuzzer inputs, or any task where the user or Agent deliberately wants a sandbox.

It activates the non-root worker, private filesystem views, hidden credentials and host control paths, network isolation, capability reduction, and systemd hardening. These controls belong only to this explicit mode.

## Permission boundary

Authentication answers who may use Ordivon. Once a trusted caller is authenticated, `trusted-local` grants the authority configured for the host service rather than reconstructing classical least-privilege gates inside every command.

A task may therefore perform Git Push, create a GitHub PR, manage Docker or systemd, use Cloudflare/AWS/Kubernetes tooling, or access credentials when the service user can do so. Ordivon records and supervises the action; it does not require a bespoke Adapter for every mature CLI.

## Network boundary

The MCP HTTP server remains loopback-bound, bearer-authenticated, and request-size bounded. Cloudflare Tunnel and Access may authenticate and transport a remote trusted caller. Transport authentication does not imply that the resulting Agent execution must be sandboxed.

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
