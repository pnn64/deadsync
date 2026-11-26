"""Append BMP Suite, deterministic RLE and Kodak cases to the prepared manifests."""

import argparse
import hashlib
import io
import json
import re
import struct
import urllib.request
from pathlib import Path

from PIL import Image


def read_bmp_header(data):
    dib = struct.unpack_from("<I", data, 14)[0]
    if dib == 12:
        w, h, _, depth = struct.unpack_from("<HHHH", data, 18)
    else:
        w, h, _, depth = struct.unpack_from("<iiHH", data, 18)
    compression = struct.unpack_from("<I", data, 30)[0] if dib >= 40 else 0
    return w, abs(h), dib, depth, compression


def rgba(path):
    with Image.open(path) as image:
        return image.size, image.convert("RGBA").tobytes()


def add_suite(dest, suite, rows):
    rendering = {}
    for row in (suite / "html/bmpsuite.html").read_text(encoding="utf-8").split("<tr>"):
        bmp = re.search(r'<td[^>]*>([gqx]/[^<]+\.bmp)</td>', row)
        png = re.search(r'<img[^>]* src="([^"/]+\.png)"', row)
        if bmp and png:
            rendering[bmp[1]] = suite / "html" / png[1]

    for path in sorted(suite.glob("[gqx]/*.bmp")):
        key = path.relative_to(suite).as_posix()
        if "rlecut" in key or "rletrns" in key:
            continue  # Multiple plausible background/alpha interpretations.
        data = path.read_bytes()
        bitmap_array = data.startswith(b"BA")
        bmp = data[14:] if bitmap_array else data
        w, h, dib, depth, compression = read_bmp_header(bmp)
        group = f"dib{dib}-{depth}bpp-bi{compression}"
        if bitmap_array:
            group = f"bitmap-array-{group}"
        reference = rendering.get(key)
        name = f"suite-{path.parent.name}-{path.stem}"
        try:
            if reference and path.stem not in ("pal8nonsquare", "rgb24prof2"):
                shape, expected = rgba(reference)
                oracle = f"suite/{reference.name}"
            elif compression in (4, 5) and dib in (40, 108, 124):
                offset = struct.unpack_from("<I", data, 10)[0]
                shape, expected = rgba(io.BytesIO(data[offset:]))
                oracle = "embedded-image/Pillow"
            else:
                shape, expected = rgba(path)
                oracle = "Pillow"
        except (OSError, ValueError):
            shape, expected, oracle = None, None, "-"
        if shape != (w, h):
            expected, oracle = None, "-"
        raw = dest / f"{name}.rgba" if expected is not None else "-"
        if expected is not None:
            raw.write_bytes(expected)
        rows.append(("bmpsuite", group, name, w, h, path, raw, oracle))


def add_rle(dest, rows):
    for depth, compression in [(4, 2), (8, 1)]:
        color_count = 1 << depth
        colors = [bytes((i, (5 * i) % 256, (53 * i) % 256, 255)) for i in range(color_count)]
        table = b"".join(bytes((b, g, r, 0)) for r, g, b, _ in colors)
        for w, h in [(511, 257), (1920, 1080)]:
            for stream in ("mixed", "runs"):
                name = f"rle{depth}-{stream}-{w}x{h}"
                raster = bytearray()
                output = [b""] * h
                for y in reversed(range(h)):
                    row = bytearray()
                    x = 0
                    while x < w:
                        count = min(113 + (y + x) % 101, w - x)
                        first = (y * 7 + x) % color_count
                        second = (first + 3) % color_count if depth == 4 else first
                        if stream == "mixed" and x == 0:
                            indices = [(first + i * i + i * 3) % color_count for i in range(count)]
                            row.extend(b"".join(colors[i] for i in indices))
                            raster.extend((0, count))
                            if depth == 4:
                                if count % 2:
                                    indices.append(0)
                                literal = bytes(a << 4 | b for a, b in zip(indices[::2], indices[1::2]))
                            else:
                                literal = bytes(indices)
                            raster.extend(literal)
                            if len(literal) % 2:
                                raster.append(0)
                        else:
                            raster.extend((count, (first << 4) | second if depth == 4 else first))
                            row.extend((colors[first] + colors[second]) * (count // 2))
                            if count % 2:
                                row.extend(colors[first])
                        x += count
                    if y:
                        raster.extend((0, 0))
                    output[y] = bytes(row)
                raster.extend((0, 1))
                offset = 54 + len(table)
                header = struct.pack("<2sIHHI", b"BM", offset + len(raster), 0, 0, offset)
                dib = struct.pack("<IiiHHIIIIII", 40, w, h, 1, depth, compression, len(raster),
                                  0, 0, color_count, 0)
                path, raw = dest / f"{name}.bmp", dest / f"{name}.rgba"
                path.write_bytes(header + dib + table + raster)
                raw.write_bytes(b"".join(output))
                rows.append(("rle", f"rle{depth}", name, w, h, path, raw, "generator/palette"))


def add_kodak(dest, cache, decode, encode):
    manifest = Path(__file__).resolve().parents[3] / "png/scripts/kodak.json"
    entries = json.loads(manifest.read_text())["files"]
    cache.mkdir(parents=True, exist_ok=True)
    for i in (1, 3, 6, 9, 13, 17, 20, 23):
        entry = entries[i - 1]
        png = cache / entry["file"]
        if not png.exists():
            with urllib.request.urlopen(entry["url"], timeout=60) as response:
                png.write_bytes(response.read())
        if hashlib.sha256(png.read_bytes()).hexdigest() != entry["sha256"]:
            raise ValueError(f"Unexpected source hash for {png}")
        name = f"kodak-{png.stem}"
        path, raw = dest / f"{name}.bmp", dest / f"{name}.rgba"
        with Image.open(png) as photo:
            w, h = photo.size
            photo.convert("RGB").save(path, format="BMP")
            raw.write_bytes(photo.convert("RGBA").tobytes())
        decode.append(("kodak", "rgb24", name, w, h, path, raw, "Kodak PNG"))
        encode.append(("kodak", "rgb24", name, w, h, raw, "bottom"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prepared", type=Path)
    parser.add_argument("suite", type=Path)
    parser.add_argument("kodak", type=Path)
    args = parser.parse_args()
    decode, encode = [], []
    add_suite(args.prepared, args.suite, decode)
    add_rle(args.prepared, decode)
    add_kodak(args.prepared, args.kodak, decode, encode)
    for kind, rows in [("decode", decode), ("encode", encode)]:
        path = args.prepared / f"{kind}.tsv"
        synthetic = [s for s in path.read_text(encoding="utf-8").splitlines() if s.startswith("synthetic\t")]
        lines = synthetic + ["\t".join(map(str, row)) for row in rows]
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"{kind}: {len(lines)} cases")


if __name__ == "__main__":
    main()
