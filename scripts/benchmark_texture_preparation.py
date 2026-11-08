"""Compare identical texture benchmark harnesses in archived production binaries."""
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
    parser.add_argument("--logs", type=Path, default=Path("target/preparation/runs"))
    parser.add_argument("--runs", type=int, default=4)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.logs.mkdir(parents=True, exist_ok=True)
    timing = re.compile(r"((?:luma|zoom|png)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"((?:luma|zoom|png)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    for version in ["before", "after"]:
        print(f"{version}: sha256 {hashlib.sha256(getattr(args, version).read_bytes()).hexdigest()}", flush=True)
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            result = subprocess.run(
                [str(getattr(args, version).resolve()), "preparation_perf::benchmark_",
                 "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (args.logs / f"{run}-{version}.log").write_text(result.stdout, encoding="utf-8")
            churn = {label: dict(re.findall(r"(\w+): (\d+)", values))
                     for label, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != 29 or len(churn) != 29:
                raise ValueError("expected 29 CPU measurements and allocation probes")
            for label, cycles, ns in matches:
                rows.append(dict(run=run, version=version, workload=label,
                                 cycles_per_op=float(cycles), ns_per_op=float(ns), **churn[label]))
            print(f"run {run + 1}/{args.runs}: {version}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted(churn):
        cpu = [statistics.median(row["cycles_per_op"] for row in rows
                                 if row["version"] == version and row["workload"] == label)
               for version in ["before", "after"]]
        print(f"{label}: {cpu[0]:.2f} -> {cpu[1]:.2f} cycles/op; {cpu[0] / cpu[1]:.3f}x")


if __name__ == "__main__":
    main()
