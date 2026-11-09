"""Verify finite compiler assembly against saved source bytes and actual checks."""
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
before = read(base / 'song-compiler-facade-before-sources-v2.json')
saved = read(base / 'song-compiler-facade-edit-baseline-v2.json')
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
    'src/rust_engine/compiler.rs', 'src/rust_engine/actors/compiler.rs',
    'tests/rust_song_compiler.rs'])
for name, row in saved.items():
    raw = base64.b64decode(row['base64'])
    assert hashlib.sha256(raw).hexdigest() == row['sha256'], name
    if name in before:
        assert row['sha256'] == before[name], name
assert all(not p.startswith('crates/deadlib-lua/') for p in changed)
assert all(not p.startswith('crates/deadsync-song-lua/tests/') for p in changed)
assert all(current[p] == before[p] for p in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])

def original(name):
    return base64.b64decode(saved['crates/deadsync-song-lua/' + name]['base64'])

for name, markers in [
    ('src/compile.rs', [b'fn merge_runtime_messages(', b'fn split_compiled_song_lua<']),
    ('src/perframe.rs', [b'fn startup_command(']),
]:
    old = original(name)
    for marker in markers:
        assert old.count(marker) == 1
        old = old.replace(marker, b'pub(crate) ' + marker)
    assert (root / 'crates/deadsync-song-lua' / name).read_bytes() == old, name

old = original('src/lua_util.rs')
pattern = rb'pub struct SongLuaStartupState \{\r?\n.*?\r?\n\}'
matches = list(re.finditer(pattern, old, re.S))
assert len(matches) == 1
start, end = matches[0].span()
expected = old[:start] + b'pub use crate::frame_capture::StartupState as SongLuaStartupState;' + old[end:]
assert (root / 'crates/deadsync-song-lua/src/lua_util.rs').read_bytes() == expected
startup = original('src/rust_engine/song_activation.rs')
pattern = rb'/// Owned compiler output for one startup actor; no retained Lua allocations\.\r?\n#\[derive\(Clone, Debug, PartialEq\)\]\r?\npub struct SongStartupState \{\r?\n.*?\r?\n\}'
matches = list(re.finditer(pattern, startup, re.S))
assert len(matches) == 1
start, end = matches[0].span()
expected = startup[:start] + b'pub use crate::frame_capture::StartupState as SongStartupState;' + startup[end:]
assert (root / 'crates/deadsync-song-lua/src/rust_engine/song_activation.rs').read_bytes() == expected
manifest = root / 'crates/deadsync-song-lua/Cargo.toml'
old = original('Cargo.toml')
assert manifest.read_bytes().startswith(old)
assert manifest.read_bytes()[len(old):].decode().replace('\r\n', '\n') == '\n[[test]]\nname = "rust_song_compiler"\nrequired-features = ["rust-engine", "test-support"]\n'
format_paths = [p for p in changed + added if p.endswith('.rs') and p not in [
    'crates/deadsync-song-lua/src/' + n for n in ['compile.rs', 'perframe.rs', 'lua_util.rs']]]
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true',
                *format_paths], cwd=root, check=True, capture_output=True)
assert sha(base / 'objective.txt') == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(base / 'checks.py') == '05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
if sys.argv[1:] == ['--preflight']:
    print(json.dumps(dict(source_count=len(current), changed=changed, added=added,
                          source_and_format_review_passed=True)))
    sys.exit(0)
assert len(sys.argv[1:]) == 4
prior, reg, focus, lint = sys.argv[1:]
alias = read(base / 'song-compiler-facade-before-type-alias.json')
alias_path = 'crates/deadsync-song-lua/src/rust_engine/compiler.rs'
assert list(alias) == [alias_path]
old = base64.b64decode(alias[alias_path]['base64'])
assert hashlib.sha256(old).hexdigest() == alias[alias_path]['sha256']
newline = b'\r\n' if b'\r\n' in old else b'\n'
marker = b'type Output<S, V> = CompiledSongLua<SongLuaOverlayActor<Kind<S, V>>>;'
argument = b'    slots: fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>,'
assert old.count(marker) == 1 and old.count(argument) == 1
expected = old.replace(marker, marker + newline +
    b'type ModelSlots<S> = fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>;')
expected = expected.replace(argument, b'    slots: ModelSlots<S>,')
assert (root / alias_path).read_bytes() == expected
live_fix = read(base / 'song-compiler-facade-before-live-fix.json')
live_path = 'crates/deadsync-song-lua/src/rust_engine/actors/song_activation.rs'
assert list(live_fix) == [live_path]
old = base64.b64decode(live_fix[live_path]['base64'])
assert hashlib.sha256(old).hexdigest() == live_fix[live_path]['sha256']
newline = b'\r\n' if b'\r\n' in old else b'\n'
marker = b'                    capture.events.clear();'
assert old.count(marker) == 1
replacement = newline.join([b'                    if !data.context.live {',
    b'                        capture.events.clear();', b'                    }'])
assert (root / live_path).read_bytes() == old.replace(marker, replacement)
prior_sources = {**current, alias_path: alias[alias_path]['sha256'],
                 live_path: live_fix[live_path]['sha256']}
previous = read(base / prior / 'result.json')
assert read(base / prior / 'sources.json') == prior_sources
assert previous['sources_unchanged'] and previous['exit_code'] == 101
assert previous['tests_failed'] == 1
assert previous['test_results'][0].startswith('test result: ok. 849 passed; 0 failed;')
assert sha(base / prior / 'run.log') == previous['log_sha256']
assert '    startup_broadcast_capture_preserves_phase_params_and_nested_delivery' in (base / prior / 'run.log').read_text()
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
assert results[focus]['tests_passed'] == 12
assert results[reg]['tests_passed'] == 386 and len(results[reg]['test_results']) == 18
assert any(line.startswith('test result: ok. 212 passed;') for line in results[reg]['test_results'])
timing = (base / focus / 'run.log').read_text()
timing_counts = [len(re.findall(r'Song lua update timing: frames=\d+ reset_ms=', case))
                 for case in re.split(r'(?m)^test \w+ \.\.\. ', timing)[1:]]
assert len(timing_counts) == 12 and sum(timing_counts) >= 2
assert all(n in [0, 2] for n in timing_counts), timing_counts

def diagnostics(p):
    return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', p.read_text()))

assert results[lint]['exit_code'] == 101
assert diagnostics(base / lint / 'run.log') == diagnostics(base / 'song-multitap-lint-v3-song-linux-debug/run.log')
report = dict(goal_complete=False, production_cutover=False, final_application_validation=False,
    source_count=len(current), changed=changed, added=added,
    native_algorithms_byte_unchanged=True, shared_startup_type_extracted=True,
    engine_unchanged=True, existing_tests_byte_unchanged=True, formatting_passed=True,
    clippy_diagnostics_match_prior_baseline=True,
    tests_passed=results[reg]['tests_passed'], test_targets=len(results[reg]['test_results']),
    final_focused_tests=results[focus]['tests_passed'], timing_diagnostic_observed=True,
    type_alias_equivalence_byte_verified=True, timing_rows=sum(timing_counts),
    prior_native_library_tests=849, prior_run=previous,
    prior_run_followed_only_by_two_rust_adapter_changes=True,
    final_rust_checks_cover_all_212_activation_cases=True,
    reference_only_setter_fields_pending=True, live_session_integration_pending=True,
    results=results)
(base / 'song-compiler-facade-reviewed-sources.json').write_text(json.dumps(current, indent=2) + '\n')
(base / 'song-compiler-facade-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: v for k, v in report.items() if k != 'results'}))
