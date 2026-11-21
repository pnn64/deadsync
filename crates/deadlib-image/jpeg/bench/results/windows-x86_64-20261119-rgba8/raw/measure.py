"""Freeze build identities and run one JPEG benchmark at a time."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
BENCH = ROOT / 'crates/deadlib-image/jpeg/bench'


def sha(path):
    return hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()


def save(path, value):
    with path.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def freeze(args):
    binaries = {}
    for suffix, source in (
        ('', ROOT / 'target/jpeg-decode-bench/release/jpeg-decode-bench.exe'),
        ('-snapshot', ROOT / 'target/jpeg-decode-bench/release/native_snapshot.exe'),
        ('-alloc', ROOT / 'target/jpeg-alloc-bench/release/jpeg-decode-bench.exe'),
    ):
        target = OUT / f'{args.name}{suffix}.exe'
        if target.exists():
            raise RuntimeError(f'refusing to overwrite {target}')
        shutil.copyfile(source, target)
        binaries[target.name] = sha(target)
    sources = sorted((ROOT / 'crates/deadlib-image/jpeg/src').rglob('*.rs'))
    sources += [BENCH / 'src/main.rs', BENCH / 'src/bin/native_snapshot.rs', BENCH / 'Cargo.toml', BENCH / 'Cargo.lock']
    patch = subprocess.check_output(['git', 'diff', '--binary', 'HEAD', '--', 'crates/deadlib-image/jpeg/src'], cwd=ROOT)
    (OUT / f'{args.name}.patch').write_bytes(patch)
    save(OUT / f'{args.name}-build.json', {
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=ROOT, text=True),
        'binaries': binaries,
        'sources': {p.relative_to(ROOT).as_posix(): sha(p) for p in sources},
        'patch_sha256': sha(OUT / f'{args.name}.patch'),
    })
    print(f'FROZEN {args.name}: {len(binaries)} binaries, {len(sources)} source identities')


def run(args):
    exe = OUT / f'{args.name}{"-alloc" if args.allocations else ""}.exe'
    identity = json.loads((OUT / f'{args.name}-build.json').read_text())
    assert sha(exe) == identity['binaries'][exe.name], 'frozen executable changed'
    lists = args.files_from or [str(OUT / 'jpeg-corpus-speed.txt'), str(OUT / 'jpeg-corpus-kodak.txt')]
    command = [str(exe)]
    if not args.raw:
        command.append('--rgba8')
    if args.native_only:
        command.append('--native-only')
    for path in lists:
        command += ['--files-from', path]
    env = dict(os.environ, RAYON_NUM_THREADS='1', JPEG_BENCH_MS=str(args.ms))
    started = time.time()
    with (OUT / f'{args.run}.tsv').open('xb') as stdout, (OUT / f'{args.run}.log').open('xb') as stderr:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
    save(OUT / f'{args.run}-run.json', {
        'command': command, 'binary_sha256': sha(exe), 'build': f'{args.name}-build.json',
        'lists': {str(p): sha(Path(p)) for p in lists},
        'RAYON_NUM_THREADS': '1', 'JPEG_BENCH_MS': str(args.ms),
        'started_unix': started, 'elapsed_seconds': time.time() - started,
        'exit_code': result.returncode,
    })
    print(f'COMPLETE {args.run}: exit={result.returncode}, seconds={time.time() - started:.1f}', flush=True)
    raise SystemExit(result.returncode)


parser = argparse.ArgumentParser(description=__doc__)
sub = parser.add_subparsers(dest='action', required=True)
f = sub.add_parser('freeze')
f.add_argument('name')
r = sub.add_parser('run')
r.add_argument('name')
r.add_argument('run')
r.add_argument('--ms', type=int, default=60)
r.add_argument('--native-only', action='store_true')
r.add_argument('--raw', action='store_true')
r.add_argument('--allocations', action='store_true')
r.add_argument('--files-from', action='append')
args = parser.parse_args()
if not args.name.replace('-', '').isalnum():
    parser.error('name must contain only letters, numbers and hyphens')
freeze(args) if args.action == 'freeze' else run(args)
