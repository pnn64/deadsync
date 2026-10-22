import argparse
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import fuzz_song_lua as fuzz


class GenerationTests(unittest.TestCase):
    def test_legacy_factors_do_not_count_unknown_pairs(self):
        old = {key: values[0] for key, values in fuzz.FACTORS.items() if key != "transform"}
        old["effect"] = "vibrate"
        self.assertLess(fuzz.coverage([old])["covered"], fuzz.coverage([old])["total"])

    def test_foreground_survives_capture_and_random_vibration_is_explicitly_excluded(self):
        case = fuzz.generate_case(1, 0, next(fuzz.factor_rows(1, 1, "random")))
        self.assertIn("root.OnCommand=function(self)\n  self:sleep(12)", fuzz.lua_source(case))
        self.assertNotIn("vibrate", fuzz.FACTORS["effect"])
        self.assertIn("vibrate", fuzz.api_coverage(set())["exclusions"])

    def test_covering_prefix_and_shards_are_reproducible(self):
        short = list(fuzz.factor_rows(731, 500, "pairwise"))
        long = list(fuzz.factor_rows(731, 510, "pairwise"))
        self.assertEqual(short, long[:500])
        self.assertEqual(fuzz.coverage(short)["missing"], [])
        self.assertTrue(fuzz.coverage(short[:2])["missing"])
        self.assertNotEqual(short, list(fuzz.factor_rows(732, 500, "pairwise")))

    def test_case_bytes_and_bounded_assets_are_reproducible(self):
        factors = next(fuzz.factor_rows(3, 1, "random"))
        case = fuzz.generate_case(3, 0, factors)
        with tempfile.TemporaryDirectory() as temporary:
            left, right = Path(temporary) / "left", Path(temporary) / "right"
            fuzz.write_case(left, case)
            fuzz.write_case(right, fuzz.generate_case(3, 0, factors))
            self.assertEqual({p.name: p.read_bytes() for p in left.iterdir()},
                             {p.name: p.read_bytes() for p in right.iterdir()})
            self.assertIn("#FGCHANGES:0=default.lua", (left / "song.sm").read_text())
            self.assertLess((left / "silence.wav").stat().st_size, 200000)
            with self.assertRaises(FileExistsError):
                fuzz.write_case(left, case)

    def test_every_factor_changes_authored_input(self):
        # A reported factor must actually vary the Lua or song timing.
        factors = {key: values[0] for key, values in fuzz.FACTORS.items()}
        factors["effect"] = "spin"  # clock is meaningful with an active effect
        baseline = fuzz.generate_case(8, 2, factors)
        for factor, values in fuzz.FACTORS.items():
            sources = set()
            for value in values:
                row = dict(factors, **{factor: value})
                case = fuzz.generate_case(8, 2, row)
                sources.add(fuzz.lua_source(case) + fuzz.simfile_source(case))
            self.assertEqual(len(sources), len(values), factor)
        self.assertEqual(baseline, fuzz.generate_case(8, 2, factors))


