"""Fail closed if the engine's locked registry graph lacks a reviewed license row.

This verifies the review inventory, not legal clearance or binary notice assembly.
Changes to the graph require updated review evidence; no implicit license allowlist.
"""
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[2]

def check():
    lock = tomllib.loads((ROOT / "crates/hmi-engine/Cargo.lock").read_text(encoding="utf-8"))
    expected = set()
    for package in lock["package"]:
        source = package.get("source")
        if source is None:
            if package["name"] != "hmi-engine":
                raise ValueError("unexpected local package")
            continue
        if not source.startswith("registry+") or not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
            raise ValueError("unreviewed source or missing checksum: " + package["name"])
        expected.add((package["name"], package["version"]))
    rows = {}
    for line in (ROOT / "docs/reuse/REUSE_02A_NOTICE_REVIEW.md").read_text(encoding="utf-8").splitlines():
        fields = [value.strip() for value in line.strip("|").split("|")]
        if len(fields) != 4 or not re.match(r"^\d+\.\d+\.\d+", fields[1]):
            continue
        key = tuple(fields[:2])
        if key in rows or not fields[2] or not fields[3]:
            raise ValueError("duplicate or incomplete license record: " + str(key))
        rows[key] = fields[2:]
    if expected != rows.keys():
        raise ValueError(f"unreviewed: {sorted(expected - rows.keys())}; stale: {sorted(rows.keys() - expected)}")
    print(f"License inventory covers {len(expected)} locked registry packages; binary notices remain a release gate.")

if __name__ == "__main__":
    check()
