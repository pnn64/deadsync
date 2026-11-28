"""Benchmark unchanged official Lua 5.1 files in fresh isolated processes."""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import struct
import subprocess
import sys
import time

FILES = [
    "constructs.lua", "locals.lua", "calls.lua", "events.lua", "nextvar.lua",
    "literals.lua", "closure.lua", "vararg.lua", "strings.lua", "pm.lua",
    "sort.lua", "math.lua", "gc.lua", "db.lua", "errors.lua", "files.lua",
    "attrib.lua", "verybig.lua",
]
RETURNS = {"locals.lua": 5, "events.lua": 12, "attrib.lua": 27, "verybig.lua": 10}
PGO_TRAINING = [name for name in FILES if name not in ("constructs.lua", "files.lua")]
PGO_DYNAMIC = "independent_chunks.lua"
PGO_ITERATIONS = 3


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_sample(
    binary: Path, source: Path, directory: Path, name: str, sample: int,
    environment: dict[str, str], timeout: float, cpu: int | None = None,
    allocations: bool = False,
) -> dict:
    cwd = directory / name / str(sample)
    shutil.copytree(source, cwd)
    # Cargo/Git do not retain the archive's empty module-search directory.
    (cwd / "libs/P1").mkdir(parents=True, exist_ok=True)
    child_command = [str(binary), "rust-bench", str(cwd / name)]
    child = subprocess.Popen(child_command, cwd=cwd, env=environment,
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE,
                             creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0)
    if cpu is not None:
        if sys.platform == "win32":
            affinity = ctypes.windll.kernel32.SetProcessAffinityMask
            affinity.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
            if not affinity(int(child._handle), 1 << cpu):
                child.kill()
                child.communicate()
                raise ctypes.WinError()
        else:
            os.sched_setaffinity(child.pid, {cpu})
    expired = False
    try:
        stdout, stderr = child.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        expired = True
        if sys.platform == "win32":
            subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"], capture_output=True)
        else:
            child.kill()
        stdout, stderr = child.communicate()
    error = stderr.decode("utf-8", "replace")
    record = next((line.split("\t") for line in error.splitlines()
                   if line.startswith("BENCH\t")), None)
    passed = (not expired and child.returncode == 0 and record is not None
              and len(record) == 5 and record[1] == name)
    if passed:
        expected = struct.pack(">d", float(RETURNS[name])).hex() if name in RETURNS else "none"
        passed = record[4] == expected
    counts = None
    if allocations and passed:
        counts = next((line.split("\t")[1:] for line in error.splitlines()
                       if line.startswith("ALLOC\t")), None)
        passed = (record[2:4] == ["unavailable", "unavailable"] and counts is not None
                  and len(counts) == 3 and all(value.isdecimal() for value in counts))
    platform_failure = (sys.platform == "win32" and name == "files.lua"
                        and child.returncode == 1 and not expired
                        and any(line.startswith("FAIL\tRuntime\t59\t")
                                and line.endswith(": Permission denied") for line in error.splitlines()))
    row = dict(file=name, sample=sample, command=child_command, exit=child.returncode,
               timeout=expired, status="pass" if passed else
               "platform-expected-failure" if platform_failure else "fail",
               stdout=stdout.decode("utf-8", "replace"), stderr=error)
    if passed:
        row["returned"] = record[4]
        if allocations:
            row["allocations"] = dict(zip(("fresh", "resized", "growth_bytes"), map(int, counts)))
        else:
            row.update(nanoseconds=int(record[2]),
                       cycles=int(record[3]) if record[3] != "unavailable" else None)
    return row


