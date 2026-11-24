"""Conservative change routing. No network access or untrusted shell interpolation."""
import json
import os
from pathlib import Path
import subprocess

FLAGS = ("frontend", "native", "package", "crates")

def classify(paths, force=False):
    result = dict.fromkeys(FLAGS, False)
    def enable(*keys):
        for key in keys:
            result[key] = True
    if force:
        enable(*FLAGS)
    for path in paths:
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

def main():
    event = os.environ["CI_EVENT"]
    head = os.environ["CI_HEAD"]
    force = event == "workflow_dispatch" or os.environ.get("CI_REF", "").startswith("refs/tags/")
    paths = None if force else changed_paths(os.environ.get("CI_BASE", ""), head, event)
    flags = classify(paths or [], force=force or paths is None)
    crates = sorted(str(p.parent) for p in Path("crates").glob("*/Cargo.toml") if p.is_file())
    if flags["crates"] and not crates:
        raise RuntimeError("Crate validation requested but no manifests found")
    if not flags["crates"]:
        crates = []
    if Path("handoff/state.json").exists():
        json.loads(Path("handoff/state.json").read_text(encoding="utf-8"))
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
        for key, value in flags.items():
            print(f"{key}={str(value).lower()}", file=out)
        print("crate_dirs=" + json.dumps(crates, separators=(",", ":")), file=out)
    print(json.dumps({"full_fallback": paths is None, "changed_count": None if paths is None else len(paths), "checks": flags, "crate_dirs": crates}))

if __name__ == "__main__":
    main()
