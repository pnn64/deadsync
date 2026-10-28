"""C arithmetic encoding -> native decoding -> native Huffman encoding -> C decoding."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
for name in ("oracle", "inspect", "rewrite", "transcode", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--pixel-corpus", type=Path)
parser.add_argument("--pin", type=Path, help="Retain a small standalone C-generated corpus")
parser.add_argument("--progressive", action="store_true")
parser.add_argument("--resume", action="store_true", help="Resume completed cases with identical tools and inputs")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parent / "fixtures"
tool_hashes = {name: hashlib.sha256(getattr(args, name).read_bytes()).hexdigest()
               for name in ("oracle", "inspect", "rewrite", "transcode")}
checkpoint = args.out / "checkpoint.jsonl"
completed = {}
if args.resume:
    entries = [json.loads(line) for line in checkpoint.read_text().splitlines()]
    assert entries[0] == {"tools": tool_hashes, "progressive": args.progressive}, "checkpoint tools differ"
    completed = {entry["case"]: entry for entry in entries[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": tool_hashes, "progressive": args.progressive}) + "\n")


def run(command):
    result = subprocess.run([str(x) for x in command], check=True, capture_output=True)
    assert not result.stderr, result.stderr.decode()


sources = []
for mode in ("sequential", "progressive"):
    for source in sorted((root / mode).glob("*.jpg")):
        sources.append((mode, source, source.with_suffix(".coef")))
if args.pixel_corpus:
    for source in sorted(args.pixel_corpus.glob("*.jpg")):
        expected = source.with_name(source.stem + "-expected.coef")
        if expected.exists():
            sources.append(("pixels", source, expected))
seen = set()
unique = []
for mode, source, expected in sources:
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest not in seen:
        unique.append((mode, source, expected))
        seen.add(digest)

# restart, DC lower, DC upper, AC split, table layout, scan layout.
# Defaults, conditioning extrema, shared contexts and all four destinations.
settings = [(r, 0, 1, 5, 1, 0) for r in (0, 1, 7)]
settings += [(1, 0, 0, 1, 0, 0), (7, 3, 7, 63, 2, 2), (0, 15, 15, 32, 3, 0)]
if args.progressive:
    settings = [(*s[:5], 3 if s[1:3] in [(0, 0), (3, 7)] else 1) for s in settings]
pin_names = ["8-gray", "12-gray", "8-444", "12-444", "8-420", "12-420",
             "8-411", "12-411", "8-410", "12-410", "8-rgb", "12-rgb",
             "8-cmyk", "8-wide-quant", "12-wide-quant"]
records, pinned = [], []
for mode, source, expected_path in unique:
    expected = expected_path.read_bytes()
    for setting in settings:
        restart, lower, upper, split, layout, scan_layout = setting
        name = f"{mode}-{source.stem}-r{restart}-c{lower}-{upper}-{split}-t{layout}-s{scan_layout}"
        path = args.out / (name + ".jpg")
        command = [args.transcode, source, path, *setting]
        prefix = args.out / (name + "-native")
        reference = args.out / (name + "-c")
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        expected_hash = hashlib.sha256(expected).hexdigest()
        record = completed.get(name)
        if record:
            assert record["source_sha256"] == source_hash and record["coefficient_sha256"] == expected_hash
            assert record["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
            for output in [prefix, reference, *(args.out / (name + f"-h{p}-c") for p in (0, 1))]:
                assert output.with_suffix(".coef").read_bytes() == expected, (name, "checkpoint output differs")
        else:
            run(command)
            run([args.inspect, path, prefix])
            assert prefix.with_suffix(".coef").read_bytes() == expected, (name, "native arithmetic coefficients")
            run([args.oracle, path, reference])
            assert reference.with_suffix(".coef").read_bytes() == expected, (name, "reference arithmetic coefficients")
            for progressive in (0, 1):
                rewritten = args.out / (name + f"-h{progressive}.jpg")
                run([args.rewrite, path, rewritten, progressive, 1, restart])
                output = args.out / (name + f"-h{progressive}-c")
                run([args.oracle, rewritten, output])
                assert output.with_suffix(".coef").read_bytes() == expected, (name, "native arithmetic-to-Huffman rewrite")
            record = {"case": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                      "source_sha256": source_hash, "coefficient_sha256": expected_hash,
                      "bytes": path.stat().st_size, "decoded_coefficients": "exact", "native_huffman_rewrites": 2}
            with checkpoint.open("a") as log:
                log.write(json.dumps(record) + "\n")
        records.append(record)
        if len(records) % 100 == 0:
            print(f"Passed {len(records)} arithmetic files and {2 * len(records)} rewrites", flush=True)
        if (args.pin and mode == "sequential" and source.stem in pin_names
                and setting == settings[pin_names.index(source.stem) % len(settings)]):
            args.pin.mkdir(parents=True, exist_ok=True)
            target = args.pin / (source.stem + ".jpg")
            shutil.copy2(path, target)
            shutil.copy2(reference.with_suffix(".coef"), target.with_suffix(".coef"))
            pinned.append({**record, "file": target.name, "command": [str(x) for x in command],
                           "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                           "coefficient_sha256": hashlib.sha256(expected).hexdigest()})
report = {"reference": "libjpeg-turbo 3.2.1 arithmetic encoder/decoder", "sources": len(unique),
          "progressive": args.progressive, "tools": tool_hashes,
          "cases": len(records), "rewrites": 2 * len(records), "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
if args.pin:
    assert len(pinned) == len(pin_names)
    (args.pin / "manifest.json").write_text(json.dumps({"reference": report["reference"], "cases": pinned}, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
