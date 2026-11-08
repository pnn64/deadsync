"""Compare codec CPU cycles on Windows in one release binary.

Example: py scripts/benchmark_bincode.py --before HEAD --runs 3 \
    --output target/bincode-perf/results.json
Source snapshots and build products stay under target; no workspace lock changes.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("CPU measurements use Windows QueryThreadCycleTime")
    if args.runs < 1:
        parser.error("--runs must be positive")
    root = Path(__file__).resolve().parents[1]
    revision = subprocess.check_output(
        ["git", "rev-parse", args.before], cwd=root, text=True
    ).strip()
    work = root / "target" / "bincode-compare" / revision[:12]
    work.mkdir(parents=True, exist_ok=True)
    archive = subprocess.check_output(
        ["git", "archive", revision, "crates/deadlib-bincode"], cwd=root
    )
    # Read regular files only, under the explicitly selected crate directory.
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        for member in source.getmembers():
            if not member.isfile():
                continue
            relative = Path(member.name).relative_to("crates/deadlib-bincode")
            if ".." in relative.parts:
                raise ValueError("snapshot path escapes the codec")
            destination = work / "before" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(source.extractfile(member).read())
    before = work / "before"
    manifest = before / "Cargo.toml"
    manifest.write_text(
        manifest.read_text().replace('name = "deadlib-bincode"', 'name = "bincode-before"')
        .replace('name = "bincode"', 'name = "bincode_before"')
    )
    library = before / "src/lib.rs"
    library.write_text(library.read_text().replace(
        '#![crate_name = "bincode"]', '#![crate_name = "bincode_before"]'
    ))
    current = root / "crates/deadlib-bincode"
    # The derive dependency is identical for both copies and is not benchmarked.
    manifest.write_text(manifest.read_text().replace(
        'path = "derive"', f'path = "{(current / "derive").as_posix()}"'
    ))
    (work / "Cargo.toml").write_text(f'''[package]
name = "bincode-comparison"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
bincode = {{ package = "deadlib-bincode", path = "{current.as_posix()}" }}
bincode-before = {{ path = "before" }}
[[bin]]
name = "paired"
path = "{(root / 'scripts/bincode_paired.rs').as_posix()}"
[profile.release]
opt-level = 3
lto = true
''')
    subprocess.run(
        ["cargo", "build", "--offline", "--release", "--manifest-path", str(work / "Cargo.toml")],
        cwd=root, check=True,
    )
    binary = work / "target/release" / ("paired.exe" if os.name == "nt" else "paired")
    pattern = re.compile(r"^(.+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+);", re.M)
    samples = []
    for run in range(args.runs):
        result = subprocess.check_output([str(binary)], cwd=root, text=True)
        (work / f"run-{run + 1}.log").write_text(result)
        measured = pattern.findall(result)
        if len(measured) != 55:
            raise ValueError(f"expected 55 workloads, got {len(measured)}")
        samples.extend({
            "run": run + 1, "workload": label,
            "before_cycles": float(old_cpu), "after_cycles": float(new_cpu),
            "before_ns": float(old_ns), "after_ns": float(new_ns),
        } for label, old_cpu, new_cpu, old_ns, new_ns in measured)
        print(f"completed run {run + 1}/{args.runs}", flush=True)
    summary = []
    for label in sorted({row["workload"] for row in samples}):
        rows = [row for row in samples if row["workload"] == label]
        old, new = [statistics.median(row[key] for row in rows)
                    for key in ["before_cycles", "after_cycles"]]
        ratios = [row["before_cycles"] / row["after_cycles"] for row in rows]
        summary.append({"workload": label, "before_cycles": old, "after_cycles": new,
                        "throughput_ratio": statistics.median(ratios),
                        "ratio_range": [min(ratios), max(ratios)]})
        print(f"{label}: {old:.2f} -> {new:.2f} cycles/op; median {statistics.median(ratios):.3f}x")
    hashes = {path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
              for path in sorted(current.rglob("*.rs"))}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({
        "before_revision": revision,
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "current_source_sha256": hashes,
        "method": "LTO release; kernel_bench paired medians; pinned Windows thread; warmed allocation checks",
        "allocations": "All 55 before/after workloads assert zero warmed allocation churn",
        "summary": summary, "samples": samples,
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
