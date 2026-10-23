"""Paired compiler-state operations and exact complete noteskin output checks."""
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
    parser.add_argument("--song", type=Path, required=True)
    parser.add_argument("--noteskin-before", type=Path, required=True)
    parser.add_argument("--noteskin-after", type=Path, required=True)
    parser.add_argument("--noteskin-tests", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=3)
    args = parser.parse_args()
    assert args.rounds > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    paths = [Path(__file__).resolve(), root / "Cargo.toml", root / "Cargo.lock", root / ".cargo/config.toml"]
    for crate in ("deadlib-lua", "deadsync-song-lua", "deadsync-noteskin"):
        for folder in ("src", "examples"):
            paths += list((root / "crates" / crate / folder).rglob("*.rs"))
    paths += [root / "crates/deadsync-noteskin/tests/fixtures/alias-normalization/NoteSkin.lua",
              root / "tests/support/song_lua_metrics.rs",
              root / "crates/deadlib-lua/tests/raw_array.rs", root / "crates/deadlib-lua/tests/upvalues.rs"]
    sources = {path.relative_to(root).as_posix(): sha(path) for path in paths}
    binaries = {key: getattr(args, key).resolve() for key in ("song", "noteskin_before", "noteskin_after", "noteskin_tests")}
    hashes = {key: sha(path) for key, path in binaries.items()}
    records, logs = [], []
    expected = None
    for trial in range(args.rounds):
        report = out / f"operations-{trial}.json"
        subprocess.run([str(binaries["song"]), str(report)], cwd=root, check=True)
        for row in json.loads(report.read_bytes())["rows"]:
            row["round"] = trial
            records.append(row)
        run = subprocess.run([str(binaries["noteskin_tests"]),
                              "compiler::aliases_perf::benchmark_noteskin_aliases", "--exact", "--ignored",
                              "--nocapture", "--test-threads=1"], cwd=root, capture_output=True, text=True, check=True)
        logs.append(dict(round=trial, stdout=run.stdout, stderr=run.stderr))
        rows = []
        for line in run.stdout.splitlines():
            if "LUA_BOUNDARY_METRIC " not in line:
                continue
            item = json.loads(line.split("LUA_BOUNDARY_METRIC ", 1)[1])
            rows.append(dict(workload=item["workload"], current=item["current"], sample=item["sample"],
                             operations=item["operations"], wall_seconds=item["ns"]/1e9,
                             thread_cpu_cycles=item["cycles"], allocations=item["allocations"],
                             reallocations=item["reallocations"], allocated_bytes=item["bytes"], round=trial))
        assert len(rows) == 4 * 2 * 9
        records.extend(rows)
        for current in ([False, True] if trial % 2 == 0 else [True, False]):
            report = out / f"noteskin-{current}-{trial}.json"
            subprocess.run([str(binaries["noteskin_after" if current else "noteskin_before"]),
                            str(root / "crates/deadsync-noteskin/tests/fixtures/alias-normalization"),
                            str(report), "9"], cwd=root, check=True)
            data = json.loads(report.read_bytes())
            if expected is None:
                expected = data["snapshot"]
            assert data["snapshot"] == expected, "complete compiled noteskin bytes changed"
            for row in data["samples"]:
                row.update(workload="noteskin_complete_aliases", current=current, operations=1, round=trial)
                records.append(row)
        print(f"{trial+1}/{args.rounds}: operations, aliases, complete noteskin bytes", flush=True)
    summary = []
    for workload in dict.fromkeys(row["workload"] for row in records):
        entry = {"workload": workload}
        for current, variant in ((False, "before"), (True, "after")):
            rows = [row for row in records if row["workload"] == workload and row["current"] == current]
            entry[variant] = {key: statistics.median(row[key]/row["operations"] for row in rows)
                              for key in ("wall_seconds", "thread_cpu_cycles", "allocations", "reallocations", "allocated_bytes")}
            entry[variant]["operations_per_second"] = 1/entry[variant]["wall_seconds"]
        entry["throughput_gain_percent"] = 100*(entry["before"]["wall_seconds"]/entry["after"]["wall_seconds"]-1)
        summary.append(entry)
    assert all(sha(root/name) == digest for name, digest in sources.items()), "source changed during measurement"
    assert all(sha(binaries[key]) == digest for key, digest in hashes.items()), "binary changed during measurement"
    result = dict(head=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
                  sources=sources, binaries={key:dict(path=str(path),sha256=hashes[key]) for key,path in binaries.items()},
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True), system=platform.platform(),
                  logical_processors=os.cpu_count(), logical_cpu=2, samples_per_variant=args.rounds*9,
                  build_settings=["--release", "--locked", "-j", "2", "--config", "profile.release.lto=false"],
                  noteskin_snapshot_sha256=hashlib.sha256(bytes(expected)).hexdigest(),
                  summary=summary, records=records, logs=logs)
    (out / "summary.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")
    for row in summary:
        print(row["workload"], f"{row['throughput_gain_percent']:+.1f}% throughput")


if __name__ == "__main__":
    main()
