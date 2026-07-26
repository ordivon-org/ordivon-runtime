#!/usr/bin/env python3
"""Minimal, auditable Cloudflare control CLI for Ordivon.

The CLI uses only the Python standard library. Run `setup` once, paste a
Cloudflare API token into the hidden prompt, and the CLI discovers the account
and zone identifiers automatically.
"""

from __future__ import annotations

import argparse
import getpass
import json
import os
import pathlib
import socket
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from typing import Any

API_BASE = "https://api.cloudflare.com/client/v4"
DEFAULT_ZONE = "ordivon.com"
DEFAULT_TIMEOUT_SECONDS = 30
CONFIG_VERSION = 1


class CloudflareError(RuntimeError):
    """A Cloudflare API or local configuration failure."""

    def __init__(
        self,
        message: str,
        *,
        status: int | None = None,
        errors: list[dict[str, Any]] | None = None,
        payload: Any = None,
    ) -> None:
        super().__init__(message)
        self.status = status
        self.errors = errors or []
        self.payload = payload


@dataclass(frozen=True)
class Config:
    api_token: str
    account_id: str
    zone_id: str
    zone_name: str
    api_base: str = API_BASE
    version: int = CONFIG_VERSION

    def public(self) -> dict[str, Any]:
        value = asdict(self)
        value.pop("api_token", None)
        return value


def default_config_path() -> pathlib.Path:
    override = os.environ.get("ORDIVON_CLOUDFLARE_CONFIG")
    if override:
        return pathlib.Path(override).expanduser()
    base = pathlib.Path(os.environ.get("XDG_CONFIG_HOME", pathlib.Path.home() / ".config"))
    return base / "ordivon" / "secrets" / "cloudflare.json"


def parse_scalar(raw: str) -> Any:
    """Parse JSON scalars/objects while preserving ordinary strings."""
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw


def normalize_api_token(raw: str) -> str:
    """Normalize common copy/paste wrappers without changing the token itself."""
    token = raw.strip()
    if token.lower().startswith("bearer "):
        token = token[7:].strip()
    if len(token) >= 2 and token[0] == token[-1] and token[0] in {"'", '"'}:
        token = token[1:-1].strip()
    return token


def normalize_dns_name(name: str, zone_name: str) -> str:
    value = name.strip().rstrip(".")
    zone = zone_name.rstrip(".")
    if value in {"", "@"}:
        return zone
    if value == zone or value.endswith(f".{zone}"):
        return value
    return f"{value}.{zone}"


def format_path(path: str, config: Config) -> str:
    return path.replace("{account_id}", config.account_id).replace("{zone_id}", config.zone_id)


def _atomic_write(path: pathlib.Path, text: str, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(path.parent, 0o700)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent, text=True)
    try:
        os.fchmod(fd, mode)
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        os.chmod(path, mode)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def save_config(path: pathlib.Path, config: Config) -> None:
    _atomic_write(path, json.dumps(asdict(config), ensure_ascii=False, indent=2) + "\n")


def load_config(path: pathlib.Path) -> Config:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise CloudflareError(f"Cloudflare configuration not found: {path}. Run `ordivon-cloudflare setup`.") from exc
    except (OSError, json.JSONDecodeError) as exc:
        raise CloudflareError(f"Cannot read Cloudflare configuration {path}: {exc}") from exc

    token = os.environ.get("CLOUDFLARE_API_TOKEN") or raw.get("api_token")
    account_id = os.environ.get("CLOUDFLARE_ACCOUNT_ID") or raw.get("account_id")
    zone_id = os.environ.get("CLOUDFLARE_ZONE_ID") or raw.get("zone_id")
    zone_name = os.environ.get("CLOUDFLARE_ZONE_NAME") or raw.get("zone_name")
    missing = [
        name
        for name, value in {
            "api_token": token,
            "account_id": account_id,
            "zone_id": zone_id,
            "zone_name": zone_name,
        }.items()
        if not value
    ]
    if missing:
        raise CloudflareError(f"Cloudflare configuration is missing: {', '.join(missing)}")
    return Config(
        api_token=str(token),
        account_id=str(account_id),
        zone_id=str(zone_id),
        zone_name=str(zone_name),
        api_base=str(raw.get("api_base") or API_BASE),
        version=int(raw.get("version") or CONFIG_VERSION),
    )


