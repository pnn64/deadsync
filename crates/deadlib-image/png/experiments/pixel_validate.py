"""Retain focused/final pixel correctness commands, hashes and output."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
PNG = ROOT / "crates/deadlib-image/png"
SERIES = os.environ.get("DEADLIB_PNG_PERF_SERIES", "pixels-20261028")
assert SERIES.replace("-", "").isalnum()
EVIDENCE = PNG / "experiments" / SERIES
REFERENCE = Path("C:/GitHub/deadsync/target/png-encoder-reference/Release/deadlib_png_oracle.exe")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    label = sys.argv[1]
    assert label.replace("-", "").isalnum()
    assert REFERENCE.is_file()
    env = dict(os.environ, CARGO_BUILD_JOBS="4", DEADLIB_LIBPNG_ORACLE=str(REFERENCE))
    target = str(ROOT / "target" / ("png-" + SERIES) / "build")
    tests = sys.argv[2:] or ["compositing", "compositor_precision"]
    encoder = REFERENCE.with_name("deadlib_png_encoder.exe")
    if "encoder_reference" in tests:
        assert encoder.is_file()
        env["DEADLIB_LIBPNG_ENCODER"] = str(encoder)
    if label in ["final", "selected"] and len(sys.argv) == 2:
        tests += ["apng_rendered"]
    command = ["cargo", "test", "--offline", "-p", "deadlib-image-png", "--release", "--target-dir", target]
    for test in tests:
        command += ["--test", test]
    command += ["--", "--include-ignored", "--nocapture"]
    commands = [(label + "-tests", command)]
    if label in ["final", "selected"] or label.endswith("-final"):
        prefix = "selected-" if label == "selected" else "" if label == "final" else label + "-"
        debug = ["cargo", "test", "--offline", "-p", "deadlib-image-png", "--target-dir", target]
        for test in tests:
            if test != "apng_rendered":
                debug += ["--test", test]
        debug += ["--", "--nocapture"]
        commands += [
            (prefix + "clippy", ["cargo", "clippy", "--offline", "-p", "deadlib-image-png", "--all-targets", "--target-dir", target, "--", "-D", "warnings"]),
            (prefix + "fmt", ["cargo", "fmt", "-p", "deadlib-image-png", "--", "--check"]),
            (prefix + "debug", debug),
        ]
    record = dict(source_sha256={p.relative_to(PNG).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))},
                  tests_sha256={p: sha(PNG / "tests" / (p + ".rs")) for p in tests},
                  oracle=dict(path=str(REFERENCE), sha256=sha(REFERENCE)), commands=[])
    if "encoder_reference" in tests:
        record["encoder_oracle"] = dict(path=str(encoder), sha256=sha(encoder))
    manifest = EVIDENCE / f"{label}-validation.json"
    assert not manifest.exists()
    for name, command in commands:
        log = EVIDENCE / f"{name}.log"
        assert not log.exists()
        print(f"Running {name}", flush=True)
        with log.open("wb") as stream:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
        record["commands"].append(dict(command=command, exit_code=result.returncode, log=log.name, sha256=sha(log)))
        manifest.write_bytes((json.dumps(record, indent=2) + "\n").encode())
        assert result.returncode == 0, log.read_text(errors="replace")[-6000:]
        print(f"{name} passed", flush=True)
    assert record["source_sha256"] == {p.relative_to(PNG).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))}


if __name__ == "__main__":
    main()
