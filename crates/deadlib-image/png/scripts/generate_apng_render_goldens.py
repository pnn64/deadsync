"""Generate independent RGBA16 APNG answers using libpng and exact fractions.

Only test fixtures use this script; neither Python nor libpng is a codec
dependency. A fresh --output directory is mandatory so existing evidence is
never overwritten. libpng decodes each original frame as a standalone PNG.
This script composes its native samples independently, using full-canvas
snapshots and disposal immediately after recording each displayed frame.
"""
from argparse import ArgumentParser
from fractions import Fraction
from functools import lru_cache
from pathlib import Path
import hashlib
import json
import struct
import subprocess
import tempfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tests/fixtures/wpt-apng"
SIGNATURE = b"\x89PNG\r\n\x1a\n"


def chunks(data):
    assert data[:8] == SIGNATURE
    offset = 8
    while offset < len(data):
        length, = struct.unpack_from(">I", data, offset)
        kind = data[offset + 4:offset + 8]
        body = data[offset + 8:offset + 8 + length]
        crc, = struct.unpack_from(">I", data, offset + 8 + length)
        assert zlib.crc32(kind + body) == crc
        yield kind, body
        offset += 12 + length
    assert offset == len(data)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def parse(data):
    header = None
    ancillary = []
    default = bytearray()
    frames = []
    animation = None
    saw_idat = False
    for kind, body in chunks(data):
        if kind == b"IHDR":
            header = body
        elif kind in [b"PLTE", b"tRNS"]:
            ancillary.append((kind, body))
        elif kind == b"acTL":
            assert animation is None and not saw_idat
            animation = struct.unpack(">II", body)
        elif kind == b"fcTL":
            assert not frames or saw_idat
            fields = struct.unpack(">IIIIIHHBB", body)
            frames.append({"fields": fields[1:], "control": body[4:],
                           "default": not saw_idat, "data": bytearray()})
        elif kind == b"IDAT":
            assert animation is not None
            saw_idat = True
            default.extend(body)
            if frames and frames[-1]["default"]:
                frames[-1]["data"].extend(body)
        elif kind == b"fdAT":
            frames[-1]["data"].extend(body[4:])
    assert header is not None
    assert animation is not None and animation[0] == len(frames)
    return header, ancillary, default, frames, animation


def decode(oracle, scratch, header, ancillary, data, width, height):
    h = struct.pack(">II", width, height) + header[8:]
    png = SIGNATURE + chunk(b"IHDR", h)
    png += b"".join(chunk(k, d) for k, d in ancillary)
    png += chunk(b"IDAT", data) + chunk(b"IEND", b"")
    scratch.write_bytes(png)
    result = subprocess.run([str(oracle), str(scratch)], capture_output=True, check=True)
    w, h, depth, color = struct.unpack(">IIII", result.stdout[:16])
    assert (w, h, depth, color) == (width, height, header[8], header[9])
    raw = result.stdout[16:]
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color]
    stride = (width * channels * depth + 7) // 8
    assert len(raw) == stride * height
    palette = next((d for k, d in ancillary if k == b"PLTE"), b"")
    trns = next((d for k, d in ancillary if k == b"tRNS"), b"")
    key = struct.unpack(">" + "H" * (len(trns) // 2), trns) if color in [0, 2] and trns else None
    mask = (1 << depth) - 1
    pixels = []
    for y in range(height):
        row = raw[y * stride:(y + 1) * stride]
        if depth == 16:
            values = struct.unpack(">" + "H" * (len(row) // 2), row)
        elif depth == 8:
            values = row
        else:
            values = [(row[x * depth // 8] >> (8 - depth - x * depth % 8)) & mask for x in range(width)]
        for x in range(width):
            v = values[x * channels:(x + 1) * channels]
            scaled = [n * 65535 // mask for n in v]
            if color == 0:
                pixels.append((scaled[0],) * 3 + (0 if key and v[0] == (key[0] & mask) else 65535,))
            elif color == 2:
                alpha = 0 if key and tuple(v) == tuple(k & mask for k in key) else 65535
                pixels.append(tuple(scaled) + (alpha,))
            elif color == 3:
                index = v[0]
                rgb = tuple(n * 257 for n in palette[index * 3:index * 3 + 3])
                assert len(rgb) == 3
                pixels.append(rgb + ((trns[index] if index < len(trns) else 255) * 257,))
            elif color == 4:
                pixels.append((scaled[0],) * 3 + (scaled[1],))
            else:
                pixels.append(tuple(scaled))
    return pixels


def nearest(value):
    return (2 * value.numerator + value.denominator) // (2 * value.denominator)


@lru_cache(maxsize=None)
def over(foreground, background):
    if foreground[3] == 0:
        return background
    af = Fraction(foreground[3], 65535)
    ab = Fraction(background[3], 65535)
    alpha = af + ab * (1 - af)
    colors = tuple(nearest((f * af + b * ab * (1 - af)) / alpha)
                   for f, b in zip(foreground[:3], background[:3]))
    return colors + (nearest(alpha * 65535),)


def packed(pixels):
    return b"".join(struct.pack(">4H", *pixel) for pixel in pixels)


def generate(path, oracle, scratch):
    header, ancillary, default, frames, animation = parse(path.read_bytes())
    width, height = struct.unpack(">II", header[:8])
    role = bool(frames and frames[0]["default"])
    plays = animation[1]
    records = bytearray(b"DLAPNGR1" + struct.pack(">6I", width, height, 1, role, plays, len(frames)))
    canvas = [(0, 0, 0, 0)] * (width * height)
    for index, frame in enumerate(frames):
        w, h, x0, y0, _, _, disposal, blend = frame["fields"]
        image = decode(oracle, scratch, header, ancillary, frame["data"], w, h)
        before = canvas.copy()
        for y in range(h):
            for x in range(w):
                dest = (y0 + y) * width + x0 + x
                pixel = image[y * w + x]
                canvas[dest] = pixel if blend == 0 else over(pixel, canvas[dest])
        records.extend(frame["control"] + packed(canvas))
        # Save the displayed result first, then apply this frame's disposal.
        if disposal == 2 and index > 0:
            canvas = before
        elif disposal == 1 or disposal == 2:
            for y in range(h):
                start = (y0 + y) * width + x0
                canvas[start:start + w] = [(0, 0, 0, 0)] * w
    return bytes(records), {"width": width, "height": height, "frames": len(frames),
                            "default_is_frame": role, "plays": plays,
                            "color_type": header[9], "bit_depth": header[8]}


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir()  # Refuse an existing destination.
    manifest = {"version": 1, "upstream_commit": json.loads((SOURCE / "upstream.json").read_text())["commit"],
                "oracle_sha256": hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
                "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                "rounding": "Exact rational straight-alpha OVER; nearest integer RGBA16 after each displayed frame",
                "files": []}
    with tempfile.TemporaryDirectory(prefix="deadlib-apng-reference-") as directory:
        scratch = Path(directory) / "frame.png"
        assert Path(directory).resolve().parent.samefile(tempfile.gettempdir())
        for path in sorted((SOURCE / "support").glob("[0-9][0-9][0-9].png")):
            data, info = generate(path, args.oracle.resolve(), scratch)
            output = args.output / (path.stem + ".rgba16.z")
            output.write_bytes(zlib.compress(data, 9))
            manifest["files"].append({"input": path.name, "input_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                                      "golden": output.name, "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                                      "uncompressed_bytes": len(data), **info})
            print(path.name, info, flush=True)
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
