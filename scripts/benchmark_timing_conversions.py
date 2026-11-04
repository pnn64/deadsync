"""Build exact core/rules sources in a small workspace and archive paired CPU runs."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = Path(os.environ.get("DEADSYNC_TIMING_ARCHIVE", ROOT / "target/timing-conversions"))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(stage, runs, debug):
    workspace = ARCHIVE / "workspace"
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    sources = []
    for name in ["deadsync-core", "deadsync-rules"]:
        sources.extend((ROOT / "crates" / name).rglob("*.rs"))
        sources.append(ROOT / "crates" / name / "Cargo.toml")
    sources.extend((ROOT / "tests/support").glob("*.rs"))
    sources.extend((ROOT / "tests/perf").glob("*.rs"))
    sources.append(Path(__file__))
    hashes = {}
    for source in sources:
        relative = source.relative_to(ROOT)
        for destination in [workspace / relative, out / "source" / relative]:
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or sha(source) != sha(destination):
                shutil.copyfile(source, destination)
        hashes[str(relative)] = sha(source)
    manifest = '''[workspace]
members = ["crates/deadsync-core", "crates/deadsync-rules"]
resolver = "3"
[profile.release]
opt-level = 3
lto = true
debug = false
incremental = false
[profile.dev]
debug = false
incremental = false
'''
    (workspace / "Cargo.toml").write_text(manifest, encoding="utf-8")
    env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static")
    command = ["cargo", "test", "--offline", "--manifest-path", str(workspace / "Cargo.toml")]
    if not debug:
        command.append("--release")
    def execute(args, name):
        result = subprocess.run(args, cwd=ROOT, env=env, capture_output=True,
                                text=True, encoding="utf-8")
        (out / name).write_text(result.stdout + result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result.stdout
    # Run every core/rules unit, integration and doc test in each stage.
    execute(command + ["--", "--test-threads=1"], "correctness.log")
    built = execute(command + ["-p", "deadsync-rules", "--lib", "--no-run", "--message-format=json"], "build.log")
    artifacts = [json.loads(line) for line in built.splitlines() if line.startswith("{")]
    binary = next(Path(a["executable"]) for a in artifacts if a.get("executable"))
    saved = out / binary.name
    shutil.copyfile(binary, saved)
    metadata = dict(stage=stage, command=command, workspace_manifest=manifest,
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    cpu=os.environ.get("PROCESSOR_IDENTIFIER"),
                    binary_sha256=sha(saved), sources=hashes,
                    script_sha256=sha(Path(__file__)), runs=runs,
                    allocations=("Not measured in debug; assertions run in release benchmarks" if debug else
                                 "Zero allocs/reallocs/frees/bytes, asserted for both variants of all six workloads"))
    metadata["correctness_sha256"] = sha(out / "correctness.log")
    metadata["lockfile"] = (workspace / "Cargo.lock").read_text(encoding="utf-8")
    metadata["passed_tests"] = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", (out / "correctness.log").read_text(encoding="utf-8"))))
    report = ROOT / f"docs/performance/2026-11-04-timing-conversions-{stage}"
    report.parent.mkdir(parents=True, exist_ok=True)
    report.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if debug:
        print(f"{stage}: debug suite passed", flush=True)
        return
    rows = []
    for number in range(runs):
        output = execute([str(saved), "benchmark_timing_conversions", "--ignored", "--nocapture", "--test-threads=1"], f"run-{number}.log")
        pairs = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", output)
        assert len(pairs) == 6, output
        for label, old_cpu, new_cpu, old_ns, new_ns in pairs:
            for variant, cpu, ns in [("original", old_cpu, old_ns), ("current", new_cpu, new_ns)]:
                rows.append(dict(run=number, workload=label, variant=variant, cycles_per_batch=cpu, ns_per_batch=ns, allocations=0))
        print(f"{stage}: run {number + 1}/{runs}", flush=True)
    with report.with_suffix(".tsv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({r["workload"] for r in rows}):
        medians = [statistics.median(float(r["cycles_per_batch"]) for r in rows if r["workload"] == label and r["variant"] == variant) for variant in ["original", "current"]]
        print(f"{label}: {medians[0]:.2f} -> {medians[1]:.2f} cycles/256, {medians[0] / medians[1]:.3f}x CPU throughput", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("stage")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    run(args.stage, args.runs, args.debug)
