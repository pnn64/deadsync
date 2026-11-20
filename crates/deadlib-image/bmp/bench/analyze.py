#!/usr/bin/env python3
"""Analyze BMP benchmark TSVs using only the Python standard library.

Examples:
  python analyze.py before.tsv after.tsv --mode native-convert --skip-reference
  python analyze.py before.tsv after.tsv --mode native-rgba --details
  python analyze.py --reference after.tsv
  python analyze.py --reference owned.tsv --native-mode native-owned-rgba
  python analyze.py before.tsv after.tsv --alloc-before before-alloc.tsv --alloc-after after-alloc.tsv

Ratios are before/after (or image/native); greater than one is better. All rows
are checked before filtering. Every observed regression is listed; there is no
statistical significance claim or hidden noise threshold. Output is Markdown.
"""

import argparse
import csv
from decimal import Decimal, InvalidOperation
import io
import math
from pathlib import Path
import re
import sys


METRICS = ("thread_cycles", "ns", "alloc_calls", "requested_bytes")
LAYOUT = re.compile(r"^(.*)-(\d+x\d+)-(top|bottom)$")


def load(path):
    raw = Path(path).read_bytes()
    encoding = "utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    reader = csv.DictReader(io.StringIO(raw.decode(encoding)), delimiter="\t")
    required = {"case", "mode", "pixels", "image_equivalent", *METRICS}
    fields = reader.fieldnames or []
    if len(fields) != len(set(fields)) or not required.issubset(fields):
        raise ValueError(f"{path}: invalid header; required fields: {sorted(required)}")
    rows = {}
    for line, row in enumerate(reader, 2):
        if None in row or any(value is None for value in row.values()):
            raise ValueError(f"{path}:{line}: malformed TSV row")
        key = row["case"], row["mode"]
        if not all(key) or key in rows:
            raise ValueError(f"{path}:{line}: empty or duplicate case/mode key {key!r}")
        if row["image_equivalent"] not in ("true", "false"):
            raise ValueError(f"{path}:{line}: invalid image_equivalent")
        if int(row["pixels"]) <= 0:
            raise ValueError(f"{path}:{line}: pixels must be positive")
        for metric in METRICS:
            value = Decimal(row[metric])
            if not value.is_finite() or value < 0:
                raise ValueError(f"{path}:{line}: invalid {metric}={row[metric]!r}")
        rows[key] = row
    if not rows:
        raise ValueError(f"{path}: no measurements")
    return rows


def same_keys(before, after, label):
    missing, added = set(before) - set(after), set(after) - set(before)
    if missing or added:
        raise ValueError(
            f"{label}: case/mode key sets differ; missing={len(missing)} "
            f"{sorted(missing)[:8]!r}; added={len(added)} {sorted(added)[:8]!r}"
        )
    for key in before:
        for field in ("pixels", "image_equivalent"):
            if before[key][field] != after[key][field]:
                raise ValueError(f"{label}: {key!r} changed {field}")


def number(row, metric):
    return Decimal(row[metric])


def ratio(before, after):
    if before == 0 and after == 0:
        return Decimal(1)
    if after == 0:
        return Decimal("Infinity")
    return before / after


def formatted(value):
    if isinstance(value, Decimal) and not value.is_finite():
        return "infinite" if value > 0 else "undefined"
    return f"{value:.6f}".rstrip("0").rstrip(".")


def geomean(values):
    if any(not value.is_finite() or value <= 0 for value in values):
        return "n/a (zero measurement)"
    return formatted(math.exp(math.fsum(math.log(float(value)) for value in values) / len(values)))


def cell(value):
    return str(value).replace("|", "\\|").replace("\n", " ")


def table(headers, rows):
    print("| " + " | ".join(map(cell, headers)) + " |")
    print("| " + " | ".join("---" for _ in headers) + " |")
    for row in rows:
        print("| " + " | ".join(map(cell, row)) + " |")
    print()


