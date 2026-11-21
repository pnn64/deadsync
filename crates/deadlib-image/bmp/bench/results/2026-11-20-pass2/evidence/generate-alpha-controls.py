import hashlib
import json
from pathlib import Path
import struct

task = Path(__file__).resolve().parent
destination = task / "alpha-controls"
destination.mkdir()
manifest = []
for width, height in ((17, 9), (511, 257), (1920, 1080)):
    count = width * height
    pattern = bytes(channel for index in range(256) for channel in ((index * 139 + 41) & 255, (index * 71 + 19) & 255, index, 0))
    source = (pattern * ((count + 255) // 256))[:count * 4]
    for scenario in ("zero", "first", "last"):
        logical = bytearray(source)
        if scenario != "zero":
            logical[(0 if scenario == "first" else count - 1) * 4 + 3] = 1
        expected_pattern = bytes(channel for index in range(256) for channel in (index, (index * 71 + 19) & 255, (index * 139 + 41) & 255, 255 if scenario == "zero" else 0))
        expected = bytearray((expected_pattern * ((count + 255) // 256))[:count * 4])
        if scenario != "zero":
            expected[(0 if scenario == "first" else count - 1) * 4 + 3] = 1
        expected_hash = hashlib.sha256(expected).hexdigest()
        for top_down in (False, True):
            header = bytearray(54)
            header[:2] = b"BM"
            struct.pack_into("<I", header, 2, 54 + len(logical))
            struct.pack_into("<I", header, 10, 54)
            struct.pack_into("<IiiHHII", header, 14, 40, width, -height if top_down else height, 1, 32, 0, len(logical))
            row_bytes = width * 4
            pixels = logical if top_down else b"".join(logical[row * row_bytes:(row + 1) * row_bytes] for row in reversed(range(height)))
            encoded = header + pixels
            name = f"bgra32-{scenario}-{width}x{height}-{'top' if top_down else 'bottom'}.bmp"
            (destination / name).write_bytes(encoded)
            manifest.append({"file": name, "width": width, "height": height, "top_down": top_down, "scenario": scenario, "input_bytes": len(encoded), "input_sha256": hashlib.sha256(encoded).hexdigest(), "expected_rgba_sha256": expected_hash})
(destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
with (destination / "manifest.tsv").open("x", encoding="utf-8", newline="\n") as output:
    output.write("file\twidth\theight\ttop_down\tscenario\n")
    for case in manifest:
        output.write(f"{case['file']}\t{case['width']}\t{case['height']}\t{str(case['top_down']).lower()}\t{case['scenario']}\n")
print(f"Generated {len(manifest)} alpha-position controls; {sum(case['input_bytes'] for case in manifest):,} encoded bytes. Each has an independent expected RGBA SHA-256.")
