"""Build immutable pixel checkpoints, replay controls, and summarize CPU medians."""
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
PNG = ROOT / "crates/deadlib-image/png"
WORK = ROOT / "target/png-pixels-20261028"
EVIDENCE = PNG / "experiments/pixels-20261028"
BASE = "22d57bf31"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, record):
    path.write_bytes((json.dumps(record, indent=2) + "\n").encode())


def sources():
    paths = sorted((PNG / "src").rglob("*.rs")) + [PNG.parent / "src/lib.rs"]
    return {p.relative_to(ROOT).as_posix(): sha(p) for p in paths}


def summarize():
    rows = []
    expected = None
    for label in ["baseline", "rgba", "palette", "opaque"]:
        samples = {}
        for mode in ["run", "control"]:
            for run in range(1, 4):
                path = EVIDENCE / f"{label}-{mode}{run}.tsv"
                for row in csv.DictReader(io.StringIO(path.read_text()), delimiter="\t"):
                    key = row["case"], row["operation"]
                    samples.setdefault(key, []).append(row)
        fingerprints = {key: group[0]["fingerprint"] for key, group in samples.items()}
        if expected is None:
            expected = fingerprints
        assert fingerprints == expected
        for (case, operation), group in samples.items():
            assert len(group) == 6
            for field in ["fingerprint", "allocations", "reallocations", "requested_bytes"]:
                assert len({row[field] for row in group}) == 1
            cycles = [float(row["cycles_per_image"]) for row in group]
            rows.append(dict(checkpoint=label, case=case, operation=operation, processes=6,
                             cycles_per_image=statistics.median(cycles), min_cycles=min(cycles), max_cycles=max(cycles),
                             MiB_per_second=statistics.median(float(row["MiB_per_second"]) for row in group),
                             **{field: group[0][field] for field in ["allocations", "reallocations", "requested_bytes", "fingerprint"]}))
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    (EVIDENCE / "summary.tsv").write_bytes(stream.getvalue().encode())
    print(f"Summarized {len(expected)} cases; every checkpoint agrees with independent known answers.")


def main():
    mode = sys.argv[1]
    if mode == "summary":
        summarize()
        return
    label = sys.argv[2]
    assert mode in ["build", "control"] and label.replace("-", "").isalnum()
    EVIDENCE.mkdir(exist_ok=True)
    name = label if mode == "build" else label + "-control"
    manifest = EVIDENCE / f"{name}.json"
    assert not manifest.exists()
    for run in range(1, 4):
        assert not (EVIDENCE / f"{label}-{'run' if mode == 'build' else 'control'}{run}.tsv").exists()
    env = dict(os.environ, CARGO_BUILD_JOBS="4", DEADLIB_PNG_BENCH_ITERS="12")
    if mode == "build":
        destination = WORK / label
        destination.mkdir(parents=True, exist_ok=False)
        before = sources()
        command = ["cargo", "build", "--offline", "-p", "deadlib-image-png", "--profile", "bench",
                   "--example", "reconstruction_bench", "--target-dir", str(WORK / "build"), "--message-format", "json"]
        result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True)
        build_log = EVIDENCE / f"{label}-build.log"
        assert not build_log.exists()
        build_log.write_bytes(result.stdout + result.stderr)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        artifacts = [json.loads(line) for line in result.stdout.splitlines()]
        binary = Path(next(a["executable"] for a in reversed(artifacts)
                           if a.get("reason") == "compiler-artifact" and a.get("executable")))
        saved = destination / binary.name
        shutil.copy2(binary, saved)
        assert before == sources()
        bench_paths = [PNG / "examples/reconstruction_bench.rs", PNG / "examples/support/cpu.rs"]
        bench_paths += [PNG / "tests/support" / (n + ".rs") for n in ["compositor", "apng_recovery", "chunks"]]
        patch = EVIDENCE / f"{label}.patch"
        patch.write_bytes(subprocess.check_output(["git", "diff", BASE, "--", "crates/deadlib-image/png/src"], cwd=ROOT))
        record = dict(label=label, commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                      source_sha256=before, benchmark_sha256={p.relative_to(ROOT).as_posix(): sha(p) for p in bench_paths},
                      executable=str(saved), executable_sha256=sha(saved), patch_sha256=sha(patch),
                      command=command, build_log_sha256=sha(build_log), driver_sha256=sha(Path(__file__)),
                      rustc=subprocess.check_output(["rustc", "--version"], text=True).strip(), platform=platform.platform(),
                      workspace_manifest_sha256=sha(ROOT / "Cargo.toml"), lock_sha256=sha(ROOT / "Cargo.lock"),
                      scope="CPU 2; Windows thread cycles; 3 warmups, 7 batches per process; 12 iterations, minimum 2048 for outputs below 4096 bytes; separate untimed allocation probe; independent rational/palette oracle outside timing; shared host and frequency uncontrolled.", outputs=[])
    else:
        original = EVIDENCE / f"{label}.json"
        record = json.loads(original.read_text())
        saved = Path(record["executable"])
        assert sha(saved) == record["executable_sha256"]
        record = dict(record, control_of=original.name, control_of_sha256=sha(original), outputs=[])
    save(manifest, record)
    for run in range(1, 4):
        result = subprocess.run([str(saved)], cwd=ROOT, env=env, capture_output=True)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        path = EVIDENCE / f"{label}-{'run' if mode == 'build' else 'control'}{run}.tsv"
        path.write_bytes(result.stdout.replace(b"\r\n", b"\n"))
        record["outputs"].append(dict(path=path.name, sha256=sha(path), stderr=result.stderr.decode()))
        save(manifest, record)
        print(f"{label} {mode} process {run} passed", flush=True)


if __name__ == "__main__":
    main()
