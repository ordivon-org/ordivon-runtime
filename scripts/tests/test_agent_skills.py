from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def load_script(name: str):
    path = REPO / "scripts" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(f"ordivon_{name}", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


report = load_script("ordivon_report")
git_skill = load_script("ordivon_git_skill")
github_skill = load_script("ordivon_github_skill")


class ReportAdapterTests(unittest.TestCase):
    def test_cargo_json_returns_primary_error_location(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "cargo.jsonl"
            path.write_text(
                json.dumps(
                    {
                        "reason": "compiler-message",
                        "message": {
                            "level": "error",
                            "message": "u64: ToSql is not satisfied",
                            "code": {"code": "E0277"},
                            "spans": [
                                {
                                    "file_name": "src/registry.rs",
                                    "line_start": 1432,
                                    "column_start": 9,
                                    "is_primary": True,
                                }
                            ],
                        },
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            normalized = report.normalize_cargo(path)
            self.assertEqual(normalized["status"], "failed")
            self.assertEqual(normalized["issues"][0]["file"], "src/registry.rs")
            self.assertEqual(normalized["issues"][0]["line"], 1432)
            self.assertEqual(normalized["issues"][0]["ruleId"], "E0277")

    def test_junit_counts_failures_and_skips(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "junit.xml"
            path.write_text(
                """<testsuite><testcase classname="suite" name="ok"/><testcase classname="suite" name="bad"><failure message="boom"/></testcase><testcase name="skip"><skipped/></testcase></testsuite>""",
                encoding="utf-8",
            )
            normalized = report.normalize_junit(path)
            self.assertEqual(normalized["summary"], {"tests": 3, "passed": 1, "failed": 1, "skipped": 1})
            self.assertEqual(normalized["issues"][0]["test"], "suite::bad")

    def test_sarif_preserves_rule_and_region(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.sarif"
            path.write_text(
                json.dumps(
                    {
                        "runs": [
                            {
                                "results": [
                                    {
                                        "level": "warning",
                                        "ruleId": "R1",
                                        "message": {"text": "review this"},
                                        "locations": [
                                            {
                                                "physicalLocation": {
                                                    "artifactLocation": {"uri": "src/main.rs"},
                                                    "region": {"startLine": 7, "startColumn": 2},
                                                }
                                            }
                                        ],
                                    }
                                ]
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            normalized = report.normalize_sarif(path)
            self.assertEqual(normalized["status"], "passed")
            self.assertEqual(normalized["summary"]["warnings"], 1)
            self.assertEqual(normalized["issues"][0]["ruleId"], "R1")


class GitSkillTests(unittest.TestCase):
    def test_status_reports_detached_head_and_untracked_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "Test"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.email", "test@example.invalid"], check=True)
            (repo / "tracked.txt").write_text("baseline\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "tracked.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "baseline"], check=True)
            subprocess.run(["git", "-C", str(repo), "checkout", "--detach", "-q"], check=True)
            (repo / "new.txt").write_text("new\n", encoding="utf-8")
            value = git_skill.status(repo)
            self.assertEqual(value["headMode"], "detached")
            self.assertTrue(value["dirty"])
            self.assertIn("new.txt", [item["path"] for item in value["files"]])

    def test_publish_ref_requires_explicit_branch_namespace(self) -> None:
        with self.assertRaisesRegex(ValueError, "refs/heads"):
            git_skill.validate_publish_ref("topic")
        with self.assertRaisesRegex(ValueError, "valid Git reference"):
            git_skill.validate_publish_ref("refs/heads/topic..invalid")
        git_skill.validate_publish_ref("refs/heads/topic")

    def test_publish_rejects_dirty_repository_before_push(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "Test"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.email", "test@example.invalid"], check=True)
            (repo / "tracked.txt").write_text("baseline\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "tracked.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "baseline"], check=True)
            (repo / "tracked.txt").write_text("dirty\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "repository is dirty"):
                git_skill.publish(repo, "origin", "refs/heads/topic")


class GitHubSkillTests(unittest.TestCase):
    def test_commands_are_structured_and_check_summary_is_stable(self) -> None:
        command = github_skill.pr_view_command("owner/repository", "123")
        self.assertEqual(command[:4], ["pr", "view", "123", "--repo"])
        self.assertNotIn("shell", command)
        summary = github_skill.summarize_checks(
            [
                {"name": "build", "bucket": "pass"},
                {"name": "test", "bucket": "pending"},
            ]
        )
        self.assertEqual(summary["status"], "pending")
        self.assertEqual(summary["summary"], {"total": 2, "pending": 1, "failed": 0})


if __name__ == "__main__":
    unittest.main()
