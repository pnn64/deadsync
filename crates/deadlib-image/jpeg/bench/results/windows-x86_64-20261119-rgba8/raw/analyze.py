"""Analyze fixed JPEG benchmark TSVs without decoding or timing images.

Usage: python analyze.py baseline=baseline.tsv candidate=candidate.tsv
Each total uses at most one valid row per fixed input/codec. Duplicate, missing,
unsupported, unexpected, or malformed rows remain explicit in the JSON report.
"""
import argparse
import csv
import io
import json
import math
from pathlib import Path

NUMERIC = ("pixels", "ns", "process_cycles", "alloc_calls", "requested_bytes")


def normalized(value):
    return value.strip().replace("\\", "/").removeprefix("./").casefold()


def decode_text(blob):
    if blob.startswith((b"\xff\xfe", b"\xfe\xff")):
        return blob.decode("utf-16")
    if b"\0" in blob[:128]:
        return blob.decode("utf-16-le" if blob[1:128:2].count(0) >= blob[:128:2].count(0) else "utf-16-be")
    return blob.decode("utf-8-sig")


def parse_tsv(blob, expected):
    lookup = {normalized(path): path for path in expected}
    rows, seen, issues, codecs, header = {}, set(), [], {"native", "image"}, None
    for line, fields in enumerate(csv.reader(io.StringIO(decode_text(blob)), delimiter="\t"), 1):
        if not fields or not any(fields):
            continue
        if fields[0].upper() in ("UNAVAILABLE", "UNSUPPORTED", "EXCLUDED"):
            issues.append(dict(kind="unsupported", line=line, details=fields))
            continue
        if fields[0] == "file":
            if header is not None:
                issues.append(dict(kind="duplicate_header", line=line))
            header = fields
            if not {"file", "codec", *NUMERIC}.issubset(header):
                issues.append(dict(kind="invalid_header", line=line, fields=fields))
                header = None
            continue
        if header is None or len(fields) != len(header):
            issues.append(dict(kind="invalid_row", line=line, fields=fields))
            continue
        row = dict(zip(header, fields))
        name, codec = normalized(row["file"]), row["codec"].strip()
        path = lookup.get(name) or next((p for n, p in lookup.items() if name.endswith("/" + n)), None)
        if not path or not codec:
            issues.append(dict(kind="unexpected_input_or_codec", line=line, file=row["file"], codec=codec))
            continue
        codecs.add(codec)
        key = (path, codec)
        if key in seen:
            rows.pop(key, None)
            issues.append(dict(kind="duplicate", line=line, file=path, codec=codec))
            continue
        seen.add(key)
        try:
            values = {field: int(row[field]) for field in NUMERIC}
            if min(values.values()) < 0 or values["pixels"] != expected[path]["pixels"]:
                raise ValueError("negative value or pixels disagree with fixed manifest")
        except ValueError as error:
            kind = "unsupported" if any(word in " ".join(fields).lower() for word in ("unavailable", "unsupported", "excluded")) else "invalid_values"
            issues.append(dict(kind=kind, line=line, file=path, codec=codec, reason=str(error)))
            continue
        rows[key] = values
    if header is None:
        issues.append(dict(kind="missing_valid_header"))
    return rows, sorted(codecs), issues


def metrics(rows, expected_count):
    totals = {field: sum(row[field] for row in rows) for field in NUMERIC}
    return dict(files=len(rows), expected_files=expected_count, complete=len(rows) == expected_count,
                totals=totals, megapixels_per_second=totals["pixels"] * 1000 / totals["ns"] if totals["ns"] else None,
                cycles_per_pixel=totals["process_cycles"] / totals["pixels"] if totals["pixels"] else None)


def compare(numerator, denominator, paths):
    paired = [p for p in paths if p in numerator and p in denominator and
              numerator[p]["process_cycles"] > 0 and denominator[p]["process_cycles"] > 0]
    ratios = [numerator[p]["process_cycles"] / denominator[p]["process_cycles"] for p in paired]
    top = sum(numerator[p]["process_cycles"] for p in paired)
    bottom = sum(denominator[p]["process_cycles"] for p in paired)
    return dict(matched_files=len(paired), expected_files=len(paths), complete=len(paired) == len(paths),
                missing_or_zero_cycle_files=sorted(set(paths) - set(paired)),
                numerator_total_cycles=top, denominator_total_cycles=bottom,
                total_cycles_ratio=top / bottom if bottom else None,
                cycles_ratio_geomean=math.exp(sum(map(math.log, ratios)) / len(ratios)) if ratios else None)


