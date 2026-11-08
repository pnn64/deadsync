"""Consolidate retained song Lua references without changing their native data.

Requires Python 3.11+ and the zstd CLI. Archives and traces are recompressed at zstd level 22;
decompressed SHA-256 hashes must match before an original is removed. Historical
filenames remain aliases, so commands and regression references keep working.
Run from any directory: python scripts/normalize_song_lua_fixtures.py
"""

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures"
DEST = FIXTURES / "full_song_lua"
PUBLISH = threading.Lock()
CACHE = ROOT / "target/full-song-lua-normalization"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def normalize(source, zstd):
    source_hash = digest(source)
    if source.name.endswith(".tar.zst"):
        assert source.name == source_hash + ".tar.zst", f"archive address mismatch: {source}"
    CACHE.mkdir(exist_ok=True)
    receipt_path = CACHE / (source_hash + ".json")
    if receipt_path.exists():
        receipt = json.loads(receipt_path.read_bytes())
        destination = DEST / receipt["name"]
        if destination.exists() and digest(destination) == receipt["sha256"]:
            return source, receipt["name"], receipt["sha256"], receipt["raw_sha256"], destination.stat().st_size
    with tempfile.TemporaryDirectory(dir=ROOT / "target", prefix="song-lua-zstd-") as scratch:
        raw = Path(scratch) / "input"
        if source.suffix == ".zst":
            subprocess.run([zstd, "-q", "-d", str(source), "-o", str(raw)], check=True)
        else:
            shutil.copyfile(source, raw)
        raw_hash = digest(raw)
        compressed = Path(scratch) / "compressed.zst"
        subprocess.run([zstd, "-q", "--ultra", "-22", "-T1", str(raw), "-o", str(compressed)], check=True)
        check = Path(scratch) / "check"
        subprocess.run([zstd, "-q", "-d", str(compressed), "-o", str(check)], check=True)
        assert digest(check) == raw_hash, f"recompression changed native data: {source}"
        compressed_hash = digest(compressed)
        is_archive = source.name.endswith(".tar.zst")
        name = (compressed_hash + ".tar.zst") if is_archive else (raw_hash + ".json.zst")
        destination = DEST / name
        # Identical traces from different historical corpora share one file.
        with PUBLISH:
            if destination.exists():
                assert digest(destination) == compressed_hash
            else:
                with tempfile.NamedTemporaryFile(dir=DEST, prefix=".normalizing-", delete=False) as pending:
                    pending_path = Path(pending.name)
                try:
                    shutil.copyfile(compressed, pending_path)
                    os.replace(pending_path, destination)
                finally:
                    pending_path.unlink(missing_ok=True)
            write_json(receipt_path, {"name": name, "sha256": compressed_hash, "raw_sha256": raw_hash})
        return source, name, compressed_hash, raw_hash, destination.stat().st_size


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    zstd = shutil.which("zstd")
    if not zstd:
        parser.error("the zstd CLI is required")
    DEST.mkdir(exist_ok=True)
    (ROOT / "target").mkdir(exist_ok=True)
    old_archives = FIXTURES / "itgmania-song-archives"
    index_path = DEST / "index.json"
    index = json.loads(index_path.read_bytes()) if index_path.exists() else {"archive_schema_version": 1, "hash": "sha256-compressed-archive", "archives": []}
    if (old_archives / "index.json").exists():
        incoming = json.loads((old_archives / "index.json").read_bytes())
        merged = {entry["source_simfile"]: entry for entry in index["archives"]}
        merged.update({entry["source_simfile"]: entry for entry in incoming["archives"]})
        index["archives"] = sorted(merged.values(), key=lambda entry: entry["source_simfile"])
    mapping_path = DEST / "references.json"
    references = json.loads(mapping_path.read_bytes()) if mapping_path.exists() else {}
    sources = list(old_archives.glob("*.tar.zst"))
    sources.extend(DEST / entry["archive"] for entry in index["archives"]
                   if entry.get("compression_level") != 22 and (DEST / entry["archive"]).exists())
    manifests = []
    for directory in ["itgmania-song-lua", "itgmania-song-lua-selected", "itgmania-song-lua-allowed"]:
        base = FIXTURES / directory
        for path in base.rglob("*"):
            if not path.is_file():
                continue
            if path.name == "_semantic_manifest.json":
                manifests.append(path)
                manifest = json.loads(path.read_bytes())
                for entry in manifest["simfiles"]:
                    fixture = (base / entry["fixture"]).resolve()
                    if fixture.parent != base and "itgmania-song-lua-micro" in fixture.parts:
                        sources.append(fixture)
            elif ".semantic.json" in path.name:
                sources.append(path)
            elif path.name == "current-result-provenance.json":
                manifests.append(path)
    sources.extend((FIXTURES / "itgmania-song-lua-micro").glob("*whole-song*.json*"))
    sources = sorted(set(sources))
    if not sources and not manifests:
        print("Fixtures already consolidated at zstd level 22; no changes.")
        return
    entries = {entry["archive"]: entry for entry in index["archives"]}
    old_bytes = sum(path.stat().st_size for path in sources)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(normalize, path, zstd) for path in sources]
        for done, future in enumerate(concurrent.futures.as_completed(futures), 1):
            source, name, compressed_hash, raw_hash, size = future.result()
            for alias, previous in list(references.items()):
                if previous == source.name:
                    references[alias] = name
            references[source.relative_to(ROOT).as_posix()] = name
            if source.name in entries:
                entry = entries[source.name]
                aliases = set(entry.get("aliases", []))
                if source.name != name:
                    aliases.add(source.name)
                entry.update(archive=name, sha256=compressed_hash, compressed_bytes=size,
                             uncompressed_sha256=raw_hash, compression_level=22, aliases=sorted(aliases))
            results.append(source)
            print(f"[{done}/{len(sources)}] {source.name} -> {name} ({size:,} bytes)", flush=True)
    for path in manifests:
        name = digest(path) + ".manifest.json"
        shutil.copyfile(path, DEST / name)
        references[path.relative_to(ROOT).as_posix()] = name
        results.append(path)
    index["compression_level"] = 22
    write_json(index_path, index)
    write_json(mapping_path, dict(sorted(references.items())))
    # Commit the new index and verify all destinations before removing originals.
    for path in results:
        assert (DEST / references[path.relative_to(ROOT).as_posix()]).is_file()
        resolved = path.resolve()
        assert resolved.is_relative_to(FIXTURES.resolve())
        if resolved != (DEST / references[path.relative_to(ROOT).as_posix()]).resolve():
            path.unlink()
    if old_archives.exists():
        for path in old_archives.iterdir():
            assert path.is_file()
            if path.name != "index.json":
                os.replace(path, DEST / path.name)
            else:
                path.unlink()
    # Remove only the now-empty directories within the verified fixture root.
    for name in ["itgmania-song-archives", "itgmania-song-lua-selected", "itgmania-song-lua-allowed", "itgmania-song-lua"]:
        base = FIXTURES / name
        if base.exists():
            for directory in sorted((p for p in base.rglob("*") if p.is_dir()), reverse=True):
                if not any(directory.iterdir()):
                    directory.rmdir()
            if not any(base.iterdir()):
                base.rmdir()
    new_bytes = sum(path.stat().st_size for path in DEST.glob("*.zst"))
    if sources:
        print(f"Complete: {len(sources)} inputs; {old_bytes:,} -> {new_bytes:,} bytes; native data unchanged.", flush=True)
    else:
        print(f"Consolidated {len(manifests)} provenance manifests; fixture payloads unchanged.", flush=True)


if __name__ == "__main__":
    main()
