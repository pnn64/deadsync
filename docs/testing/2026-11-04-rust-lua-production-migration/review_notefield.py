"""Audit NoteField bindings against their exact validation sources."""
from pathlib import Path
import hashlib
import json
import subprocess
import tomllib

base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
tests_dir = base / 'notefield-adapters-song-linux-debug'
lint_dir = base / 'notefield-lint-song-linux-debug'
tests, lint = read(tests_dir / 'result.json'), read(lint_dir / 'result.json')
assert tests['exit_code'] == 0 and tests['tests_failed'] == 0
assert tests['sources_unchanged'] and lint['sources_unchanged']
assert lint['exit_code'] == 101
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
before = read(base / 'generated-players-adapters-song-linux-debug/sources.json')
delta = {'added': sorted(current.keys() - before.keys()),
         'removed': sorted(before.keys() - current.keys()),
         'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
         'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys())}
assert not delta['removed']
assert delta['added'] == ['crates/deadsync-song-lua/src/rust_engine/actors/notefields.rs',
                          'crates/deadsync-song-lua/src/rust_engine/actors/splines.rs',
                          'crates/deadsync-song-lua/tests/rust_notefields.rs']
assert delta['changed'] == ['crates/deadsync-song-lua/Cargo.toml',
                            'crates/deadsync-song-lua/src/compile.rs',
                            'crates/deadsync-song-lua/src/rust_engine/actors.lua',
                            'crates/deadsync-song-lua/src/rust_engine/actors.rs',
                            'crates/deadsync-song-lua/src/rust_engine/actors/generated_players.rs',
                            'crates/deadsync-song-lua/src/rust_engine/actors/lifecycle.rs',
                            'crates/deadsync-song-lua/src/rust_engine/test_support.rs']
targets = sorted(t['name'] for t in tomllib.loads(
    (root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
command = tests['command']
assert targets == sorted(command[i + 1] for i, arg in enumerate(command) if arg == '--test')
assert len(tests['test_results']) == len(targets)
core = {k: v for k, v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items()
        if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k, v in core.items())
previous = read(base / 'generated-players-final-review.json')
compile_name = 'crates/deadsync-song-lua/src/compile.rs'
native = [k for k in previous['native_controls_unchanged'] if k != compile_name]
native = sorted(set(native + ['crates/deadsync-song-lua/src/live.rs']))
assert all(current[k] == before[k] for k in native)
fixtures = {p: sha(root / p) for p in previous['fixtures']}
assert fixtures == previous['fixtures']
formatting = read(base / 'notefield-format.json')
assert formatting['exit_code'] == 0
assert all(current[k] == v for k, v in formatting['files'].items())
baseline = read(base / 'notefield-edit-baseline.json')
old_compile = baseline[compile_name]['source'].encode()
new_compile = (root / compile_name).read_bytes()
assert new_compile.startswith(old_compile)
assert hashlib.sha256(old_compile).hexdigest() == before[compile_name]
accessor = formatting['native_accessor']
assert new_compile[len(old_compile):].decode() == accessor['append']
assert sha(root / compile_name) == accessor['sha256']
assert subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout'],
                      input=accessor['append'].lstrip(), text=True, capture_output=True,
                      check=True).stdout == accessor['append'].lstrip()

def diagnostics(path):
    lines = path.read_bytes().splitlines(keepends=True)
    i = next(i for i, line in enumerate(lines) if line.startswith(b'    Checking deadsync-song-lua '))
    return b''.join(lines[i+1:])

assert diagnostics(base / 'generated-players-lint-song-linux-debug/run.log') == diagnostics(lint_dir / 'run.log')
report = {'goal_complete': False,
          'scope': 'NoteField/Board/column construction, cubic spline API, tap callbacks and column update phases',
          'watched_sources': len(current), 'current_source_keys_and_hashes_match_test_and_lint': True,
          'rust_adapter_targets': targets, 'tests_passed': tests['tests_passed'],
          'tests_failed': tests['tests_failed'], 'test_log_sha256': tests['log_sha256'],
          'lint_log_sha256': lint['log_sha256'], 'lint_library_diagnostics_unchanged': True,
          'core_and_root_entries_unchanged': len(core), 'native_controls_unchanged': native,
          'native_compile_prefix_and_test_accessor': {k: v for k, v in accessor.items() if k != 'append'},
          'fixtures': fixtures, 'delta': delta,
          'remaining': 'Complete screen tree/transitions, compiler capture/baking/live orchestration, '
                       'unconditional cutover and final application/dependency/fixture/performance gates'}
(base / 'notefield-final-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: v for k, v in report.items()
                  if k not in ['native_controls_unchanged', 'fixtures', 'rust_adapter_targets',
                               'native_compile_prefix_and_test_accessor']}))
