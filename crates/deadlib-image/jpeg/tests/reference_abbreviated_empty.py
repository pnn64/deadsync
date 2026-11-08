"""B.2.4.2 permits zero Li: retain unused empty destinations, reject their use."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "turbo", "fixtures", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
manifest = json.loads((args.fixtures / "manifest.json").read_text())
source = next(r for r in manifest["sessions"] if r["part"] == "c")
payload = b"".join(bytes([tc * 16 + th]) + bytes(16) for tc in (0, 1) for th in range(4))
packet = b"\xff\xd8\xff\xc4" + (len(payload) + 2).to_bytes(2, "big") + payload + b"\xff\xd9"
streams = [packet] + [(args.fixtures / r["file"]).read_bytes() for r in source["streams"][:2]]
expected = (args.fixtures / source["images"][0][0]["file"]).read_bytes()
paths = []
for i, data in enumerate(streams):
    file = args.fixtures / f"empty-definitions.stream-{i}.jpg"
    file.write_bytes(data)
    paths.append(file)
results = []
for name in ("native", "turbo"):
    tool = getattr(args, name)
    prefix = args.out / name
    p = subprocess.run([str(s) for s in [tool, "decode", prefix, paths[0]]], capture_output=True)
    assert not p.returncode and not p.stderr and p.stdout.strip() == b"tables 1 images 0", (name, p.stderr)
    p = subprocess.run([str(s) for s in [tool, "decode", prefix, *paths]], capture_output=True)
    assert not p.returncode and not p.stderr and p.stdout.strip() == b"tables 2 images 1", (name, p.stderr)
    assert Path(str(prefix) + ".image-0.coef").read_bytes() == expected
    # Replace a needed DC destination after loading the real table packet.
    used = args.out / "used-empty.jpg"
    used.write_bytes(b"\xff\xd8\xff\xc4\x00\x13" + bytes(17) + b"\xff\xd9")
    p = subprocess.run([str(s) for s in [tool, "decode", prefix, paths[1], used, paths[2]]], capture_output=True)
    assert p.returncode or p.stderr, (name, "empty used table was accepted")
    results.append({"tool": name, "sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
                    "unused_empty_destinations": 8, "table_only_and_image_exact": True,
                    "used_empty_diagnostic": p.stderr.decode(errors="replace"), "used_empty_exit": p.returncode})
file = "empty-definitions.image-0.coef"
(args.fixtures / file).write_bytes(expected)
row = {"case": "empty-definitions", "part": "external", "new_coverage": ["eight empty Huffman destinations"],
       "table_packet_origin": "independent B.2.4.2 zero-Li definition; verbatim C packet/image follows",
       "streams": [{"file": p.name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths],
       "images": [[{"file": file, "suffix": "coef", "sha256": hashlib.sha256(expected).hexdigest()}]]}
manifest["sessions"] = [r for r in manifest["sessions"] if r["case"] != row["case"]] + [row]
(args.fixtures / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
report = {"scope": __doc__, "driver_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "results": results}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
