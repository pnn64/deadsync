"""Record the independent Rust engine's preliminary Lua 5.1 baselines."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time


def digest(paths: list[Path], root: Path) -> str:
    result = hashlib.sha256()
    for path in sorted(paths):
        result.update(path.relative_to(root).as_posix().encode())
        result.update(b"\0")
        result.update(path.read_bytes())
        result.update(b"\0")
    return result.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    sources = list((root / "crates/deadlib-lua").rglob("*.rs"))
    sources += [root / name for name in (
        "Cargo.toml", "Cargo.lock", ".cargo/config.toml",
        "crates/deadlib-lua/Cargo.toml", "scripts/benchmark_rust_lua.py",
    )]
    command = ["cargo", "bench", "-p", "deadlib-lua", "--bench", "baseline"]
    before = digest(sources, root)
    started = time.time()
    run = subprocess.run(command, cwd=root, capture_output=True, text=True)
    output.with_suffix(".tsv").write_text(run.stdout, encoding="utf-8")
    output.with_suffix(".build.log").write_text(run.stderr, encoding="utf-8")
    if run.returncode:
        sys.stderr.write(run.stderr)
        raise SystemExit(run.returncode)
    if before != digest(sources, root):
        raise SystemExit("sources changed during benchmark; results cannot be attributed")
    samples = []
    rust_heap = {}
    rust_program_layout = {}
    metadata = {}
    for line in run.stdout.splitlines():
        fields = line.split("\t")
        if fields[0] in ("execute", "compile_drop", "startup_drop"):
            metric, engine, workload, sample, repeats, ns, allocs, reallocs, size = fields
            samples.append(dict(metric=metric, engine=engine, workload=workload,
                                sample=int(sample), repeats=int(repeats), ns_per_iteration=int(ns),
                                rust_allocations_total=int(allocs), rust_reallocations_total=int(reallocs),
                                rust_allocated_bytes_total=int(size)))
        elif fields[0] == "rust_heap_estimate":
            _, name, phase, live, reserved, peak, collections, reclaimed, userdata = fields
            rust_heap.setdefault(name, {})[phase] = dict(live_bytes=int(live), reserved_bytes=int(reserved),
                observed_peak_reserved_bytes=int(peak), collections=int(collections), reclaimed_objects=int(reclaimed), userdata=int(userdata))
        elif fields[0] == "rust_program_layout":
            rust_program_layout[fields[1]] = dict(instructions=int(fields[2]), entry_registers=int(fields[3]))
        elif len(fields) == 2:
            metadata[fields[0]] = fields[1]
    def medians(metric: str, frontend: bool) -> dict[str, float]:
        result = {}
        for name in sorted({row["workload"] for row in samples
                            if row["metric"] == metric and row["workload"].startswith("frontend_") == frontend}):
            rows = [row for row in samples if row["metric"] == metric and row["workload"] == name]
            if {row["sample"] for row in rows} != set(range(5)) or len(rows) != 5:
                raise SystemExit(f"missing Rust samples for {name}")
            if any(row["engine"] != "rust" for row in rows):
                raise SystemExit(f"unexpected engine for {name}")
            result[name] = statistics.median(row["ns_per_iteration"] for row in rows)
        return result
    execution = medians("execute", False)
    frontend = medians("compile_drop", True)
    if len(execution) != 81 or len(rust_heap) != 81 or len(frontend) != 22:
        raise SystemExit("expected 81 execution workloads with heap observations and 22 frontend workloads")
    if rust_program_layout.keys() != execution.keys() | frontend.keys():
        raise SystemExit("missing program layout measurements")
    result = dict(
        command=command, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        build_and_run_seconds=time.time() - started,
        platform=platform.platform(), machine=platform.machine(),
        processor=os.environ.get("PROCESSOR_IDENTIFIER", platform.processor()),
        rustc=subprocess.check_output(["rustc", "-vV"], text=True),
        git_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root)),
        source_digest=before,
        source_hashes={p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                       for p in sorted(sources)},
        median_execute_ns=execution, samples=samples,
        median_frontend_compile_ns=frontend,
        metadata=metadata,
        rust_heap_estimates=rust_heap,
        rust_program_layout=rust_program_layout,
        missing_metrics=["exact Rust allocator live/peak heap", "process peak RSS",
                         "per-thread CPU cycles", "other Rust engines", "full standard-library workloads"],
    )
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(output)
    print(json.dumps(execution, indent=2))


if __name__ == "__main__":
    main()
