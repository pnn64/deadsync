"""Compare archived simfile test executables, alternating process order."""
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
    parser.add_argument("--filter", default="benchmark_direct_parsing")
    parser.add_argument("--runs", type=int, default=6)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    pattern = re.compile(r"((?:delimiter|bgchanges|projection|unplayed)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn_pattern = re.compile(r"((?:delimiter|bgchanges|projection|unplayed)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            executable = getattr(args, version).resolve()
            digest = hashlib.sha256(executable.read_bytes()).hexdigest()
            result = subprocess.run([str(executable), args.filter, "--ignored",
                                     "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True)
            matches = pattern.findall(result.stdout)
            churn = {label: dict(re.findall(r"(\w+): (\d+)", fields))
                     for label, fields in churn_pattern.findall(result.stdout)}
            if not matches or len(matches) != len(churn):
                raise ValueError("missing measurements or allocation probes")
            for label, cpu, ns in matches:
                rows.append(dict(run=run, version=version, workload=label,
                                 cycles_per_op=float(cpu), ns_per_op=float(ns),
                                 **churn[label], executable_sha256=digest))
            print(f"run {run + 1}/{args.runs}: {version}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["version"] == version and row["workload"] == label)
                  for version in ["before", "after"]]
        print(f"{label}: {values[0]:.2f} -> {values[1]:.2f} cycles/op; {values[0] / values[1]:.3f}x")


if __name__ == "__main__":
    main()
