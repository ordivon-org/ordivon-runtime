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

The adapter owns protocol schemas, MCP Task projection, transport authentication, and conversion between tool results and Runtime results. It also binds server-owned Principal, authority, policy identity, and concurrency limits so the Agent does not submit self-asserted governance metadata. Filesystem, Git, execution, idempotency, cancellation, result, and reconciliation semantics remain in `ordivon-exec`.

`trusted-local` is the default and inherits the service user's host authority. `isolated` explicitly activates the reduced-authority non-root worker boundary.
