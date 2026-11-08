"""Alternate archived asset metadata test binaries and record CPU/allocation results."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=4)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    logs = Path("target/asset-path-scans") / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    timing = re.compile(r"(paths/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"(paths/[^:\r\n]+): items (\d+); allocation Churn \{ ([^}]+) \}")
    metadata = {}
    for version in ["before", "after"]:
        binary = getattr(args, version).resolve()
        result = subprocess.run(
            [str(binary), "path_scan_perf::", "--test-threads=1"],
            capture_output=True, text=True, check=True,
        )
        (logs / f"{version}-checks.log").write_text(result.stdout, encoding="utf-8")
        metadata[version] = dict(binary=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                                 checks=result.stdout)
    rows = []
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            result = subprocess.run(
                [str(getattr(args, version).resolve()), "path_scan_perf::benchmark_metadata_scans",
                 "--ignored", "--nocapture", "--test-threads=1"],
                capture_output=True, text=True, check=True,
            )
            (logs / f"{run}-{version}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: (int(items), dict((key, int(value)) for key, value in re.findall(r"(\w+): (\d+)", values)))
                      for label, items, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != 21 or len(counts) != 21:
                raise ValueError("expected 21 CPU measurements and allocation probes")
            for label, cycles, ns in matches:
                items, churn = counts[label]
                if any(churn.values()):
                    raise ValueError(f"metadata scan allocated: {label} {churn}")
                rows.append(dict(run=run, version=version, workload=label, items=items,
                                 cycles_per_item=float(cycles) / items,
                                 ns_per_item=float(ns) / items, **churn))
            print(f"run {run + 1}/{args.runs}: {version}", flush=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    args.output.with_suffix(".json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    for label in sorted(counts):
        cpu = [statistics.median(row["cycles_per_item"] for row in rows
                                 if row["version"] == version and row["workload"] == label)
               for version in ["before", "after"]]
        print(f"{label}: {cpu[0]:.2f} -> {cpu[1]:.2f} cycles/item; {cpu[0] / cpu[1]:.3f}x")


if __name__ == "__main__":
    main()
