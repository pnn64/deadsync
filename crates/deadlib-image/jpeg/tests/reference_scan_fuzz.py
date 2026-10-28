"""Deterministic legal scan-graph mutations, independently decoded by C."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "oracle", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "inspect", "oracle"):
    source = getattr(args, name)
    target = snapshots / (name + source.suffix)
    shutil.copy2(source, target)
    hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    assert hashes[name] == hashlib.sha256(source.read_bytes()).hexdigest()
    setattr(args, name, target.resolve())


def run(command):
    result = subprocess.run([str(x) for x in command], capture_output=True)
    assert result.returncode == 0 and not result.stderr, (command, result.stderr.decode())
    return result.stdout


root = Path(__file__).resolve().parent / "fixtures"
sources = [root / name for name in ("sequential/8-gray.jpg", "sequential/12-gray.jpg",
    "sequential/8-420.jpg", "sequential/12-420.jpg", "sequential/8-cmyk.jpg",
    "arithmetic-model/p8-gray-17x17-w0-d1.jpg", "arithmetic-model/p12-ycck-17x17-w1-d1.jpg")]
records = []
for index in range(64):
    source = sources[index % len(sources)]
    expected = source.with_suffix(".coef").read_bytes()
    seed = 17 + index * 7919
    for arithmetic in (0, 1):
        output = args.out / "case.jpg"
        count = int(run([args.native, source, output, arithmetic, index % 2, (0, 1, 7)[index % 3], f"random{seed}"]))
        assert 1 <= count <= 4096
        for tool, name in [(args.oracle, "c"), (args.inspect, "native")]:
            prefix = args.out / name
            run([tool, output, prefix])
            assert prefix.with_suffix(".coef").read_bytes() == expected, (index, arithmetic, "coefficients")
        records.append({"source": source.name, "seed": seed, "arithmetic": arithmetic,
            "script_scans": count, "coefficients": "exact", "bytes": output.stat().st_size,
            "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
report = {"reference": "libjpeg-turbo 3.2.1 coefficient decoder", "tools": hashes,
    "cases": len(records), "reference_warnings": 0,
    "min_script_scans": min(r["script_scans"] for r in records),
    "max_script_scans": max(r["script_scans"] for r in records), "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