class CloudflareClient:
    def __init__(self, config: Config, *, timeout: int = DEFAULT_TIMEOUT_SECONDS, retries: int = 3) -> None:
        self.config = config
        self.timeout = timeout
        self.retries = retries

    def request(
        self,
        method: str,
        path: str,
        *,
        query: dict[str, Any] | None = None,
        body: Any = None,
        check_success: bool = True,
    ) -> dict[str, Any]:
        method = method.upper()
        rendered_path = format_path(path, self.config)
        if not rendered_path.startswith("/"):
            rendered_path = "/" + rendered_path
        url = self.config.api_base.rstrip("/") + rendered_path
        if query:
            clean_query = {key: value for key, value in query.items() if value is not None}
            url += "?" + urllib.parse.urlencode(clean_query, doseq=True)

        headers = {
            "Authorization": f"Bearer {self.config.api_token}",
            "Accept": "application/json",
            "User-Agent": "ordivon-cloudflare-control/1",
        }
        encoded_body: bytes | None = None
        if body is not None:
            encoded_body = json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
            headers["Content-Type"] = "application/json"

        request = urllib.request.Request(url, data=encoded_body, headers=headers, method=method)
        last_error: BaseException | None = None

        for attempt in range(self.retries + 1):
            try:
                with urllib.request.urlopen(request, timeout=self.timeout) as response:
                    status = response.status
                    raw = response.read()
                    payload = self._decode(raw, status)
                    return self._validate(payload, status, check_success)
            except urllib.error.HTTPError as exc:
                status = exc.code
                raw = exc.read()
                payload = self._decode(raw, status)
                if status == 429 or 500 <= status <= 599:
                    if attempt < self.retries:
                        retry_after = exc.headers.get("Retry-After")
                        delay = float(retry_after) if retry_after and retry_after.isdigit() else min(2**attempt, 8)
                        time.sleep(delay)
                        continue
                return self._validate(payload, status, check_success)
            except (urllib.error.URLError, TimeoutError, socket.timeout) as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(min(2**attempt, 8))
                    continue
                break

        raise CloudflareError(f"Cloudflare request failed after retries: {last_error}")

    @staticmethod
    def _decode(raw: bytes, status: int) -> dict[str, Any]:
        if not raw:
            return {"success": 200 <= status < 300, "result": None, "errors": [], "messages": []}
        try:
            payload = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise CloudflareError(f"Cloudflare returned a non-JSON response (HTTP {status})", status=status) from exc
        if not isinstance(payload, dict):
            raise CloudflareError(f"Cloudflare returned an unexpected response shape (HTTP {status})", status=status)
        return payload

    @staticmethod
    def _validate(payload: dict[str, Any], status: int, check_success: bool) -> dict[str, Any]:
        success = bool(payload.get("success", 200 <= status < 300))
        if check_success and (not 200 <= status < 300 or not success):
            errors = payload.get("errors") if isinstance(payload.get("errors"), list) else []
            detail = "; ".join(str(item.get("message") or item) for item in errors) or "unknown API error"
            raise CloudflareError(f"Cloudflare API error (HTTP {status}): {detail}", status=status, errors=errors, payload=payload)
        payload.setdefault("http_status", status)
        return payload


def emit(value: Any) -> None:
    json.dump(value, sys.stdout, ensure_ascii=False, indent=2, sort_keys=False)
    sys.stdout.write("\n")


def confirm(message: str, assume_yes: bool) -> None:
    if assume_yes:
        return
    if not sys.stdin.isatty():
        raise CloudflareError(f"Refusing non-interactive mutation without --yes: {message}")
    answer = input(f"{message} [y/N] ").strip().lower()
    if answer not in {"y", "yes"}:
        raise CloudflareError("Operation cancelled")


