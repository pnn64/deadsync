"""Check the 1% cycle target on each byte-exact Deadlib and Wuffs pair."""

import argparse
from collections import defaultdict
from pathlib import Path
from statistics import median

from summarize import read


def run_ids(paths):
    runs = defaultdict(dict)
    for file in paths:
        for row in read(file):
            key = row["case"], row["codec"]
            run = runs[file, row.get("round", "1")]
            if key in run:
                raise ValueError(f"Duplicate measurement for {key} in {file}")
            run[key] = row
    return list(runs.values())


def ratios(runs):
    expected = runs[0]
    for run in runs:
        if run.keys() != expected.keys():
            raise ValueError("The runs have different cases or codecs")
        for key, row in run.items():
            fields = row.keys() - {"ns", "cycles", "round"}
            if any(row[f] != expected[key][f] for f in fields):
                raise ValueError(f"Verification changed for {key}")

    cases = {name for name, codec in expected if codec == "deadlib-rgba"}
    matched = []
    for name in sorted(cases):
        direct, staged, rival = [(name, codec) for codec in ("deadlib-rgba", "deadlib", "wuffs")]
        if expected[direct]["status"] != expected[staged]["status"]:
            raise ValueError(f"The two Deadlib paths disagree for {name}")
        if expected[direct]["status"] != "ok" or expected[rival]["status"] != "ok":
            continue
        pairs = [(int(run[direct]["cycles"]), int(run[rival]["cycles"])) for run in runs]
        if any(min(pair) <= 0 for pair in pairs):
            raise ValueError(f"Missing cycle measurement for {name}")
        values = [a / b for a, b in pairs]
        matched.append((name, median(values), values))
    return len(cases), matched


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runs", type=Path, nargs="+")
    args = parser.parse_args()
    runs = run_ids(args.runs)
    cases, matched = ratios(runs)
    over = sorted((r for r in matched if r[1] > 1.01), key=lambda r: r[1], reverse=True)
    print(f"{len(matched) - len(over)}/{len(matched)} exact pairs at or below 1.01x Wuffs cycles "
          f"({cases} inputs, {len(runs)} paired passes)")
    if matched:
        name, ratio, _ = max(matched, key=lambda r: r[1])
        print(f"Highest median ratio: {ratio:.4f}x ({name})")
    for name, ratio, values in over:
        print(f"{name:36} {ratio:.4f}x  [{', '.join(f'{v:.3f}' for v in values)}]")
    raise SystemExit(not matched or bool(over))


if __name__ == "__main__":
    main()