def group_name(case):
    match = LAYOUT.fullmatch(case)
    return (match[1], match[2]) if match else (case, "external")


def group_order(item):
    layout, size, mode = item[0]
    area = math.prod(map(int, size.split("x"))) if size != "external" else 0
    return layout, area, size, mode


def selected(key, args, ignore_mode=False):
    return args.case in key[0] and (ignore_mode or not args.mode or key[1] in args.mode)


def require_nonzero(rows, keys, metric):
    if metric in ("thread_cycles", "ns"):
        zeros = [key for key in keys if number(rows[key], metric) == 0]
        if zeros:
            raise ValueError(f"{metric} unavailable/zero for {zeros[:8]!r}; choose an available metric")


def paired_report(before, after, args, metric, title):
    keys = sorted(key for key in before if selected(key, args))
    if not keys:
        raise ValueError("no measurements match the requested mode/case filters")
    require_nonzero(before, keys, metric)
    require_nonzero(after, keys, metric)
    comparisons = [(key, number(before[key], metric), number(after[key], metric)) for key in keys]
    ratios = [ratio(old, new) for _, old, new in comparisons]
    worst = min(zip(comparisons, ratios), key=lambda item: item[1])
    print(f"## {title}: {metric}\n")
    print(f"{len(keys)} paired measurements; geometric mean ratio **{geomean(ratios)}x**; "
          f"minimum **{formatted(min(ratios))}x**; maximum **{formatted(max(ratios))}x**. "
          f"Worst: `{worst[0][0][0]}` / `{worst[0][0][1]}`.\n")
    print("Ratios greater than 1 mean less work after the change. Orientations are grouped; "
          "sizes and modes remain separate. Groups are equally weighted per measurement, "
          "not weighted by pixel count.\n")
    groups = {}
    for key, old, new in comparisons:
        groups.setdefault((*group_name(key[0]), key[1]), []).append(ratio(old, new))
    table(("Layout", "Size", "Mode", "Cases", "Geomean ratio", "Min ratio", "Max ratio"), (
        (*key, len(values), geomean(values), formatted(min(values)), formatted(max(values)))
        for key, values in sorted(groups.items(), key=group_order)
    ))
    regressions = [(key, old, new) for key, old, new in comparisons if new > old]
    print(f"### Observed regressions ({len(regressions)})\n")
    print("Every increase is listed, including changes that may be measurement noise.\n")
    if regressions:
        table(("Case", "Mode", "Before", "After", "Before/after", "Increase %"), (
            (key[0], key[1], before[key][metric], after[key][metric], formatted(ratio(old, new)),
             formatted((new / old - 1) * 100) if old else "from zero")
            for key, old, new in sorted(regressions, key=lambda item: ratio(item[1], item[2]))
        ))
    else:
        print("None.\n")
    if args.details:
        print("### All paired measurements\n")
        table(("Case", "Mode", "Before", "After", "Before/after"), (
            (key[0], key[1], before[key][metric], after[key][metric], formatted(ratio(old, new)))
            for key, old, new in comparisons
        ))


