"""Freeze and validate one BMP source stage; never run timed benchmarks."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys

TASK = Path(__file__).resolve().parent
ROOT = TASK.parents[1]
LABEL = "candidate4-alpha-dispatch"
PARENT = "candidate3-direct-rgba"
STAGE = TASK / LABEL
LOGS = TASK / (LABEL + "-validation")
assert not STAGE.exists(), "Stage already exists; inspect completed work instead of replaying"
LOGS.mkdir(exist_ok=True)
record_path = LOGS / "commands.jsonl"
if record_path.exists():
    assert record_path.read_bytes() == b"", "Never replay a completed native command"
records = record_path.open("a", encoding="utf-8", newline="\n")


def run(name, arguments):
    print("START", name, flush=True)
    started = datetime.now(timezone.utc).isoformat()
    log = LOGS / (name + ".log")
    with log.open("xb") as output:
        process = subprocess.run([str(argument) for argument in arguments], cwd=ROOT,
                                 stdout=output, stderr=subprocess.STDOUT)
    record = {"name": name, "arguments": [str(arg) for arg in arguments],
              "cwd": str(ROOT), "started_utc": started,
              "completed_utc": datetime.now(timezone.utc).isoformat(),
              "exit_code": process.returncode, "log": str(log.relative_to(TASK))}
    records.write(json.dumps(record) + "\n")
    records.flush()
    print("DONE", name, "exit", process.returncode, flush=True)
    if process.returncode:
        raise SystemExit(process.returncode)


def freeze(source, name):
    data = source.read_bytes()
    with (STAGE / name).open("xb") as output:
        output.write(data)
    return {"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


# Preserve the matching parent symbols while the shared target still contains
# its already frozen timing executable. These debugging artifacts are not timed.
parent_binary = TASK / "timing/release/bmp-codec-bench.exe"
parent_executables = json.loads((TASK / PARENT / "executables.json").read_text(encoding="utf-8"))
parent_hash = next(item["sha256"] for item in parent_executables if item["path"] == "bmp-timing.exe")
assert hashlib.sha256(parent_binary.read_bytes()).hexdigest() == parent_hash
parent_symbols = TASK / "assembly-candidate3"
parent_symbols.mkdir(exist_ok=True)
symbol_files = []
for name in ("bmp-codec-bench.exe", "bmp_codec_bench.pdb"):
    data = (TASK / "timing/release" / name).read_bytes()
    destination = parent_symbols / name
    if destination.exists():
        assert destination.read_bytes() == data, "Previously preserved parent artifact changed"
    else:
        with destination.open("xb") as output:
            output.write(data)
    symbol_files.append({"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
with (parent_symbols / "identity.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump({"stage": PARENT, "executable_matches_frozen_parent": True,
               "captured_utc": datetime.now(timezone.utc).isoformat(), "files": symbol_files}, output, indent=2)
    output.write("\n")

run("format", ["rustfmt", "--edition", "2024", "crates/deadlib-image/bmp/src/decode.rs"])
run("freeze-source", [sys.executable, TASK / "freeze-stage.py", LABEL, PARENT])
run("source-before-build", [sys.executable, TASK / "verify-stage.py", LABEL, "before-build"])
run("format-check", ["rustfmt", "--check", "--edition", "2024",
    "crates/deadlib-image/bmp/src/decode.rs", "crates/deadlib-image/bmp/tests/bmp_optimized.rs",
    TASK / "validation/alpha_controls.rs"])

bench = "crates/deadlib-image/bmp/bench/Cargo.toml"
run("build-timing", ["cargo", "build", "--manifest-path", bench, "--release", "--locked", "--offline",
    "--bins", "-j", "2", "--target-dir", TASK / "timing"])
executables = [freeze(TASK / "timing/release/bmp-codec-bench.exe", "bmp-timing.exe"),
               freeze(TASK / "timing/release/native_snapshot.exe", "native_snapshot.exe")]
debug_symbol = freeze(TASK / "timing/release/bmp_codec_bench.pdb", "bmp_codec_bench.pdb")
with (STAGE / "debug-symbols.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(debug_symbol, output, indent=2)
    output.write("\n")
run("build-allocations", ["cargo", "build", "--manifest-path", bench, "--release", "--locked", "--offline",
    "--features", "allocations", "--bin", "bmp-codec-bench", "-j", "2", "--target-dir", TASK / "allocations"])
executables.append(freeze(TASK / "allocations/release/bmp-codec-bench.exe", "bmp-allocations.exe"))
with (STAGE / "executables.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(executables, output, indent=2)
    output.write("\n")

run("snapshot", [STAGE / "native_snapshot.exe", "verify", TASK / "baseline/native.snapshot"])
run("tests", ["cargo", "test", "--manifest-path", TASK / "validation/Cargo.toml", "--release", "--locked",
    "--offline", "-j", "2", "--target-dir", TASK / "check"])
run("clippy-codec", ["cargo", "clippy", "--manifest-path", TASK / "validation/Cargo.toml", "--release",
    "--locked", "--offline", "--all-targets", "-j", "2", "--target-dir", TASK / "check", "--", "-D", "warnings"])
run("clippy-bench", ["cargo", "clippy", "--manifest-path", bench, "--release", "--locked", "--offline",
    "--all-targets", "--all-features", "-j", "2", "--target-dir", TASK / "allocations", "--", "-D", "warnings"])

for suffix, mode, extra in (
    ("decode", "decode", []), ("owned-rgba", "owned-rgba", []), ("rgba", "rgba", []),
    ("corpus", "owned-rgba", ["-Corpus"]),
    ("alpha-controls-decode", "decode", ["-Files", "target/bmp-decode-opt-20261120-pass2/alpha-controls"]),
    ("alpha-controls-owned-rgba", "owned-rgba", ["-Files", "target/bmp-decode-opt-20261120-pass2/alpha-controls"]),
):
    run("allocations-" + suffix, ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        TASK / "measure.ps1", "-Executable", STAGE / "bmp-allocations.exe", "-Label", LABEL + "-" + suffix + "-allocations",
        "-Mode", mode, "-Allocations", *extra])
run("source-after-build", [sys.executable, TASK / "verify-stage.py", LABEL, "after-build"])
run("allocation-and-identity-check", [sys.executable, TASK / "check-candidate1.py", LABEL])
records.close()
summary = {"stage": LABEL, "parent": PARENT, "status": "build, tests, strict Clippy, snapshot and allocation checks completed",
           "timing_status": "not run", "capture": "each native command stdout and stderr merged directly into its log; native return code checked",
           "commands": [json.loads(line) for line in (LOGS / "commands.jsonl").read_text(encoding="utf-8").splitlines()],
           "executables": executables}
with (TASK / (LABEL + "-validation-summary.json")).open("x", encoding="utf-8", newline="\n") as output:
    json.dump(summary, output, indent=2)
    output.write("\n")
print("READY", LABEL, "no timings run", flush=True)
