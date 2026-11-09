"""Audit the song NOTESKIN adapter stage, not completion of the full migration."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tomllib

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
read = lambda p: json.loads(p.read_text())
tests_dir = base / 'noteskin-manager-final-adapters-song-linux-debug'
lint_dir = base / 'noteskin-manager-final-lint-song-linux-debug'
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

targets = sorted(t['name'] for t in tomllib.loads(
    (root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
command = tests['command']
assert targets == sorted(command[i + 1] for i, item in enumerate(command) if item == '--test')
assert len(tests['test_results']) == len(targets)

previous = read(base / 'gamestate-final-review.json')
before = read(base / 'gamestate-adapters-song-linux-debug/sources.json')
core_run = read(base / 'actor-meshes-final-core-core-linux-debug/sources.json')
core = {k: v for k, v in core_run.items() if k.startswith('crates/deadlib-lua/')
        or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k, v in core.items())
baseline = read(base / 'noteskin-manager-edit-baseline.json')
native = previous['native_controls_unchanged'] + [
    'crates/deadsync-song-lua/src/noteskin.rs', 'crates/deadsync-song-lua/src/compat.rs',
    'crates/deadsync-song-lua/src/lua_util.rs']
assert all(current[k] == before[k] for k in native)
compiler = 'crates/deadsync-song-lua/src/compile.rs'
original = baseline[compiler]['source'].encode()
raw = (root / compiler).read_bytes()
assert raw.startswith(original)
suffix = raw[len(original):].decode().strip()
assert suffix.startswith('#[cfg(all(feature = "rust-engine", feature = "test-support"))]\npub(crate) fn migration_install_noteskin')
assert suffix.count('fn ') == 1

fixtures = {p: sha(root / p) for p in previous['fixtures']}
assert fixtures == previous['fixtures']
formatting = read(base / 'noteskin-manager-final-format.json')
assert formatting['exit_code'] == 0 and formatting['new_native_control_format_matches']
assert all(current[k] == v for k, v in formatting['files'].items())

old_log = (base / 'gamestate-lint-song-linux-debug/run.log').read_text()
new_log = (lint_dir / 'run.log').read_text()
diagnostics = lambda s: s[s.index('    Checking deadsync-song-lua'):]
diff = list(difflib.unified_diff(diagnostics(old_log).splitlines(), diagnostics(new_log).splitlines()))
assert not diff, diff
lint_review = {'diagnostics_identical_after_build_preamble': True, 'diagnostic_diff': diff,
               'baseline': 'gamestate-lint-song-linux-debug', 'final': lint_dir.name}
(base / 'noteskin-manager-final-lint-review.json').write_text(json.dumps(lint_review, indent=2) + '\n')

delta = {'added': sorted(current.keys() - before.keys()), 'removed': sorted(before.keys() - current.keys()),
         'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
         'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys())}
report = {'goal_complete': False, 'scope': 'song NOTESKIN adapter stage only',
          'watched_sources': len(current), 'current_source_keys_and_hashes_match_test_and_lint': True,
          'rust_adapter_targets': targets, 'tests': tests, 'lint': lint,
          'lint_diagnostics_identical_to_baseline': True, 'format': 'noteskin-manager-final-format.json',
          'interpreter_and_root_entries_unchanged_since_949_test_run': len(core),
          'native_controls_unchanged': native, 'native_compiler_original_bytes_preserved': True,
          'native_compiler_addition': suffix, 'fixtures': fixtures, 'source_delta': delta}
(base / 'noteskin-manager-final-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'sources': len(current), 'targets': len(targets), 'tests_passed': tests['tests_passed'],
                  'core_entries_unchanged': len(core), 'source_delta': delta}))
