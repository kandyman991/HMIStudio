import unittest
from unittest.mock import patch
import subprocess
from plan import (classify, changed_paths, ci03_event, ci03_pin, ci03_paths,
                  ci03_intent, full_checkpoint, ci03_targeted, FULL_DESKTOP, FULL_TAG)

class PlanTests(unittest.TestCase):
    def assert_flags(self, paths, expected, force=False):
        self.assertEqual({k for k, v in classify(paths, force).items() if v}, set(expected))

    def test_docs(self):
        self.assert_flags(["AGENTS.md", "docs/reuse/AUDIT_SUMMARY.md", "handoff/state.json"], [])

    def test_roadmap_and_routing_only(self):
        self.assert_flags([
            "roadmap/milestones/DEV-01.json", "roadmap/dashboard.html",
            "roadmap/generate.mjs", "docs/MILESTONE_PLAN.md",
            ".github/ci/plan.py", ".github/ci/test_plan.py", ".github/ci/test_roadmap.py",
        ], [])
        self.assert_flags(["roadmap/dashboard.html", "crates/hmi-engine/src/lib.rs"],
                          ["frontend", "native", "crates"])
        self.assert_flags(["roadmap/dashboard.html", "src-tauri/tauri.conf.json"],
                          ["frontend", "native", "package"])

    def test_frontend(self):
        self.assert_flags(["src/app.tsx", "tests/workspace.test.mjs"], ["frontend"])

    def test_crate_and_downstream(self):
        for crate in ["project-model", "engineering-core", "tag-engine", "future"]:
            self.assert_flags([f"crates/{crate}/src/lib.rs"], ["crates", "frontend", "native"])

    def test_native(self):
        self.assert_flags(["src-tauri/src/main.rs", "src-tauri/capabilities/default.json"], ["frontend", "native"])

    def test_packaging(self):
        for path in ["src-tauri/tauri.conf.json", "src-tauri/Cargo.lock", "src-tauri/build.rs", "assets/icon.svg", "package-lock.json"]:
            self.assert_flags([path], ["frontend", "native", "package"])

    def test_unknown_and_ci(self):
        for path in [".github/workflows/ci.yml", ".github/ci/check_engine_notices.py", "future-build.sh", "docs/reference.html"]:
            self.assert_flags([path], ["frontend", "native", "package", "crates"])

    def test_force_and_union(self):
        self.assert_flags(["README.md"], ["frontend", "native", "package", "crates"], True)
        self.assert_flags(["crates/project-model/src/lib.rs", "assets/icon.svg"], ["frontend", "native", "package", "crates"])

    def test_diff_failure_and_initial_push(self):
        self.assertIsNone(changed_paths("0" * 40, "head", "push"))
        with patch("plan.git", side_effect=subprocess.CalledProcessError(1, "git")):
            self.assertIsNone(changed_paths("base", "head", "pull_request"))

    def test_rename_is_delete_and_add(self):
        with patch("plan.git", side_effect=[b"ancestor\n", b"src-tauri/build.rs\0docs/build.md\0"]) as mock:
            paths = changed_paths("base", "head", "pull_request")
            self.assert_flags(paths, ["frontend", "native", "package"])
            mock.assert_called_with("diff", "--no-renames", "--name-only", "-z", "ancestor", "head", "--")


    def ci03_env(self):
        return {
            "CI_EVENT": "pull_request", "CI_ACTION": "synchronize",
            "CI_PR": "36", "CI_DRAFT": "true",
            "GITHUB_REPOSITORY": "kandyman991/HMIStudio",
            "CI_HEAD_REPO": "kandyman991/HMIStudio",
            "CI_HEAD_REF": "feat/reuse-03b-legacy-import", "CI_BASE_REF": "main",
            "CI_BASE": "b" * 40, "CI_HEAD": "c" * 40, "CI03_TOKEN": "test-token",
        }

    def test_ci03_event_is_scoped(self):
        env = self.ci03_env()
        self.assertTrue(ci03_event(env))
        for key, value in {
            "CI_EVENT": "workflow_dispatch", "CI_ACTION": "ready_for_review",
            "CI_PR": "37", "CI_DRAFT": "false", "CI_HEAD_REPO": "fork/HMIStudio",
            "CI_HEAD_REF": "other", "CI_BASE_REF": "other",
            "GITHUB_REPOSITORY": "fork/HMIStudio",
        }.items():
            self.assertFalse(ci03_event(dict(env, **{key: value})), key)

    def test_ci03_pin_and_risk_paths(self):
        self.assertEqual(ci03_pin("a" * 40 + " " + "b" * 40, "b" * 40), "a" * 40)
        self.assertIsNone(ci03_pin("disabled", "b" * 40))
        self.assertIsNone(ci03_pin("a" * 40 + " " + "b" * 40, "c" * 40))
        self.assertTrue(ci03_paths(["crates/legacy-import/src/graphics.rs",
                                    "crates/legacy-import/tests/portable.rs"]))
        for path in ["crates/legacy-import/Cargo.lock", "crates/legacy-import/src/package.rs",
                     "crates/project-model/src/lib.rs", "crates/engineering-core/src/lib.rs",
                     "src-tauri/src/runtime_host.rs", ".github/ci/plan.py",
                     ".github/workflows/ci.yml", "unknown"]:
            self.assertFalse(ci03_paths(["crates/legacy-import/src/graphics.rs", path]), path)
        self.assertFalse(ci03_paths([]))
        self.assertFalse(ci03_paths(None))

    def test_ci03_intent_must_be_explicit_for_each_commit(self):
        self.assertTrue(ci03_intent(["message\n\nCI-03: importer-only", "CI-03: importer-only"]))
        self.assertFalse(ci03_intent(["CI-03: importer-only", "risk change"]))
        self.assertFalse(ci03_intent(["CI-03: importer-only\nCI-03: full"]))
        self.assertFalse(ci03_intent([]))

    def test_ci03_full_checkpoint_requires_actual_successful_jobs(self):
        names = ["Desktop CI", "Tag engine CI", "Engineering handoff snapshot"]
        runs = [dict(id=i, name=n, head_sha="a" * 40,
                     head_branch="feat/reuse-03b-legacy-import",
                     event="pull_request", status="completed", conclusion="success")
                for i, n in enumerate(names, 1)]
        def jobs(required):
            return {"total_count": len(required), "jobs": [
                {"name": name, "conclusion": "success"} for name in sorted(required)]}
        complete = [{"workflow_runs": runs}, jobs(FULL_DESKTOP), jobs(FULL_TAG), jobs({"snapshot"})]
        with patch("plan.ci03_api", side_effect=complete):
            self.assertTrue(full_checkpoint("a" * 40, "token"))
        incomplete = jobs(FULL_DESKTOP)
        incomplete["jobs"][0]["conclusion"] = "skipped"
        with patch("plan.ci03_api", side_effect=[{"workflow_runs": runs}, incomplete]):
            self.assertFalse(full_checkpoint("a" * 40, "token"))
        self.assertFalse(full_checkpoint("a" * 40, ""))

    def test_ci03_full_fallback_on_missing_evidence(self):
        env = self.ci03_env()
        with patch("plan.Path.read_text", return_value="disabled"):
            self.assertFalse(ci03_targeted(env))
        pin = "a" * 40 + " " + "b" * 40
        with patch("plan.Path.read_text", return_value=pin), \
                patch("plan.git", side_effect=subprocess.CalledProcessError(1, "git")):
            self.assertFalse(ci03_targeted(env))
        with patch("plan.Path.read_text", return_value=pin), \
                patch("plan.git", side_effect=[b"", b"", b"routine\nCI-03: importer-only\0"]), \
                patch("plan.changed_paths", return_value=["crates/legacy-import/src/graphics.rs"]), \
                patch("plan.full_checkpoint", side_effect=OSError("unavailable")):
            self.assertFalse(ci03_targeted(env))
        with patch("plan.Path.read_text", return_value=pin), \
                patch("plan.git", side_effect=[b"", b"", b"routine\nCI-03: importer-only\0"]), \
                patch("plan.changed_paths", return_value=["crates/legacy-import/src/graphics.rs"]), \
                patch("plan.full_checkpoint", return_value=True):
            self.assertTrue(ci03_targeted(env))

if __name__ == "__main__":
    unittest.main()
