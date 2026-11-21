"""Finalize Git byte preservation without changing any captured benchmark evidence."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess

task = Path(__file__).resolve().parent
root = task.parents[1]
archive = root / "crates/deadlib-image/bmp/bench/results/2026-11-20-pass2"
manifest_path = archive / "manifest.json"
old_bytes = manifest_path.read_bytes()
manifest = json.loads(old_bytes)
assert "git_text_conversion" not in manifest, "Finalization already completed"
previous_hash = hashlib.sha256(old_bytes).hexdigest()
existing = {row["path"]: row for row in manifest["artifacts"]}
for name, entry in existing.items():
    raw = (archive / name).read_bytes()
    if name == "evidence/archive-final.py":
        assert raw == (task / "archive-final.py").read_bytes()
    else:
        assert len(raw) == entry["bytes"] and hashlib.sha256(raw).hexdigest() == entry["sha256"], name
script_relative = "evidence/finalize-archive-text.py"
with (archive / script_relative).open("xb") as output:
    output.write(Path(__file__).read_bytes())
artifacts = []
for path in sorted(archive.rglob("*")):
    if not path.is_file() or path == manifest_path:
        continue
    name = path.relative_to(archive).as_posix()
    raw = path.read_bytes()
    source = existing.get(name, {}).get("source_task_path")
    if name == script_relative:
        source = Path(__file__).name
    artifacts.append({"path": name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest(), "source_task_path": source})
assert set(row["path"] for row in artifacts) == set(existing) | {".gitattributes", script_relative}
manifest.update({
    "finalized_utc": datetime.now(timezone.utc).isoformat(),
    "git_text_conversion": "disabled for this archive by .gitattributes: * -text",
    "previous_manifest_sha256_before_text_attribute": previous_hash,
    "files_excluding_manifest": len(artifacts),
    "bytes_excluding_manifest": sum(row["bytes"] for row in artifacts),
    "artifacts": artifacts,
})
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
for entry in artifacts:
    raw = (archive / entry["path"]).read_bytes()
    assert len(raw) == entry["bytes"] and hashlib.sha256(raw).hexdigest() == entry["sha256"], entry["path"]
paths = [str((archive / "evidence/candidate5-baseline-matrix-before-decode.tsv").relative_to(root)),
         str(manifest_path.relative_to(root))]
attributes = subprocess.check_output(["git", "check-attr", "text", "--", *paths], cwd=root, text=True)
assert len(attributes.splitlines()) == 2 and all(line.endswith(": text: unset") for line in attributes.splitlines())
summary_path = task / "archive-seal-summary.json"
summary = json.loads(summary_path.read_text(encoding="utf-8"))
summary.update({
    "artifacts_excluding_manifest": len(artifacts), "total_files": len(artifacts) + 1,
    "bytes_excluding_manifest": manifest["bytes_excluding_manifest"],
    "total_bytes": manifest["bytes_excluding_manifest"] + manifest_path.stat().st_size,
    "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
    "prior_manifest_sha256_before_text_attribute": previous_hash,
    "git_text_conversion": "disabled; raw TSV and manifest representative paths verified text: unset",
    "captured_measurement_artifacts_unchanged": True,
})
summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8", newline="\n")
print(json.dumps(summary, indent=2))
