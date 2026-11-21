import datetime
import difflib
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

task = Path(__file__).resolve().parent
root = task.parents[1]
label, parent = sys.argv[1:]
assert label.replace("-", "").isalnum() and parent.replace("-", "").isalnum()
stage = task / label
stage.mkdir()
baseline = json.loads((task / "baseline/identity.json").read_text(encoding="utf-8"))
source_files = []
parent_delta = []
for original in baseline["sources"]:
    source = root / original["path"]
    destination = stage / "sources" / original["path"]
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    raw = source.read_bytes()
    source_files.append({"path": original["path"], "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()})
    parent_source = task / parent / "sources" / original["path"]
    if original["path"].startswith("crates/deadlib-image/bmp/") and parent_source.read_bytes() != raw:
        parent_delta.extend(difflib.unified_diff(parent_source.read_text(encoding="utf-8").splitlines(keepends=True), source.read_text(encoding="utf-8").splitlines(keepends=True), fromfile=f"a/{original['path']}", tofile=f"b/{original['path']}"))
input_hashes = []
for original in baseline["inputs"]:
    raw = (root / original["path"]).read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    assert digest == original["sha256"] and len(raw) == original["bytes"], original["path"]
    input_hashes.append(original)
patch = subprocess.check_output(["git", "diff", "--binary", "--no-ext-diff", "--", "crates/deadlib-image/bmp"], cwd=root)
(stage / "changes.patch").write_bytes(patch)
(stage / "parent-delta.patch").write_text("".join(parent_delta), encoding="utf-8", newline="\n")
identity = {
    "captured_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "label": label,
    "parent_stage": parent,
    "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
    "status": "source prepared; build, correctness, allocation, and timing validation pending",
    "sources": source_files,
    "inputs": input_hashes,
    "patch_sha256": hashlib.sha256(patch).hexdigest(),
    "patch_base": "repository HEAD; parent-delta.patch compares the named parent stage",
}
control_manifest = task / "alpha-controls/manifest.json"
if control_manifest.exists():
    identity["alpha_controls"] = json.loads(control_manifest.read_text(encoding="utf-8"))
    identity["alpha_control_manifest_sha256"] = hashlib.sha256(control_manifest.read_bytes()).hexdigest()
(stage / "identity.json").write_text(json.dumps(identity, indent=2) + "\n", encoding="utf-8", newline="\n")
changed = [entry["path"] for entry, original in zip(source_files, baseline["sources"]) if entry["sha256"] != original["sha256"]]
print(json.dumps({"stage": label, "parent": parent, "changed_since_baseline": changed, "input_hashes_unchanged": len(input_hashes), "patch_sha256": identity["patch_sha256"]}, indent=2))
