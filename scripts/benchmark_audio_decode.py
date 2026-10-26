"""Archive and compare production audio-decode kernels by CPU cycles and allocations."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import re
import shutil
import statistics
import subprocess


TARGETS = {
    "resampler": ("resampler_output_kernel", "benchmark_resampler_output", 20),
    "mapped": ("mapped_output_kernel", "benchmark_mapped_output", 80),
    "planar": ("planar_kernel", "benchmark_planar_kernel", 12),
    "append": ("planar_kernel", "benchmark_mapping_kernel", 28),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("target/audio-decode-kernels"))
    parser.add_argument("--capture", help="archive executables from a Cargo JSON build log")
    parser.add_argument("--build-log", type=Path)
    parser.add_argument("--stage", action="append", default=[])
    parser.add_argument("--target", action="append", choices=TARGETS)
    parser.add_argument("--runs", type=int, default=6)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.capture:
        if not args.build_log:
            parser.error("--capture requires --build-log")
        data = args.build_log.read_bytes()
        build_log = data.decode("utf-16" if data.startswith(b"\xff\xfe") else "utf-8-sig")
        folder = args.root / args.capture
        folder.mkdir(parents=True, exist_ok=False)
        binaries = {}
        for line in build_log.splitlines():
            artifact = json.loads(line)
            if artifact.get("reason") != "compiler-artifact" or not artifact.get("executable"):
                continue
            if artifact["package_id"].split("#")[0].endswith("deadlib-audio-decode"):
                source = Path(artifact["executable"])
                dest = folder / (artifact["target"]["name"] + source.suffix)
                shutil.copy2(source, dest)
                binaries[dest.name] = digest(dest)
        if not binaries:
            raise ValueError("no audio-decode executables found")
        sources = [*Path("crates/deadlib-audio-decode").rglob("*.rs"),
                   Path("Cargo.lock"), Path("Cargo.toml"), Path(".cargo/config.toml"),
                   Path("tests/support/kernel_bench.rs"), Path("tests/support/perf.rs")]
        manifest = dict(
            stage=args.capture, binaries=binaries,
            sources={str(path).replace("\\", "/"): digest(path) for path in sources},
            rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
            head=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        )
        (folder / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        print(f"archived {len(binaries)} executables in {folder}")
    if not args.stage:
        return
    if not args.output or args.runs < 1:
        parser.error("comparison requires --output and positive --runs")
    logs = args.root / "runs" / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    label_pattern = r"((?:resampler|mapped|planar|mapping)/[^:\r\n]+)"
    timing = re.compile(label_pattern + r": cycles/op ([\d.]+); ns/op ([\d.]+)")
    churn = re.compile(label_pattern + r": allocation Churn \{ ([^}]+) \}")
    rows = []
    manifests = {stage: json.loads((args.root / stage / "manifest.json").read_text(encoding="utf-8"))
                 for stage in args.stage}
    for stage, manifest in manifests.items():
        for name, expected_hash in manifest["binaries"].items():
            if digest(args.root / stage / name) != expected_hash:
                raise ValueError(f"{stage}/{name}: archived executable changed")
    for run in range(args.runs):
        order = args.stage[run % len(args.stage):] + args.stage[:run % len(args.stage)]
        for stage in order:
            for target in args.target or TARGETS:
                executable, test, expected = TARGETS[target]
                result = subprocess.run(
                    [str((args.root / stage / (executable + ".exe")).resolve()),
                     test, "--ignored", "--nocapture", "--test-threads=1"],
                    capture_output=True, text=True, encoding="utf-8", check=True,
                )
                (logs / f"{run}-{stage}-{target}.log").write_text(result.stdout, encoding="utf-8")
                measurements = timing.findall(result.stdout)
                allocations = {label: dict(re.findall(r"(\w+): (\d+)", values))
                               for label, values in churn.findall(result.stdout)}
                if len(measurements) != expected or len(allocations) != expected:
                    raise ValueError(f"{target}: expected {expected} cycle and allocation probes")
                for label, cycles, nanos in measurements:
                    frames = int(re.search(r"frames=(\d+)", label)[1])
                    rows.append(dict(run=run, stage=stage, workload=label,
                                     cycles_per_op=float(cycles), ns_per_op=float(nanos),
                                     frames_per_cycle=frames / float(cycles),
                                     binary_sha256=manifests[stage]["binaries"][executable + ".exe"],
                                     **allocations[label]))
                print(f"run {run + 1}/{args.runs}: {stage} {target}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    for label in sorted({row["workload"] for row in rows}):
        medians = [statistics.median(row["cycles_per_op"] for row in rows
                                    if row["stage"] == stage and row["workload"] == label)
                   for stage in args.stage]
        print(f"{label}: " + " -> ".join(f"{value:.2f}" for value in medians)
              + f" cycles/op; {medians[0] / medians[-1]:.3f}x CPU throughput")


if __name__ == "__main__":
    main()
