"""Compare archived production binaries using thread cycles and allocation probes."""
import argparse
import csv
from pathlib import Path
import re
import statistics
import subprocess


TARGETS = {
    "video": ("texture_pipeline", "benchmark_yuv_conversion", 7),
    "format": ("wav_format_kernel", "benchmark_wav_format", 8),
    "bool": ("direct_kernels", "benchmark_bool_codec", 69),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", action="append", required=True, metavar="NAME=DIR")
    parser.add_argument("--runs", type=int, default=6)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    archives = dict(item.split("=", 1) for item in args.archive)
    logs = Path("target/direct-video-wav/runs") / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    measurements = re.compile(
        r"((?:yuv|format|bool)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)"
    )
    probes = re.compile(r"((?:yuv|format|bool)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    rows = []
    for run in range(args.runs):
        order = list(archives)
        order = order[run % len(order):] + order[:run % len(order)]
        for version in order:
            for target, (executable, test, expected) in TARGETS.items():
                result = subprocess.run(
                    [str((Path(archives[version]) / f"{executable}.exe").resolve()),
                     test, "--ignored", "--nocapture", "--test-threads=1"],
                    capture_output=True, text=True, encoding="utf-8", check=True,
                )
                (logs / f"{run}-{version}-{target}.log").write_text(result.stdout, encoding="utf-8")
                samples = measurements.findall(result.stdout)
                churn = {label: dict(re.findall(r"(\w+): (\d+)", values))
                         for label, values in probes.findall(result.stdout)}
                if len(samples) != expected or len(churn) != expected:
                    raise ValueError(f"{target}: expected {expected} measurements and probes")
                for label, cycles, nanos in samples:
                    rows.append(dict(run=run, version=version, target=target, workload=label,
                                     cycles_per_op=float(cycles), ns_per_op=float(nanos),
                                     **churn[label]))
                print(f"run {run + 1}/{args.runs}: {version} {target}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["version"] == version and row["workload"] == label)
                  for version in archives]
        print(f"{label}: {' -> '.join(f'{v:.2f}' for v in values)} cycles/op")


if __name__ == "__main__":
    main()
