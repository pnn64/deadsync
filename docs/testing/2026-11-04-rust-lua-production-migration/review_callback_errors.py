"""Audit categorized compiler callback errors and their final-source checks."""
import base64
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
read = lambda p: json.loads(p.read_text(encoding='utf-8-sig'))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
before = read(base / 'song-callback-errors-before-sources.json')
saved = read(base / 'song-callback-errors-edit-baseline.json')
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
    'src/rust_engine/actors/diagnostics.rs', 'tests/rust_song_diagnostics.rs'])
for name, row in saved.items():
    raw = base64.b64decode(row['base64'])
    assert hashlib.sha256(raw).hexdigest() == row['sha256'] == before[name], name
assert all(not p.startswith('crates/deadlib-lua/') for p in changed)
assert all(not p.startswith('crates/deadsync-song-lua/tests/') for p in changed)
assert all(p == 'crates/deadsync-song-lua/Cargo.toml' or
           p.startswith('crates/deadsync-song-lua/src/rust_engine/') or
           p == 'crates/deadsync-song-lua/src/rust_engine.rs' for p in changed)
assert all(current[p] == before[p] for p in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
assert all(b'CALLBACK_ERROR_DEBUG' not in (root / p).read_bytes()
           for p in changed + added if p.endswith('.rs'))
manifest = root / 'crates/deadsync-song-lua/Cargo.toml'
old = base64.b64decode(saved[manifest.relative_to(root).as_posix()]['base64'])
assert manifest.read_bytes().startswith(old)
a = tomllib.loads(old.decode('utf-8-sig'))
b = tomllib.loads(manifest.read_text(encoding='utf-8-sig'))
assert b.pop('test') == a.pop('test') + [dict(name='rust_song_diagnostics',
    **{'required-features': ['rust-engine', 'test-support']})]
assert a == b
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true',
                *[p for p in changed + added if p.endswith('.rs')]],
               cwd=root, check=True, capture_output=True)
assert sha(base / 'objective.txt') == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(base / 'checks.py') == '05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'

def cases(text):
    matches = list(re.finditer(r'(?m)^#\[test\]\nfn (\w+)\(\) \{', text))
    return {m[1]: text[m.start():matches[i+1].start() if i+1 < len(matches) else len(text)].strip()
            for i, m in enumerate(matches)}

old_tests = cases((base / 'song-callback-errors-tests-before-v2.rs').read_text())
new_tests = cases((root / 'crates/deadsync-song-lua/tests/rust_song_diagnostics.rs').read_text())
recurrent = 'queued_and_recurring_failures_preserve_dispatch_frames_and_command_labels'
for name in old_tests.keys() - {recurrent}:
    assert old_tests[name] == new_tests[name], name
original_lua = old_tests[recurrent].split('r#"', 1)[1].split('"#', 1)[0]
assert original_lua in new_tests['startup_tween_queue_failure_still_rejects_preparation']
assert 'native.unwrap_err()' in new_tests['startup_tween_queue_failure_still_rejects_preparation']
assert 'rust.unwrap_err()' in new_tests['startup_tween_queue_failure_still_rejects_preparation']
assert len(new_tests) == 11
if sys.argv[1:] == ['--preflight']:
    print(json.dumps(dict(source_count=len(current), changed=changed, added=added,
                          source_and_format_review_passed=True)))
    sys.exit(0)
assert len(sys.argv[1:]) == 3
reg, focus, lint = sys.argv[1:]
results = {}
for stage in [reg, focus, lint]:
    result = read(base / stage / 'result.json')
    assert result['sources_unchanged']
    assert read(base / stage / 'sources.json') == current
    assert sha(base / stage / 'run.log') == result['log_sha256']
    results[stage] = result
for stage in [reg, focus]:
    result = results[stage]
    assert result['exit_code'] == 0 and result['tests_failed'] == 0
    assert result['tests_passed'] > 0
    assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in line
               for line in result['test_results'])
assert results[reg]['tests_passed'] == 445 and len(results[reg]['test_results']) == 21
assert results[focus]['tests_passed'] == 37 and len(results[focus]['test_results']) == 3
assert any(line.startswith('test result: ok. 212 passed;') for line in results[reg]['test_results'])
assert any(line.startswith('test result: ok. 11 passed;') for line in results[focus]['test_results'])

def diagnostics(path):
    return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text()))

assert results[lint]['exit_code'] == 101
assert diagnostics(base / lint / 'run.log') == diagnostics(base / 'song-compiler-facade-lint-v2-song-linux-debug/run.log')
report = dict(goal_complete=False, production_cutover=False, final_application_validation=False,
    source_count=len(current), changed=changed, added=added,
    native_algorithms_byte_unchanged=True, engine_unchanged=True,
    existing_tests_byte_unchanged=True, formatting_passed=True,
    clippy_diagnostics_match_prior_baseline=True,
    tests_passed=results[reg]['tests_passed'], test_targets=len(results[reg]['test_results']),
    final_focused_tests=results[focus]['tests_passed'], callback_error_tests=len(new_tests),
    original_native_rejected_fixture_preserved=True,
    engine_error_rendering_normalized_only_in_new_diagnostic_tests=True,
    reference_only_setter_fields_pending=True, live_session_integration_pending=True,
    results=results)
(base / 'song-callback-errors-reviewed-sources.json').write_text(json.dumps(current, indent=2) + '\n')
(base / 'song-callback-errors-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: v for k, v in report.items() if k != 'results'}))
