"""Measure ordered Song input dispatch, raw field batching and callback storage."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

BASELINE = "5f6cf66777b1bbaea0b7d00c4f723fecb1861653"
METRICS = ("wall_seconds", "thread_cpu_cycles", "allocations", "reallocations", "allocated_bytes")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def summarize(records):
    result = []
    for workload in dict.fromkeys(row["workload"] for row in records):
        entry = {"workload": workload}
        for current, variant in ((False, "before"), (True, "after")):
            rows = [row for row in records if row["workload"] == workload and row["current"] == current]
            entry[variant] = {key: statistics.median(row[key] / row["operations"] for row in rows)
                              for key in METRICS}
            entry[variant]["samples"] = len(rows)
            entry[variant]["operations_per_second"] = 1 / entry[variant]["wall_seconds"]
        entry["throughput_gain_percent"] = 100 * (entry["before"]["wall_seconds"] / entry["after"]["wall_seconds"] - 1)
        entry["cycle_reduction_percent"] = 100 * (1 - entry["after"]["thread_cpu_cycles"] / entry["before"]["thread_cpu_cycles"])
        result.append(entry)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("dispatch", "audit-before", "audit-after"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    args = parser.parse_args()
    assert args.pairs > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binaries = {name: getattr(args, name).resolve() for name in ("dispatch", "audit_before", "audit_after")}
    hashes = {key: sha(path) for key, path in binaries.items()}
    paths = [Path(__file__).resolve(), root / "Cargo.toml", root / "Cargo.lock", root / ".cargo/config.toml"]
    for crate in ("deadlib-lua", "deadsync-song-lua"):
        for directory in ("src", "examples"):
            paths += list((root / "crates" / crate / directory).rglob("*.rs"))
    paths += [root / name for name in ("tests/support/song_lua_metrics.rs",
              "crates/deadsync-song-lua/tests/perf/input_dispatch_baseline.rs",
              "crates/deadsync-song-lua/tests/perf/input_dispatch_tree.rs",
              "crates/deadsync-song-lua/tests/fixtures/closed-input.lua")]
    sources = {path.relative_to(root).as_posix(): sha(path) for path in paths}
    original = subprocess.check_output(["git", "show", BASELINE + ":crates/deadsync-song-lua/src/input.rs"], cwd=root)
    frozen = (root / "crates/deadsync-song-lua/tests/perf/input_dispatch_baseline.rs").read_bytes()
    assert frozen.replace(b"\r\n", b"\n").startswith(
        b"//! Frozen input registry/dispatcher from 5f6cf66777b1.\n" + original.replace(b"\r\n", b"\n").split(b"#[cfg(test)]")[0])
    records, audits, memory = [], [], []
    event_snapshot = None

    def run(key, report, extra=()):
        with report.with_suffix(".log").open("wb") as log:
            subprocess.run([str(binaries[key]), *map(str, extra), str(report)], cwd=root,
                           stdout=log, stderr=subprocess.STDOUT, check=True)
        return json.loads(report.read_bytes())

    for pair in range(args.pairs):
        order = ("before", "after") if pair % 2 == 0 else ("after", "before")
        data = run("dispatch", out / f"dispatch-{pair}.json")
        assert data["native_artifact"]["loading_vm_destroyed"]
        memory.append(dict(pair=pair, **data["native_artifact"]))
        if event_snapshot is None:
            event_snapshot = data["event_snapshots"]
        assert data["event_snapshots"] == event_snapshot
        for row in data["rows"]:
            # Runtime rows originally contain eight ordered inputs per operation.
            records.append(dict(row, pair=pair, operations=row["operations"] * (1 if row["workload"].startswith("prepare_") else 8)))
        for variant in order:
            data = run("audit_" + variant, out / f"audit-{variant}-{pair}.json",
                       [root / "crates/deadsync-song-lua/tests/fixtures/closed-input.lua"])
            if audits:
                assert data == audits[0]["trace"], "normal live-scene input/state/output trace changed"
            assert [row["handled"] for row in data["frames"]] == [True, True, False, True, True, False]
            audits.append(dict(pair=pair, variant=variant, trace=data))
        print(f"{pair + 1}/{args.pairs}: dispatch, registry, raw event order and normal scene traces match", flush=True)
    assert all(sha(root / name) == digest for name, digest in sources.items()), "source changed during measurement"
    assert all(sha(binaries[key]) == digest for key, digest in hashes.items()), "binary changed during measurement"
    summary = summarize(records)
    result = dict(baseline_commit=BASELINE, sources=sources,
                  binaries={key: dict(path=str(path), sha256=hashes[key]) for key, path in binaries.items()},
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True), system=platform.platform(),
                  logical_processors=os.cpu_count(), logical_cpu=2,
                  build_settings=["--release", "--locked", "-j", "2", "--config", "profile.release.lto=false"],
                  runtime_unit="one input event; preparation rows are one prepared VM/registry",
                  summary=summary, records=records,
                  live_scene_audits=audits, event_snapshots=event_snapshot, native_artifact_memory=memory,
                  validation="Original functions and live scene VM remain rooted; input-only compiled artifacts themselves own no Lua. Actor actions/captures retain Lua fallback. No pixel claim.")
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for row in summary:
        print(row["workload"], f"{row['throughput_gain_percent']:+.2f}% throughput",
              f"{row['cycle_reduction_percent']:+.2f}% fewer cycles")


if __name__ == "__main__":
    main()
