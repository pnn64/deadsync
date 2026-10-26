"""Archive and compare production dimension queries and multiframe scans."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from benchmark_asset_palette_upload import function, compare


def build(args):
    root = Path.cwd()
    crate = root / "crates/deadlib-assets"
    directory = root / ("target/asset-metadata-flags" if args.flags else "target/asset-metadata-spaces" if args.spaces else "target/asset-metadata")
    directory.mkdir(parents=True, exist_ok=True)
    decode = (crate / "src/decode.rs").read_text(encoding="utf-8")
    library = (crate / "src/lib.rs").read_text(encoding="utf-8")
    names = ["image_source", "image_reader", "texture_source_size", "image_limit",
             "gif_error", "gif_reader", "gif_reader_from_reader"]
    bodies = {name: function(decode, name) for name in names}
    bodies["texture_filename_has_multiframe_hint"] = function(library, "texture_filename_has_multiframe_hint")
    source = '#![allow(dead_code)]\nuse std::path::Path;\n'
    for name, path in [("perf", root / "tests/support/perf.rs"), ("perf_fixture", crate / "src/perf_fixture.rs")]:
        source += f'#[path = "{path.as_posix()}"] mod {name};\n'
    source += bodies["texture_filename_has_multiframe_hint"]
    source += '\nmod decode {\nuse super::*;\nuse std::io::{BufReader, Read, Seek};\n'
    source += "\n\n".join(bodies[name] for name in names)
    source += f'\n#[path = "{(crate / "src/decode/metadata_perf.rs").as_posix()}"] mod metadata_perf;\n}}\n'
    if args.flags:
        bodies = {name: function(library, name) for name in ["hint_starts_with", "texture_hint_flags"]}
        source = '#![allow(dead_code)]\n'
        for name, path in [("perf", root / "tests/support/perf.rs"), ("texture_hint_perf", crate / "src/texture_hint_perf.rs")]:
            source += f'#[path = "{path.as_posix()}"] mod {name};\n'
        source += "\n".join(re.findall(r"^const HINT_.*;$", library, re.MULTILINE)) + "\n"
        source += "\n\n".join(bodies.values())
    if args.spaces:
        tests = (crate / "src/decode/metadata_perf.rs").read_text(encoding="utf-8")
        original = (crate / "src/decode/metadata_original.rs").read_text(encoding="utf-8")
        body = function(original if args.frozen else library, "texture_filename_has_multiframe_hint")
        bodies = {"texture_filename_has_multiframe_hint": body}
        source = '#![allow(dead_code)]\nuse std::{path::Path, hint::black_box};\n'
        for name, path in [("perf", root / "tests/support/perf.rs"), ("bench", root / "tests/support/kernel_bench.rs")]:
            source += f'#[path = "{path.as_posix()}"] mod {name};\n'
        source += body + '\nmod original { use std::path::Path;\n' + function(original, "texture_filename_has_multiframe_hint") + '\n}\n'
        source += function(tests, "check_name") + '\n#[test]\n' + function(tests, "multiframe_scans_preserve_shipped_and_malformed_names")
        workload = function(tests, "benchmark_metadata")
        source += '\n#[test]\n#[ignore = "optimized CPU benchmark"]\nfn benchmark_spaces() {\n' + workload[workload.index("    let shipped:"):]
        source = source.replace('include_str!("../../tests/support/asset_names.txt")',
                                f'include_str!("{(crate / "tests/support/asset_names.txt").as_posix()}")')
    snapshot = directory / (args.stage + ".rs")
    snapshot.write_bytes(source.encode("utf-8"))
    dependencies = json.loads(args.dependencies.read_text(encoding="utf-8"))["direct_dependencies"]
    argv = ["rustc", "--test", "--edition=2024", "-C", "opt-level=3", "-C", "lto=fat",
            "-C", "codegen-units=1", "-C", "target-feature=+crt-static", str(snapshot),
            "-o", str(directory / (args.stage + ".exe"))]
    used = {}
    for name in ([] if args.flags else ["memchr"] if args.spaces else ["image", "gif", "memchr"]):
        info = dependencies[name]
        path = Path(info["path"])
        assert hashlib.sha256(path.read_bytes()).hexdigest() == info["sha256"], name
        argv += ["--extern", name + "=" + str(path), "-L", "dependency=" + str(path.parent)]
        used[name] = info
    with (directory / (args.stage + ".build.log")).open("w", encoding="utf-8") as output:
        result = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT,
                                env=os.environ | {"CARGO_MANIFEST_DIR": str(crate)})
    if result.returncode:
        print((directory / (args.stage + ".build.log")).read_text(encoding="utf-8")[-6000:])
        raise SystemExit(result.returncode)
    paths = [crate / "src/decode.rs", crate / "src/lib.rs", crate / "src/decode/metadata_perf.rs",
             crate / "src/decode/metadata_original.rs", crate / "src/perf_fixture.rs",
             root / "tests/support/kernel_bench.rs", root / "tests/support/perf.rs", Path(__file__).resolve()]
    if args.flags:
        paths = [crate / "src/lib.rs", crate / "src/texture_hint_perf.rs", crate / "src/texture_hint_original.rs",
                 root / "tests/support/kernel_bench.rs", root / "tests/support/perf.rs", Path(__file__).resolve()]
    metadata = dict(argv=argv, dependencies=used, source_sha256={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
                    body_sha256={name: hashlib.sha256(body.encode()).hexdigest() for name, body in bodies.items()},
                    binary_sha256=hashlib.sha256((directory / (args.stage + ".exe")).read_bytes()).hexdigest())
    (directory / (args.stage + ".build.json")).write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(args.stage + " built: " + metadata["binary_sha256"], flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build")
    builder.add_argument("stage")
    builder.add_argument("--dependencies", type=Path, required=True)
    builder.add_argument("--flags", action="store_true")
    builder.add_argument("--spaces", action="store_true")
    builder.add_argument("--frozen", action="store_true")
    runner = commands.add_parser("compare")
    runner.add_argument("--before", type=Path, required=True)
    runner.add_argument("--after", type=Path, required=True)
    runner.add_argument("--output", type=Path, required=True)
    runner.add_argument("--runs", type=int, default=3)
    runner.add_argument("--flags", action="store_true")
    runner.add_argument("--spaces", action="store_true")
    args = parser.parse_args()
    if args.flags and args.spaces:
        parser.error("choose either --flags or --spaces")
    if getattr(args, "frozen", False) and not args.spaces:
        parser.error("--frozen requires --spaces")
    if args.command == "build":
        build(args)
    else:
        if args.runs < 1:
            parser.error("--runs must be positive")
        args.expected = 4 if args.flags or args.spaces else 12
        args.log_root = Path("target/asset-metadata-flags" if args.flags else "target/asset-metadata-spaces" if args.spaces else "target/asset-metadata")
        compare(args)


if __name__ == "__main__":
    main()
