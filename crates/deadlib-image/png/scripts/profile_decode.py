"""Build and sample the native decoder on Linux with perf (no timing claims)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--name', required=True, help='New evidence filename stem')
    parser.add_argument('--corpus', required=True, type=Path)
    parser.add_argument('--target-dir', required=True, type=Path)
    parser.add_argument('--raw', required=True, type=Path)
    parser.add_argument('--operation', choices=['decode', 'reader', 'inflate'], default='decode')
    parser.add_argument('--iterations', type=int, default=40)
    parser.add_argument('--cpu', type=int, default=0)
    args = parser.parse_args()
    assert args.iterations > 0 and args.cpu >= 0
    assert Path(args.name).name == args.name and args.name not in ['.', '..']
    package = Path(__file__).resolve().parent.parent
    output = package / 'profiling'
    output.mkdir(exist_ok=True)
    paths = {suffix: output / (args.name + suffix)
             for suffix in ['.json', '.txt', '-stderr.txt', '-build.txt']}
    assert not args.raw.exists() and not any(p.exists() for p in paths.values()), 'Evidence already exists'
    env = os.environ.copy()
    env.update(CARGO_PROFILE_RELEASE_DEBUG='1', CARGO_BUILD_JOBS='4')
    build = ['cargo', 'build', '--manifest-path', str(package / 'Cargo.toml'),
             '-p', 'deadlib-image-png', '--release', '--offline', '--example',
             'decode_profile', '--target-dir', str(args.target_dir.resolve())]
    result = subprocess.run(build, env=env, capture_output=True)
    paths['-build.txt'].write_bytes(result.stdout + result.stderr)
    result.check_returncode()
    binary = args.target_dir.resolve() / 'release/examples/decode_profile'
    env.update(DEADLIB_PNG_CORPUS=str(args.corpus.resolve()),
               DEADLIB_PNG_BENCH_ITERS=str(args.iterations), DEADLIB_PNG_PROFILE=args.operation)
    command = ['taskset', '-c', str(args.cpu), 'perf', 'record', '-F', '499', '-e',
               'cycles:u', '-g', '--call-graph', 'dwarf,4096', '-o', str(args.raw), '--', str(binary)]
    began = time.monotonic()
    result = subprocess.run(command, env=env, capture_output=True)
    wall_seconds = time.monotonic() - began
    paths['-stderr.txt'].write_bytes(result.stderr)
    result.check_returncode()
    report = ['perf', 'report', '-i', str(args.raw), '--stdio', '--no-children',
              '--sort', 'symbol', '--percent-limit', '0.5']
    paths['.txt'].write_bytes(subprocess.check_output(report))
    sources = sorted((package / 'src').rglob('*.rs')) + [
        package.parent / 'src/lib.rs', package / 'Cargo.toml', package.parent / 'Cargo.toml',
        package / 'examples/decode_profile.rs', Path(__file__).resolve()]
    metadata = dict(
        command=command, build=build, report_command=report, wall_seconds=wall_seconds,
        environment={k: env[k] for k in ['CARGO_PROFILE_RELEASE_DEBUG', 'CARGO_BUILD_JOBS',
                                       'DEADLIB_PNG_CORPUS', 'DEADLIB_PNG_BENCH_ITERS', 'DEADLIB_PNG_PROFILE']},
        rustc=subprocess.check_output(['rustc', '--version'], text=True).strip(),
        perf=subprocess.check_output(['perf', '--version'], text=True).strip(),
        binary=str(binary), binary_sha256=sha(binary), raw_profile=str(args.raw),
        raw_profile_sha256=sha(args.raw),
        source_sha256=[dict(path=os.path.relpath(p, package), sha256=sha(p)) for p in sources],
        corpus_sha256=[dict(file=p.name, sha256=sha(p)) for p in sorted(args.corpus.glob('*.png'))],
        scope='Linux cycle sampling on a shared host. Includes one corpus validation pass and the repeated sampling loop. Attribution only; use the independent benchmarks for timing comparisons.')
    paths['.json'].write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    print(paths['.txt'].read_text(), flush=True)


if __name__ == '__main__':
    main()
