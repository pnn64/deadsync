"""Final correctness gates for restored BMP production; no benchmark builds or timing."""
from datetime import datetime, timezone
import difflib
import hashlib
import json
from pathlib import Path
import re
import subprocess

task = Path(__file__).resolve().parent
root = task.parents[1]
logs = task / "final-restored-validation"
logs.mkdir()
record_path = logs / "commands.jsonl"
proof = json.loads((task / "restored-baseline-source.json").read_text(encoding="utf-8"))
records = []

def verify_sources(phase):
    checked = []
    for expected in proof["after_source_config"]:
        raw = (root / expected["path"]).read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        assert len(raw) == expected["bytes"] and digest == expected["sha256"], expected["path"]
        checked.append({"path": expected["path"], "bytes": len(raw), "sha256": digest})
    result = {"phase": phase, "captured_utc": datetime.now(timezone.utc).isoformat(), "sources": checked}
    with (logs / ("source-" + phase + ".json")).open("x", encoding="utf-8", newline="\n") as output:
        json.dump(result, output, indent=2)
        output.write("\n")

def run(name, arguments):
    print("START", name, flush=True)
    started = datetime.now(timezone.utc).isoformat()
    log = logs / (name + ".log")
    with log.open("xb") as output:
        process = subprocess.run([str(arg) for arg in arguments], cwd=root,
                                 stdout=output, stderr=subprocess.STDOUT)
    record = {"name": name, "arguments": [str(arg) for arg in arguments], "cwd": str(root),
              "started_utc": started, "completed_utc": datetime.now(timezone.utc).isoformat(),
              "exit_code": process.returncode, "log": str(log.relative_to(task))}
    records.append(record)
    with record_path.open("a", encoding="utf-8", newline="\n") as output:
        output.write(json.dumps(record) + "\n")
    print("DONE", name, "exit", process.returncode, flush=True)
    if process.returncode:
        raise SystemExit(process.returncode)

verify_sources("before")
run("format-check", ["rustfmt", "--check", "--edition", "2024",
    "crates/deadlib-image/bmp/src/decode.rs", "crates/deadlib-image/bmp/tests/bmp_optimized.rs",
    task / "validation/alpha_controls.rs"])
run("tests", ["cargo", "test", "--manifest-path", task / "validation/Cargo.toml", "--release",
    "--locked", "--offline", "-j", "2", "--target-dir", task / "check"])
run("clippy-codec", ["cargo", "clippy", "--manifest-path", task / "validation/Cargo.toml", "--release",
    "--locked", "--offline", "--all-targets", "-j", "2", "--target-dir", task / "check",
    "--", "-D", "warnings"])
verify_sources("after")
test_log = (logs / "tests.log").read_text(encoding="utf-8")
results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed;", test_log)
assert len(results) == 8 and sum(int(passed) for passed, failed in results) == 39
assert all(int(failed) == 0 for passed, failed in results)
relative = "crates/deadlib-image/bmp/tests/bmp_optimized.rs"
baseline = (task / "baseline/sources" / relative).read_text(encoding="utf-8").splitlines(keepends=True)
current = (root / relative).read_text(encoding="utf-8").splitlines(keepends=True)
patch = "".join(difflib.unified_diff(baseline, current, fromfile="a/" + relative, tofile="b/" + relative))
with (task / "retained-regression-tests.patch").open("x", encoding="utf-8", newline="\n") as output:
    output.write(patch)
summary = {
    "captured_utc": datetime.now(timezone.utc).isoformat(),
    "production_decision": "Original BMP production restored exactly; zero production optimizations retained.",
    "crate_integration_tests": 37, "independent_alpha_tests": 1, "doctests": 1,
    "formatting": "passed", "strict_codec_all_target_clippy": "passed",
    "commands": records, "source_config_files_verified_before_and_after": 22,
    "new_benchmark_builds": 0, "new_timed_benchmarks": 0, "new_snapshot_builds": 0,
    "baseline_snapshot_applicable": True,
    "baseline_snapshot_sha256": "076e78cae341e782b60f72fa3f60c0ff85142759e4821069f1012063f37bf0df",
    "rationale": "All production and harness/compiler inputs are exact baseline bytes; only public regression tests differ. Existing original baseline timing, allocation, and snapshot evidence applies.",
}
with (task / "final-restored-validation-summary.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(summary, output, indent=2)
    output.write("\n")
print("READY restored baseline production, final gates passed; no timing or benchmark rebuild", flush=True)
