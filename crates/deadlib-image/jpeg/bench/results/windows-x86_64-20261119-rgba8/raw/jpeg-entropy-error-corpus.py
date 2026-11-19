"""Generate <=966 tiny native JPEG exact-error cases; never modifies fixtures.
Run only after timing: python target/jpeg-perf-20261119/jpeg-entropy-error-corpus.py
The new output directory contains cases, manifest.json, and paths.txt (repo-relative).
"""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target" / "jpeg-perf-20261119"
FIXTURES = [
    "sequential/8-gray.jpg",
    "sequential/8-420.jpg",
    "sequential/8-cmyk.jpg",
    "sequential/8-restart.jpg",
    "progressive/8-420.jpg",
    "progressive/12-420.jpg",
]
MAX_SOURCE_BYTES = 4096
TRUNCATIONS = 80
BIT_OFFSETS = 10
MAX_CASES = len(FIXTURES) * (1 + TRUNCATIONS + 8 * BIT_OFFSETS)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def sample(values, count):
    values = sorted(set(values))
    if count <= 0:
        return []
    if len(values) <= count:
        return values
    if count == 1:
        return [values[len(values) // 2]]
    return [values[i * (len(values) - 1) // (count - 1)] for i in range(count)]


def layout(data):
    if data[:2] != b"\xff\xd8":
        raise ValueError("source lacks SOI")
    at, boundaries, scans, critical, frames = 2, {0, 1, 2}, [], [], []
    while at < len(data):
        start = at
        if data[at] != 255:
            raise ValueError("source has stray marker bytes")
        while at < len(data) and data[at] == 255:
            at += 1
        if at == len(data):
            raise ValueError("source ends in marker prefix")
        marker = data[at]
        at += 1
        boundaries.update((start, at - 1, at))
        if marker == 0xD9:
            break
        if marker == 1 or 0xD0 <= marker <= 0xD7:
            continue
        if at + 2 > len(data):
            raise ValueError("source has short marker length")
        length = int.from_bytes(data[at:at + 2], "big")
        end = at + length
        if length < 2 or end > len(data):
            raise ValueError("source has invalid segment length")
        payload = at + 2
        boundaries.update((at, at + 1, payload, end))
        if marker in (0xC0, 0xC1, 0xC2):
            frames.append({"offset": start, "marker": marker, "precision": data[payload],
                           "height": int.from_bytes(data[payload + 1:payload + 3], "big"),
                           "width": int.from_bytes(data[payload + 3:payload + 5], "big")})
        at = end
        if marker != 0xDA:
            continue
        entropy_start, cursor = end, end
        while True:
            prefix = data.find(b"\xff", cursor)
            if prefix < 0:
                raise ValueError("source entropy lacks a terminating marker")
            following = prefix + 1
            while following < len(data) and data[following] == 255:
                following += 1
            if following == len(data):
                raise ValueError("source entropy ends in marker prefix")
            code = data[following]
            if code == 0 or 0xD0 <= code <= 0xD7:
                if code == 0 and following != prefix + 1:
                    raise ValueError("source has fill before stuffed byte")
                boundaries.update((prefix, following, following + 1))
                critical.extend((prefix, following))
                cursor = following + 1
                continue
            scans.append((entropy_start, prefix))
            boundaries.update((entropy_start, prefix))
            at = prefix
            break
    if not scans or not frames or any(frame["offset"] >= scans[0][0] for frame in frames):
        raise ValueError("expected DCT frames before the first entropy payload")
    return boundaries, scans, critical, frames


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=TARGET / "jpeg-error-corpus")
    output = parser.parse_args().output.resolve()
    if output == TARGET or not output.is_relative_to(TARGET):
        raise ValueError("output must be a new child of target/jpeg-perf-20261119")
    prepared = []
    for fixture in FIXTURES:
        original = ROOT / "crates/deadlib-image/jpeg/tests/fixtures" / fixture
        data = original.read_bytes()
        if len(data) > MAX_SOURCE_BYTES:
            raise ValueError(f"fixture exceeds {MAX_SOURCE_BYTES} bytes: {fixture}")
        boundaries, scans, critical, frames = layout(data)
        first_entropy = scans[0][0]
        mandatory = {0, 1, 2, len(data) - 2, len(data) - 1,
                     first_entropy - 1, first_entropy, first_entropy + 1}
        mandatory = {at for at in mandatory if 0 <= at < len(data)}
        pool = {at + delta for at in boundaries for delta in (-2, -1, 0, 1, 2)}
        pool.update(i * (len(data) - 1) // 32 for i in range(33))
        pool = {at for at in pool if 0 <= at < len(data)}
        truncations = sorted(mandatory | set(sample(pool - mandatory, TRUNCATIONS - len(mandatory))))
        eligible = {at for start, end in scans for at in range(start, end)}
        if not eligible:
            raise ValueError(f"fixture has no entropy bytes: {fixture}")
        focused = {at + delta for at in critical for delta in (-1, 0, 1)}
        focused.update(at for start, end in scans for at in (start, start + 1, end - 1))
        focused &= eligible
        offsets = {min(eligible), max(eligible)}
        offsets.update(sample(focused - offsets, 4))
        offsets.update(sample(eligible - offsets, BIT_OFFSETS - len(offsets)))
        prepared.append((original, data, {
            "fixture": fixture, "source": original.relative_to(ROOT).as_posix(),
            "bytes": len(data), "sha256": sha(data), "frames": frames,
            "first_entropy": first_entropy, "entropy_ranges": scans,
            "truncation_positions": truncations, "bit_offsets": sorted(offsets),
        }))
    output.mkdir(parents=False, exist_ok=False)
    cases, total_bytes = [], 0

    def emit(source_index, operation, data, detail):
        nonlocal total_bytes
        ordinal = len(cases)
        if ordinal >= MAX_CASES:
            raise ValueError("case bound exceeded")
        name = f"{ordinal:04d}-s{source_index}-{operation}.jpg"
        path = output / name
        with path.open("xb") as stream:
            stream.write(data)
        total_bytes += len(data)
        cases.append({"ordinal": ordinal, "source": source_index, "operation": operation,
                      "detail": detail, "path": path.relative_to(ROOT).as_posix(),
                      "bytes": len(data), "sha256": sha(data)})

    for source_index, (_, data, settings) in enumerate(prepared):
        emit(source_index, "original", data, {})
        for at in settings["truncation_positions"]:
            emit(source_index, f"truncate-{at:04d}", data[:at], {"length": at})
        for at in settings["bit_offsets"]:
            for bit in range(8):
                changed = bytearray(data)
                changed[at] ^= 1 << bit
                if changed[:settings["first_entropy"]] != data[:settings["first_entropy"]]:
                    raise ValueError("mutation touched pre-entropy headers")
                emit(source_index, f"flip-{at:04d}-b{bit}", changed,
                     {"offset": at, "lsb_bit": bit, "xor_mask": 1 << bit})
    manifest = {
        "schema": 1, "generator": Path(__file__).relative_to(ROOT).as_posix(),
        "generator_sha256": sha(Path(__file__).read_bytes()),
        "settings": {"max_source_bytes": MAX_SOURCE_BYTES, "max_cases": MAX_CASES,
                     "max_truncations_per_source": TRUNCATIONS,
                     "max_bit_offsets_per_source": BIT_OFFSETS,
                     "bit_policy": "all 8 one-bit flips at selected entropy-only offsets",
                     "selection": "deterministic boundary anchors plus evenly spaced samples",
                     "random_seed": None, "path_base": "repository working directory"},
        "case_count": len(cases), "case_bytes": total_bytes,
        "sources": [settings for _, _, settings in prepared], "cases": cases,
    }
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (output / "paths.txt").write_text("".join(case["path"] + "\n" for case in cases), encoding="utf-8")
    print(json.dumps({"cases": len(cases), "bytes": total_bytes,
                      "manifest": str(output / "manifest.json"), "paths": str(output / "paths.txt")}))


if __name__ == "__main__":
    main()
