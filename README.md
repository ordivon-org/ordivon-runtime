# Ordivon

Ordivon is a local execution and recovery Runtime for capable Agents.

```text
human intent
→ Agent reasoning and operation
→ Ordivon durable execution
→ observable result
→ recovery or continuation
```

The installed Runtime is deliberately narrow:

- one `ordivon-mcp` server and ten MCP tools;
- one `ordivon-exec` Runtime;
- exact Git workspaces and digest-bound mutation;
- SQLite Job and Attempt truth;
- systemd/cgroup process ownership;
- cancellation, timeout, bounded output, Artifacts, and restart recovery.

Ordivon runs as `trusted-local`: the Agent inherits the service user's delegated host authority, while the Runtime preserves only execution facts that cannot be reconstructed. Untrusted code belongs in a separate VM or container boundary rather than a second execution mode inside Ordivon.

Engineering defaults:

- **thin context:** source, tests, Git, live state, and four active documents;
- **thin recovery:** Git plus minimal durable execution truth;
- **low friction:** direct execution, fast checks, and correction after observed failure;
- **one path:** no parallel Runtime, policy platform, proof hierarchy, or speculative Adapter layer.

Read [`AGENTS.md`](AGENTS.md), then [`docs/current-state.md`](docs/current-state.md). The active document set is listed in [`docs/README.md`](docs/README.md).

## Default checks

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

The complete pre-simplification implementation remains recoverable at Git tag `m-series-closed-2026-07-22`.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
