"""Archive production partner-chart projection kernels and paired CPU measurements."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import posixpath
import re
import shutil
import statistics
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = Path(os.environ.get("DEADSYNC_PARTNER_ARCHIVE", ROOT / "target/partner-charts"))
PACKAGE = Path("crates/deadsync-simfile")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def full_tests(stage, debug):
    """Build the actual simfile crate, including existing song/cache integration tests."""
    out = ARCHIVE / stage
    work = ARCHIVE / "full-workspace"
    out.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)
    packages = set()

    def visit(package):
        if package in packages:
            return
        packages.add(package)
        data = tomllib.loads(subprocess.check_output(["git", "-C", str(ROOT), "show", f"HEAD:{package}/Cargo.toml"]).decode())

        def tables(table):
            for key, value in table.items():
                if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                    for dependency in value.values():
                        if isinstance(dependency, dict) and "path" in dependency:
                            visit(posixpath.normpath(package + "/" + dependency["path"]))
                elif isinstance(value, dict):
                    tables(value)
        tables(data)

    visit(PACKAGE.as_posix())
    revision = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
    previous = ROOT / "docs/performance/2026-11-04-partner-charts-full-release.manifest.json"
    reuse = ((work / "Cargo.toml").exists() and previous.exists()
             and json.loads(previous.read_text())["git_revision"] == revision)
    archive = ARCHIVE / "full-release/source.tar" if reuse else out / "source.tar"
    if not reuse:
        with archive.open("wb") as stream:
            subprocess.run(["git", "-C", str(ROOT), "archive", "HEAD", *sorted(packages), "tests", "vendor", "Cargo.lock"],
                           stdout=stream, check=True)
        with tarfile.open(archive) as source:
            source.extractall(work, filter="data")
    overlays = {}
    sources = [PACKAGE / "src/partner_charts.rs"]
    sources += sorted(path.relative_to(ROOT) for path in (ROOT / PACKAGE / "src/partner_charts").glob("*.rs"))
    for relative in sources:
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        contents = (ROOT / relative).read_bytes()
        if not destination.exists() or destination.read_bytes() != contents:
            destination.write_bytes(contents)
        copy = out / "source" / relative
        copy.parent.mkdir(parents=True, exist_ok=True)
        copy.write_bytes(destination.read_bytes())
        overlays[relative.as_posix()] = sha(destination)
    root_manifest = subprocess.check_output(["git", "-C", str(ROOT), "show", "HEAD:Cargo.toml"]).decode()
    patches = root_manifest.split("[patch.crates-io]", 1)[1].split("[build-dependencies]", 1)[0]
    manifest = '''[workspace]
members = ["crates/deadsync-simfile"]
resolver = "3"
[workspace.dependencies]
serde_json = "1.0.154"
windows-core = "0.100.0"
[profile.release]
opt-level = 3
lto = true
debug = false
incremental = false
[profile.dev]
debug = false
incremental = false
[patch.crates-io]
''' + patches
    (work / "Cargo.toml").write_text(manifest, encoding="utf-8")
    # Existing directory-junction fixtures require the host's NTFS temp volume.
    env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static")
    command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-simfile"]
    if not debug:
        command.append("--release")
    command += ["--", "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as log:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, text=True)
    log = (out / "correctness.log").read_text(encoding="utf-8")
    metadata = dict(command=command, workspace_manifest=manifest, rustflags=env["RUSTFLAGS"],
                    git_revision=revision, rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    test_temp=env.get("TEMP"), test_tmp=env.get("TMP"),
                    source_archive=str(archive), source_archive_sha256=sha(archive), overlays=overlays, correctness_sha256=sha(out / "correctness.log"),
                    exit_code=result.returncode, lockfile=(work / "Cargo.lock").read_text(),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-04-partner-charts-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if result.returncode:
        raise RuntimeError(log[-12000:])
    print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)


def run(stage, runs, debug, baseline, *, report_prefix="partner-charts",
        benchmark="benchmark_partner_charts", extra_source=None):
    out = ARCHIVE / stage
    work = ARCHIVE / "workspace"
    out.mkdir(parents=True, exist_ok=True)
    temporary = ARCHIVE / "tmp"
    temporary.mkdir(exist_ok=True)
    paths = [PACKAGE / "src/partner_charts.rs", PACKAGE / "src/metadata_perf.rs",
             Path("tests/support/kernel_bench.rs"), Path(__file__).relative_to(ROOT)]
    paths += sorted(path.relative_to(ROOT) for path in (ROOT / PACKAGE / "src/partner_charts").glob("*.rs"))
    if extra_source:
        paths.append(extra_source.relative_to(ROOT))
    hashes = {}
    for relative in paths:
        contents = (ROOT / relative).read_bytes()
        if baseline and relative == PACKAGE / "src/partner_charts.rs":
            contents = subprocess.check_output(["git", "-C", str(ROOT), "show", f"{baseline}:{relative.as_posix()}"])
        for destination in [work / relative, out / "source" / relative]:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    # Compile the complete, verbatim production prefix with the real RSSP dependency.
    # Existing crate-wide tests require config/audio/UI; these tests exercise projection directly.
    source = (work / PACKAGE / "src/partner_charts.rs").read_text(encoding="utf-8")
    prefix = source.split("#[cfg(test)]", 1)[0]
    generated = prefix + '\n#[cfg(test)]\n#[path = "partner_charts/perf.rs"]\nmod perf;\n'
    (work / PACKAGE / "src/partner_charts.rs").write_text(generated, encoding="utf-8")
    (out / "compiled.rs").write_text(generated, encoding="utf-8")
    (work / "lib.rs").write_text('#![allow(dead_code)]\n#[path="crates/deadsync-simfile/src/partner_charts.rs"] mod current;\n'
                               '#[path="crates/deadsync-simfile/src/metadata_perf.rs"] mod perf;\n', encoding="utf-8")
    manifest = '''[package]
name = "deadsync-partner-kernels"
version = "0.0.0"
edition = "2024"
[workspace]
[lib]
path = "lib.rs"
[dependencies]
rssp = { git = "https://github.com/pnn64/rssp", branch = "main" }
memchr = "2.8.3"
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
        result = subprocess.run(args, cwd=ROOT, env=env if extra is None else dict(env, **extra),
                                capture_output=True, text=True, encoding="utf-8")
        (out / filename).write_text(result.stdout + result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result.stdout

    command = ["cargo", "test", "--offline", "--manifest-path", str(work / "Cargo.toml")]
    if not debug:
        command.append("--release")
    execute(command + ["--", "--test-threads=1"], "correctness.log")
    output = execute(command + ["--no-run", "--message-format=json"], "build.log")
    artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    binary = next(Path(a["executable"]) for a in artifacts if a.get("executable"))
    saved = out / binary.name
    shutil.copyfile(binary, saved)
    trace = out / "projection-trace.bin"
    execute([str(saved), "current::perf::dump_partner_trace", "--exact", "--ignored", "--nocapture", "--test-threads=1"],
            "trace.log", {"PARTNER_TRACE_OUT": str(trace)})
    reference = ARCHIVE / "baseline/projection-trace.bin"
    if stage != "baseline" and reference.exists():
        assert trace.read_bytes() == reference.read_bytes(), "Projection trace differs"
    metadata = dict(stage=stage, command=command, workspace_manifest=manifest, baseline_revision=baseline,
                    rustflags=env["RUSTFLAGS"], rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    cpu=os.environ.get("PROCESSOR_IDENTIFIER"), sources=hashes, binary_sha256=sha(saved), runs=runs,
                    production_prefix_sha256=hashlib.sha256(prefix.encode()).hexdigest(),
                    compiled_sha256=sha(out / "compiled.rs"), lockfile=(work / "Cargo.lock").read_text(),
                    correctness_sha256=sha(out / "correctness.log"), trace_sha256=sha(trace), trace_bytes=trace.stat().st_size,
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", (out / "correctness.log").read_text()))))
    report = ROOT / f"docs/performance/2026-11-04-{report_prefix}-{stage}"
    report.parent.mkdir(parents=True, exist_ok=True)
    report.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if debug:
        print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)
        return
    rows = []
    for number in range(runs):
        output = execute([str(saved), "current::perf::" + benchmark, "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                         f"run-{number}.log")
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
        old = [float(r["cycles_per_batch"]) for r in rows if r["workload"] == label and r["variant"] == "original"]
        new = [float(r["cycles_per_batch"]) for r in rows if r["workload"] == label and r["variant"] == "current"]
        print(f"{label}: {statistics.median(old):.2f} -> {statistics.median(new):.2f}; "
              f"paired throughput {statistics.median(a / b for a, b in zip(old, new)):.3f}x", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--baseline")
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    if args.full:
        full_tests(args.stage, args.debug)
    else:
        run(args.stage, args.runs, args.debug, args.baseline)
