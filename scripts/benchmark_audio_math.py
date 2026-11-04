"""Archive and measure audio gain, PCM and rate kernels on Windows."""
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
ARCHIVE = Path(os.environ.get("DEADSYNC_AUDIO_MATH_ARCHIVE", ROOT / "target/audio-math"))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(stage, runs, debug, baseline):
    out = ARCHIVE / stage
    work = ARCHIVE / "workspace"
    out.mkdir(parents=True, exist_ok=True)
    temporary = ARCHIVE / "tmp"
    temporary.mkdir(exist_ok=True)
    packages = ["deadlib-audio-core"]
    sources = [Path(__file__)]
    for package in packages:
        sources.extend((ROOT / "crates" / package).rglob("*.rs"))
        sources.append(ROOT / "crates" / package / "Cargo.toml")
    sources.extend(ROOT / name for name in ["tests/support/kernel_bench.rs", "tests/support/paired_bench.rs", "tests/support/perf.rs", "tests/perf/runtime_support.rs"])
    hashes = {}
    for source in sources:
        relative = source.relative_to(ROOT)
        contents = source.read_bytes()
        if baseline and relative.parts[0] == "crates" and relative.parts[2] == "src":
            contents = subprocess.check_output(["git", "-C", str(ROOT), "show", f"{baseline}:{relative.as_posix()}"])
        for destination in [work / relative, out / "source" / relative]:
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
        hashes[str(relative)] = hashlib.sha256(contents).hexdigest()
    manifest = '[workspace]\nmembers = ' + json.dumps([f"crates/{p}" for p in packages]) + '''
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
    (work / "Cargo.toml").write_text(manifest, encoding="utf-8")
    env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static", TEMP=str(temporary), TMP=str(temporary))
    def execute(args, filename, extra=None):
        result = subprocess.run(args, cwd=ROOT, env=env if extra is None else dict(env, **extra), capture_output=True, text=True, encoding="utf-8")
        (out / filename).write_text(result.stdout + result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result.stdout
    command = ["cargo", "test", "--offline", "--manifest-path", str(work / "Cargo.toml")]
    if not debug:
        command.append("--release")
    execute(command + ["--", "--test-threads=1"], "correctness.log")
    output = execute(command + ["-p", "deadlib-audio-core", "--test", "callback_kernels", "--no-run", "--message-format=json"], "build.log")
    artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    binary = next(Path(a["executable"]) for a in artifacts if a.get("executable"))
    saved = out / binary.name
    shutil.copyfile(binary, saved)
    trace_output = execute(command + ["-p", "deadlib-audio-core", "--test", "rendering_kernels", "--no-run", "--message-format=json"], "trace-build.log")
    artifacts = [json.loads(line) for line in trace_output.splitlines() if line.startswith("{")]
    trace_binary = next(Path(a["executable"]) for a in artifacts if a.get("executable"))
    trace_saved = out / trace_binary.name
    shutil.copyfile(trace_binary, trace_saved)
    trace = out / "render-trace.bin"
    execute([str(trace_saved), "dump_rendering_trace", "--exact", "--ignored", "--nocapture", "--test-threads=1"], "trace.log", {"AUDIO_CORE_TRACE_OUT": str(trace)})
    reference_trace = ARCHIVE / "baseline" / "render-trace.bin"
    if stage != "baseline" and reference_trace.exists():
        assert trace.read_bytes() == reference_trace.read_bytes(), "Complete render trace differs from baseline"
    metadata = dict(stage=stage, command=command, workspace_manifest=manifest, baseline_revision=baseline,
                    rustflags=env["RUSTFLAGS"], rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    cpu=os.environ.get("PROCESSOR_IDENTIFIER"), sources=hashes, binary_sha256=sha(saved), runs=runs,
                    lockfile=(work / "Cargo.lock").read_text(), correctness_sha256=sha(out / "correctness.log"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", (out / "correctness.log").read_text()))))
    metadata.update(trace_sha256=sha(trace), trace_bytes=trace.stat().st_size, trace_binary_sha256=sha(trace_saved))
    report = ROOT / f"docs/performance/2026-11-04-audio-math-{stage}"
    report.parent.mkdir(parents=True, exist_ok=True)
    report.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if debug:
        print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)
        return
    rows = []
    for number in range(runs):
        output = execute([str(saved), "benchmark_audio_math", "--ignored", "--nocapture", "--test-threads=1"], f"run-{number}.log")
        pairs = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", output)
        assert len(pairs) == 9, output
        churn = {label: dict(re.findall(r"(\w+): (\d+)", fields)) for label, fields in
                 re.findall(r"([\w/]+): allocation Churn \{ ([^}]+) \}", output)}
        for label, old_cpu, new_cpu, old_ns, new_ns in pairs:
            for variant, cpu, ns in [("original", old_cpu, old_ns), ("current", new_cpu, new_ns)]:
                rows.append(dict(run=number, workload=label, variant=variant, cycles_per_batch=cpu, ns_per_batch=ns,
                                 **{key: int(value) for key, value in churn[label + "/" + variant].items()}))
        print(f"{stage}: run {number + 1}/{runs}", flush=True)
    with report.with_suffix(".tsv").open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({r["workload"] for r in rows}):
        a, b = [statistics.median(float(r["cycles_per_batch"]) for r in rows if r["workload"] == label and r["variant"] == variant) for variant in ["original", "current"]]
        print(f"{label}: {a:.2f} -> {b:.2f} cycles/batch, {a/b:.3f}x CPU throughput", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("stage")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--baseline")
    args = parser.parse_args()
    run(args.stage, args.runs, args.debug, args.baseline)
