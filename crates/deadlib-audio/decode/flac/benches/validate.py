"""Run every decoder parity gate and Clippy, keeping immutable UTF-8 evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--oracle', type=Path, required=True)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--generated', type=Path, required=True)
    parser.add_argument('--ffmpeg-oracle', required=True)
    parser.add_argument('--ffmpeg-wsl')
    parser.add_argument('--fuzz-frames', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    repo = root.parents[3]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    for name, value in [('ORACLE', args.oracle.resolve()), ('CORPUS', args.corpus.resolve()),
                        ('GENERATED', args.generated.resolve()), ('FFMPEG_ORACLE', args.ffmpeg_oracle),
                        ('RAW_FUZZ_CORPUS', args.fuzz_frames.resolve())]:
        env['DEADLIB_FLAC_' + name] = str(value)
    if args.ffmpeg_wsl:
        env['DEADLIB_FLAC_FFMPEG_WSL'] = args.ffmpeg_wsl
    reports = {'REPORT': 'native-corpus.tsv', 'SEEK_REPORT': 'native-seeking.tsv',
               'ENCODING_REPORT': 'encoding-matrix.tsv', 'OGG_REPORT': 'ogg-matrix.tsv',
               'OGG_CORPUS_REPORT': 'ogg-corpus.tsv', 'FFMPEG_REPORT': 'ffmpeg-corpus.tsv',
               'WIRE_REPORT': 'wire-parity.tsv', 'FUZZ_FRAME_REPORT': 'fuzz-frame-parity.tsv'}
    for name, file in reports.items():
        env['DEADLIB_FLAC_' + name] = str(out / file)
    paths = sorted((root / 'src').glob('*.rs')) + sorted((root / 'tests').rglob('*'))
    paths = [p for p in paths if p.is_file()]
    before = {p.relative_to(root).as_posix(): sha(p) for p in paths}
    common = ['-p', 'deadlib-audio-decode-flac', '--target-dir', str(args.target_dir.resolve())]
    commands = [['cargo', 'test'] + common + ['--release', '--', '--include-ignored', '--test-threads=1'],
                ['cargo', 'clippy'] + common + ['--all-targets', '--', '-D', 'warnings']]
    results = []
    for command, name in zip(commands, ['tests', 'clippy']):
        print('Running:', ' '.join(command), flush=True)
        with (out / (name + '.log')).open('wb') as log:
            result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
        results.append({'command': command, 'exit_code': result.returncode})
        print(name, 'exit', result.returncode, flush=True)
    after = {p.relative_to(root).as_posix(): sha(p) for p in paths}
    text = (out / 'tests.log').read_text(encoding='utf-8')
    groups = [int(n) for n in re.findall(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored', text)]
    complete = len(groups) >= 3 and all(r['exit_code'] == 0 for r in results) and before == after
    complete &= all((out / p).is_file() for p in reports.values())
    report = {'finished_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'passed': complete, 'commands': results, 'unit_tests': groups[0] if groups else 0,
              'integration_and_external_gates': sum(groups[1:-1]), 'doctests': groups[-1] if groups else 0,
              'source_sha256': before, 'source_changed': before != after,
              'driver_sha256': sha(Path(__file__)), 'oracle_sha256': sha(args.oracle),
              'reference_environment': {k: v for k, v in env.items() if k.startswith('DEADLIB_FLAC_')},
              'report_sha256': {p.name: sha(p) for p in out.iterdir() if p.is_file()}}
    (out / 'validation.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: report[k] for k in ['passed', 'unit_tests', 'integration_and_external_gates', 'doctests']}), flush=True)
    raise SystemExit(0 if complete else 1)


if __name__ == '__main__':
    main()
