"""Hierarchical JPEG XT/native frame and sample gate; no production linkage.

Reference EXP filtering differs from Annex J at doubly interpolated positions.
Preserve observed raster differences rather than treating lossless deviations
as IDCT error. Literal Annex J tests establish the native expansion semantics.
Empty DAC segments and redundant EXP00 segments emitted by the reference are
removed as documented syntax repairs; table state and entropy are unchanged.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
from hierarchy_support import structure, samples, check_frame_data, lossless_reconstruction

parser = argparse.ArgumentParser()
for name in ("native", "reference", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--fixtures", type=Path)
parser.add_argument("--resume", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshot = args.out / "tools"
snapshot.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "reference"):
    source = getattr(args, name)
    target = snapshot / (name + source.suffix)
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
    completed = {r["case"]: r for r in rows[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": hashes}) + "\n")
work = args.out / "case"
work.mkdir(exist_ok=True)
if args.fixtures:
    args.fixtures.mkdir(parents=True, exist_ok=True)


def run(command, env=None):
    result = subprocess.run([str(v) for v in command], capture_output=True, env=env)
    assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.stderr.decode(errors="replace"))
    return result.stdout.decode(errors="replace")






def reference_components(header):
    p = header["precision"]
    max_h = max(s[1] for s in header["components"])
    max_v = max(s[2] for s in header["components"])
    output = []
    for i, (component_id, sh, sv) in enumerate(header["components"]):
        w = (header["width"] * sh + max_h - 1) // max_h
        h = (header["height"] * sv + max_v - 1) // max_v
        pgx = (work / f"pgx_{i}.h").read_text().split()
        assert pgx == ["PG", "ML", f"+{p}", str(w), str(h)], pgx
        raw = (work / f"pgx_{i}.raw").read_bytes()
        assert len(raw) == w * h * (1 if p <= 8 else 2)
        values = tuple(raw) if p <= 8 else struct.unpack(">" + "H" * (w * h), raw)
        output.append(((component_id, sh, sv, w, h), values))
    return output






records, fixtures, modes_seen, parity = [], [], set(), {}
modes = [("sequential-huffman", False, False, False), ("progressive-huffman", False, False, True),
         ("sequential-arithmetic", False, True, False), ("progressive-arithmetic", False, True, True),
         ("lossless-huffman", True, False, False), ("lossless-arithmetic", True, True, False)]
for mode, lossless, arithmetic, progressive in modes:
    for p in (range(2, 17) if lossless else (8, 12)):
        for levels in ((2, 3, 4) if lossless else (0, 1, 2, 3, 4)):
            for color, count, sampling in [("gray", 1, "1x1"), ("rgb444", 3, "1x1,1x1,1x1"), ("rgb420", 3, "1x1,2x2,2x2")]:
                for w, h in ((17, 24), (32, 32)):
                    for pattern in ("flat", "ramp", "noise"):
                        maximum = (1 << p) - 1
                        state, values = 0x82345678, []
                        for i in range(w * h * count):
                            state = (state * 1664525 + 1013904223) & 0xffffffff
                            if pattern == "flat":
                                value = maximum * (i % count + 1) // (count + 1)
                            elif pattern == "ramp":
                                value = i * 7319 + i * i * 117
                            else:
                                value = state >> 7
                            values.append(value & maximum)
                        pnm = f"P{5 if count == 1 else 6}\n{w} {h}\n{maximum}\n".encode()
                        pnm += bytes(values) if p <= 8 else struct.pack(">" + "H" * len(values), *values)
                        source_hash = hashlib.sha256(pnm).hexdigest()
                        name = f"{mode}-p{p}-l{levels}-{color}-{w}x{h}-{pattern}"
                        record = completed.get(name)
                        if record and lossless and "markers" in record and not record.get("independent_lossless_composition"):
                            record = None
                        selected = (w, h) == (17, 24) and pattern == "ramp" and (
                            (not lossless and levels in (0, 1, 3) and color == "rgb420") or
                            (lossless and arithmetic and levels == 2 and color == "gray") or
                            (lossless and not arithmetic and p in (8, 16) and levels == 2 and color == "gray") or
                            (lossless and arithmetic and p == 16 and levels == 4 and color == "gray"))
                        if record:
                            assert record["source_sha256"] == source_hash
                        else:
                            (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
                            (work / "source.pnm").write_bytes(pnm)
                            command = [args.reference, "-c", "-s", sampling, "-y", levels,
                                       *(["-p"] if lossless else ["-q", 85]), *( ["-a"] if arithmetic else ["-h"]),
                                       *(["-v"] if progressive else []), work / "source.pnm", work / "reference.jpg"]
                            result = subprocess.run([str(v) for v in command], capture_output=True)
                            error = result.stderr.decode(errors="replace").strip()
                            if error:
                                assert lossless and not arithmetic and "Huffman table is unsuitable" in error, (name, error)
                                record = {"case": name, "source_sha256": source_hash, "reference_encode_error": error}
                            else:
                                assert result.returncode == 0 and b"warning" not in result.stdout.lower()
                                original = (work / "reference.jpg").read_bytes()
                                header, frames, normalized, empty, empty_exp = structure(original)
                                (work / "normalized.jpg").write_bytes(normalized)
                                summary = run([args.native, work / "normalized.jpg", work / "native.samples", "clip", "frames"])
                                decoded_frames = re.findall(r"frame (\d+) SOF([0-9a-f]+) (\d+)x(\d+) (\d+) components EXP([01])([01]) (\d+) scans", summary)
                                assert len(decoded_frames) == len(frames), (name, summary)
                                for i, (_, marker, fw, fh, fn, eh, ev, _) in enumerate(decoded_frames):
                                    assert (int(marker, 16), int(fw), int(fh), int(fn)) == (frames[i]["marker"], frames[i]["width"], frames[i]["height"], len(frames[i]["components"]))
                                    assert (int(eh) << 4 | int(ev)) == frames[i]["expansion"]
                                observation = work / "reference-frames.txt"
                                observation.write_text("")
                                run([args.reference, "-c", "-U", work / "reference.jpg", work / "pgx"],
                                    os.environ | {"DEADLIB_JPEG_FRAME_DUMP": str(observation.resolve())})
                                cp = reference_components(header)
                                np, nw, nh, nc = samples((work / "native.samples").read_bytes())
                                assert (np, nw, nh) == (p, w, h)
                                errors = []
                                for (shape, actual), (expected_shape, expected) in zip(nc, cp, strict=True):
                                    assert shape == expected_shape
                                    errors.extend(abs(a - b) for a, b in zip(actual, expected, strict=True))
                                absolute = max(errors, default=0)
                                modulo = max((min(e, 65536 - e) for e in errors), default=0)
                                native_flat_mismatches = reference_flat_mismatches = 0
                                if lossless and pattern == "flat":
                                    # This measures the reference encoder, whose
                                    # deep hierarchies need not preserve even a
                                    # constant source. Decoder correctness is
                                    # established by exact raw frame data and
                                    # independent Annex J composition below.
                                    for ci, ((_, actual), (_, expected)) in enumerate(zip(nc, cp, strict=True)):
                                        constant = maximum * (ci + 1) // (count + 1)
                                        native_flat_mismatches += sum(v != constant for v in actual)
                                        reference_flat_mismatches += sum(v != constant for v in expected)
                                dumps = []
                                for i, frame in enumerate(frames):
                                    extension = "samples" if frame["marker"] & 3 == 3 else "coef"
                                    dumps.append((work / f"native.frame-{i}.{extension}").read_bytes())
                                blocks, raw_samples = check_frame_data(frames, dumps, observation.read_text())
                                if lossless:
                                    independent = lossless_reconstruction(header, frames, dumps)
                                    run([args.native, work / "normalized.jpg", work / "native-raw.samples", "raw"])
                                    raw_components = samples((work / "native-raw.samples").read_bytes())[3]
                                    assert [c[1] for c in raw_components] == independent, name
                                    clipped = independent if p == 16 else [tuple(min(max(v if v < 32768 else v - 65536, 0), maximum) for v in plane) for plane in independent]
                                    assert [c[1] for c in nc] == clipped, name
                                data_hash = hashlib.sha256(b"".join(dumps)).hexdigest()
                                key = (lossless, p, levels, color, w, h, pattern)
                                same = parity.setdefault(key, data_hash) == data_hash
                                record = {"case": name, "source_sha256": source_hash,
                                          "original_sha256": hashlib.sha256(original).hexdigest(), "sha256": hashlib.sha256(normalized).hexdigest(),
                                          "empty_dac_removed": empty, "empty_exp_removed": empty_exp,
                                          "bytes": len(normalized), "frames": len(frames),
                                          "markers": [f["marker"] for f in frames], "scans": sum(int(row[7]) for row in decoded_frames),
                                          "frame_data_sha256": data_hash, "frame_data_matches_first_entropy_mode": same,
                                          "exact_reference_dct_blocks": blocks, "exact_reference_lossless_samples": raw_samples,
                                          "independent_lossless_composition": lossless,
                                          "native_flat_source_mismatches": native_flat_mismatches,
                                          "reference_flat_source_mismatches": reference_flat_mismatches,
                                          "max_absolute_sample_difference": absolute, "max_modulo_sample_difference": modulo,
                                          "native_samples_sha256": hashlib.sha256((work / "native.samples").read_bytes()).hexdigest(),
                                          "reference_warnings": 0}
                                if selected and args.fixtures:
                                    (args.fixtures / (name + ".jpg")).write_bytes(normalized)
                                    (args.fixtures / (name + ".samples")).write_bytes((work / "native.samples").read_bytes())
                                    for i, dump in enumerate(dumps):
                                        suffix = "samples" if frames[i]["marker"] & 3 == 3 else "coef"
                                        (args.fixtures / f"{name}.frame-{i}.{suffix}").write_bytes(dump)
                            with checkpoint.open("a") as file:
                                file.write(json.dumps(record) + "\n")
                        if "markers" in record:
                            modes_seen.update(record["markers"])
                            if selected:
                                fixtures.append(record)
                        records.append(record)
                        if len(records) % 100 == 0:
                            print(f"Checked {len(records)} hierarchical cases", flush=True)

successful = [r for r in records if "markers" in r]
parity = {}
for record in successful:
    key = (record["case"].startswith("lossless"), record["case"].split("-p", 1)[1])
    record["frame_data_matches_first_entropy_mode"] = parity.setdefault(key, record["frame_data_sha256"]) == record["frame_data_sha256"]
patches = ["jpegxt-lossless-table-selection.patch", "jpegxt-hierarchy-precision.patch", "jpegxt-frame-observation.patch"]
patch_hashes = {name: hashlib.sha256((Path(__file__).parent / "oracle" / name).read_bytes()).hexdigest() for name in patches}
report = {"reference": "thorfdbg/libjpeg JPEG XT", "commit": "702114c8130ae818ccd2354cc03e879dac69e579",
          "reference_patch_sha256": patch_hashes, "tools": hashes,
          "attempted": len(records), "decoded": len(successful), "reference_encode_failures": len(records) - len(successful),
          "markers": sorted(modes_seen), "empty_dac_segments_removed": sum(r.get("empty_dac_removed", 0) for r in records),
          "empty_exp_segments_removed": sum(r.get("empty_exp_removed", 0) for r in records),
          "reference_warnings": 0, "frame_data_mode_differences": sum(not r.get("frame_data_matches_first_entropy_mode", True) for r in records),
          "exact_reference_dct_blocks": sum(r.get("exact_reference_dct_blocks", 0) for r in records),
          "exact_reference_lossless_samples": sum(r.get("exact_reference_lossless_samples", 0) for r in records),
          "max_dct_absolute_difference": max((r["max_absolute_sample_difference"] for r in successful if not r["case"].startswith("lossless")), default=0),
          "max_lossless_modulo_difference": max((r["max_modulo_sample_difference"] for r in successful if r["case"].startswith("lossless")), default=0),
          "independent_lossless_composition_max_difference": 0,
          "independent_lossless_compositions": sum(r.get("independent_lossless_composition", False) for r in successful),
          "reference_flat_source_mismatches": sum(r.get("reference_flat_source_mismatches", 0) for r in successful),
          "native_flat_source_mismatches": sum(r.get("native_flat_source_mismatches", 0) for r in successful),
          "scope": "hierarchical decoding; exact per-frame C coefficients and native sample/residual rows; final native raster regression snapshots use literal Annex J expansion rules; C final raster differences are observations, not conformance tolerances",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
if args.fixtures:
    (args.fixtures / "manifest.json").write_text(json.dumps({k: v for k, v in report.items() if k != "results"} | {"fixtures": fixtures}, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
