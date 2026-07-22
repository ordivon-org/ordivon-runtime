# ordivon-mcp

`ordivon-mcp` is the single current MCP entry over `ordivon-exec`.

It serves Streamable HTTP on a loopback address, requires a random bearer token, bounds request bodies, and exposes:

```text
workspace.open
workspace.read
workspace.mutate
workspace.diff
workspace.exec
task.observe
task.cancel
task.list
artifact.read
```

The adapter owns protocol schemas, MCP Task projection, transport authentication, and conversion between tool results and Runtime results. Filesystem, Git, execution, idempotency, concurrency, cancellation, result, and reconciliation semantics remain in `ordivon-exec`.

`trusted-local` is the default mode. `isolated` explicitly activates the non-root worker boundary for untrusted inputs.
