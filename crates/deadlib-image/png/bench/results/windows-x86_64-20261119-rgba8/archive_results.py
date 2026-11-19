"""Archive completed PNG experiments. Never build, test, time, or copy image/binary payloads."""
import argparse
import csv
from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import sys

ARCHIVE = Path(__file__).resolve().parent
ROOT = next(path for path in ARCHIVE.parents if (path / ".git").exists())
WORK = ROOT / "target/png-perf-20261119"
CONFIG = ARCHIVE / "experiment-states.json"
FINAL_TABLES = (
    "final-rgba8-run1.tsv", "final-rgba8-run2.tsv", "final-raw.tsv",
    "final-rgba8-allocations.tsv", "final-raw-allocations.tsv",
)
BASE_TABLES = (
    "baseline-rgba8.tsv", "baseline-raw.tsv",
    "baseline-rgba8-allocations.tsv", "baseline-raw-allocations.tsv",
)
PROCESSES = tuple(
    f"final-{name}-process.json" for name in ("rgba8-run1", "rgba8-run2", "raw")
)
ALLOWED = {".tsv", ".json", ".log", ".patch", ".md", ".txt", ".py", ".rs"}
PNG = "crates/deadlib-image/png"
INPUT_SUPPORT = {
    "generate_decode_corpus.py": ROOT / PNG / "scripts/generate_decode_corpus.py",
    "fetch_corpus.py": ROOT / PNG / "scripts/fetch_corpus.py",
    "kodak.json": ROOT / PNG / "scripts/kodak.json",
    "formats-generation-manifest.json": ROOT / "target/png-decode-corpus-20261117/manifest.json",
    "previous-corpus-manifest.json": ROOT / PNG / "bench/results/windows-x86_64-20261117-pipelined/manifest.json",
}
TEST_RESULT = re.compile(
    r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out"
)


def text(path):
    data = path.read_bytes()
    return data.decode("utf-16" if data[:2] in (b"\xff\xfe", b"\xfe\xff") else "utf-8-sig")


def read_json(path):
    return json.loads(text(path))


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def relative_input(value):
    path = Path(value.replace("\\", "/"))
    path = (path if path.is_absolute() else ROOT / path).resolve()
    if not path.is_relative_to(ROOT):
        raise ValueError(f"input outside repository: {value}")
    return path.relative_to(ROOT).as_posix()


def table(path):
    cases = {}
    for row in csv.DictReader(io.StringIO(text(path)), delimiter="\t"):
        name = relative_input(row["file"])
        case = cases.setdefault(name, {})
        codec = row["codec"]
        if codec in case:
            raise ValueError(f"duplicate {name}/{codec} in {path.name}")
        values = {key: int(row[key]) for key in
                  ("ns", "thread_cycles", "alloc_calls", "requested_bytes", "pixels")}
        if any(value < 0 for value in values.values()):
            raise ValueError(f"negative measurement in {path.name}")
        if "allocations" in path.name:
            if values["ns"] or values["thread_cycles"] or not values["alloc_calls"] or not values["requested_bytes"]:
                raise ValueError(f"invalid allocation counter shape in {path.name}")
        elif not values["ns"] or not values["thread_cycles"] or values["alloc_calls"] or values["requested_bytes"]:
            raise ValueError(f"invalid timing counter shape in {path.name}")
        case[codec] = values
    expected = {"native-rgba8", "png-assets-rgba8", "image-rgba8"}
    raw = "-raw" in path.name if path.name in BASE_TABLES + FINAL_TABLES else bool(
        cases and "native" in next(iter(cases.values()))
    )
    if raw:
        expected = {"native", "png", "image", "png-packed"}
    if not cases or any(set(case) != expected for case in cases.values()):
        raise ValueError(f"incomplete codec set in {path.name}")
    if any(len({row["pixels"] for row in case.values()}) != 1 for case in cases.values()):
        raise ValueError(f"unequal pixel counts in {path.name}")
    return cases


def requirements(config):
    required = set(BASE_TABLES + FINAL_TABLES + PROCESSES)
    required.update(config["baseline"]["files"])
    required.update((
        "baseline-hashes.json", "build-environment.json", "assets-manifest.json",
        "inventory.json", "inventory.py", "baseline-verify.log", "frozen-harness.patch",
        "final-hashes.json", "final-summary.json", "final-tests.log", "final-verify.log",
        "analyze-final.py", "freeze-final.py", "run-final.py",
    ))
    for experiment in config["experiments"]:
        for pair in experiment["comparisons"]:
            required.update(pair)
        required.update(experiment["evidence"])
    required.update(config["compatibility_fix"]["evidence"])
    reports = config["validation_reports"]
    required.update(reports[name]["log"] for name in ("portable_i686", "portable_aarch64"))
    for stem in reports["clippy"]["passing_stems"] + reports["clippy"]["historical_failures"]:
        required.update(stem + suffix for suffix in (".exit.txt", ".stdout.log", ".stderr.log"))
    return sorted(required)


