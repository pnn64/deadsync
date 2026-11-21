import argparse
import contextlib
import importlib.util
import io
import json
import math
from pathlib import Path
import sys

task = Path(__file__).resolve().parent
root = task.parents[1]
label = sys.argv[1]
spec = importlib.util.spec_from_file_location("bmp_analysis", root / "crates/deadlib-image/bmp/bench/analyze.py")
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)


def geomean(values):
    return math.exp(math.fsum(math.log(value) for value in values) / len(values))


result = {}
for mode, native_mode, count in (("decode", "native-decode", 18), ("owned", "native-owned-rgba", 24)):
    before = analysis.load(task / f"{label}-before-{mode}.tsv")
    after = analysis.load(task / f"{label}-after-{mode}.tsv")
    analysis.same_keys(before, after, mode)
    assert len(before) == len(after) == count
    args = argparse.Namespace(mode=None, case="", metric="thread_cycles", details=True, native_mode=native_mode)
    stream = io.StringIO()
    with contextlib.redirect_stdout(stream):
        analysis.paired_report(before, after, args, "thread_cycles", "Before/after alpha controls")
        if mode == "owned":
            analysis.reference_report(after, args)
    (task / f"{label}-{mode}-comparison.md").write_text(stream.getvalue(), encoding="utf-8", newline="\n")
    entries = []
    references = []
    for key in sorted(before):
        old, new = before[key], after[key]
        for entry in (old, new):
            assert float(entry["cycles_min"]) <= float(entry["thread_cycles"]) <= float(entry["cycles_max"])
        row = {
            "case": key[0].replace("\\", "/").rsplit("/", 1)[-1],
            "mode": key[1],
            "before_cycles": float(old["thread_cycles"]),
            "after_cycles": float(new["thread_cycles"]),
            "after_over_before": float(new["thread_cycles"]) / float(old["thread_cycles"]),
            "ns_after_over_before": float(new["ns"]) / float(old["ns"]),
        }
        (entries if key[1] == native_mode else references).append(row)
    groups = {}
    for scenario in ("zero", "first", "last"):
        for orientation in ("top", "bottom"):
            selected = [row for row in entries if f"bgra32-{scenario}-" in row["case"] and row["case"].endswith(f"-{orientation}.bmp")]
            assert len(selected) == 3
            groups[f"{scenario}-{orientation}"] = geomean([row["after_over_before"] for row in selected])
    result[mode] = {
        "all_native_after_over_before": geomean([row["after_over_before"] for row in entries]),
        "groups_after_over_before": groups,
        "native_losses": [row for row in entries if row["after_over_before"] > 1],
        "native_cases": entries,
        "reference_after_over_before": geomean([row["after_over_before"] for row in references]) if references else None,
        "reference_cases": references,
    }
with (task / f"{label}-summary.json").open("x", encoding="utf-8", newline="\n") as destination:
    json.dump(result, destination, indent=2)
    destination.write("\n")
for mode, data in result.items():
    print(json.dumps({"mode": mode, **{key: value for key, value in data.items() if key not in ("native_cases", "native_losses", "reference_cases")}}, indent=2))
    print("case\tbefore_cycles\tafter_cycles\tafter/before")
    for row in data["native_cases"] + data["reference_cases"]:
        print(f"{row['case']} ({row['mode']})\t{row['before_cycles']:.2f}\t{row['after_cycles']:.2f}\t{row['after_over_before']:.6f}")
