"""Verify repeat coverage and summarize matched JPEG timings, size and PSNR."""

import argparse
from collections import Counter, defaultdict
import csv
import gzip
import json
import math
from pathlib import Path
import statistics


def read(path):
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8-sig", newline="") as source:
        rows = list(csv.DictReader(source, delimiter="\t"))
    table = {}
    for row in rows:
        key = row["file"], row["config"], row["codec"]
        if key in table:
            raise ValueError(f"duplicate row in {path}: {key}")
        if row["status"] == "ok":
            for name in ("cycles", "ns", "output_bytes"):
                row[name] = int(row[name])
                if row[name] <= 0:
                    raise ValueError(f"missing timing or output in {path}: {key}")
        table[key] = row
    return table


def merge(paths, subset_paths=()):
    tables = [read(path) for path in paths]
    first = tables[0]
    if any(table.keys() != first.keys() for table in tables):
        raise ValueError(f"input coverage differs: {paths}")
    subsets = [read(path) for path in subset_paths]
    if any(not table.keys() <= first.keys() for table in subsets):
        raise ValueError(f"repeat contains inputs absent from full runs: {subset_paths}")
    for key, row in first.items():
        samples = [table[key] for table in tables]
        samples.extend(table[key] for table in subsets if key in table)
        stable = row.keys() - {"cycles", "ns"}
        if any(any(other[f] != row[f] for f in stable) for other in samples):
            raise ValueError(f"verification output differs: {key}")
        if row["status"] == "ok":
            row = {**row, **{name: statistics.median(r[name] for r in samples) for name in ("cycles", "ns")}}
        yield row


def summarize(rows, mode):
    groups = defaultdict(lambda: defaultdict(dict))
    rejects = []
    support = Counter()
    expected = Counter()
    for row in rows:
        key = row["file"], row["config"]
        labels = {"all", row["group"]}
        if mode == "encode":
            labels.add(row["config"])
            if row["group"] in ("photo", "capture") and row["config"] in ("q75-420", "q90-420"):
                labels.add("matched-real-" + row["config"])
        else:
            labels.add(f"sof-{row['sof']}-p{row['precision']}")
            if row["group"] in ("kodak-baseline-420", "kodak-progressive-420", "deadsync-assets"):
                labels.add("common-images")
            if row["precision"] == "8" and row["sof"] in ("c0", "c1", "c2"):
                labels.add("ordinary-8-bit")
        expected[row["codec"]] += 1
        if row["status"] != "ok":
            rejects.append({k: row[k] for k in ("codec", "file", "group", "config", "status")})
            continue
        support[row["codec"]] += 1
        for label in labels:
            groups[label][key][row["codec"]] = row

    baselines = ["deadlib"] if mode == "decode" else ["deadlib-opt", "deadlib-fixed"]
    result = {}
    for label, cases in sorted(groups.items()):
        codecs = sorted({codec for entries in cases.values() for codec in entries})
        winners = Counter(min(entries.values(), key=lambda r: r["cycles"])["codec"] for entries in cases.values())
        pairs = {}
        for native in baselines:
            pairs[native] = {}
            for codec in codecs:
                if codec == native:
                    continue
                matched = [(v[native], v[codec]) for v in cases.values() if native in v and codec in v]
                if not matched:
                    continue
                measure = lambda key, side: sum(pair[side][key] for pair in matched)
                psnr = [[float(pair[side]["psnr_db"]) for pair in matched] for side in (0, 1)]
                quality_delta = [a - b for a, b in zip(*psnr) if math.isfinite(a) and math.isfinite(b)]
                pairs[native][codec] = {
                    "matched": len(matched),
                    "native_wins": sum(a["cycles"] < b["cycles"] for a, b in matched),
                    "native_cycles": measure("cycles", 0),
                    "reference_cycles": measure("cycles", 1),
                    "cycle_ratio": measure("cycles", 0) / measure("cycles", 1),
                    "wall_ratio": measure("ns", 0) / measure("ns", 1),
                    "byte_ratio": measure("output_bytes", 0) / measure("output_bytes", 1),
                    "equal_file_cycle_geomean": statistics.geometric_mean(a["cycles"] / b["cycles"] for a, b in matched),
                    "mean_psnr_delta_db": statistics.mean(quality_delta) if quality_delta else None,
                }
        result[label] = {"cases": len(cases), "winners": dict(winners), "pairs": pairs}
    return {"groups": result, "verified": dict(support), "attempted": dict(expected), "excluded": rejects}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["decode", "encode"])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeat-subset", type=Path, action="append", default=[])
    parser.add_argument("runs", type=Path, nargs="+")
    args = parser.parse_args()
    rows = merge(args.runs, args.repeat_subset)
    args.output.write_text(json.dumps(summarize(rows, args.mode), indent=2) + "\n", encoding="utf-8")
