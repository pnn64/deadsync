"""Compare archived production-source test binaries in alternating process order.

Build texture_pipeline and panned_mixer with identical release options in both
directories. Inputs and allocation probes stay outside the CPU-cycle batches.
"""
import argparse
import csv
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--logs", type=Path, default=Path("target/texture-pipeline/runs"))
    parser.add_argument("--runs", type=int, default=4)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.logs.mkdir(parents=True, exist_ok=True)
    pattern = re.compile(r"((?:alpha|yuv|panned)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn_pattern = re.compile(r"((?:alpha|yuv|panned)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            for name, expected in [("texture_pipeline", 12), ("panned_mixer", 7)]:
                result = subprocess.run(
                    [str((getattr(args, version) / f"{name}.exe").resolve()),
                     "pipeline::benchmark_" if name == "texture_pipeline" else "kernel::benchmark_",
                     "--ignored", "--nocapture", "--test-threads=1"],
                    capture_output=True, text=True, encoding="utf-8", check=True,
                )
                (args.logs / f"{run}-{version}-{name}.log").write_text(result.stdout, encoding="utf-8")
                matches = pattern.findall(result.stdout)
                churn = {label: dict(re.findall(r"(\w+): (\d+)", values))
                         for label, values in churn_pattern.findall(result.stdout)}
                if len(matches) != expected or len(churn) != expected:
                    raise ValueError(f"{name}: expected {expected} measurements and allocation probes")
                for label, cpu, ns in matches:
                    rows.append(dict(run=run, version=version, workload=label,
                                     cycles_per_op=float(cpu), ns_per_op=float(ns), **churn[label]))
                print(f"run {run + 1}/{args.runs}: {version} {name}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        medians = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["version"] == version and row["workload"] == label)
                   for version in ["before", "after"]]
        print(f"{label}: {medians[0]:.2f} -> {medians[1]:.2f} cycles/op; {medians[0] / medians[1]:.3f}x")


if __name__ == "__main__":
    main()
