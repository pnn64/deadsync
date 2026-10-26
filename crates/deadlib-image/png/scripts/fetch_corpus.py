#!/usr/bin/env python3
"""Fetch pinned, external PNG benchmark inputs without redistributing images."""
import argparse
import hashlib
import json
from pathlib import Path
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path(__file__).with_name("kodak.json"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    args.output.mkdir(parents=True, exist_ok=True)
    total = 0
    for entry in manifest["files"]:
        name = entry["file"]
        if Path(name).name != name or not name.endswith(".png"):
            raise ValueError(f"Invalid corpus filename: {name}")
        path = args.output / name
        if path.exists():
            data = path.read_bytes()
        else:
            with urllib.request.urlopen(entry["url"], timeout=45) as response:
                data = response.read(entry["bytes"] + 1)
        if len(data) != entry["bytes"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
            raise ValueError(f"Size/SHA-256 mismatch: {name}; existing files are never replaced")
        if not path.exists():
            # Exclusive creation also avoids overwriting a file created during download.
            with path.open("xb") as output:
                output.write(data)
        total += len(data)
    print(f"Verified {len(manifest['files'])} PNG files, {total} bytes: {args.output.resolve()}")


if __name__ == "__main__":
    main()