def reference_report(rows, args):
    native = {case: row for (case, mode), row in rows.items() if mode == args.native_mode}
    image = {case: row for (case, mode), row in rows.items() if mode == "image-rgba"}
    expected = {case for case, row in native.items() if row["image_equivalent"] == "true"}
    if set(image) != expected:
        raise ValueError(f"reference key sets differ: missing image-rgba={sorted(expected-set(image))[:8]!r}; "
                         f"unexpected image-rgba={sorted(set(image)-expected)[:8]!r}")
    cases = sorted(case for case in expected if selected((case, args.native_mode), args, ignore_mode=True))
    if not native:
        print(f"No {args.native_mode} rows in this dataset; reference comparison unavailable.\n")
        return
    for case in expected:
        if image[case]["image_equivalent"] != "true" or image[case]["pixels"] != native[case]["pixels"]:
            raise ValueError(f"{case}: inconsistent reference dimensions/equivalence metadata")
    if not cases:
        raise ValueError("no equivalent RGBA cases match the filter")
    require_nonzero(rows, [(case, mode) for case in cases for mode in (args.native_mode, "image-rgba")], args.metric)
    comparisons = [(case, ratio(number(image[case], args.metric), number(native[case], args.metric))) for case in cases]
    ratios = [value for _, value in comparisons]
    excluded = sum(row["image_equivalent"] == "false" and args.case in case for case, row in native.items())
    slower = sum(value < 1 for value in ratios)
    print(f"## Equivalent RGBA comparison: {args.native_mode}, {args.metric}\n")
    print(f"{len(cases)} exact-output cases; {excluded} non-equivalent native cases excluded. "
          f"Image/native geometric mean **{geomean(ratios)}x**; worst **{formatted(min(ratios))}x**; "
          f"best **{formatted(max(ratios))}x**; **{slower} native regressions versus image**. "
          "A ratio below 1 means native is slower or uses more allocations/bytes.\n")
    table(("Case", "Pixels", "Native", "Image", "Image/native", "Native result"), (
        (case, native[case]["pixels"], native[case][args.metric], image[case][args.metric], formatted(value),
         "slower" if value < 1 else "equal" if value == 1 else "faster")
        for case, value in comparisons
    ))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("before", nargs="?")
    parser.add_argument("after", nargs="?")
    parser.add_argument("--reference", metavar="TSV", help="analyze one file against image instead of comparing stages")
    parser.add_argument("--native-mode", choices=("native-rgba", "native-owned-rgba"), default="native-rgba",
                        help="native RGBA ownership contract used in reference comparisons")
    parser.add_argument("--metric", choices=METRICS, default="thread_cycles")
    parser.add_argument("--mode", action="append", help="restrict stage output to this mode; repeatable")
    parser.add_argument("--case", default="", help="case-name substring filter, applied after complete key validation")
    parser.add_argument("--details", action="store_true", help="include every exact before/after measurement")
    parser.add_argument("--skip-reference", action="store_true", help="omit after-stage RGBA/image comparison")
    parser.add_argument("--alloc-before")
    parser.add_argument("--alloc-after")
    args = parser.parse_args()
    if args.reference:
        if args.before or args.after or args.alloc_before or args.alloc_after or args.skip_reference:
            parser.error("--reference cannot be combined with stage paths, allocations, or --skip-reference")
        reference_report(load(args.reference), args)
        return
    if not args.before or not args.after:
        parser.error("supply BEFORE AFTER, or --reference TSV")
    if bool(args.alloc_before) != bool(args.alloc_after):
        parser.error("--alloc-before and --alloc-after must be supplied together")
    before, after = load(args.before), load(args.after)
    same_keys(before, after, "timing stages")
    print(f"Inputs: `{args.before}` -> `{args.after}`. Complete key sets match ({len(before)} rows).\n")
    paired_report(before, after, args, args.metric, "Stage comparison")
    if not args.skip_reference:
        reference_report(after, args)
    if args.alloc_before:
        old_alloc, new_alloc = load(args.alloc_before), load(args.alloc_after)
        same_keys(old_alloc, new_alloc, "allocation stages")
        for allocation, timing, label in ((old_alloc, before, "before"), (new_alloc, after, "after")):
            if not set(allocation).issubset(timing):
                raise ValueError(f"{label}: allocation measurements contain unknown timing keys")
            same_keys(allocation, {key: timing[key] for key in allocation}, f"{label} allocation metadata")
        print(f"Allocation subset: {len(old_alloc)} matching keys, each present in both timing datasets.\n")
        for metric in ("alloc_calls", "requested_bytes"):
            paired_report(old_alloc, new_alloc, args, metric, "Allocation comparison")


if __name__ == "__main__":
    try:
        main()
    except (OSError, UnicodeError, ValueError, InvalidOperation) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(2)
