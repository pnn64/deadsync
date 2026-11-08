"""Archive actual audio unit-test binaries; compare lead-in CPU cycles and churn."""
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
ARCHIVES = ROOT / "target/audio-lead-in"
SOURCES = ["crates/deadlib-audio/src/stream.rs",
           "crates/deadlib-audio/src/stream/perf_tests.rs",
           "crates/deadlib-audio/src/stream/silence_perf_tests.rs",
           "tests/support/kernel_bench.rs", "tests/support/perf.rs",
           "crates/deadlib-audio/src/stream/stretch.rs",
           "crates/deadlib-audio/src/stream/stretch/perf_tests.rs"]


def build(name):
    archive = ARCHIVES / name
    archive.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "CARGO_TARGET_DIR": str(ARCHIVES / "cargo"),
           "CARGO_PROFILE_RELEASE_DEBUG": "1", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1"}
    command = ["cargo", "test", "--release", "-p", "deadlib-audio", "--lib", "--no-run",
               "--message-format=json"]
    source_hashes = {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in SOURCES}
    for index, path in enumerate(SOURCES):
        (archive / f"source-{index}.rs").write_bytes((ROOT / path).read_bytes())
    result = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True, check=True)
    artifacts = [row for line in result.stdout.splitlines() if line.startswith("{")
                 for row in [json.loads(line)] if row.get("reason") == "compiler-artifact"
                 and row["target"]["name"] == "deadlib_audio" and row.get("executable")]
    if len(artifacts) != 1:
        raise ValueError("Expected one deadlib-audio test executable")
    binary = archive / "lead-in.exe"
    shutil.copy2(artifacts[0]["executable"], binary)
    check = subprocess.run([str(binary), "--test-threads=1"], capture_output=True, text=True, check=True)
    (archive / "correctness.log").write_text(check.stdout, encoding="utf-8")
    metadata = dict(command=command,
                    environment={key: env[key] for key in (
                        "CARGO_TARGET_DIR", "CARGO_PROFILE_RELEASE_DEBUG",
                        "CARGO_PROFILE_RELEASE_CODEGEN_UNITS")},
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    source_sha256=source_hashes, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                    binary_bytes=binary.stat().st_size)
    (archive / "manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    print(check.stdout, flush=True)


def compare(names, runs, output):
    timing = re.compile(r"(lead-in/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn = re.compile(r"(lead-in/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    logs = ARCHIVES / "runs" / output.stem
    logs.mkdir(parents=True, exist_ok=True)
    for run in range(runs):
        order = names[run % len(names):] + names[:run % len(names)]
        for name in order:
            result = subprocess.run([str(ARCHIVES / name / "lead-in.exe"), "benchmark_lead_in",
                                     "--ignored", "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True)
            (logs / f"{run}-{name}.log").write_text(result.stdout, encoding="utf-8")
            measurements = timing.findall(result.stdout)
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in churn.findall(result.stdout)}
            if len(measurements) != 16 or len(counts) != 16:
                raise ValueError(f"Expected 16 timing and churn measurements: {name}")
            for label, cycles, ns in measurements:
                rows.append(dict(run=run, stage=name, workload=label, cycles_per_op=float(cycles),
                                 ns_per_op=float(ns), **counts[label]))
            print(f"run {run + 1}/{runs}: {name}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    metadata = {name: json.loads((ARCHIVES / name / "manifest.json").read_text()) for name in names}
    output.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    for label in sorted({row["workload"] for row in rows}):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == name and row["workload"] == label) for name in names]
        print(f"{label}: {' -> '.join(f'{value:.2f}' for value in values)} cycles/op; "
              f"{values[0] / values[-1]:.3f}x CPU throughput")


def verify(names, output):
    records = []
    expected = None
    for name in names:
        pcm = ARCHIVES / name / "capture.bin"
        result = subprocess.run([str(ARCHIVES / name / "lead-in.exe"), "dump_lead_in", "--ignored",
                                 "--test-threads=1"], capture_output=True, text=True, check=True,
                                env={**os.environ, "LEAD_IN_CAPTURE": str(pcm)})
        data = pcm.read_bytes()
        if expected is None:
            expected = data
        if data != expected:
            raise ValueError(f"PCM samples or transport timing changed: {name}")
        records.append(dict(stage=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                            manifest=json.loads((ARCHIVES / name / "manifest.json").read_text()),
                            stdout=result.stdout))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(records, indent=2), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build")
    parser.add_argument("--compare", nargs="+")
    parser.add_argument("--verify", nargs="+")
    parser.add_argument("--runs", type=int, default=8)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.build:
        build(args.build)
    if args.compare or args.verify:
        if not args.output or args.runs < 1:
            parser.error("comparison requires --output and positive --runs")
        if args.compare:
            compare(args.compare, args.runs, args.output)
        if args.verify:
            verify(args.verify, args.output)


if __name__ == "__main__":
    main()
