import unittest
from unittest.mock import patch
import subprocess
from plan import (classify, changed_paths, ci03_event, ci03_pin, ci03_paths,
                  ci03_intent, checkpoint_pr, checkpoint_title, full_checkpoint, ci03_targeted,
                  FULL_DESKTOP, FULL_TAG, CI03_REPO_ID)

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

    def checkpoint_fixture(self):
        head, base, current = "a" * 40, "b" * 40, "c" * 40
        names = ["Desktop CI", "Tag engine CI", "Engineering handoff snapshot"]
        paths = ["ci.yml", "tag-engine.yml", "handoff.yml"]
        branch = "feat/reuse-03b-legacy-import"
        # The historical run retains its own head, while GitHub updates the
        # associated PR head after the pin commit advances the branch.
        pr = dict(number=36,
                  base=dict(ref="main", sha=base, repo=dict(id=CI03_REPO_ID)),
                  head=dict(ref=branch, sha=current, repo=dict(id=CI03_REPO_ID)))
        runs = [dict(id=i, name=n, path=".github/workflows/" + path,
                     head_sha=head, head_branch=branch, pull_requests=[pr],
                     display_title=checkpoint_title(head, base),
                     run_attempt=1, event="pull_request",
                     status="completed", conclusion="success")
                for i, (n, path) in enumerate(zip(names, paths), 1)]
        runs[2]["display_title"] = "Read-only handoff"
        def jobs(required, run_id):
            return {"total_count": len(required), "jobs": [
                dict(name=name, conclusion="success", status="completed",
                     head_sha=head, run_id=run_id, run_attempt=1)
                for name in sorted(required)]}
        complete = [{"workflow_runs": runs}, jobs(FULL_DESKTOP, 1),
                    jobs(FULL_TAG, 2), jobs({"snapshot"}, 3)]
        return head, base, current, runs, complete

    def test_ci03_full_checkpoint_requires_actual_successful_jobs(self):
        head, base, current, runs, complete = self.checkpoint_fixture()
        with patch("plan.ci03_api", side_effect=complete):
            self.assertTrue(full_checkpoint(head, base, "token", current))
        incomplete = complete[1]
        incomplete["jobs"][0]["conclusion"] = "skipped"
        with patch("plan.ci03_api", side_effect=[complete[0], incomplete]):
            self.assertFalse(full_checkpoint(head, base, "token", current))
        self.assertFalse(full_checkpoint(head, base, "", current))
        self.assertFalse(checkpoint_pr(runs[0], head, "d" * 40, current))
        self.assertFalse(checkpoint_pr(dict(runs[0], event="workflow_dispatch"),
                                       head, base, current))
        self.assertFalse(checkpoint_pr(dict(runs[0], pull_requests=[]),
                                       head, base, current))
        self.assertFalse(checkpoint_pr(runs[0], head, base, "d" * 40))
        wrong = [dict(runs[0], path=".github/workflows/other.yml"), *runs[1:]]
        with patch("plan.ci03_api", return_value={"workflow_runs": wrong}):
            self.assertFalse(full_checkpoint(head, base, "token", current))
        head, base, current, _, complete = self.checkpoint_fixture()
        tag = complete[2]
        tag["jobs"] = [j for j in tag["jobs"] if j["name"] != "Tag engine gate"]
        tag["total_count"] = len(tag["jobs"])
        with patch("plan.ci03_api", side_effect=complete):
            self.assertFalse(full_checkpoint(head, base, "token", current))

    def test_ci03_historical_event_evidence_is_required(self):
        for index in (0, 1):
            for bad_title in ("", "PR title", checkpoint_title("a" * 40, "d" * 40),
                              checkpoint_title("c" * 40, "b" * 40)):
                head, base, current, runs, complete = self.checkpoint_fixture()
                runs[index]["display_title"] = bad_title
                with (
                    patch("plan.ci03_api", side_effect=complete),
                    patch("plan.ci03_reject", return_value=False) as reason,
                ):
                    self.assertFalse(full_checkpoint(head, base, "token", current))
                    reason.assert_called_once_with("checkpoint-event-head-base-mismatch",
                                                   runs[index]["name"])

    def test_ci03_job_evidence_is_bound_to_attempt_and_head(self):
        for field, wrong in (("run_id", 9), ("run_attempt", 2),
                             ("head_sha", "c" * 40), ("status", "in_progress")):
            head, base, current, _, complete = self.checkpoint_fixture()
            complete[1]["jobs"][0][field] = wrong
            with (
                patch("plan.ci03_api", side_effect=complete),
                patch("plan.ci03_reject", return_value=False) as reason,
            ):
                self.assertFalse(full_checkpoint(head, base, "token", current))
                reason.assert_called_once_with("checkpoint-required-jobs-unverified",
                                               "Desktop CI")

    def test_ci03_full_fallback_on_missing_evidence(self):
        env = self.ci03_env()
        with patch("plan.Path.read_text", return_value="disabled"):
            self.assertFalse(ci03_targeted(env))
        pin = "a" * 40 + " " + "b" * 40
        with patch("plan.Path.read_text", return_value=pin), \
                patch("plan.git", side_effect=subprocess.CalledProcessError(1, "git")):
            self.assertFalse(ci03_targeted(env))
        with patch("plan.Path.read_text", return_value=pin), \
                patch("plan.git", side_effect=[b"", b"", b"", b"routine\nCI-03: importer-only\0"]), \
                patch("plan.changed_paths", return_value=["crates/legacy-import/src/graphics.rs"]), \
                patch("plan.full_checkpoint", side_effect=OSError("unavailable")):
            self.assertFalse(ci03_targeted(env))
        with patch("plan.Path.read_text", return_value=pin), \
                patch("plan.git", side_effect=[b"", b"", b"", b"routine\nCI-03: importer-only\0"]), \
                patch("plan.changed_paths", return_value=["crates/legacy-import/src/graphics.rs"]), \
                patch("plan.full_checkpoint", return_value=True):
            self.assertTrue(ci03_targeted(env))

if __name__ == "__main__":
    unittest.main()
