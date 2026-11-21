import hashlib
import importlib.util
import json
from pathlib import Path
import sys

task = Path(__file__).resolve().parent
root = task.parents[1]
spec = importlib.util.spec_from_file_location("bmp_analysis", root / "crates/deadlib-image/bmp/bench/analyze.py")
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)
candidate = sys.argv[1] if len(sys.argv) > 1 else "candidate1-alpha"
result = {"candidate": candidate, "timing_status": "not run", "allocation_pairs": {}}
for suffix, count in (("decode", 120), ("owned-rgba", 218), ("rgba", 218), ("corpus", 24), ("alpha-controls-decode", 18), ("alpha-controls-owned-rgba", 24)):
    before = analysis.load(task / f"baseline-{suffix}-allocations.tsv")
    after = analysis.load(task / f"{candidate}-{suffix}-allocations.tsv")
    analysis.same_keys(before, after, suffix)
    assert len(before) == len(after) == count
    changes = []
    for key in before:
        for rows in (before, after):
            for metric in ("ns", "thread_cycles", "cycles_min", "cycles_max", "mpixels_sec"):
                assert float(rows[key][metric]) == 0, (suffix, key, metric)
        for metric in ("alloc_calls", "requested_bytes"):
            if before[key][metric] != after[key][metric]:
                changes.append({"case": key[0], "mode": key[1], "metric": metric, "before": before[key][metric], "after": after[key][metric]})
    result["allocation_pairs"][suffix] = {"rows": count, "changes": changes}
    assert not changes, (suffix, changes)
executables = json.loads((task / candidate / "executables.json").read_text(encoding="utf-8"))
for entry in executables:
    raw = (task / candidate / entry["path"]).read_bytes()
    assert len(raw) == entry["bytes"] and hashlib.sha256(raw).hexdigest() == entry["sha256"]
result["executables"] = executables
post = json.loads((task / candidate / "source-after-build.json").read_text(encoding="utf-8"))
result["source_config_hashes_verified"] = post["source_config_files_verified"]
result["repository_input_hashes_verified"] = post["repository_inputs_verified"]
result["alpha_control_hashes_verified"] = len(post["alpha_controls_verified"])
with (task / f"{candidate}-preparation-summary.json").open("x", encoding="utf-8", newline="\n") as destination:
    json.dump(result, destination, indent=2)
    destination.write("\n")
print(json.dumps(result, indent=2))
