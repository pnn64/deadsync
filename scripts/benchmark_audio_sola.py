"""Archive and compare the actual SOLA source with identical release compiler flags.

Build each stage before editing again, then compare archived executables in
rotating order. No copied kernel implementation is used by the benchmark.
"""
import argparse
import csv
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/deadlib-audio/src/stream/stretch.rs"
ARCHIVES = ROOT / "target/audio-sola"


def build(name, source, test_source=None):
    archive = ARCHIVES / name
    archive.mkdir(parents=True, exist_ok=True)
    harness = archive / "harness.rs"
    snapshot = source.read_bytes()
    (archive / "stretch.rs").write_bytes(snapshot)
    if test_source is None:
        test_source = source.parent / "perf_tests-source.rs"
        if not test_source.exists():
            test_source = SOURCE.parent / "stretch/perf_tests.rs"
    tests = test_source.read_bytes()
    (archive / "perf_tests-source.rs").write_bytes(tests)
    rate_source = source.parent / "rate_bench.rs"
    if not rate_source.exists():
        rate_source = SOURCE.parent / "stretch/rate_bench.rs"
    rate_tests = rate_source.read_bytes()
    (archive / "rate_bench.rs").write_bytes(rate_tests)
    support_sources = {}
    for filename, live in [
        ("kernel_bench.rs", ROOT / "tests/support/kernel_bench.rs"),
        ("allocations-source.rs", ROOT / "tests/support/perf.rs"),
    ]:
        frozen = source.parent / filename
        data = (frozen if frozen.exists() else live).read_bytes()
        (archive / filename).write_bytes(data)
        support_sources[filename] = hashlib.sha256(data).hexdigest()
    relocated_tests = tests.decode("utf-8").replace(
        '#[path = "../../../../../tests/support/kernel_bench.rs"]',
        f'#[path = {json.dumps((archive / "kernel_bench.rs").as_posix())}]',
    )
    relocated_tests = relocated_tests.replace(
        '#[path = "rate_bench.rs"]',
        f'#[path = {json.dumps((archive / "rate_bench.rs").as_posix())}]',
    )
    with (archive / "compiled-tests.rs").open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(relocated_tests)
    relocated = snapshot.decode("utf-8").replace(
        '#[path = "stretch/perf_tests.rs"]',
        f'#[path = {json.dumps((archive / "compiled-tests.rs").as_posix())}]',
    )
    if '#[path = "stretch/perf_tests.rs"]' not in snapshot.decode("utf-8"):
        # A source exported from the parent commit predates the test include.
        relocated += (f'\n#[cfg(test)]\n#[path = '
                      f'{json.dumps((archive / "compiled-tests.rs").as_posix())}]\n'
                      'mod perf_tests;\n')
    with (archive / "compiled.rs").open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(relocated)
    support = archive / "allocations-source.rs"
    harness.write_text(
        '#![allow(dead_code)]\n'
        f'#[path = {json.dumps((archive / "compiled.rs").as_posix())}] mod stretch;\n'
        f'mod perf_tests {{ #[path = {json.dumps(support.as_posix())}] pub mod allocations; }}\n',
        encoding="utf-8",
    )
    command = ["rustc", "--test", "--edition=2024", "-C", "opt-level=3", "-C", "lto=fat",
               "-C", "codegen-units=1", "-C", "target-feature=+crt-static",
               str(harness), "-o", str(archive / "sola.exe")]
    subprocess.run(command, check=True, cwd=ROOT)
    result = subprocess.run([str(archive / "sola.exe"), "--test-threads=1"],
                            capture_output=True, text=True, check=True)
    (archive / "correctness.log").write_text(result.stdout, encoding="utf-8")
    manifest = dict(command=command, rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    source_sha256=hashlib.sha256(snapshot).hexdigest(),
                    test_source_sha256=hashlib.sha256(tests).hexdigest(),
                    rate_source_sha256=hashlib.sha256(rate_tests).hexdigest(),
                    support_sha256=support_sources,
                    binary_sha256=hashlib.sha256((archive / "sola.exe").read_bytes()).hexdigest())
    (archive / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(result.stdout, flush=True)


def compare(names, output, runs, benchmark):
    manifests = {name: json.loads((ARCHIVES / name / "manifest.json").read_text())
                 for name in names}
    support = [manifest.get("support_sha256") for manifest in manifests.values()]
    if not all(support) or any(value != support[0] for value in support):
        raise ValueError("Rebuild all stages with identical frozen benchmark support files")
    for field in ["test_source_sha256", "rate_source_sha256"]:
        if len({manifest[field] for manifest in manifests.values()}) != 1:
            raise ValueError("Rebuild all stages with identical benchmark fixtures")
    timing = re.compile(r"(sola/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn = re.compile(r"(sola/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    storage = re.compile(r"(sola/[^:\r\n]+): storage Churn \{ ([^}]+) \}")
    fingerprints = re.compile(r"(sola/[^:\r\n]+): fingerprint ([0-9a-f]+)")
    rows = []
    expected_hashes = None
    logs = ARCHIVES / "runs" / output.stem
    logs.mkdir(parents=True, exist_ok=True)
    for run in range(runs):
        order = names[run % len(names):] + names[:run % len(names)]
        for name in order:
            binary = ARCHIVES / name / "sola.exe"
            if hashlib.sha256(binary.read_bytes()).hexdigest() != manifests[name]["binary_sha256"]:
                raise ValueError(f"Archived executable changed: {name}")
            result = subprocess.run([str(binary), benchmark, "--ignored",
                                     "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True)
            (logs / f"{run}-{name}.log").write_text(result.stdout, encoding="utf-8")
            hashes = dict(fingerprints.findall(result.stdout))
            if expected_hashes is None:
                expected_hashes = hashes
            if len(hashes) != 6 or hashes != expected_hashes:
                raise ValueError(f"PCM fingerprint mismatch: {name}: {hashes}")
            counts = {label: dict(re.findall(r"(\w+): (\d+)", values))
                      for label, values in churn.findall(result.stdout)}
            retained = {label: dict(re.findall(r"(\w+): (\d+)", values))
                        for label, values in storage.findall(result.stdout)}
            measurements = timing.findall(result.stdout)
            expected = 19 if benchmark == "benchmark_sola_kernels" else 6
            if len(measurements) != expected or len(counts) != expected:
                raise ValueError(f"Expected {expected} workloads: {name}")
            for label, cpu, ns in measurements:
                rows.append(dict(run=run, stage=name, workload=label, cycles_per_op=float(cpu),
                                 ns_per_op=float(ns), fingerprint=hashes.get(label, ""),
                                 **counts[label], **{
                                     'storage_' + key: retained.get(label, {}).get(key, '')
                                     for key in counts[label]}))
            print(f"run {run + 1}/{runs}: {name}", flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows({key: value if value != "" else "NA" for key, value in row.items()}
                        for row in rows)
    metadata = dict(platform=platform.platform(), processor=os.environ.get("PROCESSOR_IDENTIFIER"),
                    benchmark=benchmark, runs=runs, stages=manifests)
    output.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    for label in sorted({row["workload"] for row in rows}):
        values = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == name and row["workload"] == label)
                  for name in names]
        print(f"{label}: {' -> '.join(f'{value:.2f}' for value in values)} cycles/op; "
              f"{values[0] / values[-1]:.3f}x CPU throughput")


def verify(names, output):
    expected = None
    records = []
    for name in names:
        archive = ARCHIVES / name
        manifest = json.loads((archive / "manifest.json").read_text())
        binary = archive / "sola.exe"
        if hashlib.sha256(binary.read_bytes()).hexdigest() != manifest["binary_sha256"]:
            raise ValueError(f"Archived executable changed: {name}")
        pcm = archive / "pcm.bin"
        result = subprocess.run([str(binary), "dump_sola_pcm",
                                 "--ignored", "--test-threads=1"], check=True,
                                capture_output=True, text=True,
                                env={**os.environ, "SOLA_PCM_OUT": str(pcm)})
        data = pcm.read_bytes()
        if expected is None:
            expected = data
        if data != expected:
            raise ValueError(f"Full PCM bytes or pull boundaries changed: {name}")
        records.append(dict(stage=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                            manifest=manifest,
                            capture_test=result.stdout))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(records, indent=2), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build")
    parser.add_argument("--source", type=Path, default=SOURCE)
    parser.add_argument("--test-source", type=Path)
    parser.add_argument("--compare", nargs="+")
    parser.add_argument("--verify", nargs="+")
    parser.add_argument("--benchmark", choices=["benchmark_sola_kernels", "benchmark_sola_rates",
                                              "benchmark_sola_packets", "benchmark_sola_first_match",
                                              "benchmark_sola_retire"],
                        default="benchmark_sola_kernels")
    parser.add_argument("--runs", type=int, default=4)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.build:
        build(args.build, args.source, args.test_source)
    if args.compare:
        if not args.output or args.runs < 1:
            parser.error("--compare requires --output and positive --runs")
        compare(args.compare, args.output, args.runs, args.benchmark)
    if args.verify:
        if not args.output:
            parser.error("--verify requires --output")
        verify(args.verify, args.output)


if __name__ == "__main__":
    main()
