"""Read PNG framing and count residual-row filters; never run a native decoder."""
from collections import Counter
import json
from pathlib import Path
import struct
import sys
import zlib

CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
FILTERS = ("None", "Sub", "Up", "Average", "Paeth")
MAX_FILTERED_BYTES = 64 * 1024 * 1024


def inspect(path):
    wire = path.read_bytes()
    if wire[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not PNG: {path}")
    width, height, depth, color, compression, filtering, interlace = struct.unpack(
        ">IIBBBBB", wire[16:29]
    )
    if interlace or compression or filtering:
        raise ValueError(f"focus script expects ordinary noninterlaced PNG: {path}")
    stride = (width * CHANNELS[color] * depth + 7) // 8
    expected = height * (stride + 1)
    if expected > MAX_FILTERED_BYTES:
        raise ValueError(f"focus input exceeds its 64 MiB analysis cap: {path}")
    payloads = []
    metadata = []
    offset = 8
    while offset + 12 <= len(wire):
        length = int.from_bytes(wire[offset : offset + 4], "big")
        kind = wire[offset + 4 : offset + 8]
        end = offset + length + 12
        if end > len(wire):
            raise ValueError(f"truncated chunk: {path}")
        data = wire[offset + 8 : end - 4]
        if kind == b"IDAT":
            payloads.append(data)
        elif kind not in (b"IHDR", b"PLTE", b"IEND"):
            entry = {"kind": kind.decode("ascii"), "bytes": length}
            if kind in (b"iCCP", b"zTXt"):
                separator = data.find(b"\0")
                compressed = data[separator + 2 :] if separator >= 0 else b""
                if len(compressed) >= 3:
                    entry["compressed_bytes"] = len(compressed)
                    entry["first_deflate_block_type"] = (compressed[2] >> 1) & 3
            metadata.append(entry)
        offset = end
    decoder = zlib.decompressobj()
    raw = decoder.decompress(b"".join(payloads), expected + 1)
    if len(raw) != expected or not decoder.eof:
        raise ValueError(f"filtered length/end mismatch: {path}")
    counts = Counter()
    zero_rows = Counter()
    zero_alpha_rows = Counter()
    provably_zero_neighbors = Counter()
    zero_bytes = 0
    previous_known_zero = True
    for offset in range(0, len(raw), stride + 1):
        filter_byte = raw[offset]
        if filter_byte > 4:
            raise ValueError(f"invalid filter: {path}")
        name = FILTERS[filter_byte]
        row = raw[offset + 1 : offset + 1 + stride]
        counts[name] += 1
        zero_count = row.count(0)
        zero_bytes += zero_count
        zero = zero_count == stride
        if zero:
            zero_rows[name] += 1
        if color == 6 and depth == 8 and row[3::4].count(0) == width:
            zero_alpha_rows[name] += 1
        known_zero = zero and (filter_byte in (0, 1) or previous_known_zero)
        if known_zero and filter_byte in (1, 3, 4):
            provably_zero_neighbors[name] += 1
        previous_known_zero = known_zero
    return {
        "path": path.as_posix(),
        "width": width,
        "height": height,
        "color_type": color,
        "bit_depth": depth,
        "byte_stride": max(1, (CHANNELS[color] * depth + 7) // 8),
        "filtered_bytes": len(raw),
        "idat_chunks": len(payloads),
        "filters": dict(counts),
        "zero_residual_rows": dict(zero_rows),
        "zero_alpha_residual_rows": dict(zero_alpha_rows),
        "provably_zero_neighbor_rows": dict(provably_zero_neighbors),
        "zero_residual_byte_fraction": zero_bytes / (height * stride),
        "metadata": metadata,
    }


if __name__ == "__main__":
    if len(sys.argv) < 3:
        raise SystemExit("usage: analyze-filter-focus.py OUTPUT_JSON INPUT_PNG...")
    output = Path(sys.argv[1])
    records = [inspect(Path(path)) for path in sys.argv[2:]]
    text = json.dumps(records, indent=2)
    output.write_text(text + "\n", encoding="utf-8")
    print(text)
