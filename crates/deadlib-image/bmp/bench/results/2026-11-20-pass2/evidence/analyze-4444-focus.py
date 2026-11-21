import argparse
import contextlib
import importlib.util
import io
import json
import math
from pathlib import Path

task = Path(__file__).resolve().parent
root = task.parents[1]
spec = importlib.util.spec_from_file_location("bmp_analysis", root / "crates/deadlib-image/bmp/bench/analyze.py")
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)
label = "candidate2-4444-focus"
layouts = ("rgb555", "rgb565", "rgb333", "rgba4444")


def geomean(values):
    return math.exp(math.fsum(math.log(value) for value in values) / len(values)) if values else None


result = {"parent": "candidate1-logical", "candidate": "candidate2-rgba4444", "parent_decision": "conditional; see logical-alpha controls"}
for mode, native_mode, count in (("decode", "native-decode", 32), ("owned", "native-owned-rgba", 50)):
    tables = {}
    for when in ("before", "after"):
        combined = {}
        for layout in layouts:
            path = task / f"{label}-{when}-{mode}-{layout}.tsv"
            part = analysis.load(path)
            assert not set(combined).intersection(part)
            combined.update(part)
            metadata = json.loads(path.with_suffix(".json").read_text(encoding="utf-8-sig"))
            assert metadata["exit_code"] == 0 and metadata["milliseconds"] == 80 and metadata["samples"] == 5 and metadata["cpu"] == 2
        tables[when] = combined
    before, after = tables["before"], tables["after"]
    analysis.same_keys(before, after, mode)
    assert len(before) == len(after) == count
    args = argparse.Namespace(mode=None, case="", metric="thread_cycles", details=True, native_mode=native_mode)
    stream = io.StringIO()
    with contextlib.redirect_stdout(stream):
        analysis.paired_report(before, after, args, "thread_cycles", "Before/after 16-bit focus")
        if mode == "owned":
            analysis.reference_report(after, args)
    with (task / f"{label}-{mode}-comparison.md").open("x", encoding="utf-8", newline="\n") as output:
        output.write(stream.getvalue())
    entries, references = [], []
    for key in sorted(before):
        old, new = before[key], after[key]
        for entry in (old, new):
            assert float(entry["cycles_min"]) <= float(entry["thread_cycles"]) <= float(entry["cycles_max"])
        row = {
            "case": key[0],
            "mode": key[1],
            "before_cycles": float(old["thread_cycles"]),
            "after_cycles": float(new["thread_cycles"]),
            "after_over_before": float(new["thread_cycles"]) / float(old["thread_cycles"]),
            "ns_after_over_before": float(new["ns"]) / float(old["ns"]),
        }
        (entries if key[1] == native_mode else references).append(row)
    groups = {}
    for layout in layouts:
        native = [row for row in entries if row["case"].startswith(layout + "-")]
        reference = [row for row in references if row["case"].startswith(layout + "-")]
        assert len(native) == 8
        groups[layout] = {
            "native_after_over_before": geomean([row["after_over_before"] for row in native]),
            "reference_after_over_before": geomean([row["after_over_before"] for row in reference]),
            "native_losses": [row for row in native if row["after_over_before"] > 1],
        }
    result[mode] = {
        "all_native_after_over_before": geomean([row["after_over_before"] for row in entries]),
        "unchanged_native_controls_after_over_before": geomean([row["after_over_before"] for row in entries if not row["case"].startswith("rgba4444-")]),
        "reference_after_over_before": geomean([row["after_over_before"] for row in references]),
        "groups": groups,
        "native_cases": entries,
        "reference_cases": references,
        "native_losses": [row for row in entries if row["after_over_before"] > 1],
    }
with (task / f"{label}-summary.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
for mode in ("decode", "owned"):
    data = result[mode]
    print(json.dumps({"mode": mode, **{key: value for key, value in data.items() if key not in ("native_cases", "reference_cases", "native_losses")}}, indent=2))
    print("case\tbefore_cycles\tafter_cycles\tafter/before")
    for row in data["native_cases"] + data["reference_cases"]:
        print(f"{row['case']} ({row['mode']})\t{row['before_cycles']:.2f}\t{row['after_cycles']:.2f}\t{row['after_over_before']:.6f}")

