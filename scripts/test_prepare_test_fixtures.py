import hashlib
import json
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import prepare_test_fixtures as fixtures


class ArchivePartsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.store = Path(self.temp.name)
        self.data = b"original compressed archive bytes"
        self.parts = []
        for number, data in enumerate((self.data[:7], self.data[7:])):
            path = self.store / f"{number}.part"
            path.write_bytes(data)
            self.parts.append({"file": path.name, "bytes": len(data),
                               "sha256": hashlib.sha256(data).hexdigest()})
        self.entry = {"archive": "archive.tar.zst", "bytes": len(self.data),
                      "sha256": hashlib.sha256(self.data).hexdigest(), "parts": self.parts}
        self.manifest()

    def manifest(self):
        (self.store / fixtures.MANIFEST).write_text(
            json.dumps({"version": 1, "archives": [self.entry]}), encoding="utf-8")

    def test_restores_exact_bytes_and_verifies_existing_archives(self):
        self.assertEqual(fixtures.prepare(self.store), 1)
        self.assertEqual((self.store / self.entry["archive"]).read_bytes(), self.data)
        self.assertEqual(fixtures.prepare(self.store), 0)
        (self.store / self.entry["archive"]).write_bytes(b"x" * len(self.data))
        with self.assertRaisesRegex(ValueError, "existing fixture checksum"):
            fixtures.prepare(self.store)

    def test_corrupt_or_missing_parts_leave_no_archive_or_temporary_file(self):
        (self.store / "1.part").write_bytes(b"x" * self.parts[1]["bytes"])
        with self.assertRaisesRegex(ValueError, "part checksum"):
            fixtures.prepare(self.store)
        self.assertFalse((self.store / self.entry["archive"]).exists())
        self.assertEqual(list(self.store.glob(".fixture-*")), [])
        (self.store / "1.part").unlink()
        with self.assertRaises(FileNotFoundError):
            fixtures.prepare(self.store)
        self.assertEqual(list(self.store.glob(".fixture-*")), [])

    def test_rejects_wrong_whole_archive_hash(self):
        self.entry["sha256"] = "0" * 64
        self.manifest()
        with self.assertRaisesRegex(ValueError, "restored fixture checksum"):
            fixtures.prepare(self.store)
        self.assertFalse((self.store / self.entry["archive"]).exists())

    def test_rejects_path_traversal_and_oversized_parts(self):
        self.entry["archive"] = "../escaped.tar.zst"
        self.manifest()
        with self.assertRaises(ValueError):
            fixtures.prepare(self.store)
        self.entry["archive"] = "archive.tar.zst"
        self.parts[0]["bytes"] = fixtures.MAX_FILE_BYTES
        self.manifest()
        with self.assertRaisesRegex(ValueError, "part size"):
            fixtures.prepare(self.store)
        self.assertFalse((self.store / self.entry["archive"]).exists())

    def test_split_includes_new_archives_and_preserves_ignored_local_captures(self):
        subprocess.run(["git", "init", "--quiet", str(self.store)], check=True)
        store = self.store / "full_song_lua"
        store.mkdir()
        (store / "new.tar.zst").write_bytes(self.data)
        (store / "local.tar.zst").write_bytes(self.data)
        (self.store / ".gitignore").write_text("/full_song_lua/local.tar.zst\n", encoding="utf-8")
        with patch.object(fixtures, "MAX_FILE_BYTES", 12), patch.object(fixtures, "PART_BYTES", 8):
            entries = fixtures.split(store)
        self.assertEqual([entry["archive"] for entry in entries], ["new.tar.zst"])
        self.assertTrue(all(part["bytes"] <= 8 for part in entries[0]["parts"]))
        self.assertEqual((store / "new.tar.zst").read_bytes(), self.data)
        self.assertEqual((store / "local.tar.zst").read_bytes(), self.data)
        self.assertIn("/full_song_lua/new.tar.zst", (self.store / ".gitignore").read_text())


if __name__ == "__main__":
    unittest.main()
