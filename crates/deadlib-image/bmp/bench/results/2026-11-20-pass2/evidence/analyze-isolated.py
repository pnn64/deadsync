import csv
import hashlib
import json
import math
from pathlib import Path

task = Path(__file__).resolve().parent
jobs = json.loads((task / "alpha-isolated-commands.json").read_text(encoding="utf-8"))
rows = []
for job in jobs:
    args = job["command"]
    stage = Path(args[args.index("-Executable") + 1]).parent.name
    name = job["name"]
    data = list(csv.DictReader((task / f"{name}.tsv").open(encoding="utf-8-sig", newline=""), delimiter="\t"))
    assert len(data) == 1
    row = data[0]
    meta = json.loads((task / f"{name}.json").read_text(encoding="utf-8-sig"))
    assert meta["exit_code"] == 0 and meta["milliseconds"] == 80 and meta["samples"] == 5 and meta["cpu"] == 2
    assert row["image_equivalent"] == "false"
    rows.append({
        "run": name,
        "stage": stage,
        "orientation": "bottom" if "-bottom.bmp" in row["case"] else "top",
        "mode": row["mode"],
        "cycles": float(row["thread_cycles"]),
        "min": float(row["cycles_min"]),
        "max": float(row["cycles_max"]),
        "ns": float(row["ns"]),
        "executable_sha256": meta["executable_sha256"],
    })


def geomean(values):
    return math.exp(math.fsum(math.log(value) for value in values) / len(values))


groups = []
for orientation in ("bottom", "top"):
    for mode in ("native-decode", "native-owned-rgba"):
        stages = {}
        for stage in ("baseline", "candidate1-logical"):
            selected = [r for r in rows if r["orientation"] == orientation and r["mode"] == mode and r["stage"] == stage]
            assert len(selected) == 2 and len({r["executable_sha256"] for r in selected}) == 1
            values = [r["cycles"] for r in selected]
            stages[stage] = {"cycles": values, "geomean_cycles": geomean(values), "max_over_min": max(values) / min(values)}
        groups.append({
            "orientation": orientation,
            "mode": mode,
            "stages": stages,
            "logical_over_baseline": stages["candidate1-logical"]["geomean_cycles"] / stages["baseline"]["geomean_cycles"],
        })
monitor = []
for label in ("candidate1-logical-controls", "candidate2-4444-focus", "alpha-isolated"):
    path = task / f"{label}-processes.jsonl"
    records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    assert records and not any(record["external"] for record in records)
    monitor.append({"path": path.name, "checks": len(records), "external_processes": 0, "first_utc": records[0]["utc"], "last_utc": records[-1]["utc"], "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
summary = {"policy": "all independent process medians retained; stage comparison uses the geometric mean of both runs, never the fastest", "groups": groups, "runs": rows, "process_monitor": monitor}
with (task / "alpha-isolated-summary.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(summary, output, indent=2)
    output.write("\n")
print(json.dumps({"groups": groups, "process_monitor": monitor}, indent=2))
print("run\tmedian_cycles\tmin\tmax")
for row in rows:
    print(f"{row['run']}\t{row['cycles']:.2f}\t{row['min']:.2f}\t{row['max']:.2f}")

