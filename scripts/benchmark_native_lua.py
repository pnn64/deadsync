"""Compare native callback execution, capture inspection and compiler state flow."""
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


METRICS = ("wall_seconds", "thread_cpu_cycles", "allocations", "reallocations", "allocated_bytes")


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
        entry["cycles_reduction_percent"] = 100 * (1 - entry["after"]["thread_cpu_cycles"] / entry["before"]["thread_cpu_cycles"])
        result.append(entry)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before-flow", type=Path, required=True)
    parser.add_argument("--after-flow", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    args = parser.parse_args()
    assert args.pairs > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binaries = {key: getattr(args, key).resolve() for key in ("before_flow", "after_flow")}
    hashes = {key: sha(path) for key, path in binaries.items()}
    paths = [Path(__file__).resolve(), root / "Cargo.toml", root / "Cargo.lock", root / ".cargo/config.toml",
             root / "crates/deadlib-lua-legacy/Cargo.toml", root / "tests/support/song_lua_metrics.rs",
             root / "crates/deadlib-lua-legacy/examples/compiled_program.rs"]
    paths += list((root / "crates/deadlib-lua-legacy/src").rglob("*.rs"))
    sources = {path.relative_to(root).as_posix(): sha(path) for path in paths}
    records, flow, memory = [], [], []
    for pair in range(args.pairs):
        for variant in (("before_flow", "after_flow") if pair % 2 == 0 else ("after_flow", "before_flow")):
            report = out / f"{variant}-{pair}.json"
            with report.with_suffix(".log").open("wb") as log:
                subprocess.run([str(binaries[variant]), str(report)], cwd=root,
                               stdout=log, stderr=subprocess.STDOUT, check=True)
            data = json.loads(report.read_bytes())
            assert all(item["loading_vm_destroyed"] for item in data["retained_memory"])
            memory.append(dict(pair=pair, variant=variant, values=data["retained_memory"]))
            for row in data["rows"]:
                row.update(pair=pair, binary=variant)
                # Both flags in this workload run identical native compilation.
                if row["workload"].endswith("native_lower_prepare_destroy"):
                    flow.append(dict(row, current=variant == "after_flow"))
                elif row["workload"].endswith("load_prepare") and row["current"]:
                    flow.append(dict(row, workload=row["workload"] + "_flow", current=variant == "after_flow"))
                if variant == "after_flow" and not row["workload"].endswith("native_lower_prepare_destroy"):
                    records.append(row)
            print(f"{pair + 1}/{args.pairs}: {variant}, VM destruction and parity checks passed", flush=True)
    assert all(sha(root / name) == digest for name, digest in sources.items()), "source changed during measurement"
    assert all(sha(binaries[key]) == digest for key, digest in hashes.items()), "binary changed during measurement"
    summary = summarize(records)
    flow_summary = summarize(flow)
    result = dict(head=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
                  sources=sources, binaries={key: dict(path=str(path), sha256=hashes[key]) for key, path in binaries.items()},
                  rustc=subprocess.check_output(["rustc", "-Vv"], text=True), system=platform.platform(),
                  logical_processors=os.cpu_count(), logical_cpu=2,
                  build_settings=["--release", "--locked", "-j", "2", "--config", "profile.release.lto=false"],
                  runtime_operation="eight ordered input events", summary=summary, flow_summary=flow_summary,
                  records=records, flow_records=flow, retained_memory=memory)
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for row in summary + flow_summary:
        print(row["workload"], f"{row['throughput_gain_percent']:+.2f}% throughput",
              f"{row['cycles_reduction_percent']:+.2f}% fewer cycles")


if __name__ == "__main__":
    main()
