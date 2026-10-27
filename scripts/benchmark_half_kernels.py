"""Compare archived binary16 production benchmarks after each optimization."""
import argparse
import csv
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    versions = ["before", "encode", "decode", "after"]
    for version in versions:
        parser.add_argument(f"--{version}", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--logs", type=Path, default=Path("target/half-kernels/runs"))
    parser.add_argument("--runs", type=int, default=4)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.logs.mkdir(parents=True, exist_ok=True)
    pattern = re.compile(r"(half/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn_pattern = re.compile(r"(half/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    for run in range(args.runs):
        # Each stage occupies every position once over four fresh process runs.
        order = versions[run % 4:] + versions[:run % 4]
        for version in order:
            result = subprocess.run(
                [str(getattr(args, version).resolve()), "half_kernels::benchmark_",
                 "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (args.logs / f"{run}-{version}.log").write_text(result.stdout, encoding="utf-8")
            matches = pattern.findall(result.stdout)
            churn = {label: dict(re.findall(r"(\w+): (\d+)", values))
                     for label, values in churn_pattern.findall(result.stdout)}
            if len(matches) != 14 or len(churn) != 14:
                raise ValueError("expected fourteen measurements and allocation probes")
            for label, cpu, ns in matches:
                rows.append(dict(run=run, version=version, workload=label,
                                 cycles_per_op=float(cpu), ns_per_op=float(ns), **churn[label]))
            print(f"run {run + 1}/{args.runs}: {version}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        medians = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["version"] == version and row["workload"] == label)
                   for version in versions]
        print(f"{label}: " + " -> ".join(f"{value:.2f}" for value in medians)
              + f" cycles/op; total {medians[0] / medians[-1]:.3f}x")


if __name__ == "__main__":
    main()
