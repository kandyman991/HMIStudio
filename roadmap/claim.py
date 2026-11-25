"""Publish a shared reservation ref without modifying main or the task checkout."""
import argparse
import datetime
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "refs/heads/roadmap-claims/"


def git(*args, data=None):
    return subprocess.check_output(["git", *args], cwd=ROOT, input=data, text=True).strip()


def overlaps(left, right):
    return any(a == b or (a.endswith("/") and b.startswith(a)) or
               (b.endswith("/") and a.startswith(b)) for a in left for b in right)


def refs():
    raw = git("ls-remote", "--heads", "origin", PREFIX + "*")
    return dict((ref.removeprefix(PREFIX), sha) for sha, ref in
                (line.split() for line in raw.splitlines()))


def read_claim(sha):
    git("fetch", "--no-tags", "origin", sha)
    return json.loads(git("show", sha + ":roadmap/claim.json"))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("action", choices=["claim", "list", "release"])
    p.add_argument("milestone", nargs="?")
    p.add_argument("--owner")
    p.add_argument("--branch")
    p.add_argument("--scope", nargs="+")
    args = p.parse_args()
    active = refs()
    if args.action == "list":
        for group, sha in active.items():
            print(group, json.dumps(read_claim(sha)))
        return
    if not args.milestone or not re.fullmatch(r"[A-Z]+-\d{2}[A-Z]?(?:\.[0-9]+[A-Z]?)?", args.milestone):
        p.error("valid milestone required")
    git("fetch", "origin", "main")
    task = json.loads(git("show", "refs/remotes/origin/main:roadmap/milestones/" + args.milestone + ".json"))
    group = task.get("claimGroup", task["id"])
    if args.action == "release":
        if group not in active:
            p.error("no reservation")
        claim = read_claim(active[group])
        if claim["milestone"] != args.milestone or claim["branch"] != args.branch or claim["owner"] != args.owner:
            p.error("reservation owner/branch mismatch")
        # No automatic release or force overwrite. Explicit owner release only.
        git("push", "--force-with-lease=" + PREFIX + group + ":" + active[group], "origin", ":" + PREFIX + group)
        print("Released", group)
        return
    if not args.owner or not args.branch or not args.scope:
        p.error("owner, branch and explicit scope required")
    if task["phase"] not in ("planned", "in_progress", "proposed"):
        p.error("integrated milestone requires a new follow-up milestone")
    git("check-ref-format", "--branch", args.branch)
    if args.branch.startswith("roadmap-claims/") or args.branch == "main":
        p.error("invalid implementation branch")
    scope = sorted(set(args.scope))
    if any(not re.fullmatch(r"[A-Za-z0-9_.-][A-Za-z0-9_./-]*", s) or
           any(x in (".", "..") for x in s.split("/")) or "//" in s for s in scope):
        p.error("invalid repository scope")
    if group in active:
        p.error("milestone group already reserved: " + group)
    for other, sha in active.items():
        if overlaps(scope, read_claim(sha)["scope"]):
            p.error("scope overlaps reservation " + other)
    record = {"milestone": args.milestone, "owner": args.owner, "branch": args.branch,
              "scope": scope, "observedAt": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
    blob = git("hash-object", "-w", "--stdin", data=json.dumps(record, indent=2) + "\n")
    tree = git("mktree", data=f"100644 blob {blob}\tclaim.json\n")
    top = git("mktree", data=f"040000 tree {tree}\troadmap\n")
    commit = git("commit-tree", top, "-m", "Reserve " + group)
    # Empty expected ref is a compare-and-set. A simultaneous claim cannot overwrite it.
    git("push", "--force-with-lease=" + PREFIX + group + ":", "origin", commit + ":" + PREFIX + group)
    # Different groups can race. Gate rechecks all shared scopes, and the claimant
    # must stop if the post-push snapshot detects another overlapping reservation.
    for other, sha in refs().items():
        if other != group and overlaps(scope, read_claim(sha)["scope"]):
            p.error("concurrent overlapping reservation " + other + "; stop and coordinate release")
    print(json.dumps(record, indent=2))


if __name__ == "__main__":
    main()
