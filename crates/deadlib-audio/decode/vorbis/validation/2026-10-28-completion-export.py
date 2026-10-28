from pathlib import Path
import datetime
import hashlib
import json
import re
import subprocess

repo = Path.cwd()
decode = repo / 'crates/deadlib-audio/decode'
dest = decode / 'vorbis/validation'
scratch = repo / 'target/deadlib-audio-reference'
run = scratch / 'nil-eos-final-validation'
def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    path.write_bytes((json.dumps(value, indent=2, ensure_ascii=True) + '\n').encode())


def export(path, name):
    raw = path.read_bytes()
    value = raw.decode('utf-16' if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else 'utf-8-sig')
    output = dest / ('2026-10-28-completion-' + name)
    normalized = '\n'.join(line.rstrip() for line in value.splitlines()).rstrip()
    output.write_bytes((normalized + '\n' if normalized else '').encode())
    return {'path': output.relative_to(repo).as_posix(), 'sha256': sha(output),
            'original_path': path.relative_to(repo).as_posix(), 'original_sha256': sha(path)}


def counts(path):
    raw = path.read_bytes()
    value = raw.decode('utf-16' if raw[:2] in [b'\xff\xfe', b'\xfe\xff'] else 'utf-8-sig')
    rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out', value)
    assert rows, str(path)
    return dict(zip(['passed', 'failed', 'ignored', 'filtered_out'], [sum(int(r[i]) for r in rows) for i in range(4)]))


receipt = json.loads((run / 'checks.json').read_text(encoding='utf-8'))
assert receipt.get('completed_utc'), 'comprehensive checks still running'
assert len(receipt['commands']) == 8
assert all(r['exit_code'] == 0 for r in receipt['commands'])
for name, digest in receipt['source_sha256'].items():
    assert sha(decode / name) == digest, 'source changed: ' + name
