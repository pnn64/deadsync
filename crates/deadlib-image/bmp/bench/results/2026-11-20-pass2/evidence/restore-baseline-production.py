"""Restore only the BMP decoder, guarded by the complete frozen candidate source identity."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path

task = Path(__file__).resolve().parent
root = task.parents[1]
output_path = task / "restored-baseline-source.json"
assert not output_path.exists(), "Never replay a completed restoration"
baseline = json.loads((task / "baseline/identity.json").read_text(encoding="utf-8"))
candidate = json.loads((task / "candidate5-two-layouts/identity.json").read_text(encoding="utf-8"))
decode = "crates/deadlib-image/bmp/src/decode.rs"
retained_tests = "crates/deadlib-image/bmp/tests/bmp_optimized.rs"

def identity(path):
    data = path.read_bytes()
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}

# The full small source/config identity guards against overwriting any new work.
before = []
for entry in candidate["sources"]:
    found = identity(root / entry["path"])
    assert found == {key: entry[key] for key in ("bytes", "sha256")}, entry["path"]
    before.append({"path": entry["path"], **found})
assert next(row for row in before if row["path"] == decode)["sha256"] == "be772c50798d8e90e196c75ee42434aca65a28135a65cf183b2eef5a887e599e"
original_entry = next(row for row in baseline["sources"] if row["path"] == decode)
original_path = task / "baseline/sources" / decode
assert identity(original_path) == {key: original_entry[key] for key in ("bytes", "sha256")}

# This is the only production file written. Frozen candidates remain unchanged.
(root / decode).write_bytes(original_path.read_bytes())

candidate_by_path = {row["path"]: row for row in candidate["sources"]}
after = []
for entry in baseline["sources"]:
    path = entry["path"]
    expected = candidate_by_path[path] if path == retained_tests else entry
    found = identity(root / path)
    assert found == {key: expected[key] for key in ("bytes", "sha256")}, path
    after.append({"path": path, **found, "matches": "candidate5 retained regression tests" if path == retained_tests else "original baseline"})
production = [row for row in after if row["path"].startswith("crates/deadlib-image/bmp/src/")]
assert len(production) == 4 and all(row["matches"] == "original baseline" for row in production)
result = {
    "captured_utc": datetime.now(timezone.utc).isoformat(),
    "decision": "All BMP production candidates rejected; zero production optimizations retained.",
    "production_restored_from": "baseline/sources",
    "only_written_production_file": decode,
    "before_source_config": before,
    "after_source_config": after,
    "production_sources": production,
    "retained_regression_tests": retained_tests,
    "source_config_files_checked": len(after),
    "baseline_matching_files": sum(row["matches"] == "original baseline" for row in after),
    "tests_status": "Final tests/fmt/Clippy pending root compute grant; no timing or build ran during restoration.",
    "snapshot_rationale": "All production and benchmark source/config inputs match the original frozen baseline. The added integration tests do not alter its code. Existing baseline snapshot and executable identities remain the applicable behavior oracle; no rebuild or new timing is needed.",
}
with output_path.open("x", encoding="utf-8", newline="\n") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
print(json.dumps({"decision": result["decision"], "checked": len(after), "baseline_matches": result["baseline_matching_files"], "production": production, "retained_tests": retained_tests}, indent=2))
