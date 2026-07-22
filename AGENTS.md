# Ordivon Agent Entry

## Default operating model

- Prefer direct execution over pre-emptive process ceremony.
- Preserve user changes and use isolated worktrees for substantial work.
- Record commands, results, revisions, and terminal state.
- Recover quickly from reversible failures instead of adding speculative gates.
- Hard-block only irreversible external effects, credential exposure, ambiguous dispatch, and real concurrency conflicts.
- Use `trusted-local` for user-owned, Git-backed work; use strong isolation for untrusted inputs.

## Active engineering surface

The target surface is the Rust runtime, one MCP server, minimal execution policy, systemd packaging, and a few operational scripts. Historical governance runtimes, receipts, stage proof machines, and parallel M-series servers are not active architecture.

## Default verification

```bash
cargo fmt --check
cargo test --workspace
ruff check scripts/
```

Run strict Clippy, systemd integration, reboot recovery, security audits, and isolated-worker tests only when the affected boundary justifies them.

## Documentation

Use `docs/README.md` as the entry point. Git history and tag `m-series-closed-2026-07-22` are the archive; do not recreate an in-tree archive hierarchy.
