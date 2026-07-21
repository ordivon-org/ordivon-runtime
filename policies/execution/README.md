# Execution capability policies

Files in this directory define reviewed execution capability candidates. Their
presence in Git does not activate execution, authorize a caller, start a
process, or expose an MCP tool.

`ordivon-local-dev-v1.json` is intentionally machine-bound to the current WSL
workspace and stable Rust toolchain. It permits one exact request vector:

```text
cargo build --release -p ordivon-mcp
```

The evaluator resolves canonical paths and verifies the executable SHA-256.
A toolchain update, executable replacement, path change, permission weakening,
or workspace relocation makes the policy fail closed until it is reviewed and
updated. The future runner must clear inherited environment variables and
reverify the executable immediately before process creation.

Cargo compilation can execute repository-controlled build scripts, procedural
macros, linkers, and configured runners. This profile grants authority to run
the reviewed repository build graph; it is not a sandbox or a claim that
compilation is side-effect free.
