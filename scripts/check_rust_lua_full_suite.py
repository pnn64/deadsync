"""Run unchanged official Lua 5.1 all.lua/main.lua through real standalone CLIs."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import time
import urllib.request

from build_lua_cli_reference import build
from check_rust_lua_suite import SHA256, URL


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def unpack(data: bytes, destination: Path) -> None:
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        if any(not (destination / member.name).resolve().is_relative_to(destination.resolve())
               for member in archive.getmembers()):
            raise SystemExit("suite archive escapes run directory")
        archive.extractall(destination, filter="data")


def run(binary: Path, entry: str, directory: Path, timeout: float) -> dict:
    environment = os.environ.copy()
    for name in list(environment):
        if name.upper() in {"LUA_INIT", "LUA_PATH", "LUA_CPATH"}:
            del environment[name]
    # The official suite README requires extensionless temporary modules.
    environment["LUA_PATH"] = "?;./?.lua"
    environment["PATH"] = str(binary.parent) + os.pathsep + environment.get("PATH", "")
    command = ["lua", entry]
    started = time.monotonic()
    process = subprocess.Popen(command, executable=binary, cwd=directory,
                               env=environment, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=os.name != "nt")
    expired = False
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        expired = True
        if os.name == "nt":
            subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                           capture_output=True, check=False)
        else:
            os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate(timeout=10)
    marker = b"final OK !!!" if entry == "all.lua" else b"OK"
    passed = not expired and process.returncode == 0 and marker in stdout.splitlines()
    return dict(command=command, executable=str(binary), executable_sha256=sha(binary),
                cwd=str(directory), returncode=process.returncode, timeout=expired,
                status="pass" if passed else "timeout" if expired else "fail",
                completion_marker_seen=marker in stdout.splitlines(),
                stdout_hex=stdout.hex(), stderr_hex=stderr.hex(),
                stdout=stdout.decode("utf-8", "replace"),
                stderr=stderr.decode("utf-8", "replace"),
                seconds=time.monotonic() - started)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--run-root", type=Path,
                        help="Parent of retained run directories; use a native filesystem")
    parser.add_argument("--entries", nargs="+", choices=["main.lua", "all.lua"],
                        default=["main.lua", "all.lua"])
    parser.add_argument("--rust-binary", type=Path,
                        help="Development-only prebuilt runner; source provenance is not established")
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("timeout must be positive")
    root = Path(__file__).resolve().parents[1]
    if Path("/a/b/c/d").resolve().parent.is_dir():
        raise SystemExit("unchanged files.lua requires /a/b/c to be absent")
    sources = list((root / "crates/deadlib-lua").rglob("*.rs"))
    sources += [root / "crates/deadlib-lua/Cargo.toml", Path(__file__).resolve(),
                root / "scripts/build_lua_cli_reference.py", root / "scripts/check_rust_lua_suite.py"]

    def hashes() -> dict[str, str]:
        return {p.relative_to(root).as_posix(): sha(p) for p in sorted(sources)}

    before = hashes()
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(root / "target")))
    if not target.is_absolute():
        target = root / target
    target = target.resolve()
    build_command = ["cargo", "build", "--release", "-p", "deadlib-lua", "--bin", "deadlua"]
    if args.rust_binary is None:
        subprocess.run(build_command, cwd=root, check=True)
    rust = args.rust_binary.resolve() if args.rust_binary else target / "release" / (
        "deadlua.exe" if os.name == "nt" else "deadlua")
    reference, metadata = build(root)
    metadata["binary_path"] = str(reference)
    cache = root / "target/lua-reference-suites"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / "lua5.1-tests.tar.gz"
    data = archive.read_bytes() if archive.exists() else urllib.request.urlopen(URL, timeout=30).read()
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise SystemExit("official suite checksum mismatch")
    archive.write_bytes(data)
    runs = (args.run_root.resolve() if args.run_root else root / "target/lua-full-suite") / f"{time.time_ns()}-{os.getpid()}"
    results = []
    for engine, executable in [("reference", reference), ("rust", rust)]:
        for entry in args.entries:
            directory = runs / engine / entry.removesuffix(".lua")
            directory.mkdir(parents=True)
            unpack(data, directory)
            suite = directory / "lua5.1-tests"
            suite_sources = sorted(p for p in suite.rglob("*") if p.is_file())
            suite_before = {p.relative_to(suite).as_posix(): sha(p) for p in suite_sources}
            binaries = directory / "bin"
            binaries.mkdir()
            binary = binaries / ("lua.exe" if os.name == "nt" else "lua")
            shutil.copyfile(executable, binary)
            if os.name != "nt":
                binary.chmod(0o755)
            result = run(binary, entry, suite, args.timeout)
            if suite_before != {p.relative_to(suite).as_posix(): sha(p) for p in suite_sources}:
                raise SystemExit("official suite source changed during execution")
            results.append(dict(engine=engine, entry=entry, suite_source_hashes=suite_before, **result))
            print(engine, entry, result["status"], "exit", result["returncode"], flush=True)
    if before != hashes():
        raise SystemExit("sources changed during full-suite probes")
    result = dict(suite_url=URL, suite_sha256=SHA256, source_hashes=before,
                  platform=sys.platform, timeout_seconds=args.timeout, bootstrap=None,
                  suite_environment=dict(LUA_PATH="?;./?.lua", LUA_INIT=None, LUA_CPATH=None),
                  internal_test_module=False, rust_build_command=build_command,
                  rust_binary=dict(path=str(rust), sha256=sha(rust), prebuilt=args.rust_binary is not None),
                  reference=metadata, results=results,
                  limitations="Stock standalone suite mode without the internal T module. No Lua source, assertion, bootstrap or version output is rewritten. Raw process output is retained, not asserted equal: timing, memory counts and temporary filenames vary. Failures, timeouts and absent completion markers remain failures.")
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(output, flush=True)
    if any(row["status"] != "pass" for row in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
