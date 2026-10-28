"""Native lossless encodes decoded by C and Rust with exact source checks."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "oracle", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--resume", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "inspect", "oracle"):
    source = getattr(args, name)
    target = snapshots / (name + source.suffix)
    if not args.resume:
        shutil.copy2(source, target)
        assert target.read_bytes() == source.read_bytes()
    hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    setattr(args, name, target.resolve())
checkpoint = args.out / "checkpoint.jsonl"
completed = {}
if args.resume:
    entries = [json.loads(line) for line in checkpoint.read_text().splitlines()]
    assert entries[0] == {"tools": hashes}
    completed = {r["case"]: r for r in entries[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": hashes}) + "\n")
work = args.out / "case"
work.mkdir(exist_ok=True)


def run(command):
    result = subprocess.run([str(x) for x in command], capture_output=True)
    assert result.returncode == 0 and not result.stderr, (command, result.stderr.decode())


def planes(data):
    assert data[:4] == b"JLP1"
    precision, width, height, count = struct.unpack_from("<4I", data, 4)
    position, components = 20, []
    for _ in range(count):
        shape = struct.unpack_from("<5I", data, position)
        position += 20
        values = struct.unpack_from("<" + "H" * (shape[3] * shape[4]), data, position)
        position += len(values) * 2
        components.append((shape, values))
    assert position == len(data)
    return precision, width, height, components


records = []


def gate(name, command, expected, settings):
    source_hash = hashlib.sha256(expected).hexdigest()
    record = completed.get(name)
    if record:
        assert record["source_sha256"] == source_hash
    else:
        (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
        image = work / "image.jpg"
        run(command)
        run([args.oracle, "decode", image, work / "c.samples"])
        run([args.inspect, image, work / "native.samples"])
        result = (work / "c.samples").read_bytes()
        assert result == (work / "native.samples").read_bytes(), (name, "C/native mismatch")
        precision, width, height, components = planes(result)
        p, w, h, original = planes(expected)
        assert (precision, width, height) == (p, w, h)
        point, layout = settings
        for i, ((shape, actual), (before, values)) in enumerate(zip(components, original, strict=True)):
            assert shape == before
            pt = (point + i) % p if layout >= 2 else point
            assert actual == tuple((v >> pt) << pt for v in values), (name, i, pt, "source mismatch")
        record = {"case": name, "source_sha256": source_hash,
                  "sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                  "samples_sha256": hashlib.sha256(result).hexdigest(),
                  "bytes": image.stat().st_size, "samples": "exact", "reference_warnings": 0}
        with checkpoint.open("a") as file:
            file.write(json.dumps(record) + "\n")
    records.append(record)
    if len(records) % 200 == 0:
        print(f"Passed {len(records)} native lossless encodes", flush=True)


# Full precision/predictor/entropy-policy cross-product, both lossless and
# maximum Pt. Scan/restart policies cycle independently across the matrix.
colors = [("gray", [1]), ("rgb", list(b"RGB")), ("ycbcr", [1, 2, 3]),
          ("cmyk", list(b"CMYK")), ("cmykinv", list(b"CMYK")), ("ycck", [1, 2, 3, 4])]
for p in range(2, 17):
    for predictor in range(1, 8):
        for point in (0, p - 1):
            for color_index, (color, ids) in enumerate(colors):
                width, height, count = 13, 19, len(ids)
                state, source = 0x9e3779b9, []
                maximum = (1 << p) - 1
                for i in range(width * height * count):
                    state = (state * 1664525 + 1013904223) & 0xffffffff
                    source.append((0 if i % 11 == 0 else maximum if i % 7 == 0 else state >> 7) & maximum)
                raw = struct.pack("<" + "H" * len(source), *source)
                (work / "input.raw").write_bytes(raw)
                expected = bytearray(b"JLP1" + struct.pack("<4I", p, width, height, count))
                for ci, component_id in enumerate(ids):
                    expected += struct.pack("<5I", component_id, 1, 1, width, height)
                    expected += struct.pack("<" + "H" * (width * height), *source[ci::count])
                for optimize in (0, 1):
                    layout = (p + predictor + color_index + optimize) % 4
                    restart = (0, 1, 3)[(p + predictor + optimize) % 3]
                    name = f"raw-p{p}-v{predictor}-t{point}-{color}-l{layout}-r{restart}-o{optimize}"
                    command = [args.native, "raw", work / "input.raw", work / "image.jpg", width,
                               height, p, color, predictor, point, restart, layout, optimize]
                    gate(name, command, expected, (point, layout))

# Every Pt at boundary precisions, alternating extrema, full integer predictor
# over/underflow, and restart sequence wraparound with both Huffman policies.
for p in (2, 8, 12, 15, 16):
    for point in range(p):
        for predictor in (4, 5, 6, 7):
            for color, ids in (("gray", [1]), ("cmyk", list(b"CMYK"))):
                width, height, count = 9, 19, len(ids)
                maximum = (1 << p) - 1
                source = [maximum if ((i // count) % width + i // (count * width) + i % count) % 2 else 0
                          for i in range(width * height * count)]
                (work / "input.raw").write_bytes(struct.pack("<" + "H" * len(source), *source))
                expected = bytearray(b"JLP1" + struct.pack("<4I", p, width, height, count))
                for ci, component_id in enumerate(ids):
                    expected += struct.pack("<5I", component_id, 1, 1, width, height)
                    expected += struct.pack("<" + "H" * (width * height), *source[ci::count])
                for optimize in (0, 1):
                    name = f"boundary-p{p}-v{predictor}-t{point}-{color}-r1-o{optimize}"
                    command = [args.native, "raw", work / "input.raw", work / "image.jpg", width,
                               height, p, color, predictor, point, 1, 0, optimize]
                    gate(name, command, expected, (point, 0))

# Independently C-encoded sources, including pre-existing Pt. The rewritten
# integers are authoritative, so Pt=0 preserves their already reconstructed bits.
root = Path(__file__).resolve().parent / "fixtures" / "lossless"
for path in sorted(root.glob("*.jpg")):
    expected = path.with_suffix(".samples").read_bytes()
    p, _, _, _ = planes(expected)
    for predictor in (1, 4, 7):
        for optimize in (0, 1):
            for layout in (0, 1, 2, 3):
                point = 0 if layout < 2 else min(3, p - 1)
                restart = (0, 1, 3, 2)[layout]
                name = f"rewrite-{path.stem}-v{predictor}-t{point}-l{layout}-r{restart}-o{optimize}"
                command = [args.native, "rewrite", path, work / "image.jpg", predictor,
                           point, restart, layout, optimize]
                gate(name, command, expected, (point, layout))
report = {"reference": "libjpeg-turbo 3.2.1 lossless sample decoder", "tools": hashes,
          "cases": len(records), "raw_cases": 15 * 7 * 2 * 6 * 2,
          "boundary_cases": (2 + 8 + 12 + 15 + 16) * 4 * 2 * 2,
          "rewrite_cases": 45 * 3 * 2 * 4, "reference_warnings": 0,
          "scope": "native SOF3 encoding; P=2..16, PSV=1..7, 1x1 factors, gray/RGB/YCbCr/CMYK/YCCK",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
