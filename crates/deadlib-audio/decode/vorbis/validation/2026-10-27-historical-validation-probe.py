"""Reproduce the 2026-10-27 historical-corpus validation from the repo root."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

repo = Path.cwd().resolve()
decode = repo / 'crates/deadlib-audio/decode'
scratch = repo / 'target/deadlib-audio-reference'
run = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else scratch / 'historical-validation-20261027'
run.mkdir(parents=True, exist_ok=False)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(args, **kwargs):
    return subprocess.check_output(args, cwd=repo, **kwargs).decode('utf8').strip()


sources = [p for package in ('ogg', 'vorbis') for p in (decode / package / 'src').glob('*.rs')]
inputs = sources + [decode / 'ogg/Cargo.toml', decode / 'vorbis/Cargo.toml',
    decode / 'ogg/tests/libogg.rs', decode / 'ogg/tests/oracle/oracle.c',
    decode / 'vorbis/tests/historical.rs', decode / 'vorbis/tests/support/mod.rs',
    decode / 'vorbis/tests/oracle/oracle.c', decode / 'vorbis/validation/corpus.json',
    decode / 'scripts/fetch_corpus.py', repo / 'Cargo.lock']
before = {str(p.relative_to(repo)).replace('\\', '/'): sha(p) for p in inputs}
for p in sources:
    name = p.relative_to(repo).as_posix()
    baseline = subprocess.check_output(['git', 'show', 'HEAD:' + name], cwd=repo)
    assert baseline.replace(b'\r\n', b'\n') == p.read_bytes().replace(b'\r\n', b'\n'), name

manifest = json.loads((decode / 'vorbis/validation/corpus.json').read_text())
corpus = {}
for entry in manifest['files']:
    path = scratch / 'corpus' / entry['path']
    assert path.stat().st_size == entry['bytes'] and sha(path) == entry['sha256'], entry['path']
    corpus[entry['path']] = {'bytes': entry['bytes'], 'sha256': sha(path)}
assert len([e for e in manifest['files'] if e['group'] == 'ffmpeg']) == 7

env = os.environ.copy()
oracles = {}
for package in ('ogg', 'vorbis'):
    executable = scratch / f'{package}-oracle-build/Release/deadlib_{package}_oracle.exe'
    assert executable.is_file()
    env[f'DEADLIB_LIB{package.upper()}_ORACLE'] = str(executable)
    env[f'DEADLIB_{package.upper()}_EXTERNAL_CORPUS'] = str(scratch / 'corpus')
    oracles[package] = {
        'executable': str(executable), 'sha256': sha(executable),
        'reference_commit': command(['git', '-C', str(scratch / package), 'rev-parse', 'HEAD']),
        'reference_tracked_changes': command(['git', '-C', str(scratch / package), 'diff', '--name-only']),
    }
assert oracles['ogg']['reference_commit'] == 'be05b13e98b048f0b5a0f5fa8ce514d56db5f822'
assert oracles['vorbis']['reference_commit'] == '0657aee69dec8508a0011f47f3b69d7538e9d262'
assert all(not row['reference_tracked_changes'] for row in oracles.values())
env['DEADLIB_VORBIS_HISTORICAL_REPORT'] = str(run / 'historical.tsv')
jobs = [
    ('fetch', [sys.executable, str(decode / 'scripts/fetch_corpus.py'), '--group', 'ffmpeg']),
    ('fmt', ['cargo', 'fmt', '-p', 'deadlib-audio-decode-ogg', '-p', 'deadlib-audio-decode-vorbis', '--', '--check']),
    ('clippy', ['cargo', 'clippy', '-p', 'deadlib-audio-decode-ogg', '-p', 'deadlib-audio-decode-vorbis', '--all-targets', '--all-features', '--', '-D', 'warnings']),
    ('vorbis-release', ['cargo', 'test', '-p', 'deadlib-audio-decode-vorbis', '--release', '--all-features', '--test', 'historical', '--', '--include-ignored', '--nocapture']),
    ('vorbis-debug', ['cargo', 'test', '-p', 'deadlib-audio-decode-vorbis', '--test', 'historical', '--', '--include-ignored', '--nocapture']),
    ('ogg-release', ['cargo', 'test', '-p', 'deadlib-audio-decode-ogg', '--release', '--test', 'libogg', 'libogg_historical_vorbis_pages_and_packets', '--', '--include-ignored', '--nocapture']),
]
checks = {
    'schema': 1, 'started_utc': datetime.now(timezone.utc).isoformat(),
    'git_head': command(['git', 'rev-parse', 'HEAD']),
    'rustc': command(['rustc', '-Vv']),
    'production_sources_unchanged_from_head': True,
    'source_sha256': before, 'corpus': corpus, 'oracles': oracles, 'jobs': [],
}
for name, args in jobs:
    print(name + ': ' + ' '.join(args), flush=True)
    begin = time.perf_counter()
    result = subprocess.run(args, cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode('utf8', errors='strict')
    output = '\n'.join(line.rstrip() for line in output.splitlines()).rstrip('\n')
    if output: output += '\n'
    log = run / (name + '.log')
    log.write_text(output, encoding='utf8', newline='\n')
    checks['jobs'].append({'name': name, 'command': args, 'exit_code': result.returncode,
        'wall_seconds': time.perf_counter() - begin, 'log_sha256': sha(log),
        'test_summaries': re.findall(r'test result:.*', output)})
    (run / 'checks.json').write_text(json.dumps(checks, indent=2) + '\n', encoding='utf8', newline='\n')
    if result.returncode:
        print(output, flush=True)
        raise SystemExit(result.returncode)
    print(name + ': OK ' + ' '.join(checks['jobs'][-1]['test_summaries']), flush=True)

assert before == {str(p.relative_to(repo)).replace('\\', '/'): sha(p) for p in inputs}
for package, row in oracles.items():
    assert sha(Path(row['executable'])) == row['sha256']
report = (run / 'historical.tsv').read_text()
assert len(report.splitlines()) == 5
checks['source_stable_through_run'] = True
checks['pcm_report_sha256'] = sha(run / 'historical.tsv')
checks['completed_utc'] = datetime.now(timezone.utc).isoformat()
checks['all_checks_passed'] = True
(run / 'checks.json').write_text(json.dumps(checks, indent=2) + '\n', encoding='utf8', newline='\n')
print('Complete: ' + str(run), flush=True)
