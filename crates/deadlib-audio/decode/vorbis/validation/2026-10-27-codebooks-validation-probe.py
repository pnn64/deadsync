from pathlib import Path
from datetime import datetime, timezone
import csv, hashlib, importlib.util, json, py_compile, subprocess, sys

repo = Path.cwd(); decode = repo / 'crates/deadlib-audio/decode'; scratch = repo / 'target/deadlib-audio-reference'
run = scratch / 'codebook-profile-validation-20261027'; run.mkdir(exist_ok=False)
script = decode / 'scripts/profile_codebooks.py'; driver = decode / 'vorbis/benches/support/codebook_profile.rs'
sys.path.insert(0, str(script.parent))
spec = importlib.util.spec_from_file_location('codebook_probe', script)
probe = importlib.util.module_from_spec(spec); spec.loader.exec_module(probe)
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return p.read_text(encoding='utf8')
def rows(p): return list(csv.DictReader(read(p).splitlines(), delimiter='\t'))
source = read(decode / 'vorbis/src/codebook.rs')
for text in (source.replace('pub struct Codebook {', 'pub struct Changed {', 1), probe.instrument(source)):
    try: probe.instrument(text)
    except ValueError: pass
    else: raise AssertionError('instrumentation must reject changed/repeated anchors')
py_compile.compile(str(script), doraise=True)
checks = {'started_utc': datetime.now(timezone.utc).isoformat(), 'commands': [],
    'syntax_and_unique_anchor_failure_checks': True, 'runs': [], 'raw_channel_samples_bit_compared': 0}
previous = json.loads(read(scratch / 'residue-cursor-profile-20261027/manifest.json'))
previous_cases = {row['path']: row for row in previous['files']}
compared_previous = 0
for name, folder in [('main', 'codebook-profile-checked-20261027'), ('floor0', 'codebook-profile-floor0-checked-20261027')]:
    root = scratch / folder; m = json.loads(read(root / 'manifest.json'))
    assert m['source_input_binary_hashes_rechecked'] and m['completed_utc']
    for mapping in ('source_sha256', 'input_sha256'):
        for path, h in m[mapping].items(): assert sha(repo / path) == h, path
    assert sha(repo / m['binary_path']) == m['binary_sha256']
    for path, h in m['generated_sha256'].items(): assert sha(root / path) == h, path
    for path, h in m['worker_log_sha256'].items(): assert sha(root / path) == h, path
    for path, h in m['table_sha256'].items(): assert sha(root / path) == h, path
    for p in (decode / 'vorbis/src').glob('*.rs'):
        expected = probe.instrument(read(p)) if p.name == 'codebook.rs' else read(p)
        assert (root / 'profiled/src' / p.name).read_bytes() == expected.encode()
    assert (root / 'src/main.rs').read_bytes() == driver.read_bytes()
    assert len(rows(root / 'books.tsv')) == m['book_rows']
    assert len(rows(root / 'symbols.tsv')) == m['symbol_rows']
    for case in m['files']:
        assert case['bit_exact'] and case['instrumentation_self_tests_passed']
        checks['raw_channel_samples_bit_compared'] += case['raw_channel_samples']
        if case['path'] in previous_cases:
            old = previous_cases[case['path']]
            for key in ('raw_channel_samples', 'audio_packets', 'discarded_packets', 'truncated_packets', 'zero_span_granules', 'pcm_bits_fnv1a64'): assert case[key] == old[key], (case['path'], key)
            compared_previous += 1
    if name == 'main':
        assert len(m['files']) == 7 and all(case['zero_span_granules'] == 0 for case in m['files'])
        assert sum(case['truncated_packets'] for case in m['files']) == 4
    else:
        assert len(m['files']) == 1 and m['files'][0]['zero_span_granules'] == 1
        binary = repo / m['binary_path']
        args = [str(binary), str(repo / m['files'][0]['path']), '0']
        result = subprocess.run(args, cwd=repo, capture_output=True)
        assert result.returncode != 0 and b'granule' in result.stderr.lower()
        (run / 'floor0-strict.log').write_bytes(result.stderr.replace(b'\r\n', b'\n'))
        checks['strict_original_floor0_rejected'] = {'command': args, 'exit_code': result.returncode}
    args = ['cargo', 'clippy', '--offline', '--release', '--manifest-path', str(root / 'Cargo.toml'), '--all-targets', '--', '-D', 'warnings']
    result = subprocess.run(args, cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (run / (name + '-clippy.log')).write_bytes(result.stdout.replace(b'\r\n', b'\n'))
    assert result.returncode == 0, result.stdout.decode()
    checks['commands'].append({'command': args, 'exit_code': result.returncode, 'log': name + '-clippy.log'})
    checks['runs'].append({'name': name, 'folder': folder, 'manifest_sha256': sha(root / 'manifest.json'),
        'source_input_generated_worker_table_binary_hashes_rechecked': True, 'instrumented_source_reconstructs_exactly': True})
assert compared_previous == 6
assert checks['raw_channel_samples_bit_compared'] == 42738816
checks['six_prior_synthesis_pcm_digests_and_counts_match'] = True
for args in [
    ['cargo', 'fmt', '-p', 'deadlib-audio-decode-ogg', '-p', 'deadlib-audio-decode-vorbis', '--', '--check'],
    ['rustfmt', '--check', '--edition', '2024', str(driver)],
]:
    result = subprocess.run(args, cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0, result.stdout.decode()
    checks['commands'].append({'command': args, 'exit_code': result.returncode})
assert not subprocess.check_output(['git', 'diff', 'HEAD', '--', str(decode / 'ogg/src'), str(decode / 'vorbis/src')])
checks['production_sources_unchanged_from_head'] = True
checks['completed_utc'] = datetime.now(timezone.utc).isoformat()
(run / 'checks.json').write_bytes((json.dumps(checks, indent=2) + '\n').encode())
print('Checked eight files, 42,738,816 sample bit patterns, counter self-tests, source reconstruction, formatting and release Clippy.')
