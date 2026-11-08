"""Compare VM-free actor inputs and isolated native compiler allocation changes."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess

METRICS = ("wall_seconds", "thread_cpu_cycles", "allocations", "reallocations", "allocated_bytes")


def summarize(rows):
    return {key: statistics.median(row[key] / row["operations"] for row in rows) for key in METRICS}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("sdk-before", "sdk-contracts", "sdk-decode", "actor"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    args = parser.parse_args()
    assert args.pairs > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binaries = {key: getattr(args, key).resolve() for key in ("sdk_before", "sdk_contracts", "sdk_decode", "actor")}
    paths = list((root / "crates/deadlib-lua-legacy/src").rglob("*.rs"))
    paths += list((root / "crates/deadsync-song-lua/src").rglob("*.rs"))
    paths += [root / "crates/deadsync-song-lua/examples/actor_input.rs", Path(__file__).resolve()]
    sources = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    hashes = {key: hashlib.sha256(path.read_bytes()).hexdigest() for key, path in binaries.items()}
    records, actor_records, traces, memory = [], [], [], []
    for pair in range(args.pairs):
        variants = ("sdk_before", "sdk_contracts", "sdk_decode")
        # The first three orders balance every stage position; all six also
        # balance adjacency. Reversing alone always leaves contracts in the middle.
        orders = ((0, 1, 2), (1, 2, 0), (2, 0, 1),
                  (2, 1, 0), (1, 0, 2), (0, 2, 1))
        order = tuple(variants[index] for index in orders[pair % len(orders)])
        for key in (*order, "actor"):
            report = out / f"{key}-{pair}.json"
            with report.with_suffix(".log").open("wb") as log:
                subprocess.run([str(binaries[key]), str(report)], cwd=root, stdout=log,
                               stderr=subprocess.STDOUT, check=True)
            data = json.loads(report.read_bytes())
            if key == "actor":
                assert data["loading_vm_destroyed"]
                if traces: assert data["trace"] == traces[0]
                traces.append(data["trace"])
                memory.append(data["retained_memory"])
                actor_records += [dict(row, pair=pair) for row in data["rows"]]
            else:
                records += [dict(row, variant=key, pair=pair) for row in data["rows"]
                            if row["workload"].endswith("_native_lower_prepare_destroy")]
        print(f"{pair+1}/{args.pairs}: compiler samples and exact actor state traces recorded", flush=True)
    compiler = []
    for workload in dict.fromkeys(row["workload"] for row in records):
        entry = {"workload": workload}
        for key in ("sdk_before", "sdk_contracts", "sdk_decode"):
            entry[key] = summarize([row for row in records if row["workload"] == workload and row["variant"] == key])
        compiler.append(entry)
    runtime = {"before": summarize([row for row in actor_records if row["workload"] == "actor_input_runtime" and not row["current"]]),
               "after": summarize([row for row in actor_records if row["workload"] == "actor_input_runtime" and row["current"]])}
    prepare = summarize([row for row in actor_records if row["workload"] == "actor_input_snapshot_prepare"])
    assert all(hashlib.sha256((root/name).read_bytes()).hexdigest() == digest for name, digest in sources.items())
    assert all(hashlib.sha256(binaries[key].read_bytes()).hexdigest() == digest for key, digest in hashes.items())
    result = dict(compiler=compiler, runtime=runtime, prepare=prepare, compiler_records=records,
                  actor_records=actor_records, traces=traces, retained_memory=memory, sources=sources,
                  binaries={key: dict(path=str(path), sha256=hashes[key]) for key,path in binaries.items()},
                  build_settings=["--release", "--locked", "-j", "2", "--config", "profile.release.lto=false"],
                  logical_cpu=2, compiler_unit="one dump/lower/machine prepare/destruction", runtime_unit="one input",
                  scope="Native input snapshot owns actor states and code. Live scene dispatch, lifecycle, shared scalar state and timers retain Lua. Render validation reported separately.")
    (out/"summary.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
    for entry in compiler:
        print(entry["workload"], {key: entry[key] for key in ("sdk_before", "sdk_contracts", "sdk_decode")})
    print("runtime", runtime, "prepare", prepare)


if __name__ == "__main__":
    main()
