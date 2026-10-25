"""Probe pinned official Lua 5.1 files in isolated Rust/reference processes."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import time
import urllib.request

URL = "https://www.lua.org/tests/lua5.1-tests.tar.gz"
SHA256 = "49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad"
FILES = ["constructs.lua", "locals.lua", "calls.lua", "events.lua", "nextvar.lua",
         "literals.lua", "closure.lua", "vararg.lua", "strings.lua", "pm.lua", "sort.lua", "math.lua", "gc.lua", "db.lua"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--files", nargs="+", default=FILES)
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    sources = list((root / "crates/deadlib-lua").rglob("*.rs"))
    sources += [root / "crates/deadlib-lua/Cargo.toml", Path(__file__).resolve()]
    def source_hashes() -> dict[str, str]:
        return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    before = source_hashes()
    cache = root / "target/lua-reference-suites"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / "lua5.1-tests.tar.gz"
    data = archive.read_bytes() if archive.exists() else urllib.request.urlopen(URL).read()
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise SystemExit("official suite checksum mismatch")
    archive.write_bytes(data)
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as packed:
        for entry in packed.getmembers():
            if not (cache / entry.name).resolve().is_relative_to(cache.resolve()):
                raise SystemExit("archive path escapes test directory")
        packed.extractall(cache, filter="data")
    command = ["cargo", "build", "--release", "-p", "deadlib-lua", "--example", "official_suite"]
    subprocess.run(command, cwd=root, check=True)
    binary = root / "target/release/examples" / ("official_suite.exe" if sys.platform == "win32" else "official_suite")
    suite = cache / "lua5.1-tests"
    results = []
    for filename in args.files:
        source = (suite / filename).resolve()
        if not source.is_relative_to(suite.resolve()) or not source.is_file():
            raise SystemExit(f"invalid suite file: {filename}")
        row = {"file": filename, "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
        for engine in ("reference", "rust"):
            started = time.monotonic()
            try:
                run = subprocess.run([str(binary), engine, str(source)], cwd=suite,
                                     capture_output=True, timeout=args.timeout)
                row[engine] = dict(status="pass" if run.returncode == 0 else "fail",
                                   returncode=run.returncode, stdout=run.stdout.decode("utf-8", "replace"),
                                   stderr=run.stderr.decode("utf-8", "replace"))
            except subprocess.TimeoutExpired as error:
                row[engine] = dict(status="timeout", stdout=(error.stdout or b"").decode("utf-8", "replace"),
                                   stderr=(error.stderr or b"").decode("utf-8", "replace"))
            row[engine]["seconds"] = time.monotonic() - started
        results.append(row)
        print(filename, "reference=" + row["reference"]["status"], "rust=" + row["rust"]["status"], flush=True)
    if before != source_hashes():
        raise SystemExit("sources changed during suite probes; results cannot be attributed")
    result = dict(suite_url=URL, suite_sha256=SHA256, bootstrap="_U=true; function Message(s) print(s) end",
                  command=command, results=results, instruction_limit=100_000_000,
                  file_timeout_seconds=args.timeout,
                  source_hashes=before,
                  limitations="Files are independent probes, not all.lua, the complete suite, or internal C API tests. No tests are removed or rewritten.")
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(output)
    if any(row[engine]["status"] != "pass" for row in results for engine in ("reference", "rust")):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
