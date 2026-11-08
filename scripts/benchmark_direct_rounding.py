"""Measure archived production binaries; rotate process order and retain churn probes."""
import argparse
import csv
from pathlib import Path
import re
import statistics
import subprocess

TARGETS = {
    "wav": ("wav_float_kernel", "benchmark_wav_float", 18),
    "key": ("assets", "benchmark_key_kernel", 9),
    "resampler": ("resampler_output_kernel", "benchmark_resampler_output", 20),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", action="append", required=True, metavar="NAME=DIR")
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    archives = dict(item.split("=", 1) for item in args.archive)
    if args.runs < 1:
        parser.error("--runs must be positive")
    logs = Path("target/direct-rounding/runs") / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    name, test, expected = TARGETS[args.target]
    pattern = re.compile(r"((?:wav|key|resampler)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn_pattern = re.compile(r"((?:wav|key|resampler)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    for run in range(args.runs):
        order = list(archives)
        order = order[run % len(order):] + order[:run % len(order)]
        for version in order:
            result = subprocess.run(
                [str((Path(archives[version]) / f"{name}.exe").resolve()), test,
                 "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (logs / f"{run}-{version}.log").write_text(result.stdout, encoding="utf-8")
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
                   for version in archives]
        print(f"{label}: {' -> '.join(f'{v:.2f}' for v in medians)} cycles/op")


if __name__ == "__main__":
    main()
