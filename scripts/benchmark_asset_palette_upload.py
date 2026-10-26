"""Build unchanged production decoder/queue bodies and compare CPU/allocation results."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import re
import statistics
import subprocess
import os


def function(source, name):
    match = re.search(r"(?:pub )?fn " + name + r"(?:<[^\n]+>)?\(", source)
    start = source.index("{", match.start())
    end, depth = start + 1, 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[match.start():end]


def build(args):
    root = Path.cwd()
    directory = root / "target/asset-palette-upload"
    directory.mkdir(parents=True, exist_ok=True)
    crate = root / "crates/deadlib-assets"
    decode = (crate / "src/decode.rs").read_text(encoding="utf-8")
    upload = (crate / "src/upload.rs").read_text(encoding="utf-8")
    # Omit the unrelated existing tests; keep production code and the new tests.
    upload = upload[:upload.index("#[cfg(test)]\nmod tests {")]
    names = ["check_image_alloc", "image_limit", "decode_error", "gif_error", "gif_reader",
             "decode_gif", "decode_indexed_bmp", "unpack_palette", "key_rgba"]
    bodies = {name: function(decode, name) for name in names}
    source = '#![allow(dead_code)]\n'
    for name, path in [("perf", root / "tests/support/perf.rs"),
                       ("perf_fixture", crate / "src/perf_fixture.rs")]:
        source += f'#[path = "{path.as_posix()}"] mod {name};\n'
    prefix = source
    source += 'mod decode {\nuse image::RgbaImage;\nuse std::{io::{BufReader, Read, Seek, SeekFrom}, path::Path};\n'
    source += "\n\n".join(bodies.values())
    source += f'\n#[path = "{(crate / "src/decode/palette_perf.rs").as_posix()}"] mod palette_perf;\n}}\n'
    # The queue's existing path attribute resolves relative to this snapshot.
    upload = upload.replace('#[path = "upload_perf.rs"]',
                            f'#[path = "{(crate / "src/upload_perf.rs").as_posix()}"]')
    if "mod upload_perf;" not in upload:
        upload += f'\n#[path = "{(crate / "src/upload_perf.rs").as_posix()}"] mod upload_perf;\n'
    source += "mod upload {\n" + upload
    if args.paired:
        source += f'\ninclude!("{(crate / "src/upload_original.rs").as_posix()}");\n'
        source += f'#[path = "{(root / "tests/support/asset_upload_pairs.rs").as_posix()}"] mod pairs;\n'
    source += "\n}\n"
    if args.cache:
        dynamic = (crate / "src/dynamic.rs").read_text(encoding="utf-8")
        bodies = {name: function(dynamic, name) for name in
                  ["banner_cache_opthash", "banner_path_hash", "dynamic_image_cache_path_for"]}
        source = prefix + 'mod dynamic {\nuse std::{fs, path::{Path, PathBuf}, hash::Hasher};\nuse twox_hash::XxHash64;\n'
        source += '#[derive(Clone, Copy, Debug)] pub struct BannerCacheOptions { pub enabled: bool }\n'
        source += "\n\n".join(bodies.values())
        source += f'\n#[path = "{(crate / "src/cache_filename_perf.rs").as_posix()}"] mod cache_filename_perf;\n}}\n'
    snapshot = directory / (args.stage + ".rs")
    snapshot.write_bytes(source.encode("utf-8"))
    dependencies = json.loads(args.dependencies.read_text(encoding="utf-8"))["direct_dependencies"]
    argv = ["rustc", "--test", "--edition=2024", "-C", "opt-level=3", "-C", "lto=fat",
            "-C", "codegen-units=1", "-C", "target-feature=+crt-static", str(snapshot),
            "-o", str(directory / (args.stage + ".exe"))]
    used = {}
    for name in (["memchr", "twox_hash"] if args.cache else ["image", "gif", "rustc_hash", "deadlib_render_core", "deadlib_video"]):
        info = dependencies[name]
        path = Path(info["path"])
        assert hashlib.sha256(path.read_bytes()).hexdigest() == info["sha256"], name
        argv += ["--extern", name + "=" + str(path), "-L", "dependency=" + str(path.parent)]
        used[name] = info
    env = os.environ | {"CARGO_MANIFEST_DIR": str(crate)}
    log = directory / (args.stage + ".build.log")
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT, env=env)
    if result.returncode:
        print(log.read_text(encoding="utf-8")[-6000:])
        raise SystemExit(result.returncode)
    paths = [crate / ("src/" + path) for path in ["decode.rs", "upload.rs", "decode/palette_perf.rs",
             "decode/palette_original.rs", "upload_perf.rs", "upload_original.rs", "perf_fixture.rs"]]
    paths += [root / "tests/support/perf.rs", root / "tests/support/kernel_bench.rs"]
    if args.paired:
        paths += [root / "tests/support/asset_upload_pairs.rs"]
    if args.cache:
        paths = [crate / ("src/" + path) for path in ["dynamic.rs", "cache_filename_perf.rs", "cache_filename_original.rs", "perf_fixture.rs"]]
        paths += [root / "tests/support/perf.rs", root / "tests/support/kernel_bench.rs"]
    metadata = dict(argv=argv, dependencies=used, source_sha256={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
                    extracted_body_sha256={name: hashlib.sha256(body.encode()).hexdigest() for name, body in bodies.items()},
                    binary_sha256=hashlib.sha256((directory / (args.stage + ".exe")).read_bytes()).hexdigest())
    (directory / (args.stage + ".build.json")).write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(args.stage + " built: " + metadata["binary_sha256"], flush=True)


def compare(args):
    logs = Path("target/asset-palette-upload") / args.output.stem
    logs.mkdir(parents=True, exist_ok=True)
    timing = re.compile(r"((?:palette|uploads|cachepaths)/[^:\r\n]+): cycles/op ([\d.]+); ns/op ([\d.]+)")
    allocation = re.compile(r"((?:palette|uploads|cachepaths)/[^:\r\n]+): allocation Churn \{ ([^}]+) \}")
    metadata, rows = {}, []
    for version in ["before", "after"]:
        binary = getattr(args, version).resolve()
        digest = hashlib.sha256(binary.read_bytes()).hexdigest()
        checked = logs.parent / ("checks-" + digest + ".json")
        if not checked.exists():
            result = subprocess.run([str(binary), "--skip", "benchmark_", "--test-threads=1"], capture_output=True, text=True, check=True)
            checked.write_text(json.dumps(dict(sha256=digest, checks=result.stdout)), encoding="utf-8")
        metadata[version] = json.loads(checked.read_text(encoding="utf-8"))
        assert metadata[version]["sha256"] == digest
    for run in range(args.runs):
        for version in (["before", "after"] if run % 2 == 0 else ["after", "before"]):
            result = subprocess.run([str(getattr(args, version).resolve()), "benchmark_", "--skip", "benchmark_paired", "--ignored", "--nocapture", "--test-threads=1"],
                                    capture_output=True, text=True, check=True)
            (logs / f"{run}-{version}.log").write_text(result.stdout, encoding="utf-8")
            counts = {label: dict((k, int(v)) for k, v in re.findall(r"(\w+): (\d+)", values)) for label, values in allocation.findall(result.stdout)}
            matches = timing.findall(result.stdout)
            if len(matches) != args.expected or len(counts) != args.expected:
                raise ValueError(f"expected {args.expected} CPU measurements and allocation probes")
            for label, cycles, ns in matches:
                rows.append(dict(run=run, version=version, workload=label, cycles_per_op=float(cycles), ns_per_op=float(ns), **counts[label]))
            print(f"run {run + 1}/{args.runs}: {version}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)
    args.output.with_suffix(".json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    for label in sorted(counts):
        cpu = [statistics.median(row["cycles_per_op"] for row in rows if row["version"] == version and row["workload"] == label) for version in ["before", "after"]]
        print(f"{label}: {cpu[0]:.2f} -> {cpu[1]:.2f} cycles/op; {cpu[0]/cpu[1]:.3f}x")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build")
    builder.add_argument("stage")
    builder.add_argument("--dependencies", type=Path, required=True)
    builder.add_argument("--paired", action="store_true")
    builder.add_argument("--cache", action="store_true")
    comparison = commands.add_parser("compare")
    comparison.add_argument("--before", type=Path, required=True)
    comparison.add_argument("--after", type=Path, required=True)
    comparison.add_argument("--output", type=Path, required=True)
    comparison.add_argument("--runs", type=int, default=3)
    comparison.add_argument("--expected", type=int, default=33)
    args = parser.parse_args()
    if args.command == "compare" and args.runs < 1:
        parser.error("--runs must be positive")
    (build if args.command == "build" else compare)(args)


if __name__ == "__main__":
    main()
