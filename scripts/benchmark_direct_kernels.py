"""Compare archived production test binaries, alternating process order.

Build both directories with identical Cargo options. Each must contain
deadsync_rules.exe, deadlib_render_backend_software.exe, and marker_kernel.exe.
The paired benchmarks' right-hand columns call the production implementation.
"""
import argparse
import csv
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--logs", type=Path, default=Path("target/direct-kernels/runs"))
    parser.add_argument("--runs", type=int, default=4)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.logs.mkdir(parents=True, exist_ok=True)
    rows = []
    pattern = re.compile(
        r"((?:density|opacity|markers|text-build)/[^:\r\n]+): cycles/op "
        r"([\d.]+)(?: -> ([\d.]+))?; ns/op ([\d.]+)(?: -> ([\d.]+))?"
    )
    targets = [
        ("deadsync_rules", "benchmark_density_kernel", 12),
        ("deadlib_render_backend_software", "benchmark_opacity_kernel", 12),
        ("marker_kernel", "benchmark_marker_kernel", 16),
    ]
    for run in range(args.runs):
        order = ["before", "after"] if run % 2 == 0 else ["after", "before"]
        for version in order:
            for name, test, expected in targets:
                result = subprocess.run(
                    [str((getattr(args, version) / f"{name}.exe").resolve()), test,
                     "--ignored", "--nocapture", "--test-threads=1"],
                    capture_output=True, text=True, encoding="utf-8", check=True,
                )
                (args.logs / f"{run}-{version}-{name}.log").write_text(result.stdout, encoding="utf-8")
                matches = pattern.findall(result.stdout)
                if len(matches) != expected:
                    raise ValueError(f"{name}: expected {expected} measurements, got {len(matches)}")
                for label, old_cpu, current_cpu, old_ns, current_ns in matches:
                    rows.append((run, version, label, float(current_cpu or old_cpu), float(current_ns or old_ns)))
                print(f"run {run + 1}/{args.runs}: {version} {name}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output, delimiter="\t")
        writer.writerow(["run", "version", "workload", "cycles_per_op", "ns_per_op"])
        writer.writerows(rows)
    for label in sorted({row[2] for row in rows}):
        medians = [statistics.median(row[3] for row in rows if row[1] == version and row[2] == label)
                   for version in ["before", "after"]]
        print(f"{label}: {medians[0]:.2f} -> {medians[1]:.2f} cycles/op; {medians[0] / medians[1]:.3f}x")


if __name__ == "__main__":
    main()
