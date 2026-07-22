# Ordivon Current State

This is the single current-state and roadmap document. It describes what exists now, what is live, and the next observable state changes. It is not a historical record or a future capability catalogue.

## Product identity

Ordivon is a local execution and recovery Runtime for capable Agents. It gives an authenticated Agent high-bandwidth access to the user's machine while preserving the execution facts that cannot be reconstructed after interruption.

It is not an Agent loop, governance operating system, policy platform, document knowledge base, cloud scheduler, or multi-tenant control plane.

## Current execution path

```text
MCP client
→ ordivon-mcp
→ ordivon-exec
→ SQLite Job / Attempt truth
→ systemd transient unit and cgroup
→ task runner
→ result and Artifacts
→ reconciliation
```

The public MCP surface contains nine tools: workspace open, read, mutate, diff, and exec; task observe, cancel, and list; and Artifact read.

## Three distinct truths

| Surface | Current truth |
|---|---|
| Repository `main` | Pre-adoption mainline; it does not contain the simplification candidate. |
| Candidate | `agent/post-m-series-simplification`, Issue #25, Draft PR #26. |
| Live remote service | `mcp.ordivon.com` still reaches Supergateway and Desktop Commander on port 8811. |

Never present candidate code as merged code or local E2E evidence as proof of the live Cloudflare route.

## Three long-term shifts

### 1. Thin active context

An Agent should recover the repository's main mental model from `AGENTS.md`, this file, Runtime architecture, relevant source, and tests. Historical records remain in Git or dated files but do not enter the default context.

### 2. Thin recovery and governance

Git owns code history and rollback. SQLite, systemd/cgroups, and bounded result files preserve only non-reconstructable execution facts. Governance acts at actual failure points; it does not create a second proof system around ordinary reversible work.

### 3. Reallocated responsibility

The user owns intent, preferences, risk posture, and final commitments. The Agent owns implementation, testing, repair, and ordinary operations. The Runtime owns durable execution identity and recovery. Classical engineering mechanisms remain where they produce causal feedback or preserve reality, not as ceremony.

## Only active repository milestone

**Adopt the simplified Runtime into `main` as the single current architecture.**

Before PR #26 becomes ready:

1. review the complete `main..candidate` replacement diff for accidental capability loss;
2. run exact-head Fast, Merge, and Release acceptance;
3. merge without changing the live 8811 route.

Issue #25 remains the only open implementation Issue until this state change is complete.

## Post-adoption route

### A. Real local use

Install the Rust MCP on a separate loopback port and use it for real Ordivon, FinHarness, and research tasks. Do not add capabilities during installation. Measure where the Agent still falls back to Desktop Commander or manual intervention.

### B. Recovery minimization

Use Dogfood evidence to delete any persistent state, policy object, checker, or helper that does not preserve a non-reconstructable fact or improve observed recovery. Keep Git, minimal SQLite task truth, process ownership, cancellation, terminal results, and backup/restore.

### C. Remote cutover

Expose the Rust MCP through a separate Cloudflare route, verify authenticated full-authority operation, then switch the production hostname with the old bridge available for rollback. Remote transport does not imply reduced Agent authority.

### D. Capability growth by failure

Add a capability only after repeated real tasks demonstrate a missing operation. Prefer mature host CLIs through `workspace.exec`. Do not prebuild GitHub, Cloudflare, Docker, Browser, Scheduler, Memory, Skill Registry, or Agent-loop platforms.

## Truth precedence

Use observed source, tests, Git identity, and live process state first; then this document, Runtime architecture, and the Runbook. Dated records and closed Issues explain history only.

## Explicit non-goals

No new governance platform, Receipt hierarchy, document Registry, policy engine, Agent loop, cloud scheduler, capability catalogue, multi-tenancy, or UI is planned. Future work begins from a concrete failed task, not an imagined platform surface.
