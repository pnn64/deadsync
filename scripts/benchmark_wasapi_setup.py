"""Measure archived WASAPI production-test binaries in rotating process order."""
import argparse
import csv
import hashlib
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", action="append", required=True, help="stage=executable")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--test", default="performance_tests::benchmark_setup")
    parser.add_argument("--workloads", type=int, default=6)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    archives = [value.split("=", 1) for value in args.archive]
    timing = re.compile(r"(wasapi/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"(wasapi/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    logs = args.output.with_suffix(".runs")
    logs.mkdir(parents=True, exist_ok=True)
    hashes = {stage: hashlib.sha256(Path(binary).read_bytes()).hexdigest()
              for stage, binary in archives}
    rows = []
    for run in range(args.runs):
        for stage, binary in archives[run % len(archives):] + archives[:run % len(archives)]:
            result = subprocess.run(
                [str(Path(binary).resolve()), args.test, "--ignored",
                 "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (logs / f"{run}-{stage}.log").write_text(result.stdout + result.stderr, encoding="utf-8")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != args.workloads or len(counts) != args.workloads:
                raise ValueError(f"expected {args.workloads} timing and allocation measurements")
            for label, cycles, ns in matches:
                rows.append(dict(run=run, stage=stage, workload=label,
                                 cycles_per_op=float(cycles), ns_per_op=float(ns),
                                 **counts[label], sha256=hashes[stage]))
            print(f"run {run + 1}: {stage}", flush=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        medians = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == stage and row["workload"] == label)
                   for stage, _ in archives]
        print(f"{label}: " + " -> ".join(f"{value:.2f}" for value in medians)
              + f" cycles/op; {medians[0] / medians[-1]:.3f}x CPU throughput")


if __name__ == "__main__":
    main()
