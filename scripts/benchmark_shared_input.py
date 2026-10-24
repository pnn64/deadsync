"""Measure native callback groups, input names and real scene input snapshots."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess

METRICS = ("wall_seconds", "thread_cpu_cycles", "allocations", "reallocations", "allocated_bytes")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def summarize(rows):
    result = {key: statistics.median(row[key] / row["operations"] for row in rows) for key in METRICS}
    result["operations_per_second"] = 1 / result["wall_seconds"]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("before-actor", "after-actor", "group", "frames", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    args = parser.parse_args()
    assert args.pairs > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binaries = {name: getattr(args, name).resolve() for name in ("before_actor", "after_actor", "group", "frames")}
    binary_hashes = {name: sha(path) for name, path in binaries.items()}
    paths = [p for crate in ("deadlib-lua-legacy", "deadsync-song-lua", "deadsync-noteskin")
             for p in (root / "crates" / crate / "src").rglob("*.rs")]
    paths += [Path(__file__).resolve(), root / "crates/deadlib-lua-legacy/examples/grouped_program.rs",
              root / "crates/deadsync-song-lua/examples/input_frames.rs",
              root / "crates/deadsync-song-lua/examples/actor_input.rs",
              root / "crates/deadsync-song-lua/tests/perf/input_frame_2355.rs",
              root / "crates/deadsync-song-lua/tests/fixtures/actor-input.lua",
              root / "crates/deadsync-song-lua/tests/fixtures/stateful-input.lua"]
    sources = {p.relative_to(root).as_posix(): sha(p) for p in paths}
    template = (root / "crates/deadsync-song-lua/tests/fixtures/actor-input.lua").read_text()
    fixtures = {}
    for count in (1, 16, 64):
        source = template.replace("        SCREENMAN:GetTopScreen():AddInputCallback", f"        for index=1,{count} do\n        SCREENMAN:GetTopScreen():AddInputCallback", 1)
        source = source.replace("return event.DeviceInput.down and event.PlayerNumber == \"PlayerNumber_P1\"", "return false")
        assert source.count("return false") == 1
        source = source.replace("        end)", "        end)\n        end", 1)
        fixture = out / f"actors-{count}.lua"
        fixture.write_text(source, encoding="utf-8")
        fixtures[count] = fixture
    records, memory, reports = [], [], []
    traces = {}

    def run(name, workload, pair, fixture=None):
        report = out / f"{workload}-{name}-{pair}.json"
        command = [str(binaries[name]), str(report)]
        if fixture is not None:
            command.append(str(fixture))
        with report.with_suffix(".log").open("wb") as log:
            subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
        data = json.loads(report.read_bytes())
        reports.append(dict(path=str(report), sha256=sha(report), command=command))
        records.extend(dict(row, binary=name, case=workload, pair=pair) for row in data["rows"])
        if "trace" in data:
            assert data["loading_vm_destroyed"]
            if workload in traces:
                assert data["trace"] == traces[workload], f"input trace differs: {report}"
            else:
                traces[workload] = data["trace"]
            memory.append(dict(binary=name, case=workload, pair=pair, values=data["retained_memory"]))
        elif "memory" in data:
            memory.append(dict(binary=name, case=workload, pair=pair, values=data["memory"]))
        else:
            assert data["equal_fields"]
        print(workload, name, pair + 1, "recorded", flush=True)

    for pair in range(args.pairs):
        for name in (("group", "frames") if pair % 2 == 0 else ("frames", "group")):
            run(name, name, pair)
        for count in (1, 16, 64):
            # Fixtures repeat the same callback and always return false, so every
            # callback executes. No callback captures the loop index.
            fixture = fixtures[count]
            for name in (("before_actor", "after_actor") if pair % 2 == 0 else ("after_actor", "before_actor")):
                run(name, f"actors_{count}", pair, fixture)
        run("after_actor", "stateful", pair, root / "crates/deadsync-song-lua/tests/fixtures/stateful-input.lua")
    summaries = []
    for case in dict.fromkeys(row["case"] for row in records):
        for workload in dict.fromkeys(row["workload"] for row in records if row["case"] == case):
            variants = {}
            selected = [row for row in records if row["case"] == case and row["workload"] == workload]
            for row in selected:
                key = row["binary"]
                if "current" in row:
                    suffixes = ("_native", "_lua") if key.endswith("actor") else ("_after", "_before")
                    key += suffixes[0 if row["current"] else 1]
                variants.setdefault(key, []).append(row)
            summaries.append(dict(case=case, workload=workload, variants={key: summarize(rows) for key, rows in variants.items()}))
    assert all(sha(root / name) == digest for name, digest in sources.items())
    assert all(sha(binaries[name]) == digest for name, digest in binary_hashes.items())
    result = dict(summary=summaries, records=records, retained_memory=memory, traces=traces,
                  reports=reports, source_sha256=sources,
                  fixtures={f"actors_{n}": dict(path=str(path), sha256=sha(path)) for n, path in fixtures.items()},
                  binaries={name: dict(path=str(path), sha256=binary_hashes[name]) for name, path in binaries.items()},
                  pairs=args.pairs, logical_cpu=2,
                  build_settings=["--release", "--locked", "-j", "2", "--config", "profile.release.lto=false"],
                  validation="Exact Lua/native actor state traces in every actor process; loading-VM finalizer before detached native execution. Shared scalar bits and ordered actions checked by focused tests; rendering checked separately.")
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for row in summaries:
        print(row["case"], row["workload"], {key: round(value["thread_cpu_cycles"], 2) for key, value in row["variants"].items()})


if __name__ == "__main__":
    main()
