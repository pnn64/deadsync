"""Review this adapter phase against its preserved bytes and actual results."""
import base64
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
read = lambda p: json.loads(p.read_text(encoding='utf-8-sig'))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
before = read(base / 'song-multitap-before-sources.json')
saved = read(base / 'song-multitap-edit-baseline.json')
crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
          'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}',
    'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
    *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
changed = sorted(p for p in current.keys() & before.keys() if current[p] != before[p])
added = sorted(current.keys() - before.keys())
assert not before.keys() - current.keys()
assert set(changed) <= set(saved)
assert added == sorted('crates/deadsync-song-lua/' + n for n in [
    'src/rust_engine/song_multitap.rs', 'src/rust_engine/actors/song_multitap.rs',
    'src/rust_engine/test_support/multitap.rs', 'tests/rust_song_multitap.rs'])
assert all(not p.startswith('crates/deadlib-lua/') for p in changed)
assert all(not p.startswith('crates/deadsync-song-lua/tests/') for p in changed)
assert all(current[p] == before[p] for p in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
for path, marker, replacement in [
    ('src/compile.rs', b'fn create_default_dummy_actor(', b'pub(crate) fn create_default_dummy_actor('),
    ('src/multitap.rs', b'const fn multitap_visible_start(', b'pub(crate) const fn multitap_visible_start('),
    ('src/rust_engine/song_prepare_globals.rs', b'    fn compiler_value_field(', b'    pub(super) fn compiler_value_field('),
]:
    path = 'crates/deadsync-song-lua/' + path
    old = base64.b64decode(saved[path]['base64'])
    assert old.count(marker) == 1
    expected = old.replace(marker, replacement)
    if path.endswith('song_prepare_globals.rs'):
        expected = subprocess.run(['rustfmt', '--edition', '2024', '--config',
            'skip_children=true', '--emit', 'stdout'], input=expected, cwd=root,
            capture_output=True, check=True).stdout
    assert (root / path).read_bytes() == expected, path
format_paths = [p for p in changed + added if p.endswith('.rs') and p not in [
    'crates/deadsync-song-lua/src/compile.rs', 'crates/deadsync-song-lua/src/multitap.rs']]
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true',
                *format_paths], cwd=root, check=True, capture_output=True)
assert sha(base / 'objective.txt') == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(base / 'checks.py') == '05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
assert len(sys.argv[1:]) == 3
results = {}
reg, focus, lint = sys.argv[1:]
preformat = read(base / 'song-multitap-before-format.json')
format_only = sorted(preformat)
assert format_only == ['crates/deadsync-song-lua/src/rust_engine.rs',
                       'crates/deadsync-song-lua/src/rust_engine/song_prepare_globals.rs']
for path, data in preformat.items():
    old = base64.b64decode(data['base64'])
    assert hashlib.sha256(old).hexdigest() == data['sha256']
    formatted = subprocess.run(['rustfmt', '--edition', '2024', '--config',
        'skip_children=true', '--emit', 'stdout'], input=old, cwd=root,
        capture_output=True, check=True).stdout
    assert (root / path).read_bytes() == formatted, path
for stage in [reg, focus, lint]:
    result = read(base / stage / 'result.json')
    snapshot = read(base / stage / 'sources.json')
    assert result['sources_unchanged']
    if stage == reg:
        assert snapshot.keys() == current.keys()
        for path in current:
            assert snapshot[path] == (preformat[path]['sha256'] if path in preformat else current[path]), path
    else:
        assert snapshot == current
    assert sha(base / stage / 'run.log') == result['log_sha256']
    results[stage] = result
a, b, c = results[reg], results[lint], results[focus]
assert a['exit_code'] == 0 and a['tests_failed'] == 0
assert a['tests_passed'] == 325 and len(a['test_results']) == 12
assert c['exit_code'] == 0 and c['tests_passed'] == 24 and c['tests_failed'] == 0
assert len(c['test_results']) == 2
assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in line for line in a['test_results'] + c['test_results'])
def diagnostics(p):
    return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', p.read_text()))
assert b['exit_code'] == 101
assert diagnostics(base / lint / 'run.log') == diagnostics(base / 'song-update-driver-lint-v3-song-linux-debug/run.log')
report = dict(goal_complete=False, production_cutover=False, final_application_validation=False,
    source_count=len(current), changed=changed, added=added,
    native_algorithms_byte_unchanged=True, engine_unchanged=True,
    existing_tests_byte_unchanged=True, formatting_passed=True,
    clippy_diagnostics_match_prior_baseline=True, tests_passed=a['tests_passed'],
    test_targets=len(a['test_results']), final_focused_tests=c['tests_passed'],
    broad_suite_followed_only_by_rustfmt=True, formatting_only_paths=format_only, results=results)
(base / 'song-multitap-reviewed-sources.json').write_text(json.dumps(current, indent=2) + '\n')
(base / 'song-multitap-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: v for k, v in report.items() if k != 'results'}))
