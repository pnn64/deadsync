"""Restore split fixture archives, or split new and tracked archives before committing.

Run after `git submodule update --init tests/fixtures`. Uses only Python's
standard library. --split keeps every published part below 50,000,000 bytes.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

MAX_FILE_BYTES = 50_000_000
PART_BYTES = 48_000_000
MANIFEST = "split-archives.json"
IGNORE_MARKER = "# Restored split archives (managed by prepare_test_fixtures.py)"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def child(root, name):
    path = Path(name)
    if not name or path.is_absolute() or any(part in (".", "..") for part in name.replace("\\", "/").split("/")):
        raise ValueError(f"fixture path must be relative: {name}")
    target = (root / path).resolve()
    if not target.is_relative_to(root.resolve()):
        raise ValueError(f"fixture path escapes its store: {name}")
    return target


def load_archives(store):
    manifest = store / MANIFEST
    if not manifest.is_file():
        return []
    data = json.loads(manifest.read_bytes())
    if data["version"] != 1:
        raise ValueError("unsupported split archive manifest version")
    archives = data["archives"]
    if len({entry["archive"] for entry in archives}) != len(archives):
        raise ValueError("duplicate split archive")
    return archives


def prepare(store):
    restored = 0
    for entry in load_archives(store):
        destination = child(store, entry["archive"])
        if destination.is_file():
            if destination.stat().st_size != entry["bytes"] or digest(destination) != entry["sha256"]:
                raise ValueError(f"existing fixture checksum differs: {destination}")
            continue
        total = 0
        whole = hashlib.sha256()
        pending_path = None
        try:
            with tempfile.NamedTemporaryFile(dir=store, prefix=".fixture-", delete=False) as pending:
                pending_path = Path(pending.name)
                for part in entry["parts"]:
                    path = child(store, part["file"])
                    if not 0 < part["bytes"] < MAX_FILE_BYTES or path.stat().st_size != part["bytes"]:
                        raise ValueError(f"invalid fixture part size: {path}")
                    checksum = hashlib.sha256()
                    with path.open("rb") as source:
                        while block := source.read(1024 * 1024):
                            checksum.update(block)
                            whole.update(block)
                            pending.write(block)
                            total += len(block)
                    if checksum.hexdigest() != part["sha256"]:
                        raise ValueError(f"fixture part checksum differs: {path}")
            if total != entry["bytes"] or whole.hexdigest() != entry["sha256"]:
                raise ValueError(f"restored fixture checksum differs: {destination}")
            pending_path.replace(destination)
            restored += 1
        finally:
            if pending_path is not None:
                pending_path.unlink(missing_ok=True)
    return restored


def split(store):
    tracked = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "*.tar.zst"],
        cwd=store).decode().split("\0")
    entries = {entry["archive"]: entry for entry in load_archives(store)}
    for name in filter(None, tracked):
        archive = child(store, name)
        if archive.stat().st_size < MAX_FILE_BYTES:
            continue
        checksum = digest(archive)
        parts_dir = store / "archive-parts" / checksum
        parts_dir.mkdir(parents=True, exist_ok=True)
        parts = []
        with archive.open("rb") as source:
            while block := source.read(PART_BYTES):
                part = parts_dir / f"{len(parts):03}.part"
                part.write_bytes(block)
                parts.append({"file": part.relative_to(store).as_posix(),
                              "bytes": len(block), "sha256": hashlib.sha256(block).hexdigest()})
        entries[name] = {"archive": name, "bytes": archive.stat().st_size,
                         "sha256": checksum, "parts": parts}
    archives = sorted(entries.values(), key=lambda entry: entry["archive"])
    (store / MANIFEST).write_text(json.dumps({"version": 1, "archives": archives}, indent=2) + "\n", encoding="utf-8")
    ignore = store.parent / ".gitignore"
    previous = ignore.read_text(encoding="utf-8") if ignore.exists() else ""
    previous = previous.split(IGNORE_MARKER)[0].rstrip()
    rules = ["/full_song_lua/.fixture-*"] + ["/full_song_lua/" + entry["archive"] for entry in archives]
    ignore.write_text(previous + "\n\n" + IGNORE_MARKER + "\n" + "\n".join(rules) + "\n", encoding="utf-8")
    if archives:
        subprocess.run(["git", "update-index", "--force-remove", "--",
                        *[entry["archive"] for entry in archives]], cwd=store, check=True)
    return archives


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture-root", type=Path,
                        default=Path(__file__).resolve().parents[1] / "tests/fixtures")
    parser.add_argument("--split", action="store_true",
                        help="split new or tracked oversized archives and untrack the restored originals; keeps local files")
    args = parser.parse_args()
    store = args.fixture_root.resolve() / "full_song_lua"
    if not (store / "index.json").is_file():
        parser.error("initialize fixtures first: git submodule update --init tests/fixtures")
    if args.split:
        archives = split(store)
        print(f"{len(archives)} split archives; parts are at most {PART_BYTES:,} bytes")
    else:
        print(f"Restored {prepare(store)} archives; split fixture checksums verified")


if __name__ == "__main__":
    main()
