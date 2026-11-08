"""Exact C/native frame-data gate for direct hierarchical frame encoding."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from hierarchy_support import structure, check_frame_data, samples, lossless_reconstruction

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "reference", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--sources", type=Path, default=Path(__file__).parent / "fixtures" / "hierarchical")
parser.add_argument("--resume", action="store_true")
parser.add_argument("--expected-hashes", type=Path, help="Require the exact bytes of a previously C-verified report")
args = parser.parse_args()
baseline = None if args.expected_hashes is None else {r["case"]: r for r in json.loads(args.expected_hashes.read_text())["results"]}
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
    rows = [json.loads(row) for row in checkpoint.read_text().splitlines()]
    assert rows[0] == {"tools": hashes}
    completed = {row["case"]: row for row in rows[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": hashes}) + "\n")
work = args.out / "case"
work.mkdir(exist_ok=True)


def run(command, env=None):
    result = subprocess.run([str(v) for v in command], capture_output=True, env=env)
    assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.stderr.decode(errors="replace"))


settings = [("source", "optimized", 0, 0), ("preserve", "optimized", 0, 0)]
for mode in ("sequential-huffman", "progressive-huffman", "sequential-arithmetic", "progressive-arithmetic"):
    for policy in ("optimized", "fixed"):
        for restart in (0, 1, 3):
            for initial in ((0, 3, 13) if mode.startswith("progressive") else (0,)):
                settings.append((mode, policy, restart, initial))
records = []
source_count = 0
for path in sorted(args.sources.glob("*.jpg")):
    source = path.read_bytes()
    _, source_frames, normalized, empty_dac, empty_exp = structure(source)
    assert source == normalized and empty_dac == empty_exp == 0
    expected = [(path.parent / f"{path.stem}.frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}").read_bytes() for i, f in enumerate(source_frames)]
    source_hash = hashlib.sha256(source).hexdigest()
    data_hash = hashlib.sha256(b"".join(expected)).hexdigest()
    source_count += 1
    for mode, policy, restart, initial in settings:
        name = f"{path.stem}-{mode}-{policy}-r{restart}-a{initial}"
        record = completed.get(name)
        if record:
            assert record["source_sha256"] == source_hash and record["frame_data_sha256"] == data_hash
        else:
            (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
            image = work / "image.jpg"
            if mode == "source":
                image.write_bytes(source)
            else:
                run([args.native, path, image, mode, policy, restart, initial])
            encoded = image.read_bytes()
            if baseline is not None:
                record = baseline[name].copy()
                assert record["source_sha256"] == source_hash and record["frame_data_sha256"] == data_hash
                assert record["sha256"] == hashlib.sha256(encoded).hexdigest(), name
                assert record["bytes"] == len(encoded), name
            else:
                header, frames, normalized, empty_dac, empty_exp = structure(encoded)
                assert encoded == normalized and empty_dac == empty_exp == 0
                assert len(frames) == len(source_frames)
                observation = work / "reference-frames.txt"
                observation.write_text("")
                run([args.reference, "-c", "-U", image, work / "pgx"], os.environ | {"DEADLIB_JPEG_FRAME_DUMP": str(observation.resolve())})
                blocks, count = check_frame_data(frames, expected, observation.read_text())
                run([args.inspect, image, work / "native.samples", "raw", "frames"])
                dumps = [(work / f"native.frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}").read_bytes() for i, f in enumerate(frames)]
                assert dumps == expected, name
                pure_lossless = all(f["marker"] & 3 == 3 for f in frames)
                if pure_lossless:
                    native = [c[1] for c in samples((work / "native.samples").read_bytes())[3]]
                    assert native == lossless_reconstruction(header, frames, expected), name
                    target = path.with_suffix(".expected.samples")
                    if target.exists():
                        assert (work / "native.samples").read_bytes() == target.read_bytes(), name
                record = {"case": name, "source_sha256": source_hash, "frame_data_sha256": data_hash,
                          "sha256": hashlib.sha256(encoded).hexdigest(), "bytes": len(encoded),
                          "markers": [f["marker"] for f in frames], "exact_reference_dct_blocks": blocks,
                          "exact_reference_lossless_samples": count, "independent_lossless_composition": pure_lossless,
                          "reference_warnings": 0}
            with checkpoint.open("a") as file:
                file.write(json.dumps(record) + "\n")
        records.append(record)
        if len(records) % 100 == 0:
            print(f"Checked {len(records)} hierarchy encodes", flush=True)
patches = ["jpegxt-lossless-table-selection.patch", "jpegxt-hierarchy-precision.patch", "jpegxt-frame-observation.patch"]
patch_hashes = {name: hashlib.sha256((Path(__file__).parent / "oracle" / name).read_bytes()).hexdigest() for name in patches}
report = {"reference": "thorfdbg/libjpeg JPEG XT", "commit": "702114c8130ae818ccd2354cc03e879dac69e579",
          "tools": hashes, "sources": source_count, "cases": len(records), "reference_warnings": 0,
          "reference_patch_sha256": patch_hashes,
          "validation": "byte-identical to previously C-verified files" if baseline is not None else "C/native frame-data comparisons",
          "source_checks": source_count, "rewrite_cases": len(records) - source_count,
          "unique_output_sha256": len({r["sha256"] for r in records}),
          "exact_reference_dct_blocks": sum(r["exact_reference_dct_blocks"] for r in records),
          "exact_reference_lossless_samples": sum(r["exact_reference_lossless_samples"] for r in records),
          "independent_lossless_compositions": sum(r["independent_lossless_composition"] for r in records),
          "scope": "direct native hierarchical frame encoding; exact C/native frame data, both entropy coders, fixed/optimized Huffman, sequential/progressive DCT, Al through 13, row/MCU restarts; final C raster differences are not treated as IDCT tolerances",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