receipt['scope'] = 'Final Ogg/Vorbis decoding completion checks, including unknown empty EOS; performance excluded.'
receipt['source_reverified_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
receipt['validated_codec_commit'] = subprocess.check_output(
    ['git', 'log', '-1', '--format=%H', '--', 'crates/deadlib-audio/decode/vorbis/src'], text=True).strip()
assert not subprocess.check_output(['git', 'diff', '--',
    'crates/deadlib-audio/decode/ogg/src', 'crates/deadlib-audio/decode/vorbis/src',
    'crates/deadlib-audio/decode/ogg/tests', 'crates/deadlib-audio/decode/vorbis/tests',
    'crates/deadlib-audio/decode/fuzz/src', 'crates/deadlib-audio/decode/fuzz/fuzz_targets'], text=True)
receipt['exports'] = {}
for command in receipt['commands']:
    name = command['name']
    path = run / (name + '.log')
    assert sha(path) == command['log_sha256']
    receipt['exports'][name] = export(path, name + '.log')
    if name in ['release', 'independent', 'fuzz-tests']:
        command['test_totals'] = counts(path)
receipt['exports']['driver'] = export(repo / 'target/vorbis-completion/final_decoder_checks.py', 'driver.py')
receipt['exports']['exporter'] = export(Path(__file__), 'export.py')
receipt['extracted_manifest_sha256'] = {}
for package in ['ogg', 'vorbis']:
    manifest = decode / package / 'Cargo.toml'
    copied = scratch / 'nil-eos-independent' / package / 'Cargo.toml'
    assert manifest.read_bytes() == copied.read_bytes()
    receipt['extracted_manifest_sha256'][package] = sha(manifest)
receipt['format'] = 'UTF-8, LF line endings, line-final whitespace removed. Original file hashes are retained.'
prior = json.loads((dest / '2026-10-28-reference-seeking-checks.json').read_text(encoding='utf-8'))
receipt['references'] = prior['references']
receipt['assets'] = prior['assets']
receipt['external_inputs'] = prior['external_inputs']
receipt['upstream'] = prior['upstream']
for package, ref in receipt['references'].items():
    root = scratch / package
    assert subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip() == ref['revision']
    assert not subprocess.check_output(['git', '-C', str(root), 'diff', '--', '.'], text=True)
    binary = scratch / (package + '-oracle-build') / 'Release' / ('deadlib_' + package + '_oracle.exe')
    assert sha(binary) == ref['binary_sha256']
for name, digest in receipt['assets'].items():
    assert sha(repo / name) == digest
for name, digest in receipt['external_inputs'].items():
    assert sha(scratch / 'corpus' / name) == digest
upstream = scratch / 'corpus/upstream-completion'
manifest = decode / 'vorbis/validation/2026-10-26-upstream.sha256'
entries = manifest.read_text(encoding='utf-8-sig').splitlines()
assert len(entries) == 528
for line in entries:
    digest, name = line.split(None, 1)
    assert sha(upstream / name.strip()) == digest
receipt['upstream']['final_manifest_path'] = manifest.relative_to(repo).as_posix()
receipt['upstream']['final_manifest_sha256'] = sha(manifest)
receipt['upstream']['final_files_reverified'] = 528

packrun = scratch / 'songpack-validation-nil-eos-20261028'
pack = json.loads((packrun / 'checks.json').read_text(encoding='utf-8'))
assert pack.get('completed_utc') and len(pack['commands']) == 8
assert all(r['exit_code'] == 0 for r in pack['commands'])
for name, digest in pack['source_sha256'].items():
    assert sha(decode / name) == digest, 'songpack source changed: ' + name
packroot = scratch / 'corpus' / pack['pack']['name']
for entry in pack['pack']['files']:
    path = packroot / entry['member']
    assert path.stat().st_size == entry['bytes'] and sha(path) == entry['sha256']
archive = packroot / 'archive.zip'
assert archive.stat().st_size == pack['pack']['archive']['bytes']
assert sha(archive) == pack['pack']['archive']['sha256']
pack['final_inputs_reverified'] = {'members': 64, 'archive': True}
pack['phase'] = 'All eight song-pack gates rerun after unknown empty EOS fix; final source hashes reverified.'
pack['exports'] = {}
for path in sorted(packrun.glob('*.log')):
    pack['exports'][path.stem] = export(path, 'songpacks-' + path.name)
    command = next(c for c in pack['commands'] if c['label'] == path.stem)
    assert sha(path) == command['log_sha256']
assert len(pack['exports']) == 8
for path in sorted(packrun.glob('*.tsv')):
    pack['exports'][path.name] = export(path, 'songpacks-' + path.name)
write_json(dest / '2026-10-28-completion-songpacks.json', pack)
receipt['songpacks'] = {'receipt': '2026-10-28-completion-songpacks.json', 'sha256': sha(dest / '2026-10-28-completion-songpacks.json')}

asanroot = scratch / 'unknown-nil-eos-asan'
asanlog = repo / 'target/vorbis-completion/linux-unknown-eos-asan.log'
asantext = asanlog.read_text(encoding='utf-8')
assert 'Done 6085 runs in 301 second(s)' in asantext
assert not any(asanroot.joinpath('artifacts').iterdir())
binary = '/tmp/deadlib-reference-seeking-asan/x86_64-unknown-linux-gnu/release/vorbis_stream'
digest = subprocess.check_output(['wsl', '-d', 'Debian', '--', 'sha256sum', binary], text=True).split()[0]
assert digest == '6dd4c4720882c106e881bd7d980b54e78c5cc46d77cc4802a9b0bff2061c7965'
initial = json.loads((asanroot / 'initial-corpus.sha256.json').read_text(encoding='utf-8'))
assert len(initial) == 61
for name, digest in initial.items():
    assert sha(asanroot / 'corpus' / name) == digest
receipt['sanitizer'] = {
    'platform': 'Debian WSL x86_64-unknown-linux-gnu', 'target': 'vorbis_stream',
    'nightly': '1.99.0-nightly (8ab9fdff5 2026-07-30)', 'llvm': '22.1.8', 'cargo_fuzz': '0.13.2',
    'binary': binary, 'binary_sha256': '6dd4c4720882c106e881bd7d980b54e78c5cc46d77cc4802a9b0bff2061c7965',
    'exit_code': 0, 'executions': 6085, 'seconds': 301, 'peak_rss_mb': 424,
    'initial_seeds': 61, 'new_units': 399, 'failure_artifacts': [],
    'args': ['-max_total_time=300', '-max_len=131073', '-timeout=20', '-rss_limit_mb=2048', '-seed=2744', '-print_final_stats=1',
             '-dict=/mnt/c/Github/deadsync/crates/deadlib-audio/decode/fuzz/ogg-vorbis.dict',
             '-artifact_prefix=/mnt/c/Github/deadsync/target/deadlib-audio-reference/unknown-nil-eos-asan/artifacts/',
             '/mnt/c/Github/deadsync/target/deadlib-audio-reference/unknown-nil-eos-asan/corpus'],
    'floor0': '/mnt/c/Github/deadsync/target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg',
    'environment_note': 'Windows ASAN runtime DLL unavailable; installed WSL sanitizer toolchain used.',
    'scope': 'Bounded native state/reset/pull/push/seek checks, including paired identical-history reference readers; no external C fuzz oracle.',
    'log': export(asanlog, 'asan.log'),
    'build_log': export(repo / 'target/vorbis-completion/linux-unknown-eos-build.log', 'asan-build.log'),
    'initial_manifest': export(asanroot / 'initial-corpus.sha256.json', 'asan-initial-corpus.sha256.json'),
}

oldlog = repo / 'target/vorbis-completion/final-full-reference-tests.log'
oldcounts = counts(oldlog)
assert oldcounts == {'passed': 219, 'failed': 0, 'ignored': 0, 'filtered_out': 17}
prior['completed_retained_block_full_reference_suite'] = {
    'phase': 'After retained-block fix, before song-pack and unknown-nil-EOS additions',
    'exit_code': 0, **oldcounts, 'log': export(oldlog, 'retained-block-full-reference.log')}
prior['baseline_full_reference_suite']['phase'] = 'Before retained-block correction; final retained-block rerun completed below.'
prior['pending_goal_validation'] = {'resolved': True, 'completion_receipt': '2026-10-28-completion-checks.json',
                                    'note': 'Original session 31858 completed successfully; later current-source gates recorded separately.'}
write_json(dest / '2026-10-28-reference-seeking-checks.json', prior)
receipt['fixture'] = {'unknown-nil-eos-v1.ogg': sha(decode / 'vorbis/tests/fixtures/unknown-nil-eos-v1.ogg')}
write_json(dest / '2026-10-28-completion-checks.json', receipt)
print('Exported final validation, song-pack rerun, sanitizer and resolved prior-suite receipt')
print([(r['name'], r.get('test_totals')) for r in receipt['commands']])
