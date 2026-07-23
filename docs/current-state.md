# Ordivon Current State

## Product identity

Ordivon is a trusted-local execution and recovery Runtime for capable Agents. It gives an authenticated Agent high-bandwidth access to the user's machine while preserving the execution facts needed after interruption.

It is not an Agent loop, governance operating system, policy platform, knowledge base, cloud scheduler, or multi-tenant control plane.

## Production path

```text
MCP client
→ Cloudflare Access and Tunnel
→ 127.0.0.1:8897
→ ordivon-mcp
→ ordivon-exec
→ SQLite Job / Attempt truth
→ systemd transient unit and cgroup
→ result, Artifacts, and reconciliation
```

The Rust service is the single Ordivon MCP origin. The former Supergateway/Desktop Commander bridge has been retired. A separate Docker MCP listener on `127.0.0.1:18812` is an independent tool system and does not share Ordivon Runtime state.

## Current surface

The public MCP surface contains ten tools: workspace open, close, read, mutate, diff, and exec; task observe, cancel, and list; and Artifact read.

All clients share one transactional global execution limit. Different Workspaces may execute concurrently, while each Workspace admits only one active command tree.

## Sources of truth

- Git owns code history, diff, rollback, and workspace identity.
- SQLite owns Job, Attempt, reservation, cancellation, and terminal resolution.
- systemd/cgroups own live process-tree observation and termination.
- bounded result and Artifact files own output bytes and Digests.

Everything else should be reconstructed instead of persisted as a second proof system.

## Operating rule

Use Ordivon on real tasks. Add a capability only after repeated work demonstrates a missing operation that existing host CLIs through `workspace.exec` cannot provide. Delete state, checks, and helpers that do not preserve non-reconstructable execution truth or improve observed recovery.

## Explicit non-goals

No parallel Runtime, policy engine, Receipt hierarchy, document Registry, Agent loop, distributed scheduler, capability catalogue, multi-tenancy, UI, or internal sandbox platform is planned.
