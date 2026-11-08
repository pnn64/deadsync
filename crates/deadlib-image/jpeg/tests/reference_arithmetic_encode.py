"""Native arithmetic encoding with exact C coefficient validation and resumable evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
for name in ("native", "oracle", "inspect", "out"):
    parser.add_argument("--" + name, required=True, type=Path)
parser.add_argument("--corpus", action="append", type=Path, default=[])
parser.add_argument("--resume", action="store_true")
parser.add_argument("--only-corpus", action="store_true")
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
snapshots = args.out / "tools"
snapshots.mkdir(exist_ok=True)
tool_hashes = {}
for name in ("native", "oracle", "inspect"):
    source = getattr(args, name)
    target = snapshots / (name + source.suffix)
    if not args.resume:
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        shutil.copy2(source, target)
        assert hashlib.sha256(target.read_bytes()).hexdigest() == digest
    tool_hashes[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    setattr(args, name, target.resolve())
checkpoint = args.out / "checkpoint.jsonl"
completed = {}
if args.resume:
    entries = []
    lines = checkpoint.read_bytes().splitlines(keepends=True)
    offset = 0
    for index, line in enumerate(lines):
        try:
            entries.append(json.loads(line))
        except (json.JSONDecodeError, UnicodeDecodeError):
            # A stopped write may leave an incomplete final record. Earlier
            # corruption is always an error; the unfinished case is rerun.
            if index != len(lines) - 1:
                raise
            with checkpoint.open("r+b") as file:
                file.truncate(offset)
            print("Discarded incomplete final checkpoint record", flush=True)
            break
        offset += len(line)
        if index == len(lines) - 1 and not line.endswith(b"\n"):
            with checkpoint.open("ab") as file:
                file.write(b"\n")
    assert entries[0] == {"tools": tool_hashes}, "snapshot tools changed"
    completed = {r["case"]: r for r in entries[1:]}
else:
    checkpoint.write_text(json.dumps({"tools": tool_hashes}) + "\n")


def run(command):
    result = subprocess.run([str(x) for x in command], check=True, capture_output=True)
    assert not result.stderr, result.stderr.decode()


root = Path(__file__).resolve().parent / "fixtures"
sources = []
for group in (() if args.only_corpus else ("sequential", "progressive", "arithmetic", "arithmetic-progressive")):
    for path in sorted((root / group).glob("*.jpg")):
        sources.append((group, path, path.with_suffix(".coef")))
for index, corpus in enumerate(args.corpus):
    for path in sorted(corpus.glob("*.jpg")):
        expected = path.with_name(path.stem + "-expected.coef")
        if expected.exists():
            sources.append((f"corpus{index}", path, expected))
seen = set()
unique = []
for group, path, expected in sources:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest not in seen:
        unique.append((group, path, expected, digest))
        seen.add(digest)
settings = [(r, 0, 1, 5) for r in (0, 1, 7)]
settings += [(1, 0, 0, 1), (7, 3, 7, 63), (0, 15, 15, 32)]
records = []
for group, source, expected_path, source_hash in unique:
    expected = expected_path.read_bytes()
    coefficient_hash = hashlib.sha256(expected).hexdigest()
    for progressive in (0, 1):
        for restart, lower, upper, split in settings:
            name = f"{group}-{source.stem}-p{progressive}-r{restart}-c{lower}-{upper}-{split}"
            path = args.out / (name + ".jpg")
            native = args.out / (name + "-native")
            reference = args.out / (name + "-c")
            record = completed.get(name)
            if record:
                assert record["source_sha256"] == source_hash and record["coefficient_sha256"] == coefficient_hash
                assert record["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
                assert native.with_suffix(".coef").read_bytes() == expected
                assert reference.with_suffix(".coef").read_bytes() == expected
            else:
                run([args.native, source, path, progressive, 1, restart, 1, lower, upper, split])
                run([args.oracle, path, reference])
                assert reference.with_suffix(".coef").read_bytes() == expected, (name, "C coefficients")
                run([args.inspect, path, native])
                assert native.with_suffix(".coef").read_bytes() == expected, (name, "native coefficients")
                record = {"case": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                          "source_sha256": source_hash, "coefficient_sha256": coefficient_hash,
                          "bytes": path.stat().st_size, "c_coefficients": "exact", "native_coefficients": "exact"}
                with checkpoint.open("a") as file:
                    file.write(json.dumps(record) + "\n")
            records.append(record)
            if len(records) % 200 == 0:
                print(f"Passed {len(records)} native arithmetic encodes", flush=True)
report = {"reference": "libjpeg-turbo 3.2.1 C coefficient decoder", "tools": tool_hashes,
          "sources": len(unique), "cases": len(records), "reference_warnings": 0, "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
