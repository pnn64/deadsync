"""Two-way exact T.81 lossless sample gate using the independent JPEG XT codec.

The pinned reference has a documented predictive table-selection setup fix;
its arithmetic coder, predictors and decoder are unchanged. See the oracle
patch and LOSSLESS-VALIDATION.md. PGX output avoids color/upsampling changes.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "reference", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--fixtures", type=Path)
parser.add_argument("--resume", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "inspect", "reference"):
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
if args.fixtures:
    args.fixtures.mkdir(parents=True, exist_ok=True)


def run(command):
    result = subprocess.run([str(x) for x in command], capture_output=True)
    # This reference CLI returns zero on some errors, so stderr is mandatory.
    assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.stderr.decode(errors="replace"))


def frame(data):
    assert data[:2] == b"\xff\xd8"
    position = 2
    while position < len(data):
        assert data[position] == 255
        marker = data[position + 1]
        length = int.from_bytes(data[position + 2:position + 4], "big")
        payload = data[position + 4:position + 2 + length]
        if marker in (0xc3, 0xcb):
            p = payload[0]
            h, w = struct.unpack_from(">HH", payload, 1)
            count = payload[5]
            assert len(payload) == 6 + 3 * count
            specs = [(payload[6 + 3 * i], payload[7 + 3 * i] >> 4, payload[7 + 3 * i] & 15) for i in range(count)]
            max_h = max(s[1] for s in specs)
            max_v = max(s[2] for s in specs)
            shapes = [(ci, sh, sv, (w * sh + max_h - 1) // max_h, (h * sv + max_v - 1) // max_v) for ci, sh, sv in specs]
            return p, w, h, shapes
        position += 2 + length
    raise AssertionError("missing lossless SOF")


def serialize(p, w, h, components):
    result = bytearray(b"JLP1" + struct.pack("<4I", p, w, h, len(components)))
    for shape, values in components:
        result += struct.pack("<5I", *shape)
        result += struct.pack("<" + "H" * len(values), *values)
    return bytes(result)


def parse_samples(data):
    assert data[:4] == b"JLP1"
    p, w, h, count = struct.unpack_from("<4I", data, 4)
    position, components = 20, []
    for _ in range(count):
        shape = struct.unpack_from("<5I", data, position)
        position += 20
        length = shape[3] * shape[4]
        values = struct.unpack_from("<" + "H" * length, data, position)
        position += 2 * length
        components.append((shape, values))
    assert position == len(data)
    return p, w, h, components


def reference_samples(image):
    encoded = image.read_bytes()
    p, w, h, shapes = frame(encoded)
    normalized, points = normalize_point_headers(encoded)
    target = image
    if normalized != encoded:
        target = work / "normalized.jpg"
        target.write_bytes(normalized)
    run([args.reference, "-c", "-U", target, work / "pgx"])
    components = []
    for i, shape in enumerate(shapes):
        header = (work / f"pgx_{i}.h").read_text().split()
        assert header == ["PG", "ML", f"+{p}", str(shape[3]), str(shape[4])], header
        raw = (work / f"pgx_{i}.raw").read_bytes()
        length = shape[3] * shape[4]
        assert len(raw) == length * (1 if p <= 8 else 2)
        values = tuple(raw) if p <= 8 else struct.unpack(">" + "H" * length, raw)
        point = points[shape[0]]
        if point:
            # All Annex H predictors preserve addition of a constant. Clearing
            # Pt changes only the initial prediction by this offset; coded
            # differences, contexts and entropy bytes stay unchanged. Undo it
            # and apply Pt explicitly, avoiding the reference's Pt output bug.
            offset = (1 << (p - 1)) - (1 << (p - point - 1))
            values = tuple(v - offset for v in values)
            assert all(0 <= v < 1 << (p - point) for v in values)
            values = tuple(v << point for v in values)
        components.append((shape, values))
    return serialize(p, w, h, components)


def normalize_point_headers(data):
    out, points, position = bytearray(data), {}, 2
    while position < len(data):
        assert data[position] == 255
        marker = data[position + 1]
        if marker == 0xd9:
            break
        length = int.from_bytes(data[position + 2:position + 4], "big")
        end = position + 2 + length
        if marker == 0xda:
            count = data[position + 4]
            point = data[end - 1] & 15
            for i in range(count):
                points[data[position + 5 + 2 * i]] = point
            out[end - 1] &= 0xf0
            while end < len(data):
                if data[end] != 255:
                    end += 1
                elif data[end + 1] == 0 or 0xd0 <= data[end + 1] <= 0xd7:
                    end += 2
                else:
                    break
        position = end
    return bytes(out), points


records, fixtures = [], []


def append(record):
    records.append(record)
    if len(records) % 200 == 0:
        print(f"Passed {len(records)} arithmetic lossless cases", flush=True)


def verify_native(name, command, expected, point, layout):
    source_hash = hashlib.sha256(expected).hexdigest()
    record = completed.get(name)
    if record:
        assert record["source_sha256"] == source_hash
    else:
        (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
        image = work / "native.jpg"
        run(command)
        actual = reference_samples(image)
        run([args.inspect, image, work / "native.samples"])
        assert actual == (work / "native.samples").read_bytes(), (name, "reference/native mismatch")
        p, w, h, components = parse_samples(expected)
        target = serialize(p, w, h, [(shape, tuple((v >> ((point + i) % p if layout >= 2 else point))
                                                   << ((point + i) % p if layout >= 2 else point) for v in values))
                                    for i, (shape, values) in enumerate(components)])
        assert actual == target, (name, "source mismatch")
        record = {"case": name, "direction": "native encode/reference+native decode",
                  "source_sha256": source_hash, "sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                  "samples_sha256": hashlib.sha256(actual).hexdigest(), "bytes": image.stat().st_size,
                  "samples": "exact", "reference_warnings": 0}
        normalized, _ = normalize_point_headers(image.read_bytes())
        record["point_header_normalized"] = normalized != image.read_bytes()
        record["reference_input_sha256"] = hashlib.sha256(normalized).hexdigest()
        with checkpoint.open("a") as file:
            file.write(json.dumps(record) + "\n")
    append(record)


# Reference-encoded samples: all precisions, gray/RGB, common subsampling,
# no/one/three-row restarts. Heights align to vertical sampling because the
# reference has a known partial-last-MCU-row decoding issue (documented).
for p in range(2, 17):
    for label, count, sampling, max_h in [("gray", 1, "1x1", 1), ("rgb444", 3, "1x1,1x1,1x1", 1),
                                         ("rgb420", 3, "1x1,2x2,2x2", 2), ("rgb410", 3, "1x1,4x2,4x2", 4)]:
        w, h = 17, 24
        maximum = (1 << p) - 1
        source = tuple((0 if i % 7 == 0 else maximum if i % 11 == 0 else i * 7319 + i * i * 117) & maximum for i in range(w * h * count))
        payload = bytes(source) if p <= 8 else struct.pack(">" + "H" * len(source), *source)
        pnm = f"P{5 if count == 1 else 6}\n{w} {h}\n{maximum}\n".encode() + payload
        for restart in (0, 1, 3):
            name = f"reference-p{p}-{label}-r{restart}"
            image = work / "reference.jpg"
            (work / "input.pnm").write_bytes(pnm)
            # Regenerate on resume: the following native cases need this source.
            run([args.reference, "-p", "-a", "-c", "-s", sampling, "-z", ((w + max_h - 1) // max_h) * restart,
                 work / "input.pnm", image])
            expected = reference_samples(image)
            if max_h == 1:
                assert all(tuple(source[i::count]) == values for i, (_, values) in enumerate(parse_samples(expected)[3]))
            record = completed.get(name)
            if record:
                assert record["sha256"] == hashlib.sha256(image.read_bytes()).hexdigest()
            else:
                run([args.inspect, image, work / "native.samples"])
                assert expected == (work / "native.samples").read_bytes(), (name, "decoder mismatch")
                record = {"case": name, "direction": "reference encode/native+reference decode",
                          "source_sha256": hashlib.sha256(pnm).hexdigest(), "sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                          "samples_sha256": hashlib.sha256(expected).hexdigest(), "bytes": image.stat().st_size,
                          "samples": "exact", "reference_warnings": 0}
                with checkpoint.open("a") as file:
                    file.write(json.dumps(record) + "\n")
            append(record)
            if restart == 1 and label != "rgb410":
                fixtures.append(record)
                if args.fixtures:
                    shutil.copy2(image, args.fixtures / (name + ".jpg"))
                    (args.fixtures / (name + ".samples")).write_bytes(expected)
            for predictor, point, layout, rows, lower, upper in [(4, 0, 0, 0, 0, 1), (5, p - 1, 2, 1, 3, 7),
                                                               (7, min(3, p - 1), 3, 3, 15, 15)]:
                case = f"rewrite-{name}-v{predictor}-t{point}-l{layout}-r{rows}-c{lower}-{upper}"
                verify_native(case, [args.native, "rewrite", image, work / "native.jpg", predictor, point, rows, layout, 1, 1, lower, upper], expected, point, layout)

# Native encoding at every predictor/precision with both magnitude-context
# regimes, sharing and separate scans, four destinations across the model,
# CMYK conventions, YCCK and point transforms.
colors = [("gray", [1]), ("rgb", list(b"RGB")), ("ycbcr", [1, 2, 3]),
          ("cmyk", list(b"CMYK")), ("cmykinv", list(b"CMYK")), ("ycck", [1, 2, 3, 4])]
for p in range(2, 17):
    for predictor in range(1, 8):
        for ci, (color, ids) in enumerate(colors):
            w, h, count = 13, 24, len(ids)
            maximum = (1 << p) - 1
            source = tuple((0 if i % 7 == 0 else maximum if i % 11 == 0 else i * 6211 + i * i * 37) & maximum for i in range(w * h * count))
            expected = serialize(p, w, h, [((component_id, 1, 1, w, h), source[i::count]) for i, component_id in enumerate(ids)])
            (work / "input.raw").write_bytes(struct.pack("<" + "H" * len(source), *source))
            for conditioning_index, (lower, upper) in enumerate([(0, 1), (0, 0), (3, 7), (15, 15)]):
                point = (0, p - 1, min(3, p - 1))[(predictor + ci + conditioning_index) % 3]
                layout = (p + predictor + ci + conditioning_index) % 4
                rows = (0, 1, 3)[(predictor + conditioning_index) % 3]
                case = f"raw-p{p}-v{predictor}-t{point}-{color}-l{layout}-r{rows}-c{lower}-{upper}"
                verify_native(case, [args.native, "raw", work / "input.raw", work / "native.jpg", w, h, p, color,
                                    predictor, point, rows, layout, 1, 1, lower, upper], expected, point, layout)

report = {"reference": "thorfdbg/libjpeg JPEG XT", "commit": "702114c8130ae818ccd2354cc03e879dac69e579",
          "setup_patch": "tests/oracle/jpegxt-lossless-table-selection.patch",
          "setup_patch_sha256": hashlib.sha256((Path(__file__).parent / "oracle/jpegxt-lossless-table-selection.patch").read_bytes()).hexdigest(),
          "tools": hashes, "cases": len(records), "reference_encoded": 180, "native_rewrites": 540,
          "native_raw": 2520, "reference_warnings": 0, "reference_prediction": "PSV=1, Pt=0",
          "point_header_normalized": sum(r.get("point_header_normalized", False) for r in records),
          "point_header_normalization": "Nonzero Pt is cleared in SOS only; entropy unchanged. Subtract constant prediction offset, then restore Pt. Reference Pt output bug verified against libjpeg-turbo Huffman output.",
          "scope": "SOF11, P=2..16, native PSV=1..7, common subsampling, gray/RGB/YCbCr/CMYK/YCCK, DAC and row restarts",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
if args.fixtures:
    (args.fixtures / "manifest.json").write_text(json.dumps({k: v for k, v in report.items() if k != "results"} | {"fixtures": fixtures}, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
