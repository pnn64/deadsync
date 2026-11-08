"""Compare archived production kernels using thread cycles and allocation probes."""
import argparse
import csv
import hashlib
from pathlib import Path
import re
import statistics
import subprocess


TARGETS = {
    "rows": ("note_row_kernel.exe", "benchmark_note_rows", 3),
    "notes": ("note_scan_kernel.exe", "benchmark_note_scan_kernel", 16),
    "creation": ("rules.exe", "benchmark_timing_creation", 7),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    binary, test, expected = TARGETS[args.target]
    timing = re.compile(r"((?:rows|notes|bpm|timing-build)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"((?:rows|notes|bpm|timing-build)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    archives = {stage: getattr(args, stage) / binary for stage in ["before", "after"]}
    hashes = {stage: hashlib.sha256(path.read_bytes()).hexdigest() for stage, path in archives.items()}
    logs = Path("target/row-parser-tables/runs") / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    rows = []
    for run in range(args.runs):
        order = ["before", "after"] if run % 2 == 0 else ["after", "before"]
        for stage in order:
            result = subprocess.run(
                [str(archives[stage].resolve()), test, "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (logs / f"{run}-{stage}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != expected or len(counts) != expected:
                raise ValueError(f"expected {expected} cycle and allocation measurements")
            for label, cycles, ns in matches:
                rows.append(dict(run=run, stage=stage, workload=label, cycles_per_op=float(cycles),
                                 ns_per_op=float(ns), **counts[label], sha256=hashes[stage]))
            print(f"run {run + 1}: {stage} {args.target}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == stage and row["workload"] == label)
                  for stage in archives]
        print(f"{label}: {values[0]:.2f} -> {values[1]:.2f} cycles/op; {values[0] / values[1]:.3f}x")


if __name__ == "__main__":
    main()
