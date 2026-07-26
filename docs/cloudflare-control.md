# Ordivon Cloudflare Control

A small Cloudflare control plane for the Ordivon Runtime. It uses only the Python standard library and stores the API token outside the repository.

## One-time installation

Create a custom Cloudflare API token restricted to the Ordivon account and `ordivon.com`. The useful permission groups are:

- Zone: Read
- DNS: Read and Edit
- Zone Settings: Read and Edit
- SSL and Certificates: Read and Edit
- Zone WAF / Rulesets: Read and Edit
- Cache Purge
- Account Settings: Read
- Workers Scripts and Routes: Read and Edit
- Cloudflare Tunnel: Read and Edit
- Access Apps and Policies: Read and Edit
- Workers KV Storage: Read and Edit
- D1: Read and Edit
- R2 Storage: Read and Edit
- Queues: Read and Edit
- AI Gateway: Read and Edit

Then run:

```bash
./scripts/install-cloudflare-control
```

Paste the token into the hidden prompt. The installer verifies it, discovers the account ID and `ordivon.com` zone ID, and writes:

```text
~/.config/ordivon/secrets/cloudflare.json
```

The file and parent secrets directory are mode `0600` and `0700` respectively. The token is never passed as a command-line argument.

## Core commands

```bash
ordivon-cloudflare verify
ordivon-cloudflare doctor
ordivon-cloudflare inventory
ordivon-cloudflare inventory --output /root/backups/cloudflare-inventory.json
```

`doctor` probes each product independently, so an unavailable product such as R2 does not hide the status of DNS, Tunnel, Access, or Workers.

## DNS

```bash
ordivon-cloudflare dns list
ordivon-cloudflare dns list --name mcp --type CNAME
ordivon-cloudflare dns upsert CNAME api target.example.com --proxied true
ordivon-cloudflare dns delete CNAME api
```

Mutations require an interactive confirmation. Use `--yes` only in an already-reviewed automated plan.

## Zone settings

```bash
ordivon-cloudflare setting get ssl
ordivon-cloudflare setting get always_use_https
ordivon-cloudflare setting set always_use_https on
ordivon-cloudflare setting set min_tls_version '"1.2"'
```

Values are parsed as JSON when possible; otherwise they are sent as strings.

## Account resources

```bash
ordivon-cloudflare resource list d1
ordivon-cloudflare resource list kv
ordivon-cloudflare resource list r2
ordivon-cloudflare resource list queue
ordivon-cloudflare resource list ai-gateway

ordivon-cloudflare resource create d1 ordivon-dev-db --location apac
ordivon-cloudflare resource create kv ordivon-dev-kv
ordivon-cloudflare resource create r2 ordivon-dev-assets --location apac
ordivon-cloudflare resource create queue ordivon-dev-jobs
ordivon-cloudflare resource create ai-gateway ordivon-ai
```

R2 creation will fail with a clear Cloudflare error until R2 has been enabled once in the Dashboard.

## Generic API escape hatch

The CLI is not limited to its high-level commands:

```bash
ordivon-cloudflare request GET '/zones/{zone_id}/rulesets'
ordivon-cloudflare request GET '/accounts/{account_id}/cfd_tunnel' --query is_deleted=false
ordivon-cloudflare request PATCH '/zones/{zone_id}/settings/always_use_https' \
  --data '{"value":"on"}'
```

`{account_id}` and `{zone_id}` are replaced from the local configuration. POST, PUT, PATCH, and DELETE requests require confirmation unless `--yes` is supplied.

## Replacing or revoking the token

```bash
ordivon-cloudflare setup --force
```

After replacing it locally, revoke the old token in the Cloudflare Dashboard. Never place the token in Git, Issues, shell history, or chat messages.
