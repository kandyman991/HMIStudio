"""Conservative change routing and read-only CI-03 checkpoint verification."""
import json
import os
from pathlib import Path
import subprocess
import re
from urllib.parse import urlencode
from urllib.request import Request, urlopen

FLAGS = ("frontend", "native", "package", "crates")

def classify(paths, force=False):
    result = dict.fromkeys(FLAGS, False)
    def enable(*keys):
        for key in keys:
            result[key] = True
    if force:
        enable(*FLAGS)
    for path in paths:
        if path.startswith("roadmap/") or path in {
            ".github/ci/plan.py", ".github/ci/test_plan.py", ".github/ci/test_roadmap.py",
            ".github/ci/roadmap_gate.py", ".github/ci/test_roadmap_gate.py",
            ".github/ci/test_roadmap_sync.py", ".github/workflows/roadmap-sync.yml",
        }:
            # Desktop CI's changes job validates these development-only inputs.
            continue
        if ("/" not in path and path.endswith(".md")) or (
            path.startswith("docs/") and path.endswith(".md")
        ) or path == "handoff/state.json":
            continue
        if path.startswith("crates/"):
            # Conservative: independent crate tests and all downstream native consumers.
            enable("crates", "native", "frontend")
        elif path.startswith("src-tauri/src/") or path.startswith("src-tauri/capabilities/"):
            enable("native", "frontend")
        elif path.startswith("src-tauri/") or path == "assets/icon.svg":
            # Configuration, manifests, build scripts, resources, sidecars and icons.
            enable("native", "frontend", "package")
        elif path.startswith(("src/", "tests/", "assets/")) or path in {
            "index.html", "tsconfig.json", "package.json", "package-lock.json"
        }:
            enable("frontend")
            if path in {"package.json", "package-lock.json"}:
                enable("native", "package")
        else:
            # New build inputs, CI rules, unknown paths: never silently under-test.
            enable(*FLAGS)
    return result

def git(*args):
    return subprocess.check_output(["git", *args])

def changed_paths(base, head, event):
    if not base or set(base) == {"0"}:
        return None
    try:
        if event == "pull_request":
            base = git("merge-base", base, head).decode().strip()
        raw = git("diff", "--no-renames", "--name-only", "-z", base, head, "--")
        return [os.fsdecode(p) for p in raw.split(b"\0") if p]
    except subprocess.CalledProcessError:
        return None


# Narrow pilot only: new policy/lock/schema/consumer/unknown paths force CI-02.
CI03_REPO = "kandyman991/HMIStudio"
CI03_BRANCH = "feat/reuse-03b-legacy-import"
CI03_PIN = "docs/CI03_CHECKPOINT.md"
CI03_PATHS = {
    CI03_PIN, "CHAT_HANDOFF.md", "docs/LEGACY_IMPORT.md",
    "crates/legacy-import/src/graphics.rs",
    "crates/legacy-import/src/lib.rs",
    "crates/legacy-import/src/resources.rs",
    "crates/legacy-import/src/primitives.rs",
    "crates/legacy-import/tests/portable.rs",
}
CI03_TRAILER = "CI-03: importer-only"
FULL_CRATES = ("engineering-core", "project-model", "tag-engine", "hmi-engine", "legacy-import")
FULL_OS = ("ubuntu-24.04", "windows-latest")
FULL_DESKTOP = {"changes", "frontend", "Required CI"} | {
    f"crates ({os_name}, crates/{crate})" for os_name in FULL_OS for crate in FULL_CRATES
} | {"desktop (ubuntu-24.04, deb)", "desktop (windows-latest, msi)"}
FULL_TAG = {"Tag engine routing", "Tag engine gate", "audit"} | {
    f"check ({os_name})" for os_name in FULL_OS
}
CI03_REPO_ID = 1469988677


def ci03_event(env):
    return (
        env.get("CI_EVENT") == "pull_request"
        and env.get("CI_ACTION") == "synchronize"
        and env.get("CI_PR") == "36"
        and env.get("CI_DRAFT") == "true"
        and env.get("GITHUB_REPOSITORY") == CI03_REPO
        and env.get("CI_HEAD_REPO") == CI03_REPO
        and env.get("CI_HEAD_REF") == CI03_BRANCH
        and env.get("CI_BASE_REF") == "main"
    )


def ci03_pin(text, base):
    parts = text.strip().split()
    if len(parts) != 2 or not all(re.fullmatch(r"[0-9a-f]{40}", p) for p in parts):
        return None
    return parts[0] if parts[1] == base else None


def ci03_paths(paths):
    return bool(paths) and set(paths) <= CI03_PATHS


def ci03_intent(messages):
    return bool(messages) and all(
        m.strip().splitlines()[-1:] == [CI03_TRAILER] for m in messages
    )


