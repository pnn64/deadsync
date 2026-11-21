import datetime
import hashlib
import json
from pathlib import Path
import sys

task = Path(__file__).resolve().parent
root = task.parents[1]
label, phase = sys.argv[1:]
identity = json.loads((task / label / "identity.json").read_text(encoding="utf-8"))
for group in ("sources", "inputs"):
    for entry in identity[group]:
        raw = (root / entry["path"]).read_bytes()
        assert len(raw) == entry["bytes"] and hashlib.sha256(raw).hexdigest() == entry["sha256"], entry["path"]
controls = json.loads((task / "alpha-controls/manifest.json").read_text(encoding="utf-8"))
if "alpha_controls" in identity:
    assert controls == identity["alpha_controls"]
    assert hashlib.sha256((task / "alpha-controls/manifest.json").read_bytes()).hexdigest() == identity["alpha_control_manifest_sha256"]
for entry in controls:
    raw = (task / "alpha-controls" / entry["file"]).read_bytes()
    assert len(raw) == entry["input_bytes"] and hashlib.sha256(raw).hexdigest() == entry["input_sha256"], entry["file"]
result = {
    "captured_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "stage": label,
    "phase": phase,
    "source_config_files_verified": len(identity["sources"]),
    "repository_inputs_verified": len(identity["inputs"]),
    "alpha_controls_verified": controls,
}
with (task / label / f"source-{phase}.json").open("x", encoding="utf-8", newline="\n") as destination:
    json.dump(result, destination, indent=2)
    destination.write("\n")
print(f"Verified {label}: {len(identity['sources'])} source/config files, {len(identity['inputs'])} repository inputs, {len(controls)} alpha controls ({phase}).")
