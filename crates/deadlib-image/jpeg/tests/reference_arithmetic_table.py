"""Compare independently transcribed T.81 probability facts with pinned C reference data."""
import argparse
import hashlib
import json
from pathlib import Path
import re

parser = argparse.ArgumentParser()
parser.add_argument("--reference-source", required=True, type=Path)
parser.add_argument("--out", required=True, type=Path)
args = parser.parse_args()
native_source = Path(__file__).resolve().parents[1] / "src/arithmetic.rs"
native = [(int(q, 16), int(l), int(m), int(s == "true")) for q, l, m, s in re.findall(
    r"p\((0x[0-9a-f]+),\s*(\d+),\s*(\d+),\s*(true|false)\)", native_source.read_text())]
reference = [(int(i), int(q, 16), int(l), int(m), int(s)) for i, q, l, m, s in re.findall(
    r"V\(\s*(\d+),\s*(0x[0-9a-f]+),\s*(\d+),\s*(\d+),\s*(\d+)\s*\)", args.reference_source.read_text())]
assert len(native) == 113 and len(reference) == 114
assert [r[0] for r in reference] == list(range(114))
assert native == [r[1:] for r in reference[:113]], "probability value/transition mismatch"
# The C library adds a non-adapting bin; the native fixed coder resets state zero instead.
assert reference[113] == (113, 0x5a1d, 113, 113, 0)
report = {"reference": "libjpeg-turbo 3.2.1 jaricom.c probability data (test only)",
          "reference_source_sha256": hashlib.sha256(args.reference_source.read_bytes()).hexdigest(),
          "probability_states": len(native), "values_and_transitions": "exact",
          "native_table_sha256": hashlib.sha256(json.dumps(native, separators=(",", ":")).encode()).hexdigest()}
args.out.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
