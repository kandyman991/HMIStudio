"""Require a meaningful, scoped roadmap update for implementation PRs.

The base must already contain DEV-01. Old branches must integrate main before
the new gate applies; a roadmap-only maintenance PR has no implementation scope.
"""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PREFIX = "roadmap/milestones/"


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True)


def read_at(ref, path):
    try:
        return json.loads(git("show", f"{ref}:{path}"))
    except subprocess.CalledProcessError:
        return None


def implementation(path):
    # Documentation and curated continuity records are not implementation.
    if path.startswith(PREFIX) or path in ("roadmap/dashboard.html", "roadmap/context.json"):
        return False
    if path.startswith("docs/") and path.endswith(".md"):
        return False
    if "/" not in path and path.endswith(".md"):
        return False
    return path != "handoff/state.json"


def covers(path, scope):
    return any(path == p or (p.endswith("/") and path.startswith(p)) for p in scope)


def check(base, head, branch, number=None):
    try:
        git("cat-file", "-e", f"{base}:roadmap/generate.mjs")
    except subprocess.CalledProcessError:
        return  # bootstrap migration only
    ancestor = git("merge-base", base, head).strip()
    paths = git("diff", "--name-only", "--no-renames", "-z", ancestor, head).split("\0")
    changed = [p for p in paths if p and implementation(p)]
    if not changed:
        return
    # Reservations are visible before PR and immutable for a specific claim;
    # base evidence is not a substitute for an actual active branch reservation.
    candidates = []
    for path in paths:
        if not path.startswith(PREFIX) or not path.endswith(".json"):
            continue
        prior = read_at(ancestor, path)
        current = read_at(head, path)
        if not current or current.get("phase") not in ("in_progress", "proposed"):
            continue
        if current["phase"] == "proposed" and number is not None and current.get("completionPr") != number:
            continue
        a = current.get("assignment", {})
        if a.get("branch") != branch or not a.get("scope"):
            continue
        old_history = (prior or {}).get("history", [])
        history = current.get("history", [])
        if history[:len(old_history)] != old_history or len(history) <= len(old_history):
            continue
        if not any(e["type"] in ("started", "handoff", "completion_proposed")
                   for e in history[len(old_history):]):
            continue
        if not all(covers(p, a["scope"]) for p in changed):
            continue
        group = current.get("claimGroup", current["id"])
        try:
            claim = read_at(f"refs/remotes/origin/roadmap-claims/{group}", "roadmap/claim.json")
        except subprocess.CalledProcessError:
            claim = None
        if not claim or claim.get("milestone") != current["id"] or claim.get("branch") != branch or claim.get("owner") != a.get("owner") or claim.get("scope") != a["scope"]:
            continue
        refs = git("for-each-ref", "--format=%(refname)", "refs/remotes/origin/roadmap-claims/").splitlines()
        for ref in refs:
            if ref == f"refs/remotes/origin/roadmap-claims/{group}":
                continue
            other = read_at(ref, "roadmap/claim.json")
            if not other or not other.get("scope"):
                raise ValueError("Malformed shared reservation " + ref)
            if any(covers(p, other["scope"]) for p in changed) or any(
                    covers(p, [other_path]) or covers(other_path, [p])
                    for p in a["scope"] for other_path in other["scope"]):
                raise ValueError("Overlapping active reservation " + ref)
        candidates.append(current["id"])
    if len(candidates) != 1:
        raise ValueError("Implementation PR must update exactly one scoped, reserved milestone with new dated history; matching milestones: " + ", ".join(candidates))


def main():
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path or os.environ.get("GITHUB_EVENT_NAME") != "pull_request":
        return
    event = json.loads(Path(event_path).read_text())
    pr = event["pull_request"]
    check(pr["base"]["sha"], pr["head"]["sha"], pr["head"]["ref"], pr["number"])


if __name__ == "__main__":
    main()
