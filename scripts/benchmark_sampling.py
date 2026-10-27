"""Compare archived CPU kernel benchmarks using thread CPU cycles."""
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
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=6)
    parser.add_argument("--test-filter", default="benchmark_sampling")
    parser.add_argument("--log-dir", type=Path, default=Path("target/direct-sampling/runs"))
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    rows = []
    expected_labels = None
    pattern = re.compile(r"([\w/]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocations = re.compile(r"([\w/]+): allocation Churn \{ (.*?) \}")
    logs = args.log_dir / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            executable = getattr(args, version).resolve()
            result = subprocess.run([str(executable), args.test_filter, "--ignored", "--nocapture", "--test-threads=1"], capture_output=True, text=True, check=True)
            (logs / f"{run}-{version}.log").write_text(result.stdout)
            matches = pattern.findall(result.stdout)
            labels = {match[0] for match in matches}
            if not labels or len(labels) != len(matches):
                raise ValueError("Missing or duplicate measurements")
            if expected_labels is None:
                expected_labels = labels
            elif labels != expected_labels:
                raise ValueError(f"Workloads changed: {expected_labels} -> {labels}")
            counts = dict(allocations.findall(result.stdout))
            digest = hashlib.sha256(executable.read_bytes()).hexdigest()
            for label, cpu, ns in matches:
                rows.append((run, version, label, float(cpu), float(ns), counts.get(label, ""), digest))
        print(f"run {run + 1}/{args.runs}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output, delimiter="\t")
        writer.writerow(["run", "version", "workload", "cycles_per_op", "ns_per_op", "allocations", "sha256"])
        writer.writerows(rows)
    for label in dict.fromkeys(row[2] for row in rows):
        old, new = [statistics.median(row[3] for row in rows if row[1] == version and row[2] == label) for version in ["before", "after"]]
        print(f"{label}: {old:.2f} -> {new:.2f} cycles/op; {old/new:.3f}x")


if __name__ == "__main__":
    main()
