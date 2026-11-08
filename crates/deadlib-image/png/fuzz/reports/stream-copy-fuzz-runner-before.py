"""Run ASan fuzzing with feedback restricted to the two native image crates.

Linux, nightly Rust and cargo-fuzz are required. Output and target directories
must be fresh. The seed ZIP contains a directory for each selected target.
Invocation records and compiled object sections independently audit isolation.
"""
from argparse import ArgumentParser
from pathlib import Path, PurePosixPath
import hashlib
import json
import os
import re
import subprocess
import time
import zipfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def audit_build(target_dir, output, targets=('png_stream', 'inflate_stream')):
    audit_dir = output / 'objects'
    audit_dir.mkdir(exist_ok=True)
    artifacts = []
    # Cargo's artifact layout can use per-package build/<name>/<hash>/out
    # directories instead of release/deps. Search the selected target subtree.
    dependencies = target_dir / 'x86_64-unknown-linux-gnu/release'

    def inspect_object(path):
        sections = subprocess.check_output(['readelf', '-SW', str(path)], text=True)
        found = re.findall(r'\[\s*\d+\]\s+(__sancov_(?:cntrs|pcs))\s+\S+\s+[0-9a-f]+\s+[0-9a-f]+\s+([0-9a-f]+)', sections)
        return {'counter_bytes': sum(int(size, 16) for name, size in found if name == '__sancov_cntrs'),
                'pc_table_bytes': sum(int(size, 16) for name, size in found if name == '__sancov_pcs')}

    for archive in sorted(dependencies.rglob('*.rlib')):
        crate = archive.name[3:].rsplit('-', 1)[0]
        counters = tables = objects = 0
        members = subprocess.check_output(['ar', 't', str(archive)], text=True).splitlines()
        for index, member in enumerate(members):
            if not member.endswith('.o'):
                continue
            path = audit_dir / f'{crate}-{archive.stem.rsplit("-", 1)[1]}-{index}.o'
            path.write_bytes(subprocess.check_output(['ar', 'p', str(archive), member]))
            stats = inspect_object(path)
            counters += stats['counter_bytes']
            tables += stats['pc_table_bytes']
            objects += 1
        native = crate in {'deadlib_image_png', 'deadlib_image'}
        assert (counters > 0 and tables > 0) if native else (counters == 0 and tables == 0), crate
        artifacts.append({'crate': crate, 'path': str(archive), 'sha256': sha(archive),
                          'objects': objects, 'native_feedback': native,
                          'counter_bytes': counters, 'pc_table_bytes': tables})
    assert {'deadlib_image_png', 'deadlib_image'} <= {item['crate'] for item in artifacts}
    harnesses = []
    for target in targets:
        objects = sorted(dependencies.rglob(target + '*.rcgu.o'))
        assert objects, ('Missing saved harness object', target)
        for path in objects:
            stats = inspect_object(path)
            assert stats['counter_bytes'] == stats['pc_table_bytes'] == 0
            harnesses.append({'path': str(path), 'sha256': sha(path), **stats})
    invocations = [json.loads(path.read_text()) for path in sorted((output / 'invocations').glob('*.json'))]
    assert any(item['crate'] == 'deadlib_image_png' for item in invocations)
    assert all(not item['removed_feedback_arguments'] for item in invocations if item['native_feedback'])
    for item in invocations:
        if not item['native_feedback']:
            assert not any('sanitizer-coverage-' in flag or 'sancov' in flag for flag in item['passed_arguments'])
    return {'artifact_audit': artifacts, 'harness_object_audit': harnesses,
            'compiler_invocations': invocations, 'isolation_verified': True}


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument('--cargo-fuzz', type=Path, required=True)
    parser.add_argument('--seed-archive', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--targets', nargs='+', choices=['png_stream', 'inflate_stream', 'decoder_limits'], default=['png_stream', 'inflate_stream'])
    args = parser.parse_args()
    assert os.name == 'posix', 'Use Linux for the ASan campaign'
    package = Path(__file__).resolve().parent.parent
    output = args.output_dir.resolve()
    output.mkdir(parents=True)
    target_dir = args.target_dir.resolve()
    target_dir.mkdir(parents=True)
    wrapper = output / 'native_rustc.py'
    wrapper.write_bytes((package / 'fuzz/scripts/native_rustc.py').read_bytes())
    wrapper.chmod(0o755)
    corpus_root = output / 'corpus'
    corpus_root.mkdir()
    with zipfile.ZipFile(args.seed_archive) as archive:
        for item in archive.infolist():
            name = PurePosixPath(item.filename)
            assert not name.is_absolute() and '..' not in name.parts
            assert len(name.parts) == 2 and name.parts[0] in args.targets
            assert item.file_size <= 65536
            destination = corpus_root.joinpath(*name.parts)
            destination.parent.mkdir(exist_ok=True)
            assert not destination.exists()
            destination.write_bytes(archive.read(item))
    env = os.environ.copy()
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    env.pop('RUSTC_WORKSPACE_WRAPPER', None)
    env.update(RUSTFLAGS='-Csave-temps', RUSTC_WRAPPER=str(wrapper),
               DEADLIB_PNG_NATIVE_FUZZ_AUDIT=str(output / 'invocations'),
               CARGO_BUILD_JOBS='4', CC='/usr/bin/clang', CFLAGS='-fsanitize=address')
    source_paths = sorted((package / 'src').rglob('*.rs')) + sorted((package / 'fuzz/fuzz_targets').glob('*.rs')) + sorted((package / 'fuzz/support').glob('*.rs')) + sorted((package / 'tests/support').glob('*.rs')) + [
        package.parent / 'src/lib.rs', package / 'fuzz/Cargo.toml', package / 'fuzz/Cargo.lock',
        package / 'fuzz/scripts/native_rustc.py', Path(__file__).resolve()]
    sources = [{'path': p.relative_to(package.parent).as_posix(), 'sha256': sha(p)} for p in source_paths]
    metadata = {'baseline_png_commit': 'fae2e5bd2', 'source_sha256': sources,
                'seed_archive_sha256': sha(args.seed_archive), 'commands': [], 'campaigns': [],
                'environment': {key: env[key] for key in ['RUSTFLAGS', 'RUSTC_WRAPPER',
                    'DEADLIB_PNG_NATIVE_FUZZ_AUDIT', 'CARGO_BUILD_JOBS', 'CC', 'CFLAGS']},
                'rustc': subprocess.check_output(['rustup', 'run', 'nightly', 'rustc', '-vV'], text=True),
                'scope': 'Feedback from native image crates only. Harnesses/reference crates retain ASan but no SanitizerCoverage. Native-crate std/alloc monomorphizations can contribute feedback. This is not source-only coverage or conformance proof.'}
    manifest = output / 'manifest.json'

    def save():
        manifest.write_text(json.dumps(metadata, indent=2) + '\n')

    for target in args.targets:
        runs = {'png_stream': 50000, 'inflate_stream': 20000, 'decoder_limits': 30000}[target]
        corpus = corpus_root / target
        digest, sizes = hashlib.sha256(), []
        for seed in sorted(corpus.iterdir(), key=lambda p: p.name.encode()):
            data = seed.read_bytes()
            digest.update(seed.name.encode() + b'\0' + data)
            sizes.append(len(data))
        initial = {'files': len(sizes), 'bytes': sum(sizes), 'maximum_file_bytes': max(sizes),
                   'sha256': digest.hexdigest(), 'hash_method': 'Ascending literal UTF-8 filename bytes, NUL, file contents'}
        command = ['rustup', 'run', 'nightly', str(args.cargo_fuzz.resolve()), 'run', target,
                   str(corpus), '--target-dir', str(target_dir), '--', f'-runs={runs}',
                   '-seed=928671', '-max_len=65536', '-timeout=10', '-rss_limit_mb=1024', '-print_final_stats=1']
        log = output / f'{target}.log'
        print(f'Running native-feedback {target}: {runs} executions, {len(sizes)} seeds', flush=True)
        began = time.monotonic()
        with log.open('wb') as stream:
            result = subprocess.run(command, cwd=package, env=env, stdout=stream, stderr=subprocess.STDOUT)
        metadata['commands'].append({'command': command, 'exit_code': result.returncode,
                                     'wall_seconds': round(time.monotonic() - began, 3),
                                     'log': str(log), 'log_sha256': sha(log)})
        save()
        assert result.returncode == 0, log
        text = log.read_text(errors='replace')
        actual = int(re.search(r'stat::number_of_executed_units:\s+(\d+)', text)[1])
        assert actual == runs
        binary = target_dir / 'x86_64-unknown-linux-gnu/release' / target
        symbols = subprocess.check_output(['nm', str(binary)], text=True, stderr=subprocess.DEVNULL)
        assert re.search(r'^[0-9a-f]+ T __asan_init$', symbols, re.M)
        campaign = {'target': target, 'runs': actual, 'initial_corpus': initial,
                    'binary': str(binary), 'binary_sha256': sha(binary), 'asan_init_defined': True,
                    'fuzz_seconds': int(re.search(r'Done \d+ runs in (\d+) second', text)[1]),
                    'peak_rss_mib': int(re.search(r'stat::peak_rss_mb:\s+(\d+)', text)[1])}
        metadata['campaigns'].append(campaign)
        save()
        print(json.dumps(campaign), flush=True)

    audit = audit_build(target_dir, output, args.targets)
    metadata.update(audit)
    for entry in sources:
        assert sha(package.parent / entry['path']) == entry['sha256']
    metadata['final_source_hashes_verified'] = True
    save()
    print(json.dumps({'isolation_verified': True, 'archive_audits': len(audit['artifact_audit']),
                      'harness_objects': len(audit['harness_object_audit']), 'successful_executions': sum(row['runs'] for row in metadata['campaigns'])}), flush=True)


if __name__ == '__main__':
    main()
