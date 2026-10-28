"""Build, preserve and measure one reconstruction checkpoint on Windows."""
import csv
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
PNG = ROOT / "crates/deadlib-image/png"
WORK = ROOT / "target/png-reconstruction-20261028"
EVIDENCE = PNG / "experiments/reconstruction-20261028"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    label = sys.argv[1]
    assert label.replace("-", "").isalnum()
    EVIDENCE.mkdir(exist_ok=True)
    destination = WORK / label
    destination.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "4"
    command = ["cargo", "build", "--offline", "-p", "deadlib-image-png", "--profile", "bench",
               "--example", "reconstruction_bench", "--target-dir", str(WORK / "build"), "--message-format", "json"]
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True)
    (EVIDENCE / f"{label}-build.log").write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    artifacts = [json.loads(line) for line in result.stdout.splitlines()]
    binary = Path(next(a["executable"] for a in reversed(artifacts)
                       if a.get("reason") == "compiler-artifact" and a.get("executable")))
    saved = destination / binary.name
    shutil.copy2(binary, saved)
    record = dict(label=label, command=command, commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                  rustc=subprocess.check_output(["rustc", "--version"], text=True).strip(),
                  executable=str(saved), executable_sha256=sha(saved),
                  source_sha256={p.relative_to(PNG).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))},
                  benchmark_sha256=sha(PNG / "examples/reconstruction_bench.rs"),
                  cpu_helper_sha256=sha(ROOT / "tests/support/kernel_bench.rs"),
                  shared_image_sha256=sha(PNG.parent / "src/lib.rs"),
                  workspace_manifest_sha256=sha(ROOT / "Cargo.toml"), workspace_lock_sha256=sha(ROOT / "Cargo.lock"),
                  driver_sha256=sha(Path(__file__)),
                  scope="7 batches per process, 3 warmups, current thread pinned to CPU 2; Windows QueryThreadCycleTime. Allocation counters enabled only in a separate untimed operation. Shared host and frequency uncontrolled.")
    outputs = []
    for run in range(1, 4):
        result = subprocess.run([str(saved)], cwd=ROOT, env=env, capture_output=True)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        path = EVIDENCE / f"{label}-run{run}.tsv"
        assert not path.exists()
        path.write_bytes(result.stdout.replace(b"\r\n", b"\n"))
        outputs.append(dict(path=path.name, sha256=sha(path), stderr=result.stderr.decode()))
        print(f"{label} run {run} complete", flush=True)
    record["outputs"] = outputs
    (EVIDENCE / f"{label}.json").write_text(json.dumps(record, indent=2) + "\n")
    rows = list(csv.DictReader((EVIDENCE / f"{label}-run2.tsv").open(), delimiter="\t"))
    print(json.dumps([{k: r[k] for k in ("case", "operation", "cycles_per_image", "allocations", "requested_bytes")} for r in rows], indent=2))


if __name__ == "__main__":
    main()
