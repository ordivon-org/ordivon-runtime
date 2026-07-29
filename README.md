# Ordivon Runtime

Ordivon Runtime is the owner-trusted effect execution and recovery subsystem of **Ordivon**. It is not the complete Ordivon system.

```text
participant purpose or Host-admitted work
→ cognition and coordination
→ Ordivon Runtime
→ durable execution
→ observable result
→ recovery or continuation
```

The Runtime owns the deterministic commitment boundary between Agent decisions and reality: Git Workspaces, stable operation identity, Jobs and Attempts, at-most-once physical dispatch, process-tree ownership, bounded evidence, cancellation, reconciliation, and recovery. Higher-level cognition, planning, memory, coordination, and product interfaces belong to Ordivon or other components above the Runtime. The kernel contract is defined in [`docs/effect-kernel.md`](docs/effect-kernel.md).

An authenticated participant or Host uses the canonical `trusted_local` profile by default and receives the installed service user's local authority. `contained_local` is an explicit no-fallback reduction profile: it removes inherited credentials and capabilities, blocks network access, hides host state directories, and exposes only the Workspace, Attempt bundle, declared caches, Runner, and Git metadata required for source commitment. It is not a strong multi-tenant or hostile-code boundary. Run high-consequence untrusted workloads behind a disposable VM or container owned outside Runtime.

For repository work, start with [`AGENTS.md`](AGENTS.md). Read [`docs/runtime.md`](docs/runtime.md) only when the task crosses Runtime boundaries, [`docs/recovery.md`](docs/recovery.md) for Registry diagnosis and repair, [`docs/agent-ux.md`](docs/agent-ux.md) for Patch, ExecPlan, progress, report, Git, and GitHub adapter contracts, [`docs/compatibility.md`](docs/compatibility.md) for retained consumers and deletion triggers, [`docs/effect-comparison.md`](docs/effect-comparison.md) for the Issue #56 baseline, and [`docs/operations.md`](docs/operations.md) for deployment, rollback, and Workspace reclaim.

## Identity

- Project family: **Ordivon**
- Component: **Ordivon Runtime**
- Repository: `ordivon-runtime`
- Service binary: `ordivon-runtime`
- MCP adapter: `ordivon-runtime-mcp`
- Runtime core crate: `ordivon-runtime-core`

The persisted `/var/lib/ordivon` namespace and `ORDIVON_*` environment variables remain stable because they belong to the Ordivon project family rather than one executable name.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
