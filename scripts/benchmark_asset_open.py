"""Benchmark complete production file dispatch against archived binaries."""
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
    directory = root / "target/asset-open"
    directory.mkdir(parents=True, exist_ok=True)
    decode = (crate / "src/decode.rs").read_text(encoding="utf-8")
    library = (crate / "src/lib.rs").read_text(encoding="utf-8")
    names = ["image_signature", "image_source", "image_reader", "texture_source_size", "decode_image",
             "check_image_alloc", "image_limit", "png_error", "decode_error", "gif_error", "gif_reader",
             "gif_reader_from_reader", "decode_gif", "decode_gif_from_reader", "decode_indexed_bmp",
             "decode_indexed_bmp_from_reader", "key_palette", "unpack_palette", "decode_png",
             "decode_png_from_reader", "key_rgba"]
    bodies = {name: function(decode, name) for name in names if re.search(r"\bfn " + name + r"(?:<|\()", decode)}
    fallback = {name: function(library, name) for name in ["open_image_fallback_mode", "open_image_fallback"]}
    source = '#![allow(dead_code)]\nuse image::{ImageFormat, ImageReader};\nuse log::warn;\nuse std::path::Path;\n'
    for name, path in [("perf", root / "tests/support/perf.rs"), ("perf_fixture", crate / "src/perf_fixture.rs")]:
        source += f'#[path = "{path.as_posix()}"] mod {name};\n'
    source += "\n\n".join(fallback.values())
    source += '\nmod decode {\nuse super::*;\nuse image::RgbaImage;\nuse std::io::{BufReader, Read, Seek, SeekFrom};\n'
    source += "\n\n".join(bodies.values())
    source += f'\n#[path = "{(crate / "src/decode/open_perf.rs").as_posix()}"] mod open_perf;\n}}\n'
    snapshot = directory / (args.stage + ".rs")
    snapshot.write_bytes(source.encode("utf-8"))
    dependencies = json.loads(args.dependencies.read_text(encoding="utf-8"))["direct_dependencies"]
    argv = ["rustc", "--test", "--edition=2024", "-C", "opt-level=3", "-C", "lto=fat",
            "-C", "codegen-units=1", "-C", "target-feature=+crt-static", str(snapshot),
            "-o", str(directory / (args.stage + ".exe"))]
    used = {}
    for name in ["image", "png", "gif", "log"]:
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
    paths = [crate / "src/decode.rs", crate / "src/lib.rs", crate / "src/decode/open_perf.rs",
             crate / "src/decode/open_original.rs", crate / "src/perf_fixture.rs",
             root / "tests/support/kernel_bench.rs", root / "tests/support/perf.rs"]
    metadata = dict(argv=argv, dependencies=used, source_sha256={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
                    body_sha256={name: hashlib.sha256(body.encode()).hexdigest() for name, body in (bodies | fallback).items()},
                    binary_sha256=hashlib.sha256((directory / (args.stage + ".exe")).read_bytes()).hexdigest())
    (directory / (args.stage + ".build.json")).write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(args.stage + " built: " + metadata["binary_sha256"], flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build")
    builder.add_argument("stage")
    builder.add_argument("--dependencies", type=Path, required=True)
    runner = commands.add_parser("compare")
    runner.add_argument("--before", type=Path, required=True)
    runner.add_argument("--after", type=Path, required=True)
    runner.add_argument("--output", type=Path, required=True)
    runner.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    if args.command == "build":
        build(args)
    else:
        if args.runs < 1:
            parser.error("--runs must be positive")
        args.expected = 10
        args.log_root = Path("target/asset-open")
        compare(args)


if __name__ == "__main__":
    main()