def losses(numerator, denominator, expected, threshold):
    out = []
    for path in sorted(expected):
        a, b = numerator.get(path), denominator.get(path)
        if a and b and b["process_cycles"] > 0 and a["process_cycles"] > b["process_cycles"] * threshold:
            out.append(dict(file=path, group=expected[path]["group"], cycles_ratio=a["process_cycles"] / b["process_cycles"],
                            ns_ratio=a["ns"] / b["ns"] if b["ns"] else None))
    return sorted(out, key=lambda row: (-row["cycles_ratio"], row["file"]))


def summarize(rows, codecs, issues, expected):
    paths = sorted(expected)
    grouped = {group: [p for p in paths if expected[p]["group"] == group] for group in sorted({v["group"] for v in expected.values()})}
    modes = {codec: {p: rows[(p, codec)] for p in paths if (p, codec) in rows} for codec in codecs}
    rollup = lambda subset: {codec: metrics([modes[codec][p] for p in subset if p in modes[codec]], len(subset)) for codec in codecs}
    return dict(issues=issues, complete=not issues and all(len(modes[c]) == len(paths) for c in codecs),
                missing_valid_rows={c: sorted(set(paths) - set(modes[c])) for c in codecs},
                codecs=rollup(paths), native_over_image=compare(modes["native"], modes["image"], paths),
                groups={g: dict(codecs=rollup(ps), native_over_image=compare(modes["native"], modes["image"], ps)) for g, ps in grouped.items()},
                native_image_losses=losses(modes["native"], modes["image"], expected, 1.0))


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runs", nargs="+", metavar="LABEL=TSV")
    parser.add_argument("--selection", type=Path, default=here / "jpeg-corpus-selection.json")
    parser.add_argument("--kodak", type=Path, default=here / "jpeg-corpus-kodak.json")
    parser.add_argument("--baseline", help="Label to compare against; defaults to the first run.")
    parser.add_argument("--regression-pct", type=float, default=5.0, help="Large native cycle regression threshold (default 5%%).")
    args = parser.parse_args()
    if not math.isfinite(args.regression_pct) or args.regression_pct < 0:
        parser.error("--regression-pct must be finite and nonnegative")
    selection = json.loads(args.selection.read_text(encoding="utf-8-sig"))["speed_candidates"]
    photos = json.loads(args.kodak.read_text(encoding="utf-8-sig"))["inputs"]
    expected = {}
    for item in selection + photos:
        path, group = item["path"].replace("\\", "/"), item["group"]
        group = {"shipped-assets": "shipped", "runtime-stratified": "runtime"}.get(group, group)
        group = "tinyfixtures" if group.startswith("small-generated-") else group
        if normalized(path) in {normalized(p) for p in expected}:
            parser.error("duplicate path in fixed manifests: " + path)
        expected[path] = dict(group=group, pixels=item["width"] * item["height"])
    if len(selection) != 45 or len(photos) != 60 or len(expected) != 105:
        parser.error("fixed corpus changed: expected 45 original + 60 Kodak = 105 paths")
    reports, native = {}, {}
    for argument in args.runs:
        label, separator, filename = argument.partition("=")
        if not separator or not label or not filename or label in reports:
            parser.error("each run needs a distinct LABEL=TSV")
        rows, codecs, issues = parse_tsv(Path(filename).read_bytes(), expected)
        reports[label] = dict(tsv=filename, **summarize(rows, codecs, issues, expected))
        native[label] = {p: rows[(p, "native")] for p in expected if (p, "native") in rows}
    baseline = args.baseline or next(iter(reports))
    if baseline not in reports:
        parser.error("--baseline must name a supplied run")
    comparisons = {label: dict(baseline=baseline, candidate_over_baseline=compare(values, native[baseline], sorted(expected)),
                              large_regressions=losses(values, native[baseline], expected, 1 + args.regression_pct / 100))
                   for label, values in native.items() if label != baseline}
    print(json.dumps(dict(expected_files=105, cycle_ratios="numerator / denominator; below 1 is faster",
                          partial_totals="Only unique valid fixed-corpus rows; all omissions/duplicates are listed, never zero-filled.",
                          large_regression_percent=args.regression_pct, runs=reports, comparisons=comparisons), indent=2))


if __name__ == "__main__":
    main()
