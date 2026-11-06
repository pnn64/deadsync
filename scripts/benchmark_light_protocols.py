"""Snapshot cabinet-light protocol kernels and measure paired thread cycles and allocations."""
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
ARCHIVE = Path(os.environ.get("DEADSYNC_LIGHT_PROTOCOL_ARCHIVE", ROOT / "target/light-protocol"))
PACKAGE = Path("crates/deadsync-lights")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def item(source, marker):
    start = source.index(marker)
    brace = source.index("{", start)
    depth, end = 1, brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


PROTOCOLS = ["hid_blue_dot", "minimaid_hid", "pac_drive", "gpb", "linux_leds"]
BRIDGES = {
    "linux_leds": "pub(super) fn performance_outputs(state: &State, itgio: bool) -> [Option<bool>; OUTPUT_COUNT] { build_outputs(if itgio { &ITGIO_MAP } else { &PIUIO_MAP }, state) } pub(super) fn performance_entry_size() -> usize { std::mem::size_of::<MappedLight>() }",
    "gpb": "pub(super) fn performance_report(state: &State) -> [u8; REPORT_SIZE] { build_report(state) }",
    "hid_blue_dot": "pub(super) const fn performance_reports(state: &State) -> [[u8; REPORT_SIZE]; 2] { [build_cabinet_report(state), build_pad_report(state)] } pub(super) const fn performance_cabinet_report(state: &State) -> [u8; REPORT_SIZE] { build_cabinet_report(state) } pub(super) const fn performance_pad_report(state: &State) -> [u8; REPORT_SIZE] { build_pad_report(state) }",
    "minimaid_hid": "pub(super) fn performance_report(state: &State) -> [u8; REPORT_SIZE] { build_report(state) }",
    "pac_drive": "pub(super) const fn performance_report(state: &State, ordering: PacDriveLightOrdering) -> [u8; REPORT_SIZE] { build_report(state, ordering) }",
}


def fragments(sources):
    library = sources["lib"]
    result = {"PLAYER_COUNT": re.search(r"^const PLAYER_COUNT[^\n]+", library, re.M)[0]}
    for name, kind in [("Player", "enum"), ("CabinetLight", "enum"), ("ButtonLight", "enum"), ("PacDriveLightOrdering", "enum"), ("State", "struct")]:
        marker = f"pub {kind} {name}"
        start = library.index(marker)
        derive = library[library.rfind("#[derive(", 0, start):start]
        result[name] = derive + item(library, marker)
        if name != "PacDriveLightOrdering": result[name + "_impl"] = item(library, "impl " + name + " {")
    result["set_flag"] = item(library, "const fn set_flag(")
    for name in PROTOCOLS:
        source = sources[name]
        if name == "linux_leds":
            result[name + "_imports"] = source.splitlines()[0]
            result[name + "_constants"] = re.search(r"^const OUTPUT_COUNT[^\n]+", source, re.M)[0] + "\n" + source[source.index("const PIUIO_LIGHTS:"):source.index("pub struct Driver")]
            result[name + "_kernel"] = source[source.index("#[derive(Clone, Copy)]\nstruct BoardMap"):source.index("#[cfg(test)]")]
            result[name + "_bridge"] = BRIDGES[name]
            continue
        result[name + "_imports"] = source.splitlines()[0] + "\nuse std::time::Duration;\n"
        result[name + "_constants"] = "\n".join(re.findall(r"^const [A-Z][^\n]+", source, re.M))
        marker = "const fn build_cabinet_report(" if name == "hid_blue_dot" else ("fn build_report(" if name in ["minimaid_hid", "gpb"] else "const fn build_report(")
        result[name + "_kernel"] = source[source.index(marker):source.index("#[cfg(test)]")]
        result[name + "_bridge"] = BRIDGES[name]
    return result


def assembled(parts):
    source = "\n\n".join(value for key, value in parts.items() if not any(key.startswith(name + "_") for name in PROTOCOLS))
    for name in PROTOCOLS:
        source += "\nmod " + name + " {\n" + "\n".join(parts[name + "_" + key] for key in ["imports", "constants", "kernel", "bridge"]) + "\n}\n"
    return source


def full_tests(stage):
    base = json.loads((ROOT / "docs/performance/2026-11-04-partner-charts-full-release.manifest.json").read_text())
    archive = Path(base["source_archive"])
    assert sha(archive) == base["source_archive_sha256"]
    work = archive.parent.parent / "full-workspace"
    assert (work / "Cargo.toml").exists()
    changed = subprocess.check_output(["git", "diff", "--name-only", base["git_revision"], "HEAD", "--", "crates", "tests", "vendor", "Cargo.lock"], cwd=ROOT, text=True).splitlines()
    paths = {Path(name) for name in changed}
    paths.update(path.relative_to(ROOT) for path in (ROOT / PACKAGE).rglob("*.rs"))
    paths.add(PACKAGE / "Cargo.toml")
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for relative in sorted(paths):
        contents = (ROOT / relative).read_bytes()
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists() or destination.read_bytes() != contents:
            destination.write_bytes(contents)
        snapshot = out / "source" / relative
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        snapshot.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static")
    command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-lights", "--lib", "--release", "--", "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
    log = (out / "correctness.log").read_text()
    binaries = list((work / "target/release/deps").glob("deadsync_lights-*.exe"))
    binary = max(binaries, key=lambda p: p.stat().st_mtime) if binaries else None
    if binary:
        shutil.copyfile(binary, out / binary.name)
    metadata = dict(command=command, base_revision=base["git_revision"],
                    source_archive=str(archive), source_archive_sha256=sha(archive), overlays=hashes,
                    checkout_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    binary_sha256=sha(out / binary.name) if binary else None,
                    correctness_sha256=sha(out / "correctness.log"), exit_code=result.returncode,
                    rustflags=env["RUSTFLAGS"], test_temp=env.get("TEMP"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    lockfile=(work / "Cargo.lock").read_text(),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-04-light-protocol-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2))
    if result.returncode:
        raise RuntimeError(log[-12000:])
    print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)


