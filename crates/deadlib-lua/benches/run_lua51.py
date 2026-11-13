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


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--files", nargs="+", choices=FILES, default=FILES)
    parser.add_argument("--samples", type=int, default=9)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--cpu", type=int, help="Pin benchmark children to one logical processor")
    parser.add_argument("--output", type=Path)
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
    paths += [crate / "Cargo.toml", crate / "build.rs", crate / "examples/official_suite.rs"]
    hashes = {p.relative_to(root).as_posix(): sha256(p) for p in paths}
    command = ["cargo", "build", "--release", "--locked", "-p", "deadlib-lua",
               "--example", "official_suite", "-j2"]
    subprocess.run(command, cwd=root, check=True)
    target = (root / Path(os.environ.get("CARGO_TARGET_DIR", "target"))).resolve()
    suffix = ".exe" if sys.platform == "win32" else ""
    directory = target / "lua51-benchmark" / f"{time.time_ns()}-{os.getpid()}"
    directory.mkdir(parents=True)
    binary = directory / f"official_suite{suffix}"
    shutil.copy2(target / f"release/examples/official_suite{suffix}", binary)
    environment = {k: v for k, v in os.environ.items()
                   if not k.upper().startswith(("LUA_", "LUAJIT_", "LUNA_", "COMPARISON_SETUP"))}
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
    for name, sample in jobs:
        if name in failed:
            continue
        cwd = directory / "runs" / name / str(sample)
        shutil.copytree(source, cwd)
        # Cargo/Git do not retain the archive's empty module-search directory.
        (cwd / "libs/P1").mkdir(parents=True, exist_ok=True)
        child_command = [str(binary), "rust-bench", str(cwd / name)]
        child = subprocess.Popen(child_command, cwd=cwd, env=environment,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE,
                                 creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0)
        if args.cpu is not None:
            if sys.platform == "win32":
                affinity = ctypes.windll.kernel32.SetProcessAffinityMask
                affinity.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
                if not affinity(int(child._handle), 1 << args.cpu):
                    child.kill()
                    child.communicate()
                    raise ctypes.WinError()
            else:
                os.sched_setaffinity(child.pid, {args.cpu})
        expired = False
        try:
            stdout, stderr = child.communicate(timeout=args.timeout)
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
        passed = not expired and child.returncode == 0 and record is not None
        if passed and name in RETURNS:
            passed = record[4] == struct.pack(">d", float(RETURNS[name])).hex()
        platform_failure = (sys.platform == "win32" and name == "files.lua"
                            and child.returncode == 1 and not expired
                            and any(line.startswith("FAIL\tRuntime\t59\t")
                                    and line.endswith(": Permission denied") for line in error.splitlines()))
        row = dict(file=name, sample=sample, command=child_command, exit=child.returncode,
                   timeout=expired, status="pass" if passed else
                   "platform-expected-failure" if platform_failure else "fail",
                   stdout=stdout.decode("utf-8", "replace"), stderr=error)
        if passed:
            row.update(nanoseconds=int(record[2]),
                       cycles=int(record[3]) if record[3] != "unavailable" else None,
                       returned=record[4])
        else:
            failed.add(name)
        rows.append(row)
        output.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        print(name, sample, row["status"], flush=True)
    if any(sha256(root / name) != expected for name, expected in hashes.items()):
        raise SystemExit("crate sources changed during benchmark; results cannot be attributed")
    metadata["summary"] = {
        name: dict(samples=len(times), median_ms=statistics.median(times) / 1e6)
        for name in args.files
        if (times := [r["nanoseconds"] for r in rows if r["file"] == name and r["status"] == "pass"])
    }
    output.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(output)
    if any(r["status"] == "fail" for r in rows):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
