from collections import Counter
import csv
import hashlib
import json
from pathlib import Path

task = Path(__file__).resolve().parent
root = task.parents[1]
baseline = task / "baseline"
identity = json.loads((baseline / "identity.json").read_text(encoding="utf-8"))


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


for group, directory in (("sources", "sources"), ("inputs", "inputs")):
    for entry in identity[group]:
        original = root / entry["path"]
        frozen = baseline / directory / entry["path"]
        assert original.stat().st_size == frozen.stat().st_size == entry["bytes"], entry["path"]
        assert sha256(original) == sha256(frozen) == entry["sha256"], entry["path"]
executables = json.loads((baseline / "executables.json").read_text(encoding="utf-8"))
for entry in executables:
    assert sha256(baseline / entry["path"]) == entry["sha256"], entry["path"]

allocations = {}
representative = {}
for mode, expected_rows, expected_native in (
    ("decode", 120, 120),
    ("owned-rgba", 218, 120),
    ("rgba", 218, 120),
    ("corpus", 24, 12),
):
    path = task / f"baseline-{mode}-allocations.tsv"
    with path.open(encoding="utf-8-sig", newline="") as source:
        rows = list(csv.DictReader(source, delimiter="\t"))
    assert len(rows) == expected_rows, (mode, len(rows))
    assert len({(row["case"], row["mode"]) for row in rows}) == len(rows), mode
    for row in rows:
        for metric in ("ns", "thread_cycles", "cycles_min", "cycles_max", "mpixels_sec"):
            assert float(row[metric]) == 0, (mode, row["case"], metric)
    native = [row for row in rows if row["mode"].startswith("native-")]
    assert len(native) == expected_native, (mode, len(native))
    allocations[mode] = {
        "rows": len(rows),
        "native_cases": len(native),
        "native_alloc_calls_sum": sum(int(row["alloc_calls"]) for row in native),
        "native_requested_bytes_sum": sum(int(row["requested_bytes"]) for row in native),
        "native_allocation_call_distribution": dict(sorted(Counter(row["alloc_calls"] for row in native).items())),
        "reference_cases": sum(row["mode"] == "image-rgba" for row in rows),
    }
    for case in ("indexed1-1920x1080-bottom", "indexed8-1920x1080-bottom", "rgb24-1920x1080-bottom", "rgba4444-1920x1080-bottom", "rgba32-1920x1080-bottom", "bgra32-zero-1920x1080-bottom"):
        for row in rows:
            if row["case"] == case:
                representative.setdefault(case, {})[row["mode"]] = {
                    "alloc_calls": int(row["alloc_calls"]),
                    "requested_bytes": int(row["requested_bytes"]),
                }

snapshot = baseline / "native.snapshot"
snapshot_hash = sha256(snapshot)
result = {
    "head": identity["head"],
    "source_config_hashes_verified": len(identity["sources"]),
    "input_hashes_verified": len(identity["inputs"]),
    "executables": executables,
    "snapshot_bytes": snapshot.stat().st_size,
    "snapshot_sha256": snapshot_hash,
    "matches_historical_behavior_snapshot": snapshot_hash == "076e78cae341e782b60f72fa3f60c0ff85142759e4821069f1012063f37bf0df",
    "allocations": allocations,
    "representative_allocations": representative,
}
with (task / "preparation-summary.json").open("x", encoding="utf-8", newline="\n") as destination:
    json.dump(result, destination, indent=2)
    destination.write("\n")
print(json.dumps({key: value for key, value in result.items() if key != "representative_allocations"}, indent=2))
