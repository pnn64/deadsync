"""Prepare real and generated JPEG files plus lossless encoder source rasters."""

import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess
from urllib.request import urlopen

from PIL import Image


def prepare(repo, work):
    decode, encode = [], []
    data = work / "corpus"
    data.mkdir(exist_ok=True)
    jpeg = repo / "crates/deadlib-image/jpeg"
    fixtures = jpeg / "tests/fixtures"
    png_spec = json.loads((jpeg.parent / "png/scripts/kodak.json").read_text())["files"]
    cjpeg = work / "turbo-build/Release/cjpeg-static.exe"

    def add_decode(group, path):
        decode.append((group, path.relative_to(repo).as_posix()))

    def make_jpeg(image, name, *options):
        out = data / f"{name}.jpg"
        if not out.exists():
            ppm = data / f"{name}.ppm"
            image.save(ppm)
            subprocess.run([cjpeg, "-quality", "85", *options, "-outfile", out, ppm], check=True)
            ppm.unlink()
        return out

    photos = []
    for entry in png_spec:
        name = entry["file"]
        source = repo / "target/png-codec-comparison/kodak" / name
        if not source.is_file():
            source = data / name
        if not source.is_file():
            for suffix in ("", "?verify=1", "?verify=2"):
                try:
                    with urlopen(entry["url"] + suffix, timeout=20) as response:
                        payload = response.read()
                    if hashlib.sha256(payload).hexdigest() == entry["sha256"]:
                        source.write_bytes(payload)
                        break
                except OSError:
                    pass
        assert hashlib.sha256(source.read_bytes()).hexdigest() == entry["sha256"], source
        with Image.open(source) as im:
            rgb = im.convert("RGB")
        photos.append((source.stem, rgb))
        add_decode("kodak-baseline-420", make_jpeg(rgb, f"{source.stem}-baseline-420", "-sample", "2x2"))
        add_decode("kodak-progressive-420", make_jpeg(rgb, f"{source.stem}-progressive-420", "-sample", "2x2", "-progressive"))

    for name, rgb in photos[::6]:
        add_decode("kodak-gray", make_jpeg(rgb.convert("L"), f"{name}-gray", "-grayscale"))
        add_decode("kodak-rgb-444", make_jpeg(rgb, f"{name}-rgb-444", "-rgb", "-sample", "1x1"))
        path = data / f"{name}-cmyk.jpg"
        rgb.convert("CMYK").save(path, quality=85)
        add_decode("kodak-cmyk", path)
    for suffix, sample in (("444", "1x1"), ("422", "2x1"), ("440", "1x2"), ("411", "4x1"), ("410", "4x2")):
        add_decode(f"kodak-ycbcr-{suffix}", make_jpeg(photos[8][1], f"kodim09-ycbcr-{suffix}", "-sample", sample))
    for folder in ("sequential", "progressive", "arithmetic", "arithmetic-progressive", "metadata"):
        for path in sorted((fixtures / folder).glob("*.jpg")):
            add_decode(f"{folder}-fixtures", path)
    for folder in ("lossless", "lossless-arithmetic"):
        for depth in (2, 8, 10, 12, 16):
            pattern = f"p{depth}-v*-t0-l0*.jpg" if folder == "lossless" else f"reference-p{depth}-gray*.jpg"
            for path in list(sorted((fixtures / folder).glob(pattern)))[:1]:
                add_decode(folder, path)
    for folder in ("dnl", "hierarchical", "hierarchical-native", "pyramid-native"):
        candidates = list(sorted((fixtures / folder).glob("*.jpg")))
        for path in candidates[::max(1, len(candidates) // 5)][:6]:
            add_decode(folder, path)
    for path in sorted((fixtures / "t83").glob("*.JPG")):
        add_decode("itu-t83", path)
    for line in (jpeg / "bench/results/windows-x86_64-20261119-rgba8/corpus-public-repo.txt").read_text().splitlines():
        path = repo / line
        if line.startswith("assets/") and path.is_file():
            add_decode("deadsync-assets", path)

    def add_raw(name, im, group, modes):
        path = data / f"{name}.raw"
        path.write_bytes(im.tobytes())
        for mode in modes:
            encode.append((group, path.relative_to(repo).as_posix(), im.width, im.height, len(im.getbands()), 8, mode))

    for name, im in photos[::4]:
        add_raw(name, im, "photo", ("q75-420", "q90-420"))
    for path in [repo / "save/screenshots/initials-mega-verified.png",
                 repo / "tests/fixtures/interactive_song_lua/windows-before.png",
                 repo / "tests/fixtures/interactive_song_lua/windows-after.png"]:
        if path.is_file():
            with Image.open(path) as im:
                add_raw(path.stem, im.convert("RGB"), "capture", ("q75-420", "q90-420"))
    for name, im in photos[3:6]:
        add_raw(name + "-modes", im, "photo-variants",
                ("q85-444", "q85-422", "q85-440", "q85-411", "q85-410",
                 "q85-progressive", "q85-arithmetic"))
        add_raw(name + "-gray", im.convert("L"), "photo-gray", ("q85-gray", "lossless"))
    for width, height in ((37, 23), (1920, 1080)):
        row = bytes(component for x in range(width) for component in (x % 256, x // 8 % 256, 145))
        for pattern, rgb in (
            ("gradient", row * height),
            ("noise", random.Random(947).randbytes(width * height * 3)),
        ):
            add_raw(f"{pattern}-{width}x{height}", Image.frombytes("RGB", (width, height), rgb),
                    "synthetic", ("q75-420",))
    for precision, mode in ((12, "q85-444"), (16, "lossless")):
        path = data / f"ramp-{precision}.raw"
        width, height = 640, 360
        mask = (1 << precision) - 1
        path.write_bytes(b"".join(struct.pack("<H", (x*71 + y*137 + (x//17)*379) & mask)
                                  for y in range(height) for x in range(width)))
        encode.append(("high-precision", path.relative_to(repo).as_posix(), width, height, 1, precision, mode))

    (work / "decode-inputs.tsv").write_text("".join("\t".join(row) + "\n" for row in decode), encoding="utf-8")
    (work / "encode-inputs.tsv").write_text("".join("\t".join(map(str, row)) + "\n" for row in encode), encoding="utf-8")
    manifest = {path: hashlib.sha256((repo / path).read_bytes()).hexdigest()
                for path in sorted({row[1] for row in decode + encode})}
    (work / "inputs.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"{len(decode)} decoder cases, {len(encode)} encoder configurations")


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("repo", type=Path)
    cli.add_argument("work", type=Path)
    args = cli.parse_args()
    prepare(args.repo.resolve(), args.work.resolve())
