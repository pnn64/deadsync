"""Run one immutable PNG encoder measurement and retain its provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
WORK = Path(__file__).resolve().parent

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('name')
    parser.add_argument('executable')
    parser.add_argument('--ms', default='40')
    parser.add_argument('--extra', nargs=argparse.REMAINDER, default=[])
    args = parser.parse_args()
    executable = (ROOT / args.executable).resolve()
    manifest = json.loads((WORK / 'corpus.json').read_text(encoding='utf-8'))
    inputs = [str(ROOT / entry['path']) for entry in manifest['files']]
    command = [str(executable), '--generated', '--sizes', manifest['sizes'], *args.extra, *inputs]
    out = WORK / (args.name + '.tsv')
    err = WORK / (args.name + '.log')
    identity = WORK / (args.name + '.json')
    if any(path.exists() for path in (out, err, identity)):
        raise SystemExit('Refusing to overwrite retained measurement ' + args.name)
    source_paths = sorted((ROOT / 'crates/deadlib-image/png/src').rglob('*.rs'))
    source_paths += [ROOT / path for path in [
        'crates/deadlib-image/src/lib.rs', 'crates/deadlib-image/png/Cargo.toml',
        'crates/deadlib-image/png/bench/Cargo.toml', 'crates/deadlib-image/png/bench/Cargo.lock',
        'crates/deadlib-image/png/bench/src/bin/encode.rs', '.cargo/config.toml']]
    before = {str(path.relative_to(ROOT)).replace('\\', '/'): digest(path) for path in source_paths}
    executable_snapshot = executable.with_suffix('.sources.json')
    if not executable_snapshot.exists():
        executable_snapshot.write_text(json.dumps({
            'executable_sha256': digest(executable), 'sources': before,
            'captured': 'first measurement immediately after this binary build',
        }, indent=2) + '\n', encoding='utf-8')
    built = json.loads(executable_snapshot.read_text(encoding='utf-8'))
    if built['executable_sha256'] != digest(executable):
        raise SystemExit('Executable changed since source snapshot')
    record = {
        'utc': datetime.now(timezone.utc).isoformat(), 'command': command,
        'png_bench_ms': args.ms, 'executable_sha256': digest(executable),
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'sources': before,
        'executable_source_snapshot': built,
        'inputs': {entry['path']: digest(ROOT / entry['path']) for entry in manifest['files']},
    }
    env = dict(os.environ, PNG_BENCH_MS=args.ms)
    with out.open('xb') as stdout, err.open('xb') as stderr:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
    record['exit_code'] = result.returncode
    record['sources_unchanged_during_run'] = before == {
        str(path.relative_to(ROOT)).replace('\\', '/'): digest(path) for path in source_paths}
    record['output_sha256'] = digest(out)
    record['log_sha256'] = digest(err)
    record['inputs_unchanged_during_run'] = record['inputs'] == {
        entry['path']: digest(ROOT / entry['path']) for entry in manifest['files']}
    identity.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    print(f'{args.name}: exit={result.returncode}, sources_unchanged={record["sources_unchanged_during_run"]}, output={out}')
    if result.returncode:
        print(err.read_text(encoding='utf-8', errors='replace')[-6000:])
    if not record['sources_unchanged_during_run'] or not record['inputs_unchanged_during_run']:
        raise SystemExit('Production/benchmark sources or inputs changed during measurement')
    sys.exit(result.returncode)

if __name__ == '__main__':
    main()
