"""Save Ogg source/binaries, then interleave CPU-cycle and allocation probes."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import tomllib

REPO = Path(__file__).resolve().parents[4]
PACKAGE = Path("crates/deadlib-audio/decode/ogg")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def invoke(command):
    return subprocess.check_output([str(v) for v in command], cwd=REPO)


def capture(root, name, revision):
    folder = root / name
    folder.mkdir(parents=True, exist_ok=False)
    package = folder / PACKAGE
    for name in ("src", "benches"):
        if name == "src" and revision:
            files = invoke(["git", "ls-tree", "-r", "--name-only", revision, PACKAGE / name]).decode().splitlines()
            for source in files:
                destination = folder / source
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(invoke(["git", "show", f"{revision}:{source}"]))
        else:
            shutil.copytree(REPO / PACKAGE / name, package / name)
    shutil.copy2(REPO / PACKAGE / "Cargo.toml", package / "Cargo.toml")
    shutil.copytree(REPO / "tests/support", folder / "tests/support")
    profiles = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["profile"]
    manifest = f'[workspace]\nmembers = ["{PACKAGE.as_posix()}"]\nresolver = "3"\n'
    for profile in ("release", "bench"):
        manifest += f"\n[profile.{profile}]\n"
        for key, value in profiles[profile].items():
            manifest += f"{key} = {json.dumps(value)}\n"
    (folder / "Cargo.toml").write_text(manifest, encoding="utf-8")
    build = invoke(["cargo", "bench", "--offline", "--manifest-path", folder / "Cargo.toml",
                    "--bench", "kernels", "--no-run", "--message-format=json"])
    (folder / "build.jsonl").write_bytes(build)
    artifacts = [json.loads(line) for line in build.splitlines()]
    artifact, = [a for a in artifacts if a.get("reason") == "compiler-artifact"
                 and a.get("executable") and a["target"]["name"] == "kernels"]
    binary = Path(artifact["executable"])
    record = dict(binary=str(binary), binary_sha256=sha(binary), profile=artifact["profile"],
                  source_revision=revision,
                  head=invoke(["git", "rev-parse", "HEAD"]).decode().strip(),
                  rustc=invoke(["rustc", "-Vv"]).decode(),
                  cargo_config=tomllib.loads((REPO / ".cargo/config.toml").read_text()),
                  rustflags={k: os.environ.get(k) for k in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")},
                  sources={str(p.relative_to(folder)): sha(p) for p in folder.rglob("*.rs")
                           if "target" not in p.relative_to(folder).parts},
                  wire_sha256=hashlib.sha256(invoke([binary, "verify"])).hexdigest())
    (folder / "manifest.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"Captured {folder}", flush=True)
    return binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=REPO / "target/ogg-kernels-20261028")
    parser.add_argument("--capture")
    parser.add_argument("--source-revision", help="use historical production source with today's identical probes")
    parser.add_argument("--stage", action="append", default=[])
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if args.capture:
        binary = capture(root, args.capture, args.source_revision)
        (root / args.capture / "initial.log").write_bytes(invoke([binary]))
    if not args.stage:
        return
    if not args.output or args.runs < 3:
        parser.error("comparison needs --output and at least three runs")
    manifests = {s: json.loads((root / s / "manifest.json").read_text()) for s in args.stage}
    assert len({m["wire_sha256"] for m in manifests.values()}) == 1, "writer bytes differ"
    assert all(m["profile"] == next(iter(manifests.values()))["profile"] for m in manifests.values())
    for stage, m in manifests.items():
        assert sha(Path(m["binary"])) == m["binary_sha256"]
        for source, digest in m["sources"].items():
            assert sha(root / stage / source) == digest
    controls = [p for p in next(iter(manifests.values()))["sources"]
                if "/src/" not in p.replace("\\", "/")]
    for source in controls:
        assert len({m["sources"][source] for m in manifests.values()}) == 1, source
    rows = []
    timing = re.compile(r"([^\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"([^\r\n]+): allocation Churn \{ ([^}]+) \}")
    for run in range(args.runs):
        order = args.stage[run % len(args.stage):] + args.stage[:run % len(args.stage)]
        for stage in order:
            output = invoke([manifests[stage]["binary"]]).decode()
            counts = {label: dict(re.findall(r"(\w+): (\d+)", fields))
                      for label, fields in allocation.findall(output)}
            measurements = timing.findall(output)
            assert len(measurements) == len(counts) == 24
            for label, cycles, ns in measurements:
                rows.append(dict(run=run, stage=stage, workload=label,
                                 cycles_per_op=float(cycles), ns_per_op=float(ns),
                                 **{k: int(v) for k, v in counts[label].items()}))
            print(f"Run {run + 1}/{args.runs}: {stage}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    summary = []
    for label in counts:
        values = [statistics.median(r["cycles_per_op"] for r in rows
                                    if r["workload"] == label and r["stage"] == stage)
                  for stage in args.stage]
        print(f"{label}: " + " -> ".join(f"{v:.2f}" for v in values)
              + f" cycles/op; {values[0] / values[-1]:.3f}x CPU throughput")
        for stage in args.stage:
            group = [r for r in rows if r["workload"] == label and r["stage"] == stage]
            cycles = [r["cycles_per_op"] for r in group]
            median = statistics.median(cycles)
            summary.append(dict(workload=label, stage=stage, median_cycles=median,
                                min_cycles=min(cycles), max_cycles=max(cycles),
                                mad_cycles=statistics.median(abs(v - median) for v in cycles),
                                median_ns=statistics.median(r["ns_per_op"] for r in group),
                                heap={k: group[0][k] for k in counts[label]}))
    for stage, m in manifests.items():
        assert sha(Path(m["binary"])) == m["binary_sha256"]
        for source, digest in m["sources"].items():
            assert sha(root / stage / source) == digest
    args.output.with_suffix(".json").write_text(
        json.dumps(dict(stages=manifests, runs=args.runs, summary=summary), indent=2) + "\n",
        encoding="utf-8")


if __name__ == "__main__":
    main()
