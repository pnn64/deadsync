"""Build and compare resident-input Vorbis workers; Python 3.11+, stdlib only."""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tomllib
import uuid

DECODE = Path(__file__).resolve().parent.parent
REPO = DECODE.parents[2]
PINS = {"ogg": "be05b13e98b048f0b5a0f5fa8ce514d56db5f822",
        "vorbis": "0657aee69dec8508a0011f47f3b69d7538e9d262"}

def invoke(args: list[str | Path], *, log: Path | None = None) -> str:
    result = subprocess.run([str(a) for a in args], cwd=REPO, capture_output=True, text=True)
    if log:
        log.write_text(result.stdout + result.stderr, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {args}\n{result.stderr[-4000:]}")
    return result.stdout

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def label(path: Path) -> str:
    try:
        return path.relative_to(REPO).as_posix()
    except ValueError:
        return str(path)

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", type=Path, nargs="*")
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--reference-root", type=Path, default=REPO / "target/deadlib-audio-reference")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--min-seconds", type=float, default=0.15)
    parser.add_argument("--seed", type=int, default=1337)
    parser.add_argument("--imdct", action="store_true", help="compare isolated inverse transforms at all legal block sizes")
    args = parser.parse_args()
    if args.repeats < 3 or args.min_seconds <= 0:
        parser.error("use at least three repeats and a positive duration")
    if args.imdct and args.files:
        parser.error("--imdct uses generated spectra; do not supply files")
    run = (args.run_root or REPO / f"target/deadlib-audio-reference/benchmark-{uuid.uuid4().hex}").resolve()
    run.mkdir(parents=True, exist_ok=False)
    reference = args.reference_root.resolve()
    for name, commit in PINS.items():
        path = reference / name
        if invoke(["git", "-C", path, "rev-parse", "HEAD"]).strip() != commit:
            raise RuntimeError(f"reference {name} does not match pinned {commit}")
        if invoke(["git", "-C", path, "status", "--porcelain", "--untracked-files=no"]).strip():
            raise RuntimeError(f"reference {name} has modified tracked sources")
    build = invoke(["cargo", "bench", "--locked", "-p", "deadlib-audio-decode-vorbis", "--bench", "pipeline", "--no-run", "--message-format=json"], log=run / "rust-build.log")
    binaries = []
    rust_profile = None
    for line in build.splitlines():
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact"
            and "deadlib-audio-decode-vorbis" in message.get("package_id", "")
            and message["target"]["name"] == "pipeline" and message.get("executable")):
            binaries.append(Path(message["executable"]))
            rust_profile = message["profile"]
    if len(binaries) != 1:
        raise RuntimeError(f"cannot identify exact native/Symphonia worker: {binaries}")
    rust = binaries[0]
    cbuild = reference / "vorbis-benchmark-build"
    configure = ["cmake", "-S", DECODE / "vorbis/benches/oracle", "-B", cbuild,
                 f"-DREFERENCE_ROOT={reference}", "-DCMAKE_BUILD_TYPE=Release"]
    if os.name == "nt":
        configure += ["-A", "x64"]
    invoke(configure, log=run / "c-configure.log")
    invoke(["cmake", "--build", cbuild, "--config", "Release", "--target", "deadlib_vorbis_benchmark", "--parallel", "4"], log=run / "c-build.log")
    oracle = cbuild / ("Release/deadlib_vorbis_benchmark.exe" if os.name == "nt" else "deadlib_vorbis_benchmark")
    files = args.files or [DECODE / "vorbis/tests/fixtures" / name for name in ("mono.ogg", "stereo.ogg", "surround.ogg")]
    if not args.files and not args.imdct:
        files += [REPO / "assets/music/in_two (loop).ogg"]
        song = reference / "corpus/itgmania/goin-under.ogg"
        if song.exists():
            files.append(song)
    files = [Path(str(n)) for n in (64, 128, 256, 512, 1024, 2048, 4096, 8192)] if args.imdct else [p.resolve() for p in files]
    phases = ("imdct",) if args.imdct else ("full", "stream4096", "startup")
    engines_available = ("native", "libvorbis") if args.imdct else ("native", "symphonia", "libvorbis")
    manifest = {
        "started_utc": datetime.now(timezone.utc).isoformat(), "git_head": invoke(["git", "rev-parse", "HEAD"]).strip(),
        "rustc": invoke(["rustc", "-Vv"]), "cmake": invoke(["cmake", "--version"]).splitlines()[0],
        "platform": platform.platform(), "processor": platform.processor(), "logical_cpus": os.cpu_count(),
        "reference_pins": PINS, "seed": args.seed, "repeats": args.repeats, "min_seconds": args.min_seconds,
        "binary_sha256": {"rust": sha(rust), "libvorbis": sha(oracle)},
        "cargo_artifact_profile": rust_profile,
        "cargo_profiles": tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["profile"],
        "cargo_lock_sha256": sha(REPO / "Cargo.lock"),
        "rustflags_environment": {name: os.environ.get(name) for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")},
        "cmake_release_flags": [line for line in (cbuild / "CMakeCache.txt").read_text(encoding="utf-8").splitlines() if line.startswith(("CMAKE_C_FLAGS_RELEASE:", "CMAKE_CXX_FLAGS_RELEASE:", "CMAKE_EXE_LINKER_FLAGS_RELEASE:"))],
        "input_sha256": {} if args.imdct else {label(p): sha(p) for p in files},
        "workload": "isolated_imdct" if args.imdct else "container_pipeline",
        "source_sha256": {label(p): sha(p) for directory in (DECODE / "vorbis/src", DECODE / "ogg/src", DECODE / "vorbis/benches") for p in directory.rglob("*") if p.is_file()},
        "timer": "Rust Instant / C++ steady_clock; resident input; no process/file-load time",
        "io": None if args.imdct else "forward-only Read callbacks; full and startup uncapped, stream4096 capped at 4096 bytes/read",
    }
    manifest["source_sha256"][label(Path(__file__).resolve())] = sha(Path(__file__).resolve())
    for path in (REPO / "Cargo.toml", DECODE / "vorbis/Cargo.toml", DECODE / "ogg/Cargo.toml"):
        manifest["source_sha256"][label(path)] = sha(path)
    cargo_config = REPO / ".cargo/config.toml"
    if cargo_config.exists():
        manifest["cargo_config"] = tomllib.loads(cargo_config.read_text(encoding="utf-8"))
        manifest["source_sha256"][label(cargo_config)] = sha(cargo_config)
    (run / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    randomizer = random.Random(args.seed)
    rows = []
    counts = {}
    def worker(engine: str, phase: str, iterations: int, path: Path) -> dict:
        command = ([oracle, phase, str(iterations), path] if engine == "libvorbis"
                   else [rust, engine, phase, str(iterations), path])
        values = invoke(command).strip().split("\t")
        if len(values) != 7 or values[:2] != [engine, phase]:
            raise RuntimeError(f"invalid worker output: {values}")
        n, ns, samples, reads, consumed = map(int, values[2:])
        if n != iterations or ns <= 0 or samples <= 0:
            raise RuntimeError(f"invalid measurement: {values}")
        if phase != "startup":
            if samples % iterations:
                raise RuntimeError("nonintegral repeated sample count")
            count = samples // iterations
            if counts.setdefault(path, count) != count:
                raise RuntimeError(f"decoded count mismatch: {engine} {path}: {count} != {counts[path]}")
        return {"path": label(path), "engine": engine, "phase": phase,
                "iterations": n, "elapsed_ns": ns, "samples": samples,
                "read_calls": reads, "bytes_read": consumed,
                "ns_per_run": ns / n, "samples_per_run": samples / n,
                "msamples_per_second": samples * 1000 / ns}
    for path in files:
        for phase in phases:
            iterations = {}
            for engine in engines_available:
                pilot = worker(engine, phase, 1, path)
                iterations[engine] = min(10000, max(1, math.ceil(args.min_seconds * 1e9 / pilot["elapsed_ns"])))
            for trial in range(args.repeats):
                engines = list(iterations)
                randomizer.shuffle(engines)
                for engine in engines:
                    row = worker(engine, phase, iterations[engine], path)
                    row["trial"] = trial
                    rows.append(row)
            print(f"{label(path)}: {phase} complete", flush=True)
    with (run / "trials.tsv").open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader(); writer.writerows(rows)
    summary = []
    for path in files:
        for phase in phases:
            for engine in engines_available:
                group = [r for r in rows if r["path"] == label(path) and r["engine"] == engine and r["phase"] == phase]
                times = [r["ns_per_run"] for r in group]
                median = statistics.median(times)
                summary.append({"path": label(path), "engine": engine, "phase": phase,
                    "median_ns": median, "min_ns": min(times), "max_ns": max(times),
                    "mad_ns": statistics.median(abs(t - median) for t in times),
                    "samples_per_run": group[0]["samples_per_run"],
                    "median_msamples_per_second": statistics.median(r["msamples_per_second"] for r in group),
                    "read_calls_per_run": group[0]["read_calls"] / group[0]["iterations"],
                    "bytes_read_per_run": group[0]["bytes_read"] / group[0]["iterations"]})
    with (run / "summary.tsv").open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(summary[0]), delimiter="\t")
        writer.writeheader(); writer.writerows(summary)
    print(f"Benchmark evidence retained at {run}")

if __name__ == "__main__":
    main()
