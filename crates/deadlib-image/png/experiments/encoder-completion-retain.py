"""Retain the final encoder audit after every recorded process succeeds."""
from pathlib import Path
import hashlib
import json
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


specs = [
    ('release', 'png-encoder-completion-release.log', 'cargo test -p deadlib-image -p deadlib-image-png --release -- --nocapture', (145, 0, 21)),
    ('oracles', 'png-encoder-completion-oracles.log', 'cargo test -p deadlib-image-png --release -- --ignored --nocapture', (21, 0, 0)),
    ('independent', 'png-encoder-completion-independent.log', './crates/deadlib-image/png/scripts/verify_independent.ps1 -LogPath target/png-encoder-completion-independent.log', (145, 0, 21)),
    ('clippy', 'png-encoder-clippy.log', 'cargo clippy -p deadlib-image -p deadlib-image-png --all-targets -- -D warnings', None),
    ('fmt', 'png-encoder-completion-fmt.log', 'cargo fmt -p deadlib-image -p deadlib-image-png -- --check', None),
]
checks = []
for name, source_name, invocation, expected in specs:
    source = target / source_name
    body = read(source)
    assert 'test result: FAILED' not in body, source
    stats = None
    if expected:
        summaries = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', body)
        stats = tuple(sum(int(m[i]) for m in summaries) for i in range(3))
        assert stats == expected, (name, stats, expected)
    if name == 'oracles':
        for text in ['C libpng encoder audit: 1620 reference files and 3240 native files',
                     'Verified 24 corpus files, 216 native-sample libpng comparisons',
                     'Regenerated 35 libpng/fraction APNG answers',
                     'Regenerated 240 independent libpng/Fraction precision animations']:
            assert text in body, text
    if name == 'clippy':
        assert 'Finished' in body and 'error:' not in body
    if name == 'fmt':
        assert not body.strip()
    destination = package / f'encoder-completion-{name}.log'
    destination.write_text(body, encoding='utf-8', newline='\n')
    checks.append({'command': invocation, 'exit_code': 0,
                   'exit_code_evidence': 'Observed terminal exec session result',
                   'passed': stats[0] if stats else None,
                   'failed': stats[1] if stats else None,
                   'ignored': stats[2] if stats else None,
                   'log': destination.name, 'sha256': sha(destination),
                   'original_log': str(source), 'original_sha256': sha(source),
                   'normalization': 'Decode PowerShell UTF-16/BOM when present; CRLF to LF; retain all lines and diagnostics'})

isolated_log = read(target / 'png-encoder-completion-independent.log')
isolated = Path(re.search(r'^Isolated workspace: (.+)$', isolated_log, re.M)[1])
copied = [package / 'Cargo.toml'] + [f for n in ['src', 'tests', 'benches', 'examples'] for f in (package / n).rglob('*') if f.is_file()]
for path in copied:
    assert sha(path) == sha(isolated / 'png' / path.relative_to(package)), path
for path in (package.parent / 'src').rglob('*.rs'):
    assert sha(path) == sha(isolated / path.relative_to(package.parent))
assert isolated_log.rstrip().endswith('deadlib-image v0.1.0 (' + str(isolated) + ')')
digest = hashlib.sha256()
for path in sorted(copied, key=lambda p: p.relative_to(package).as_posix().encode()):
    digest.update(path.relative_to(package).as_posix().encode() + b'\0' + bytes.fromhex(sha(path)))

fuzz_path = package / 'fuzz/reports/encoder-source.json'
fuzz = json.loads(fuzz_path.read_text())
assert fuzz['isolation_verified'] and fuzz['final_source_hashes_verified']
for item in fuzz['source_sha256']:
    assert sha(package.parent / item['path']) == item['sha256'], item
for item in fuzz['artifact_audit']:
    assert (item['counter_bytes'] > 0 and item['pc_table_bytes'] > 0) if item['native_feedback'] else (item['counter_bytes'] == item['pc_table_bytes'] == 0)
assert len(fuzz['artifact_audit']) == 16 and len(fuzz['harness_object_audit']) == 4
assert all(x['counter_bytes'] == x['pc_table_bytes'] == 0 for x in fuzz['harness_object_audit'])
assert {c['target']: c['runs'] for c in fuzz['campaigns']} == {'encode_stream': 30000, 'deflate_stream': 20000}
seeds = json.loads((package / 'fuzz/reports/encoder-seeds.json').read_text())
assert sha(target / 'png-encoder-20261028-seeds.zip') == seeds['archive_sha256'] == fuzz['seed_archive_sha256']
for item, campaign in zip(fuzz['commands'], fuzz['campaigns']):
    assert item['exit_code'] == 0 and campaign['asan_init_defined']
    log = package / f'fuzz/reports/encoder-{campaign["target"]}.log'
    assert sha(log) == item['log_sha256']
    assert f'Done {campaign["runs"]} runs' in read(log)
    assert campaign['initial_corpus']['sha256'] == seeds['targets'][campaign['target']]['sha256']

