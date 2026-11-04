"""Snapshot SMX effect kernels and measure paired thread cycles and allocations."""
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
import tarfile

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = Path(os.environ.get("DEADSYNC_SMX_EFFECTS_ARCHIVE", ROOT / "target/smx-effects"))
PACKAGE = Path("crates/deadsync-smx")


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


def fragments(panels, roles, gifs, score):
    return {
        "panel_constants": panels[panels.index("/// Pads addressed"):panels.index("/// Map a logical column")],
        "effect_engine": panels[panels.index("const MIN_FRAME_DURATION_S:"):panels.index("// 30Hz worker thread")],
        "results_roles": item(roles, "pub fn results_role_candidates("),
        "full_pad_anim": item(gifs, "pub struct FullPadAnim"),
        "panel_anim": item(gifs, "pub struct PanelAnim"),
        "panel_anim_impl": item(gifs, "impl PanelAnim"),
        "panel_bytes": re.search(r"^pub const PANEL_RGB_BYTES[^\n]+", gifs, re.M)[0],
        "panel_frame": re.search(r"^pub type PanelFrame[^\n]+", gifs, re.M)[0],
        "grade_enum": item(score, "pub enum Grade"),
        "grade_suffix": item(score, "pub const fn gif_suffix("),
        "grade_base": item(score, "pub const fn gif_base("),
    }


def assembled(parts, source_root):
    source = "extern crate self as deadsync_score;\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n" + parts["grade_enum"]
    source += "\nimpl Grade {\n" + parts["grade_suffix"] + "\n" + parts["grade_base"] + "\n}\n"
    source += "mod gifs {use crate::panels::{LEDS_PER_PANEL, PANELS};\n" + "\n".join(parts[key] for key in ["panel_bytes", "panel_frame", "full_pad_anim", "panel_anim", "panel_anim_impl"]) + "\n}\n"
    source += "mod panel_fx {use deadsync_score::Grade;\n" + parts["results_roles"] + "\n}\n"
    source += "mod panels {use std::sync::Arc; use crate::gifs::{FullPadAnim,PanelAnim,PanelFrame};\n" + parts["panel_constants"] + parts["effect_engine"]
    source += f'\n#[path="{(source_root / PACKAGE / "src/effects_performance.rs").as_posix()}"] mod effects_performance;\n}}\n'
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
    fixtures = out / "lighting-assets.tar"
    with fixtures.open("wb") as stream:
        subprocess.run(["git", "archive", "HEAD", "assets/smx-pad-lights", "assets/smx-judge-lights"],
                       cwd=ROOT, stdout=stream, check=True)
    with tarfile.open(fixtures) as source:
        source.extractall(work, filter="data")
    gifs = [path for name in ["smx-pad-lights", "smx-judge-lights"]
            for path in (work / "assets" / name).rglob("*.gif")]
    assert gifs and all(path.read_bytes()[:6] in [b"GIF87a", b"GIF89a"] for path in gifs)
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
    command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-smx", "--lib", "--release", "--", "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
    log = (out / "correctness.log").read_text()
    binaries = list((work / "target/release/deps").glob("deadsync_smx-*.exe"))
    binary = max(binaries, key=lambda p: p.stat().st_mtime) if binaries else None
    if binary:
        shutil.copyfile(binary, out / binary.name)
    metadata = dict(command=command, base_revision=base["git_revision"],
                    source_archive=str(archive), source_archive_sha256=sha(archive), overlays=hashes,
                    fixtures_sha256=sha(fixtures), shipped_gif_count=len(gifs),
                    checkout_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    binary_sha256=sha(out / binary.name) if binary else None,
                    correctness_sha256=sha(out / "correctness.log"), exit_code=result.returncode,
                    rustflags=env["RUSTFLAGS"], test_temp=env.get("TEMP"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    lockfile=(work / "Cargo.lock").read_text(),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-04-smx-effects-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2))
    if result.returncode:
        raise RuntimeError(log[-12000:])
    print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)


def run(stage, runs, debug, baseline):
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    paths = [PACKAGE / "src/panels.rs", PACKAGE / "src/panel_fx.rs", PACKAGE / "src/gifs.rs", PACKAGE / "src/effects_performance.rs", PACKAGE / "src/effects_original.rs",
             Path("crates/deadsync-score/src/lib.rs"), Path("tests/support/perf.rs"), Path("tests/support/kernel_bench.rs"), Path(__file__).relative_to(ROOT)]
    hashes = {}
    for relative in paths:
        if relative == Path("crates/deadsync-score/src/lib.rs") or (baseline and relative in [PACKAGE / "src/panels.rs", PACKAGE / "src/panel_fx.rs"]):
            contents = subprocess.check_output(["git", "show", f"{baseline or 'HEAD'}:{relative.as_posix()}"], cwd=ROOT)
        else:
            contents = (ROOT / relative).read_bytes()
        destination = out / "source" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    source_root = out / "source"
    parts = fragments(*[(source_root / relative).read_text(encoding="utf-8") for relative in [PACKAGE / "src/panels.rs", PACKAGE / "src/panel_fx.rs", PACKAGE / "src/gifs.rs", Path("crates/deadsync-score/src/lib.rs")]])
    source = '#![allow(dead_code)]\n' + assembled(parts, source_root)
    source += f'\n#[path="{(source_root / "tests/support/perf.rs").as_posix()}"] mod lighting_allocations;\n'
    compiled = out / "compiled.rs"
    compiled.write_text(source, encoding="utf-8")
    binary = out / "effects.exe"
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
    trace = out / "effects-trace.bin"
    execute([str(binary), "panels::effects_performance::dump_effects_trace", "--exact", "--ignored", "--test-threads=1"], "trace.log", {"SMX_EFFECTS_TRACE_OUT": str(trace)})
    reference = ARCHIVE / "baseline/effects-trace.bin"
    if stage != "baseline" and reference.exists():
        assert trace.read_bytes() == reference.read_bytes(), "Effect trace differs"
    metadata = dict(stage=stage, command=command, baseline_revision=baseline, sources=hashes,
                    fragments={key: hashlib.sha256(value.encode()).hexdigest() for key, value in parts.items()},
                    compiled_sha256=sha(compiled), binary_sha256=sha(binary), correctness_sha256=sha(out / "correctness.log"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True), cpu=os.environ.get("PROCESSOR_IDENTIFIER"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", output))), runs=runs,
                    trace_sha256=sha(trace), trace_bytes=trace.stat().st_size)
    report = ROOT / f"docs/performance/2026-11-04-smx-effects-{stage}"
    report.with_suffix(".manifest.json").write_text(json.dumps(metadata, indent=2))
    if debug:
        print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)
        return
    rows = []
    for index in range(runs):
        output = execute([str(binary), "panels::effects_performance::benchmark_effects", "--exact", "--ignored", "--nocapture", "--test-threads=1"], f"run-{index}.log")
        pairs = re.findall(r"([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)", output)
        assert len(pairs) == 15, output
        churn = {label: dict(re.findall(r"(\w+): (\d+)", fields)) for label, fields in re.findall(r"([\w/]+): allocation Churn \{ ([^}]+) \}", output)}
        for label, old_cpu, new_cpu, old_ns, new_ns in pairs:
            for variant, cpu, ns in [("original", old_cpu, old_ns), ("current", new_cpu, new_ns)]:
                counts = {key: int(value) for key, value in churn[label + "/" + variant].items()}
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
