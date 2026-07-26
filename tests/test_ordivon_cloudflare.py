from __future__ import annotations

import importlib.util
import pathlib
import sys
import tempfile
import unittest


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
