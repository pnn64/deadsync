"""Measure frozen/current draw capture in one release test executable on Windows."""
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=6)
    args = parser.parse_args()
    assert args.rounds > 0
    binary = args.binary.resolve()
    records = []
    for index in range(args.rounds):
        env = os.environ.copy()
        env.pop("DEADSYNC_PERF_REVERSE", None)
        if index % 2:
            env["DEADSYNC_PERF_REVERSE"] = "1"
        run = subprocess.run(
            [str(binary), "draw_capture::perf::benchmark_draw_capture", "--exact",
             "--ignored", "--nocapture", "--test-threads=1"],
            env=env, capture_output=True, text=True, encoding="utf-8",
        )
        assert run.returncode == 0, run.stdout + run.stderr
        current = []
        for line in run.stdout.splitlines():
            if "DRAW_CAPTURE_METRIC " in line:
                value = json.loads(line.split("DRAW_CAPTURE_METRIC ", 1)[1])
                value.update(round=index + 1, order="after-first" if index % 2 else "before-first")
                current.append(value)
        assert len(current) == 8, run.stdout
        records.extend(current)
        print(f"Completed round {index + 1}/{args.rounds}", flush=True)
    summary = []
    for name in dict.fromkeys(row["workload"] for row in records):
        values = {}
        for variant in ("before", "after"):
            samples = [sample for row in records if row["workload"] == name and row["variant"] == variant
                       for sample in row["samples"]]
            values[variant] = {key: statistics.median(sample[key] / sample["frames"] for sample in samples)
                               for key in ("ns", "thread_cycles", "allocations", "reallocations", "allocated_bytes")}
            values[variant]["frames_per_second"] = 1e9 / values[variant]["ns"]
        values.update(workload=name, throughput_gain_percent=100 * (values["before"]["ns"] / values["after"]["ns"] - 1),
                      cycle_reduction_percent=100 * (1 - values["after"]["thread_cycles"] / values["before"]["thread_cycles"]))
        summary.append(values)
    root = Path(__file__).resolve().parent.parent
    source_paths = ["crates/deadsync-song-lua/src/draw_capture.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_capture.rs",
                    "crates/deadsync-song-lua/tests/perf/draw_capture/baseline.rs",
                    "tests/support/perf.rs", "Cargo.lock", "Cargo.toml",
                    "scripts/benchmark_song_lua_draw_capture.py"]
    report = dict(baseline_commit="3d8094d9e", worktree_head=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  executable_sha256=sha256(binary), source_sha256={path: sha256(root / path) for path in source_paths},
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True), system=platform.platform(),
                  processor=platform.processor(), logical_processors=os.cpu_count(), logical_cpu=2,
                  profile="release, opt-level=3, LTO disabled; identical executable and allocator for both variants",
                  cycles="QueryThreadCycleTime: scheduled current-thread CPU cycles, including kernel time; not a user-only PMU counter",
                  allocation_scope="current-thread Rust GlobalAlloc, including mlua; direct native malloc excluded",
                  timing_scope="capture: run calls retaining frames; conversion: consume capture, construct and drop output frames; fixture setup and Lua teardown excluded",
                  samples_per_variant_per_workload=args.rounds * 9, summary=summary, records=records)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
