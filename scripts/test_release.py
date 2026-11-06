#!/usr/bin/env python3
"""Tests for scripts/release.py. Run: python3 -I scripts/test_release.py"""
import json, pathlib, sys, tempfile, unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import release

CHANGELOG = "# Changelog\n\n## Unreleased\n\nNew things.\n\n## 0.0.9 - 2026-01-01\n\nOld things.\n"


class Release(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = pathlib.Path(folder.name)
        (self.root / "Cargo.toml").write_text('[package]\nname = "termite"\nversion = "0.1.0"\n\n[dependencies]\nx = { version = "0.1.0" }\n')
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "other"\nversion = "0.1.0"\n\n[[package]]\nname = "termite"\nversion = "0.1.0"\n')
        (self.root / "CHANGELOG.md").write_text(CHANGELOG)
        (self.root / "site/public").mkdir(parents=True)
        (self.root / "site/public/releases.json").write_text('{"releases": []}')
        self.dist = self.root / "dist"
        self.dist.mkdir()
        (self.dist / "tmite-linux-x86.tar.gz").write_bytes(b"one")
        (self.dist / "SHA256SUMS").write_text("left out")

    def record(self):
        return json.loads((self.root / "site/public/releases.json").read_text())["releases"]

    def test_given_a_changelog_then_notes_are_only_the_unreleased_section(self):
        self.assertEqual(release.notes(self.root), "New things.")

    def test_given_an_empty_section_then_notes_stop(self):
        (self.root / "CHANGELOG.md").write_text("## Unreleased\n\n## 0.0.9 - 2026-01-01\n")
        with self.assertRaises(SystemExit):
            release.notes(self.root)

    def test_given_a_bump_then_only_this_package_moves_to_the_next_minor(self):
        self.assertEqual(release.bump(self.root, "2026-11-06"), "0.2.0")
        self.assertEqual(release.version(self.root), "0.2.0")
        self.assertIn('x = { version = "0.1.0" }', (self.root / "Cargo.toml").read_text())
        lock = (self.root / "Cargo.lock").read_text()
        self.assertIn('name = "other"\nversion = "0.1.0"', lock)
        self.assertIn('name = "termite"\nversion = "0.2.0"', lock)

    def test_given_a_bump_then_the_changelog_is_dated_and_a_new_section_opens(self):
        release.bump(self.root, "2026-11-06")
        text = (self.root / "CHANGELOG.md").read_text()
        self.assertIn("## Unreleased\n\n## 0.1.0 - 2026-11-06\n\nNew things.\n\n## 0.0.9", text)

    def test_given_two_releases_then_the_second_holds_the_hash_of_the_first(self):
        release.add_record(self.root, "abc", "2026-11-06", self.dist)
        first = self.record()[0]
        self.assertIsNone(first["previous"])
        self.assertEqual(list(first["files"]), ["tmite-linux-x86.tar.gz"])
        release.bump(self.root, "2026-11-06")
        release.add_record(self.root, "def", "2026-11-07", self.dist)
        newest = self.record()[0]
        self.assertEqual(newest["version"], "0.2.0")
        self.assertEqual(newest["previous"], release.digest(first))

    def test_given_a_version_already_recorded_then_it_stops(self):
        release.add_record(self.root, "abc", "2026-11-06", self.dist)
        with self.assertRaises(SystemExit):
            release.add_record(self.root, "abc", "2026-11-06", self.dist)


if __name__ == "__main__":
    unittest.main()
