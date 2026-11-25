"""Validate the offline roadmap inside Desktop CI's existing required changes job."""
import json
import os
from pathlib import Path
import re
import subprocess
import unittest
from roadmap_gate import main as enforce_roadmap_gate


ROOT = Path(__file__).resolve().parents[2]


def integration_base():
    event_file = os.environ.get("GITHUB_EVENT_PATH")
    if not event_file:
        return None

    event = json.loads(Path(event_file).read_text(encoding="utf-8"))
    default = event["repository"]["default_branch"]
    event_name = os.environ["GITHUB_EVENT_NAME"]
    if event_name == "pull_request" and event["pull_request"]["base"]["ref"] == default:
        base = event["pull_request"]["base"]["sha"]
    elif event_name == "push" and event["ref"] == f"refs/heads/{default}":
        base = event["after"]
    else:
        base = subprocess.check_output(
            ["git", "rev-parse", f"refs/remotes/origin/{default}"], cwd=ROOT, text=True,
        ).strip()
    if not re.fullmatch("[a-f0-9]{40}", base):
        raise ValueError("Missing integration base SHA for roadmap evidence")
    return base


class RoadmapTests(unittest.TestCase):
    def test_implementation_milestone_update(self):
        enforce_roadmap_gate()

    def test_generated_views_and_evidence(self):
        args = ["node", "roadmap/generate.mjs", "--check", "--verify-migration"]
        base = integration_base()
        if base:
            args.extend(["--base", base])
        subprocess.run(args, cwd=ROOT, check=True)

    def test_roadmap_regressions(self):
        subprocess.run(["node", "--test", "roadmap/roadmap.test.mjs"], cwd=ROOT, check=True)
