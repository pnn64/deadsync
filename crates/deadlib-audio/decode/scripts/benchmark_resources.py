"""Separate heap counters, fresh-process memory, and validated random-seek timing.

Python 3.11+, standard library only. See vorbis/RESOURCES.md for metric scopes.
"""
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
import struct
import subprocess
import tomllib
import uuid

from benchmark import DECODE, REPO, PINS, invoke, label, sha, rust_worker, registry_packages

ENGINES = ("native", "symphonia", "libvorbis")
PHASES = ("full", "startup", "seek_open", "seek_batch")


def executable(directory: Path, name: str) -> Path:
    return directory / (f"Release/{name}.exe" if os.name == "nt" else name)


def build_c(reference: Path, directory: Path, source: Path, target: str,
            run: Path, tag: str, allocations: bool | None = None) -> Path:
    command = ["cmake", "-S", source, "-B", directory,
               f"-DREFERENCE_ROOT={reference}", "-DCMAKE_BUILD_TYPE=Release"]
    if allocations is not None:
        command += [f"-DDEADLIB_TRACK_ALLOCATIONS={'ON' if allocations else 'OFF'}"]
    if os.name == "nt":
        command += ["-A", "x64"]
    invoke(command, log=run / f"{tag}-configure.log")
    invoke(["cmake", "--build", directory, "--config", "Release", "--target", target,
            "--parallel", "4"], log=run / f"{tag}-build.log")
    return executable(directory, target)


