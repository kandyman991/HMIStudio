from unittest import TestCase
from unittest.mock import patch
import roadmap_gate as gate


class GateTests(TestCase):
    def run_gate(self, paths, current=None, claim=None, other=None):
        old = {"history": []}
        def git(*args):
            if args[0] == "merge-base":
                return "ancestor"
            if args[0] == "for-each-ref" and other:
                return "refs/remotes/origin/roadmap-claims/FIX-02"
            if args[0] == "diff":
                return "\0".join(paths)
            return ""
        def read(ref, path):
            return (other if ref.endswith("/FIX-02") else claim) if path == "roadmap/claim.json" else old if ref == "ancestor" else current
        with patch.object(gate, "git", side_effect=git), patch.object(gate, "read_at", side_effect=read):
            gate.check("base", "head", "feat/test")

    def test_no_implementation(self):
        self.run_gate(["docs/MILESTONE_PLAN.md", "roadmap/dashboard.html"])

    def test_missing_or_unrelated_milestone_fails(self):
        with self.assertRaises(ValueError):
            self.run_gate(["src/main.ts"])
        with self.assertRaises(ValueError):
            self.run_gate(["src/main.ts", "roadmap/milestones/FIX-01.json"], {"phase": "planned"})

    def test_scoped_claim_and_history_required(self):
        p = ["src/main.ts", "roadmap/milestones/FIX-01.json"]
        a = {"owner": "Claude", "branch": "feat/test", "scope": ["src/"]}
        r = {"id": "FIX-01", "phase": "proposed", "assignment": a,
             "history": [{"type": "completion_proposed"}]}
        claim = {"milestone": "FIX-01", **a}
        self.run_gate(p, r, claim)
        with self.assertRaisesRegex(ValueError, "Overlapping active reservation"):
            self.run_gate(p, r, claim, {"scope": ["src/main.ts"]})
        with self.assertRaises(ValueError):
            self.run_gate(p, r, {**claim, "branch": "feat/other"})
        with self.assertRaises(ValueError):
            self.run_gate(["crates/engine/src/lib.rs", p[1]], r, claim)
        with self.assertRaises(ValueError):
            self.run_gate(p, {**r, "history": []}, claim)
