"""Snapshot life/combo/golf state kernels and measure thread cycles and allocations."""
import argparse
import csv
import filecmp
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

filecmp.BUFSIZE = 1024 * 1024

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = Path(os.environ.get("DEADSYNC_SCORING_STATE_ARCHIVE", ROOT / "target/scoring-state"))
PACKAGE = Path("crates/deadsync-rules")


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def item(source, marker):
    start = source.index(marker)
    brace = source.index("{", start)
    depth, end = 1, brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def fragments(source):
    parts = {name: source[name].split('#[cfg(test)]')[0] for name in ['life.rs', 'combo.rs', 'golf.rs', 'scroll.rs', 'song_time.rs', 'core_note.rs']}
    parts['grade'] = '#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]\n' + item(source['judgment.rs'], 'pub enum JudgeGrade {')
    hold = source['note.rs']
    constants = '\n'.join(re.findall(r'pub const (?:MAX_HOLD_LIFE|TIMING_WINDOW_SECONDS_HOLD|TIMING_WINDOW_SECONDS_ROLL):[^;]+;', hold))
    parts['hold'] = 'use crate::song_time::{SongTimeNs, song_time_ns_delta_seconds, song_time_ns_from_seconds};\n' + re.search(r'pub const TIMING_WINDOW_ADD_S:[^;]+;', source['timing.rs'])[0] + '\n' + constants + '\n#[derive(Clone, Copy, Debug, Default, PartialEq)]\n' + item(hold, 'pub struct HoldLifeAdvance {') + '\n#[inline(always)]\n#[must_use]\n' + item(hold, 'pub const fn hold_window_seconds(') + '\n#[inline(always)]\n#[must_use]\n' + item(hold, 'pub fn advance_hold_life_ns(')
    return parts


def assembled(parts):
    golf = parts['golf.rs'].replace('fn strokes_from_ms(', 'pub(super) fn strokes_from_ms(').replace('fn strokes_from_abs_ms(', 'pub(super) fn strokes_from_abs_ms(')
    if 'fn strokes_from_abs_ms(' not in golf:
        golf += '\npub(super) use strokes_from_ms as strokes_from_abs_ms;\n'
    return 'extern crate self as deadsync_core;\npub mod song_time {\n' + parts['song_time.rs'] + '\n}\npub mod note {\n' + parts['core_note.rs'] + '\n' + parts['hold'] + '\n}\npub mod judgment {\n' + parts['grade'] + '\n}\n' + '\n'.join('pub mod ' + name[:-3] + ' {\n' + (golf if name == 'golf.rs' else parts[name]) + '\n}\n' for name in ['life.rs', 'combo.rs', 'golf.rs', 'scroll.rs'])


