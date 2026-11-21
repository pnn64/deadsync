"""Prove the reduced production source is baseline plus only two frozen changes."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys

task = Path(__file__).resolve().parent
root = task.parents[1]
label = sys.argv[1]
assert label == "candidate5-two-layouts"
relative = Path("crates/deadlib-image/bmp/src/decode.rs")
baseline = (task / "baseline/sources" / relative).read_text(encoding="utf-8")
rgba4444 = (task / "candidate2-rgba4444/sources" / relative).read_text(encoding="utf-8")
direct = (task / "candidate3-direct-rgba/sources" / relative).read_text(encoding="utf-8")
current = (root / relative).read_text(encoding="utf-8")

# Reuse only each independently reviewed target section from its frozen source.
arm = "        (16, Some(alpha)) =>"
end = "        (32, None) =>"
old = baseline[baseline.index(arm):baseline.index(end, baseline.index(arm))]
new = rgba4444[rgba4444.index(arm):rgba4444.index(end, rgba4444.index(arm))]
assert baseline.count(old) == 1
expected = baseline.replace(old, new)

prefix = "        Format::Masked {\n            red,\n            green,\n            blue,\n            alpha: Some(alpha),\n            opaque_if_zero: false,"
next_arm = "        Format::Rgb24 | Format::Masked { .. } => {"
copy_arm = direct[direct.index(prefix):direct.index(next_arm, direct.index(prefix))]
assert expected.count(next_arm) == 1
expected = expected.replace(next_arm, copy_arm + next_arm)

old_copy = """            [0xff, 0xff00, 0xff0000, 0xff000000] => {
                output.copy_from_slice(&row[..output.len()]);
            }
"""
assert expected.count(old_copy) == 1
expected = expected.replace(old_copy, "")
assert current == expected, "Unexpected production difference beyond the two exact-layout transformations"

identity = json.loads((task / "baseline/identity.json").read_text(encoding="utf-8"))
other_sources = []
for entry in identity["sources"]:
    path = entry["path"]
    if path.startswith("crates/deadlib-image/bmp/src/") and path != relative.as_posix():
        raw = (root / path).read_bytes()
        assert len(raw) == entry["bytes"] and hashlib.sha256(raw).hexdigest() == entry["sha256"], path
        other_sources.append(path)
result = {
    "captured_utc": datetime.now(timezone.utc).isoformat(),
    "stage": label,
    "production_base": "baseline",
    "retained_origins": ["candidate2-rgba4444", "candidate3-direct-rgba"],
    "discarded_transformations": ["source alpha prescan", "logical alpha prescan", "opaque-alpha dispatch relocation"],
    "exact_text_equivalence": True,
    "decode_sha256": hashlib.sha256((root / relative).read_bytes()).hexdigest(),
    "other_production_sources_unchanged": other_sources,
    "regression_tests": "All three useful added regression tests are retained; no new performance path.",
}
with (task / (label + "-source-proof.json")).open("x", encoding="utf-8", newline="\n") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
print(json.dumps(result, indent=2))
