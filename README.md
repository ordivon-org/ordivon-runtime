# Ordivon Runtime

Ordivon Runtime is the trusted-local execution and recovery subsystem of **Ordivon**. It is not the complete Ordivon system.

```text
human intent
→ Agent reasoning and operation
→ Ordivon Runtime
→ durable execution
→ observable result
→ recovery or continuation
```

The Runtime owns the deterministic execution boundary: Git Workspaces, request identity, Jobs and Attempts, process-tree ownership, bounded Artifacts, cancellation, reconciliation, and recovery. Higher-level cognition, planning, memory, coordination, and product interfaces belong to Ordivon or other components above the Runtime.

An authenticated Agent receives the installed service user's trusted-local authority. Ordivon Runtime preserves execution facts that cannot be reconstructed after interruption; it is not a sandbox for untrusted code. Run untrusted workloads behind a separate VM or container boundary.

For repository work, start with [`AGENTS.md`](AGENTS.md). Read [`docs/runtime.md`](docs/runtime.md) only when the task crosses Runtime boundaries, and [`docs/recovery.md`](docs/recovery.md) only for Registry diagnosis, backup, repair, or restore.

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
