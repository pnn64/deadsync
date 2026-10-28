"""Exact native lossless sample gate against the public libjpeg-turbo API.

No reference implementation is linked into the Rust package. Executables are
snapshotted; one case directory bounds disk growth. Pt intentionally discards
source low bits, so each decoded sample must equal (source >> Pt) << Pt.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "oracle", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--fixtures", type=Path)
parser.add_argument("--resume", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "oracle"):
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
    assert result.returncode == 0 and not result.stderr, (command, result.stderr.decode())


def verify(data, source, width, height, precision, count, point, layout):
    assert data[:4] == b"JLP1"
    assert struct.unpack_from("<4I", data, 4) == (precision, width, height, count)
    position = 20
    for ci in range(count):
        shape = struct.unpack_from("<5I", data, position)
        assert shape[1:] == (1, 1, width, height)
        position += 20
        actual = struct.unpack_from("<" + "H" * (width * height), data, position)
        pt = (point + ci) % precision if layout == 2 else point
        expected = tuple((v >> pt) << pt for v in source[ci::count])
        assert actual == expected, (precision, ci, pt, "source mismatch")
        position += width * height * 2
    assert position == len(data)


records, fixtures = [], []
settings = []
for p in range(2, 17):
    for predictor in range(1, 8):
        for point in (0, p - 1):
            for layout in range(3):
                for restart in (0, 2):
                    for count in (1, 3, 4):
                        settings.append((p, predictor, point, layout, restart, count, 17, 11, 0))
# Every transform at the API boundaries, narrow/tall grids, restart wraparound,
# and full-range pathological predictors. The reference decoder cannot handle
# its encoder's >4-component output; that case is pinned separately.
for p in (2, 8, 12, 15, 16):
    for point in range(p):
        for pattern in range(1, 5):
            count = (1, 2, 4, 4)[pattern - 1]
            layout = 1 if pattern == 4 else 0
            for predictor in (4, 5, 6, 7):
                width, height = ((1, 19) if pattern == 1 else (23, 1) if pattern == 2 else (9, 19))
                settings.append((p, predictor, point, layout, 1, count, width, height, pattern))
for p, predictor, point, layout, restart, count, width, height, pattern in settings:
    name = f"p{p}-v{predictor}-t{point}-l{layout}-r{restart}-c{count}-{width}x{height}-w{pattern}"
    maximum = (1 << p) - 1
    state, source = 0x9e3779b9, []
    for i in range(width * height * count):
        state = (state * 1664525 + 1013904223) & 0xffffffff
        x, y = (i // count) % width, i // (count * width)
        if pattern == 0:
            value = state >> 7
            if i % 7 == 0:
                value = maximum if i % 2 else 0
        elif pattern == 1:
            value = maximum if y % 2 else 0
        elif pattern == 2:
            value = maximum if x % 2 else 0
        elif pattern == 3:
            value = maximum if (x + y) % 2 else 0
        else:
            value = (x * 7319 + y * 2171 + i * i * 117)
        source.append(value & maximum)
    raw = struct.pack("<" + "H" * len(source), *source)
    source_hash = hashlib.sha256(raw).hexdigest()
    # Retain three independently encoded layouts per precision. Their predictor
    # cycles through all seven selections, with lossless and transformed cases.
    fixture = (pattern == 0 and count == 3 and restart == 2
               and predictor == (p - 2) % 7 + 1 and point == (0 if layout == 0 else p - 1))
    record = completed.get(name)
    if record:
        assert record["source_sha256"] == source_hash
        if fixture and args.fixtures:
            assert (args.fixtures / (name + ".jpg")).exists()
    else:
        (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
        (work / "input.raw").write_bytes(raw)
        image = work / "image.jpg"
        run([args.oracle, "encode", work / "input.raw", image, width, height,
             p, count, predictor, point, restart, layout])
        run([args.oracle, "decode", image, work / "c.samples"])
        run([args.native, image, work / "native.samples"])
        actual = (work / "c.samples").read_bytes()
        assert actual == (work / "native.samples").read_bytes(), (name, "C/native mismatch")
        verify(actual, source, width, height, p, count, point, layout)
        record = {"case": name, "source_sha256": source_hash,
                  "sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                  "samples_sha256": hashlib.sha256(actual).hexdigest(),
                  "bytes": image.stat().st_size, "samples": "exact", "reference_warnings": 0}
        if fixture and args.fixtures:
            shutil.copy2(image, args.fixtures / (name + ".jpg"))
            (args.fixtures / (name + ".samples")).write_bytes(actual)
        with checkpoint.open("a") as file:
            file.write(json.dumps(record) + "\n")
    if fixture:
        fixtures.append(record)
    records.append(record)
    if len(records) % 200 == 0:
        print(f"Passed {len(records)} exact lossless cases", flush=True)
report = {"reference": "libjpeg-turbo 3.2.1 lossless sample API", "tools": hashes,
          "cases": len(records), "sample_precision": list(range(2, 17)), "predictors": list(range(1, 8)),
          "reference_warnings": 0, "scope": "SOF3; 1x1 component factors; interleaved and separate scans",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
if args.fixtures:
    (args.fixtures / "manifest.json").write_text(json.dumps({"reference": report["reference"], "fixtures": fixtures}, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
