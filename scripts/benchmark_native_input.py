"""Archive production input kernel tests and compare cycles and allocations."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ARCHIVES = ROOT / "target/native-input-kernels"
SOURCE = ROOT / "crates/deadlib-input-native"


def build(stage):
    archive = ARCHIVES / stage
    archive.mkdir(parents=True, exist_ok=False)
    paths = [*SOURCE.rglob("*.rs"), SOURCE / "Cargo.toml", ROOT / "Cargo.lock",
             ROOT / "Cargo.toml", ROOT / ".cargo/config.toml",
             ROOT / "tests/support/kernel_bench.rs", ROOT / "tests/support/perf.rs"]
    sources = {path.relative_to(ROOT).as_posix(): path.read_bytes() for path in paths}
    command = ["cargo", "test", "-p", "deadlib-input-native", "--release", "--locked",
               "--test", "native_kernels", "--no-run", "--message-format=json"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    (archive / "build.log").write_text(result.stderr, encoding="utf-8")
    result.check_returncode()
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    binary = next(Path(item["executable"]) for item in artifacts
                  if item.get("executable") and item.get("target", {}).get("name") == "native_kernels")
    for path, data in sources.items():
        if (ROOT / path).read_bytes() != data:
            raise RuntimeError(f"Source changed during build: {path}")
        snapshot = archive / path
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        snapshot.write_bytes(data)
    executable = archive / "native.exe"
    executable.write_bytes(binary.read_bytes())
    result = subprocess.run([str(executable), "--test-threads=1"], capture_output=True, text=True)
    (archive / "correctness.log").write_text(result.stdout + result.stderr, encoding="utf-8")
    result.check_returncode()
    manifest = dict(command=command, rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    source_sha256={path: hashlib.sha256(data).hexdigest() for path, data in sources.items()},
                    binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest())
    (archive / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(result.stdout, flush=True)


def compare(stages, output, runs, group):
    timing = re.compile(r"(native/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn = re.compile(r"(native/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    logs = ARCHIVES / "runs" / output.stem
    logs.mkdir(parents=True, exist_ok=False)
    manifests = {s: json.loads((ARCHIVES / s / "manifest.json").read_text()) for s in stages}
    for run in range(runs):
        for stage in stages[run % len(stages):] + stages[:run % len(stages)]:
            executable = ARCHIVES / stage / "native.exe"
            if hashlib.sha256(executable.read_bytes()).hexdigest() != manifests[stage]["binary_sha256"]:
                raise RuntimeError(f"Archived executable changed: {stage}")
            result = subprocess.run([str(executable), f"benchmark_{group}", "--ignored",
                                     "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True)
            (logs / f"{run}-{stage}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in churn.findall(result.stdout)}
            measurements = timing.findall(result.stdout)
            expected = dict(uuid=6, wgi=5, hid=20)[group]
            if len(measurements) != expected or len(counts) != expected:
                raise ValueError(f"Expected {expected} workloads: {stage}")
            for label, cycles, ns in measurements:
                rows.append(dict(run=run, stage=stage, workload=label, cycles_per_op=float(cycles),
                                 ns_per_op=float(ns), **counts[label], sha256=manifests[stage]["binary_sha256"]))
            print(f"run {run + 1}/{runs}: {stage}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    output.with_suffix(".manifest.json").write_text(json.dumps(dict(
        platform=platform.platform(), processor=os.environ.get("PROCESSOR_IDENTIFIER"), stages=manifests,
    ), indent=2), encoding="utf-8")
    for label in sorted({r["workload"] for r in rows}):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == stage and row["workload"] == label)
                  for stage in stages]
        print(f"{label}: {' -> '.join(f'{value:.2f}' for value in values)} cycles/op; "
              f"{values[0] / values[-1]:.3f}x CPU throughput")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build")
    parser.add_argument("--compare", nargs="+")
    parser.add_argument("--group", choices=["uuid", "wgi", "hid"])
    parser.add_argument("--runs", type=int, default=8)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.build:
        build(args.build)
    if args.compare:
        if not args.output or not args.group or args.runs < 1:
            parser.error("--compare requires --output, --group and positive --runs")
        compare(args.compare, args.output, args.runs, args.group)


if __name__ == "__main__":
    main()
