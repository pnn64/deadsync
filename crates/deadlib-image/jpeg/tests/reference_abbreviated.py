"""Persistent B.4/B.5 sessions, with exact coefficient and sample comparisons.

Turbo uses its genuine public abbreviated-stream API. JPEG XT has no such
adapter here: rare-process C checks receive an explicitly reconstructed
interchange stream using an independent raw table bank. Those checks establish
entropy/frame-data parity, not independent parsing of abbreviated syntax.
"""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure, samples, check_frame_data
from stream_support import marker_ranges, update_tables, prefix_tables, without, clear_stale_exp


def sha(data):
    return hashlib.sha256(data).hexdigest()


parser = argparse.ArgumentParser()
for name in ("native", "turbo", "reference", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--part", choices=("c", "native", "rare"), required=True)
parser.add_argument("--resume", action="store_true")
parser.add_argument("--fixtures", type=Path)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
work = args.out / "case"
work.mkdir(exist_ok=True)
tools = args.out / "tools"
tools.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "turbo", "reference"):
    source = getattr(args, name)
    target = tools / (name + source.suffix)
    if not args.resume:
        shutil.copy2(source, target)
        assert source.read_bytes() == target.read_bytes()
    hashes[name] = sha(target.read_bytes())
    setattr(args, name, target.resolve())
checkpoint = args.out / "checkpoint.jsonl"
identity = {"tools": hashes, "part": args.part, "driver": sha(Path(__file__).read_bytes()),
            "stream_support": sha(Path(__file__).with_name("stream_support.py").read_bytes())}
if args.resume:
    previous = [json.loads(line) for line in checkpoint.read_text().splitlines()]
    assert previous[0] == identity
    completed = {r["case"]: r for r in previous[1:]}
else:
    checkpoint.write_text(json.dumps(identity) + "\n")
    completed = {}
records, retained, coverage = [], [], set()
if args.fixtures:
    args.fixtures.mkdir(parents=True, exist_ok=True)


def run(command, env=None, strict=True):
    result = subprocess.run([str(s) for s in command], capture_output=True, env=env)
    if strict:
        assert not result.returncode and not result.stderr and b"warning" not in result.stdout.lower(), (
            command, result.returncode, result.stderr.decode(errors="replace"), result.stdout.decode(errors="replace"))
    return result


def save(record):
    records.append(record)
    with checkpoint.open("a") as file:
        file.write(json.dumps(record) + "\n")
    if len(records) % 25 == 0:
        print(f"Checked {len(records)} {args.part} sessions", flush=True)


def suffix(prefix, tail):
    return Path(str(prefix) + tail)


def decoders(paths, count, turbo=True):
    manifest = work / "streams.txt"
    manifest.write_text("".join(str(p.resolve()) + "\n" for p in paths))
    expected = f"tables {len(paths) - count} images {count}"
    for who in (("native", "turbo") if turbo else ("native",)):
        command = [getattr(args, who), "decode" if who == "native" else "decode-list", work / who,
                   "@" + str(manifest.resolve()) if who == "native" else manifest]
        assert run(command).stdout.decode().strip() == expected


def concatenate(paths, count, turbo=True):
    joined = work / "joined.bin"
    joined.write_bytes(b"".join(p.read_bytes() for p in paths))
    expected = f"tables {len(paths) - count} images {count}"
    for who in (("native", "turbo") if turbo else ("native",)):
        assert run([getattr(args, who), "decode", work / (who + "-joined"), joined]).stdout.decode().strip() == expected


def retain(name, paths, expected, keys):
    fresh = set(keys) - coverage
    if not fresh or not args.fixtures:
        return None
    coverage.update(fresh)
    saved_paths, saved_expected = [], []
    for i, path in enumerate(paths):
        dest = args.fixtures / f"{args.part}-{name}.stream-{i}.jpg"
        dest.write_bytes(path.read_bytes())
        saved_paths.append({"file": dest.name, "sha256": sha(dest.read_bytes())})
    for i, image in enumerate(expected):
        dumps = []
        for tail, data in image:
            dest = args.fixtures / f"{args.part}-{name}.image-{i}.{tail}"
            dest.write_bytes(data)
            dumps.append({"file": dest.name, "suffix": tail, "sha256": sha(data)})
        saved_expected.append(dumps)
    row = {"case": name, "part": args.part, "streams": saved_paths, "images": saved_expected,
           "new_coverage": sorted(fresh)}
    retained.append(row)
    return row


