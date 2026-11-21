"""Seal the compact BMP evidence archive after final restoration gates; never run benchmarks."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re

task = Path(__file__).resolve().parent
root = task.parents[1]
archive = root / "crates/deadlib-image/bmp/bench/results/2026-11-20-pass2"
assert not archive.exists(), "Never overwrite a sealed archive"
final = json.loads((task / "final-restored-validation-summary.json").read_text(encoding="utf-8"))
assert len(final["commands"]) == 3 and all(row["exit_code"] == 0 for row in final["commands"])
assert final["crate_integration_tests"] == 37 and final["independent_alpha_tests"] == final["doctests"] == 1
assert final["new_benchmark_builds"] == final["new_timed_benchmarks"] == final["new_snapshot_builds"] == 0
restored = json.loads((task / "restored-baseline-source.json").read_text(encoding="utf-8"))
for row in restored["after_source_config"]:
    raw = (root / row["path"]).read_bytes()
    assert len(raw) == row["bytes"] and hashlib.sha256(raw).hexdigest() == row["sha256"], row["path"]
audit = json.loads((task / "candidate5-summary-audit.json").read_text(encoding="utf-8"))
assert audit["status"] == "passed" and audit["raw_before_after_record_pairs"] == 622

extensions = {".tsv", ".log", ".json", ".jsonl", ".md", ".py", ".ps1", ".patch"}
exclude = {"archive-pre-final.py", "archive-seal-summary.json", "archive-audit-summary.json"}
sources = [path for path in task.iterdir() if path.is_file() and path.suffix in extensions and path.name not in exclude]
stages = ["baseline", "candidate1-alpha", "candidate1-logical", "candidate2-rgba4444",
          "candidate3-direct-rgba", "candidate4-alpha-dispatch", "candidate5-two-layouts"]
for stage in stages:
    sources.extend(path for path in (task / stage).iterdir() if path.is_file() and path.suffix in {".json", ".patch"})
for directory in ["candidate4-alpha-dispatch-validation", "candidate5-two-layouts-validation", "final-restored-validation"]:
    sources.extend(path for path in (task / directory).iterdir() if path.is_file() and path.suffix in {".json", ".jsonl", ".log"})
for relative in [
    "alpha-controls/manifest.json", "alpha-controls/manifest.tsv",
    "validation/Cargo.toml", "validation/Cargo.lock", "validation/alpha_controls.rs",
    "assembly-candidate2/identity.json", "assembly-candidate2/symbols.json",
    "assembly-candidate3/identity.json",
]:
    source = task / relative
    assert source.is_file(), relative
    sources.append(source)
sources = sorted(set(sources))
assert all(path.stat().st_size < 1_000_000 for path in sources), "Unexpected large artifact"
assert sum(path.stat().st_size for path in sources) < 20_000_000, "Archive is no longer compact"

readme = (task / "archive-report.md").read_text(encoding="utf-8")
reproduce = (task / "archive-reproduce.md").read_text(encoding="utf-8")
assert "PENDING" not in readme and "will replace" not in readme
validation = {
    "status": "complete; all production candidates rejected, original production restored",
    "production_optimizations_retained": 0,
    "baseline_git_head": "945da378a907bb7c4fec1f432d02d320c9d2a462",
    "production": restored["production_sources"],
    "retained_tests": "evidence/retained-regression-tests.patch",
    "final_restored_validation": final,
    "candidate5_summary_audit": audit,
    "snapshot": {"records": 22570, "bytes": 42819895,
                 "sha256": "076e78cae341e782b60f72fa3f60c0ff85142759e4821069f1012063f37bf0df",
                 "exact_candidate_replays": stages[1:],
                 "restored_production": "Exact original source and compiler/harness inputs; original snapshot applies."},
    "allocation_records_per_candidate": 622,
    "candidate_allocation_changes": [],
    "early_log_limitation": "Early PowerShell transcripts omitted native Cargo output; those results were observed through Core and exit-checked. Candidate4/candidate5 and final restored-source gates have direct durable native stdout/stderr and checked exit statuses.",
    "archive_omits": ["executables", "PDBs", "large behavior snapshot", "generated BMP payloads", "build caches", "full disassembly"],
}
documents = {"README.md": readme, "REPRODUCE.md": reproduce,
             ".gitattributes": "# Preserve raw evidence bytes and manifest hashes across Git checkouts.\n* -text\n",
             "validation-status.json": json.dumps(validation, indent=2) + "\n"}
planned = {"manifest.json", *documents, *("evidence/" + p.relative_to(task).as_posix() for p in sources)}
link_count = 0
for name in ("README.md", "REPRODUCE.md"):
    for reference in re.findall(r"\[[^\]]+\]\(([^)]+)\)", documents[name]):
        if "://" in reference or reference.startswith("#"):
            continue
        reference = reference.split("#", 1)[0]
        destination = (archive / name).parent / reference
        relative = destination.resolve().relative_to(archive.resolve()).as_posix()
        assert relative in planned, (name, reference)
        link_count += 1

archive.mkdir(parents=True)
entries = []
for source in sources:
    relative = "evidence/" + source.relative_to(task).as_posix()
    destination = archive / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    raw = source.read_bytes()
    with destination.open("xb") as output:
        output.write(raw)
    assert destination.read_bytes() == raw
    entries.append({"path": relative, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest(),
                    "source_task_path": source.relative_to(task).as_posix()})
for name, content in documents.items():
    raw = content.encode("utf-8")
    with (archive / name).open("xb") as output:
        output.write(raw)
    entries.append({"path": name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest(),
                    "source_task_path": {"README.md": "archive-report.md", "REPRODUCE.md": "archive-reproduce.md"}.get(name)})
entries.sort(key=lambda row: row["path"])
manifest = {
    "created_utc": datetime.now(timezone.utc).isoformat(),
    "decision": "Zero BMP production optimizations retained; original source restored exactly.",
    "files_excluding_manifest": len(entries), "bytes_excluding_manifest": sum(row["bytes"] for row in entries),
    "hash": "SHA-256", "self_excluded": "manifest.json",
    "artifacts": entries,
}
with (archive / "manifest.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(manifest, output, indent=2)
    output.write("\n")
actual = {path.relative_to(archive).as_posix() for path in archive.rglob("*") if path.is_file()}
assert actual == {row["path"] for row in entries} | {"manifest.json"}
for entry in entries:
    raw = (archive / entry["path"]).read_bytes()
    assert len(raw) == entry["bytes"] and hashlib.sha256(raw).hexdigest() == entry["sha256"], entry["path"]
summary = {
    "archive": str(archive), "status": "sealed and verified",
    "artifacts_excluding_manifest": len(entries), "total_files": len(actual),
    "bytes_excluding_manifest": manifest["bytes_excluding_manifest"],
    "total_bytes": sum(path.stat().st_size for path in archive.rglob("*") if path.is_file()),
    "local_report_links_verified": link_count,
    "manifest_sha256": hashlib.sha256((archive / "manifest.json").read_bytes()).hexdigest(),
    "all_copied_bytes_match_source": True, "all_manifest_hashes_verified": True,
    "production_optimizations_retained": 0,
}
with (task / "archive-seal-summary.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(summary, output, indent=2)
    output.write("\n")
print(json.dumps(summary, indent=2))
