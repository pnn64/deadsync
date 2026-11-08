"""Fresh, isolated ASan/libFuzzer campaigns with retained source and seed hashes."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import subprocess
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TARGETS = ('primitives', 'frame', 'native', 'ogg', 'encode')
HIDDEN = subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-root', type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=120)
    parser.add_argument('--seed', type=int, default=9639)
    parser.add_argument('--corpus', type=Path)
    parser.add_argument('--targets', nargs='+', choices=TARGETS, default=TARGETS)
    args = parser.parse_args()
    if args.seconds < 1:
        parser.error('seconds must be positive')
    run = args.run_root.resolve()
    run.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    tools = REPO / 'target/deadlib-audio-reference/fuzz-tools/bin'
    env['PATH'] = str(tools) + os.pathsep + env['PATH']
    msvc = None
    if os.name == 'nt':
        vswhere = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        install = subprocess.check_output([str(vswhere), '-latest', '-products', '*', '-requires',
            'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'],
            text=True, creationflags=HIDDEN).strip()
        version = (Path(install) / 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt').read_text().strip()
        msvc = Path(install) / f'VC/Tools/MSVC/{version}/bin/Hostx64/x64'
        if not (msvc / 'clang_rt.asan_dynamic-x86_64.dll').exists():
            raise RuntimeError('MSVC AddressSanitizer runtime is missing')
        env['PATH'] = str(msvc) + os.pathsep + env['PATH']
    # A wrapper policy is not part of Cargo's dependency artifact fingerprint.
    # A fresh build directory prevents stale reference arithmetic settings.
    build = run / 'build'

    def command(argv, log):
        print('Running:', ' '.join(map(str, argv)), flush=True)
        with (run / log).open('w') as output:
            subprocess.run(list(map(str, argv)), env=env, cwd=REPO, stdout=output,
                stderr=subprocess.STDOUT, creationflags=HIDDEN, check=True)

    seed = ['cargo', 'run', '--locked', '--manifest-path', HERE / 'Cargo.toml',
        '--target-dir', REPO / 'target/deadlib-flac-fuzz-check', '--release', '--features', 'seed',
        '--bin', 'seed', '--', run / 'corpus']
    if args.corpus:
        seed.append(args.corpus.resolve())
    command(seed, 'seeds.log')
    wrapper = run / ('reference-rustc.exe' if os.name == 'nt' else 'reference-rustc')
    command(['rustc', '--edition=2024', HERE / 'reference_rustc.rs', '-O', '-o', wrapper], 'wrapper-build.log')
    env['RUSTC_WRAPPER'] = str(wrapper)
    env['DEADLIB_FLAC_RUSTC_AUDIT'] = str(run / 'compiler-audit')
    command(['cargo', '+nightly', 'fuzz', 'build', '--fuzz-dir', HERE, '--target-dir', build], 'build.log')
    sources = {}
    for root in [HERE.parent / 'src', HERE.parent.parent / 'ogg/src', HERE]:
        for path in root.rglob('*'):
            if path.is_file() and not set(path.parts) & {'target', 'corpus', 'artifacts', 'coverage', '__pycache__', 'reports'}:
                sources[str(path.relative_to(REPO))] = sha(path)
    initial = {str(p.relative_to(run)): sha(p) for p in (run / 'corpus').rglob('*') if p.is_file()}
    (run / 'initial-corpus.json').write_text(json.dumps(initial, indent=2) + '\n')

    def version(argv):
        return subprocess.check_output(argv, cwd=REPO, env=env, text=True, creationflags=HIDDEN).strip()

    host = re.search(r'^host: (.+)$', version(['rustc', '+nightly', '-Vv']), re.M).group(1)
    provenance = {'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
        'seconds': args.seconds, 'seed': args.seed, 'targets': args.targets, 'sanitizer': 'address',
        'rustc': version(['rustc', '+nightly', '-Vv']), 'cargo_fuzz': version(['cargo', 'fuzz', '--version']),
        'git_parent': version(['git', 'rev-parse', 'HEAD']), 'msvc_bin': str(msvc),
        'source_sha256': sources, 'official_corpus': str(args.corpus),
        'reference_policy': 'Only symphonia_bundle_flac has overflow-checks=off to preserve release wrapping; native overflow checks, all ASan and feedback retained',
        'compiler_wrapper_sha256': sha(wrapper),
        'initial_corpus_sha256': sha(run / 'initial-corpus.json')}
    (run / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    jobs = []
    for i, target in enumerate(args.targets):
        exe = build / host / 'release' / (target + ('.exe' if os.name == 'nt' else ''))
        artifacts = run / 'artifacts' / target
        artifacts.mkdir(parents=True)
        argv = [str(exe), str(run / 'corpus' / target), f'-max_total_time={args.seconds}',
            '-max_len=131072', '-timeout=20', '-rss_limit_mb=2048', f'-seed={args.seed+i}',
            '-print_final_stats=1', f'-artifact_prefix={artifacts.as_posix()}/', f'-dict={HERE / "flac.dict"}']
        output = (run / f'{target}.log').open('w')
        process = subprocess.Popen(argv, cwd=REPO, env=env, stdout=output, stderr=subprocess.STDOUT, creationflags=HIDDEN)
        jobs.append((target, process, output, argv, sha(exe), time.monotonic()))
        print(f'Started {target}: PID {process.pid}', flush=True)
    while any(process.poll() is None for _, process, *_ in jobs):
        time.sleep(1)
    results = []
    for target, process, output, argv, binary, started in jobs:
        output.close()
        log = (run / f'{target}.log').read_text(errors='replace')
        stats = {key: int(value) for key, value in re.findall(r'stat::(\w+):\s+(\d+)', log)}
        done = re.findall(r'#(\d+)\s+DONE\s+cov:\s+(\d+)\s+ft:\s+(\d+)', log)
        result = {'target': target, 'exit_code': process.returncode, 'binary_sha256': binary,
            'arguments': argv, 'stats': stats, 'coverage': list(map(int, done[-1])) if done else None,
            'elapsed_upper_bound_seconds': time.monotonic()-started}
        results.append(result)
        print(f'{target}: exit {process.returncode}, {stats}', flush=True)
    (run / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    if any(r['exit_code'] != 0 for r in results):
        raise RuntimeError(f'Fuzz failure; preserve and replay artifacts in {run}')


if __name__ == '__main__':
    main()
