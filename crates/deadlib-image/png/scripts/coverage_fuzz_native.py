"""Replay isolated-feedback corpora and report only native source coverage.

This fixed replay is excluded from fuzz execution totals. The whole-package
report includes unexecuted encoding utilities; per-file gaps require review.
"""
from argparse import ArgumentParser
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument('--campaign-output', type=Path, required=True)
    parser.add_argument('--cargo-fuzz', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    package = Path(__file__).resolve().parent.parent
    campaign = args.campaign_output.resolve()
    campaign_manifest = json.loads((campaign / 'manifest.json').read_text())
    assert campaign_manifest['isolation_verified'] and campaign_manifest['final_source_hashes_verified']
    output = args.output_dir.resolve()
    output.mkdir(parents=True)
    target_dir = args.target_dir.resolve()
    target_dir.mkdir(parents=True)
    env = os.environ.copy()
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    env.pop('RUSTC_WRAPPER', None)
    env.pop('RUSTC_WORKSPACE_WRAPPER', None)
    env.update(RUSTFLAGS='-Z coverage-options=branch', CARGO_BUILD_JOBS='4',
               CC='/usr/bin/clang', CXX='/usr/bin/clang++')
    prefix = ['rustup', 'run', 'nightly']
    rustc = subprocess.check_output(prefix + ['rustc', '-vV'], text=True)
    host = re.search(r'^host: (.+)$', rustc, re.M)[1]
    sysroot = Path(subprocess.check_output(prefix + ['rustc', '--print', 'sysroot'], text=True).strip())
    tools = sysroot / 'lib/rustlib' / host / 'bin'
    sources = sorted((package / 'src').rglob('*.rs')) + sorted((package.parent / 'src').rglob('*.rs'))
    source_hashes = [{'path': path.relative_to(package.parent).as_posix(), 'sha256': sha(path)} for path in sources]
    commands, corpora, binaries, profiles = [], [], [], []
    with (output / 'commands.log').open('wb') as log:
        def run(command, destination=None):
            commands.append(command)
            log.write(('COMMAND: ' + json.dumps(command) + '\n').encode())
            log.flush()
            result = subprocess.run(command, cwd=package, env=env,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if destination:
                destination.write_bytes(result.stdout)
            else:
                log.write(result.stdout)
            log.write(result.stderr)
            log.write(('\nEXIT: ' + str(result.returncode) + '\n').encode())
            log.flush()
            assert result.returncode == 0, (command, result.stderr[-4000:])

        for target in ['png_stream', 'inflate_stream']:
            coverage = package / 'fuzz/coverage' / target
            assert not coverage.exists(), ('Existing replay output', coverage)
            corpus = campaign / 'corpus' / target
            digest, sizes = hashlib.sha256(), []
            for seed in sorted(corpus.iterdir(), key=lambda path: path.name.encode()):
                data = seed.read_bytes()
                digest.update(seed.name.encode() + b'\0' + data)
                sizes.append(len(data))
            corpora.append({'target': target, 'files': len(sizes), 'bytes': sum(sizes),
                            'sha256': digest.hexdigest(),
                            'hash_method': 'Ascending literal UTF-8 filename bytes, NUL, file contents'})
            print(f'Replaying {target}: {len(sizes)} fixed corpus files', flush=True)
            command = prefix + [str(args.cargo_fuzz.resolve()), 'coverage', target, str(corpus),
                                '--target-dir', str(target_dir), '--sanitizer', 'none', '--jobs', '4']
            run(command)
            profiles.append(coverage / 'coverage.profdata')
            binaries.append(target_dir / host / 'release' / target)
        merged = output / 'native.profdata'
        run([str(tools / 'llvm-profdata'), 'merge', '-sparse', '-o', str(merged)] + [str(p) for p in profiles])
        options = [str(binaries[0]), '-object', str(binaries[1]), '-instr-profile', str(merged)]
        options += [str(path) for path in sources]
        run([str(tools / 'llvm-cov'), 'report'] + options, output / 'native-report.txt')
        run([str(tools / 'llvm-cov'), 'export', '-summary-only'] + options, output / 'native-summary.json')
        run([str(tools / 'llvm-cov'), 'show', '-show-line-counts-or-regions', '-show-branches=count'] + options,
            output / 'native-lines.txt')
    summary = json.loads((output / 'native-summary.json').read_text())
    files = summary['data'][0]['files']
    requested = {str(path) for path in sources}
    assert files and all(row['filename'] in requested for row in files)
    assert str(package / 'src/codec.rs') in {row['filename'] for row in files}
    annotations, current_file = [], None
    for line in (output / 'native-lines.txt').read_text().splitlines():
        if line.endswith(':') and line[:-1] in requested:
            current_file = line[:-1]
        match = re.match(r'^\s*(\d+)\|\s*0\|(.*)$', line)
        if match:
            annotations.append({'file': current_file, 'line': int(match[1]), 'source': match[2].strip()})
    (output / 'native-zero-count-annotations.json').write_text(json.dumps(annotations, indent=2) + '\n')
    for entry in source_hashes:
        assert sha(package.parent / entry['path']) == entry['sha256']
    manifest = {'baseline_png_commit': 'fae2e5bd2', 'source_sha256': source_hashes,
                'campaign_manifest_sha256': sha(campaign / 'manifest.json'),
                'environment': {key: env[key] for key in ['RUSTFLAGS', 'CARGO_BUILD_JOBS', 'CC', 'CXX']},
                'rustc': rustc, 'commands': commands, 'replayed_corpora': corpora,
                'binaries': [{'path': str(path), 'sha256': sha(path)} for path in binaries],
                'profiles': [{'path': str(path), 'sha256': sha(path)} for path in profiles + [merged]],
                'runner_sha256': sha(Path(__file__)), 'files': files,
                'totals': summary['data'][0]['totals'], 'final_source_hashes_verified': True,
                'scope': 'Fixed replay of corpora grown using native-crate feedback. Reports include only explicit native source filenames; harnesses, references and stdlib source are excluded. Package totals include encoding utilities never called by these targets, so they are not decoder-only percentages. Replay is not additional fuzzing or proof of conformance.'}
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print((output / 'native-report.txt').read_text(), flush=True)
    print(json.dumps({'mapped_files': len(files), 'zero_count_annotations': len(annotations)}), flush=True)


if __name__ == '__main__':
    main()
