"""Summarize matched PNG measurements, keeping output size and coverage visible."""

import argparse
from collections import Counter, defaultdict
import csv
import gzip
import json
from pathlib import Path
import statistics


BASELINES = {
    "decode-rgba8": ("deadlib",),
    "decode-rgba16": ("deadlib",),
    "decode-fpng": ("deadlib",),
    "decode-apng": ("deadlib",),
    "encode-rgba8": ("deadlib-fast", "deadlib-rle", "deadlib-rle-up"),
    "encode-native": ("deadlib-fast", "deadlib-rle", "deadlib-rle-up"),
    "encode-apng": ("deadlib-fast", "deadlib-rle"),
}
METRICS = ("cycles", "ns", "output_bytes")


def read(path):
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8-sig", newline="") as source:
        rows = list(csv.DictReader(source, delimiter="\t"))
    indexed = {}
    for row in rows:
        key = (row["workload"], row["file"], row["codec"])
        if key in indexed:
            raise ValueError(f"{path}: duplicate row {key}")
        for field in METRICS:
            row[field] = int(row[field])
        if row["status"] == "ok" and min(row[field] for field in METRICS) <= 0:
            raise ValueError(f"{path}: expected a timing row {key}")
        indexed[key] = row
    return indexed


def merge(paths):
    runs = [read(path) for path in paths]
    first = runs[0]
    if any(run.keys() != first.keys() for run in runs):
        raise ValueError(f"Different case coverage: {paths}")
    for key, row in first.items():
        copies = [run[key] for run in runs]
        stable = row.keys() - {"cycles", "ns"}
        if any(any(other[field] != row[field] for field in stable) for other in copies):
            raise ValueError(f"Different outputs or verification outcomes for {key}")
        yield {**row, **{key: statistics.median(r[key] for r in copies) for key in ("cycles", "ns")}}


def metrics(pairs):
    return {
        "matched": len(pairs),
        "wins": sum(a["cycles"] < b["cycles"] for a, b in pairs),
        "ties": sum(a["cycles"] == b["cycles"] for a, b in pairs),
        "native_total_cycles": sum(a["cycles"] for a, _ in pairs),
        "reference_total_cycles": sum(b["cycles"] for _, b in pairs),
        "total_ratios": {
            key: sum(a[key] for a, _ in pairs) / sum(b[key] for _, b in pairs)
            for key in METRICS
        },
        "equal_file_geomeans": {
            key: statistics.geometric_mean(a[key] / b[key] for a, b in pairs)
            for key in METRICS
        },
    }


def summarize(rows):
    grouped = defaultdict(lambda: defaultdict(dict))
    failures = []
    for row in rows:
        code, group, file = row["workload"], row["group"], row["file"]
        if row["status"] != "ok":
            failures.append({k: row[k] for k in ("workload", "group", "file", "codec", "status")})
            continue
        groups = {group, "all"}
        if group == "formats":
            groups.add(f"color-{row['color']}-depth-{row['depth']}-adam7-{row['adam7']}")
        for name in groups:
            grouped[(code, name)][file][row["codec"]] = row

    comparisons = defaultdict(dict)
    for (workload, group), files in sorted(grouped.items()):
        codecs = sorted({codec for entries in files.values() for codec in entries})
        winners = Counter()
        for entries in files.values():
            shortest = min(row["cycles"] for row in entries.values())
            tied = [row["codec"] for row in entries.values() if row["cycles"] == shortest]
            winners[tied[0] if len(tied) == 1 else "tie"] += 1
        results = {
            "cases": len(files),
            "file_winners": dict(winners),
            "aggregates": {},
            "pairs": {},
        }
        for codec in codecs:
            valid = [entries[codec] for entries in files.values() if codec in entries]
            results["aggregates"][codec] = {"cases": len(valid), **{
                key: sum(row[key] for row in valid) for key in METRICS
            }}
        for base in BASELINES[workload]:
            results["pairs"][base] = {}
            for codec in codecs:
                if base == codec:
                    continue
                pairs = [(entries[base], entries[codec]) for entries in files.values()
                         if base in entries and codec in entries]
                if pairs:
                    results["pairs"][base][codec] = metrics(pairs)
        comparisons[workload][group] = results
    return {"comparisons": comparisons, "excluded": failures}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("runs", nargs="+", type=Path, help="TSVs, ordered by matching workload")
    args = parser.parse_args()
    # A file pair has the same first part (decode-, encode-, fpng- or animation-).
    batches = defaultdict(list)
    for path in args.runs:
        batches[path.name.split("-", 1)[0]].append(path)
    all_rows = [row for paths in batches.values() for row in merge(paths)]
    args.output.write_text(json.dumps(summarize(all_rows), indent=2) + "\n", encoding="utf-8")
