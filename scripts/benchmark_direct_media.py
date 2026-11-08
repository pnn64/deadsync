"""Measure archived production media kernels with CPU cycles and allocation counts."""
import argparse
import csv
import hashlib
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", action="append", required=True, help="stage=directory")
    parser.add_argument("--target", choices=["assets", "planar", "mapping"], required=True)
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--logs", type=Path, default=Path("target/direct-media/runs"))
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    archives = [value.split("=", 1) for value in args.archive]
    binary, test, expected = {
        "assets": ("assets.exe", "direct_media_perf::benchmark_direct_media", 14),
        "planar": ("planar.exe", "benchmark_planar_kernel", 12),
        "mapping": ("planar.exe", "benchmark_mapping_kernel", 18),
    }[args.target]
    timing = re.compile(r"((?:resize|gif|planar|mapping)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"((?:resize|gif|planar|mapping)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    args.logs.mkdir(parents=True, exist_ok=True)
    for stage, directory in archives:
        executable = Path(directory) / binary
        print(f"{stage}: sha256 {hashlib.sha256(executable.read_bytes()).hexdigest()}", flush=True)
    for run in range(args.runs):
        order = archives[run % len(archives):] + archives[:run % len(archives)]
        for stage, directory in order:
            result = subprocess.run(
                [str((Path(directory) / binary).resolve()), test,
                 "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, encoding="utf-8", check=True,
            )
            (args.logs / f"{args.target}-{run}-{stage}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != expected or len(counts) != expected:
                raise ValueError(f"expected {expected} timing and allocation measurements")
            for label, cycles, ns in matches:
                rows.append(dict(run=run, stage=stage, workload=label,
                                 cycles_per_op=float(cycles), ns_per_op=float(ns), **counts[label]))
            print(f"run {run + 1}: {stage} {args.target}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
