"""Reconcile recorded GitHub observations; never confer acceptance."""
import datetime
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REPO = "kandyman991/HMIStudio"
REQUIRED = {"Desktop CI", "Engineering handoff snapshot"}


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def api(path, paginate=False):
    cmd = ["gh", "api"]
    if paginate:
        cmd += ["--paginate"]
    raw = subprocess.check_output(cmd + [path], cwd=ROOT, text=True)
    if not paginate:
        return json.loads(raw)
    pages = []
    decoder = json.JSONDecoder()
    while raw.strip():
        page, end = decoder.raw_decode(raw.lstrip())
        pages.append(page)
        raw = raw.lstrip()[end:]
    return pages


def observe(previous, value, now):
    return previous if {k: v for k, v in previous.items() if k != "observedAt"} == value else {**value, "observedAt": now}


def pr_snapshot(previous, info, raw_runs, now):
    head = info["head"]["sha"]
    if info["number"] != previous["number"]:
        raise ValueError("PR number mismatch")
    if not info["head"]["repo"] or info["head"]["repo"]["full_name"].lower() != REPO.lower():
        raise ValueError("Unexpected PR source repository")
    if info["base"]["ref"] != "main" or info["base"]["repo"]["full_name"].lower() != REPO.lower():
        raise ValueError("Unexpected PR target")
    status = "merged" if info["merged"] else ("closed" if info["state"] == "closed" else "draft" if info["draft"] else "open")
    latest = {}
    for run in raw_runs:
        if run["head_sha"] != head or run["event"] != "pull_request":
            continue
        name = run["name"]
        if name not in latest or run["id"] > latest[name]["id"]:
            latest[name] = run
    runs = []
    for name, run in sorted(latest.items()):
        state = run["status"]
        if state not in ("completed", "in_progress"):
            state = "queued"
        result = run["conclusion"] if state == "completed" else "pending"
        if not result:
            raise ValueError("Completed workflow without conclusion")
        runs.append({"id": run["id"], "name": name, "head": head, "state": state, "result": result})
    value = {"number": info["number"], "head": head, "branch": info["head"]["ref"], "state": status, "runs": runs}
    if status == "merged":
        value["merge"] = info["merge_commit_sha"]
        git("merge-base", "--is-ancestor", value["merge"], "refs/remotes/origin/main")
    return observe(previous, value, now)


def advance(task, now):
    if task["phase"] != "proposed" or not task.get("completionPr"):
        return
    pr = next((p for p in task.get("prs", []) if p["number"] == task["completionPr"]), None)
    if not pr:
        return
    if pr["state"] == "closed":
        task["phase"] = "in_progress" if task.get("assignment") or any(p["state"] in ("open", "draft") for p in task.get("prs", [])) else "planned"
        task.pop("proposal", None)
        task.pop("completionPr", None)
        task.setdefault("history", []).append({"date": now[:10], "type": "note", "summary": "Completion PR closed without merge; no completion recorded."})
        return
    required = REQUIRED | set(task.get("requiredWorkflows", []))
    successful = {r["name"] for r in pr["runs"] if r["state"] == "completed" and r["result"] == "success"}
    if pr["state"] != "merged" or not required <= successful or any(
            r["state"] != "completed" or r["result"] != "success" for r in pr["runs"]):
        return
    if any(p["state"] in ("draft", "open") for p in task.get("prs", [])):
        return
    task["phase"] = "merged"
    task.pop("proposal", None)
    task.pop("assignment", None)
    task.setdefault("history", []).append({"date": now[:10], "type": "merged", "summary": f"GitHub verified designated completion PR #{pr['number']} merged at {pr['merge'][:12]} with successful recorded exact-head workflows. Acceptance remains separate."})


def reservations():
    prefix = "refs/heads/roadmap-claims/"
    raw = git("ls-remote", "--heads", "origin", prefix + "*")
    result = []
    for line in raw.splitlines():
        sha, ref = line.split()
        git("fetch", "--no-tags", "origin", ref)
        if git("rev-parse", "FETCH_HEAD") != sha:
            raise ValueError("Reservation changed during observation; retry")
        claim = json.loads(git("show", sha + ":roadmap/claim.json"))
        result.append({"group": ref.removeprefix(prefix), **claim})
    return sorted(result, key=lambda claim: claim["group"])


def main():
    git("fetch", "--no-tags", "origin", "main")
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    cache = {}
    for path in sorted((ROOT / "roadmap/milestones").glob("*.json")):
        task = json.loads(path.read_text())
        original = json.loads(path.read_text())
        if task["phase"] not in ("merged", "accepted"):
            for i, previous in enumerate(task.get("prs", [])):
                if previous["state"] == "closed" or (previous["state"] == "merged" and previous["number"] != task.get("completionPr")):
                    continue
                n = previous["number"]
                if n not in cache:
                    info = api(f"repos/{REPO}/pulls/{n}")
                    pages = api(f"repos/{REPO}/actions/runs?head_sha={info['head']['sha']}&event=pull_request&per_page=100", True)
                    cache[n] = (info, [r for page in pages for r in page["workflow_runs"]])
                task["prs"][i] = pr_snapshot(previous, *cache[n], now)
            advance(task, now)
        for i, previous in enumerate(task.get("issues", [])):
            info = api(f"repos/{REPO}/issues/{previous['number']}")
            if "pull_request" in info:
                raise ValueError("Issue evidence points to a PR")
            task["issues"][i] = observe(previous, {"number": previous["number"], "label": previous["label"], "state": info["state"]}, now)
        if task != original:
            path.write_text(json.dumps(task, indent=2, ensure_ascii=False) + "\n")
            print("Updated", task["id"])
    context_path = ROOT / "roadmap/context.json"
    context = json.loads(context_path.read_text())
    current = reservations()
    if current != context.get("reservations", []):
        context["reservations"] = current
        context_path.write_text(json.dumps(context, indent=2, ensure_ascii=False) + "\n")
        print("Updated shared reservation snapshots")


if __name__ == "__main__":
    main()
