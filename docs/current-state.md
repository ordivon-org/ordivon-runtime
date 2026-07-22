# Ordivon Current State

This document is the single current-state entry for the repository. It describes what exists now, what is live, and the only active milestone. It is not a historical record or a future capability catalogue.

## Product identity

Ordivon is a local execution runtime for capable agents. It converts a repository task into a bounded, observable, recoverable execution while preserving the facts that cannot be reconstructed after a crash or race.

It is not currently a general governance operating system, organizational policy platform, cloud scheduler, knowledge graph, or multi-tenant control plane.

## Current code path

```text
MCP client
→ ordivon-mcp
→ ordivon-exec
→ SQLite Job / Attempt Registry
→ systemd transient unit and cgroup
→ task runner
→ bounded result and Artifacts
→ reconciliation and recovery
```

The current public MCP surface contains nine tools: workspace open, read, mutate, diff, and exec; task observe, cancel, and list; and Artifact read.

## Three distinct truths

Do not collapse these states:

| Surface | Current truth |
|---|---|
| Repository `main` | The pre-adoption mainline; it does not yet contain the simplification candidate. |
| Simplification candidate | Published branch `agent/post-m-series-simplification`, tracked by Issue #25 and Draft PR #26; this is the proposed current architecture. |
| Live remote service | `mcp.ordivon.com` still reaches Supergateway and Desktop Commander on port 8811. It does not run the Rust MCP candidate. |

Use Git and the workstation for exact identity:

```bash
git rev-parse HEAD
git branch --show-current
git status --short --branch
systemctl status ordivon-mcp-bridge.service
```

A document must never present candidate code as merged code or local E2E evidence as proof of the live Cloudflare path.

## Persistent truth

The Runtime preserves Job, Attempt, idempotency, concurrency reservation, launch identity, process identity, cancellation intent, result, and Artifact identity. Git preserves repository history. Dated reports preserve historical observations only.

## Only active repository milestone

**Adopt the simplified Runtime into `main` without restoring the removed governance platform or parallel M-series surfaces.**

Exit conditions:

1. the candidate diff is reviewable as one coherent replacement of the old active surface;
2. active documents agree on product identity, current topology, and operating boundary;
3. exact-head fast and merge acceptance pass;
4. published Draft PR #26 receives ordinary diff review;
5. `main` adopts one MCP server and one execution Runtime.

Remote Rust MCP migration is the next milestone, not part of repository adoption. It begins only after the candidate is in `main` and keeps the current 8811 bridge available for rollback.

## Current issue disposition

- #25 is the only open implementation Issue. It tracks adoption of the simplified Runtime into `main` and explicitly excludes live remote migration.
- #22, the horizontal capability-fabric umbrella, is closed as superseded by #25.
- #23, the GitHub-hosted read-only worker, is closed as not planned because CI is verification rather than the execution substrate.
- #24, the Docker worker and draft Receipt path, is closed as superseded by the systemd, cgroup, SQLite, and Runner implementation.

Do not reopen these Issues to recover old capability catalogues. A future Issue must advance the current milestone through one observable state change.

## Truth precedence

When sources disagree, use this order:

1. observed code, tests, Git identity, and live process state;
2. this current-state document;
3. active architecture and operations documents listed in `docs/README.md`;
4. dated audits, acceptance reports, closure notes, and retrospectives;
5. Git history and closed Issues.

Dated records explain how a decision was reached. They do not define the current Runtime.

## Explicit non-goals

The active milestone does not add agent loops, cloud scheduling, browser automation, deployment adapters, a policy engine, a document Registry, Receipt generation, governance debt, multi-tenancy, or a user interface.

A future capability becomes work only when a concrete user task cannot be completed by the current path and a smaller extension cannot solve it.
