
"""Archive and measure score import kernels with paired frozen production functions."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = ROOT / "target/score-import-kernels"

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(stage, runs, messages=False):
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    target = "score_import_kernels"
    benchmark = "benchmark_pack_messages" if messages else "benchmark_import_kernels"
    command = ["cargo", "test", "-p", "deadsync-score", "--release", "--locked",
               "--test", target, "--no-run", "--message-format=json"]
    build = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    (out / "build.log").write_text(build.stdout + build.stderr, encoding="utf-8")
    if build.returncode:
        raise RuntimeError(build.stderr + build.stdout)
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    binary = next(Path(a["executable"]) for a in artifacts
                  if a.get("target", {}).get("name") == target and a.get("executable"))
    saved = out / binary.name
    shutil.copyfile(binary, saved)
    check = subprocess.run([str(saved), "--test-threads=1"], cwd=ROOT, check=True,
                           capture_output=True, text=True, encoding="utf-8")
    (out / "correctness.log").write_text(check.stdout, encoding="utf-8")
    paths = list((ROOT / "crates/deadsync-score").rglob("*.rs"))
    paths += [ROOT / p for p in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml",
                                "crates/deadsync-score/Cargo.toml", "tests/support/kernel_bench.rs",
                                "tests/support/perf.rs", "scripts/benchmark_score_import.py"]]
    hashes = {}
    for path in paths:
        relative = path.relative_to(ROOT)
        source_target = out / "source" / relative
        source_target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, source_target)
        hashes[str(relative)] = sha(path)
    manifest = dict(stage=stage, command=command, runs=runs,
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True, encoding="utf-8"),
                    platform=platform.platform(), cpu=os.environ.get("PROCESSOR_IDENTIFIER"),
                    binary=saved.name, binary_sha256=sha(saved), sources=hashes)
    rows = []
    for number in range(runs):
        result = subprocess.run([str(saved), benchmark, "--ignored", "--nocapture",
                                 "--test-threads=1"], cwd=ROOT, check=True, capture_output=True,
                                text=True, encoding="utf-8")
        (out / f"run-{number}.log").write_text(result.stdout, encoding="utf-8")
        timings = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", result.stdout)
        allocations = dict(re.findall(r"([\w/]+): allocation Churn \{ ([^}]+) \}", result.stdout))
        if len(timings) != 6:
            raise ValueError("Missing workload: " + result.stdout)
        for label, old_cycles, new_cycles, old_ns, new_ns in timings:
            for variant, cpu, ns in [("original", old_cycles, old_ns), ("current", new_cycles, new_ns)]:
                counts = dict(re.findall(r"(\w+): (\d+)", allocations[label + "/" + variant]))
                rows.append(dict(run=number, workload=label, variant=variant, cycles_per_op=cpu,
                                 ns_per_op=ns, **counts))
        print(f"{stage}: run {number + 1}/{runs}", flush=True)
    report = ROOT / f"docs/performance/2026-11-02-score-import-{stage}.tsv"
    report.parent.mkdir(parents=True, exist_ok=True)
    with report.open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    report.with_suffix(".manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    for label in sorted({row["workload"] for row in rows}):
        values = [statistics.median(float(row["cycles_per_op"]) for row in rows
                  if row["workload"] == label and row["variant"] == variant)
                  for variant in ["original", "current"]]
        ratios = []
        for number in range(runs):
            pair = {row["variant"]: float(row["cycles_per_op"]) for row in rows
                    if row["workload"] == label and row["run"] == number}
            ratios.append(pair["original"] / pair["current"])
        print(f"{label}: median cycles {values[0]:.2f} -> {values[1]:.2f}; "
              f"median paired throughput {statistics.median(ratios):.3f}x", flush=True)

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("stage")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--messages", action="store_true")
    args = parser.parse_args()
    run(args.stage, args.runs, args.messages)