def workload(path: Path, resource: Path, oracle: Path, directory: Path,
             number: int, count: int, window: int, seed: int) -> tuple[Path, dict]:
    values = list(map(int, invoke([resource, "info", path, "-"]).strip().split("\t")))
    if len(values) != 5:
        raise ValueError("invalid reference geometry")
    frames, channels, rate, large_block, floor0 = values
    if frames <= 0 or not 1 <= channels <= 255 or floor0 not in (0, 1):
        raise ValueError("invalid reference geometry")
    # Explicit boundaries plus a fixed 64-bit LCG. Duplicated boundaries for
    # very short files are intentional; targets are retained-frame positions.
    targets = [min(t, frames - 1) for t in (0, 1, 127, 255, frames // 2, frames - 1)]
    state = seed & ((1 << 64) - 1)
    while len(targets) < count:
        state = (state * 6364136223846793005 + 1442695040888963407) & ((1 << 64) - 1)
        targets.append(state % frames)
    requests = [(target, min(window, frames - target)) for target in targets[:count]]
    request_path = directory / f"{number}-oracle-requests.bin"
    request_path.write_bytes(struct.pack("<Q", count) + b"".join(
        struct.pack("<QQ", target, want) for target, want in requests))
    result = subprocess.run([str(oracle), "oggseek", str(path), str(request_path)],
                            cwd=REPO, capture_output=True)
    (directory / f"{number}-oracle-stderr.log").write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(f"reference PCM windows failed: {result.stderr!r}")
    wire = memoryview(result.stdout)
    position = 0

    def take(size: int) -> bytes:
        nonlocal position
        if size < 0 or position + size > len(wire):
            raise ValueError("truncated reference seek output")
        value = bytes(wire[position:position + size])
        position += size
        return value

    def integer(size: int) -> int:
        return int.from_bytes(take(size), "little")

    if integer(4) != 1:
        raise ValueError("seek benchmark requires one logical stream")
    serial, actual_channels, actual_rate, actual_frames = integer(4), integer(4), integer(4), integer(8)
    if (actual_channels, actual_rate, actual_frames) != (channels, rate, frames):
        raise ValueError("reference geometry disagreement")
    output = bytearray(b"DLVSEEK1" + struct.pack("<IIIIQI", channels, rate, large_block, floor0, frames, count))
    for target, want in requests:
        if integer(8) != target:
            raise ValueError("reference seek did not land at requested frame")
        planes = [bytearray() for _ in range(channels)]
        received = 0
        while (chunk := integer(4)) != 0:
            if integer(4) != 0 or received + chunk > want:
                raise ValueError("invalid reference seek record")
            for plane in planes:
                block = take(chunk * 4)
                if any(not math.isfinite(x[0]) for x in struct.iter_unpack("<f", block)):
                    raise ValueError("nonfinite reference PCM")
                plane.extend(block)
            received += chunk
        if received != want:
            raise ValueError("reference seek sample count mismatch")
        output.extend(struct.pack("<QI", target, want))
        for plane in planes:
            output.extend(plane)
    if position != len(wire):
        raise ValueError("trailing reference seek data")
    destination = directory / f"{number}-seek.bin"
    destination.write_bytes(output)
    return destination, {"frames": frames, "channels": channels, "rate": rate,
        "large_block": large_block, "floor0": bool(floor0), "serial": serial,
        "requests": requests, "expected_channel_samples": sum(w * channels for _, w in requests),
        "sha256": sha(destination), "oracle_requests_sha256": sha(request_path),
        "oracle_output_sha256": hashlib.sha256(result.stdout).hexdigest()}


def write_table(path: Path, rows: list[dict]) -> None:
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", type=Path, nargs="*")
    parser.add_argument("--reference-root", type=Path, default=REPO / "target/deadlib-audio-reference")
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--resource-repeats", type=int, default=3)
    parser.add_argument("--requests", type=int, default=100)
    parser.add_argument("--window", type=int, default=256)
    parser.add_argument("--seed", type=int, default=1337)
    parser.add_argument("--min-seconds", type=float, default=0.15)
    args = parser.parse_args()
    if (args.repeats < 3 or args.resource_repeats < 1 or not 6 <= args.requests <= 10000
            or not 1 <= args.window <= 4096 or not math.isfinite(args.min_seconds) or args.min_seconds <= 0):
        parser.error("invalid repeat count, duration, request count, or window length")
    run = (args.run_root or REPO / f"target/deadlib-audio-reference/resources-{uuid.uuid4().hex}").resolve()
    run.mkdir(parents=True, exist_ok=False)
    reference = args.reference_root.resolve()
    for name, commit in PINS.items():
        source = reference / name
        if invoke(["git", "-C", source, "rev-parse", "HEAD"]).strip() != commit:
            raise RuntimeError(f"reference {name} differs from pinned source")
        if invoke(["git", "-C", source, "status", "--porcelain", "--untracked-files=no"]).strip():
            raise RuntimeError(f"reference {name} has modified tracked sources")
    workers, profiles, packages = {}, {}, {}
    for name, target, features in (("rust_counted", "resources", ["--features", "bench-alloc"]),
                                   ("rust_memory", "resources", []), ("rust_timing", "pipeline", [])):
        build = invoke(["cargo", "bench", "--locked", "-p", "deadlib-audio-decode-vorbis",
                        "--bench", target, "--no-run", "--message-format=json", *features], log=run / f"{name}-build.log")
        workers[name], profiles[name] = rust_worker(build, target)
        packages[name] = registry_packages(build)
    csource = DECODE / "vorbis/benches/oracle"
    cbuild = reference / "vorbis-benchmark-build"
    workers["c_memory"] = build_c(reference, cbuild, csource, "deadlib_vorbis_resources", run, "c-memory", False)
    workers["c_counted"] = build_c(reference, reference / "vorbis-resource-alloc-build", csource,
                                    "deadlib_vorbis_resources", run, "c-counted", True)
    workers["oracle"] = build_c(reference, reference / "vorbis-oracle-build", DECODE / "vorbis/tests/oracle",
                                 "deadlib_vorbis_oracle", run, "oracle")
    for name in ("rust_counted", "c_counted"):
        if invoke([workers[name], "--selftest"]).strip() != "allocator-selftest:ok":
            raise RuntimeError("allocator self-test failed")
    files = args.files or [DECODE / "vorbis/tests/fixtures" / name for name in
                          ("mono.ogg", "stereo.ogg", "eight-channel.ogg")]
    if not args.files:
        files.append(REPO / "assets/music/in_two (loop).ogg")
        song = reference / "corpus/itgmania/goin-under.ogg"
        if song.exists():
            files.append(song)
    files = [p.resolve() for p in files]
    directory = run / "workloads"
    directory.mkdir()
    inputs, metadata = {}, {}
    for number, path in enumerate(files):
        inputs[path], metadata[label(path)] = workload(path, workers["c_memory"], workers["oracle"],
                                                       directory, number, args.requests, args.window, args.seed)
    source_files = {p for folder in (DECODE / "ogg/src", DECODE / "vorbis/src", DECODE / "vorbis/benches",
                                   DECODE / "vorbis/tests/oracle") for p in folder.rglob("*") if p.is_file()}
    source_files.update((Path(__file__).resolve(), DECODE / "scripts/benchmark.py", REPO / "Cargo.toml",
                         REPO / "Cargo.lock", REPO / ".cargo/config.toml", DECODE / "ogg/Cargo.toml", DECODE / "vorbis/Cargo.toml"))
    manifest = {"started_utc": datetime.now(timezone.utc).isoformat(),
        "git_head": invoke(["git", "rev-parse", "HEAD"]).strip(), "reference_pins": PINS,
        "arguments": {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items() if k != "files"},
        "platform": platform.platform(), "processor": platform.processor(), "logical_cpus": os.cpu_count(),
        "rustc": invoke(["rustc", "-Vv"]), "cmake": invoke(["cmake", "--version"]).splitlines()[0],
        "workers": {name: {"path": label(path), "sha256": sha(path)} for name, path in workers.items()},
        "cargo_artifact_profiles": profiles, "registry_packages": packages,
        "cargo_profiles": tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["profile"],
        "cargo_config": tomllib.loads((REPO / ".cargo/config.toml").read_text(encoding="utf-8")),
        "rustflags_environment": {name: os.environ.get(name) for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")},
        "cmake_release_flags": [line for line in (cbuild / "CMakeCache.txt").read_text(encoding="utf-8").splitlines()
                                 if line.startswith(("CMAKE_C_FLAGS_RELEASE:", "CMAKE_CXX_FLAGS_RELEASE:", "CMAKE_EXE_LINKER_FLAGS_RELEASE:"))],
        "source_sha256": {label(p): sha(p) for p in sorted(source_files)},
        "input_sha256": {label(p): sha(p) for p in files}, "workloads": metadata,
        "allocator_self_tests": "passed both actual instrumented workers",
        "metric_scopes": {"heap": "requested bytes; Rust global allocator / only libogg+libvorbis+libvorbisfile C allocation calls; input and harness excluded",
                          "process_memory": "fresh uninstrumented process PeakWorkingSetSize; input, workload, loader and harness included; seek_batch includes warm/validation",
                          "seek_timing": "uninstrumented; resident input; seek_open constructs+destroys; seek_batch warmed context, exact requested consumption; PCM validation excluded",
                          "api_exposed_samples": "API-visible samples, not hidden synthesized frames; native page callbacks may exceed demand"}}
    (run / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    def resource(engine: str, phase: str, path: Path, counted: bool) -> dict:
        if engine == "libvorbis":
            command = [workers["c_counted" if counted else "c_memory"], phase, path, inputs[path]]
        else:
            command = [workers["rust_counted" if counted else "rust_memory"], engine, phase, path, inputs[path]]
        row = json.loads(invoke(command))
        if (row["engine"], row["phase"], row["instrumented"]) != (engine, phase, counted):
            raise ValueError("resource worker identity mismatch")
        if counted and row["process_peak_working_set_bytes"] is not None:
            raise ValueError("instrumented worker reported process memory")
        if not counted and any(row[k] is not None for k in ("operations", "requested_bytes", "peak_extra_bytes", "retained_delta_bytes")):
            raise ValueError("uninstrumented worker reported heap counters")
        if counted and phase == "full" and row["retained_delta_bytes"] != 0:
            raise ValueError("unexpected retained full-decode allocation")
        return {"path": label(path), **row}

    randomizer = random.Random(args.seed)
    rows = []
    for path in files:
        for phase in PHASES:
            for trial in range(args.resource_repeats):
                cases = [(engine, counted) for engine in ENGINES for counted in (True, False)]
                randomizer.shuffle(cases)
                for engine, counted in cases:
                    row = resource(engine, phase, path, counted)
                    row["trial"] = trial
                    rows.append(row)
                write_table(run / "resources.tsv", rows)
            print(f"{label(path)}: {phase} resources validated", flush=True)

    def timing(engine: str, phase: str, path: Path, iterations: int) -> dict:
        if engine == "libvorbis":
            command = [workers["c_memory"], "seek_timing" if phase == "seek_batch" else "seek_open_timing", path, inputs[path], str(iterations)]
        else:
            command = [workers["rust_timing"], engine, phase, str(iterations), path, inputs[path]]
        fields = invoke(command).strip().split("\t")
        if len(fields) != 10 or fields[:2] != [engine, phase]:
            raise ValueError(f"invalid timing output: {fields}")
        n, elapsed, units, reads, consumed, seeks, tells, exposed = map(int, fields[2:])
        expected = metadata[label(path)]["expected_channel_samples"] if phase == "seek_batch" else 1
        if n != iterations or elapsed <= 0 or units != expected * n:
            raise ValueError("timing count mismatch")
        return {"path": label(path), "engine": engine, "phase": phase, "iterations": n,
                "elapsed_ns": elapsed, "ns_per_run": elapsed / n, "units": units,
                "reads_per_run": reads / n, "bytes_read_per_run": consumed / n,
                "seeks_per_run": seeks / n, "tells_per_run": tells / n, "api_exposed_samples_per_run": exposed / n}

    trials = []
    for path in files:
        for phase in ("seek_open", "seek_batch"):
            iterations = {engine: min(10000, max(1, math.ceil(args.min_seconds * 1e9 /
                          timing(engine, phase, path, 1)["elapsed_ns"]))) for engine in ENGINES}
            for trial in range(args.repeats):
                engines = list(ENGINES)
                randomizer.shuffle(engines)
                for engine in engines:
                    row = timing(engine, phase, path, iterations[engine])
                    row["trial"] = trial
                    trials.append(row)
                write_table(run / "seek-trials.tsv", trials)
            print(f"{label(path)}: {phase} timing complete", flush=True)
    summary = []
    for path in files:
        for phase in ("seek_open", "seek_batch"):
            for engine in ENGINES:
                group = [r for r in trials if (r["path"], r["phase"], r["engine"]) == (label(path), phase, engine)]
                times = [r["ns_per_run"] for r in group]
                median = statistics.median(times)
                summary.append({"path": label(path), "engine": engine, "phase": phase,
                    "median_ns": median, "min_ns": min(times), "max_ns": max(times),
                    "mad_ns": statistics.median(abs(t - median) for t in times),
                    **{k: group[0][k] for k in ("reads_per_run", "bytes_read_per_run", "seeks_per_run", "tells_per_run", "api_exposed_samples_per_run")}})
    write_table(run / "seek-summary.tsv", summary)
    # Detect source/input/executable drift during long runs before accepting data.
    for mapping in (manifest["source_sha256"], manifest["input_sha256"]):
        for name, expected in mapping.items():
            if sha(REPO / name) != expected:
                raise RuntimeError(f"source/input changed during measurement: {name}")
    for name, path in workers.items():
        if sha(path) != manifest["workers"][name]["sha256"]:
            raise RuntimeError(f"worker changed during measurement: {name}")
    (run / "complete.json").write_text(json.dumps({"completed_utc": datetime.now(timezone.utc).isoformat(),
        "resource_rows": len(rows), "timing_rows": len(trials), "source_input_binary_hashes_rechecked": True}, indent=2) + "\n", encoding="utf-8")
    print(f"Resource/seek evidence retained at {run}", flush=True)


if __name__ == "__main__":
    main()
