import unittest
from unittest.mock import patch
import subprocess
from plan import classify, changed_paths

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

if __name__ == "__main__":
    unittest.main()
