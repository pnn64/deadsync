"""Archive JPEG evidence after all measurement processes finish.
Default: list proposed source files only (no hashes or writes).
Use --write once; existing README.md is allowed, existing raw/manifest outputs are not.
Restore root-level raw tools/manifests to target/jpeg-perf-20261119 before running them.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = Path(__file__).resolve().parent
DEST = ROOT / "crates/deadlib-image/jpeg/bench/results/windows-x86_64-20261119-rgba8"
FIXED = """
analyze.py measure.py paired.py package-evidence.py helper-check-oracle-selftest.ps1
jpeg-corpus-inventory.py jpeg-corpus-select.py jpeg-corpus-kodak.py
jpeg-entropy-error-corpus.py jpeg-corpus-selection.json jpeg-corpus-kodak.json
jpeg-corpus-speed.txt jpeg-corpus-kodak.txt jpeg-corpus-diagnostic.txt
jpeg-corpus-native-correctness.txt jpeg-corpus-artifact-hashes.json
cmyk-repeat.txt progressive-repeat.txt
jpeg-corpus-notes.md jpeg-entropy-native-snapshot.md
jpeg-error-corpus/manifest.json jpeg-error-corpus/paths.txt
helper-check-oracle-20261119/helper-self-check-status.json
""".split()
PATTERNS = ("*.tsv", "*-build.json", "*-run.json", "*-paired.jsonl", "*.log",
            "*-repeat.txt", "*-focus.txt", "*.py", "*.ps1",
            "helper-check-oracle-20261119/helper-self-check-*.stdout.txt",
            "helper-check-oracle-20261119/helper-self-check-*.stderr.txt")
OMIT = {"jpeg-corpus-inventory-run.log"}
ALLOWED = {".tsv", ".json", ".jsonl", ".log", ".txt", ".md", ".patch", ".py", ".ps1"}

def sha(data):
    return hashlib.sha256(data).hexdigest()

def read_json(path):
    data = path.read_bytes()
    return json.loads(data.decode("utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"))

def chosen(extra):
    paths = {WORK / name for name in FIXED}
    for pattern in PATTERNS:
        paths.update(WORK.glob(pattern))
    for build in WORK.glob("*-build.json"):
        patch = WORK / (build.name.removesuffix("-build.json") + ".patch")
        if patch.is_file():
            paths.add(patch)
    paths.update((WORK / name).resolve() for name in extra)
    paths = sorted(p for p in paths if p.name not in OMIT)
    for path in paths:
        if not path.is_file() or not path.resolve().is_relative_to(WORK) or path.suffix not in ALLOWED:
            raise ValueError(f"missing or disallowed evidence file: {path}")
        if path.name.endswith("-analysis.json") or path.name in {"jpeg-corpus-inventory.json", "jpeg-corpus-runtime-inventory.json", "jpeg-corpus-target-groups.json"}:
            raise ValueError(f"inventory/derived analysis must remain outside archive: {path}")
    return paths

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--output", type=Path, default=DEST)
    parser.add_argument("--extra", action="append", default=[], metavar="WORK_RELATIVE_FILE")
    args = parser.parse_args()
    paths = chosen(args.extra)
    if not args.write:
        print(json.dumps(dict(files=[p.relative_to(WORK).as_posix() for p in paths],
                              total_source_bytes=sum(p.stat().st_size for p in paths),
                              note="Names/sizes only. No hashing, copying, compression or writes."), indent=2))
        return
    dest = args.output.resolve()
    if not dest.is_relative_to(ROOT) or dest == ROOT or dest.is_relative_to(WORK):
        raise ValueError("archive destination must be a distinct repository subdirectory")
    reserved = [dest / name for name in ("raw", "manifest.json", "corpus-public-repo.txt", "corpus-local-runtime.txt")]
    if any(path.exists() for path in reserved):
        raise FileExistsError("refusing to overwrite existing archive outputs")
    items = read_json(WORK / "jpeg-corpus-selection.json")["speed_candidates"] + read_json(WORK / "jpeg-corpus-kodak.json")["inputs"]
    runtime = sorted(i["path"] for i in items if i["group"] == "runtime-stratified")
    reproducible = sorted(i["path"] for i in items if i["group"] != "runtime-stratified")
    if len(items) != 105 or len({i["path"] for i in items}) != 105 or len(runtime) != 19 or len(reproducible) != 86:
        raise ValueError("frozen corpus partition changed; expected 105 = 19 local + 86 repo/public")
    (dest / "raw").mkdir(parents=True)
    records, states = [], {}
    for path in paths:
        before = path.stat()
        data = path.read_bytes()
        after = path.stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise RuntimeError(f"source changed while packaging: {path}")
        states[path] = (after.st_size, after.st_mtime_ns)
        compressed = path.suffix != ".tsv" and (path.suffix in {".log", ".jsonl", ".patch"} or len(data) >= 65536 or
                      data.startswith((b"\xff\xfe", b"\xfe\xff")) or path.name.endswith((".stdout.txt", ".stderr.txt")))
        stored = gzip.compress(data, compresslevel=9, mtime=0) if compressed else data
        relative = Path("raw") / path.relative_to(WORK)
        target = dest / (str(relative) + (".gz" if compressed else ""))
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(stored)
        check = target.read_bytes()
        if check != stored or (gzip.decompress(check) if compressed else check) != data:
            raise RuntimeError(f"archive byte verification failed: {target}")
        records.append(dict(source=path.relative_to(ROOT).as_posix(), archive=target.relative_to(dest).as_posix(),
                            original_bytes=len(data), original_sha256=sha(data), stored_bytes=len(stored),
                            stored_sha256=sha(stored), compression="gzip" if compressed else "none"))
    generated = []
    for filename, entries in (("corpus-public-repo.txt", reproducible), ("corpus-local-runtime.txt", runtime)):
        data = ("\n".join(entries) + "\n").encode("utf-8")
        with (dest / filename).open("xb") as stream:
            stream.write(data)
        generated.append(dict(archive=filename, bytes=len(data), sha256=sha(data)))
    for path, state in states.items():
        current = path.stat()
        if (current.st_size, current.st_mtime_ns) != state:
            raise RuntimeError(f"source changed before archive completion: {path}")
    manifest = dict(schema=1, created_utc=datetime.now(timezone.utc).isoformat(), source_root=WORK.relative_to(ROOT).as_posix(),
                    corpus=dict(full_count=105, unbundled_local_runtime=19, repository_or_publicly_regenerable=86,
                                groups=dict(Counter(i["group"] for i in items)),
                                note="105-input results include 19 local runtime artworks not redistributed here. The 86-input subset is a distinct reproduction scope; never relabel 105-input aggregates as 86-input results."),
                    records=records, generated_files=generated,
                    omitted=["executables", "image corpora", "snapshot payloads/directories", "large inventory dumps",
                             "*-analysis.json", "proposal-only patches/audit notes"],
                    restore="Decompress .gz without transcoding; restore root-level tools/manifests to target/jpeg-perf-20261119. Keep nested generated-corpus manifests as references until their generator creates a fresh output directory.")
    with (dest / "manifest.json").open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(manifest, stream, indent=2)
        stream.write("\n")
    print(f"Archived {len(records)} source files; {sum(r['stored_bytes'] for r in records):,} stored bytes; 105 = 86 + 19 inputs.")

if __name__ == "__main__":
    main()