def ci03_api(path, token):
    # Fixed public GitHub API origin. Read-only token, bounded replies/time.
    req = Request(
        "https://api.github.com/repos/" + CI03_REPO + path,
        headers={
            "Authorization": "Bearer " + token,
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urlopen(req, timeout=8) as response:
        raw = response.read(4 * 1024 * 1024 + 1)
    if len(raw) > 4 * 1024 * 1024:
        raise ValueError("CI checkpoint API response too large")
    return json.loads(raw)


def checkpoint_pr(run, head, base):
    # Manual runs have no PR base association and cannot establish this pilot pin.
    return run.get("event") == "pull_request" and any(
        p.get("number") == 36
        and p.get("base", {}).get("ref") == "main"
        and p.get("base", {}).get("sha") == base
        and p.get("base", {}).get("repo", {}).get("id") == CI03_REPO_ID
        and p.get("head", {}).get("ref") == CI03_BRANCH
        and p.get("head", {}).get("sha") == head
        and p.get("head", {}).get("repo", {}).get("id") == CI03_REPO_ID
        for p in (run.get("pull_requests") or [])
    )


def full_checkpoint(head, base, token):
    if not token:
        return False
    expected = {
        "Desktop CI": (".github/workflows/ci.yml", FULL_DESKTOP),
        "Tag engine CI": (".github/workflows/tag-engine.yml", FULL_TAG),
        "Engineering handoff snapshot": (".github/workflows/handoff.yml", {"snapshot"}),
    }
    query = urlencode({"head_sha": head, "status": "success", "per_page": 50})
    runs = ci03_api("/actions/runs?" + query, token)["workflow_runs"]
    for name, (workflow_path, required) in expected.items():
        candidates = [
            run for run in runs
            if run.get("name") == name
            and run.get("head_sha") == head
            and run.get("head_branch") == CI03_BRANCH
            and run.get("path") == workflow_path
            and checkpoint_pr(run, head, base)
            and run.get("status") == "completed"
            and run.get("conclusion") == "success"
        ]
        verified = False
        for run in candidates[:3]:
            run_id = run.get("id")
            if not isinstance(run_id, int):
                continue
            jobs = ci03_api(f"/actions/runs/{run_id}/jobs?per_page=100", token)
            # Never infer full validation from a successful aggregate or a skipped job.
            if jobs["total_count"] > len(jobs["jobs"]):
                continue
            successful = {
                j["name"] for j in jobs["jobs"] if j.get("conclusion") == "success"
            }
            if required <= successful:
                verified = True
                break
        if not verified:
            return False
    return True


def ci03_targeted(env):
    if not ci03_event(env):
        return False
    try:
        base, head = env["CI_BASE"], env["CI_HEAD"]
        checkpoint = ci03_pin(Path(CI03_PIN).read_text(encoding="utf-8"), base)
        if checkpoint is None or checkpoint == head:
            return False
        git("merge-base", "--is-ancestor", base, checkpoint)
        git("merge-base", "--is-ancestor", checkpoint, head)
        if git("rev-list", "--merges", f"{checkpoint}..{head}").strip():
            return False
        paths = changed_paths(checkpoint, head, "push")
        if not ci03_paths(paths):
            return False
        messages = git("log", "--format=%B%x00", f"{checkpoint}..{head}").decode()
        if not ci03_intent([m for m in messages.split("\0") if m.strip()]):
            return False
        return full_checkpoint(checkpoint, base, env.get("CI03_TOKEN", ""))
    except (OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.CalledProcessError):
        # Unavailable evidence is never permission to skip checks.
        return False

def main():
    event = os.environ["CI_EVENT"]
    head = os.environ["CI_HEAD"]
    force = event == "workflow_dispatch" or os.environ.get("CI_REF", "").startswith("refs/tags/")
    paths = None if force else changed_paths(os.environ.get("CI_BASE", ""), head, event)
    targeted = paths is not None and ci03_targeted(os.environ)
    flags = classify(paths or [], force=force or paths is None)
    if targeted:
        flags = dict.fromkeys(FLAGS, False)
        flags["crates"] = True
    crates = sorted(str(p.parent) for p in Path("crates").glob("*/Cargo.toml") if p.is_file())
    if flags["crates"] and not crates:
        raise RuntimeError("Crate validation requested but no manifests found")
    if targeted:
        if "crates/legacy-import" not in crates:
            raise RuntimeError("CI-03 importer manifest missing")
        crates = ["crates/legacy-import"]
    elif not flags["crates"]:
        crates = []
    if Path("handoff/state.json").exists():
        json.loads(Path("handoff/state.json").read_text(encoding="utf-8"))
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
        for key, value in flags.items():
            print(f"{key}={str(value).lower()}", file=out)
        print("crate_dirs=" + json.dumps(crates, separators=(",", ":")), file=out)
        print("crate_os=" + json.dumps(["ubuntu-24.04"] if targeted else list(FULL_OS), separators=(",", ":")), file=out)
        print("targeted=" + str(targeted).lower(), file=out)
    print(json.dumps({"ci03_targeted": targeted, "full_fallback": paths is None, "changed_count": None if paths is None else len(paths), "checks": flags, "crate_dirs": crates}))

if __name__ == "__main__":
    main()
