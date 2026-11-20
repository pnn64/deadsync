import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path

work = Path(__file__).resolve().parent
checks = [
    ("final-tests.log", "cargo test --release -p deadlib-image-png", 79827),
    ("final-clippy.log", "cargo clippy -p deadlib-image-png --all-targets -- -D warnings", 79827),
    ("final-bench-clippy.log", "cargo clippy --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --bin encode_stages -- -D warnings", 79827),
    ("final-bench-alloc-clippy.log", "cargo clippy --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --features allocations -- -D warnings", 79827),
    ("final-fmt.log", "cargo fmt --package deadlib-image-png --check", 19756),
    ("final-bench-fmt.log", "cargo fmt --manifest-path crates/deadlib-image/png/bench/Cargo.toml --check", 19756),
    ("final-dependencies.log", "cargo tree -p deadlib-image-png --edges normal", 19756),
]
records = []
for name, command, session in checks:
    data = (work / name).read_bytes()
    records.append({"log": name, "command": command, "child_exit_code": 0,
                    "observed_terminal_session": session, "sha256": hashlib.sha256(data).hexdigest()})
results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", (work / "final-tests.log").read_text(encoding="utf-8"))
assert results and all(int(failed) == 0 for _, failed, _ in results)
totals = dict(zip(("passed", "failed", "ignored"), (sum(int(row[i]) for row in results) for i in range(3))))
record = {"created_utc": datetime.now(timezone.utc).isoformat(), "test_summaries": len(results),
          "tests": totals, "checks": records,
          "exit_code_evidence": "Child exits observed in capture.py terminal results. Session 79827 completed with exit 0. Session 19756 printed exit=0 for both format checks and the dependency check, then failed only while printing the captured Unicode dependency tree through cp1252. Those checks were not rerun. Remaining build/measurement work resumed with PYTHONIOENCODING=utf-8.",
          "ignored": "The 21 optional external-oracle tests were not run; regular Rust png and flate2 comparisons were run.",
          "scope": "No new unsafe production code. Final source correctness validation includes the six-byte-prefix scanner and its added 14-17-byte boundary cases."}
(work / "final-validation.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"test_summaries": len(results), "tests": totals, "checks": len(records)}))
