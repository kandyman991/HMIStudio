"""Regressions for the exact engine notice and source gate."""
import copy
import json
import unittest
import tomllib
from check_engine_notices import ENGINE, INVENTORY, GIT_SOURCE, check_inventory

class EngineNoticeTests(unittest.TestCase):
    def setUp(self):
        self.lock = tomllib.loads((ENGINE / "Cargo.lock").read_text(encoding="utf-8"))
        self.inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))

    def test_reviewed_graph(self):
        self.assertEqual(len(check_inventory(self.lock, self.inventory)), 220)

    def test_missing_inventory(self):
        self.inventory["packages"].pop()
        with self.assertRaises(ValueError):
            check_inventory(self.lock, self.inventory)

    def test_changed_checksum(self):
        p = next(p for p in self.lock["package"] if "checksum" in p)
        p["checksum"] = "0" * 64
        with self.assertRaises(ValueError):
            check_inventory(self.lock, self.inventory)

    def test_unreviewed_git_revision(self):
        p = next(p for p in self.lock["package"] if p.get("source") == GIT_SOURCE)
        p["source"] = GIT_SOURCE.replace("ca18bb73", "deadbeef")
        with self.assertRaises(ValueError):
            check_inventory(self.lock, self.inventory)

    def test_unreviewed_git_license(self):
        p = next(p for p in self.inventory["packages"] if p.get("source") == GIT_SOURCE)
        p["selected_license"] = "MIT"
        with self.assertRaises(ValueError):
            check_inventory(self.lock, self.inventory)

    def test_duplicate_inventory(self):
        self.inventory["packages"].append(copy.deepcopy(self.inventory["packages"][0]))
        with self.assertRaises(ValueError):
            check_inventory(self.lock, self.inventory)