def command_setup(args: argparse.Namespace) -> int:
    path = pathlib.Path(args.config).expanduser()
    if path.exists() and not args.force:
        raise CloudflareError(f"Configuration already exists: {path}. Use --force to replace it.")

    if args.token_stdin:
        token = normalize_api_token(sys.stdin.readline())
    else:
        token = normalize_api_token(getpass.getpass("Cloudflare API token: "))
    if not token:
        raise CloudflareError("Token cannot be empty")
    if any(character.isspace() for character in token):
        raise CloudflareError("The pasted credential contains internal whitespace and is not a valid Cloudflare API token")

    bootstrap = Config(api_token=token, account_id="", zone_id="", zone_name=args.zone)
    client = CloudflareClient(bootstrap)
    try:
        verification = client.request("GET", "/user/tokens/verify")
    except CloudflareError as exc:
        if exc.status == 401:
            raise CloudflareError(
                "Cloudflare rejected this credential as an invalid API token. Create a token under "
                "Cloudflare Dashboard > My Profile > API Tokens (or Manage Account > API Tokens). "
                "Paste the one-time token secret, not a token name/ID, Global API Key, Tunnel token, "
                "or Zero Trust Access Service Token client ID/secret.",
                status=exc.status,
                errors=exc.errors,
                payload=exc.payload,
            ) from exc
        raise
    status = (verification.get("result") or {}).get("status")
    if status not in {None, "active"}:
        raise CloudflareError(f"Cloudflare token is not active: {status}")

    zones = client.request("GET", "/zones", query={"name": args.zone, "per_page": 50}).get("result") or []
    exact = [zone for zone in zones if zone.get("name") == args.zone]
    if len(exact) != 1:
        names = [zone.get("name") for zone in zones]
        raise CloudflareError(f"Expected exactly one accessible zone named {args.zone!r}; found {len(exact)}. Visible zones: {names}")

    zone = exact[0]
    account_id = ((zone.get("account") or {}).get("id"))
    zone_id = zone.get("id")
    if not account_id or not zone_id:
        raise CloudflareError("Cloudflare zone response did not contain account_id and zone_id")

    config = Config(api_token=token, account_id=account_id, zone_id=zone_id, zone_name=args.zone)
    save_config(path, config)
    emit({"configured": True, "config_path": str(path), **config.public()})
    return 0


def command_verify(args: argparse.Namespace, client: CloudflareClient) -> int:
    result = client.request("GET", "/user/tokens/verify")
    emit({"config": client.config.public(), "token": result.get("result")})
    return 0


def _capture(client: CloudflareClient, method: str, path: str, *, query: dict[str, Any] | None = None) -> dict[str, Any]:
    try:
        response = client.request(method, path, query=query)
        return {
            "ok": True,
            "http_status": response.get("http_status"),
            "result": response.get("result"),
            "result_info": response.get("result_info"),
        }
    except CloudflareError as exc:
        return {
            "ok": False,
            "http_status": exc.status,
            "errors": exc.errors,
            "message": str(exc),
        }


def inventory(client: CloudflareClient) -> dict[str, Any]:
    account = client.config.account_id
    zone = client.config.zone_id
    endpoints = {
        "zone": ("GET", f"/zones/{zone}", None),
        "dns_records": ("GET", f"/zones/{zone}/dns_records", {"per_page": 1000}),
        "zone_settings": ("GET", f"/zones/{zone}/settings", None),
        "rulesets": ("GET", f"/zones/{zone}/rulesets", {"per_page": 50}),
        "certificate_packs": ("GET", f"/zones/{zone}/ssl/certificate_packs", {"status": "all", "per_page": 50}),
        "tunnels": ("GET", f"/accounts/{account}/cfd_tunnel", {"is_deleted": "false", "per_page": 1000}),
        "access_apps": ("GET", f"/accounts/{account}/access/apps", {"per_page": 1000}),
        "workers": ("GET", f"/accounts/{account}/workers/scripts", None),
        "pages_projects": ("GET", f"/accounts/{account}/pages/projects", {"page": 1, "per_page": 10}),
        "d1_databases": ("GET", f"/accounts/{account}/d1/database", {"page": 1, "per_page": 1000}),
        "kv_namespaces": ("GET", f"/accounts/{account}/storage/kv/namespaces", {"page": 1, "per_page": 1000}),
        "r2_buckets": ("GET", f"/accounts/{account}/r2/buckets", {"per_page": 1000}),
        "queues": ("GET", f"/accounts/{account}/queues", None),
        "ai_gateways": ("GET", f"/accounts/{account}/ai-gateway/gateways", {"page": 1, "per_page": 100}),
    }
    return {
        "generated_at": datetime.now(UTC).isoformat(),
        "config": client.config.public(),
        "resources": {
            name: _capture(client, method, path, query=query)
            for name, (method, path, query) in endpoints.items()
        },
    }


