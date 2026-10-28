"""Run the current PNG correctness gates and retain command/source identities."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[4]
PNG = ROOT / "crates/deadlib-image/png"
EVIDENCE = PNG / "experiments/reconstruction-20261028"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "4"
    env["DEADLIB_LIBPNG_ENCODER"] = str(ROOT / "target/png-encoder-reference/Release/deadlib_png_encoder.exe")
    env["DEADLIB_LIBPNG_ORACLE"] = str(ROOT / "target/png-encoder-reference/Release/deadlib_png_oracle.exe")
    env["DEADLIB_PNG_CORPUS"] = str(ROOT / "target/deadlib-image-reference/kodak")
    for key in ["DEADLIB_LIBPNG_ENCODER", "DEADLIB_LIBPNG_ORACLE", "DEADLIB_PNG_CORPUS"]:
        assert Path(env[key]).exists()
    target = str(ROOT / "target/png-reconstruction-20261028/build")
    record = dict(source_sha256={p.relative_to(PNG).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))},
                  reference_binaries={key: dict(path=env[key], sha256=sha(Path(env[key])))
                                      for key in ["DEADLIB_LIBPNG_ENCODER", "DEADLIB_LIBPNG_ORACLE"]}, commands=[])
    commands = [
        ("release", ["cargo", "test", "--offline", "-p", "deadlib-image", "-p", "deadlib-image-png", "--release", "--target-dir", target, "--", "--include-ignored", "--nocapture"]),
        ("debug", ["cargo", "test", "--offline", "-p", "deadlib-image-png", "--target-dir", target, "--lib", "--test", "static_png", "--test", "compositing"]),
        ("clippy", ["cargo", "clippy", "--offline", "-p", "deadlib-image", "-p", "deadlib-image-png", "--all-targets", "--target-dir", target, "--", "-D", "warnings"]),
        ("fmt", ["cargo", "fmt", "-p", "deadlib-image", "-p", "deadlib-image-png", "--", "--check"]),
    ]
    for label, command in commands:
        path = EVIDENCE / f"{label}.log"
        assert not path.exists()
        print(f"Running {label}; output in {path}", flush=True)
        with path.open("wb") as stream:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
        record["commands"].append(dict(label=label, command=command, exit_code=result.returncode, log=path.name, sha256=sha(path)))
        (EVIDENCE / "validation.json").write_text(json.dumps(record, indent=2) + "\n")
        assert result.returncode == 0, path.read_text(errors="replace")[-6000:]
        print(f"{label} passed", flush=True)
    assert record["source_sha256"] == {p.relative_to(PNG).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))}


if __name__ == "__main__":
    main()
