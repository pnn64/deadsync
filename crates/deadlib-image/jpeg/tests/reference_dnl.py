"""DNL encoding/decoding gate. C failures are explicit, never passing claims.

Native output and input headers remain unchanged for all native checks.
Original-header C frame/sample checks are attempted first. If C rejects or
misinterprets DNL/DHP, a separately recorded C-input copy restores final header
heights and removes DNL; entropy is unchanged. This proves entropy parity only
for that comparison, not independent C parsing of the original DNL syntax.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure, samples, check_frame_data

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "lossless", "hierarchy", "reference", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--resume", action="store_true")
parser.add_argument("--sources", type=Path, help="Optional additional source JPEG directory")
parser.add_argument("--fixtures", type=Path)
parser.add_argument("--only-additional", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "inspect", "lossless", "hierarchy", "reference"):
    source = getattr(args, name)
    target = snapshots / (name + source.suffix)
    if not args.resume:
        shutil.copy2(source, target)
        assert source.read_bytes() == target.read_bytes()
    hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    setattr(args, name, target.resolve())
checkpoint = args.out / "checkpoint.jsonl"
if args.resume:
    rows = [json.loads(row) for row in checkpoint.read_text().splitlines()]
    assert rows[0] == {"tools": hashes}
    completed = {row["case"]: row for row in rows[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": hashes}) + "\n")
    completed = {}
work = args.out / "case"
work.mkdir(exist_ok=True)


def run(command, env=None, strict=True):
    result = subprocess.run([str(v) for v in command], capture_output=True, env=env)
    if strict:
        assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.stderr.decode(errors="replace"))
    return result


def marker_ranges(data):
    position = 2
    while position < len(data):
        start = position
        assert data[position] == 255
        while data[position] == 255:
            position += 1
        marker = data[position]
        position += 1
        if marker == 0xd9:
            yield marker, start, position
            break
        size = int.from_bytes(data[position:position + 2], "big")
        assert size >= 2
        end = position + size
        if marker == 0xda:
            while end < len(data):
                if data[end] != 255:
                    end += 1
                else:
                    j = end + 1
                    while data[j] == 255:
                        j += 1
                    if data[j] == 0 or 0xd0 <= data[j] <= 0xd7:
                        end = j + 1
                    else:
                        break
        yield marker, start, end
        position = end


def known_headers(data, header, frames, exp_zero=False):
    output, fi, previous_expansion = bytearray(data[:2]), 0, 0
    for marker, start, end in marker_ranges(data):
        segment = bytearray(data[start:end])
        if marker == 0xdc:
            continue
        if marker == 0xde:
            segment[5:7] = header["height"].to_bytes(2, "big")
        if marker in (0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf):
            frame = frames[fi]
            fi += 1
            segment[5:7] = frame["height"].to_bytes(2, "big")
            if exp_zero and previous_expansion and not frame["expansion"]:
                output.extend(b"\xff\xdf\x00\x03\x00")
            previous_expansion = frame["expansion"]
        output.extend(segment)
    assert fi == len(frames)
    return bytes(output)


def c_check(image, family, frames, expected):
    observation = work / "observations.txt"
    observation.write_text("")
    result = run([args.reference, "-c", "-U", image, work / "pgx"],
                 os.environ | {"DEADLIB_JPEG_FRAME_DUMP": str(observation.resolve())}, False)
    if result.returncode or result.stderr or b"warning" in result.stdout.lower():
        return None, result.stderr.decode(errors="replace") or result.stdout.decode(errors="replace")
    try:
        if family == "lossless":
            p, _, _, components = samples(expected[0])
            count = 0
            for i, (shape, values) in enumerate(components):
                header = (work / f"pgx_{i}.h").read_text().split()
                assert header == ["PG", "ML", f"+{p}", str(shape[3]), str(shape[4])], (header, shape)
                raw = (work / f"pgx_{i}.raw").read_bytes()
                actual = tuple(raw) if p <= 8 else struct.unpack(">" + "H" * (len(raw) // 2), raw)
                assert actual == values, "C visible samples differ"
                count += len(values)
            return (0, count), None
        return check_frame_data(frames, expected, observation.read_text()), None
    except (AssertionError, OSError, struct.error) as failure:
        return None, str(failure)[:1000]


def native_check(image, family, frames, expected, target):
    if family == "dct":
        run([args.inspect, image, work / "native"])
        assert (work / "native.coef").read_bytes() == expected[0]
        expected_planes = b"".join(struct.pack("<" + "H" * len(v), *v) for _, v in samples(target)[3])
        assert (work / "native.planes").read_bytes() == expected_planes
    elif family == "lossless":
        run([args.lossless, image, work / "native.samples"])
        assert (work / "native.samples").read_bytes() == expected[0] == target
    else:
        run([args.hierarchy, image, work / "native.samples", "raw", "frames"])
        assert (work / "native.samples").read_bytes() == target
        actual = [(work / f"native.frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}").read_bytes() for i, f in enumerate(frames)]
        assert actual == expected


def fixture_keys(frames, direct):
    return {(f["marker"], f["precision"], direct) for f in frames
            if (f["precision"] in (2,16) if f["marker"] & 3 == 3 else f["precision"] in (8,12))}


root = Path(__file__).parent / "fixtures"
sources = []
for directory in ("sequential", "progressive", "arithmetic", "arithmetic-progressive", "arithmetic-model", "lossless", "lossless-arithmetic", "hierarchical", "hierarchical-native", "pyramid-native"):
    for path in sorted((root / directory).glob("*.jpg")):
        family = "hierarchy" if directory.startswith("hierarchical") or directory == "pyramid-native" else "lossless" if directory.startswith("lossless") else "dct"
        sources.append((path, family, directory + "-" + path.stem))
if args.sources:
    for path in sorted(args.sources.glob("*.jpg")):
        header, frames, _, _, _ = structure(path.read_bytes(), False)
        family = "hierarchy" if header["marker"] == 0xde else "lossless" if frames[0]["marker"] & 3 == 3 else "dct"
        sources.append((path, family, "additional-" + path.stem))
if args.only_additional:
    assert args.sources
    sources = [row for row in sources if row[2].startswith("additional-")]

records, retained, coverage = [], [], set()
for source, family, source_name in sources:
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    settings = [("0", 0, False, False), ("0", 1, False, True), ("0", 3, False, False), ("same", 1, False, False), ("1", 3, False, True)]
    if family == "hierarchy":
        settings += [("0", 0, True, False), ("0", 1, True, True), ("0", 3, True, False)]
    additional = source_name.startswith("additional-")
    if additional:
        settings = [("0", 0, False, False)]
    for initial, restart, omit, fixed in settings:
        name = f"{source_name}-h{initial}-r{restart}-d{int(omit)}-f{int(fixed)}"
        record = completed.get(name)
        cached_fixture = None
        if record and args.fixtures and additional:
            _, source_frames, _, _, _ = structure(source.read_bytes(), False)
            keys = fixture_keys(source_frames, record["source_original_header_reference_exact"])
            if keys - coverage:
                paths = [args.fixtures / f"{source.stem}.frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}" for i,f in enumerate(source_frames)]
                target_path = args.fixtures / (source.stem + ".samples")
                image_path = args.fixtures / (source.stem + ".jpg")
                if not all(p.exists() for p in paths + [target_path,image_path]) or "target_sha256" not in record:
                    record = None
                else:
                    assert hashlib.sha256(image_path.read_bytes()).hexdigest() == source_hash
                    assert hashlib.sha256(b"".join(p.read_bytes() for p in paths)).hexdigest() == record["frame_data_sha256"]
                    assert hashlib.sha256(target_path.read_bytes()).hexdigest() == record["target_sha256"]
                    cached_fixture = record | {"fixture": source.stem, "source_markers": [f["marker"] for f in source_frames]}
        if record:
            assert record["source_sha256"] == source_hash
        else:
            (args.out / "current-case.json").write_text(json.dumps({"case": name}) + "\n")
            image = work / "image.jpg"
            run([args.native, family, source, image, initial, restart, int(omit), int(fixed)])
            encoded = image.read_bytes()
            header, frames, unchanged, dac, exp = structure(encoded, False)
            assert encoded == unchanged and dac == exp == 0
            assert all(f["dnl_segments"] == 1 and (f["declared_height"] == 0 if initial == "0" else f["declared_height"] == f["height"] if initial == "same" else f["declared_height"] == 1) for f in frames)
            assert (header["declared_height"] == 0) == omit if family == "hierarchy" else True
            expected = [image.with_suffix(f".frame-{i}.{'samples' if f['marker'] & 3 == 3 else 'coef'}").read_bytes() for i, f in enumerate(frames)]
            target = image.with_suffix(".expected.samples").read_bytes()
            source_direct, source_diagnostic = None, None
            if additional:
                source_header, source_frames, valid_source, _, _ = structure(source.read_bytes(), False)
                assert valid_source == source.read_bytes()
                assert [(f["precision"],f["width"],f["height"],f["components"]) for f in source_frames] == [(f["precision"],f["width"],f["height"],f["components"]) for f in frames]
                source_exact, source_diagnostic = c_check(source, family, source_frames, expected)
                source_direct = source_exact is not None
                if not source_direct:
                    source_copy = work / "source-known.jpg"
                    source_copy.write_bytes(known_headers(source.read_bytes(), source_header, source_frames, True))
                    source_exact, source_error = c_check(source_copy, family, source_frames, expected)
                    assert source_exact is not None, (name, "independent source entropy", source_diagnostic, source_error)
            native_check(image, family, frames, expected, target)
            # DNL adds six bytes per frame and changes SOF/DHP heights only.
            plain = work / "plain.jpg"
            run([args.native, family, source, plain, "none", restart, 0, int(fixed)])
            canonical = known_headers(encoded, header, frames)
            assert canonical == plain.read_bytes(), name
            exact, diagnostic = c_check(image, family, frames, expected)
            direct = exact is not None
            reference_input = encoded
            if not direct:
                reference_input = known_headers(encoded, header, frames, True)
                normalized = work / "normalized.jpg"
                normalized.write_bytes(reference_input)
                exact, normalized_diagnostic = c_check(normalized, family, frames, expected)
                assert exact is not None, (name, diagnostic, normalized_diagnostic)
            record = {"case": name, "family": family, "source_sha256": source_hash,
                      "sha256": hashlib.sha256(encoded).hexdigest(), "bytes": len(encoded),
                      "frame_data_sha256": hashlib.sha256(b"".join(expected)).hexdigest(),
                      "target_sha256": hashlib.sha256(target).hexdigest(),
                      "reference_input_sha256": hashlib.sha256(reference_input).hexdigest(),
                      "DNL_segments": len(frames), "unknown_DHP": omit,
                      "initial_height": initial, "markers": [f["marker"] for f in frames],
                      "original_header_reference_exact": direct,
                      "original_header_reference_diagnostic": diagnostic,
                      "known_header_reference_exact": not direct,
                      "exact_dct_blocks": exact[0], "exact_lossless_samples": exact[1],
                      "native_frame_data_and_target": "exact", "canonical_encoder_bytes": "identical after height/DNL normalization"}
            if additional:
                record |= {"independent_DNL_source": True, "source_original_header_reference_exact": source_direct,
                           "source_original_header_reference_diagnostic": source_diagnostic}
            if args.fixtures and additional:
                keys = fixture_keys(source_frames, source_direct)
                if keys - coverage:
                    args.fixtures.mkdir(parents=True, exist_ok=True)
                    fixture_name = source.stem
                    (args.fixtures / (fixture_name + ".jpg")).write_bytes(source.read_bytes())
                    (args.fixtures / (fixture_name + ".samples")).write_bytes(target)
                    for i, (frame, dump) in enumerate(zip(source_frames, expected, strict=True)):
                        (args.fixtures / f"{fixture_name}.frame-{i}.{'samples' if frame['marker'] & 3 == 3 else 'coef'}").write_bytes(dump)
                    retained.append(record | {"fixture": fixture_name, "source_markers": [f["marker"] for f in source_frames]})
                    coverage |= keys
            with checkpoint.open("a") as file:
                file.write(json.dumps(record) + "\n")
        if cached_fixture:
            retained.append(cached_fixture)
            coverage |= keys
        records.append(record)
        if len(records) % 100 == 0:
            print(f"Checked {len(records)} DNL cases", flush=True)
report = {"reference": "thorfdbg/libjpeg JPEG XT", "commit": "702114c8130ae818ccd2354cc03e879dac69e579",
          "tools": hashes, "sources": len(sources), "cases": len(records),
          "original_header_reference_exact": sum(r["original_header_reference_exact"] for r in records),
          "known_header_reference_exact": sum(r["known_header_reference_exact"] for r in records),
          "DNL_segments": sum(r["DNL_segments"] for r in records), "unknown_DHP": sum(r["unknown_DHP"] for r in records),
          "exact_dct_blocks": sum(r["exact_dct_blocks"] for r in records), "exact_lossless_samples": sum(r["exact_lossless_samples"] for r in records),
          "reference_patch_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (Path(__file__).parent / "oracle").glob("jpegxt-*.patch")},
          "scope": "Native original-header DNL encoding and decoding; provisional zero/known/redefined SOF heights, unknown DHP, fixed/optimized tables and restarts. Exact native targets and pre-entropy frame data. C original-header failures/mismatches remain explicit; separate known-header C-input copies preserve entropy and prove entropy parity only. No complete conformance or performance claim.",
          "results": records}
report["by_initial_height"] = {initial: {"cases": sum(r["initial_height"] == initial for r in records), "original_header_reference_exact": sum(r["initial_height"] == initial and r["original_header_reference_exact"] for r in records)} for initial in ("0","same","1")}
report["independent_DNL_sources"] = sum(r.get("independent_DNL_source",False) for r in records)
report["independent_source_original_header_reference_exact"] = sum(r.get("source_original_header_reference_exact",False) for r in records)
report["by_process"] = {str(marker): {"cases": sum(marker in r["markers"] for r in records), "original_header_reference_exact": sum(marker in r["markers"] and r["original_header_reference_exact"] for r in records)} for marker in sorted({m for r in records for m in r["markers"]})}
if args.fixtures and retained:
    manifest = {"provenance": "Independently C-authored DNL JPEG inputs (-n); only empty DAC/EXP00 syntax repaired. Original DNL and unknown heights remain. Native frame expectations independently compared with original or explicitly known-header C decoding. Not a standards conformance corpus.",
                "reference": report["reference"], "commit": report["commit"], "tools": hashes, "reference_patch_sha256": report["reference_patch_sha256"],
                "cases": retained, "files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(args.fixtures.iterdir()) if p.suffix in (".jpg", ".coef", ".samples")}}
    (args.fixtures / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
