# Ordivon

Ordivon is a local execution and recovery Runtime for capable Agents.

```text
human intent
→ Agent reasoning and operation
→ durable execution
→ observable result
→ recovery or continuation
```

An authenticated Agent receives the installed service user's trusted-local authority. Ordivon preserves the execution facts that cannot be reconstructed after interruption; it is not a sandbox for untrusted code. Run untrusted workloads behind a separate VM or container boundary.

For repository work, start with [`AGENTS.md`](AGENTS.md). Read [`docs/runtime.md`](docs/runtime.md) only when the task crosses Runtime boundaries, and [`docs/recovery.md`](docs/recovery.md) only for Registry diagnosis, backup, repair, or restore.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
