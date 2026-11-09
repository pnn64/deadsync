"""Verify the statistics adapter stage against its exact validated source."""
from pathlib import Path
import hashlib
import json
import subprocess
import tomllib

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
tests_dir, lint_dir = base / 'statistics-adapters-song-linux-debug', base / 'statistics-lint-song-linux-debug'
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
previous = read(base / 'profiles-final-review.json')
before = read(base / 'profiles-adapters-song-linux-debug/sources.json')
core_run = read(base / 'actor-meshes-final-core-core-linux-debug/sources.json')
core = {k: v for k, v in core_run.items() if k.startswith('crates/deadlib-lua/')
        or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k, v in core.items())
native = [p for p in previous['native_controls_unchanged'] if p != 'crates/deadsync-song-lua/src/live.rs']
assert all(current[k] == before[k] for k in native)
control = read(base / 'statistics-native-control-review.json')
assert current[control['file']] == control['final_sha256']
original = read(base / 'statistics-edit-baseline.json')[control['file']]
assert hashlib.sha256(original['source'].encode()).hexdigest() == original['sha256']
assert (root / control['file']).read_bytes().startswith(original['source'].encode())
assert control['added_cfg_test_support_helpers'] == 4 and control['added_helpers_rustfmt_exit'] == 0
fixtures = {p: sha(root / p) for p in previous['fixtures']}
assert fixtures == previous['fixtures']
fixtures.update(read(base / 'statistics-fixture-control.json'))
assert all(sha(root / p) == v for p, v in fixtures.items())
formatting = read(base / 'statistics-format.json')
assert formatting['exit_code'] == 0
assert all(current[k] == v for k, v in formatting['files'].items())

old_log = (base / 'profiles-lint-song-linux-debug/run.log').read_bytes()
new_log = (lint_dir / 'run.log').read_bytes()
def library_diagnostics(data):
    lines = data.splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.startswith(b'    Checking deadsync-song-lua '))
    return b''.join(lines[start + 1:]), b''.join(lines[:start])
old_library, old_prelude = library_diagnostics(old_log)
new_library, new_prelude = library_diagnostics(new_log)
assert old_library == new_library
(base / 'statistics-lint-review.json').write_text(json.dumps({
    'complete_log_identical_to_baseline': old_log == new_log, 'library_diagnostics_identical_to_baseline': True, 'baseline_prelude': old_prelude.decode(), 'current_prelude': new_prelude.decode(), 'baseline': 'profiles-lint-song-linux-debug',
    'final': lint_dir.name, 'log_sha256': lint['log_sha256']}, indent=2) + '\n')
delta = {'added': sorted(current.keys() - before.keys()), 'removed': sorted(before.keys() - current.keys()),
         'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
         'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys())}
assert not delta['removed']
report = {'goal_complete': False, 'scope': 'STATSMAN/stage statistics adapter and explicit reset/feed/write phases only',
          'watched_sources': len(current), 'current_source_keys_and_hashes_match_test_and_lint': True,
          'rust_adapter_targets': targets, 'tests': tests, 'lint': lint,
          'lint_log_identical_to_baseline': old_log == new_log, 'lint_library_diagnostics_identical_to_baseline': True, 'format': 'statistics-format.json',
          'interpreter_and_root_entries_unchanged_since_949_test_run': len(core),
          'native_controls_unchanged': native, 'native_live_control': control, 'fixtures': fixtures, 'source_delta': delta}
(base / 'statistics-final-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'sources': len(current), 'targets': len(targets), 'tests_passed': tests['tests_passed'],
                  'core_entries_unchanged': len(core), 'source_delta': delta}))