def preflight(config):
    missing = [name for name in requirements(config) if not (WORK / name).is_file()]
    missing.extend(path.relative_to(ROOT).as_posix() for path in INPUT_SUPPORT.values() if not path.is_file())
    problems = []
    tables = {}
    for path in sorted(WORK.glob("*.tsv")):
        try:
            tables[path.name] = table(path)
        except (KeyError, TypeError, ValueError) as error:
            problems.append(str(error))
    baseline = tables.get("baseline-rgba8.tsv", {})
    if baseline and len(baseline) != 116:
        problems.append(f"baseline has {len(baseline)} cases, expected 116")
    for name in BASE_TABLES + FINAL_TABLES:
        if name in tables and set(tables[name]) != set(baseline):
            problems.append(f"{name}: full corpus differs from baseline")
    for name, cases in tables.items():
        for path, rows in cases.items():
            if path not in baseline:
                problems.append(f"{name}: input absent from full baseline: {path}")
            elif next(iter(rows.values()))["pixels"] != next(iter(baseline[path].values()))["pixels"]:
                problems.append(f"{name}: pixel count differs from baseline: {path}")
    for experiment in config["experiments"]:
        for before, after in experiment["comparisons"]:
            if before in tables and after in tables:
                if set(tables[before]) != set(tables[after]):
                    problems.append(f"{experiment['id']}: paired corpus differs")
                elif any(set(tables[before][path]) != set(tables[after][path]) for path in tables[before]):
                    problems.append(f"{experiment['id']}: paired codec set differs")
    for name in PROCESSES:
        if (WORK / name).is_file():
            record = read_json(WORK / name)
            if record.get("exit_code") != 0 or not record.get("finished_utc"):
                problems.append(f"{name}: process did not complete successfully")
            if record.get("compiler_processes_before") or record.get("compiler_processes_after"):
                problems.append(f"{name}: compiler process observed at a boundary")
    if (WORK / "final-summary.json").is_file():
        summary = read_json(WORK / "final-summary.json")
        for name in FINAL_TABLES:
            item = summary.get("measurements", {}).get(name)
            if not item or not (WORK / name).is_file() or item.get("sha256") != digest(WORK / name):
                problems.append(f"{name}: final summary is missing or stale")
    result = {
        "missing_required_files": missing, "problems": problems,
        "tables_present": {name: len(cases) for name, cases in tables.items()},
        "heavy_hashing_and_copying": "Deferred until --write after all timing finishes.",
    }
    return result, tables


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def save(relative, data):
    destination = ARCHIVE / relative
    if not destination.resolve().is_relative_to(ARCHIVE):
        raise ValueError("archive destination escapes its directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + ".tmp")
    temporary.write_bytes(data)
    temporary.replace(destination)


def save_json(relative, value):
    save(relative, (json.dumps(value, indent=2) + "\n").encode())


def needs_gzip(relative, data):
    if relative.endswith(".patch"):
        return True
    try:
        decoded = data.decode("utf-16" if data[:2] in (b"\xff\xfe", b"\xfe\xff") else "utf-8-sig")
    except UnicodeDecodeError:
        return False
    lines = decoded.splitlines()
    return bool(lines and not lines[-1].strip(" \t")) or any(
        line.endswith((" ", "\t")) or re.match(r"^[ \t]* \t", line)
        for line in lines
    )


def retain(relative, data, records):
    compressed = needs_gzip(relative, data)
    destination = relative + ".gz" if compressed else relative
    save(destination, gzip.compress(data, mtime=0) if compressed else data)
    records.append({
        "logical_path": relative, "stored_path": destination,
        "storage": "gzip" if compressed else "identity",
        "original_bytes": len(data), "original_sha256": hashlib.sha256(data).hexdigest(),
    })


def corpus(cases):
    selected = read_json(WORK / "assets-manifest.json")["files"]
    assets = {item["benchmark_path"].replace("\\", "/"): item for item in selected}
    old = read_json(INPUT_SUPPORT["previous-corpus-manifest.json"])
    previous = old["corpus_sha256"]
    records = []
    for name, rows in sorted(cases.items()):
        path = ROOT / name
        sha = digest(path)
        expected = assets[name]["sha256"] if name in assets else previous.get(name)
        if expected is None or sha != expected.lower():
            raise ValueError(f"corpus hash missing or changed: {name}")
        with path.open("rb") as source:
            header = source.read(29)
        if header[:8] != b"\x89PNG\r\n\x1a\n":
            raise ValueError(f"not PNG: {name}")
        width = int.from_bytes(header[16:20], "big")
        height = int.from_bytes(header[20:24], "big")
        if any(row["pixels"] != width * height for row in rows.values()):
            raise ValueError(f"pixel count differs from frozen input: {name}")
        record = {
            "benchmark_path": name, "sha256": sha, "bytes": path.stat().st_size,
            "width": width, "height": height, "bit_depth": header[24],
            "color_type": header[25], "interlaced": bool(header[28]),
            "group": "assets" if name in assets else
                     "photos" if re.fullmatch(r"kodim\d+\.png", path.name) else "formats",
            "baseline_hash_evidence": "raw/assets-manifest.json" if name in assets else
                                     "raw/previous-corpus-manifest.json",
        }
        if name in assets:
            record["original_asset_path"] = assets[name]["source"]
        records.append(record)
    counts = {group: sum(item["group"] == group for item in records)
              for group in ("formats", "photos", "assets")}
    if counts != {"formats": 60, "photos": 24, "assets": 32}:
        raise ValueError(f"unexpected corpus groups: {counts}")
    return {"cases": len(records), "groups": counts, "inputs": records}


def validation(config):
    matches = list(TEST_RESULT.finditer(text(WORK / "final-tests.log")))
    totals = {
        "suites": len(matches), "passed": sum(int(m[2]) for m in matches),
        "failed": sum(int(m[3]) for m in matches),
        "ignored": sum(int(m[4]) for m in matches),
    }
    if not matches or totals["failed"] or any(m[1] != "ok" for m in matches):
        raise ValueError("final test log is incomplete or contains failures")
    if totals != config["validation_reports"]["final_release"]["expected_totals"]:
        raise ValueError(f"final test log differs from its independently counted complete run: {totals}")
    exits = {}
    for path in sorted(WORK.glob("*.exit.txt")):
        exits[path.name.removesuffix(".exit.txt")] = int(text(path).strip())
    for stem in config["validation_reports"]["clippy"]["passing_stems"]:
        if exits.get(stem) != 0:
            raise ValueError(f"final Clippy result missing or unsuccessful: {stem}")
    return {
        "release_tests": totals, "test_log": "raw/final-tests.log",
        "reported_checks": config["validation_reports"], "recorded_exit_codes": exits,
        "fuzz_note": "No historical sanitizer/fuzz result is counted as a new run by this archive.",
    }


def write_archive(config, tables):
    frozen = read_json(WORK / "final-hashes.json")
    for name, expected in frozen["sources_sha256"].items():
        if digest(ROOT / name) != expected:
            raise ValueError(f"source changed since final freeze: {name}")
    for name, expected in frozen["executables_sha256"].items():
        if digest(WORK / name) != expected:
            raise ValueError(f"frozen executable missing or changed: {name}")
    inputs = corpus(tables["baseline-rgba8.tsv"])
    checks = validation(config)
    evidence = []
    # Gzip preserves whitespace-bearing evidence without adding whitespace errors to Git.
    for path in sorted(WORK.iterdir()):
        if path.is_file() and path.suffix.lower() in ALLOWED:
            retain("raw/" + path.name, path.read_bytes(), evidence)
    for name, source in INPUT_SUPPORT.items():
        retain("raw/" + name, source.read_bytes(), evidence)
    save_json("corpus.json", inputs)
    save_json("validation.json", checks)
    base = config["baseline_commit"]
    fix = config["compatibility_fix_commit"]
    retain("compatibility-fix.patch", git("show", "--format=fuller", "--no-ext-diff", fix, "--", PNG + "/src", PNG + "/tests"), evidence)
    retain("final-performance.patch", git("diff", "--no-ext-diff", fix, "--", PNG + "/src", PNG + "/tests"), evidence)
    bench_paths = [PNG + "/bench/" + name for name in
                   ("Cargo.toml", "Cargo.lock", "src", "summarize.py", "README.md")]
    retain("final-harness.patch", git("diff", "--no-ext-diff", base, "--", *bench_paths), evidence)
    baseline_sources = {
        name: hashlib.sha256(git("show", base + ":" + name)).hexdigest()
        for name in frozen["sources_sha256"]
    }
    save_json("baseline-source-hashes.json", {
        "commit": base, "representation": "Git blob bytes, before checkout newline conversion.",
        "sha256": baseline_sources,
    })
    # Exclude this manifest itself; every retained payload and the archive script is hashed.
    files = []
    for path in sorted(ARCHIVE.rglob("*")):
        if path.is_file() and path != ARCHIVE / "manifest.json" and "__pycache__" not in path.parts:
            if path.suffix.lower() in {".exe", ".png", ".dll", ".pdb", ".zip"}:
                raise ValueError(f"unexpected binary/image in archive: {path}")
            files.append({
                "path": path.relative_to(ARCHIVE).as_posix(),
                "bytes": path.stat().st_size, "sha256": digest(path),
            })
    manifest = {
        "schema": 1, "archived_utc": datetime.now(timezone.utc).isoformat(),
        "baseline_commit": base, "compatibility_fix_commit": fix,
        "head_when_archived": git("rev-parse", "HEAD").decode().strip(),
        "scope": "116 buffered PNG inputs, equivalent RGBA8 output plus raw-format controls.",
        "corpus": "corpus.json", "source_states": "experiment-states.json",
        "input_reproduction": {
            "instructions": "README.md",
            "generator": "raw/generate_decode_corpus.py",
            "generation_manifest": "raw/formats-generation-manifest.json",
            "photo_fetcher": "raw/fetch_corpus.py", "photo_manifest": "raw/kodak.json",
            "selected_assets": "raw/assets-manifest.json",
        },
        "frozen_build_and_binary_hashes": "raw/final-hashes.json",
        "validation": "validation.json", "final_summary": "raw/final-summary.json",
        "final_measurements": ["raw/" + name for name in FINAL_TABLES],
        "timing_process_records": ["raw/" + name for name in PROCESSES],
        "evidence_storage": evidence,
        "verified_before_copy": {
            "frozen_source_hashes": len(frozen["sources_sha256"]),
            "frozen_executable_hashes": len(frozen["executables_sha256"]),
            "corpus_input_hashes": len(inputs["inputs"]),
            "complete_full_corpus_tables": len(BASE_TABLES + FINAL_TABLES),
            "final_summary_matches_raw_tsv_hashes": True,
        },
        "interpretation": [
            "No executable or PNG payload is included; hashes identify those inputs/artifacts.",
            "Logical evidence paths map to stored paths in evidence_storage; gzip preserves original bytes and avoids Git whitespace errors.",
            "A prepared unmeasured CRC candidate is not a measured rejection.",
            "Packed-unpack comparisons included temporary fixed-wide tables; final binaries do not.",
            "Packed-length source/binary lineage has the explicit limitation in experiment-states.json.",
            "stored-fixed-before.tsv is the accepted stored comparison baseline; stored-before.tsv is historical.",
            "Totals weight one decode per corpus input; geometric means weight per-file ratios equally.",
            "No per-file universal speed or requested-byte advantage is implied by corpus averages.",
            "The performance gate verdict belongs to final-summary.json and the parent RESULTS.md.",
        ],
        "retained_files": files,
    }
    save_json("manifest.json", manifest)
    return {"archived_files": len(files), "corpus": inputs["groups"], "tests": checks["release_tests"]}


def verify_archive():
    manifest = read_json(ARCHIVE / "manifest.json")
    for item in manifest["retained_files"]:
        path = ARCHIVE / item["path"]
        if path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
            raise ValueError(f"archived file changed: {item['path']}")
    for item in manifest["evidence_storage"]:
        stored = (ARCHIVE / item["stored_path"]).read_bytes()
        original = gzip.decompress(stored) if item["storage"] == "gzip" else stored
        if len(original) != item["original_bytes"] or hashlib.sha256(original).hexdigest() != item["original_sha256"]:
            raise ValueError(f"original evidence mismatch: {item['logical_path']}")
    return {"verified_files": len(manifest["retained_files"]), "corpus_inputs": 116}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--check", action="store_true", help="light preflight; do not copy corpus or binary data")
    action.add_argument("--write", action="store_true", help="only after all timing has finished")
    action.add_argument("--verify", action="store_true", help="verify retained archive hashes without original binaries")
    args = parser.parse_args()
    if args.verify:
        print(json.dumps(verify_archive(), indent=2))
        return 0
    config = read_json(CONFIG)
    report, tables = preflight(config)
    if report["missing_required_files"] or report["problems"]:
        print(json.dumps(report, indent=2))
        return 2
    if args.check:
        print(json.dumps(report, indent=2))
        return 0
    print(json.dumps(write_archive(config, tables), indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"archive refused: {error}", file=sys.stderr)
        raise SystemExit(1)
