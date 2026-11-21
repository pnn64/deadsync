"""Archive explicitly selected PNG encoder evidence; never copy PNGs or executables."""
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import re
import struct
import subprocess
import sys

WORK = Path(__file__).resolve().parent
ROOT = WORK.parents[1]
DEST = ROOT / "crates/deadlib-image/png/bench/results/2026-11-20-encode"
TEXT = {".tsv", ".json", ".log", ".py", ".patch", ".rs", ".txt", ".md"}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def decode(data):
    return data.decode("utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig")


def json_bytes(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode("utf-8")


def capture(command):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=30)
    return {"command": command, "exit_code": result.returncode,
            "stdout": result.stdout.decode("utf-8", errors="replace"),
            "stderr": result.stderr.decode("utf-8", errors="replace")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", action="append", required=True,
                        help="include NAME.tsv/.json/.log and NAME-* or NAME.* evidence; repeatable")
    parser.add_argument("--include", action="append", default=[],
                        help="explicit extra text filename inside the scratch directory; repeatable")
    args = parser.parse_args()
    if DEST.exists() and any(DEST.iterdir()):
        raise SystemExit(f"Refusing to overwrite nonempty archive: {DEST}")
    selected = {"corpus.json", "run.py", "capture.py", "baseline-encode.rs", Path(__file__).name}
    for prefix in args.prefix:
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", prefix):
            raise ValueError(f"invalid run prefix: {prefix!r}")
        matches = {p.name for p in WORK.iterdir() if p.is_file() and p.suffix in {".tsv", ".json", ".log"}
                   and (p.name.startswith(prefix + ".") or p.name.startswith(prefix + "-"))}
        if not matches:
            raise ValueError(f"no evidence matches prefix {prefix}")
        selected.update(matches)
    for name in args.include:
        if Path(name).name != name or Path(name).suffix not in TEXT:
            raise ValueError(f"extra evidence must be a text filename inside {WORK}: {name!r}")
        selected.add(name)
    payloads, originals = {}, {}
    for name in sorted(selected):
        path = WORK / name
        if path.stat().st_size > 8 * 1024 * 1024:
            raise ValueError(f"text evidence exceeds 8 MiB limit: {name}")
        data = path.read_bytes()
        decode(data)  # Fail before creating an archive if a purported text file is binary.
        payloads[name] = data
        originals[name] = {"path": path.relative_to(ROOT).as_posix(), "sha256": sha(data)}
    if sha(payloads["baseline-encode.rs"]) != "6b3b184913ab8db948ff4e302542016375fb9c7e7bacb679f9ea7750c1e02753":
        raise ValueError("reconstructed baseline benchmark source no longer matches recorded identity")
    records = {}
    inputs = {}
    generated = {}
    for name, data in list(payloads.items()):
        if not name.endswith(".json"):
            continue
        record = json.loads(decode(data))
        if not isinstance(record, dict) or "command" not in record:
            continue
        stem = name[:-5]
        for suffix, field in ((".tsv", "output_sha256"), (".log", "log_sha256")):
            companion = stem + suffix
            if companion not in payloads or sha(payloads[companion]) != record[field]:
                raise ValueError(f"missing or altered run evidence: {companion}")
        if not record.get("sources_unchanged_during_run") or not record.get("inputs_unchanged_during_run"):
            raise ValueError(f"unstable or incomplete run provenance: {name}")
        built = record["executable_source_snapshot"]
        if built["executable_sha256"] != record["executable_sha256"]:
            raise ValueError(f"conflicting executable identity: {name}")
        current = record["sources"]
        changed = sorted(path for path in current.keys() | built["sources"].keys()
                         if current.get(path) != built["sources"].get(path))
        records[stem] = {"provenance": name, "utc": record["utc"], "command": record["command"],
                         "png_bench_ms": record["png_bench_ms"], "exit_code": record["exit_code"],
                         "working_tree_head_at_measurement": record["head"],
                         "executable_sha256": record["executable_sha256"],
                         "measured_source_identity": f"{name}: executable_source_snapshot.sources",
                         "working_tree_differences_from_measured_sources": changed}
        for path, digest in record["inputs"].items():
            if path in inputs and inputs[path] != digest:
                raise ValueError(f"input changed across selected runs: {path}")
            inputs[path] = digest
        for row in csv.DictReader(io.StringIO(decode(payloads[stem + ".tsv"])), delimiter="\t"):
            if row["file"].startswith("generated/"):
                dimensions = [int(row["width"]), int(row["height"])]
                if row["file"] in generated and generated[row["file"]] != dimensions:
                    raise ValueError(f"generated dimensions changed: {row['file']}")
                generated[row["file"]] = dimensions
    if not records:
        raise ValueError("selection contains no complete run provenance records")
    corpus = json.loads(decode(payloads["corpus.json"]))
    input_entries = []
    for entry in corpus["files"]:
        name = entry["path"]
        if name not in inputs:
            raise ValueError(f"corpus input has no recorded SHA256: {name}")
        path = (ROOT / name).resolve()
        if not path.is_relative_to(ROOT):
            raise ValueError(f"corpus path escaped repository: {name}")
        tracked = subprocess.run(["git", "ls-files", "--error-unmatch", "--", name],
                                 cwd=ROOT, capture_output=True).returncode == 0
        item = {**entry, "sha256": inputs[name], "tracked_at_archive": tracked,
                "available_locally_at_archive": path.is_file(), "included_in_archive": False,
                "fresh_clone_availability": "tracked repository file" if tracked else "requires separately supplied exact input"}
        if path.is_file():
            data = path.read_bytes()
            if sha(data) != inputs[name]:
                raise ValueError(f"current input differs from measurement: {name}")
            item["bytes"] = len(data)
            if data.startswith(b"\x89PNG\r\n\x1a\n"):
                item["width"], item["height"], item["bit_depth"], item["color_type"] = struct.unpack(">IIBB", data[16:26])
        input_entries.append(item)
    now = datetime.now(timezone.utc).isoformat()
    facts = {"captured_utc": now, "scope": "Archive-time host facts; not a per-run environment snapshot.",
             "platform": platform.platform(), "python": sys.version,
             "relevant_environment": {key: os.environ.get(key) for key in
                                      ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET")},
             "rustc": capture(["rustc", "-Vv"]), "cargo": capture(["cargo", "-V"]),
             "git_head": capture(["git", "rev-parse", "HEAD"])}
    if os.name == "nt":
        facts["hardware"] = capture(["powershell", "-NoProfile", "-Command",
            "@{Processor = @(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfLogicalProcessors); "
            "OS = Get-CimInstance Win32_OperatingSystem | Select-Object Caption,BuildNumber} | ConvertTo-Json -Depth 4"])
    payloads["environment.json"] = json_bytes(facts)
    payloads["runs.json"] = json_bytes({"source_identity_rule":
        "Measured code is identified by executable_source_snapshot.sources in each run record. "
        "Working-tree HEAD/sources may differ when an earlier frozen executable is rerun.", "runs": records})
    payloads["input-manifest.json"] = json_bytes({"external_files": input_entries,
        "external_total_bytes_available": sum(item.get("bytes", 0) for item in input_entries),
        "generated_cases": [{"file": name, "width": size[0], "height": size[1]}
                            for name, size in sorted(generated.items())],
        "generated_identity": "Deterministic algorithms in the measured encode.rs source snapshots; source hashes are in run provenance.",
        "preparation": "Decode external PNG to RGBA8; force every alpha byte to 255 outside timing.",
        "images_archived": False})
    payloads[".gitattributes"] = b"* -text whitespace=-blank-at-eol,-blank-at-eof,-space-before-tab,-cr-at-eol\n"
    payloads["README.md"] = ("# PNG encoder measurement evidence\n\n"
        "Raw TSVs, logs and provenance JSON are copied byte for byte. No image inputs or executables are included.\n"
        "`archive-manifest.json` records the SHA256 of every archived artifact except itself.\n"
        "`input-manifest.json` distinguishes tracked inputs from local-only files and records original input SHA256, dimensions and sizes.\n"
        "`runs.json` indexes exact historical commands and identifies measured sources through the frozen executable snapshot, "
        "which may differ from the working-tree HEAD when the executable was rerun.\n"
        "`environment.json` is explicitly an archive-time snapshot, not retroactive proof of every run's host state.\n\n"
        "`baseline-encode.rs` reconstructs the original five-mode harness by removing only the later Rle benchmark modes. "
        "Its raw SHA256 matches the recorded baseline/steps 1-5 source identity "
        "`6b3b184913ab8db948ff4e302542016375fb9c7e7bacb679f9ea7750c1e02753`.\n\n"
        "## Reproduction\n\n"
        "Run these examples from the repository root on Windows. The current committed harness reproduces generated inputs. "
        "To reproduce historical measurements exactly, supply the exact source versions identified in each run record "
        "and every local-only external PNG matching its input SHA256; hashes alone do not supply missing source or image bytes.\n\n"
        "```powershell\n"
        "$pngEvidence = 'crates/deadlib-image/png/bench/results/2026-11-20-encode'\n"
        "$pngCorpus = Get-Content -Raw \"$pngEvidence/corpus.json\" | ConvertFrom-Json\n"
        "cargo build --release --locked --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --target-dir target/png-encode-repro\n"
        "$env:PNG_BENCH_MS = '40'\n"
        "& target/png-encode-repro/release/encode.exe --generated --sizes $pngCorpus.sizes\n"
        "# With every external input available at its original path:\n"
        "$pngInputs = @($pngCorpus.files | ForEach-Object { $_.path })\n"
        "& target/png-encode-repro/release/encode.exe --generated --sizes $pngCorpus.sizes @pngInputs\n"
        "# Allocation measurements use a separate instrumented build:\n"
        "cargo build --release --locked --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --features allocations --target-dir target/png-encode-repro-alloc\n"
        "& target/png-encode-repro-alloc/release/encode.exe --generated --sizes $pngCorpus.sizes @pngInputs\n"
        "# Example analysis, naming matching input/codec sets from this archive:\n"
        "python crates/deadlib-image/png/bench/compare_encode.py baseline.tsv after.tsv --corpus \"$pngEvidence/corpus.json\" --output comparison.json\n"
        "```\n\n"
        "The archived `run.py` and `capture.py` retain the original command-capture implementation. "
        "Their paths are relative to their original `target/png-encode-opt-20261120` location; restore them there to use those wrappers.\n"
        "Allocation columns are requested allocation/reallocation bytes, not peak RSS. Timing TSVs and allocation TSVs are separate builds. "
        "A zero or truncated validation log is not evidence of a passing check.\n").encode("utf-8")
    manifest = {"created_utc": now, "archive_invocation": sys.argv, "selected_prefixes": args.prefix,
                "extra_files": args.include, "artifacts": []}
    for name, data in sorted(payloads.items()):
        manifest["artifacts"].append({"path": name, "bytes": len(data), "sha256": sha(data),
                                      **({"copied_from": originals[name]} if name in originals else {"generated_at_archive": True})})
    payloads["archive-manifest.json"] = json_bytes(manifest)
    for name, source in originals.items():
        if sha((ROOT / source["path"]).read_bytes()) != source["sha256"]:
            raise ValueError(f"evidence changed during archive preparation: {name}")
    DEST.mkdir(parents=True, exist_ok=True)
    for name, data in payloads.items():
        with (DEST / name).open("xb") as out:
            out.write(data)
    for item in manifest["artifacts"]:
        if sha((DEST / item["path"]).read_bytes()) != item["sha256"]:
            raise ValueError(f"archive verification failed: {item['path']}")
    print(f"Archived and verified {len(manifest['artifacts'])} artifacts in {DEST}")
    print(f"External input bytes recorded, not copied: {sum(item.get('bytes', 0) for item in input_entries)}")


if __name__ == "__main__":
    main()
