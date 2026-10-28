"""Fetch and verify pinned external song packs; never redistribute their media."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()


def matches(path, entry):
    return path.is_file() and path.stat().st_size == entry["bytes"] and digest(path) == entry["sha256"]


def inside(root, relative):
    result = (root / relative).resolve()
    if not result.is_relative_to(root):
        raise ValueError(f"path escapes corpus destination: {relative}")
    return result


def fetch(pack, destination, offline):
    root = inside(destination, pack["name"])
    root.mkdir(parents=True, exist_ok=True)
    archive = root / "archive.zip"
    spec = pack["archive"]
    if not matches(archive, spec):
        if offline:
            raise ValueError(f"missing or changed archive: {archive}")
        if not spec["url"].startswith("https://"):
            raise ValueError("song pack downloads require HTTPS")
        # curl supports the official CDN here; Python's default HTTP client is
        # rejected by that CDN. No authentication or installer is involved.
        with tempfile.NamedTemporaryFile(dir=root, suffix=".part", delete=False) as output:
            temporary = Path(output.name)
        try:
            subprocess.run([
                "curl", "--fail", "--location", "--retry", "2", "--max-time", "300",
                "--max-filesize", str(spec["bytes"]), "--output", str(temporary), spec["url"],
            ], check=True)
            if not matches(temporary, spec):
                raise ValueError(f"archive hash/size mismatch: {pack['name']}")
            os.replace(temporary, archive)
        finally:
            temporary.unlink(missing_ok=True)
    with zipfile.ZipFile(archive) as source:
        for entry in pack["files"]:
            target = inside(root, entry["member"])
            if matches(target, entry):
                continue
            info = source.getinfo(entry["member"])
            if info.file_size != entry["bytes"]:
                raise ValueError(f"member size mismatch: {entry['member']}")
            # Read only the explicitly pinned members. Do not extract links,
            # executables, permissions or any unlisted archive member.
            data = source.read(info)
            if hashlib.sha256(data).hexdigest() != entry["sha256"]:
                raise ValueError(f"member hash mismatch: {entry['member']}")
            target.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(dir=target.parent, suffix=".part", delete=False) as output:
                temporary = Path(output.name)
                output.write(data)
            try:
                os.replace(temporary, target)
            finally:
                temporary.unlink(missing_ok=True)
    print(f"verified {pack['name']}: {len(pack['files'])} audio/chart members in {root}", flush=True)


def main():
    decode = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=decode / "vorbis/validation/songpacks.json")
    parser.add_argument("--destination", type=Path, default=decode.parents[2] / "target/deadlib-audio-reference/corpus")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    if manifest["schema"] != 1:
        raise ValueError("unsupported song pack manifest")
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    for pack in manifest["packs"]:
        fetch(pack, destination, args.offline)


if __name__ == "__main__":
    main()
