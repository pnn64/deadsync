"""Alternate archived Rust test executables; record CPU cycles and allocation probes."""
import argparse
import csv
import hashlib
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--filter", default="benchmark_slice_kernels")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--logs", type=Path, default=Path("target/slice-kernels/runs"))
    parser.add_argument("--runs", type=int, default=4)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    logs = args.logs / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    pattern = re.compile(r"([^\s:]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"([^\s:]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    labels = None
    hashes = {version: hashlib.sha256(getattr(args, version).read_bytes()).hexdigest()
              for version in ["before", "after"]}
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            result = subprocess.run(
                [str(getattr(args, version).resolve()), args.filter, "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (logs / f"{run}-{version}.log").write_text(result.stdout, encoding="utf-8")
            probes = {label: dict((key, int(value)) for key, value in re.findall(r"(\w+): (\d+)", fields))
                      for label, fields in allocation.findall(result.stdout)}
            matches = pattern.findall(result.stdout)
            current_labels = [label for label, _, _ in matches]
            if not current_labels or (labels is not None and current_labels != labels):
                raise ValueError("benchmark workloads changed or are missing")
            labels = current_labels
            for label, cycles, nanos in matches:
                counts = probes[label]
                rows.append([run, version, label, cycles, nanos, *counts.values(), hashes[version]])
            print(f"run {run + 1}/{args.runs}: {version}", flush=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output, delimiter="\t")
        writer.writerow(["run", "version", "workload", "cycles_per_op", "ns_per_op", *counts.keys(), "binary_sha256"])
        writer.writerows(rows)
    for label in labels:
        medians = [statistics.median(float(row[3]) for row in rows if row[1] == version and row[2] == label)
                   for version in ["before", "after"]]
        print(f"{label}: {medians[0]:.2f} -> {medians[1]:.2f} cycles/op; {medians[0] / medians[1]:.3f}x")


if __name__ == "__main__":
    main()
