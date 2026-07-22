# Ordivon

Ordivon is a local execution and recovery Runtime for capable Agents.

```text
human intent
→ Agent reasoning and operation
→ Ordivon durable execution
→ observable result
→ recovery or continuation
```

In `trusted-local`, the Agent inherits the service user's delegated host authority, including network, credentials, Git, Docker, systemd, cloud CLIs, files, and executables. `isolated` is an explicit reduced-authority mode for untrusted input.

Ordivon follows three engineering defaults:

- **thin context:** source, tests, Git, live state, and a few current documents;
- **thin recovery:** Git plus only the execution facts that cannot be reconstructed;
- **reallocated responsibility:** the user owns intent, the Agent owns implementation and operation, and the Runtime owns durable execution truth.

## Current surface

- one `ordivon-mcp` server and nine MCP tools;
- one `ordivon-exec` Runtime;
- SQLite Job and Attempt truth;
- systemd/cgroup process ownership;
- cancellation, timeout, bounded output, Artifacts, and restart recovery.

Read [`AGENTS.md`](AGENTS.md), then [`docs/current-state.md`](docs/current-state.md). The complete active document set contains only four files and is listed in [`docs/README.md`](docs/README.md).

## Default checks

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

The complete M0–M7 tree remains recoverable at Git tag `m-series-closed-2026-07-22`.
