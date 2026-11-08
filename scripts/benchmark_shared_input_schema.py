"""Compare per-callback and reused Song schemas with the same native Group."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from benchmark_shared_input import sha, summarize


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("before", "after", "fixtures", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    args = parser.parse_args()
    assert args.pairs > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binaries = {name: getattr(args, name).resolve() for name in ("before", "after")}
    hashes = {name: sha(path) for name, path in binaries.items()}
    records, receipts, traces, memory = [], [], {}, []
    fixtures = {count: (args.fixtures / f"actors-{count}.lua").resolve() for count in (1, 16, 64)}
    for pair in range(args.pairs):
        for count, fixture in fixtures.items():
            for name in (("before", "after") if pair % 2 == 0 else ("after", "before")):
                report = out / f"{count}-{name}-{pair}.json"
                command = [str(binaries[name]), str(report), str(fixture), "prepare"]
                with report.with_suffix(".log").open("wb") as log:
                    subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
                data = json.loads(report.read_bytes())
                assert data["loading_vm_destroyed"]
                assert all(row["workload"] == "actor_input_snapshot_prepare" for row in data["rows"])
                if count in traces:
                    assert traces[count] == data["trace"]
                else:
                    traces[count] = data["trace"]
                records.extend(dict(row, variant=name, callbacks=count, pair=pair) for row in data["rows"])
                memory.append(dict(callbacks=count, variant=name, values=data["retained_memory"]))
                receipts.append(dict(command=command, sha256=sha(report)))
                print(count, name, pair + 1, "recorded", flush=True)
    summaries = []
    for count in fixtures:
        variants = {name: summarize([row for row in records if row["callbacks"] == count and row["variant"] == name]) for name in binaries}
        summaries.append(dict(callbacks=count, **variants))
    assert all(sha(path) == hashes[name] for name, path in binaries.items())
    result = dict(summary=summaries, records=records, reports=receipts, retained_memory=memory,
                  trace_sha256={count: hashlib.sha256(json.dumps(trace, sort_keys=True).encode()).hexdigest() for count, trace in traces.items()},
                  binaries={name: dict(path=str(path), sha256=hashes[name]) for name, path in binaries.items()},
                  fixtures={count: dict(path=str(path), sha256=sha(path)) for count, path in fixtures.items()},
                  pairs=args.pairs, logical_cpu=2,
                  scope="Both variants use shared cells/scratch and static common enum names. Only schema preparation changes; before repeats actor proofs and bindings per callback. All native/Lua traces match exactly.")
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for row in summaries:
        print(row["callbacks"], "cycles", row["before"]["thread_cpu_cycles"], "->", row["after"]["thread_cpu_cycles"], "allocations", row["before"]["allocations"], "->", row["after"]["allocations"])


if __name__ == "__main__":
    main()
