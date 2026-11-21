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
baseline_exe = json.loads((task / "baseline/executables.json").read_text(encoding="utf-8"))[0]
result = {}
for mode, expected_rows, native_mode in (
    ("decode", 120, "native-decode"),
    ("owned-rgba", 218, "native-owned-rgba"),
    ("rgba", 218, "native-rgba"),
    ("corpus", 24, "native-owned-rgba"),
):
    rows = analysis.load(task / f"baseline-{mode}.tsv")
    allocations = analysis.load(task / f"baseline-{mode}-allocations.tsv")
    analysis.same_keys(rows, allocations, f"{mode} timing/allocation")
    assert len(rows) == expected_rows, (mode, len(rows))
    provenance = json.loads((task / f"baseline-{mode}.json").read_text(encoding="utf-8"))
    assert provenance["executable_sha256"] == baseline_exe["sha256"]
    assert (provenance["milliseconds"], provenance["samples"], provenance["cpu"], provenance["sizes"], provenance["exit_code"]) == (20, 5, 2, "", 0)
    for row in rows.values():
        assert float(row["thread_cycles"]) > 0 and float(row["ns"]) > 0
        assert float(row["cycles_min"]) <= float(row["thread_cycles"]) <= float(row["cycles_max"])
    native = {case: row for (case, operation), row in rows.items() if operation == native_mode}
    reference = {case: row for (case, operation), row in rows.items() if operation == "image-rgba"}
    entry = {"rows": len(rows), "native_cases": len(native), "reference_cases": len(reference)}
    if reference:
        assert set(reference) == {case for case, row in native.items() if row["image_equivalent"] == "true"}
        for metric in ("thread_cycles", "ns"):
            pairs = [
                {"case": case, "native": float(native[case][metric]), "image": float(reference[case][metric]), "image_over_native": float(reference[case][metric]) / float(native[case][metric])}
                for case in sorted(reference)
            ]
            entry[metric] = {
                "image_over_native_geomean": math.exp(math.fsum(math.log(pair["image_over_native"]) for pair in pairs) / len(pairs)),
                "wins": sum(pair["image_over_native"] > 1 for pair in pairs),
                "ties": sum(pair["image_over_native"] == 1 for pair in pairs),
                "losses": [pair for pair in pairs if pair["image_over_native"] < 1],
                "worst": min(pairs, key=lambda pair: pair["image_over_native"]),
            }
        stream = io.StringIO()
        with contextlib.redirect_stdout(stream):
            analysis.reference_report(rows, argparse.Namespace(native_mode=native_mode, metric="thread_cycles", case=""))
        (task / f"baseline-{mode}-reference.md").write_text(stream.getvalue(), encoding="utf-8", newline="\n")
    result[mode] = entry
for boundary in ("before", "after"):
    record = json.loads((task / f"baseline-boundary-{boundary}.json").read_text(encoding="utf-8"))
    assert record["processes"] == [], (boundary, record["processes"])
with (task / "baseline-summary.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
print(json.dumps(result, indent=2))