def command_doctor(args: argparse.Namespace, client: CloudflareClient) -> int:
    report = inventory(client)
    summary = {
        name: {
            "ok": item["ok"],
            "http_status": item.get("http_status"),
            "message": item.get("message"),
        }
        for name, item in report["resources"].items()
    }
    emit({"generated_at": report["generated_at"], "config": report["config"], "checks": summary})
    return 0


def command_inventory(args: argparse.Namespace, client: CloudflareClient) -> int:
    report = inventory(client)
    if args.output:
        destination = pathlib.Path(args.output).expanduser()
        _atomic_write(destination, json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        emit({"written": str(destination), "resource_count": len(report["resources"])})
    else:
        emit(report)
    return 0


def list_dns(client: CloudflareClient, *, name: str | None = None, record_type: str | None = None) -> list[dict[str, Any]]:
    query: dict[str, Any] = {"per_page": 1000}
    if name:
        query["name"] = normalize_dns_name(name, client.config.zone_name)
    if record_type:
        query["type"] = record_type.upper()
    response = client.request("GET", "/zones/{zone_id}/dns_records", query=query)
    result = response.get("result") or []
    if not isinstance(result, list):
        raise CloudflareError("Unexpected DNS list response")
    return result


def command_dns_list(args: argparse.Namespace, client: CloudflareClient) -> int:
    emit(list_dns(client, name=args.name, record_type=args.type))
    return 0


def command_dns_upsert(args: argparse.Namespace, client: CloudflareClient) -> int:
    record_type = args.type.upper()
    name = normalize_dns_name(args.name, client.config.zone_name)
    existing = list_dns(client, name=name, record_type=record_type)
    if len(existing) > 1:
        raise CloudflareError(f"Refusing ambiguous upsert: {len(existing)} {record_type} records exist for {name}")

    proxied: bool | None
    if args.proxied == "true":
        proxied = True
    elif args.proxied == "false":
        proxied = False
    else:
        proxied = None

    body: dict[str, Any] = {
        "type": record_type,
        "name": name,
        "content": args.content,
        "ttl": args.ttl,
    }
    if args.comment is not None:
        body["comment"] = args.comment
    if proxied is not None:
        body["proxied"] = proxied

    if existing:
        current = existing[0]
        if args.comment is None and current.get("comment") is not None:
            body["comment"] = current.get("comment")
        if proxied is None and current.get("proxied") is not None:
            body["proxied"] = current.get("proxied")
        confirm(f"Update DNS record {record_type} {name}", args.yes)
        response = client.request("PUT", f"/zones/{{zone_id}}/dns_records/{current['id']}", body=body)
        action = "updated"
    else:
        confirm(f"Create DNS record {record_type} {name}", args.yes)
        response = client.request("POST", "/zones/{zone_id}/dns_records", body=body)
        action = "created"

    emit({"action": action, "record": response.get("result")})
    return 0


def command_dns_delete(args: argparse.Namespace, client: CloudflareClient) -> int:
    record_type = args.type.upper()
    name = normalize_dns_name(args.name, client.config.zone_name)
    existing = list_dns(client, name=name, record_type=record_type)
    if not existing:
        raise CloudflareError(f"No {record_type} record found for {name}")
    if len(existing) > 1:
        raise CloudflareError(f"Refusing ambiguous delete: {len(existing)} records match {record_type} {name}")
    confirm(f"Delete DNS record {record_type} {name}", args.yes)
    response = client.request("DELETE", f"/zones/{{zone_id}}/dns_records/{existing[0]['id']}")
    emit({"deleted": response.get("result"), "previous": existing[0]})
    return 0


def command_setting_get(args: argparse.Namespace, client: CloudflareClient) -> int:
    emit(client.request("GET", f"/zones/{{zone_id}}/settings/{args.setting_id}").get("result"))
    return 0


def command_setting_set(args: argparse.Namespace, client: CloudflareClient) -> int:
    value = parse_scalar(args.value)
    confirm(f"Set zone setting {args.setting_id} to {value!r}", args.yes)
    response = client.request("PATCH", f"/zones/{{zone_id}}/settings/{args.setting_id}", body={"value": value})
    emit(response.get("result"))
    return 0


RESOURCE_ENDPOINTS: dict[str, tuple[str, str, str]] = {
    "d1": ("/accounts/{account_id}/d1/database", "name", "name"),
    "kv": ("/accounts/{account_id}/storage/kv/namespaces", "title", "title"),
    "r2": ("/accounts/{account_id}/r2/buckets", "name", "name"),
    "queue": ("/accounts/{account_id}/queues", "queue_name", "queue_name"),
    "ai-gateway": ("/accounts/{account_id}/ai-gateway/gateways", "id", "id"),
}


def resource_create_body(resource_type: str, name: str, location: str | None = None) -> dict[str, Any]:
    if resource_type == "d1":
        body: dict[str, Any] = {"name": name}
        if location:
            body["primary_location_hint"] = location
        return body
    if resource_type == "kv":
        return {"title": name}
    if resource_type == "r2":
        body = {"name": name}
        if location:
            body["locationHint"] = location
        return body
    if resource_type == "queue":
        return {"queue_name": name}
    if resource_type == "ai-gateway":
        return {
            "id": name,
            "collect_logs": False,
            "cache_ttl": None,
            "cache_invalidate_on_update": True,
            "rate_limiting_interval": None,
            "rate_limiting_limit": None,
        }
    raise CloudflareError(f"Unknown resource type: {resource_type}")


def command_resource_list(args: argparse.Namespace, client: CloudflareClient) -> int:
    endpoint = RESOURCE_ENDPOINTS[args.resource_type][0]
    query = {"page": 1, "per_page": 1000} if args.resource_type in {"d1", "kv"} else None
    emit(client.request("GET", endpoint, query=query).get("result"))
    return 0


def command_resource_create(args: argparse.Namespace, client: CloudflareClient) -> int:
    endpoint = RESOURCE_ENDPOINTS[args.resource_type][0]
    body = resource_create_body(args.resource_type, args.name, args.location)
    confirm(f"Create Cloudflare {args.resource_type} resource {args.name}", args.yes)
    emit(client.request("POST", endpoint, body=body).get("result"))
    return 0


def parse_query(items: list[str]) -> dict[str, str]:
    query: dict[str, str] = {}
    for item in items:
        if "=" not in item:
            raise CloudflareError(f"Query parameter must be KEY=VALUE: {item}")
        key, value = item.split("=", 1)
        if not key:
            raise CloudflareError(f"Query parameter key cannot be empty: {item}")
        query[key] = value
    return query


def command_request(args: argparse.Namespace, client: CloudflareClient) -> int:
    method = args.method.upper()
    if args.data is not None and args.data_file is not None:
        raise CloudflareError("Use only one of --data or --data-file")
    body = None
    if args.data is not None:
        try:
            body = json.loads(args.data)
        except json.JSONDecodeError as exc:
            raise CloudflareError(f"--data must be valid JSON: {exc}") from exc
    elif args.data_file is not None:
        try:
            body = json.loads(pathlib.Path(args.data_file).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise CloudflareError(f"Cannot read JSON body from {args.data_file}: {exc}") from exc

    if method in {"POST", "PUT", "PATCH", "DELETE"}:
        confirm(f"Send {method} request to {args.path}", args.yes)
    emit(client.request(method, args.path, query=parse_query(args.query), body=body))
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="ordivon-cloudflare", description=__doc__)
    parser.add_argument("--config", default=str(default_config_path()), help="configuration file path")
    subparsers = parser.add_subparsers(dest="command", required=True)

    setup = subparsers.add_parser("setup", help="prompt for a token and discover account/zone IDs")
    setup.add_argument("--zone", default=DEFAULT_ZONE)
    setup.add_argument("--token-stdin", action="store_true", help="read one token line from stdin instead of hidden prompt")
    setup.add_argument("--force", action="store_true", help="replace an existing configuration")

    subparsers.add_parser("verify", help="verify the configured token")
    subparsers.add_parser("doctor", help="probe permissions and enabled Cloudflare products")

    inventory_parser = subparsers.add_parser("inventory", help="read a full Cloudflare account/zone inventory")
    inventory_parser.add_argument("--output", help="write JSON atomically instead of printing it")

    dns = subparsers.add_parser("dns", help="manage DNS records")
    dns_sub = dns.add_subparsers(dest="dns_command", required=True)
    dns_list = dns_sub.add_parser("list")
    dns_list.add_argument("--name")
    dns_list.add_argument("--type")
    dns_upsert = dns_sub.add_parser("upsert")
    dns_upsert.add_argument("type")
    dns_upsert.add_argument("name")
    dns_upsert.add_argument("content")
    dns_upsert.add_argument("--proxied", choices=["preserve", "true", "false"], default="preserve")
    dns_upsert.add_argument("--ttl", type=int, default=1, help="1 means automatic")
    dns_upsert.add_argument("--comment")
    dns_upsert.add_argument("--yes", action="store_true")
    dns_delete = dns_sub.add_parser("delete")
    dns_delete.add_argument("type")
    dns_delete.add_argument("name")
    dns_delete.add_argument("--yes", action="store_true")

    setting = subparsers.add_parser("setting", help="read or update one zone setting")
    setting_sub = setting.add_subparsers(dest="setting_command", required=True)
    setting_get = setting_sub.add_parser("get")
    setting_get.add_argument("setting_id")
    setting_set = setting_sub.add_parser("set")
    setting_set.add_argument("setting_id")
    setting_set.add_argument("value", help="JSON value or plain string")
    setting_set.add_argument("--yes", action="store_true")

    resource = subparsers.add_parser("resource", help="list or create common account resources")
    resource_sub = resource.add_subparsers(dest="resource_command", required=True)
    resource_list = resource_sub.add_parser("list")
    resource_list.add_argument("resource_type", choices=sorted(RESOURCE_ENDPOINTS))
    resource_create = resource_sub.add_parser("create")
    resource_create.add_argument("resource_type", choices=sorted(RESOURCE_ENDPOINTS))
    resource_create.add_argument("name")
    resource_create.add_argument("--location", choices=["wnam", "enam", "weur", "eeur", "apac", "oc"])
    resource_create.add_argument("--yes", action="store_true")

    raw = subparsers.add_parser("request", help="call any Cloudflare v4 endpoint")
    raw.add_argument("method", choices=["GET", "POST", "PUT", "PATCH", "DELETE"])
    raw.add_argument("path", help="supports {account_id} and {zone_id} placeholders")
    raw.add_argument("--query", action="append", default=[], metavar="KEY=VALUE")
    raw.add_argument("--data", help="inline JSON request body")
    raw.add_argument("--data-file", help="JSON request body file")
    raw.add_argument("--yes", action="store_true")
    return parser


def dispatch(args: argparse.Namespace) -> int:
    if args.command == "setup":
        return command_setup(args)

    config = load_config(pathlib.Path(args.config).expanduser())
    client = CloudflareClient(config)

    if args.command == "verify":
        return command_verify(args, client)
    if args.command == "doctor":
        return command_doctor(args, client)
    if args.command == "inventory":
        return command_inventory(args, client)
    if args.command == "dns":
        if args.dns_command == "list":
            return command_dns_list(args, client)
        if args.dns_command == "upsert":
            return command_dns_upsert(args, client)
        if args.dns_command == "delete":
            return command_dns_delete(args, client)
    if args.command == "setting":
        if args.setting_command == "get":
            return command_setting_get(args, client)
        if args.setting_command == "set":
            return command_setting_set(args, client)
    if args.command == "resource":
        if args.resource_command == "list":
            return command_resource_list(args, client)
        if args.resource_command == "create":
            return command_resource_create(args, client)
    if args.command == "request":
        return command_request(args, client)
    raise CloudflareError(f"Unhandled command: {args.command}")


def main() -> int:
    parser = build_parser()
    try:
        return dispatch(parser.parse_args())
    except CloudflareError as exc:
        error = {"ok": False, "error": str(exc)}
        if exc.status is not None:
            error["http_status"] = exc.status
        if exc.errors:
            error["cloudflare_errors"] = exc.errors
        emit(error)
        return 1
    except KeyboardInterrupt:
        emit({"ok": False, "error": "Interrupted"})
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
