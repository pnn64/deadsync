"""Retain the completed decoder audit; refuse incomplete or changed inputs."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

root = next(p for p in Path(__file__).resolve().parents if (p / 'crates/deadlib-image/png/Cargo.toml').is_file())
package = root / 'crates/deadlib-image/png'
target = root / 'target'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    raw = path.read_bytes()
    return raw.decode('utf-16' if raw[:2] == b'\xff\xfe' else 'utf-8-sig').replace('\r\n', '\n')


def command(args):
    return subprocess.check_output(args, cwd=root, text=True).strip()


def inventory(base, files):
    records = [{'path': p.relative_to(base).as_posix(), 'bytes': p.stat().st_size,
                'sha256': sha(p)} for p in sorted(files)]
    digest = hashlib.sha256()
    for item in sorted(records, key=lambda r: r['path'].encode('utf-8')):
        digest.update(item['path'].encode('utf-8') + b'\0')
        digest.update(bytes.fromhex(item['sha256']))
    return {'files': len(records), 'bytes': sum(x['bytes'] for x in records),
            'sha256': digest.hexdigest(),
            'hash_method': 'Ascending UTF-8 relative path, NUL, raw SHA256 content digest'}


specs = [
    ('release', 'cargo test -p deadlib-image -p deadlib-image-png --release -- --nocapture', (138, 0, 20)),
    ('oracles', 'cargo test -p deadlib-image-png --release -- --ignored --nocapture', (20, 0, 0)),
    ('independent', './crates/deadlib-image/png/scripts/verify_independent.ps1 -LogPath target/png-completion-independent.log', (138, 0, 20)),
    ('strict', 'cargo test -p deadlib-image-png --release --test zlib_headers c_zlib_header_and_history_oracle -- --ignored --nocapture', (1, 0, 0)),
    ('pkzip', 'cargo test -p deadlib-image-png --release --test huffman c_zlib_huffman_oracle -- --ignored --nocapture', (1, 0, 0)),
    ('clippy', 'cargo clippy -p deadlib-image -p deadlib-image-png --all-targets -- -D warnings', None),
    ('fmt', 'cargo fmt -p deadlib-image -p deadlib-image-png -- --check', None),
]
checks = []
for name, invocation, expected in specs:
    path = target / f'png-completion-{name}.log'
    body = read(path)
    assert 'test result: FAILED' not in body, path
    stats = None
    if expected:
        summaries = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', body)
        stats = tuple(sum(int(m[i]) for m in summaries) for i in range(3))
        assert stats == expected, (path, stats, expected)
    if name == 'oracles':
        assert 'Verified 24 corpus files, 216 native-sample libpng comparisons' in body
        assert 'Regenerated 35 libpng/fraction APNG answers' in body
        assert 'Regenerated 240 independent libpng/Fraction precision animations' in body
        assert 'rust_png_control_fields_and_all_delay_words ... ok' in body
    if name == 'clippy':
        assert 'Finished' in body and 'error:' not in body
    if name == 'fmt':
        assert not body.strip()
    destination = package / f'decoder-completion-{name}.log'
    destination.write_text(body, encoding='utf-8', newline='\n')
    checks.append({'name': name, 'command': invocation, 'exit_code': 0,
                   'exit_code_evidence': 'Observed terminal exec session result',
                   'passed': stats[0] if stats else None,
                   'failed': stats[1] if stats else None,
                   'ignored': stats[2] if stats else None,
                   'log': destination.name, 'sha256': sha(destination),
                   'original_log': str(path), 'original_sha256': sha(path),
                   'normalization': 'Decode PowerShell UTF-16/BOM when present; CRLF to LF; retain diagnostics and all lines'})

isolated_log = read(target / 'png-completion-independent.log')
isolated = Path(re.search(r'^Isolated workspace: (.+)$', isolated_log, re.M)[1])
copied = []
for entry in ['Cargo.toml', 'src', 'tests', 'benches', 'examples']:
    path = package / entry
    copied += [path] if path.is_file() else [p for p in path.rglob('*') if p.is_file()]
for path in copied:
    other = isolated / 'png' / path.relative_to(package)
    assert sha(path) == sha(other), (path, other)
for path in (package.parent / 'src').rglob('*'):
    if path.is_file():
        assert sha(path) == sha(isolated / path.relative_to(package.parent))
assert isolated_log.rstrip().endswith('deadlib-image v0.1.0 (' + str(isolated) + ')'), isolated_log[-300:]

fuzz_path = package / 'fuzz/reports/huffman-permutations-source.json'
fuzz = json.loads(fuzz_path.read_text())
for item in fuzz['source_sha256']:
    assert sha(package.parent / item['path']) == item['sha256'], item
assert fuzz['isolation_verified']
assert len(fuzz['artifact_audit']) == 16 and len(fuzz['harness_object_audit']) == 4
for item in fuzz['artifact_audit']:
    assert (item['counter_bytes'] > 0 and item['pc_table_bytes'] > 0) if item['native_feedback'] else (item['counter_bytes'] == item['pc_table_bytes'] == 0)
assert all(x['counter_bytes'] == x['pc_table_bytes'] == 0 for x in fuzz['harness_object_audit'])
for campaign in fuzz['campaigns']:
    assert campaign['runs'] == 100000 and campaign['asan_init_defined']
    text = read(package / f'fuzz/reports/huffman-permutations-{campaign["target"]}.log')
    assert 'Done 100000 runs' in text
    assert 'stat::number_of_executed_units: 100000' in text

production = sorted((package / 'src').rglob('*.rs')) + sorted((package.parent / 'src').rglob('*.rs'))
subprocess.run(['git', 'diff', '--exit-code', 'e30aac88a827ef45eb5928e557fb20ef74c1fadc', '--',
                'crates/deadlib-image/src', 'crates/deadlib-image/png/src'], cwd=root, check=True)
references = []
for kind in ['default', 'strict', 'pkzip']:
    suffix = '' if kind == 'default' else '-' + kind
    build = target / ('png-completion-oracle' + suffix)
    binary = build / 'Release/deadlib_png_oracle.exe'
    assert binary.is_file()
    flags = [] if kind == 'default' else ['-DDEADLIB_ZLIB_' + ('STRICT_WINDOW' if kind == 'strict' else 'PKZIP_WORKAROUND') + '=ON']
    references.append({'kind': kind, 'binary': str(binary), 'sha256': sha(binary),
                       'configure': ['cmake', '-S', 'crates/deadlib-image/png/tests/oracle', '-B', str(build),
                                     '-DREFERENCE_ROOT=' + str(target / 'deadlib-image-reference'), *flags, '-A', 'x64'],
                       'build': ['cmake', '--build', str(build), '--config', 'Release', '--target', 'deadlib_png_oracle', '--parallel', '4']})

report = {
    'date': '2026-10-28', 'status': 'complete',
    'scope': 'PNG/APNG native decoding, metadata, streaming, stored-sample composition, correctness and independent reuse. Performance, encoder completion and application integration excluded by the user scope.',
    'git_head_at_validation': command(['git', 'rev-parse', 'HEAD']),
    'production_baseline': 'e30aac88a827ef45eb5928e557fb20ef74c1fadc',
    'production_unchanged_from_baseline': True,
    'rustc': command(['rustc', '-Vv']), 'cargo': command(['cargo', '--version']),
    'python': command(['python', '--version']), 'cmake': command(['cmake', '--version']),
    'checks': checks,
    'production_sources': [{'path': p.relative_to(root).as_posix(), 'sha256': sha(p)} for p in production],
    'validation_input_tree': inventory(package, copied),
    'isolated_workspace': str(isolated), 'isolated_copied_png_files_verified': len(copied),
    'isolated_shared_sources_verified': True,
    'reference_helper': [{'path': p.relative_to(package).as_posix(), 'sha256': sha(p)} for p in sorted((package / 'tests/oracle').glob('*')) if p.is_file()],
    'verification_tools': [{'path': str(p.relative_to(package)), 'sha256': sha(p)} for p in
                           [package / 'scripts/verify_independent.ps1', package / 'scripts/generate_apng_render_goldens.py',
                            package / 'scripts/generate_compositor_goldens.py']],
    'references': references,
    'reference_libpng_commit': command(['git', '-C', str(target / 'deadlib-image-reference/libpng'), 'rev-parse', 'HEAD']),
    'reference_zlib_commit': command(['git', '-C', str(target / 'deadlib-image-reference/zlib'), 'rev-parse', 'HEAD']),
    'reference_zlib_worktree_note': 'CMake renames the source zconf.h; generated build-directory headers are used. No reference implementation edits were made.',
    'reference_corpus': inventory(target / 'deadlib-image-reference/kodak', (target / 'deadlib-image-reference/kodak').glob('*.png')),
    'optional_test_environment': {'DEADLIB_LIBPNG_ORACLE': str(target / 'png-completion-oracle/Release/deadlib_png_oracle.exe'),
                                  'DEADLIB_PNG_CORPUS': str(target / 'deadlib-image-reference/kodak')},
    'strict_test_environment': {'DEADLIB_ZLIB_STRICT_WINDOW': '1', 'DEADLIB_LIBPNG_ORACLE': str(target / 'png-completion-oracle-strict/Release/deadlib_png_oracle.exe')},
    'pkzip_test_environment': {'DEADLIB_ZLIB_PKZIP_WORKAROUND': '1', 'DEADLIB_LIBPNG_ORACLE': str(target / 'png-completion-oracle-pkzip/Release/deadlib_png_oracle.exe')},
    'reused_sanitizer_evidence': {'manifest': str(fuzz_path.relative_to(package)), 'sha256': sha(fuzz_path),
                                'source_files_rehashed': len(fuzz['source_sha256']), 'all_source_files_match': True,
                                'runs': {c['target']: c['runs'] for c in fuzz['campaigns']}, 'fresh_executions_this_audit': 0,
                                'native_archive_audits': 2, 'dependency_archive_audits': 14, 'harness_object_audits': 4},
    'coverage_review': ['coverage-reports/README.md', 'coverage-reports/boundaries-decoder-gaps.json'],
    'reference_policy_differences': ['reserved-bit ancillary acceptance', 'malformed core ordering recovery', 'pre-output references',
                                     'advertised-window enforcement', 'extended distance alphabets', 'libpng hIST ordering/drop'],
    'specification_checked': ['https://www.w3.org/TR/2025/REC-png-3-20250624/', 'https://www.w3.org/2025/06/REC-PNG-20250624-errata'],
    'retainer_script_sha256': sha(Path(__file__)),
}
(package / 'decoder-completion-validation.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps({'checks': [{k: x[k] for k in ['name', 'passed', 'failed', 'ignored']} for x in checks],
                  'copied_files_verified': len(copied), 'production_files': len(production),
                  'fuzz_source_files_match': len(fuzz['source_sha256'])}, indent=2))
