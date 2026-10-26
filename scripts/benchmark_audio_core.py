"""Archive actual callback test executables and compare CPU cycles and allocations."""
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
ARCHIVES = ROOT / "target/audio-core-kernels"
SOURCE = ROOT / "crates/deadlib-audio-core"


def build(stage, target):
    archive = ARCHIVES / stage
    archive.mkdir(parents=True, exist_ok=True)
    sources = {path.relative_to(ROOT).as_posix(): path.read_bytes()
               for path in [*SOURCE.rglob("*.rs"), SOURCE / "Cargo.toml", ROOT / "Cargo.toml",
                            ROOT / "Cargo.lock", ROOT / ".cargo/config.toml",
                            ROOT / "tests/support/kernel_bench.rs", ROOT / "tests/support/perf.rs"]}
    command = ["cargo", "test", "-p", "deadlib-audio-core", "--release", "--test",
               target, "--no-run", "--message-format=json"]
    print(f"building {stage} ({target})", flush=True)
    build_env = {**os.environ, "CARGO_TARGET_DIR": str(ARCHIVES / "build")}
    result = subprocess.run(command, cwd=ROOT, env=build_env, capture_output=True, text=True)
    if result.returncode:
        print(result.stdout, result.stderr)
        result.check_returncode()
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    binary = next(Path(item["executable"]) for item in artifacts
                  if item.get("executable") and item.get("target", {}).get("name") == target)
    for path, data in sources.items():
        if (ROOT / path).read_bytes() != data:
            raise RuntimeError(f"Source changed during build: {path}")
        snapshot = archive / path
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        snapshot.write_bytes(data)
    executable = archive / "callback.exe"
    executable.write_bytes(binary.read_bytes())
    result = subprocess.run([str(executable), "--test-threads=1"], capture_output=True, text=True)
    (archive / "correctness.log").write_text(result.stdout, encoding="utf-8")
    if result.returncode:
        print(result.stdout, result.stderr)
        result.check_returncode()
    manifest = dict(command=command, target=target, cargo_target_dir=build_env["CARGO_TARGET_DIR"],
                    head=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    source_sha256={path: hashlib.sha256(data).hexdigest() for path, data in sources.items()},
                    binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest())
    (archive / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(result.stdout, flush=True)


def compare(stages, output, runs, bench_filter):
    timing = re.compile(r"((?:callback|pan)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn = re.compile(r"((?:callback|pan)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    logs = ARCHIVES / "runs" / output.stem
    logs.mkdir(parents=True, exist_ok=True)
    for run in range(runs):
        for stage in stages[run % len(stages):] + stages[:run % len(stages)]:
            manifest = json.loads((ARCHIVES / stage / "manifest.json").read_text())
            target = manifest.get("target", "callback_kernels")
            benchmark = bench_filter or ("benchmark_panned_mix" if target == "panned_kernel" else "benchmark_callback_kernels")
            result = subprocess.run([str(ARCHIVES / stage / "callback.exe"), benchmark,
                                     "--ignored", "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True)
            (logs / f"{run}-{stage}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in churn.findall(result.stdout)}
            measurements = timing.findall(result.stdout)
            expected = (36,) if target == "panned_kernel" else (16, 32, 40)
            if bench_filter:
                expected = (8 if "mode_parser" in bench_filter else 36 if "panned_mix" in bench_filter else 16,)
            if len(measurements) not in expected or len(counts) != len(measurements):
                raise ValueError(f"Expected {expected} workloads: {stage}")
            for label, cycles, ns in measurements:
                rows.append(dict(run=run, stage=stage, workload=label, cycles_per_op=float(cycles),
                                 ns_per_op=float(ns), **counts[label]))
            print(f"run {run + 1}/{runs}: {stage}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    output.with_suffix(".manifest.json").write_text(json.dumps(dict(
        platform=platform.platform(), processor=os.environ.get("PROCESSOR_IDENTIFIER"), benchmark=bench_filter,
        stages={stage: json.loads((ARCHIVES / stage / "manifest.json").read_text()) for stage in stages},
    ), indent=2), encoding="utf-8")
    common = set.intersection(*(set(row["workload"] for row in rows if row["stage"] == stage)
                                for stage in stages))
    for label in sorted(common):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == stage and row["workload"] == label)
                  for stage in stages]
        print(f"{label}: {' -> '.join(f'{value:.2f}' for value in values)} cycles/op; "
              f"{values[0] / values[-1]:.3f}x CPU throughput")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build")
    parser.add_argument("--target", choices=["callback_kernels", "panned_kernel"], default="callback_kernels")
    parser.add_argument("--compare", nargs="+")
    parser.add_argument("--benchmark", choices=["render::kernels::benchmark_callback_kernels",
                        "quantization::benchmark_callback_kernels_quantization",
                        "mode_parser::benchmark_callback_kernels_mode_parser", "benchmark_panned_mix"])
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.build:
        build(args.build, args.target)
    if args.compare:
        if not args.output or args.runs < 1:
            parser.error("--compare requires --output and positive --runs")
        compare(args.compare, args.output, args.runs, args.benchmark)


if __name__ == "__main__":
    main()
