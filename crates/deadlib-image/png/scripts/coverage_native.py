"""Reproduce native-only LLVM coverage without changing production manifests.

Requires the selected rustup toolchain and its llvm-tools-preview component.
Uses a fresh output directory; retained profiles and binaries support inspection.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', default='nightly')
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--branches', action='store_true', help='requires nightly Rust')
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    shared = package.parent
    destination = args.output_dir.resolve()
    destination.mkdir(parents=True)  # Never merge profiles from an earlier run.
    raw_dir = destination / 'raw'
    raw_dir.mkdir()
    env = os.environ.copy()
    env['RUSTFLAGS'] = '-C instrument-coverage' + (' -Z coverage-options=branch' if args.branches else '')
    env['LLVM_PROFILE_FILE'] = str(raw_dir / '%p-%m.profraw')
    env['CARGO_BUILD_JOBS'] = '4'
    prefix = ['rustup', 'run', args.toolchain]
    commands = []
    log = (destination / 'commands.log').open('w', encoding='utf-8', newline='\n')

    def run(command, output=None):
        commands.append(command)
        log.write('COMMAND: ' + json.dumps(command) + '\n')
        log.flush()
        result = subprocess.run(command, cwd=package, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if output:
            output.write_bytes(result.stdout)
        else:
            log.write(result.stdout.decode('utf-8', errors='replace'))
        log.write(result.stderr.decode('utf-8', errors='replace'))
        log.flush()
        if result.returncode:
            print(f'Failed command; see {destination / "commands.log"}', file=sys.stderr)
            raise SystemExit(result.returncode)
        return result.stdout.decode('utf-8').strip()

    version = run(prefix + ['rustc', '-vV'])
    host = next(line.split(': ', 1)[1] for line in version.splitlines() if line.startswith('host:'))
    sysroot = Path(run(prefix + ['rustc', '--print', 'sysroot']))
    tools = sysroot / 'lib/rustlib' / host / 'bin'
    suffix = '.exe' if os.name == 'nt' else ''
    profdata = tools / ('llvm-profdata' + suffix)
    cov = tools / ('llvm-cov' + suffix)
    if not profdata.is_file() or not cov.is_file():
        raise SystemExit(f'Install llvm-tools-preview for {args.toolchain}')
    run([str(cov), '--version'])
    build = destination / 'build.jsonl'
    run(prefix + ['cargo', 'test', '--manifest-path', str(package / 'Cargo.toml'),
                 '-p', 'deadlib-image', '-p', 'deadlib-image-png', '--lib', '--tests',
                 '--no-run', '--message-format=json', '--target-dir', str(args.target_dir.resolve())], build)
    artifacts = [item for line in build.read_text(encoding='utf-8').splitlines()
                 if (item := json.loads(line)).get('reason') == 'compiler-artifact'
                 and item.get('executable') and item['profile']['test']]
    objects = sorted({item['executable'] for item in artifacts})
    # Library unit-test binaries compile cfg(test) functions inside native files.
    # Run them, but report only the ordinary library mapped in integration tests.
    report_objects = sorted({item['executable'] for item in artifacts if 'lib' not in item['target']['kind']})
    assert objects, 'Cargo reported no test executables'
    for i, obj in enumerate(objects):
        run([obj, '--test-threads=4'])
        print(f'Coverage test executable {i + 1}/{len(objects)} passed', flush=True)
    profiles = sorted(str(p) for p in raw_dir.glob('*.profraw'))
    assert profiles, 'Test execution emitted no coverage profiles'
    merged = destination / 'native.profdata'
    run([str(profdata), 'merge', '-sparse', '-o', str(merged)] + profiles)
    sources = sorted((package / 'src').rglob('*.rs')) + sorted((shared / 'src').rglob('*.rs'))
    options = [report_objects[0], '-instr-profile', str(merged)]
    for obj in report_objects[1:]:
        options += ['-object', obj]
    # Explicit source operands exclude test harnesses, references and stdlib.
    options += [str(p) for p in sources]
    run([str(cov), 'report'] + options, destination / 'native-report.txt')
    run([str(cov), 'export', '-summary-only'] + options, destination / 'native-summary.json')
    run([str(cov), 'export'] + options, destination / 'native-details.json')
    run([str(cov), 'show', '-show-line-counts-or-regions', '-show-branches=count'] + options,
        destination / 'native-lines.txt')

    def hashes(paths):
        return [{'path': p.relative_to(shared).as_posix(),
                 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths]

    manifest = {
        'rustc': version, 'branches': args.branches, 'commands': commands,
        'production_source_sha256': hashes(sources),
        'test_source_sha256': hashes(sorted((package / 'tests').rglob('*.rs'))),
        'test_executables': [{'path': p, 'sha256': hashlib.sha256(Path(p).read_bytes()).hexdigest()}
                             for p in objects],
        'profiles': len(profiles), 'coverage_objects': report_objects,
        'scope': 'Ordinary shared/PNG library mapped by integration tests; library unit binaries, harness/reference files and stdlib excluded from report. All Rust integration/unit tests run, optional C tests ignored. This is not fuzz coverage or a conformance proof.',
    }
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    log.close()
    print((destination / 'native-report.txt').read_text(encoding='utf-8'), flush=True)


if __name__ == '__main__':
    main()
