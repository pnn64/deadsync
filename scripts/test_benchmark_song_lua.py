import json
from pathlib import Path
import tempfile
import unittest

from benchmark_song_lua import normalized_report_hash


class ReportIdentityTests(unittest.TestCase):
    def digest(self, actors, *, name="Quad", path="Screen/Quad", extra=None):
        value = {"command_queries": {"actual": [
            {"scene": 1, "results": [{"actor": actor, "name": name, "path": path}]}
            for actor in actors
        ]}, "native_trace": "/tmp/.tmpABC123/trace.json", "extra": extra}
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "report.json"
            file.write_text(json.dumps(value, indent=2))
            return normalized_report_hash(file)

    def test_addresses_change_without_changing_aliases(self):
        self.assertEqual(self.digest(["0x1234", "0x1234", "0x5678"]),
                         self.digest(["0x9999", "0x9999", "0x8888"]))

    def test_different_aliases_are_not_hidden(self):
        self.assertNotEqual(self.digest(["0x1234", "0x1234"]),
                            self.digest(["0x1234", "0x5678"]))

    def test_names_ancestry_and_other_values_remain_exact(self):
        expected = self.digest(["0x1234"])
        self.assertNotEqual(expected, self.digest(["0x9999"], name="Other"))
        self.assertNotEqual(expected, self.digest(["0x9999"], path="Screen/Other"))
        self.assertNotEqual(expected, self.digest(["0x9999"], extra=1))

    def test_untyped_hexadecimal_strings_remain_exact(self):
        self.assertNotEqual(self.digest([], extra={"actor": "0x1234"}),
                            self.digest([], extra={"actor": "0x5678"}))


if __name__ == "__main__":
    unittest.main()
