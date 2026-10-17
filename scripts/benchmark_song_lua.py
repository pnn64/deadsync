"""Measure unchanged song Lua inputs with perf counters and allocation accounting.

Run on Linux with release full_song_lua and song_lua_performance executables.
The JSON config supplies named fixture/archive or compile/simfile cases. All
reports remain uncompressed; extraction paths and opaque actor addresses are
normalized when hashing complete reports. Actor alias relationships remain exact.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shlex
import subprocess
from collections import deque


def normalized_report_hash(path):
    digest = hashlib.sha256()
    temporary = re.compile(rb"/tmp/\.tmp[A-Za-z0-9]+")
    actor = re.compile(rb'^(\s*"actor": ")0x([0-9a-fA-F]+)(",?\s*)$')
    scene_line = re.compile(rb'^        "scene": (\d+),?\s*$')
    name_line = re.compile(rb'^\s*"name": ".*",\s*$')
    path_line = re.compile(rb'^\s*"path": ".*"\s*$')
    handles = {}
    scene = 0
    with path.open("rb") as stream:
        pending = deque()
        lines = iter(stream)
        def take():
            return pending.popleft() if pending else next(lines, b"")
        while line := take():
            if match := scene_line.match(line):
                scene = int(match[1])
            if match := actor.match(line):
                # Only the recorder's actor/name/path handle shape qualifies.
                # Preserve names, ancestry and alias relationships; arbitrary
                # hexadecimal strings remain part of the exact comparison.
                name, ancestry = take(), take()
                pending.extendleft((ancestry, name))
                if name_line.match(name) and path_line.match(ancestry):
                    key = (scene, match[2])
                    identifier = handles.setdefault(key, len(handles) + 1)
                    line = match[1] + f"<actor:{identifier}>".encode() + match[3]
            digest.update(temporary.sub(b"/tmp/<extracted>", line))
    return digest.hexdigest()


def perf_values(path):
    values = {}
    for line in path.read_text().splitlines():
        fields = line.split("\t")
        if len(fields) >= 3:
            try:
                values[fields[2].strip()] = float(fields[0])
            except ValueError:
                pass
    assert values.get("cycles:u", 0) > 0, "hardware cycles were not collected"
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--fixture-runner", type=Path, required=True)
    parser.add_argument("--compile-runner", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument("--perf-command", default="perf")
    parser.add_argument("--program-prefix", default="")
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args()
    assert args.iterations > 0
    args.out.mkdir(parents=True, exist_ok=True)
    config = json.loads(args.config.read_bytes())
    before = json.loads(args.compare.read_bytes()) if args.compare else None
    results = []
    for case in config["cases"]:
        samples = []
        for index in range(args.iterations):
            folder = args.out / (case["name"] + "-" + str(index))
            folder.mkdir(exist_ok=False)
            report = folder / "report.json"
            control, ack = folder / "control.fifo", folder / "ack.fifo"
            os.mkfifo(control)
            os.mkfifo(ack)
            environment = ["env", "RUST_MIN_STACK=33554432",
                "DEADSYNC_PERF_CONTROL=" + str(control.resolve()),
                "DEADSYNC_PERF_ACK=" + str(ack.resolve())]
            if case["kind"] == "compile":
                program = [str(args.compile_runner.resolve()), "--compile", case["simfile"],
                    "--report", str(report.resolve()), "--iterations", "1"]
                if "renderer" in case:
                    program += ["--renderer", case["renderer"]]
                if "chart_index" in case:
                    program += ["--chart-index", str(case["chart_index"])]
            else:
                environment.append("DEADSYNC_SONG_LUA_METRICS=" + str((folder / "allocations.json").resolve()))
                program = [str(args.fixture_runner.resolve()), "--archive", case["archive"],
                    "--report", str(report.resolve())]
                if "through_second" in case:
                    program += ["--through-second", str(case["through_second"])]
            command = shlex.split(args.perf_command) + ["stat", "--no-big-num", "-x", "\t",
                "-e", "cycles:u,instructions:u,task-clock", "--delay=-1", "--control",
                "fifo:" + str(control.resolve()) + "," + str(ack.resolve()),
                "-o", str((folder / "perf.tsv").resolve()), "--",
                "/usr/bin/time", "--quiet", "-f", '{"max_rss_kb":%M}', "-o", str((folder / "rss.json").resolve())]
            command += shlex.split(args.program_prefix) + environment + program
            with (folder / "output.log").open("wb") as log:
                completed = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
            assert completed.returncode == case.get("expected_exit", 0), (case["name"], completed.returncode)
            if case["kind"] == "compile":
                data = json.loads(report.read_bytes())
                measurement = data["samples"][0]
                fingerprint = data["compiled_sha256"]
                items = 1
            else:
                measurement = json.loads((folder / "allocations.json").read_bytes())
                fingerprint = normalized_report_hash(report)
                # Progress is emitted after all frame checks, without parsing
                # a multi-gigabyte JSON report into another object graph.
                match = re.search(rb"(\d+) native frames replayed;", (folder / "output.log").read_bytes())
                assert match, "complete replay frame summary missing"
                items = int(match[1])
            counters = perf_values(folder / "perf.tsv")
            sample = dict(measurement, counters=counters, items=items,
                items_per_second=items / measurement["wall_seconds"],
                items_per_cycle=items / counters["cycles:u"],
                fingerprint=fingerprint, exit_code=completed.returncode,
                max_rss_kb=json.loads((folder / "rss.json").read_bytes())["max_rss_kb"])
            samples.append(sample)
            control.unlink()
            ack.unlink()
            print(case["name"], index, json.dumps(sample), flush=True)
        assert len({s["fingerprint"] for s in samples}) == 1, "output changed across repeated runs"
        if before:
            previous = next(c for c in before["cases"] if c["name"] == case["name"])
            assert previous["samples"][0]["fingerprint"] == samples[0]["fingerprint"], case["name"] + " behavior changed"
        results.append(dict(case, samples=samples))
        (args.out / "results.json").write_text(json.dumps({"cases": results}, indent=2) + "\n")


if __name__ == "__main__":
    main()