def run(stage, runs, debug, baseline):
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    paths = [PACKAGE / "src/lib.rs", *[PACKAGE / ("src/" + name + ".rs") for name in PROTOCOLS],
             PACKAGE / "src/protocol_performance.rs", PACKAGE / "src/protocol_original.rs",
             Path("tests/support/perf.rs"), Path("tests/support/kernel_bench.rs"), Path(__file__).relative_to(ROOT)]
    hashes = {}
    for relative in paths:
        contents = (ROOT / relative).read_bytes()
        if baseline and relative in [PACKAGE / ("src/" + name + ".rs") for name in PROTOCOLS]:
            contents = subprocess.check_output(["git", "show", f"{baseline}:{relative.as_posix()}"], cwd=ROOT)
        destination = out / "source" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    source_root = out / "source"
    parts = fragments({name: (source_root / PACKAGE / ("src/" + name + ".rs")).read_text(encoding="utf-8") for name in ["lib", *PROTOCOLS]})
    source = '#![allow(dead_code)]\n' + assembled(parts)
    source += f'\n#[path="{(source_root / PACKAGE / "src/protocol_performance.rs").as_posix()}"] mod protocol_performance;\n'
    compiled = out / "compiled.rs"
    compiled.write_text(source, encoding="utf-8")
    binary = out / "protocol.exe"
    command = ["rustc", "--test", "--edition=2024", "-C", "opt-level=" + ("0" if debug else "3"), "-C", "target-feature=+crt-static", "-C", "debuginfo=0"]
    if not debug:
        command += ["-C", "lto=fat"]
    command += [str(compiled), "-o", str(binary)]
    temporary = ARCHIVE / "tmp"
    temporary.mkdir(exist_ok=True)
    env = dict(os.environ, TEMP=str(temporary), TMP=str(temporary))

    def execute(args, filename, extra=None):
        result = subprocess.run(args, cwd=ROOT, env=env if extra is None else dict(env, **extra), capture_output=True, text=True)
        (out / filename).write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result.stdout

    execute(command, "build.log")
    output = execute([str(binary), "--test-threads=1"], "correctness.log")
    trace = out / "protocol-trace.bin"
    execute([str(binary), "protocol_performance::dump_protocol_trace", "--exact", "--ignored", "--test-threads=1"], "trace.log", {"LIGHT_PROTOCOL_TRACE_OUT": str(trace)})
    reference = ARCHIVE / "baseline-linux/protocol-trace.bin"
    if stage != "baseline-linux" and reference.exists():
        assert trace.read_bytes() == reference.read_bytes(), "Protocol trace differs"
    metadata = dict(stage=stage, command=command, baseline_revision=baseline, sources=hashes,
                    fragments={key: hashlib.sha256(value.encode()).hexdigest() for key, value in parts.items()},
                    compiled_sha256=sha(compiled), binary_sha256=sha(binary), correctness_sha256=sha(out / "correctness.log"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True), cpu=os.environ.get("PROCESSOR_IDENTIFIER"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", output))), runs=runs,
                    trace_sha256=sha(trace), trace_bytes=trace.stat().st_size)
    report = ROOT / f"docs/performance/2026-11-04-light-protocol-{stage}"
    report.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2))
    if debug:
        print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)
        return
    rows = []
    for index in range(runs):
        output = execute([str(binary), "protocol_performance::benchmark_protocols", "--exact", "--ignored", "--nocapture", "--test-threads=1"], f"run-{index}.log")
        pairs = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", output)
        assert len(pairs) == 24, output
        churn = {label: dict(re.findall(r"(\w+): (\d+)", fields)) for label, fields in re.findall(r"([\w/]+): allocation Churn \{ ([^}]+) \}", output)}
        for label, old_cpu, new_cpu, old_ns, new_ns in pairs:
            for variant, cpu, ns in [("original", old_cpu, old_ns), ("current", new_cpu, new_ns)]:
                counts = {key: int(value) for key, value in churn[label + "/" + variant].items()}
                assert not any(counts.values()), (label, counts)
                rows.append(dict(run=index, workload=label, variant=variant, cycles_per_fixture=cpu, ns_per_fixture=ns, **counts))
        print(f"{stage}: run {index + 1}/{runs}", flush=True)
    with report.with_suffix(".tsv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({r["workload"] for r in rows}):
        old = [float(r["cycles_per_fixture"]) for r in rows if r["workload"] == label and r["variant"] == "original"]
        new = [float(r["cycles_per_fixture"]) for r in rows if r["workload"] == label and r["variant"] == "current"]
        print(f"{label}: {statistics.median(old):.2f} -> {statistics.median(new):.2f}; paired throughput {statistics.median(a / b for a, b in zip(old, new)):.3f}x", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--baseline")
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    if args.runs < 1 or (args.full and args.debug):
        parser.error("positive runs required; --debug applies to focused kernels")
    if args.full:
        full_tests(args.stage)
    else:
        run(args.stage, args.runs, args.debug, args.baseline)
