"""Verify SCREENMAN publication/request migration and its exact validated sources."""
from pathlib import Path
import hashlib
import json
import subprocess
import tomllib

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
tests_dir, lint_dir = base / 'screen-manager-adapters-song-linux-debug', base / 'screen-manager-lint-song-linux-debug'
tests, lint = read(tests_dir / 'result.json'), read(lint_dir / 'result.json')
assert tests['exit_code'] == 0 and tests['tests_failed'] == 0
assert tests['sources_unchanged'] and lint['sources_unchanged'] and lint['exit_code'] == 101
for directory, result in [(tests_dir, tests), (lint_dir, lint)]:
    assert sha(directory / 'run.log') == result['log_sha256']

crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
          'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output([
    'git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached', '--others',
    '--exclude-standard', '-z', '--', *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert current == read(tests_dir / 'sources.json') == read(lint_dir / 'sources.json')

targets = sorted(t['name'] for t in tomllib.loads(
    (root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
command = tests['command']
assert targets == sorted(command[i + 1] for i, value in enumerate(command) if value == '--test')
assert len(tests['test_results']) == len(targets)
previous = read(base / 'input-final-review.json')
before = read(base / 'input-adapters-song-linux-debug/sources.json')
core_run = read(base / 'actor-meshes-final-core-core-linux-debug/sources.json')
core = {k: v for k, v in core_run.items() if k.startswith('crates/deadlib-lua/')
        or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k, v in core.items())
live_name = 'crates/deadsync-song-lua/src/live.rs'
native = [k for k in previous['native_controls_unchanged'] if k != live_name]
assert all(current[k] == before[k] for k in native)
baseline = read(base / 'screen-manager-edit-baseline.json')
old_live = baseline[live_name]['source'].encode()
new_live = (root / live_name).read_bytes()
assert hashlib.sha256(old_live).hexdigest() == before[live_name] == baseline[live_name]['sha256']
assert new_live.startswith(old_live)
append = new_live[len(old_live):]
expected = '''
#[cfg(all(feature = "rust-engine", feature = "test-support"))]
pub(crate) fn migration_take_output(lua: &Lua) -> mlua::Result<SongLuaLiveOutput> {
    take_output(lua)
}
'''
assert append.decode() == expected
formatted = subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout'],
                           input=expected.lstrip(), text=True, capture_output=True, check=True).stdout
assert formatted == append.decode().lstrip()
append_report = {'original_bytes': len(old_live), 'appended_bytes': len(append),
                 'original_sha256': before[live_name], 'sha256': current[live_name],
                 'append_sha256': hashlib.sha256(append).hexdigest(),
                 'original_prefix_unchanged': True, 'appended_accessor_format_checked': True}
fixtures = {p: sha(root / p) for p in previous['fixtures']}
assert fixtures == previous['fixtures']
formatting = read(base / 'screen-manager-format.json')
assert formatting['exit_code'] == 0
assert all(current[k] == v for k, v in formatting['files'].items())

old_log = (base / 'input-lint-song-linux-debug/run.log').read_bytes()
new_log = (lint_dir / 'run.log').read_bytes()


def library_diagnostics(data):
    lines = data.splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.startswith(b'    Checking deadsync-song-lua '))
    return b''.join(lines[start + 1:]), b''.join(lines[:start])


old_library, old_prelude = library_diagnostics(old_log)
new_library, new_prelude = library_diagnostics(new_log)
assert old_library == new_library
(base / 'screen-manager-lint-review.json').write_text(json.dumps({
    'baseline': 'input-lint-song-linux-debug', 'final': lint_dir.name,
    'library_diagnostics_identical_to_baseline': True,
    'baseline_prelude': old_prelude.decode(), 'current_prelude': new_prelude.decode(),
    'log_sha256': lint['log_sha256']}, indent=2) + '\n')

delta = {'added': sorted(current.keys() - before.keys()), 'removed': sorted(before.keys() - current.keys()),
         'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
         'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys())}
assert not delta['removed']
assert delta['changed'] == ['crates/deadsync-song-lua/Cargo.toml', live_name,
                            'crates/deadsync-song-lua/src/rust_engine.rs',
                            'crates/deadsync-song-lua/src/rust_engine/managers.rs',
                            'crates/deadsync-song-lua/src/rust_engine/test_support.rs']
assert delta['added'] == ['crates/deadsync-song-lua/src/rust_engine/managers/screen.rs',
                          'crates/deadsync-song-lua/tests/rust_screen_manager.rs']
report = {'goal_complete': False, 'scope': 'process-owned SCREENMAN publication and screen request observation/consumption',
          'watched_sources': len(current), 'current_source_keys_and_hashes_match_test_and_lint': True,
          'rust_adapter_targets': targets, 'tests': tests, 'lint': lint,
          'lint_review': 'screen-manager-lint-review.json', 'format': 'screen-manager-format.json',
          'interpreter_and_root_entries_unchanged_since_949_test_run': len(core),
          'native_controls_unchanged': native, 'native_live_test_accessor': append_report,
          'fixtures': fixtures, 'source_delta': delta}
(base / 'screen-manager-final-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'sources': len(current), 'targets': len(targets), 'tests_passed': tests['tests_passed'],
                  'core_entries_unchanged': len(core), 'native_live_test_accessor': append_report, 'source_delta': delta}))
