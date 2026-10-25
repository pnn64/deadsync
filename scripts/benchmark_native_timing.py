"""Compare archived production timing and note-scan kernels, rotating process order."""
import argparse
import csv
import hashlib
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", action="append", required=True, help="stage=directory")
    parser.add_argument("--target", choices=["timing", "notes"], required=True)
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    archives = [value.split("=", 1) for value in args.archive]
    binary, expected = ("native_timing_kernel.exe", 7) if args.target == "timing" else ("note_scan_kernel.exe", 12)
    timing = re.compile(r"((?:delta|classify|notes)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"((?:delta|classify|notes)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    logs = Path("target/native-timing/runs")
    logs.mkdir(parents=True, exist_ok=True)
    rows = []
    hashes = {}
    for stage, directory in archives:
        hashes[stage] = hashlib.sha256((Path(directory) / binary).read_bytes()).hexdigest()
        print(f"{stage}: sha256 {hashes[stage]}", flush=True)
    for run in range(args.runs):
        for stage, directory in archives[run % len(archives):] + archives[:run % len(archives)]:
            result = subprocess.run(
                [str((Path(directory) / binary).resolve()), "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (logs / f"{args.target}-{run}-{stage}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values)) for label, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != expected or len(counts) != expected:
                raise ValueError(f"expected {expected} timing and allocation measurements")
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
        values = [statistics.median(row["cycles_per_op"] for row in rows if row["stage"] == stage and row["workload"] == label)
                  for stage, _ in archives]
        print(f"{label}: " + " -> ".join(f"{value:.2f}" for value in values) + f" cycles/op; {values[0] / values[-1]:.3f}x")


if __name__ == "__main__":
    main()
