import importlib.util
from pathlib import Path
from unittest import TestCase

spec = importlib.util.spec_from_file_location("roadmap_sync", Path(__file__).resolve().parents[2] / "roadmap/sync.py")
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)


class SyncTests(TestCase):
    def test_snapshot_drops_old_head_and_selects_latest_run(self):
        head = "b" * 40
        info = {"number": 21, "head": {"sha": head, "ref": "feat/test", "repo": {"full_name": sync.REPO}},
                "base": {"ref": "main", "repo": {"full_name": sync.REPO}}, "merged": False, "state": "open", "draft": False}
        def run(id, sha, result):
            return {"id": id, "name": "Desktop CI", "head_sha": sha, "event": "pull_request",
                    "status": "completed", "conclusion": result}
        new = sync.pr_snapshot({"number": 21}, info, [run(1, "a"*40, "success"), run(2, head, "failure"), run(3, head, "success")], "now")
        self.assertEqual(new["runs"], [{"id": 3, "name": "Desktop CI", "head": head, "state": "completed", "result": "success"}])
        self.assertEqual(sync.pr_snapshot(new, info, [run(3, head, "success")], "later"), new)

    def test_only_explicit_completion_pr_advances_and_never_accepts(self):
        runs = [{"name": name, "state": "completed", "result": "success"} for name in sync.REQUIRED]
        pr = {"number": 21, "state": "merged", "merge": "b"*40, "runs": runs}
        task = {"phase": "proposed", "proposal": "review", "prs": [pr], "assignment": {"owner": "agent"}}
        sync.advance(task, "2026-11-25T00:00:00Z")
        self.assertEqual(task["phase"], "proposed")
        task["completionPr"] = 21
        task["requiredWorkflows"] = ["Tag engine CI"]
        sync.advance(task, "2026-11-25T00:00:00Z")
        self.assertEqual(task["phase"], "proposed")
        runs.append({"name": "Tag engine CI", "state": "completed", "result": "success"})
        sync.advance(task, "2026-11-25T00:00:00Z")
        self.assertEqual(task["phase"], "merged")
        self.assertNotIn("acceptance", task)
        self.assertNotIn("assignment", task)

    def test_closed_candidate_is_not_completed(self):
        task = {"phase": "proposed", "proposal": "review", "completionPr": 1,
                "prs": [{"number": 1, "state": "closed"}]}
        sync.advance(task, "2026-11-25T00:00:00Z")
        self.assertEqual(task["phase"], "planned")
        self.assertNotIn("completionPr", task)
