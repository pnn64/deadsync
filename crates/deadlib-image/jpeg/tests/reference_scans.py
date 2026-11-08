"""C/native coefficient gates for native scan generation and caller scripts.

Reuse one case directory to bound disk usage; retain hashes and completed-case
evidence. Executables are snapshotted before work so Cargo cannot replace them.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "inspect", "oracle", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
for name in ("corpus", "sampling-corpus", "model-corpus"):
    parser.add_argument("--" + name, action="append", default=[], type=Path)
parser.add_argument("--resume", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
hashes = {}
for name in ("native", "inspect", "oracle"):
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
    entries = [json.loads(line) for line in checkpoint.read_text().splitlines()]
    assert entries[0] == {"tools": hashes}
    completed = {r["case"]: r for r in entries[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": hashes}) + "\n")
work = args.out / "case"
work.mkdir(exist_ok=True)


def run(command):
    result = subprocess.run([str(x) for x in command], capture_output=True)
    assert result.returncode == 0 and not result.stderr, (command, result.stderr.decode())


def partial(data, al):
    out = bytearray(data)
    count = struct.unpack_from("<I", data, 16)[0]
    position = 20
    for _ in range(count):
        shape = struct.unpack_from("<7I", data, position)
        position += 28 + 128
        for _ in range(shape[5] * shape[6]):
            value = struct.unpack_from("<h", data, position)[0]
            struct.pack_into("<64h", out, position, (value >> al) << al, *([0] * 63))
            position += 128
    assert position == len(data)
    return bytes(out)


root = Path(__file__).resolve().parent / "fixtures"
sources = []
for group in ("sequential", "progressive", "arithmetic", "arithmetic-progressive", "arithmetic-model"):
    for path in sorted((root / group).glob("*.jpg")):
        sources.append((group, "model" if group == "arithmetic-model" else "ordinary", path, path.with_suffix(".coef")))
for kind, corpora in [("ordinary", args.corpus), ("sampling", args.sampling_corpus), ("model", args.model_corpus)]:
    for index, corpus in enumerate(corpora):
        for path in sorted(corpus.glob("*.jpg")):
            expected = path.with_name(path.stem + "-expected.coef")
            if expected.exists():
                sources.append((f"{kind}{index}", kind, path, expected))
seen, unique = set(), []
for group, kind, path, expected in sources:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest not in seen:
        unique.append((group, kind, path, expected, digest))
        seen.add(digest)
records = []
for group, kind, source, expected_path, source_hash in unique:
    expected = expected_path.read_bytes()
    if kind == "ordinary":
        settings = [("auto1", 1, 0), ("auto3", 0, 1), ("auto13", 1, 7),
                    ("split3", 1, 1), ("separate", 1, 7), ("mixed", 0, 0), ("partial3", 1, 0)]
    elif kind == "sampling":
        settings = [("split3", 1, 1), ("separate", 0, 7), ("partial3", 1, 0)]
    else:
        settings = [("auto13", 0, 1), ("split13", 1, 7), ("partial13", 1, 0)]
    for mode, optimize, restart in settings:
        target = partial(expected, int(mode[7:])) if mode.startswith("partial") else expected
        expected_hash = hashlib.sha256(target).hexdigest()
        for arithmetic in (0, 1):
            name = f"{group}-{source.stem}-{mode}-a{arithmetic}-o{optimize}-r{restart}"
            record = completed.get(name)
            if record:
                assert record["source_sha256"] == source_hash and record["coefficient_sha256"] == expected_hash
            else:
                (args.out / "current-case.json").write_text(json.dumps({"case": name, "source": str(source)}) + "\n")
                output = work / "image.jpg"
                run([args.native, source, output, arithmetic, optimize, restart, mode])
                for tool, prefix in [(args.oracle, work / "c"), (args.inspect, work / "native")]:
                    run([tool, output, prefix])
                    assert prefix.with_suffix(".coef").read_bytes() == target, (name, "coefficient mismatch")
                record = {"case": name, "source_sha256": source_hash, "coefficient_sha256": expected_hash,
                          "sha256": hashlib.sha256(output.read_bytes()).hexdigest(), "bytes": output.stat().st_size,
                          "coefficients": "exact", "reference_warnings": 0}
                with checkpoint.open("a") as file:
                    file.write(json.dumps(record) + "\n")
            records.append(record)
            if len(records) % 200 == 0:
                print(f"Passed {len(records)} scan-script encodes", flush=True)
report = {"reference": "libjpeg-turbo 3.2.1 coefficient decoder", "tools": hashes,
          "sources": len(unique), "cases": len(records), "reference_warnings": 0,
          "case_files": "one reusable case directory; hashes retained", "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
