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
    reference = [p for p in (root / "vendor/lua-src/lua-5.1.5").iterdir()
                 if p.suffix in (".c", ".h")]
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
    reference_heap = {}
    metadata = {}
    for line in run.stdout.splitlines():
        fields = line.split("\t")
        if fields[0] in ("execute", "compile_drop", "startup_drop"):
            metric, engine, workload, sample, repeats, ns, allocs, reallocs, size = fields
            samples.append(dict(metric=metric, engine=engine, workload=workload,
                                sample=int(sample), repeats=int(repeats), ns_per_iteration=int(ns),
                                rust_allocations_total=int(allocs), rust_reallocations_total=int(reallocs),
                                rust_allocated_bytes_total=int(size)))
        elif fields[0] == "reference_live_heap_bytes":
            reference_heap[fields[1]] = int(fields[2])
        elif len(fields) == 2:
            metadata[fields[0]] = fields[1]
    ratios = {}
    for name in sorted({s["workload"] for s in samples if s["metric"] == "execute"}):
        rust = {s["sample"]: s["ns_per_iteration"] for s in samples
                if s["metric"] == "execute" and s["engine"] == "rust" and s["workload"] == name}
        official = {s["sample"]: s["ns_per_iteration"] for s in samples
                    if s["metric"] == "execute" and s["engine"] == "lua51" and s["workload"] == name}
        if len(rust) != 5 or rust.keys() != official.keys():
            raise SystemExit(f"missing paired measurements for {name}")
        ratios[name] = statistics.median(rust[i] / official[i] for i in rust)
    if len(ratios) != 12:
        raise SystemExit(f"expected 12 workload measurements, received {len(ratios)}")
    result = dict(
        command=command, utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        build_and_run_seconds=time.time() - started,
        platform=platform.platform(), machine=platform.machine(),
        processor=os.environ.get("PROCESSOR_IDENTIFIER", platform.processor()),
        rustc=subprocess.check_output(["rustc", "-vV"], text=True),
        git_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root)),
        source_digest=before, reference_source_digest=digest(reference, root),
        source_hashes={p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                       for p in sorted(sources)},
        median_paired_rust_over_lua51=ratios, samples=samples,
        reference_live_heap_bytes=reference_heap, metadata=metadata,
        missing_metrics=["Rust live/peak heap", "process peak RSS", "native C allocation counts",
                         "per-thread CPU cycles", "other Rust engines", "full standard-library workloads"],
    )
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(output)
    print(json.dumps(ratios, indent=2))


if __name__ == "__main__":
    main()
