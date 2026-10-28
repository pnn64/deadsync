"""Source-pyramid gate: independent sampling/filter/FDCT and C frame decoding.

The C reference's Pt>0 initial predictor and omitted-EXP state are defective.
A separate C-input copy clears base Pt (with shifted base expectations) and
inserts EXP00 when the reference incorrectly retains previous expansion flags.
Original native JPEG bytes and differential entropy remain unchanged. No C
final-raster comparison is claimed.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure, samples, check_frame_data

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "reference", "out"):
    parser.add_argument("--" + name, type=Path, required=True)
parser.add_argument("--resume", action="store_true")
parser.add_argument("--fixtures", type=Path, help="Retain twelve representative C-checked source encodes and pre-entropy expectations")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
tool_dir = args.out / "tools"
tool_dir.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "inspect", "reference"):
    source = getattr(args, name)
    target = tool_dir / (name + source.suffix)
    if not args.resume:
        shutil.copy2(source, target)
        assert source.read_bytes() == target.read_bytes()
    hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    setattr(args, name, target.resolve())
checkpoint = args.out / "checkpoint.jsonl"
if args.resume:
    rows = [json.loads(r) for r in checkpoint.read_text().splitlines()]
    assert rows[0] == {"tools": hashes}
    done = {r["case"]: r for r in rows[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": hashes}) + "\n")
    done = {}
work = args.out / "case"
work.mkdir(exist_ok=True)


def run(command, env=None):
    result = subprocess.run([str(v) for v in command], capture_output=True, env=env)
    assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.stderr.decode(errors="replace"))


def ceildiv(x, y):
    return (x + y - 1) // y


def planes(data):
    return [([list(v[y * s[3]:(y + 1) * s[3]]) for y in range(s[4])]) for s, v in samples(data)[3]]


def expand(rows, flags, width, height):
    if flags & 0x10:
        rows = [[v for a, b in zip(row, row[1:] + row[-1:]) for v in (a, (a + b) // 2)] for row in rows]
    if flags & 1:
        rows = [row for a, b in zip(rows, rows[1:] + rows[-1:]) for row in (a, [(x + y) // 2 for x, y in zip(a, b)])]
    result = [row[:width] for row in rows[:height]]
    assert len(result) == height and all(len(row) == width for row in result)
    return result


def source_planes(values, width, height, components):
    """Integrate rational component footprints; replicate source edge samples."""
    mh, mv = max(c[1] for c in components), max(c[2] for c in components)
    count, output = len(components), []
    for i, (_, sh, sv) in enumerate(components):
        rows = []
        for y in range(ceildiv(height * sv, mv)):
            row = []
            for x in range(ceildiv(width * sh, mh)):
                x0, x1, y0, y1 = x * mh, (x + 1) * mh, y * mv, (y + 1) * mv
                total = 0
                for sy in range(y0 // sv, ceildiv(y1, sv)):
                    for sx in range(x0 // sh, ceildiv(x1, sh)):
                        weight = (min(x1, (sx + 1) * sh) - max(x0, sx * sh)) * (min(y1, (sy + 1) * sv) - max(y0, sy * sv))
                        total += weight * values[(min(sy, height - 1) * width + min(sx, width - 1)) * count + i]
                row.append((total + mh * mv // 2) // (mh * mv))
            rows.append(row)
        output.append(rows)
    return output


def downsample(source, frame, flags):
    mh, mv = max(c[1] for c in frame["components"]), max(c[2] for c in frame["components"])
    horizontal = [(-1, 1), (0, 2), (1, 1)] if flags & 0x10 else [(0, 1)]
    vertical = [(-1, 1), (0, 2), (1, 1)] if flags & 1 else [(0, 1)]
    divisor = sum(w for _, w in horizontal) * sum(w for _, w in vertical)
    output = []
    for old, (_, sh, sv) in zip(source, frame["components"]):
        rows = []
        for y in range(ceildiv(frame["height"] * sv, mv)):
            row = []
            for x in range(ceildiv(frame["width"] * sh, mh)):
                sx, sy = x * (2 if flags & 0x10 else 1), y * (2 if flags & 1 else 1)
                total = sum(old[min(max(sy + dy, 0), len(old) - 1)][min(max(sx + dx, 0), len(old[0]) - 1)] * wx * wy
                            for dy, wy in vertical for dx, wx in horizontal)
                row.append((total + divisor // 2) // divisor)
            rows.append(row)
        output.append(rows)
    return output


basis = [[0.5 * (math.sqrt(0.5) if k == 0 else 1) * math.cos((2 * x + 1) * k * math.pi / 16) for x in range(8)] for k in range(8)]


def coefficients(data):
    assert data[:4] == b"JCO1"
    count = struct.unpack_from("<I", data, 16)[0]
    result, pos = [], 20
    for _ in range(count):
        shape = struct.unpack_from("<7I", data, pos)
        quant = struct.unpack_from("<64H", data, pos + 28)
        pos += 156
        length = shape[5] * shape[6] * 64
        values = struct.unpack_from("<" + "h" * length, data, pos)
        pos += 2 * length
        result.append((shape, quant, values))
    assert pos == len(data)
    return result


def float_fdct(rows, quant):
    horizontal = [[sum(row[x] * basis[k][x] for x in range(8)) for k in range(8)] for row in rows]
    transformed = [sum(horizontal[y][kx] * basis[ky][y] for y in range(8)) / quant[ky * 8 + kx] for ky in range(8) for kx in range(8)]
    return [math.floor(v + 0.5) if v >= 0 else math.ceil(v - 0.5) for v in transformed]


def validate_source(frames, dumps, references, original, base_point, mid_point, partial):
    targets = [None] * len(frames)
    targets[-1] = original
    for i in range(len(frames) - 2, -1, -1):
        targets[i] = downsample(targets[i + 1], frames[i], frames[i + 1]["expansion"])
    maximum, blocks, lp_count = 0, 0, 0
    for fi, (frame, data, target) in enumerate(zip(frames, dumps, targets, strict=True)):
        differential = frame["marker"] in (0xc5, 0xc6, 0xc7, 0xcd, 0xce, 0xcf)
        old = references[fi - 1] if differential else None
        predicted = [expand(old[i], frame["expansion"], len(rows[0]), len(rows)) for i, rows in enumerate(target)] if old else None
        if data[:4] == b"JLP1":
            point = base_point if fi == 0 else mid_point if fi + 1 < len(frames) else 0
            expected = []
            for i, rows in enumerate(target):
                out = []
                for y, row in enumerate(rows):
                    for x, value in enumerate(row):
                        if predicted:
                            delta = (value - predicted[i][y][x] + 32768) % 65536 - 32768
                            value = ((abs(delta) >> point) << point) * (-1 if delta < 0 else 1) & 65535
                        else:
                            value = value >> point << point
                        out.append(value)
                expected.append(tuple(out))
            assert [c[1] for c in samples(data)[3]] == expected, (fi, "independent source residuals")
            lp_count += sum(len(v) for v in expected)
        else:
            for i, (shape, quant, actual) in enumerate(coefficients(data)):
                _, _, _, width, height, bw, bh = shape
                for by in range(bh):
                    for bx in range(bw):
                        rows = []
                        for y in range(8):
                            row = []
                            for x in range(8):
                                sx, sy = min(bx * 8 + x, width - 1), min(by * 8 + y, height - 1)
                                v = target[i][sy][sx]
                                v = (v - predicted[i][sy][sx] + 32768) % 65536 - 32768 if predicted else v - (1 << (frame["precision"] - 1))
                                row.append(v)
                            rows.append(row)
                        expected = float_fdct(rows, quant)
                        block = actual[(by * bw + bx) * 64:(by * bw + bx + 1) * 64]
                        if partial and fi == 0:
                            assert block[0] in {((expected[0] + delta) >> 3) << 3 for delta in (-1, 0, 1)}
                            assert not any(block[1:])
                        else:
                            error = max(abs(a - b) for a, b in zip(block, expected))
                            assert error <= 1, (fi, i, bx, by, error)
                            maximum = max(maximum, error)
                        blocks += 1
    return blocks, maximum, lp_count


def normalize_reference_input(data, expected, precision, point):
    """Documented C-only syntax/predictor workarounds; no entropy changes."""
    out, position, base, changed = bytearray(data), 2, False, 0
    previous_expansion, pending_expansion, inserts = 0, None, []
    while position < len(data):
        start = position
        assert data[position] == 255
        marker = data[position + 1]
        position += 2
        if marker == 0xd9:
            break
        size = int.from_bytes(data[position:position + 2], "big")
        end = position + size
        if marker == 0xdf:
            pending_expansion = data[end - 1]
        if marker in (0xc5, 0xc6, 0xc7, 0xcd, 0xce, 0xcf):
            if pending_expansion is None and previous_expansion:
                inserts.append(start)
            previous_expansion = pending_expansion or 0
            pending_expansion = None
        if marker in (0xc3, 0xcb):
            base = True
        elif marker in (0xc7, 0xcf):
            base = False
        if marker == 0xda:
            if base and point:
                assert out[end - 1] == point
                out[end - 1] = 0
                changed += 1
            while end < len(data):
                if data[end] != 255:
                    end += 1
                elif data[end + 1] == 0 or 0xd0 <= data[end + 1] <= 0xd7:
                    end += 2
                else:
                    break
        position = end
    for start in reversed(inserts):
        out[start:start] = b"\xff\xdf\x00\x03\x00"
    if not point:
        return bytes(out), expected, changed, len(inserts)
    assert changed
    delta = (1 << (precision - 1)) - (1 << (precision - point - 1))
    dump = bytearray(expected[0])
    pos = 20
    for shape, values in samples(dump)[3]:
        pos += 20
        restored = [(v >> point) + delta for v in values]
        struct.pack_into("<" + "H" * len(values), dump, pos, *restored)
        pos += len(values) * 2
    return bytes(out), [bytes(dump)] + expected[1:], changed, len(inserts)


def settings():
    for p in range(2, 17):
        for mode in ("lp-huf", "lp-arith"):
            for variant in range(8):
                color = ("gray", "rgb", "ycbcr", "cmyk", "ycck")[variant % 5]
                yield p, color, mode, 2 + variant % 2, 75, ("444", "420", "422")[variant % 3], variant % 3, (0 if variant % 2 == 0 else min(3, p - 1)), min(2, p - 1), False, variant % 2, False, variant % 4
    for p in (8, 12):
        for mode in ("dct-huf", "prog-huf", "dct-arith", "prog-arith"):
            for color in ("gray", "rgb", "ycbcr", "cmyk", "ycck"):
                for variant in range(8):
                    yield p, color, mode, 2 + variant % 2, (30, 75, 100)[variant % 3], ("444", "420", "422")[variant % 3], variant % 3, 0, 0, variant >= 4, variant % 2, variant % 2 == 0, variant % 4


def source_values(p, color, pattern, width, height):
    channels = 1 if color == "gray" else 4 if color in ("cmyk", "ycck") else 3
    maximum = (1 << p) - 1
    def pixel(x, y, c):
        if pattern == 0:
            return maximum // 2
        if pattern == 1:
            return (x * 193 + y * 317 + c * 911) & maximum
        if pattern == 2:
            return (x * 7319 + y * y * 117 + c * 771 + x * y * 131) & maximum
        return maximum if (x + y + c) % 2 else 0
    return [pixel(x, y, c) for y in range(height) for x in range(width) for c in range(channels)]


records = []
retained = []
for values in settings():
    p, color, mode, depth, quality, sampling, restart, base_point, mid_point, partial, fixed, correction, pattern = values
    name = "-".join(str(v) for v in values)
    selected = ((p in (2, 16) and mode.startswith("lp-") and color == "gray" and pattern == 1)
                or (not mode.startswith("lp-") and color == "ycck" and partial and pattern == 2))
    width, height = 17, 24
    original = source_values(p, color, pattern, width, height)
    raw = struct.pack("<" + "H" * len(original), *original)
    record = done.get(name)
    if record is not None:
        assert record["source_sha256"] == hashlib.sha256(raw).hexdigest(), (name, "changed source samples")
    if record is None:
        (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
        source = work / "source.raw"
        source.write_bytes(raw)
        image = work / "image.jpg"
        run([args.native, source, image, p, width, height, color, mode, depth, quality, sampling, restart, base_point, mid_point, int(partial), fixed, int(correction)])
        encoded = image.read_bytes()
        header, frames, unchanged, dac, exp = structure(encoded)
        assert encoded == unchanged and dac == exp == 0
        dumps = [image.with_suffix(f".frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}").read_bytes() for i, f in enumerate(frames)]
        references = [planes(image.with_suffix(f".reference-{i}.samples").read_bytes()) for i in range(len(frames))]
        target = source_planes(original, width, height, header["components"])
        blocks, error, lp_count = validate_source(frames, dumps, references, target, base_point, mid_point, partial)
        run([args.inspect, image, work / "native.samples", "raw", "frames"])
        native_dumps = [(work / f"native.frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}").read_bytes() for i, f in enumerate(frames)]
        assert dumps == native_dumps
        assert planes((work / "native.samples").read_bytes()) == references[-1]
        exact_source = mode.startswith("lp-") or correction
        if exact_source:
            assert references[-1] == target, (name, "final source samples")
        normalized, c_dumps, changed, exp_zero = normalize_reference_input(encoded, dumps, p, base_point)
        c_input = work / "reference.jpg"
        c_input.write_bytes(normalized)
        observation = work / "observations.txt"
        observation.write_text("")
        run([args.reference, "-c", "-U", c_input, work / "pgx"], os.environ | {"DEADLIB_JPEG_FRAME_DUMP": str(observation.resolve())})
        c_blocks, c_samples = check_frame_data(frames, c_dumps, observation.read_text())
        record = {"case": name, "sha256": hashlib.sha256(encoded).hexdigest(), "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                  "frame_data_sha256": hashlib.sha256(b"".join(dumps)).hexdigest(),
                  "reference_input_sha256": hashlib.sha256(normalized).hexdigest(), "bytes": len(encoded), "markers": [f["marker"] for f in frames],
                  "exact_reference_dct_blocks": c_blocks, "exact_reference_lossless_samples": c_samples, "base_point_normalized_scans": changed,
                  "inserted_exp_zero_markers": exp_zero,
                  "independent_source_lossless_samples": lp_count, "independent_float_fdct_blocks": blocks, "max_full_precision_fdct_error": error,
                  "exact_final_source": exact_source, "reference_warnings": 0}
        with checkpoint.open("a") as file:
            file.write(json.dumps(record) + "\n")
        if args.fixtures and selected:
            args.fixtures.mkdir(parents=True, exist_ok=True)
            (args.fixtures / (name + ".jpg")).write_bytes(encoded)
            (args.fixtures / (name + ".source.raw")).write_bytes(source.read_bytes())
            (args.fixtures / (name + ".samples")).write_bytes((work / "native.samples").read_bytes())
            for i, (frame, dump) in enumerate(zip(frames, dumps, strict=True)):
                suffix = "samples" if frame["marker"] & 3 == 3 else "coef"
                (args.fixtures / f"{name}.frame-{i}.{suffix}").write_bytes(dump)
    if args.fixtures and selected:
        assert (args.fixtures / (name + ".jpg")).exists(), "Retaining fixtures from an old checkpoint requires a fresh output directory"
        retained.append(record)
    records.append(record)
    if len(records) % 50 == 0:
        print(f"Checked {len(records)} source pyramids", flush=True)
report = {"reference": "thorfdbg/libjpeg JPEG XT", "commit": "702114c8130ae818ccd2354cc03e879dac69e579", "tools": hashes,
          "cases": len(records), "reference_warnings": 0, "results": records,
          "scope": "Native raster-to-hierarchy source encoding. Independent rational sampling, K.5 product 1-2-1 filtering with one rounding, signed Pt and float FDCT (<=1 quantized level); partial DC masks applied to +/-1 candidates. Exact C/native pre-reconstruction frame values. C-input copies normalize base Pt>0 and insert EXP00 for stale C expansion state; no C final-raster claim."}
for field in ("exact_reference_dct_blocks", "exact_reference_lossless_samples", "base_point_normalized_scans", "inserted_exp_zero_markers", "independent_source_lossless_samples", "independent_float_fdct_blocks", "exact_final_source"):
    report[field] = sum(r.get(field, 0) for r in records)
report["max_full_precision_fdct_error"] = max(r["max_full_precision_fdct_error"] for r in records)
report["direct_reference_cases"] = sum(r["base_point_normalized_scans"] == 0 and r.get("inserted_exp_zero_markers", 0) == 0 for r in records)
report["reference_patch_sha256"] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (Path(__file__).parent / "oracle").glob("jpegxt-*.patch")}
if args.fixtures:
    manifest = {"provenance": "Native source-pyramid encodes with pre-entropy expectations; independently checked by the C frame observer and Python source/FDCT models. Not an independent standards conformance corpus.",
                "generator": "examples/pyramid_encode.rs and tests/reference_pyramid.py --fixtures", "tools": hashes, "cases": retained,
                "files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(args.fixtures.iterdir()) if p.suffix in (".jpg", ".raw", ".coef", ".samples")}}
    (args.fixtures / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
