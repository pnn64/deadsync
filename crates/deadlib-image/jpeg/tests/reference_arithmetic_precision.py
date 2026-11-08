"""C progressive arithmetic encoding of the full signed 16-bit coefficient model."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
for name in ("transcode", "oracle", "inspect", "corpus", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--pin", type=Path)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("transcode", "oracle", "inspect"):
    source = getattr(args, name)
    target = snapshots / (name + source.suffix)
    shutil.copy2(source, target)
    hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    assert hashes[name] == hashlib.sha256(source.read_bytes()).hexdigest()
    setattr(args, name, target.resolve())


def run(command):
    result = subprocess.run([str(x) for x in command], check=True, capture_output=True)
    assert not result.stderr, result.stderr.decode()


records = []
pinned = []
for source in sorted(args.corpus.glob("*.jpg")):
    expected = source.with_name(source.stem + "-expected.coef").read_bytes()
    for scan in (1, 3):
        for restart, lower, upper, split in [(0, 0, 1, 5), (1, 0, 0, 1), (7, 15, 15, 63)]:
            name = f"{source.stem}-s{scan}-r{restart}"
            path = args.out / (name + ".jpg")
            run([args.transcode, source, path, restart, lower, upper, split, 2, scan])
            for tool, suffix in [(args.oracle, "-c"), (args.inspect, "-native")]:
                prefix = args.out / (name + suffix)
                run([tool, path, prefix])
                assert prefix.with_suffix(".coef").read_bytes() == expected, (name, suffix)
            records.append({"case": name, "coefficients": "exact", "bytes": path.stat().st_size,
                            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                            "coefficient_sha256": hashlib.sha256(expected).hexdigest()})
            if (args.pin and scan == 3 and restart == 1
                    and source.stem in ("p8-gray-17x17-w0-d1", "p12-ycck-17x17-w1-d1")):
                args.pin.mkdir(parents=True, exist_ok=True)
                target = args.pin / (source.stem + ".jpg")
                shutil.copy2(path, target)
                shutil.copy2(args.out / (name + "-c.coef"), target.with_suffix(".coef"))
                pinned.append({**records[-1], "file": target.name,
                               "scan_layout": scan, "restart_interval": restart,
                               "conditioning": [lower, upper, split], "table_layout": 2})
report = {"reference": "libjpeg-turbo 3.2.1 progressive arithmetic coefficient encoder/decoder",
          "tools": hashes, "cases": len(records), "reference_warnings": 0, "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
if args.pin:
    assert len(pinned) == 2
    (args.pin / "manifest.json").write_text(json.dumps({"reference": report["reference"],
        "tools": hashes, "cases": pinned}, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
