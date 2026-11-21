"""Independently recompute final candidate statistics from raw TSVs; no timing."""
from datetime import datetime, timezone
import csv
import json
import math
from pathlib import Path

task = Path(__file__).resolve().parent
checked = 0

def close(actual, expected, label):
    global checked
    checked += 1
    if expected is None:
        assert actual is None, (label, actual, expected)
    else:
        assert math.isclose(actual, expected, rel_tol=1e-10, abs_tol=1e-7), (label, actual, expected)

def table(path):
    with path.open(encoding="utf-8-sig", newline="") as source:
        rows = list(csv.DictReader(source, delimiter="\t"))
    return {(r["case"], r["mode"]): r for r in rows}

def stats(actual, rows, label):
    ratios = [r["after_cycles"] / r["before_cycles"] for r in rows]
    assert actual["cases"] == len(rows), label
    assert actual["wins"] == sum(x < 1 for x in ratios), label
    assert actual["losses"] == sum(x > 1 for x in ratios), label
    close(actual["geomean"], math.exp(math.fsum(map(math.log, ratios)) / len(rows)) if rows else None, label + " geometric")
    for field, key in [("before_cycles", "before_cycles"), ("after_cycles", "after_cycles")]:
        close(actual[field], math.fsum(r[key] for r in rows), label + field)
    before = math.fsum(r["before_cycles"] for r in rows)
    after = math.fsum(r["after_cycles"] for r in rows)
    bns = math.fsum(r["before_ns"] for r in rows)
    ans = math.fsum(r["after_ns"] for r in rows)
    pixels = sum(r["pixels"] for r in rows)
    close(actual["cycle_sum_ratio"], after / before if rows else None, label + " cycle sum")
    close(actual["ns_sum_ratio"], ans / bns if rows else None, label + " ns sum")
    close(actual["before_mpix_s"], 1000 * pixels / bns if rows else None, label + " before throughput")
    close(actual["after_mpix_s"], 1000 * pixels / ans if rows else None, label + " after throughput")

record_count = 0
for family, modes in [("matrix", ("decode", "owned", "rgba", "corpus")), ("alpha", ("decode", "owned"))]:
    prefix = "candidate5-baseline-" + family
    summary = json.loads((task / (prefix + "-summary.json")).read_text(encoding="utf-8"))
    for mode in modes:
        m = summary["modes"][mode]
        before = table(task / (prefix + "-before-" + mode + ".tsv"))
        after = table(task / (prefix + "-after-" + mode + ".tsv"))
        assert before.keys() == after.keys()
        rows = m["records"]
        assert len(rows) == len(before) == m["rows"]
        assert {(r["case"], r["mode"]) for r in rows} == set(before)
        for row in rows:
            key = (row["case"], row["mode"])
            assert row["pixels"] == int(before[key]["pixels"]) == int(after[key]["pixels"])
            assert row["image_equivalent"] == (before[key]["image_equivalent"] == "true")
            for when, raw in [("before", before[key]), ("after", after[key])]:
                for field, raw_key in [("cycles", "thread_cycles"), ("ns", "ns"), ("min", "cycles_min"), ("max", "cycles_max")]:
                    close(row[when + "_" + field], float(raw[raw_key]), str(key) + when + field)
            close(row["after_over_before"], row["after_cycles"] / row["before_cycles"], str(key) + " ratio")
            close(row["ns_after_over_before"], row["after_ns"] / row["before_ns"], str(key) + " ns ratio")
        record_count += len(rows)
        native = [r for r in rows if r["mode"].startswith("native")]
        reference = [r for r in rows if not r["mode"].startswith("native")]
        stats(m["native"], native, prefix + mode + " native")
        stats(m["reference"], reference, prefix + mode + " reference")
        if family == "matrix":
            stats(m["targets"], [r for r in native if r["target"]], mode + " targets")
            stats(m["controls"], [r for r in native if not r["target"]], mode + " controls")
            for layout, group in m["groups"].items():
                stats(group["native"], [r for r in native if r["layout"] == layout], mode + layout)
                stats(group["reference"], [r for r in reference if r["layout"] == layout], mode + layout + " reference")
            for size, group in m["control_sizes"].items():
                stats(group, [r for r in native if not r["target"] and r["size"] == size], mode + size)
        else:
            for alpha, group in m["by_alpha"].items():
                stats(group, [r for r in native if r["alpha"] == alpha], mode + alpha)
            for size, group in m["by_size"].items():
                stats(group, [r for r in native if r["size"] == size], mode + size)
    process = summary["process_checks"]
    assert not process["external"] and not process["last"]["own_pids"]
assert record_count == 622
result = {"captured_utc": datetime.now(timezone.utc).isoformat(), "status": "passed",
          "raw_before_after_record_pairs": record_count, "numeric_checks": checked,
          "scope": "Every final C5 native/reference median, range, cycle/ns ratio, group sum, geometric ratio, throughput and cohort count independently recalculated from raw TSVs."}
with (task / "candidate5-summary-audit.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
print(json.dumps(result, indent=2))
