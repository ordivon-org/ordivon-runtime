# Execution capability policy

The Runtime accepts an explicit capability policy for each executable profile. A policy closes over the executable digest, exact argument vectors, environment, runtime/output bounds, allowed CWD roots, and concurrency.

`ordivon-local-dev-v1.json` is the one tracked local example. It is deliberately machine-bound to `/root/projects/Ordivon` and the current stable Cargo binary. Its presence does not activate execution or authorize a caller.

Any toolchain update, executable replacement, repository relocation, permission weakening, or requested argv change makes the policy fail closed until the file is intentionally regenerated. Do not broaden the policy merely to avoid that explicit update.

The profile authorizes exactly:

```text
cargo build --release -p ordivon-mcp
```

Cargo can execute repository-controlled build scripts, procedural macros, linkers, and configured runners. This policy is an authority boundary, not a sandbox or a claim that compilation is side-effect free.
