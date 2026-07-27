from __future__ import annotations

import importlib.util
import pathlib
import sys
import tempfile
import unittest
from unittest import mock


MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "scripts" / "ordivon_cloudflare.py"
SPEC = importlib.util.spec_from_file_location("ordivon_cloudflare", MODULE_PATH)
assert SPEC and SPEC.loader
cf = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = cf
SPEC.loader.exec_module(cf)


class CloudflareControlTests(unittest.TestCase):
    def test_parse_scalar(self) -> None:
        self.assertTrue(cf.parse_scalar("true"))
        self.assertEqual(cf.parse_scalar("123"), 123)
        self.assertEqual(cf.parse_scalar('{"a":1}'), {"a": 1})
        self.assertEqual(cf.parse_scalar("strict"), "strict")

    def test_normalize_api_token(self) -> None:
        self.assertEqual(cf.normalize_api_token("  abc  "), "abc")
        self.assertEqual(cf.normalize_api_token("Bearer abc"), "abc")
        self.assertEqual(cf.normalize_api_token('"abc"'), "abc")
        self.assertEqual(cf.normalize_api_token("'abc'"), "abc")

    def test_classify_api_token(self) -> None:
        self.assertEqual(cf.classify_api_token("cfut_example"), "user")
        self.assertEqual(cf.classify_api_token("cfat_example"), "account")
        self.assertEqual(cf.classify_api_token("cfk_example"), "global_api_key")
        self.assertEqual(cf.classify_api_token("legacy-token"), "legacy_or_unknown")

    def test_client_sends_exact_bearer_header(self) -> None:
        class Response:
            status = 200

            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc, traceback):
                return False

            def read(self):
                return b'{"success":true,"result":{"status":"active"},"errors":[],"messages":[]}'

        config = cf.Config(
            api_token="cfat_test-secret",
            account_id="account",
            zone_id="zone",
            zone_name="ordivon.com",
            token_type="account",
        )
        client = cf.CloudflareClient(config, retries=0)
        with mock.patch.object(cf.urllib.request, "urlopen", return_value=Response()) as urlopen:
            client.request("GET", "/accounts/account/tokens/verify")
        request = urlopen.call_args.args[0]
        self.assertEqual(request.get_header("Authorization"), "Bearer cfat_test-secret")
        self.assertEqual(request.full_url, "https://api.cloudflare.com/client/v4/accounts/account/tokens/verify")

    def test_verify_uses_user_endpoint(self) -> None:
        class Client:
            def __init__(self) -> None:
                self.paths = []

            def request(self, method, path):
                self.paths.append(path)
                return {"result": {"status": "active"}}

        client = Client()
        _, verified_type = cf.verify_api_token(client, "user", "account-id")
        self.assertEqual(verified_type, "user")
        self.assertEqual(client.paths, ["/user/tokens/verify"])

    def test_verify_uses_account_endpoint(self) -> None:
        class Client:
            def __init__(self) -> None:
                self.paths = []

            def request(self, method, path):
                self.paths.append(path)
                return {"result": {"status": "active"}}

        client = Client()
        _, verified_type = cf.verify_api_token(client, "account", "account-id")
        self.assertEqual(verified_type, "account")
        self.assertEqual(client.paths, ["/accounts/account-id/tokens/verify"])

    def test_verify_legacy_falls_back_to_account_endpoint(self) -> None:
        class Client:
            def __init__(self) -> None:
                self.paths = []

            def request(self, method, path):
                self.paths.append(path)
                if path == "/user/tokens/verify":
                    raise cf.CloudflareError("invalid", status=401)
                return {"result": {"status": "active"}}

        client = Client()
        _, verified_type = cf.verify_api_token(client, "legacy_or_unknown", "account-id")
        self.assertEqual(verified_type, "account")
        self.assertEqual(
            client.paths,
            ["/user/tokens/verify", "/accounts/account-id/tokens/verify"],
        )

    def test_normalize_dns_name(self) -> None:
        self.assertEqual(cf.normalize_dns_name("@", "ordivon.com"), "ordivon.com")
        self.assertEqual(cf.normalize_dns_name("www", "ordivon.com"), "www.ordivon.com")
        self.assertEqual(cf.normalize_dns_name("mcp.ordivon.com.", "ordivon.com"), "mcp.ordivon.com")

    def test_format_path(self) -> None:
        config = cf.Config("token", "account", "zone", "ordivon.com")
        self.assertEqual(cf.format_path("/accounts/{account_id}/zones/{zone_id}", config), "/accounts/account/zones/zone")

    def test_config_round_trip_and_permissions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "cloudflare.json"
            config = cf.Config("secret", "account", "zone", "ordivon.com")
            cf.save_config(path, config)
            loaded = cf.load_config(path)
            self.assertEqual(loaded, config)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_resource_create_bodies(self) -> None:
        self.assertEqual(cf.resource_create_body("d1", "db", "apac"), {"name": "db", "primary_location_hint": "apac"})
        self.assertEqual(cf.resource_create_body("kv", "config"), {"title": "config"})
        self.assertEqual(cf.resource_create_body("r2", "assets", "apac"), {"name": "assets", "locationHint": "apac"})
        self.assertEqual(cf.resource_create_body("queue", "jobs"), {"queue_name": "jobs"})
        self.assertEqual(cf.resource_create_body("ai-gateway", "ordivon-ai")["id"], "ordivon-ai")


if __name__ == "__main__":
    unittest.main()
