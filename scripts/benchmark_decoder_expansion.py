"""Compare archived decoder stages using CPU cycles, throughput, and allocations."""
import argparse
import csv
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", action="append", required=True, metavar="NAME=EXE")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--logs", type=Path, default=Path("target/decoder-expansion/runs"))
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--kind", choices=["all", "png", "bmp"], default="all")
    args = parser.parse_args()
    stages = [stage.split("=", 1) for stage in args.stage]
    if args.runs < 1 or len({name for name, _ in stages}) != len(stages):
        parser.error("runs must be positive and stage names must be unique")
    args.logs.mkdir(parents=True, exist_ok=True)
    timing = re.compile(r"(expand/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn = re.compile(r"(expand/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    targets = [(kind, count) for kind, count in [("png", 24), ("bmp", 9)]
               if args.kind in ("all", kind)]
    rows = []
    for run in range(args.runs):
        order = stages[run % len(stages):] + stages[:run % len(stages)]
        for name, executable in order:
            for kind, expected in targets:
                test = f"benchmark_{kind}_expansion"
                result = subprocess.run(
                    [str(Path(executable).resolve()), test, "--ignored", "--nocapture", "--test-threads=1"],
                    capture_output=True, text=True, encoding="utf-8", check=True,
                )
                (args.logs / f"{run}-{name}-{test}.log").write_text(result.stdout, encoding="utf-8")
                samples = timing.findall(result.stdout)
                allocations = {label: dict(re.findall(r"(\w+): (\d+)", values))
                               for label, values in churn.findall(result.stdout)}
                if len(samples) != expected or len(allocations) != expected:
                    raise ValueError(f"{test}: expected {expected} measurements and allocation probes")
                for label, cycles, ns in samples:
                    pixels = int(label.rsplit("/", 1)[1]) ** 2
                    rows.append(dict(run=run, stage=name, workload=label, pixels=pixels,
                                     cycles_per_op=float(cycles), ns_per_op=float(ns),
                                     pixels_per_cycle=pixels / float(cycles),
                                     megapixels_per_second=pixels * 1000 / float(ns),
                                     **allocations[label]))
                print(f"run {run + 1}/{args.runs}: {name} {test}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        medians = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == name and row["workload"] == label)
                   for name, _ in stages]
        print(f"{label}: " + " -> ".join(f"{value:.2f}" for value in medians)
              + f" cycles/op; total {medians[0] / medians[-1]:.3f}x")


if __name__ == "__main__":
    main()
