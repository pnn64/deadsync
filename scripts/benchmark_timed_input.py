"""Compare schema preparation, enum frames and VM-free input/update units."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from benchmark_shared_input import sha, summarize


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("sdk-before", "sdk-after", "frames", "timed", "actor-before", "actor-after", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    args = parser.parse_args()
    assert args.pairs > 0
    root = Path(__file__).resolve().parent.parent
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binaries = {name: getattr(args, name).resolve() for name in ("sdk_before", "sdk_after", "frames", "timed", "actor_before", "actor_after")}
    hashes = {name: sha(path) for name, path in binaries.items()}
    paths = [p for crate in ("deadlib-lua-legacy", "deadsync-song-lua", "deadsync-noteskin") for p in (root/"crates"/crate/"src").rglob("*.rs")]
    paths += [root/name for name in ("crates/deadlib-lua-legacy/examples/prepared_schema.rs", "crates/deadsync-song-lua/examples/input_frames.rs", "crates/deadsync-song-lua/examples/timed_input.rs", "crates/deadsync-song-lua/examples/actor_input.rs", "crates/deadsync-song-lua/tests/perf/input_frame_2355.rs", "crates/deadsync-song-lua/tests/fixtures/actor-input.lua", "crates/deadsync-song-lua/tests/fixtures/timed-input.lua")]
    paths += [Path(__file__).resolve(), root/"scripts/benchmark_shared_input.py"]
    paths += [root/name for name in ("Cargo.toml", "Cargo.lock", ".cargo/config.toml", "crates/deadlib-lua-legacy/Cargo.toml", "crates/deadsync-song-lua/Cargo.toml", "tests/support/song_lua_metrics.rs")]
    sources = {p.relative_to(root).as_posix(): sha(p) for p in paths}
    template = (root/"crates/deadsync-song-lua/tests/fixtures/actor-input.lua").read_text()
    fixtures = {}
    for count in (1, 16, 64):
        source = template.replace("        SCREENMAN:GetTopScreen():AddInputCallback", f"        for index=1,{count} do\n        SCREENMAN:GetTopScreen():AddInputCallback", 1)
        source = source.replace('return event.DeviceInput.down and event.PlayerNumber == "PlayerNumber_P1"', "return false")
        assert source.count("return false") == 1
        source = source.replace("        end)", "        end)\n        end", 1)
        fixture = out/f"actors-{count}.lua"
        fixture.write_text(source, encoding="utf-8")
        fixtures[count] = fixture
    records, reports, memory, traces = [], [], [], {}

    def run(name, pair, case, fixture=None):
        report = out/f"{case}-{name}-{pair}.json"
        command = [str(binaries[name]), str(report)]
        if fixture is not None:
            command += [str(fixture), "prepare"]
        with report.with_suffix(".log").open("wb") as log:
            subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
        data = json.loads(report.read_bytes())
        reports.append(dict(command=command, report_sha256=sha(report)))
        records.extend(dict(row, binary=name, pair=pair, case=case) for row in data["rows"])
        if "trace" in data:
            assert data["loading_vm_destroyed"]
            if case in traces:
                assert data["trace"] == traces[case], f"exact trace changed: {report}"
            else:
                traces[case] = data["trace"]
            memory.append(dict(case=case, binary=name, pair=pair, values=data["retained_memory"]))
        if name == "frames":
            assert data["equal_fields"]
        print(case, name, pair+1, "recorded", flush=True)

    for pair in range(args.pairs):
        for name in (("sdk_before", "sdk_after") if pair%2 == 0 else ("sdk_after", "sdk_before")):
            run(name, pair, "schema")
        run("frames", pair, "frames")
        run("timed", pair, "timed")
        for count, fixture in fixtures.items():
            for name in (("actor_before", "actor_after") if pair%2 == 0 else ("actor_after", "actor_before")):
                run(name, pair, f"actors_{count}", fixture)
    summary = []
    for case in dict.fromkeys(r["case"] for r in records):
        for workload in dict.fromkeys(r["workload"] for r in records if r["case"] == case):
            groups = {}
            for row in records:
                if row["case"] != case or row["workload"] != workload:
                    continue
                name = row["binary"]
                if name == "sdk_before":
                    # Its two flags run the identical legacy path. Keep both in
                    # raw samples; use the false rows for the stage comparison.
                    if row["current"]:
                        continue
                    name = "original"
                elif name == "sdk_after":
                    name = "prepared" if row["current"] else "indexed"
                elif "current" in row:
                    name += "_after" if row["current"] else "_before"
                groups.setdefault(name, []).append(row)
            summary.append(dict(case=case, workload=workload, variants={name: summarize(rows) for name, rows in groups.items()}))
    assert all(sha(root/name) == digest for name, digest in sources.items())
    assert all(sha(path) == hashes[name] for name, path in binaries.items())
    result = dict(summary=summary, records=records, reports=reports, traces=traces, retained_memory=memory,
                  trace_sha256={name: hashlib.sha256(json.dumps(trace, sort_keys=True).encode()).hexdigest() for name, trace in traces.items()},
                  sources=sources, fixtures={n: dict(path=str(p), sha256=sha(p)) for n,p in fixtures.items()},
                  binaries={name: dict(path=str(path), sha256=hashes[name]) for name,path in binaries.items()},
                  pairs=args.pairs, logical_cpu=2,
                  scope="Input/update callbacks are an independent native snapshot of the authored root. Default scene dispatch, host updates and other lifecycle behavior remain Lua. All reported actor and timer traces match exactly; a loading-VM finalizer precedes detached execution.")
    (out/"summary.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")
    for row in summary:
        print(row["case"], row["workload"], {name: round(value["thread_cpu_cycles"], 2) for name,value in row["variants"].items()})


if __name__ == "__main__":
    main()
