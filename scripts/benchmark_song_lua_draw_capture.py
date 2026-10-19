"""Measure frozen/current song Lua workloads in one release test executable on Windows."""
import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
from pathlib import Path


def sha256(path):
    return hashlib.file_digest(path.open("rb"), "sha256").hexdigest()

def units(sample):
    return sample["units"] if "units" in sample else sample["frames"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=6)
    parser.add_argument("--baseline-commit", help="require this compiled baseline identity")
    parser.add_argument("--test-filter", action="append", help="exact ignored benchmark name; may be repeated")
    args = parser.parse_args()
    assert args.rounds > 0
    binary = args.binary.resolve()
    records = []
    baseline = None
    workloads = None
    for index in range(args.rounds):
        env = os.environ.copy()
        env.pop("DEADSYNC_PERF_REVERSE", None)
        if index % 2:
            env["DEADSYNC_PERF_REVERSE"] = "1"
        current = []
        for test_filter in args.test_filter or ["draw_capture::perf::benchmark_draw_capture"]:
            run = subprocess.run(
                [str(binary), test_filter, "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                env=env, capture_output=True, text=True, encoding="utf-8",
            )
            assert run.returncode == 0, run.stdout + run.stderr
            found = False
            for line in run.stdout.splitlines():
                if "DRAW_CAPTURE_METRIC " in line:
                    found = True
                    value = json.loads(line.split("DRAW_CAPTURE_METRIC ", 1)[1])
                    identity = value.get("baseline_commit", "3d8094d9e")
                    if baseline is None:
                        baseline = identity
                    assert identity == baseline
                    if args.baseline_commit:
                        assert identity == args.baseline_commit, (identity, args.baseline_commit)
                    value.update(round=index + 1, order="after-first" if index % 2 else "before-first")
                    current.append(value)
            assert found, (test_filter, run.stdout)
        names = {row["workload"] for row in current}
        assert names and len(current) == 2 * len(names), run.stdout
        assert all({row["variant"] for row in current if row["workload"] == name} == {"before", "after"} for name in names)
        if workloads is None:
            workloads = names
        assert names == workloads
        assert all(len(row["samples"]) == 9 for row in current)
        records.extend(current)
        print(f"Completed round {index + 1}/{args.rounds}", flush=True)
    summary = []
    for name in dict.fromkeys(row["workload"] for row in records):
        values = {}
        for variant in ("before", "after"):
            samples = [sample for row in records if row["workload"] == name and row["variant"] == variant
                       for sample in row["samples"]]
            values[variant] = {key: statistics.median(sample[key] / units(sample) for sample in samples)
                               for key in ("ns", "thread_cycles", "allocations", "reallocations", "allocated_bytes")}
            values[variant]["units_per_second"] = 1e9 / values[variant]["ns"]
            unit = next(row.get("unit", "scene_frames") for row in records if row["workload"] == name)
            if unit == "scene_frames":
                values[variant]["frames_per_second"] = values[variant]["units_per_second"]
        values.update(workload=name, throughput_gain_percent=100 * (values["before"]["ns"] / values["after"]["ns"] - 1),
                      cycle_reduction_percent=100 * (1 - values["after"]["thread_cycles"] / values["before"]["thread_cycles"]))
        values["unit"] = unit
        summary.append(values)
    root = Path(__file__).resolve().parent.parent
    source_paths = ["crates/deadsync-song-lua/src/draw_capture.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_capture.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_convert.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_convert_before.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_capture/baseline.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_capture/state_baseline.rs",
                    "crates/deadsync-song-lua/src/lua_util.rs",
                    "crates/deadsync-song-lua/tests/perf/mesh.rs",
                    "crates/deadsync-song-lua/tests/perf/mesh_before.rs",
                    "crates/deadsync-song-lua/src/rage_file.rs",
                    "crates/deadsync-song-lua/tests/perf/rage_file.rs",
                    "crates/deadsync-song-lua/tests/perf/rage_file_before.rs",
                    "crates/deadsync-song-lua/tests/perf/file_open.rs",
                    "crates/deadsync-song-lua/tests/perf/file_open_before.rs",
                    "crates/deadsync-song-lua/src/values.rs",
                    "crates/deadsync-song-lua/tests/perf/values.rs",
                    "crates/deadsync-song-lua/tests/perf/values_before.rs",
                    "crates/deadsync-song-lua/src/cmd.rs",
                    "crates/deadsync-song-lua/tests/perf/cmd.rs",
                    "crates/deadsync-song-lua/tests/perf/cmd_before.rs",
                    "crates/deadsync-song-lua/tests/perf/cmd_dispatch.rs",
                    "crates/deadsync-song-lua/tests/perf/cmd_dispatch_before.rs",
                    "tests/fixtures/song_lua/spooky-door.lua",
                    "tests/fixtures/song_lua/runtime-eases.lua",
                    "crates/deadsync-song-lua/src/raw_vm.rs",
                    "crates/deadsync-song-lua/src/compiled_scene/runtime.rs",
                    "crates/deadsync-song-lua/src/model_texture.rs",
                    "crates/deadsync-song-lua/tests/perf/compiled_render.rs",
                    "crates/deadsync-song-lua/tests/perf/compiled_render_before.rs",
                    "crates/deadsync-song-lua/tests/perf/compiled_physics.rs",
                    "crates/deadsync-song-lua/tests/perf/compiled_physics_before.rs",
                    "crates/deadsync-song-lua/src/compiled_scene.rs",
                    "crates/deadsync-song-lua/src/compat.rs",
                    "crates/deadsync-song-lua/tests/perf/material_clock.rs",
                    "crates/deadsync-song-lua/tests/perf/material_clock_before.rs",
                    "tests/support/perf.rs", "Cargo.lock", "Cargo.toml",
                    "scripts/benchmark_song_lua_draw_capture.py"]
    report = dict(baseline_commit=baseline, worktree_head=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  executable_sha256=sha256(binary), source_sha256={path: sha256(root / path) for path in source_paths},
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True), system=platform.platform(),
                  processor=platform.processor(), logical_processors=os.cpu_count(), logical_cpu=2,
                  profile="release, opt-level=3, LTO disabled; identical executable and allocator for both variants",
                  cycles="QueryThreadCycleTime: scheduled current-thread CPU cycles, including kernel time; not a user-only PMU counter",
                  allocation_scope="current-thread Rust GlobalAlloc, including mlua; direct native malloc excluded",
                  test_filters=args.test_filter or ["draw_capture::perf::benchmark_draw_capture"],
                  timing_scope="values: warmed cloned input handle plus helper and result destruction; equality: borrowed input pair; hex API: warmed Lua callback matching BinaryToHex, including returned LuaString handle drop; fixture setup and final VM teardown excluded; capture: run calls retaining scene frames; conversion: consume captured input and retain converted output, output drop excluded; render: projected frame updates; physics: simulation ticks with empty render bindings and reused sound output; mesh/predicate: repeated reads, including returned mesh destruction; file core: buffered reads with cursor reset; file API: seek/read pairs including returned LuaString handle drop; file open: reopen existing virtual output, no disk I/O; path text/paths: separator conversion or lexical normalization, warm output reused except cold path; snapshot copies: virtual lookup plus copy, returned baseline buffer drop included; cmd: preprocess sources including returned output/error drop; quoted scans: unchanged delimiter controls; newline scans: count line-feed bytes; command lists: parse and consume every slice, including final empty/whitespace slice; prepared chunks: preprocessing plus Lua compilation and Function handle drop, no file I/O; reads/ticks: unchanged reads or update plus UV sampling; initialization: repeated scene initialization or create/drop clocks; fixture setup excluded; see each workload unit",
                  samples_per_variant_per_workload=args.rounds * 9, summary=summary, records=records)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
