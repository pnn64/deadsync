import csv, sys, statistics as s
from collections import defaultdict
from pathlib import Path

for filename in sys.argv[1:]:
    print(Path(filename).name)
    with open(filename, newline="") as f:
        rows = list(csv.DictReader(f))
    cases = defaultdict(list)
    for row in rows:
        cases[row["case"], row["mode"]].append(row)
    for (clip, mode), vals in cases.items():
        stages = list(dict.fromkeys(r["stage"] for r in vals))
        if len(stages) != 2:
            continue
        cyc = [{r["round"]: int(r["cycles"])/int(r["repeats"]) for r in vals if r["stage"]==stage and r["kind"]=="cycles"} for stage in stages]
        pairs = sorted(set(cyc[0]) & set(cyc[1]))
        if not pairs:
            continue
        ratios = [cyc[1][k]/cyc[0][k] for k in pairs]
        av = [s.median(c[k] for k in pairs) for c in cyc]
        allocation = [{(int(r["calls"])/int(r["repeats"]), int(r["bytes"])/int(r["repeats"])) for r in vals if r["stage"]==stage and r["kind"]=="alloc"} for stage in stages]
        units = int(vals[0]["units"])/int(vals[0]["repeats"])
        ratio = s.median(ratios)
        print(f"{clip:7s} {mode:14s} {len(pairs):2d} {av[0]:12.0f} -> {av[1]:12.0f} cyc; paired {100*(ratio-1):+6.2f}% [{100*(min(ratios)-1):+6.2f}%, {100*(max(ratios)-1):+6.2f}%]; throughput {units*1e9/av[0]:.1f} -> {units*1e9/av[1]:.1f} ({100*(1/ratio-1):+.2f}% paired); alloc {allocation[0]} -> {allocation[1]}")