def c_case(name, params, reset=False):
    if name in completed:
        record = completed[name]
        records.append(record)
        if record.get("fixture"):
            retained.append(record["fixture"])
            coverage.update(record["fixture"]["new_coverage"])
        return
    (args.out / "current-case.json").write_text(json.dumps({"case": name}))
    prefix = work / "c"
    run([args.turbo, "encode-reset" if reset else "encode", prefix, *params])
    p, color, quality, progressive, arithmetic, lossless, predictor, point, restart, sampling, repeats = params
    paths = [suffix(prefix, ".tables.jpg")] + [suffix(prefix, f".image-{i}.jpg") for i in range(repeats)]
    bank = {}
    update_tables(bank, paths[0].read_bytes(), True)
    if reset:
        assert {0xcc, 0xdd} <= {m for m, _, _ in marker_ranges(paths[1].read_bytes())}
        paths[2].write_bytes(without(paths[2].read_bytes(), {0xcc, 0xdd}))
        assert not {0xcc, 0xdd} & {m for m, _, _ in marker_ranges(paths[2].read_bytes())}
    for image in paths[1:]:
        assert 0xdb not in {m for m, _, _ in marker_ranges(image.read_bytes())}
    decoders(paths, repeats)
    expected, count = [], 0
    extension = "samples" if lossless else "coef"
    for i in range(repeats):
        actual = suffix(work / "native", f".image-{i}.{extension}").read_bytes()
        assert actual == suffix(work / "turbo", f".image-{i}.{extension}").read_bytes()
        if lossless:
            precision, width, height, components = samples(actual)
            assert (precision, width, height) == (p, 31, 25)
            for channel, (_, values) in enumerate(components):
                wanted = tuple((((x * 193 + y * 317 + channel * 911 + x * y * 53 + i * 977 + i * x * y * 7)
                                 & ((1 << p) - 1)) >> point) << point for y in range(25) for x in range(31))
                assert values == wanted
                count += len(values)
        expected.append([(extension, actual)])
    concatenate(paths, repeats)
    for i, image in enumerate(expected):
        for tail, data in image:
            for who in ("native", "turbo"):
                assert suffix(work / (who + "-joined"), f".image-{i}.{tail}").read_bytes() == data
    key = f"{('LP' if lossless else 'P' if progressive else 'S')}-a{arithmetic}-p{p}"
    fixture = retain(name, paths, expected, [key] if p in ((2, 16) if lossless else (8, 12)) else [])
    save({"case": name, "images": repeats, "lossless_samples": count, "params": params,
          "stream_sha256": [sha(f.read_bytes()) for f in paths], "bytes": sum(f.stat().st_size for f in paths),
          "data_sha256": sha(b"".join(data for image in expected for _, data in image)),
          "fixture": fixture, "soi_reset": reset, "genuine_c_b5": True})


def c_gate():
    colors = ("gray", "rgb", "ycbcr", "cmyk", "ycck")
    for p, color, progressive, arithmetic, restart, quality in itertools.product(
            (8, 12), colors, (0, 1), (0, 1), (0, 1), (30, 85, 100)):
        for sampling in (("444",) if color == "gray" else ("444", "420", "422")):
            name = f"dct-p{p}-{color}-v{progressive}-a{arithmetic}-r{restart}-q{quality}-{sampling}"
            c_case(name, (p, color, quality, progressive, arithmetic, 0, 1, 0, restart, sampling, 3))
    for p, ci, point_index, restart in itertools.product(range(2, 17), range(5), (0, 1), (0, 1, 3)):
        point = 0 if point_index == 0 else p - 1
        predictor = (p + ci + point + restart) % 7 + 1
        name = f"lp-p{p}-{colors[ci]}-pt{point}-r{restart}-pred{predictor}"
        c_case(name, (p, colors[ci], 85, 0, 0, 1, predictor, point, restart, "444", 3))
    for p, progressive, color in itertools.product((8, 12), (0, 1), ("rgb", "ycck")):
        name = f"reset-p{p}-v{progressive}-{color}"
        c_case(name, (p, color, 85, progressive, 1, 0, 1, 0, 1, "420", 2), True)


