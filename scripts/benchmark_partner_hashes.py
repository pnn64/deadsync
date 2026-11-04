"""Archive paired measurements for partner dispatch, hash masking and comment bypass."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

import benchmark_partner_charts as harness

ROOT = harness.ROOT
ARCHIVE = Path(os.environ.get("DEADSYNC_PARTNER_HASH_ARCHIVE", ROOT / "target/partner-hashes"))
harness.ARCHIVE = ARCHIVE


def full_tests(stage):
    # The previous complete snapshot contains the unchanged dependency tree.
    # Verify the Git delta is covered by these overlays before reusing its build cache.
    base = json.loads((ROOT / "docs/performance/2026-11-04-partner-charts-full-release.manifest.json").read_text())
    archive = Path(base["source_archive"])
    assert harness.sha(archive) == base["source_archive_sha256"]
    work = archive.parent.parent / "full-workspace"
    assert (work / "Cargo.toml").exists(), "Run the original full-suite snapshot first"
    package = Path("crates/deadsync-simfile")
    sources = [package / "src/partner_charts.rs"]
    sources += sorted(path.relative_to(ROOT) for path in (ROOT / package / "src/partner_charts").glob("*.rs"))
    changes = subprocess.check_output(["git", "-C", str(ROOT), "diff", "--name-only", base["git_revision"], "HEAD", "--", "crates", "tests", "vendor", "Cargo.lock"], text=True).splitlines()
    assert set(changes) <= {path.as_posix() for path in sources}, changes
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    overlays = {}
    for relative in sources:
        data = (ROOT / relative).read_bytes()
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists() or destination.read_bytes() != data:
            destination.write_bytes(data)
        saved = out / "source" / relative
        saved.parent.mkdir(parents=True, exist_ok=True)
        saved.write_bytes(data)
        overlays[relative.as_posix()] = hashlib.sha256(data).hexdigest()
    env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static")
    command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-simfile", "--lib", "--release", "--", "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
    log = (out / "correctness.log").read_text()
    binaries = list((work / "target/release/deps").glob("deadsync_simfile-*.exe"))
    binary = max(binaries, key=lambda p: p.stat().st_mtime)
    shutil.copyfile(binary, out / binary.name)
    metadata = dict(command=command, base_revision=base["git_revision"],
                    source_archive=str(archive), source_archive_sha256=harness.sha(archive),
                    checkout_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                    overlays=overlays, binary_sha256=harness.sha(out / binary.name),
                    correctness_sha256=harness.sha(out / "correctness.log"), exit_code=result.returncode,
                    rustflags=env["RUSTFLAGS"], test_temp=env.get("TEMP"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    lockfile=(work / "Cargo.lock").read_text(),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-04-partner-hashes-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2))
    if result.returncode:
        raise RuntimeError(log[-10000:])
    print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--baseline")
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    if args.full and args.debug:
        parser.error("--debug applies to the focused kernels; full validation uses release")
    if args.full:
        full_tests(args.stage)
    else:
        harness.run(args.stage, args.runs, args.debug, args.baseline,
                    report_prefix="partner-hashes", benchmark="projection::benchmark_partner_hashes",
                    extra_source=Path(__file__).resolve())