class VerdictTests(unittest.TestCase):
    def evaluate(self, *, oracle="semantic", manifest_status="ok", capture_code=0,
                 replay_code=0, replay_pass=True, missing_report=False,
                 capture_timeout=False, replay_timeout=False, runtime_errors=None,
                 corrupt_capture=False, corrupt_report=False, retained_vm=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case = fuzz.generate_case(1, 0, next(fuzz.factor_rows(1, 1, "random")))
            fuzz.write_case(root / "case", case)
            args = argparse.Namespace(oracle=oracle, harness="harness", runner="runner",
                                      itgmania_bin="engine", fallback_theme="Simply Love", beat_step=0.25,
                                      max_events=1000, timeout=60)
            calls = []
            def run(command, cwd, log, timeout, environment=None):
                calls.append(command)
                log.write_text("test subprocess\n")
                if len(calls) == 1:
                    out = root / "result"
                    data = {"runtime_errors": runtime_errors or [], "dropped_events": 0}
                    if oracle == "semantic":
                        (out / "oracle").mkdir()
                        fuzz.write_json(out / "oracle/song.sm.semantic.json", data)
                        fuzz.write_json(out / "oracle/_semantic_manifest.json", {
                            "simfiles": [{"status": manifest_status, "lua_entries": 1}]})
                    else:
                        fuzz.write_json(out / "native.json", data)
                    if corrupt_capture:
                        target = out / ("oracle/song.sm.semantic.json" if oracle == "semantic" else "native.json")
                        target.write_text("truncated {")
                    return {"timeout": True} if capture_timeout else {"returncode": capture_code}
                if not missing_report:
                    fuzz.write_json(root / "result/deadsync.json", {
                        "compiled_checks_passed": replay_pass,
                        "execution_path": "compiled-song-lua",
                        "retained_lua_vm": retained_vm,
                        "pending_coverage": ["pixels"]})
                    if corrupt_report:
                        (root / "result/deadsync.json").write_text("truncated {")
                return {"timeout": True} if replay_timeout else {"returncode": replay_code}
            with patch.object(fuzz, "run_process", side_effect=run):
                result = fuzz.evaluate_case(root / "case", case, root / "result", args)
            self.assertEqual(result, fuzz.read_json(root / "result/result.json"))
            return result, calls

    def test_pass_requires_successful_process_and_explicit_report(self):
        for oracle in ("semantic", "native"):
            self.assertEqual(self.evaluate(oracle=oracle)[0]["status"], "pass")
            self.assertEqual(self.evaluate(oracle=oracle, replay_pass=False)[0]["status"], "mismatch")
            self.assertEqual(self.evaluate(oracle=oracle, replay_code=2)[0]["status"], "mismatch")
            self.assertEqual(self.evaluate(oracle=oracle, missing_report=True)[0]["status"], "deadsync_error")
            self.assertEqual(self.evaluate(oracle=oracle, retained_vm=True)[0]["status"], "mismatch")

    def test_partial_or_crashed_oracle_never_replays(self):
        for arguments, expected in [({"manifest_status": "partial"}, "oracle_incomplete"),
                                    ({"runtime_errors": ["bad Lua"]}, "oracle_incomplete"),
                                    ({"capture_code": 1}, "oracle_error"),
                                    ({"capture_timeout": True}, "oracle_timeout")]:
            result, calls = self.evaluate(**arguments)
            self.assertEqual(result["status"], expected)
            self.assertEqual(len(calls), 1)

    def test_replay_timeout_is_distinct_from_mismatch(self):
        self.assertEqual(self.evaluate(replay_timeout=True)[0]["status"], "deadsync_timeout")

    def test_malformed_reports_are_saved_as_engine_errors(self):
        result, calls = self.evaluate(corrupt_capture=True)
        self.assertEqual(result["status"], "oracle_error")
        self.assertEqual(len(calls), 1)
        self.assertEqual(self.evaluate(corrupt_report=True)[0]["status"], "deadsync_error")

    def test_exact_test_selection_prevents_unrelated_passes(self):
        _, calls = self.evaluate()
        self.assertIn(fuzz.TEST_NAME, calls[1])
        self.assertIn("--exact", calls[1])
        _, calls = self.evaluate(oracle="native")
        self.assertNotIn("--production", calls[1])
        self.assertIn(fuzz.TEST_NAME, calls[1])
        self.assertIn("--exact", calls[1])
        self.assertIn("--headless", calls[0])


class ProcessTests(unittest.TestCase):
    def test_hung_process_is_terminated_and_output_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "output.log"
            result = fuzz.run_process([sys.executable, "-c",
                                       "import time; print('started', flush=True); time.sleep(30)"],
                                      temporary, log, 0.5)
            self.assertTrue(result["timeout"])
            self.assertIn("started", log.read_text())


if __name__ == "__main__":
    unittest.main()
