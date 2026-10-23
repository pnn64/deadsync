"""Measure paired Lua boundary operations, retaining samples and build identities."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--noteskin", type=Path)
    parser.add_argument("--song", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--stage", required=True)
    args = parser.parse_args()
    assert args.rounds > 0 and (args.noteskin or args.song)
    root = Path(__file__).resolve().parent.parent
    runs = []
    if args.noteskin:
        runs.append((args.noteskin.resolve(), "compiler::callbacks_perf::benchmark_noteskin_callbacks_and_commands"))
    if args.song:
        runs.append((args.song.resolve(), "lua_util::upvalue_batches_perf::benchmark_upvalue_batches"))
    paths = [root / "Cargo.toml", root / "Cargo.lock", root / ".cargo/config.toml", Path(__file__).resolve()]
    for crate in ("deadlib-lua", "deadsync-song-lua", "deadsync-noteskin"):
        paths += list((root / "crates" / crate / "src").rglob("*.rs"))
    paths += list((root / "crates/deadsync-song-lua/tests/perf").glob("upvalue_batches*.rs"))
    paths += [root / name for name in (
        "crates/deadlib-lua/tests/upvalues.rs", "crates/deadlib-lua/tests/ignored_args.rs",
        "tests/support/perf.rs",
        "tests/perf/resource_support.rs",
        "crates/deadsync-song-lua/tests/perf/draw_capture.rs",
    )]
    sources = {path.relative_to(root).as_posix(): sha(path) for path in paths}
    binaries = {str(path): sha(path) for path, _ in runs}
    env = os.environ.copy()
    env["RUST_MIN_STACK"] = "33554432"
    records = []
    logs = []
    for round_index in range(args.rounds):
        for binary, test in runs:
            run = subprocess.run([str(binary), test, "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                                 cwd=root, env=env, capture_output=True, encoding="utf-8")
            logs.append(dict(round=round_index + 1, test=test, stdout=run.stdout, stderr=run.stderr,
                             returncode=run.returncode))
            assert run.returncode == 0, run.stdout + run.stderr
            rows = []
            for line in run.stdout.splitlines():
                if "LUA_BOUNDARY_METRIC " in line:
                    row = json.loads(line.split("LUA_BOUNDARY_METRIC ", 1)[1])
                    row["round"] = round_index + 1
                    rows.append(row)
            names = {row["workload"] for row in rows}
            assert len(names) in ((6, 8) if "noteskin" in test else (5,))
            assert len(rows) == len(names) * 18
            records.extend(rows)
            print(f"{round_index + 1}/{args.rounds}: {test}", flush=True)
    summary = []
    for name in dict.fromkeys(row["workload"] for row in records):
        variants = {}
        for current in (False, True):
            rows = [row for row in records if row["workload"] == name and row["current"] == current]
            variants["after" if current else "before"] = {
                key: statistics.median(row[key] / row["operations"] for row in rows)
                for key in ("ns", "cycles", "allocations", "reallocations", "bytes")
            }
            variants["after" if current else "before"]["operations_per_second"] = 1e9 / variants["after" if current else "before"]["ns"]
        summary.append(dict(workload=name, **variants,
                            throughput_gain_percent=100 * (variants["before"]["ns"] / variants["after"]["ns"] - 1)))
    assert all(sha(root / name) == digest for name, digest in sources.items()), "source changed during measurement"
    assert all(sha(Path(name)) == digest for name, digest in binaries.items()), "binary changed during measurement"
    result = dict(stage=args.stage, head=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True), system=platform.platform(),
                  logical_cpu=2, rounds=args.rounds, samples_per_variant=args.rounds * 9,
                  build_settings=["--release", "--locked", "-j", "2", "--config", "profile.release.lto=false"],
                  binaries=binaries, sources=sources, summary=summary, records=records, logs=logs)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for row in summary:
        print(row["workload"], f"{row['throughput_gain_percent']:+.1f}% throughput", flush=True)


if __name__ == "__main__":
    main()