def full_tests(stage):
    base = json.loads((ROOT / "docs/performance/2026-11-04-partner-charts-full-release.manifest.json").read_text(encoding="utf-8"))
    archive = Path(base["source_archive"])
    assert sha(archive) == base["source_archive_sha256"]
    work = archive.parent.parent / "full-workspace"
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    root_manifest = tomllib.loads(subprocess.check_output(["git", "show", "HEAD:Cargo.toml"], cwd=ROOT, text=True))
    packages = set()

    def visit(package):
        if package in packages:
            return
        packages.add(package)
        manifest = tomllib.loads(subprocess.check_output(["git", "show", f"HEAD:{package}/Cargo.toml"], cwd=ROOT, text=True))

        def tables(table):
            for key, value in table.items():
                if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                    for name, dependency in value.items():
                        if isinstance(dependency, dict):
                            if dependency.get("workspace"):
                                dependency = root_manifest["workspace"]["dependencies"][name]
                                parent = "."
                            else:
                                parent = package
                            if isinstance(dependency, dict) and "path" in dependency:
                                visit(posixpath.normpath(parent + "/" + dependency["path"]))
                elif isinstance(value, dict):
                    tables(value)
        tables(manifest)

    visit(PACKAGE.as_posix())
    supplement = out / "dependency-source.tar"
    with supplement.open("wb") as stream:
        subprocess.run(["git", "archive", "HEAD", *sorted(packages)], cwd=ROOT, stdout=stream, check=True)
    # Preserve cache mtimes when extending the earlier simfile workspace.
    with tarfile.open(supplement) as source:
        for member in source:
            if member.isdir():
                continue
            assert member.isfile(), member.name
            destination = (work / member.name).resolve()
            assert destination.is_relative_to(work.resolve())
            contents = source.extractfile(member).read()
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
    workspace_manifest = base["workspace_manifest"].replace('members = ["crates/deadsync-simfile"]', 'members = ["crates/deadsync-simfile", "crates/deadsync-notefield"]')
    workspace_manifest = workspace_manifest.replace('[workspace.dependencies]', '[workspace.package]\nversion = ' + json.dumps(root_manifest["workspace"]["package"]["version"]) + '\n[workspace.dependencies]\nmlua = { package = "deadlib-lua-legacy", version = "=0.12.2", path = "crates/deadlib-lua-legacy" }')
    assert '"crates/deadsync-notefield"' in workspace_manifest
    if (work / "Cargo.toml").read_text(encoding="utf-8") != workspace_manifest:
        (work / "Cargo.toml").write_text(workspace_manifest, encoding="utf-8")
    changed = subprocess.check_output(["git", "diff", "--name-only", base["git_revision"], "HEAD", "--", "crates", "tests", "vendor", "Cargo.lock"], cwd=ROOT, text=True).splitlines()
    paths = {Path(name) for name in changed}
    paths.update(path.relative_to(ROOT) for path in (ROOT / PACKAGE).rglob("*.rs"))
    paths.add(PACKAGE / "Cargo.toml")
    fixtures = out / "notefield-assets.tar"
    with fixtures.open("wb") as stream:
        subprocess.run(["git", "archive", "HEAD", "assets/noteskins/dance/cel"], cwd=ROOT, stdout=stream, check=True)
    with tarfile.open(fixtures) as source:
        source.extractall(work, filter="data")
    pngs = list((work / "assets/noteskins/dance/cel").glob("*.png"))
    assert pngs and all(path.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n" for path in pngs)
    common = Path(subprocess.check_output(["git", "rev-parse", "--git-common-dir"], cwd=ROOT, text=True).strip())
    if not common.is_absolute():
        common = ROOT / common
    fixture_root = Path(os.environ.get("DEADSYNC_NOTEFIELD_FIXTURE_ROOT", common.resolve().parent))
    external_fixtures = {}
    for path in (ROOT / PACKAGE / "src").rglob("*.rs"):
        for name in re.findall(r'tests/fixtures/[\w/.-]+', path.read_text(encoding="utf-8")):
            relative = Path(name)
            source = ROOT / relative
            if not source.is_file():
                source = fixture_root / relative
            contents = source.read_bytes()
            destination = work / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
            snapshot = out / "fixtures" / relative
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            snapshot.write_bytes(contents)
            external_fixtures[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    hashes = {}
    for relative in sorted(paths):
        contents = (ROOT / relative).read_bytes()
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists() or destination.read_bytes() != contents: destination.write_bytes(contents)
        snapshot = out / "source" / relative
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        snapshot.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    temporary = ARCHIVE / "compile-temp"
    temporary.mkdir(exist_ok=True)
    build_env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static", TEMP=str(temporary), TMP=str(temporary))
    build_command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-rules", "--lib", "--release", "--no-run", "--message-format=json"]
    with (out / "build.log").open("w", encoding="utf-8") as stream:
        build = subprocess.run(build_command, cwd=ROOT, env=build_env, stdout=stream, stderr=subprocess.STDOUT)
    if build.returncode: raise RuntimeError((out / "build.log").read_text(encoding="utf-8")[-12000:])
    binaries = []
    for line in (out / "build.log").read_text(encoding="utf-8").splitlines():
        try: event = json.loads(line)
        except json.JSONDecodeError: continue
        if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == "deadsync_rules" and event.get("executable"):
            binaries.append(Path(event["executable"]))
    assert len(binaries) == 1, binaries
    binary = binaries[0]
    command = [str(binary), "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=work, stdout=stream, stderr=subprocess.STDOUT)
    shutil.copyfile(binary, out / binary.name)
    log = (out / "correctness.log").read_text(encoding="utf-8")
    metadata = dict(build_command=build_command, command=command, base_revision=base["git_revision"],
                    source_archive=str(archive), source_archive_sha256=sha(archive), workspace_manifest=workspace_manifest, overlays=hashes,
                    dependency_archive=str(supplement), dependency_archive_sha256=sha(supplement), dependency_packages=sorted(packages),
                    external_fixture_root=str(fixture_root), external_fixtures=external_fixtures,
                    fixtures_sha256=sha(fixtures), shipped_png_count=len(pngs),
                    checkout_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    binary_sha256=sha(out / binary.name), build_sha256=sha(out / "build.log"),
                    correctness_sha256=sha(out / "correctness.log"), exit_code=result.returncode,
                    rustflags=build_env["RUSTFLAGS"], compile_temp=str(temporary), test_temp=os.environ.get("TEMP"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    lockfile=(work / "Cargo.lock").read_text(encoding="utf-8"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-04-scoring-state-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if result.returncode: raise RuntimeError(log[-12000:])
    print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)


def run(stage, runs, debug, baseline, reference_stage):
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    paths = [PACKAGE / ('src/' + name) for name in ['life.rs', 'combo.rs', 'golf.rs', 'scroll.rs', 'note.rs', 'timing.rs', 'judgment.rs', 'scoring_state_performance.rs', 'scoring_state_original.rs']] + [Path('crates/deadsync-core/src/song_time.rs'), Path('crates/deadsync-core/src/note.rs'), Path('tests/perf/stream_alloc.rs'), Path('tests/support/kernel_bench.rs'), Path(__file__).relative_to(ROOT)]
    hashes = {}
    for relative in paths:
        contents = (ROOT / relative).read_bytes()
        if baseline and relative in [PACKAGE / ("src/" + name) for name in ["life.rs", "combo.rs", "golf.rs", "scroll.rs", "note.rs"]]:
            contents = subprocess.check_output(["git", "show", f"{baseline}:{relative.as_posix()}"], cwd=ROOT)
        destination = out / "source" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    source_root = out / "source"
    inputs = {name: (source_root / PACKAGE / 'src' / name).read_text(encoding='utf-8') for name in ['life.rs', 'combo.rs', 'golf.rs', 'scroll.rs', 'note.rs', 'timing.rs', 'judgment.rs']}
    inputs.update({name: (source_root / path).read_text(encoding='utf-8') for name, path in {'song_time.rs': 'crates/deadsync-core/src/song_time.rs', 'core_note.rs': 'crates/deadsync-core/src/note.rs'}.items()})
    parts = fragments(inputs)
    source = '#![allow(dead_code)]\n' + assembled(parts)
    source += '\nuse golf::*;\n'
    source += f'\n#[path="{(source_root / "tests/perf/stream_alloc.rs").as_posix()}"] pub mod perf_alloc;\n'
    source += f'\n#[path="{(source_root / "tests/support/kernel_bench.rs").as_posix()}"] pub mod perf_bench;\n'
    source += f'\n#[path="{(source_root / PACKAGE / "src/scoring_state_performance.rs").as_posix()}"] mod scoring_state_performance;\n'
    compiled = out / "compiled.rs"
    compiled.write_text(source, encoding="utf-8")
    binary = out / "scoring_state.exe"
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
    trace = out / "scoring_state-trace.bin"
    execute([str(binary), "scoring_state_performance::dump_scoring_state_trace", "--exact", "--ignored", "--test-threads=1"], "trace.log", {"NOTEFIELD_SCORING_STATE_TRACE_OUT": str(trace)})
    reference = ARCHIVE / (reference_stage or ("baseline-debug" if debug else "baseline")) / "scoring_state-trace.bin"
    if trace != reference and reference.exists():
        assert filecmp.cmp(trace, reference, shallow=False), "SCORING_STATE trace differs"
    metadata = dict(stage=stage, command=command, baseline_revision=baseline, sources=hashes,
                    fragments={key: hashlib.sha256(value.encode()).hexdigest() for key, value in parts.items()},
                    compiled_sha256=sha(compiled), binary_sha256=sha(binary), correctness_sha256=sha(out / "correctness.log"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True), cpu=os.environ.get("PROCESSOR_IDENTIFIER"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", output))), runs=runs,
                    trace_sha256=sha(trace), trace_bytes=trace.stat().st_size,
                    reference_trace=str(reference), reference_trace_sha256=sha(reference) if reference.exists() else None,
                    ablation_runs=0 if baseline or debug else runs)
    report = ROOT / f"docs/performance/2026-11-04-scoring-state-{stage}"
    report.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2))
    if debug:
        print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)
        return
    rows = []
    for index in range(runs):
        output = execute([str(binary), "scoring_state_performance::benchmark_scoring_state", "--exact", "--ignored", "--nocapture", "--test-threads=1"], f"run-{index}.log")
        if not baseline:
            output += execute([str(binary), "scoring_state_performance::benchmark_golf_ablations", "--exact", "--ignored", "--nocapture", "--test-threads=1"], f"ablation-{index}.log")
        pairs = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", output)
        assert len(pairs) == (24 if baseline else 28), output
        churn = {label: dict(re.findall(r"(\w+): (\d+)", fields)) for label, fields in re.findall(r"([\w/]+): allocation Churn \{ ([^}]+) \}", output)}
        for label, old_cpu, new_cpu, old_ns, new_ns in pairs:
            for variant, cpu, ns in [("original", old_cpu, old_ns), ("current", new_cpu, new_ns)]:
                counts = {key: int(value) for key, value in churn[label + "/" + variant].items()}
                if label == 'scroll/errors/64':
                    assert churn[label + '/original'] == churn[label + '/current']
                else:
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
    parser.add_argument("--reference", help="stage with the identical correctness corpus")
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    if args.runs < 1 or (args.full and args.debug):
        parser.error("positive runs required; --debug applies to focused kernels")
    if args.full:
        full_tests(args.stage)
    else:
        run(args.stage, args.runs, args.debug, args.baseline, args.reference)
