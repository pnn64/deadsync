"""Build actual score-crate stages, then compare frozen binaries in rotating order."""
import argparse
import csv
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import shutil
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = ROOT / "target/score-kernels"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def build(stage):
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    command = ["cargo", "test", "-p", "deadsync-score", "--release",
               "--test", "score_kernels", "--no-run", "--message-format=json", "--locked"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    (out / "build.log").write_text(result.stdout + result.stderr, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(result.stderr + result.stdout)
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    binary = next(Path(a["executable"]) for a in artifacts
                  if a.get("target", {}).get("name") == "score_kernels" and a.get("executable"))
    saved = out / binary.name
    shutil.copyfile(binary, saved)
    correctness = subprocess.run([str(saved), "--test-threads=1"], check=True,
                                capture_output=True, text=True, cwd=ROOT)
    (out / "correctness.log").write_text(correctness.stdout, encoding="utf-8")
    paths = list((ROOT / "crates/deadsync-score").rglob("*.rs"))
    paths += [ROOT / p for p in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml",
                                "tests/support/kernel_bench.rs", "tests/support/perf.rs",
                                "crates/deadsync-score/Cargo.toml"]]
    hashes = {}
    for path in paths:
        relative = path.relative_to(ROOT)
        target = out / "source" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
        hashes[str(relative)] = digest(path)
    manifest = dict(command=command, rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    binary=saved.name, binary_sha256=digest(saved), sources=hashes)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(correctness.stdout, flush=True)
    print("Archived " + stage, flush=True)

def compare(stages, output, runs):
    manifests = {s: json.loads((ARCHIVE / s / "manifest.json").read_text(encoding="utf-8"))
                 for s in stages}
    # Inputs, instrumentation and compiler settings must be identical.
    for path in ["crates/deadsync-score/tests/score_kernels.rs",
                 "crates/deadsync-score/tests/support/score_kernels_original.rs",
                 "tests/support/kernel_bench.rs", "tests/support/perf.rs",
                 "Cargo.toml", "Cargo.lock", ".cargo/config.toml"]:
        values = {m["sources"][str(Path(path))] for m in manifests.values()}
        if len(values) != 1:
            raise ValueError("Different benchmark configuration: " + path)
    if len({m["rustc"] for m in manifests.values()}) != 1:
        raise ValueError("Different compilers")
    rows = []
    logs = ARCHIVE / "runs" / output.stem
    logs.mkdir(parents=True, exist_ok=True)
    for run in range(runs):
        order = stages[run % len(stages):] + stages[:run % len(stages)]
        for stage in order:
            manifest = manifests[stage]
            binary = ARCHIVE / stage / manifest["binary"]
            if digest(binary) != manifest["binary_sha256"]:
                raise ValueError("Archived binary changed: " + stage)
            result = subprocess.run([str(binary), "benchmark_score_kernels", "--ignored",
                                     "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True, cwd=ROOT)
            (logs / f"{run}-{stage}.log").write_text(result.stdout, encoding="utf-8")
            timings = re.findall(r"([\w/]+): cycles/op ([\d.]+); ns/op ([\d.]+)", result.stdout)
            allocations = dict(re.findall(r"([\w/]+): allocation Churn \{ ([^}]+) \}", result.stdout))
            if len(timings) != 16 or set(allocations) != {t[0] for t in timings}:
                raise ValueError("Missing workload")
            for label, cycles, ns in timings:
                counts = dict(re.findall(r"(\w+): (\d+)", allocations[label]))
                rows.append(dict(run=run, stage=stage, workload=label,
                                 cycles_per_op=float(cycles), ns_per_op=float(ns), **counts))
            print(f"run {run + 1}/{runs}: {stage}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    output.with_suffix(".manifest.json").write_text(json.dumps(dict(
        platform=platform.platform(), processor=os.environ.get("PROCESSOR_IDENTIFIER"),
        runs=runs, stages=manifests), indent=2), encoding="utf-8")
    for label in sorted({r["workload"] for r in rows}):
        values = [statistics.median(r["cycles_per_op"] for r in rows
                                    if r["stage"] == s and r["workload"] == label) for s in stages]
        print(f"{label}: " + " -> ".join(f"{v:.2f}" for v in values)
              + f" cycles/op; {values[0] / values[-1]:.3f}x CPU throughput")


def paired(output, runs):
    command = ["cargo", "test", "-p", "deadsync-score", "--release", "--locked",
               "--test", "score_kernels_paired", "--no-run", "--message-format=json"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=True)
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    binary = next(Path(a["executable"]) for a in artifacts
                  if a.get("target", {}).get("name") == "score_kernels_paired" and a.get("executable"))
    archive = ARCHIVE / "paired"
    archive.mkdir(parents=True, exist_ok=True)
    saved = archive / binary.name
    shutil.copyfile(binary, saved)
    paths = [ROOT / p for p in ["crates/deadsync-score/src/ghost.rs",
             "crates/deadsync-score/src/itl.rs", "crates/deadsync-score/src/lib.rs",
             "crates/deadsync-score/tests/score_kernels_paired.rs",
             "crates/deadsync-score/tests/support/score_kernels_original.rs",
             "tests/support/kernel_bench.rs", "Cargo.toml", "Cargo.lock", ".cargo/config.toml"]]
    hashes = {}
    for path in paths:
        relative = path.relative_to(ROOT)
        target = archive / "source" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
        hashes[str(relative)] = digest(path)
    manifest = dict(command=command, rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    binary_sha256=digest(saved), sources=hashes, runs=runs,
                    platform=platform.platform(), processor=os.environ.get("PROCESSOR_IDENTIFIER"))
    rows = []
    for run in range(runs):
        if digest(saved) != manifest["binary_sha256"]:
            raise ValueError("Paired binary changed")
        result = subprocess.run([str(saved), "benchmark_paired_score_kernels", "--ignored",
                                 "--nocapture", "--test-threads=1"], cwd=ROOT,
                                capture_output=True, text=True, check=True)
        (archive / f"run-{run}.log").write_text(result.stdout, encoding="utf-8")
        timings = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", result.stdout)
        if len(timings) != 16:
            raise ValueError("Missing paired workload")
        for label, old_cpu, new_cpu, old_ns, new_ns in timings:
            rows.append(dict(run=run, workload=label, original_cycles=float(old_cpu),
                             current_cycles=float(new_cpu), original_ns=float(old_ns), current_ns=float(new_ns)))
        print(f"paired run {run + 1}/{runs}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    output.with_suffix(".manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    for label in sorted({r["workload"] for r in rows}):
        old, new = [statistics.median(r[key] for r in rows if r["workload"] == label)
                    for key in ["original_cycles", "current_cycles"]]
        print(f"{label}: {old:.2f} -> {new:.2f} cycles/op; {old / new:.3f}x CPU throughput")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build")
    parser.add_argument("--paired", action="store_true")
    parser.add_argument("--compare", nargs="+")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.build:
        build(args.build)
    if args.paired:
        if not args.output or args.runs < 1:
            parser.error("--paired requires --output and positive --runs")
        paired(args.output, args.runs)
    if args.compare:
        if not args.output or args.runs < 1:
            parser.error("--compare requires --output and positive --runs")
        compare(args.compare, args.output, args.runs)

if __name__ == "__main__":
    main()