def profiled_build(
    root: Path, source: Path, directory: Path, suffix: str, command: list[str],
    environment: dict[str, str], timeout: float, allocations: bool = False,
) -> tuple[Path, list[str], dict]:
    rustc = environment.get("RUSTC", "rustc")
    version = subprocess.check_output([rustc, "-vV"], cwd=root, text=True)
    host = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
    libdir = subprocess.check_output([rustc, "--print", "target-libdir"], cwd=root, text=True)
    profdata = Path(libdir.strip()).parent / "bin" / f"llvm-profdata{suffix}"
    if not profdata.is_file():
        raise SystemExit("llvm-profdata is missing from this Rust toolchain. Install llvm-tools-preview.")

    def build(flag: str, target: Path) -> tuple[Path, list[str]]:
        env = environment.copy()
        args = command + ["--target", host, "--target-dir", str(target)]
        separator = "\x1f"
        if "CARGO_ENCODED_RUSTFLAGS" in env:
            flags = env["CARGO_ENCODED_RUSTFLAGS"]
            env["CARGO_ENCODED_RUSTFLAGS"] = separator.join(filter(None, [flags, flag]))
        elif "RUSTFLAGS" in env:
            env["CARGO_ENCODED_RUSTFLAGS"] = separator.join([*env["RUSTFLAGS"].split(), flag])
        else:
            # Cargo joins the target tuple's flags with matching cfg flags.
            args += ["--config", f"target.{host}.rustflags={json.dumps([flag])}"]
        subprocess.run(args, cwd=root, env=env, check=True)
        return target / host / f"release/examples/official_suite{suffix}", args

    raw = directory / "profiles"
    raw.mkdir()
    instrumented, instrument_command = build(f"-Cprofile-generate={raw}", directory / "generate")
    training_env = dict(environment, LLVM_PROFILE_FILE=str(raw / "fixture-%p-%m.profraw"))
    training_files = [*PGO_TRAINING, PGO_DYNAMIC]
    jobs = [(name, trial) for trial in range(PGO_ITERATIONS) for name in training_files]
    random.Random(51051).shuffle(jobs)
    for name, trial in jobs:
        training_source = source.parent / "pgo" if name == PGO_DYNAMIC else source
        row = run_sample(instrumented, training_source, directory / "training", name, trial,
                         training_env, timeout, allocations=allocations)
        if row["status"] != "pass":
            raise SystemExit(f"PGO training failed on {name} (sample {trial})\n{row['stderr']}")
        if name == PGO_DYNAMIC and row["stdout"].splitlines() != ["compiled 8192 independent chunks"]:
            raise SystemExit(f"PGO training produced unexpected output on {name} (sample {trial})")
    profiles = sorted(raw.glob("fixture-*.profraw"))
    if len(profiles) != len(jobs):
        raise SystemExit(f"PGO expected {len(jobs)} fixture profiles, found {len(profiles)}")
    merged = directory / "merged.profdata"
    subprocess.run([str(profdata), "merge", "-o", str(merged), *map(str, profiles)],
                   cwd=root, check=True)
    binary, build_command = build(f"-Cprofile-use={merged}", directory / "optimized")
    return binary, build_command, dict(
        training_files=training_files, training_samples=PGO_ITERATIONS, rustc=version.strip(),
        instrument_command=instrument_command, profile=str(merged), profile_sha256=sha256(merged),
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--files", nargs="+", choices=FILES, default=FILES)
    parser.add_argument("--samples", type=int, default=9)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--cpu", type=int, help="Pin benchmark children to one logical processor")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--pgo", action="store_true",
                        help="Train on the portable suite and short generated chunks with constructs.lua held out")
    parser.add_argument("--allocations", action="store_true",
                        help="Count allocator requests in a separate build without cycle timings")
    args = parser.parse_args()
    if args.samples < 1 or args.timeout <= 0 or (args.cpu is not None and args.cpu < 0):
        parser.error("samples and timeout must be positive; CPU must be nonnegative")

    bench = Path(__file__).resolve().parent
    root = bench.parents[2]
    source = bench / "lua5.1-tests"
    manifest = json.loads((bench / "lua51-source.json").read_text(encoding="utf-8"))
    for name, expected in manifest["files"].items():
        if sha256(source / name) != expected:
            raise SystemExit(f"upstream source checksum mismatch: {name}")
    if "files.lua" in args.files and Path("/a/b/c/d").resolve().parent.is_dir():
        raise SystemExit("files.lua requires the /a/b/c parent directory to be absent")

    crate = bench.parent
    paths = sorted((crate / "src").rglob("*.rs"))
    paths += [crate / "Cargo.toml", crate / "build.rs", crate / "examples/official_suite.rs",
              Path(__file__).resolve()]
    if args.pgo:
        paths.append(bench / "pgo" / PGO_DYNAMIC)
    if args.allocations:
        paths.append(crate / "examples/official_suite/allocations.rs")
    elif sys.platform == "linux" and os.uname().machine == "x86_64":
        paths.append(crate / "examples/official_suite/linux_cycles.rs")
    hashes = {p.relative_to(root).as_posix(): sha256(p) for p in paths}
    command = ["cargo", "build", "--release", "--locked", "-p", "deadlib-lua",
               "--example", "official_suite", "-j2"]
    if args.allocations:
        command += ["--features", "bench-allocations"]
    target = (root / Path(os.environ.get("CARGO_TARGET_DIR", "target"))).resolve()
    suffix = ".exe" if sys.platform == "win32" else ""
    directory = target / "lua51-benchmark" / f"{time.time_ns()}-{os.getpid()}"
    directory.mkdir(parents=True)
    environment = {k: v for k, v in os.environ.items()
                   if not k.upper().startswith(("LUA_", "LUAJIT_", "LUNA_", "COMPARISON_SETUP"))}
    pgo = None
    if args.pgo:
        built, command, pgo = profiled_build(root, source, directory, suffix, command,
                                            environment, args.timeout, args.allocations)
    else:
        subprocess.run(command, cwd=root, check=True)
        built = target / f"release/examples/official_suite{suffix}"
    binary = directory / f"official_suite{suffix}"
    shutil.copy2(built, binary)
    jobs = [(name, sample) for sample in range(args.samples) for name in args.files]
    random.Random(51051).shuffle(jobs)
    rows = []
    failed = set()
    output = args.output.resolve() if args.output else directory / "results.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    metadata = dict(
        method="Unchanged official Lua 5.1 sources; fresh VM/process and private source copy per sample; "
               "bootstrap and driver preparation outside clock; dofile compilation/execution inside; "
               "_U=true, T=nil, seeded RNG, C locale; no soft-mode workload reduction",
        source=manifest, source_hashes=hashes, build_command=command,
        binary=str(binary), binary_sha256=sha256(binary), cpu=args.cpu, rows=rows,
    )
    if pgo is not None:
        metadata["profile_guided"] = pgo
    if args.allocations:
        metadata["measurement"] = "successful Rust global allocator requests during dofile"
    for name, sample in jobs:
        if name in failed:
            continue
        row = run_sample(binary, source, directory / "runs", name, sample,
                         environment, args.timeout, args.cpu, args.allocations)
        if row["status"] != "pass":
            failed.add(name)
        rows.append(row)
        output.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        print(name, sample, row["status"], flush=True)
    if any(sha256(root / name) != expected for name, expected in hashes.items()):
        raise SystemExit("crate sources changed during benchmark; results cannot be attributed")
    summary = {}
    for name in args.files:
        passed = [row for row in rows if row["file"] == name and row["status"] == "pass"]
        if not passed:
            continue
        if args.allocations:
            summary[name] = dict(samples=len(passed), **{
                f"median_{key}": statistics.median(row["allocations"][key] for row in passed)
                for key in ("fresh", "resized", "growth_bytes")
            })
            continue
        cycles = [row["cycles"] for row in passed if row["cycles"] is not None]
        summary[name] = dict(
            samples=len(passed),
            median_ms=statistics.median(row["nanoseconds"] for row in passed) / 1e6,
            median_cycles=statistics.median(cycles) if cycles else None,
        )
    metadata["summary"] = summary
    output.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(output)
    if any(r["status"] == "fail" for r in rows):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