subprocess.run(['git', 'diff', '--exit-code', '38d14f427', '--', 'crates/deadlib-image/png/src', 'crates/deadlib-image/src'], cwd=root, check=True)
production = sorted((package / 'src').rglob('*.rs')) + sorted((package.parent / 'src').rglob('*.rs'))
report = {
    'date': '2026-10-28', 'status': 'complete', 'production_changes': [],
    'scope': 'PNG/APNG encoding through encode/encode_writer, native and reference precision, metadata, compression correctness, input rejection, IO and independent reuse. Performance is excluded by the user.',
    'baseline_production_commit': '38d14f427',
    'first_milestone_commit': '337da6b64',
    'git_head_at_validation': command(['git', 'rev-parse', 'HEAD']),
    'checks': checks,
    'production_sources': [{'path': p.relative_to(root).as_posix(), 'sha256': sha(p)} for p in production],
    'isolated_workspace': str(isolated), 'all_copied_package_files_match': True,
    'copied_package_files': len(copied), 'copied_input_sha256': digest.hexdigest(),
    'copied_input_digest_method': 'Ascending UTF-8 relative path, NUL, raw content SHA256 digest',
    'shared_sources_match': True, 'production_dependency_tree': 'deadlib-image-png -> deadlib-image only',
    'rustc': command(['rustc', '-Vv']), 'cargo': command(['cargo', '--version']),
    'python': command(['python', '--version']), 'cmake': command(['cmake', '--version']),
    'reference_libpng_commit': command(['git', '-C', str(target / 'deadlib-image-reference/libpng'), 'rev-parse', 'HEAD']),
    'reference_zlib_commit': command(['git', '-C', str(target / 'deadlib-image-reference/zlib'), 'rev-parse', 'HEAD']),
    'c_reference_build_commands': [
        'cmake -S crates/deadlib-image/png/tests/oracle -B target/png-encoder-reference -DREFERENCE_ROOT=C:/GitHub/deadsync/target/deadlib-image-reference -A x64',
        'cmake --build target/png-encoder-reference --config Release --target deadlib_png_encoder deadlib_png_oracle --parallel 4'],
    'reference_binaries': [{'path': str(target / f'png-encoder-reference/Release/{name}.exe'),
                            'sha256': sha(target / f'png-encoder-reference/Release/{name}.exe')}
                           for name in ['deadlib_png_encoder', 'deadlib_png_oracle']],
    'oracle_environment': {'DEADLIB_LIBPNG_ENCODER': str(target / 'png-encoder-reference/Release/deadlib_png_encoder.exe'),
                           'DEADLIB_LIBPNG_ORACLE': str(target / 'png-encoder-reference/Release/deadlib_png_oracle.exe'),
                           'DEADLIB_PNG_CORPUS': str(target / 'deadlib-image-reference/kodak')},
    'reference_helper_sources': [{'path': path.relative_to(package).as_posix(), 'sha256': sha(path)}
                                 for path in sorted((package / 'tests/oracle').glob('*')) if path.is_file()],
    'new_sanitizer_executions': 50000,
    'sanitizer_evidence': {'manifest': fuzz_path.relative_to(package).as_posix(), 'sha256': sha(fuzz_path),
                           'source_files_verified': len(fuzz['source_sha256']),
                           'native_archives': 2, 'dependency_archives_without_feedback': 14,
                           'harness_objects_without_feedback': 4, 'asan_runtime_verified': True,
                           'corpus_seed_manifest': 'fuzz/reports/encoder-seeds.json',
                           'corpus_seed_manifest_sha256': sha(package / 'fuzz/reports/encoder-seeds.json')},
    'encoder_conditions': ['conforming output datastreams', 'legal-depth scaling retains original high bits', 'only standardized method/type values are generated; no private methods exposed'],
    'reference_adapter_notes': ['Rust png exposes <=8-bit gray/RGB transparency keys as one byte per component',
                                'Rust png sub-byte Adam7 scatter uses a zero-initialized buffer',
                                'Rust png interlaced APNG subframes use canvas stride'],
    'specification': 'https://www.w3.org/TR/2025/REC-png-3-20250624/',
    'retainer_sha256': sha(Path(__file__)),
}
(package / 'encoder-completion-validation.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps({'checks': [{k: x[k] for k in ['passed', 'failed', 'ignored']} for x in checks],
                  'copied_files': len(copied), 'production_files': len(production),
                  'new_sanitizer_executions': 50000, 'current_fuzz_sources_verified': len(fuzz['source_sha256'])}, indent=2))