def c_rare(path, family, frames, expected):
    observation = work / "observation.txt"
    observation.write_text("")
    result = run([args.reference, "-c", "-U", path, work / "pgx"],
                 os.environ | {"DEADLIB_JPEG_FRAME_DUMP": str(observation.resolve())}, False)
    if result.returncode or result.stderr or b"warning" in result.stdout.lower():
        return None, result.stderr.decode(errors="replace") or result.stdout.decode(errors="replace")
    try:
        if family == "lossless":
            p, _, _, components = samples(expected[0])
            count = 0
            for i, (shape, values) in enumerate(components):
                assert (work / f"pgx_{i}.h").read_text().split() == ["PG", "ML", f"+{p}", str(shape[3]), str(shape[4])]
                raw = (work / f"pgx_{i}.raw").read_bytes()
                actual = tuple(raw) if p <= 8 else struct.unpack(">" + "H" * (len(raw) // 2), raw)
                assert actual == values, "C lossless samples differ"
                count += len(values)
            return (0, count), None
        return check_frame_data(frames, expected, observation.read_text()), None
    except (AssertionError, OSError, struct.error) as failure:
        return None, str(failure)[:1000]


def native_gate(rare):
    root = Path(__file__).parent / "fixtures"
    directories = (("lossless-arithmetic", "hierarchical", "hierarchical-native", "pyramid-native") if rare else
                   ("sequential", "progressive", "arithmetic", "arithmetic-progressive", "arithmetic-model", "lossless"))
    sources = [p for d in directories for p in sorted((root / d).glob("*.jpg"))]
    # Each batch changes table destinations across many images using one cache.
    for fixed, restart in itertools.product((0, 1), (0, 1, 3)):
        for start in range(0, len(sources), 32):
            batch = sources[start:start + 32]
            name = f"fixed{fixed}-r{restart}-batch{start // 32}"
            source_hashes = [sha(p.read_bytes()) for p in batch]
            if name in completed:
                record = completed[name]
                assert record["sources"] == source_hashes
                records.append(record)
                for fixture in record["fixtures"]:
                    retained.append(fixture)
                    coverage.update(fixture["new_coverage"])
                continue
            (args.out / "current-case.json").write_text(json.dumps({"case": name, "sources": [str(p) for p in batch]}))
            prefix = work / "encoded"
            count = len(batch) * 2
            assert run([args.native, "encode", prefix, fixed, restart, 2, *batch]).stdout.decode().strip() == f"images {count}"
            paths = [suffix(prefix, f".image-{i}{tail}") for i in range(count) for tail in (".tables.jpg", ".jpg")]
            decoders(paths, count, not rare)
            bank, expectations, fixtures, c_counts, c_copies = {}, [], [], [0, 0], []
            for i in range(count):
                packet, image = paths[i * 2:i * 2 + 2]
                wire = image.read_bytes()
                update_tables(bank, packet.read_bytes(), True)
                header, frames, _, _, _ = structure(wire, False)
                family = "hierarchy" if header["marker"] == 0xde else "lossless" if frames[0]["marker"] & 3 == 3 else "dct"
                tails = ([f"frame-{j}.{'samples' if f['marker'] & 3 == 3 else 'coef'}" for j, f in enumerate(frames)] + ["samples"]
                         if family == "hierarchy" else ["samples" if family == "lossless" else "coef"])
                expected = [(tail, suffix(prefix, f".image-{i}.expected.{tail}").read_bytes()) for tail in tails]
                for tail, data in expected:
                    assert suffix(work / "native", f".image-{i}.{tail}").read_bytes() == data, (name, i, tail)
                    if not rare:
                        assert suffix(work / "turbo", f".image-{i}.{tail}").read_bytes() == data, (name, i, tail)
                if i % 2 and family != "hierarchy":
                    assert packet.read_bytes() == b"\xff\xd8\xff\xd9"
                if rare:
                    equivalent = prefix_tables(bank, wire)
                    c_input = work / "equivalent.jpg"
                    c_input.write_bytes(equivalent)
                    frame_data = [data for _, data in expected[:-1]] if family == "hierarchy" else [expected[0][1]]
                    check, reason = c_rare(c_input, family, frames, frame_data)
                    if check is None:
                        fixed_exp = clear_stale_exp(equivalent, frames)
                        assert fixed_exp != equivalent, (name, i, reason)
                        c_input.write_bytes(fixed_exp)
                        check, other = c_rare(c_input, family, frames, frame_data)
                        assert check is not None, (name, i, reason, other)
                        c_copies.append({"image": i, "reason": reason, "copy": "insert EXP00 for stale C expansion state"})
                    c_counts = [a + b for a, b in zip(c_counts, check)]
                update_tables(bank, wire)
                expectations.append(expected)
                # Capture a complete prefix to make retained mixed sessions reproducible.
                keys = [f"marker-{f['marker']:02x}-p{f['precision']}" for f in frames
                        if f['precision'] in ((2, 16) if f['marker'] & 3 == 3 else (8, 12))]
                if i % 2 == 1 and set(keys) - coverage:
                    fixture = retain(name + f"-through{i}", paths[:(i + 1) * 2], expectations, keys)
                    if fixture:
                        fixtures.append(fixture)
            concatenate(paths, count, not rare)
            for i, expected in enumerate(expectations):
                for tail, data in expected:
                    for who in (("native",) if rare else ("native", "turbo")):
                        assert suffix(work / (who + "-joined"), f".image-{i}.{tail}").read_bytes() == data
            save({"case": name, "images": count, "sources": source_hashes,
                  "source_files": [str(p.relative_to(root)) for p in batch], "fixed": fixed, "restart": restart,
                  "stream_sha256": sha(b"".join(p.read_bytes() for p in paths)), "bytes": sum(p.stat().st_size for p in paths),
                  "exact_c_dct_blocks": c_counts[0], "exact_c_lossless_samples": c_counts[1],
                  "c_exp_workaround": c_copies, "genuine_c_b5": not rare, "fixtures": fixtures})


if args.part == "c":
    c_gate()
else:
    native_gate(args.part == "rare")
report = {"part": args.part, "tools": hashes, "driver": identity["driver"], "sessions": len(records),
          "unique_images": sum(r["images"] for r in records), "separate_and_concatenated": True,
          "reference": "libjpeg-turbo 3.2.1 public B.5 API" if args.part != "rare" else "JPEG XT 702114c equivalent interchange only",
          "scope": "Exact coefficients/quantizers and lossless samples. Metadata stays image-local. No pixel-tolerance claims.",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
if args.fixtures:
    (args.fixtures / (args.part + "-manifest.json")).write_text(json.dumps({"tools": hashes, "sessions": retained}, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}), flush=True)
