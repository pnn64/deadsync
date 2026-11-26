"""Validate repeated measurements and report exact-output, matched-case comparisons."""

import argparse
import csv
import gzip
import io
from collections import Counter, defaultdict
from pathlib import Path
from statistics import geometric_mean, median


def read(path):
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, mode="rt", encoding="utf-8-sig", newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def bundle(work, destination):
    destination.mkdir(parents=True, exist_ok=True)
    for mode in ("decode", "encode"):
        rows = [dict(row, round=run) for run in (1, 2) for api in ("", "-python")
                for row in read(work / f"{mode}{api}-{run}.tsv")]
        for row in rows:
            if row["codec"] == "pillow" and row["status"].startswith("cannot identify image file <_io.BytesIO object at "):
                row["status"] = "cannot identify BMP input"
        with (destination / f"{mode}.tsv.gz").open("wb") as target:
            with gzip.GzipFile(fileobj=target, mode="wb", mtime=0) as compressed:
                with io.TextIOWrapper(compressed, encoding="utf-8", newline="") as stream:
                    writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
                    writer.writeheader()
                    writer.writerows(rows)
    indexed = (work / "indexed.tsv").read_text(encoding="utf-8-sig")
    (destination / "indexed.tsv").write_text(indexed, encoding="utf-8", newline="\n")


def results(rows):
    runs = defaultdict(dict)
    for row in rows:
        key = row["case"], row["codec"]
        assert row["round"] not in runs[key], key
        runs[key][row["round"]] = row
    cases = defaultdict(dict)
    for (name, codec), run in runs.items():
        assert run.keys() == {"1", "2"}, (name, codec)
        a, b = run["1"], run["2"]
        for field in a.keys() - {"ns", "cycles", "round"}:
            assert a[field] == b[field], (name, codec, field, a[field], b[field])
        combined = dict(a)
        for field in ("w", "h", "input_bytes", "output_bytes"):
            combined[field] = int(a[field])
        for field in ("ns", "cycles"):
            numbers = tuple(int(r[field]) for r in (a, b))
            assert all(n > 0 for n in numbers) if a["status"] == "ok" else numbers == (0, 0), (name, codec)
            combined[field] = geometric_mean(numbers) if a["status"] == "ok" else 0
            combined[field + "_runs"] = numbers
        cases[name][codec] = combined
    for name, case in cases.items():
        assert case.keys() == next(iter(cases.values())).keys(), name
    return cases


def exact(row):
    return row["status"] == "ok"


def pairwise(cases):
    for codec in sorted({codec for case in cases.values() for codec in case} - {"deadlib"}):
        pairs = [(case["deadlib"], case[codec]) for case in cases.values()
                 if exact(case["deadlib"]) and exact(case[codec])]
        if not pairs:
            continue
        ratios = [other["cycles"] / native["cycles"] for native, other in pairs]
        wins = sum(ratio > 1.03 for ratio in ratios)
        losses = sum(ratio < 1 / 1.03 for ratio in ratios)
        passes = [geometric_mean(other["cycles_runs"][i] / native["cycles_runs"][i]
                                 for native, other in pairs) for i in range(2)]
        speed = geometric_mean(other["ns"] / native["ns"] for native, other in pairs)
        print(f"{codec:12s} {len(pairs):4d} {wins:4d} {len(pairs)-wins-losses:4d} {losses:4d} "
              f"{geometric_mean(ratios):6.3f}x {speed:6.3f}x "
              f"({passes[0]:.3f}x, {passes[1]:.3f}x)")


def cohort(cases, names):
    measured = [cases[name] for name in names]
    if not measured:
        return
    common = set.intersection(*({codec for codec, r in c.items() if exact(r)} for c in measured))
    if not common:
        print("   no codec has exact output on every case")
        return
    means = []
    for codec in common:
        rates = (case[codec]["w"] * case[codec]["h"] * 1000 / case[codec]["ns"] for case in measured)
        means.append((geometric_mean(rates), codec))
    print("   " + "  ".join(f"{codec} {rate:,.1f}" for rate, codec in sorted(means, reverse=True)))


def rankings(cases):
    counts = Counter()
    skips = 0
    for case in cases.values():
        if not exact(case["deadlib"]):
            continue
        comparable = [r for r in case.values() if exact(r)]
        if len(comparable) <= 1:
            skips += 1
            continue
        best = min(comparable, key=lambda r: r["cycles"])
        counts[best["codec"]] += 1
    print(f"Lowest cycle counts on Deadlib-comparable cases: {counts.most_common()}; {skips} without rivals")


def summarize(mode, cases):
    print(f"\n{mode.upper()} ({len(cases)} cases)")
    origins = ["synthetic", "bmpsuite", "rle", "kodak"] if mode == "decode" else ["synthetic", "kodak"]
    for codec in sorted(next(iter(cases.values())).keys()):
        coverage = [sum(exact(case[codec]) for case in cases.values() if case[codec]["origin"] == origin)
                    for origin in origins]
        print(f"{codec:12s} {sum(coverage):4d} exact {dict(zip(origins, coverage))}")
    print("Pairs  /  Deadlib wins, within 3%, loses  /  cycles, elapsed, cycle ratio each round")
    pairwise(cases)
    rankings(cases)
    groups = defaultdict(list)
    for name, case in cases.items():
        native = case["deadlib"]
        group = native["origin"], native["group"]
        if mode == "encode" and native["origin"] == "synthetic":
            group += (native["wire"].split("/")[-1],)
        if mode != "decode" or native["w"] * native["h"] >= 100_000:
            groups[group].append(name)
    print("Group throughput in MP/s, requiring exact results on the full cohort")
    for group, names in sorted(groups.items()):
        print(*group, f"({len(names)})")
        cohort(cases, names)
    if mode == "decode":
        print("All-codec common corpus")
        common = {name: c for name, c in cases.items() if all(exact(r) for r in c.values())}
        print(f"{len(common)} cases, {sum(c['deadlib']['w']*c['deadlib']['h'] for c in common.values())} pixels")
        totals = {codec: sum(c[codec]["cycles"] for c in common.values()) for codec in next(iter(cases.values()))}
        print("Total cycles, relative to Deadlib:", sorted(((v / totals["deadlib"], k) for k, v in totals.items())))
        print("Median larger-case run-to-run ratio:", median([
            max(r["cycles_runs"]) / min(r["cycles_runs"]) for c in cases.values() for r in c.values()
            if exact(r) and r["w"] * r["h"] >= 100_000]))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("results", type=Path)
    ap.add_argument("--bundle-from", type=Path)
    args = ap.parse_args()
    if args.bundle_from:
        bundle(args.bundle_from, args.results)
    for mode in ("decode", "encode"):
        summarize(mode, results(read(args.results / f"{mode}.tsv.gz")))


if __name__ == "__main__":
    main()
