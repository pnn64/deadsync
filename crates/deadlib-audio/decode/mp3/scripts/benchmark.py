"""Snapshot MP3 probes and source, then interleave saved cycle/allocation runs."""
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

REPO = Path(__file__).resolve().parents[5]
PACKAGE = Path("crates/deadlib-audio/decode/mp3")


def invoke(command):
    return subprocess.check_output([str(v) for v in command], cwd=REPO)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verification(command):
    digest = hashlib.sha256()
    size = 0
    with subprocess.Popen(list(map(str, command)), cwd=REPO, stdout=subprocess.PIPE) as process:
        while chunk := process.stdout.read(1 << 20):
            digest.update(chunk)
            size += len(chunk)
        if process.wait():
            raise subprocess.CalledProcessError(process.returncode, command)
    return digest.hexdigest(), size


def capture(root, stage, fixtures, source_stage):
    folder = root / stage
    folder.mkdir(parents=True, exist_ok=False)
    package = folder / PACKAGE
    source = root / source_stage if source_stage else REPO
    for name in ("src", "benches"):
        shutil.copytree((REPO if name == "benches" else source) / PACKAGE / name, package / name)
    shutil.copy2(source / PACKAGE / "Cargo.toml", package / "Cargo.toml")
    shutil.copytree(source / "tests/support", folder / "tests/support")
    profiles = tomllib.loads((REPO / "Cargo.toml").read_text())["profile"]
    manifest = f'[workspace]\nmembers = ["{PACKAGE.as_posix()}"]\nresolver = "3"\n'
    for profile in ("release", "bench"):
        manifest += f"\n[profile.{profile}]\n"
        for key, value in profiles[profile].items():
            manifest += f"{key} = {json.dumps(value)}\n"
    (folder / "Cargo.toml").write_text(manifest, encoding="utf-8")
    # Keep Cargo's package identity, symbol hashes and build paths fixed across
    # production stages. Save a separate executable before the next rebuild.
    build_root = root / "build"
    for name in ("src", "benches"):
        shutil.copytree(package / name, build_root / PACKAGE / name, dirs_exist_ok=True)
    shutil.copy2(package / "Cargo.toml", build_root / PACKAGE / "Cargo.toml")
    shutil.copytree(folder / "tests/support", build_root / "tests/support", dirs_exist_ok=True)
    shutil.copy2(folder / "Cargo.toml", build_root / "Cargo.toml")
    for path in (build_root / PACKAGE).rglob("*.rs"):
        path.touch()
    build = invoke(["cargo", "bench", "--offline", "--manifest-path", build_root / "Cargo.toml",
                    "--bench", "kernels", "--no-run", "--message-format=json"])
    (folder / "build.jsonl").write_bytes(build)
    artifacts = [json.loads(line) for line in build.splitlines()]
    artifact, = [a for a in artifacts if a.get("reason") == "compiler-artifact"
                 and a.get("executable") and a["target"]["name"] == "kernels"]
    binary = folder / Path(artifact["executable"]).name
    shutil.copy2(artifact["executable"], binary)
    output_sha, output_bytes = verification([binary, "verify", *fixtures])
    record = dict(binary=str(binary), binary_sha256=sha(binary), profile=artifact["profile"],
                  source_stage=source_stage, build_root=str(build_root),
                  head=invoke(["git", "rev-parse", "HEAD"]).decode().strip(),
                  rustc=invoke(["rustc", "-Vv"]).decode(),
                  config=tomllib.loads((REPO / ".cargo/config.toml").read_text()),
                  rustflags={k: os.environ.get(k) for k in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")},
                  sources={str(p.relative_to(folder)): sha(p) for p in folder.rglob("*.rs")
                           if "target" not in p.relative_to(folder).parts},
                  fixtures={str(p): sha(p) for p in fixtures},
                  output_sha256=output_sha, output_bytes=output_bytes)
    initial = invoke([binary, *fixtures])
    record["workloads"] = re.findall(r"([^\r\n]+): cycles/op [\d.]+; ns/op [\d.]+", initial.decode())
    assert record["workloads"], "no benchmark measurements"
    (folder / "manifest.json").write_text(json.dumps(record, indent=2) + "\n")
    (folder / "initial.log").write_bytes(initial)
    print(initial.decode(), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=REPO / "target/mp3-perf-20261028")
    parser.add_argument("--capture")
    parser.add_argument("--source-stage", help="rebuild saved production source with current probes through the fixed build path")
    parser.add_argument("--fixture", action="append", type=Path, default=[])
    parser.add_argument("--stage", action="append", default=[])
    parser.add_argument("--verify-corpus", action="append", type=Path, default=[],
                        help="compare exact PCM for every MP3 in these directories instead of timing")
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if args.capture:
        capture(root, args.capture, [p.resolve() for p in args.fixture], args.source_stage)
    if not args.stage:
        return
    if not args.output or (not args.verify_corpus and args.runs < 3):
        parser.error("comparison needs --output and at least three runs")
    manifests = {s: json.loads((root / s / "manifest.json").read_text()) for s in args.stage}
    first = next(iter(manifests.values()))
    assert len({m["output_sha256"] for m in manifests.values()}) == 1, "output bits differ"
    for stage, record in manifests.items():
        assert record["fixtures"] == first["fixtures"]
        assert record["profile"] == first["profile"]
        assert record["rustc"] == first["rustc"]
        assert record["config"] == first["config"]
        assert record["rustflags"] == first["rustflags"]
        assert record.get("build_root") == first.get("build_root")
        assert sha(Path(record["binary"])) == record["binary_sha256"]
        for source, digest in record["sources"].items():
            assert sha(root / stage / source) == digest
            if "/src/" not in source.replace("\\", "/"):
                assert digest == first["sources"][source], source
        for fixture, digest in record["fixtures"].items():
            assert sha(Path(fixture)) == digest
    if args.verify_corpus:
        fixtures = sorted(p.resolve() for directory in args.verify_corpus for p in directory.glob("*.mp3"))
        assert fixtures, "no corpus MP3s found"
        outputs = {}
        for stage, record in manifests.items():
            command = [record["binary"], "verify", *map(str, fixtures)]
            digest, size = verification(command)
            outputs[stage] = dict(command=command, sha256=digest, bytes=size,
                                  binary_sha256=record["binary_sha256"])
            print(f"Verified {stage}: {len(fixtures)} files; {size} bytes; SHA-256 {digest}", flush=True)
        assert len({v["sha256"] for v in outputs.values()}) == 1, "corpus PCM differs"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(dict(passed=True, files=len(fixtures), outputs=outputs,
                                              fixtures={str(p): sha(p) for p in fixtures}), indent=2) + "\n")
        return
    rows = []
    timing = re.compile(r"([^\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"([^\r\n]+): allocation Churn \{ ([^}]+) \}")
    for run in range(args.runs):
        order = args.stage[run % len(args.stage):] + args.stage[:run % len(args.stage)]
        for stage in order:
            output = invoke([manifests[stage]["binary"], *first["fixtures"]]).decode()
            counts = {label: dict(re.findall(r"(\w+): (\d+)", fields))
                      for label, fields in allocation.findall(output)}
            measurements = timing.findall(output)
            assert len(measurements) == len(counts)
            if "workloads" in first:
                assert {m[0] for m in measurements} == set(first["workloads"])
            else:
                assert len(measurements) == 13 + len(first["fixtures"])
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
        values = []
        for stage in args.stage:
            group = [r for r in rows if r["workload"] == label and r["stage"] == stage]
            cycles = [r["cycles_per_op"] for r in group]
            median = statistics.median(cycles)
            values.append(median)
            summary.append(dict(workload=label, stage=stage, median_cycles=median,
                                min_cycles=min(cycles), max_cycles=max(cycles),
                                mad_cycles=statistics.median(abs(v - median) for v in cycles),
                                median_ns=statistics.median(r["ns_per_op"] for r in group),
                                heap={k: group[0][k] for k in counts[label]}))
        print(f"{label}: " + " -> ".join(f"{v:.2f}" for v in values)
              + f" cycles/op; {values[0] / values[-1]:.3f}x CPU throughput")
    args.output.with_suffix(".json").write_text(
        json.dumps(dict(stages=manifests, runs=args.runs, summary=summary), indent=2) + "\n")


if __name__ == "__main__":
    main()
