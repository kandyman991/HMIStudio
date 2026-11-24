"""Verify exact dependency identities and reviewed license/notice inventory.

This is a source-scope inventory gate, not binary distribution clearance.
The one public Git revision is an explicit, exact remediation exception.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "crates/hmi-engine"
INVENTORY = ROOT / "docs/reuse/REUSE_02B_DEPENDENCY_INVENTORY.json"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
GIT_REVISION = "ca18bb73bac4ff2627943c760c97123062988468"
GIT_TREE = "3b8b55a602b7c2818965625bacee645dc9c0fdc9"
GIT_SOURCE = f"git+https://github.com/FreeOpcUa/async-opcua.git?rev={GIT_REVISION}#{GIT_REVISION}"
GIT_LICENSE_SHA = "1f256ecad192880510e84ad60474eab7589218784b9a50bc7ceee34c2b91f1d5"
GIT_PACKAGES = {
    "async-opcua", "async-opcua-client", "async-opcua-core",
    "async-opcua-core-namespace", "async-opcua-crypto", "async-opcua-macros",
    "async-opcua-nodes", "async-opcua-server", "async-opcua-types",
}

def identity(package):
    return package["name"], package["version"], package["source"]

def check_inventory(lock, inventory):
    if (inventory.get("schema"), inventory.get("git_revision"), inventory.get("git_tree"),
        inventory.get("git_source"), inventory.get("git_license_sha256")) != (
        1, GIT_REVISION, GIT_TREE, GIT_SOURCE, GIT_LICENSE_SHA):
        raise ValueError("unreviewed Git source or inventory schema")
    expected = {}
    local_count = 0
    for package in lock["package"]:
        source = package.get("source")
        if source is None:
            if package["name"] != "hmi-engine":
                raise ValueError("unexpected local package")
            local_count += 1
            continue
        if source == REGISTRY:
            if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
                raise ValueError("missing registry checksum")
        elif source != GIT_SOURCE or package["name"] not in GIT_PACKAGES or package["version"] != "0.19.0":
            raise ValueError("unreviewed source: " + str(source))
        key = identity(package)
        if key in expected:
            raise ValueError("duplicate lock identity")
        expected[key] = package.get("checksum")
    if local_count != 1:
        raise ValueError("expected one local engine")
    rows = {}
    for row in inventory["packages"]:
        key = identity(row)
        if key in rows or not all(row.get(k) for k in ("declared_license", "selected_license", "review")):
            raise ValueError("duplicate or incomplete license inventory")
        rows[key] = row
        if key[2] == GIT_SOURCE and (row["declared_license"] != "MPL-2.0" or row["selected_license"] != "MPL-2.0"):
            raise ValueError("Git license mismatch")
    if expected.keys() != rows.keys():
        raise ValueError(f"missing or stale inventory: {sorted(expected.keys() ^ rows.keys())}")
    for key, checksum in expected.items():
        if rows[key].get("checksum") != checksum:
            raise ValueError("reviewed checksum mismatch: " + str(key))
    if {key[0] for key in expected if key[2] == GIT_SOURCE} != GIT_PACKAGES:
        raise ValueError("unexpected Git package set")
    return rows

def check_git_checkout():
    # Cargo metadata resolves the exact locked graph and reports source paths.
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--features", "test-server", "--format-version", "1"],
        cwd=ENGINE, text=True))
    packages = [p for p in metadata["packages"] if (p.get("source") or "").startswith("git+")]
    if {p["name"] for p in packages} != GIT_PACKAGES or len(packages) != len(GIT_PACKAGES):
        raise ValueError("metadata Git package set mismatch")
    roots = set()
    for p in packages:
        if p["source"] != GIT_SOURCE or p["license"] != "MPL-2.0" or p["version"] != "0.19.0":
            raise ValueError("metadata source/license mismatch")
        directory = Path(p["manifest_path"]).parent
        root = Path(subprocess.check_output(["git", "-C", str(directory), "rev-parse", "--show-toplevel"], text=True).strip())
        roots.add(root)
    if len(roots) != 1:
        raise ValueError("unexpected Git checkout layout")
    root = roots.pop()
    def git(*args):
        return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()
    if git("rev-parse", "HEAD") != GIT_REVISION or git("rev-parse", "HEAD^{tree}") != GIT_TREE:
        raise ValueError("Git checkout identity mismatch")
    if git("status", "--porcelain", "--untracked-files=no"):
        raise ValueError("Git checkout tracked modifications")
    if hashlib.sha256((root / "LICENSE.md").read_bytes()).hexdigest() != GIT_LICENSE_SHA:
        raise ValueError("upstream license checksum mismatch")

def main():
    rows = check_inventory(tomllib.loads((ENGINE / "Cargo.lock").read_text(encoding="utf-8")),
                           json.loads(INVENTORY.read_text(encoding="utf-8")))
    check_git_checkout()
    print(f"Exact license inventory verified: {len(rows)} dependencies; binary notices remain a release gate.")

if __name__ == "__main__":
    main()
