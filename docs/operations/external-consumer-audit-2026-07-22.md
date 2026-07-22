# External Consumer Audit — 2026-07-22

Scope: read-only inspection of local systemd, processes, listeners, Cloudflared container metadata, and non-secret environment keys.

## Observed findings

1. `ordivon-mcp-bridge.service` is the only installed system unit matching Ordivon/MCP names.
2. Its command is Supergateway 3.4.3 wrapping Desktop Commander 0.2.46, not any removed M-series binary and not the new Rust server.
3. The service runs as root and listens on `*:8811`; `/healthz` returns `ok`.
4. `ordivon-cloudflared` uses host networking and routes the public MCP destination to `http://localhost:8811`.
5. The Cloudflare environment file exposes only the key name `TUNNEL_TOKEN`; its value was not read into the report.
6. No systemd or Cloudflare configuration file contained `ordivon-m1-cli`, M4/M6/M7 server names, old feature names, or Python governance package paths.
7. A separate rootless Docker MCP gateway listens on `127.0.0.1:18812`; it is not the Cloudflare origin for `mcp.ordivon.com`.
8. Historical PostgreSQL, NATS, Temporal, and Temporal database containers remain running. The simplified Runtime does not consume them, but this audit did not prove that no separate project or user workflow still does.
9. Cloudflared recent logs contain repeated remote stream-cancellation messages. Their operational significance is unknown from this audit alone.

## Dispositions

| Finding | Disposition | Priority |
|---|---|---|
| Live bridge is Desktop Commander, not Rust Ordivon | migrate in a separate maintenance window | P0 before claiming remote Rust deployment |
| Port 8811 listens on all interfaces | remove or restrict after alternate control path is proven | P0 remote-boundary risk |
| Cloudflare origin points to 8811 | retain temporarily to preserve current access | explicit temporary exception |
| No removed M-series binary consumers found | accepted | closed for inspected paths |
| Rootless Docker MCP gateway on 18812 | separate system; no change in this stage | non-goal |
| Historical PostgreSQL/NATS/Temporal containers | confirm ownership, then stop/remove only in a separate operation | P1 external infrastructure debt |
| Repeated Cloudflare stream cancellations | observe during migration validation | P1 / Unknown |

## Audit limitation

This audit did not query Cloudflare control-plane policy or alter live configuration. It therefore does not prove Access policy, hostname routing, token rotation, or external client behavior.
