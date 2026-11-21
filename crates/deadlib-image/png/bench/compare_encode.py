"""Summarize PNG encoding TSVs, retaining every measured regression."""
import argparse
import csv
import io
import json
import math
from pathlib import Path


METRICS = ("thread_cycles", "ns", "encoded_bytes", "alloc_calls", "requested_bytes")
NATIVE = ("native-buffered", "native-writer", "native-rle-buffered", "native-rle-writer",
          "native-rle-up-buffered", "native-rle-up-writer")
REFERENCES = ("image-default", "png-fast-adaptive", "png-balanced-adaptive", "png-fast-up")
CORPUS = Path(__file__).resolve().parent / "results/2026-11-20-encode/corpus.json"


def read(path):
    data = path.read_bytes()
    text = data.decode("utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig")
    rows = {}
    for row in csv.DictReader(io.StringIO(text), delimiter="\t"):
        key = (row["file"].replace("\\", "/"), row["codec"])
        if key in rows:
            raise ValueError(f"duplicate key in {path}: {key}")
        for field in ("width", "height", "pixels", *METRICS):
            row[field] = int(row[field])
            if row[field] < 0:
                raise ValueError(f"negative {field}: {key}")
        if not row["width"] or not row["height"] or row["pixels"] != row["width"] * row["height"]:
            raise ValueError(f"invalid dimensions: {key}")
        if not row["encoded_bytes"] or len(row["encoded_fnv1a64"]) != 16:
            raise ValueError(f"invalid encoded identity: {key}")
        int(row["encoded_fnv1a64"], 16)
        rows[key] = row
    if not rows:
        raise ValueError(f"no rows in {path}")
    modes = {mode for _, mode in rows}
    files = {file for file, _ in rows}
    if len(rows) != len(modes) * len(files):
        raise ValueError(f"incomplete file/codec matrix in {path}")
    for file in files:
        dimensions = {(rows[file, mode]["width"], rows[file, mode]["height"]) for mode in modes}
        if len(dimensions) != 1:
            raise ValueError(f"codec dimensions disagree: {file}")
    return rows


def stats(files, left, right):
    """Ratios are right/left; lower is better, with one observation per file."""
    positive = all(x > 0 and y > 0 for x, y in zip(left, right))
    return {
        "geomean": math.exp(sum(math.log(y / x) for x, y in zip(left, right)) / len(files)) if positive else None,
        "left_total": sum(left), "right_total": sum(right),
        "wins": sum(y < x for x, y in zip(left, right)),
        "ties": sum(y == x for x, y in zip(left, right)),
        "losses": [{"file": f, "left": x, "right": y, "ratio": y / x if x else None}
                   for f, x, y in zip(files, left, right) if y > x],
    }


def compare(files, left, right):
    out = {}
    for metric in METRICS:
        a, b = [r[metric] for r in left], [r[metric] for r in right]
        if any(a + b):
            if metric in ("thread_cycles", "ns") and not all(x > 0 for x in a + b):
                raise ValueError(f"mixed zero and nonzero {metric} measurements")
            out[metric] = stats(files, a, b)
    return out


def groups(rows, manifest):
    mapping = {item["path"].replace("\\", "/"): item["group"] for item in manifest["files"]}
    result = {"all": sorted({file for file, _ in rows})}
    for file in result["all"]:
        matches = {group for path, group in mapping.items() if file == path or file.endswith("/" + path)}
        if file.startswith("generated/"):
            group = "generated"
        elif len(matches) == 1:
            group = matches.pop()
        else:
            raise ValueError(f"missing or ambiguous corpus group: {file}")
        result.setdefault(group, []).append(file)
    return result


def summary(rows, grouped):
    result = {}
    modes = sorted({mode for _, mode in rows})
    for group, files in grouped.items():
        codecs, pairs = {}, {}
        for mode in modes:
            values = [rows[file, mode] for file in files]
            totals = {metric: sum(row[metric] for row in values) for metric in METRICS}
            pixels = sum(row["pixels"] for row in values)
            codecs[mode] = {"total_pixels": pixels, **totals,
                            "aggregate_mpix_s": pixels * 1000 / totals["ns"] if totals["ns"] else None}
        for native in NATIVE:
            for reference in REFERENCES:
                if native in modes and reference in modes:
                    pairs[f"{native}/{reference}"] = compare(files,
                        [rows[file, reference] for file in files], [rows[file, native] for file in files])
        result[group] = {"files": len(files), "codecs": codecs, "pairs": pairs}
    return result


def select_modes(rows, selection):
    if selection is None:
        return rows
    modes = set(selection.split(","))
    available = {mode for _, mode in rows}
    if not modes <= available:
        raise ValueError(f"requested codecs missing: {sorted(modes - available)}")
    return {key: row for key, row in rows.items() if key[1] in modes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("after", type=Path, nargs="?")
    parser.add_argument("--corpus", type=Path, default=CORPUS)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--modes", help="compare only these comma-separated codecs, present in both inputs")
    parser.add_argument("--allow-output-drift", action="store_true",
                        help="report changed PNG bytes/fingerprints instead of rejecting them")
    args = parser.parse_args()
    before = select_modes(read(args.baseline), args.modes)
    grouped = groups(before, json.loads(args.corpus.read_text(encoding="utf-8-sig")))
    result = {"ratio_direction": "right/left; lower is better", "selected_modes": args.modes,
              "baseline": summary(before, grouped)}
    if args.after:
        after = select_modes(read(args.after), args.modes)
        if before.keys() != after.keys():
            raise ValueError(f"input/codec keys differ: missing={sorted(before.keys() - after.keys())}, "
                             f"added={sorted(after.keys() - before.keys())}")
        drift = []
        for key, old in before.items():
            new = after[key]
            if any(old[field] != new[field] for field in ("width", "height", "pixels")):
                raise ValueError(f"dimensions changed: {key}")
            identity = ("encoded_bytes", "encoded_fnv1a64")
            if any(old[field] != new[field] for field in identity):
                drift.append({"file": key[0], "codec": key[1],
                              "before": {field: old[field] for field in identity},
                              "after": {field: new[field] for field in identity}})
        if drift and not args.allow_output_drift:
            raise ValueError(f"encoded output changed in {len(drift)} rows; use --allow-output-drift to report it")
        result["output_drift"] = drift
        result["after"] = summary(after, grouped)
        modes = sorted({mode for _, mode in before})
        result["changes"] = {group: {mode: compare(files,
            [before[file, mode] for file in files], [after[file, mode] for file in files])
            for mode in modes} for group, files in grouped.items()}
    encoded = json.dumps(result, indent=2, allow_nan=False) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
        print(args.output)
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
