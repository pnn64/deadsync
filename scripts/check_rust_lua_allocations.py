"""Record current-source allocator failure tests (not a whole-VM parity claim)."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--target', choices=['i686-unknown-linux-gnu'])
    parser.add_argument('--debug', action='store_true')
    parser.add_argument('--packages', action='store_true', help='Also run the complete interpreter package test suite')
    parser.add_argument('--checks', action='store_true', help='Also run formatting, Clippy and normal-build checks')
    parser.add_argument('--miri-only', action='store_true', help='Run ownership, collector, hook, identity, pattern, library, traceback, substitution, debug, format and loader cleanup tests under nightly Miri instead of native tests')
    args = parser.parse_args()
    if args.miri_only and (args.target or args.debug or args.packages or args.checks):
        parser.error('--miri-only cannot be combined with native profile/check options')
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    logs = output.with_suffix('')
    logs.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    if args.target:
        if platform.system() != 'Linux':
            parser.error('i686 requires Linux and a multilib toolchain')
        environment['CFLAGS_i686_unknown_linux_gnu'] = '-msse2 -mfpmath=sse'

    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def sources():
        paths = [root / 'Cargo.toml', root / 'Cargo.lock', root / '.cargo/config.toml', Path(__file__).resolve()]
        for directory in ['crates/deadlib-lua']:
            paths += [p for p in (root / directory).rglob('*')
                      if p.is_file() and (p.suffix == '.rs' or p.name == 'Cargo.toml'
                                         or 'fixtures' in p.relative_to(root / directory).parts)]
        return {p.relative_to(root).as_posix(): digest(p) for p in sorted(set(paths))}

    before = sources()
    commands = []

    def run(name, command):
        started = time.monotonic()
        log = logs / (name + '.log')
        # Keep live raw output, including an interrupted compiler/interpreter run.
        with log.open('wb') as stream:
            result = subprocess.run(command, cwd=root, env=environment, stdout=stream, stderr=subprocess.STDOUT)
        text = log.read_text(encoding='utf-8', errors='replace')
        row = dict(name=name, command=command, returncode=result.returncode,
                   seconds=time.monotonic() - started,
                   log=os.path.relpath(log, output.parent), log_sha256=digest(log),
                   test_results=re.findall(r'test result: .*', text))
        if name == 'allocation-failures':
            markers = re.findall(r'\b(ALLOCATION_FAILURE_POINTS|ALLOCATION_CHECK_STACK_FAILURE_POINTS|TABLE_MUTATION_FAILURE_POINTS|TRACEBACK_GLOBAL_REJECTION_POINTS|SUBSTITUTION_GLOBAL_REJECTION_POINTS|FORMAT_GLOBAL_REJECTION_POINTS) (\d+)', text)
            row['rejection_points_by_marker'] = {name: sum(int(count) for kind, count in markers if kind == name) for name, _ in markers}
            row['rejected_allocation_points'] = sum(row['rejection_points_by_marker'].values())
            row['complete'] = result.returncode == 0 and len(row['test_results']) == 6 and row['rejected_allocation_points'] > 0
        elif name in {'ownership', 'ownership-miri', 'ownership-collector-hooks-identity-patterns-libraries-traces-substitutions-debug-format-loader-miri'}:
            points = re.findall(r'CUSTOM_BOOTSTRAP_REJECTION_POINTS (\d+)', text)
            row['custom_bootstrap_rejection_points'] = sum(map(int, points))
            row['custom_operation_rejection_points'] = sum(map(int, re.findall(r'CUSTOM_OPERATION_REJECTION_POINTS (\d+)', text)))
            row['complete'] = (result.returncode == 0 and len(row['test_results']) == 1
                               and row['custom_bootstrap_rejection_points'] > 0
                               and row['custom_operation_rejection_points'] > 0)
            if name == 'ownership-collector-hooks-identity-patterns-libraries-traces-substitutions-debug-format-loader-miri':
                row['test_groups'] = ['memory::tests', 'gc::allocation_tests', 'hooks::allocation_tests', 'identity::allocation_tests', 'pattern::', 'strings::allocation_tests', 'tables::allocation_tests', 'traces::allocation_tests', 'substitution_allocation_tests', 'debug::allocation_tests', 'format_allocation_tests', 'loads::allocation_tests']
                row['collector_rejection_points'] = sum(map(int, re.findall(r'COLLECTOR_REJECTION_POINTS (\d+)', text)))
                row['hook_rejection_points'] = sum(map(int, re.findall(r'HOOK_REJECTION_POINTS (\d+)', text)))
                row['identity_rejection_points'] = sum(map(int, re.findall(r'IDENTITY_REJECTION_POINTS (\d+)', text)))
                row['pattern_rejection_points'] = sum(map(int, re.findall(r'PATTERN_REJECTION_POINTS (\d+)', text)))
                row['library_rejection_points'] = sum(map(int, re.findall(r'LIBRARY_REJECTION_POINTS (\d+)', text)))
                row['traceback_rejection_points'] = sum(map(int, re.findall(r'TRACEBACK_REJECTION_POINTS (\d+)', text)))
                row['substitution_rejection_points'] = sum(map(int, re.findall(r'SUBSTITUTION_REJECTION_POINTS (\d+)', text)))
                row['debug_local_rejection_points'] = sum(map(int, re.findall(r'DEBUG_LOCAL_REJECTION_POINTS (\d+)', text)))
                row['format_rejection_points'] = sum(map(int, re.findall(r'FORMAT_REJECTION_POINTS (\d+)', text)))
                row['loader_cleanup_verified'] = 'loader_cleanup_preserves_active_jobs_and_borrowed_files_without_allocating ... ok' in text
                row['complete'] = (row['complete'] and row['collector_rejection_points'] > 0
                                   and row['hook_rejection_points'] > 0 and row['identity_rejection_points'] > 0
                                   and row['pattern_rejection_points'] > 0 and row['library_rejection_points'] > 0
                                   and row['traceback_rejection_points'] > 0 and row['substitution_rejection_points'] > 0
                                   and row['debug_local_rejection_points'] > 0
                                   and row['format_rejection_points'] > 0 and row['loader_cleanup_verified'])
        elif name == 'bindings':
            row['loader_rejection_points'] = sum(map(int, re.findall(r'BINDING_LOADER_REJECTION_POINTS (\d+)', text)))
            row['marking_rejection_points'] = sum(map(int, re.findall(r'BINDING_MARK_REJECTION_POINTS (\d+)', text)))
            row['complete'] = (result.returncode == 0 and len(row['test_results']) == 1
                               and row['loader_rejection_points'] > 0
                               and row['marking_rejection_points'] > 0)
        elif name in {'collector', 'collector-miri'}:
            row['collector_rejection_points'] = sum(map(int, re.findall(r'COLLECTOR_REJECTION_POINTS (\d+)', text)))
            row['complete'] = (result.returncode == 0 and len(row['test_results']) == 1
                               and row['collector_rejection_points'] > 0)
        elif name == 'hooks':
            row['hook_rejection_points'] = sum(map(int, re.findall(r'HOOK_REJECTION_POINTS (\d+)', text)))
            row['complete'] = (result.returncode == 0 and len(row['test_results']) == 1
                               and row['hook_rejection_points'] > 0)
        elif name == 'identity':
            row['identity_rejection_points'] = sum(map(int, re.findall(r'IDENTITY_REJECTION_POINTS (\d+)', text)))
            row['complete'] = (result.returncode == 0 and len(row['test_results']) == 1
                               and row['identity_rejection_points'] > 0)
        elif name == 'formatting':
            row['format_rejection_points'] = sum(map(int, re.findall(r'FORMAT_REJECTION_POINTS (\d+)', text)))
            row['complete'] = result.returncode == 0 and len(row['test_results']) == 1 and row['format_rejection_points'] > 0
        elif name == 'loader-cleanup':
            row['loader_cleanup_verified'] = 'loader_cleanup_preserves_active_jobs_and_borrowed_files_without_allocating ... ok' in text
            row['complete'] = result.returncode == 0 and len(row['test_results']) == 1 and row['loader_cleanup_verified']
        elif name in {'tracebacks', 'substitutions'}:
            row['traceback_rejection_points'] = sum(map(int, re.findall(r'TRACEBACK_REJECTION_POINTS (\d+)', text)))
            row['substitution_rejection_points'] = sum(map(int, re.findall(r'SUBSTITUTION_REJECTION_POINTS (\d+)', text)))
            row['debug_local_rejection_points'] = sum(map(int, re.findall(r'DEBUG_LOCAL_REJECTION_POINTS (\d+)', text)))
            covered = row['traceback_rejection_points'] > 0 if name == 'tracebacks' else (row['substitution_rejection_points'] > 0 and row['debug_local_rejection_points'] > 0)
            row['complete'] = result.returncode == 0 and len(row['test_results']) == 1 and covered
        elif name in {'patterns', 'libraries'}:
            marker = 'PATTERN_REJECTION_POINTS' if name == 'patterns' else 'LIBRARY_REJECTION_POINTS'
            points = sum(map(int, re.findall(r'\b' + marker + r' (\d+)', text)))
            row['pattern_rejection_points' if name == 'patterns' else 'library_rejection_points'] = points
            row['complete'] = result.returncode == 0 and len(row['test_results']) == 1 and points > 0
        else:
            row['complete'] = result.returncode == 0
        commands.append(row)
        print(f'{name}: exit={result.returncode}, seconds={row["seconds"]:.1f}', flush=True)

    run('rustc', ['rustc', '-vV'])
    run('cargo', ['cargo', '-V'])
    if args.miri_only:
        run('miri-rustc', ['rustc', '+nightly', '-vV'])
        run('miri-version', ['cargo', '+nightly', 'miri', '--version'])
        run('ownership-collector-hooks-identity-patterns-libraries-traces-substitutions-debug-format-loader-miri', ['cargo', '+nightly', 'miri', 'test', '-p', 'deadlib-lua',
            '--lib', '--', '--nocapture', '--test-threads=1',
            'memory::tests', 'gc::allocation_tests', 'hooks::allocation_tests', 'identity::allocation_tests', 'pattern::', 'strings::allocation_tests', 'tables::allocation_tests', 'traces::allocation_tests', 'substitution_allocation_tests', 'debug::allocation_tests', 'format_allocation_tests', 'loads::allocation_tests'])
    target = ['--target', args.target] if args.target else []
    profile = [] if args.debug else ['--release']
    if args.packages:
        run('packages', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target])
    if not args.miri_only:
        run('ownership', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'memory::tests', '--', '--nocapture', '--test-threads=1'])
        run('bindings', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'binding', '--', '--nocapture', '--test-threads=1'])
        run('collector', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'gc::allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('hooks', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'hooks::allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('identity', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'identity::allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('patterns', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', '--', '--nocapture', '--test-threads=1', 'pattern::',
            'strings::allocation_tests', 'pattern_preparation'])
        run('libraries', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'tables::allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('tracebacks', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'traces::allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('substitutions', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', '--', '--nocapture', '--test-threads=1', 'substitution_allocation_tests', 'debug::allocation_tests'])
        run('formatting', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'format_allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('loader-cleanup', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--lib', 'loads::allocation_tests', '--', '--nocapture', '--test-threads=1'])
        run('allocation-failures', ['cargo', 'test', '-p', 'deadlib-lua', *profile, *target,
            '--test', 'allocation_failures', '--test', 'table_allocation_failures',
            '--test', 'string_allocation_failures', '--test', 'traceback_allocation_failures',
            '--test', 'substitution_allocation_failures', '--test', 'format_allocation_failures', '--', '--nocapture', '--test-threads=1'])
    if args.checks:
        run('format', ['cargo', 'fmt', '-p', 'deadlib-lua', '--check'])
        run('clippy', ['cargo', 'clippy', '-p', 'deadlib-lua', '--all-targets', '--no-deps', *target, '--', '-D', 'warnings'])
        run('normal-build', ['cargo', 'check', '-p', 'deadlib-lua', '--all-targets', *target])
    after = sources()
    report = dict(scope='Allocator regression checkpoint; full standalone parity remains open',
                  platform=platform.platform(), machine=platform.machine(), target=args.target,
                  profile='miri' if args.miri_only else 'debug' if args.debug else 'release',
                  environment={k: v for k, v in environment.items() if k in {
                      'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTUP_TOOLCHAIN',
                      'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CFLAGS',
                      'CFLAGS_i686_unknown_linux_gnu', 'CC', 'CXX', 'LDFLAGS', 'MIRIFLAGS'}},
                  source_hashes=before, source_unchanged=before == after, commands=commands)
    report['passed'] = report['source_unchanged'] and all(row['complete'] for row in commands)
    output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    raise SystemExit(0 if report['passed'] else 1)


if __name__ == '__main__':
    main()
