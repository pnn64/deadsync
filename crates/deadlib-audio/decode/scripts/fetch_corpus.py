"""Fetch hash-pinned external test media into ignored scratch storage."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
from urllib.request import Request, urlopen


def matches(path, entry):
    if not path.is_file() or path.stat().st_size != entry["bytes"]:
        return False
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest() == entry["sha256"]


def fetch(root, entry):
    destination = (root / entry["path"]).resolve()
    if not destination.is_relative_to(root):
        raise ValueError(f"corpus path escapes destination: {entry['path']}")
    if matches(destination, entry):
        print(f"verified {entry['path']}", flush=True)
        return
    if not entry["url"].startswith("https://"):
        raise ValueError("corpus downloads require HTTPS")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        request = Request(entry["url"], headers={"User-Agent": "deadlib-audio-corpus/1"})
        with urlopen(request, timeout=30) as source:
            with tempfile.NamedTemporaryFile(dir=destination.parent, prefix=destination.name + ".", suffix=".part", delete=False) as output:
                temporary = Path(output.name)
                size = 0
                digest = hashlib.sha256()
                while chunk := source.read(1 << 20):
                    size += len(chunk)
                    if size > entry["bytes"]:
                        raise ValueError(f"oversized download: {entry['path']}")
                    digest.update(chunk)
                    output.write(chunk)
        if size != entry["bytes"] or digest.hexdigest() != entry["sha256"]:
            raise ValueError(f"corpus hash/size mismatch: {entry['path']}")
        os.replace(temporary, destination)
        temporary = None
        print(f"downloaded {entry['path']}", flush=True)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main():
    decode = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=decode / "vorbis/validation/corpus.json")
    parser.add_argument("--destination", type=Path, default=decode.parents[2] / "target/deadlib-audio-reference/corpus")
    parser.add_argument("--group", action="append", default=[])
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    if manifest["schema"] != 1:
        raise ValueError("unsupported corpus manifest")
    root = args.destination.resolve()
    root.mkdir(parents=True, exist_ok=True)
    selected = [entry for entry in manifest["files"] if not args.group or entry["group"] in args.group]
    if not selected:
        raise ValueError("no matching corpus group")
    for entry in selected:
        fetch(root, entry)


if __name__ == "__main__":
    main()
