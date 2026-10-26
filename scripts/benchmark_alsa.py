"""Archive real Linux ALSA test binaries and compare hardware counters per operation.

Run in Linux/WSL. Build each stage before editing again. New archives run the
current unit tests and fixtures; a preload fixture supplies two cards and eight
devices backed by real libasound null PCMs. No sound hardware is needed.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ARCHIVES = ROOT / "target/alsa-performance"
SOURCE = ROOT / "crates/deadlib-audio-backend-alsa/src"
WORKLOADS = {"shared_alias": 2_000_000, "shared_hw": 2_000_000,
             "exclusive_hw": 2_000_000, "exclusive_plughw": 2_000_000,
             "enumerate": 20_000, "frames_48000": 40_000_000,
             "frames_44100": 40_000_000, "frames_other": 40_000_000,
             "frames_mixed": 40_000_000, "timing_48000": 1_000_000,
             "timing_44100": 1_000_000, "timing_96000": 1_000_000,
             "rate_capabilities": 200_000, "probe_null": 20_000,
             "enumerate_failed": 100_000, "clock_read_mono": 5_000_000,
             "clock_read_raw": 5_000_000, "clock_map_recent": 40_000_000,
             "clock_map_future": 40_000_000, "clock_map_edges": 40_000_000,
             "timing_zero": 1_000_000, "timing_status_error": 1_000_000,
             "timing_monotonic": 1_000_000}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(name, debug):
    archive = ARCHIVES / name
    archive.mkdir(parents=True, exist_ok=True)
    if (archive / "manifest.json").exists():
        raise FileExistsError(f"Archive already exists; use a fresh stage name: {archive}")
    command = ["cargo", "test", "--locked"]
    if not debug:
        command.append("--release")
    command += ["-p",
               "deadlib-audio-backend-alsa", "--lib", "--no-run", "--message-format=json"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=True)
    (archive / "build.log").write_text(result.stderr)
    messages = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    executable = next(item["executable"] for item in reversed(messages) if item.get("executable"))
    binary = archive / "alsa-tests"
    shutil.copyfile(executable, binary)
    subprocess.run(["cc", "-shared", "-fPIC", "-O2", str(ROOT / "scripts/alsa_fixture.c"),
                    "-o", str(archive / "fixture.so"), "-lasound", "-ldl"], check=True)
    checks = subprocess.run([str(binary), "--test-threads=1"], capture_output=True, text=True, check=True)
    (archive / "correctness.log").write_text(checks.stdout + checks.stderr)
    fixture_logs = []
    fixture_env = {**os.environ, "LD_PRELOAD": str(archive / "fixture.so")}
    for layout in ["regular", "empty", "unmatched_default", "bad_name", "probe_error",
                   "bad_card_info", "bad_card_name", "missing_card_name", "long_names", "empty_names"]:
        result = subprocess.run([str(binary), "fixture_enumeration", "--ignored", "--test-threads=1"],
                                env={**fixture_env, "ALSA_FIXTURE_LAYOUT": layout},
                                capture_output=True, text=True, check=True)
        fixture_logs.append(f"layout={layout}\n{result.stdout}{result.stderr}")
    for delay in ["-1", "0", "240", "4294967295"]:
        for failed in [False, True]:
            for monotonic in [False, True]:
                env = {**fixture_env, "ALSA_FIXTURE_DELAY": delay}
                if failed:
                    env["ALSA_FIXTURE_STATUS_ERROR"] = "1"
                if monotonic:
                    env["ALSA_FIXTURE_MONOTONIC"] = "1"
                result = subprocess.run([str(binary), "fixture_timing", "--ignored", "--test-threads=1"],
                                        env=env, capture_output=True, text=True, check=True)
                fixture_logs.append(f"delay={delay} status_error={failed} monotonic={monotonic}\n{result.stdout}{result.stderr}")
    matrix = subprocess.run([str(binary), "fixture_rate_matrix", "--ignored", "--test-threads=1"],
                            env={**fixture_env, "ALSA_FIXTURE_RATE_MATRIX": "1"},
                            capture_output=True, text=True, check=True)
    fixture_logs.append(f"rate_matrix=2048\n{matrix.stdout}{matrix.stderr}")
    for edge in ["min_error", "max_error", "reject_min", "reject_edges", "min_zero",
                 "min_max_reversed", "duplicate_bounds"]:
        for mask in ["0", "1"]:
            result = subprocess.run([str(binary), "fixture_rate_edges", "--ignored", "--test-threads=1"],
                                    env={**fixture_env, "ALSA_FIXTURE_RATE_EDGE": edge,
                                         "ALSA_FIXTURE_RATE_MASK": mask},
                                    capture_output=True, text=True, check=True)
            fixture_logs.append(f"rate_edge={edge}/{mask}\n{result.stdout}{result.stderr}")
    for monotonic in [False, True]:
        env = {**fixture_env, "ALSA_FIXTURE_DELAY": "240"}
        if monotonic:
            env["ALSA_FIXTURE_MONOTONIC"] = "1"
        result = subprocess.run([str(binary), "fixture_clock_health", "--ignored", "--test-threads=1"],
                                env=env, capture_output=True, text=True, check=True)
        fixture_logs.append(f"clock_health=9 extremes monotonic={monotonic}\n{result.stdout}{result.stderr}")
    (archive / "fixture.log").write_text("\n".join(fixture_logs))
    for source in [SOURCE / "lib.rs", SOURCE / "perf_tests.rs", ROOT / "scripts/alsa_fixture.c"]:
        shutil.copyfile(source, archive / source.name)
    manifest = dict(command=command, rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    target_dir=os.environ.get("CARGO_TARGET_DIR"),
                    source_sha256=digest(SOURCE / "lib.rs"), binary_sha256=digest(binary),
                    tests_sha256=digest(SOURCE / "perf_tests.rs"),
                    fixture_sha256=digest(ROOT / "scripts/alsa_fixture.c"),
                    cpu=Path("/proc/cpuinfo").read_text().split("\n\n")[0],
                    alsa=subprocess.check_output(["pkg-config", "--modversion", "alsa"], text=True).strip())
    (archive / "manifest.json").write_text(json.dumps(manifest, indent=2))
    print(checks.stdout, f"{len(fixture_logs) - 1} fixture scenarios and 2048 rate masks passed for {name}", flush=True)


def compare(names, output, runs, workloads, fixture_stage):
    rows = []
    logdir = ARCHIVES / "runs" / output.stem
    logdir.mkdir(parents=True, exist_ok=True)
    profiles = set()
    for name in names:
        archive = ARCHIVES / name
        manifest = json.loads((archive / "manifest.json").read_text())
        assert digest(archive / "alsa-tests") == manifest["binary_sha256"]
        profiles.add("--release" in manifest["command"])
    if len(profiles) != 1:
        raise ValueError("Compare binaries compiled with the same profile")
    for run in range(runs):
        for workload in workloads:
            for name in names[run % len(names):] + names[:run % len(names)]:
                archive = ARCHIVES / name
                prefix = logdir / f"{run}-{workload}-{name}"
                fixture = ARCHIVES / fixture_stage / "fixture.so" if fixture_stage else archive / "fixture.so"
                aliases = {"enumerate_failed": "enumerate", "timing_zero": "timing_48000",
                           "timing_status_error": "timing_48000", "timing_monotonic": "timing_48000"}
                env = {**os.environ, "LC_ALL": "C", "LD_PRELOAD": str(fixture),
                       "ALSA_PERF_WORKLOAD": aliases.get(workload, workload),
                       "ALSA_PERF_ITERATIONS": str(WORKLOADS[workload])}
                if workload.startswith("timing_"):
                    env["ALSA_FIXTURE_DELAY"] = "240"
                if workload in ["rate_capabilities", "probe_null"] or workload.startswith("clock_"):
                    env.pop("LD_PRELOAD", None)
                if workload == "enumerate_failed":
                    env["ALSA_FIXTURE_LAYOUT"] = "probe_error"
                if workload == "timing_zero":
                    env["ALSA_FIXTURE_DELAY"] = "0"
                if workload == "timing_status_error":
                    env["ALSA_FIXTURE_STATUS_ERROR"] = "1"
                if workload == "timing_monotonic":
                    env["ALSA_FIXTURE_MONOTONIC"] = "1"
                # One allowed CPU, with userspace cycles/instructions and scheduled CPU time.
                cpu = min(os.sched_getaffinity(0))
                command = ["perf", "stat", "-x", ";", "-e", "cycles:u,instructions:u,task-clock",
                           "--", "taskset", "-c", str(cpu), str(archive / "alsa-tests"),
                           "benchmark_alsa", "--ignored", "--nocapture", "--test-threads=1"]
                result = subprocess.run(command, env=env, capture_output=True, text=True, check=True)
                prefix.with_suffix(".out").write_text(result.stdout)
                prefix.with_suffix(".perf").write_text(result.stderr)
                counters = {}
                for line in result.stderr.splitlines():
                    fields = line.split(";")
                    if len(fields) > 2 and fields[2] == "task-clock:u":
                        fields[2] = "task-clock"
                    if len(fields) > 2 and fields[2] in ["cycles:u", "instructions:u", "task-clock"]:
                        counters[fields[2]] = float(fields[0])
                counts = re.search(r"allocations=(\d+) reallocations=(\d+) requested_bytes=(\d+)", result.stdout)
                timed = re.search(r"operations=(\d+) elapsed_ns=(\d+)", result.stdout)
                operations, elapsed = map(int, timed.groups())
                assert counters["cycles:u"] > 0 and operations == WORKLOADS[workload]
                rows.append(dict(run=run, stage=name, workload=workload, operations=operations,
                                 cycles_per_op=counters["cycles:u"] / operations,
                                 instructions_per_op=counters["instructions:u"] / operations,
                                 cpu_ns_per_op=counters["task-clock"] * 1_000_000 / operations,
                                 ns_per_op=elapsed / operations,
                                 **dict(zip(["allocations", "reallocations", "requested_bytes"],
                                            map(int, counts.groups())))))
        print(f"run {run + 1}/{runs} complete", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for workload in workloads:
        cycles = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == name and row["workload"] == workload) for name in names]
        print(f"{workload}: {' -> '.join(f'{value:.2f}' for value in cycles)} cycles/op; "
              f"{cycles[0] / cycles[-1]:.3f}x CPU throughput")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build")
    parser.add_argument("--debug", action="store_true", help="build/check a debug archive")
    parser.add_argument("--compare", nargs="+")
    parser.add_argument("--workloads", nargs="+", choices=list(WORKLOADS), default=list(WORKLOADS))
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--fixture-stage", help="load one identical fixture into every compared binary")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.build:
        build(args.build, args.debug)
    if args.compare:
        if args.output is None or args.runs < 1:
            parser.error("--compare requires --output and positive --runs")
        compare(args.compare, args.output, args.runs, args.workloads, args.fixture_stage)


if __name__ == "__main__":
    main()
