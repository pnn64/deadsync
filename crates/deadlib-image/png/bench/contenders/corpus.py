"""Prepare TSV input lists from the independent format corpus and existing PNGs."""

import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
from urllib.error import URLError
from urllib.request import urlopen
import zlib


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def animation(path, width, height, frames, kind):
    out = bytearray(b"\x89PNG\r\n\x1a\n")
    out += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    separate = kind == "poster"
    out += chunk(b"acTL", struct.pack(">II", frames, 2))
    sequence = 0
    for index in range(frames + separate):
        cropped = kind != "full" and index > 0
        w, h = (width // 2, height // 2) if cropped else (width, height)
        x, y = (width // 5, height // 4) if cropped else (0, 0)
        row = b"\0" + b"".join(bytes((i % 256, index * 15 % 256, 240, 127 if cropped else 255)) for i in range(w))
        data = zlib.compress(row * h, 6)
        if not (separate and index == 0):
            control = struct.pack(">IIIIIHHBB", sequence, w, h, x, y, index + 1, 60,
                                  index % 3 if cropped else 0, int(cropped))
            out += chunk(b"fcTL", control)
            sequence += 1
        if index == 0:
            out += chunk(b"IDAT", data)
        else:
            out += chunk(b"fdAT", struct.pack(">I", sequence) + data)
            sequence += 1
    path.write_bytes(out + chunk(b"IEND", b""))


def write_manifests(repo, work):
    fixtures = repo / "crates/deadlib-image/png/tests/fixtures"
    bench = Path(__file__).resolve().parents[1]
    formats = sorted((work / "formats").glob("*.png"))
    assert len(formats) == 60
    photos = []
    (work / "kodak").mkdir(exist_ok=True)
    for entry in json.loads((bench.parent / "scripts/kodak.json").read_text())["files"]:
        path = work / "kodak" / entry["file"]
        if not path.exists():
            for suffix in ("", "?verify=1", "?verify=2"):
                try:
                    with urlopen(entry["url"] + suffix, timeout=30) as response:
                        data = response.read()
                except URLError:
                    continue
                if hashlib.sha256(data).hexdigest() == entry["sha256"]:
                    path.write_bytes(data)
                    break
        assert hashlib.sha256(path.read_bytes()).hexdigest() == entry["sha256"], path
        photos.append(path)
    assets = []
    archive = bench / "results/windows-x86_64-20261119-rgba8/corpus.json"
    for entry in json.loads(archive.read_text())["inputs"]:
        if entry["group"] == "assets":
            path = repo / entry["original_asset_path"]
            if path.is_file():
                assets.append(path)
    assert assets
    pngsuite = sorted(path for path in (fixtures / "pngsuite").glob("*.png")
                      if path.name.startswith(("t", "f", "s", "z", "g", "i", "o", "p", "c")) and path.name != "PngSuite.png")

    screenshots = []
    archive = bench / "results/2026-11-20-encode/corpus.json"
    for entry in json.loads(archive.read_text())["files"]:
        path = repo / entry["path"]
        if entry["group"] == "screenshots" and path.is_file():
            screenshots.append(path)
    assert screenshots

    from PIL import Image
    generated = work / "generated"
    generated.mkdir(exist_ok=True)
    synthetic = []
    for width, height in [(17, 13), (640, 360), (1920, 1080), (3840, 2160)]:
        for name in ("flat", "gradient", "ui-alpha", "noise"):
            if name == "noise":
                data = random.Random(6281).randbytes(width * height * 4)
            elif name == "flat":
                data = bytes((16, 40, 80, 255)) * (width * height)
            elif name == "gradient":
                row = b"".join(bytes((x * 255 // width, x % 256, 144, 255)) for x in range(width))
                data = row * height
            else:
                rows = []
                for band in range(8):
                    row = b"".join(bytes((band * 28, 140, x // 48 * 29 % 256, x // 32 * 51 % 256))
                                   for x in range(width))
                    rows.append(row * (((band+1)*height//8) - (band*height//8)))
                data = b"".join(rows)
            path = generated / f"{name}-{width}x{height}.png"
            Image.frombytes("RGBA", (width, height), data).save(path, compress_level=1)
            synthetic.append(path)

    animations = []
    for width, height, frames in [(63, 37, 4), (640, 360, 8), (1280, 720, 12)]:
        for kind in ("full", "crop", "poster"):
            path = generated / f"apng-{kind}-{width}x{height}.png"
            animation(path, width, height, frames, kind)
            animations.append(path)

    groups = {
        "decode": [("formats", formats), ("kodak", photos), ("assets", assets), ("pngsuite", pngsuite)],
        "encode": [("formats", formats), ("screenshots", screenshots), ("photos", photos[::4]),
                   ("generated", synthetic)],
        "fpng": [("screenshots", screenshots), ("photos", photos[::4]), ("generated", synthetic)],
        "animation": [("animated", animations)],
    }
    inputs = {}
    for mode, selection in groups.items():
        entries = [(group, path.relative_to(repo).as_posix())
                   for group, paths in selection for path in paths]
        (work / f"{mode}-inputs.tsv").write_text(
            "".join(f"{group}\t{path}\n" for group, path in entries), encoding="utf-8")
        inputs[mode] = [{"group": group, "path": path,
                         "sha256": hashlib.sha256((repo / path).read_bytes()).hexdigest()}
                        for group, path in entries]
        print(mode, {group: len(paths) for group, paths in selection})
    (work / "inputs.json").write_text(json.dumps(inputs, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("repo", type=Path)
    cli.add_argument("output", type=Path)
    args = cli.parse_args()
    write_manifests(args.repo.resolve(), args.output.resolve())
