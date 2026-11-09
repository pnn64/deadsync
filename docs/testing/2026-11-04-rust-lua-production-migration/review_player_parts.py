"""Audit player Init assembly against the exact regression and lint sources."""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import tomllib

base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()

tests_dir = base / 'player-parts-adapters-song-linux-debug'
lint_dir = base / 'player-parts-lint-song-linux-debug'
tests, lint = read(tests_dir / 'result.json'), read(lint_dir / 'result.json')
assert tests['exit_code'] == 0 and tests['tests_failed'] == 0
assert not tests['sources_unchanged'] and lint['sources_unchanged']
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
tested = read(tests_dir / 'sources.json')
assert tested == read(lint_dir / 'sources.json')
assert current.keys() == tested.keys()
changed_during_tests = read(base / 'player-parts-concurrent-adapters-delta.json')
assert [e['path'] for e in changed_during_tests] == [
    'crates/deadsync-theme-simply-love/src/screens/select_music.rs']
assert {k for k in current if current[k] != tested[k]} == {e['path'] for e in changed_during_tests}
assert all(current[e['path']] == e['after'] and tested[e['path']] == e['before']
           for e in changed_during_tests)
dependencies = read(base / 'player-parts-adapter-dependencies.json')
assert dependencies['exit_code'] == 0
assert dependencies['deadsync_theme_simply_love_in_test_dependency_graph'] is False
assert sha(base / 'player-parts-adapter-dependencies.txt') == dependencies['log_sha256']
assert 'deadsync-theme-simply-love ' not in (base / 'player-parts-adapter-dependencies.txt').read_text()
before = read(base / 'notefield-adapters-song-linux-debug/sources.json')
delta = {'added': sorted(current.keys() - before.keys()),
         'removed': sorted(before.keys() - current.keys()),
         'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
         'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys())}
assert not delta['removed']
external_added = read(base / 'player-parts-concurrent-added.json')
assert len(external_added) == 21 and all(k.startswith('crates/deadsync-notefield/') for k in external_added)
assert all(current[k] == tested[k] == v for k, v in external_added.items())
assert delta['added'] == sorted([*external_added,
    'crates/deadsync-song-lua/src/rust_engine/actors/player_parts.rs',
    'crates/deadsync-song-lua/tests/rust_player_parts.rs'])
owned = ['crates/deadsync-song-lua/Cargo.toml',
         'crates/deadsync-song-lua/src/compile.rs',
         'crates/deadsync-song-lua/src/lua_util.rs',
         'crates/deadsync-song-lua/src/rust_engine/actors.lua',
         'crates/deadsync-song-lua/src/rust_engine/actors.rs',
         'crates/deadsync-song-lua/src/rust_engine/actors/generated_players.rs',
         'crates/deadsync-song-lua/src/rust_engine/actors/notefields.rs',
         'crates/deadsync-song-lua/src/rust_engine/test_support.rs']
external = (read(base / 'player-parts-concurrent-delta.json')
            + read(base / 'player-parts-concurrent-v2-delta.json'))
assert sorted(owned + [e['path'] for e in external]) == delta['changed']
for e in changed_during_tests:
    previous_edit = next(old for old in external if old['path'] == e['path'])
    assert previous_edit['after'] == e['before']
    previous_edit['after'] = e['after']
assert all(current[e['path']] == e['after'] for e in external)

targets = sorted(t['name'] for t in tomllib.loads(
    (root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
command = tests['command']
assert targets == sorted(command[i + 1] for i, arg in enumerate(command) if arg == '--test')
assert len(tests['test_results']) == len(targets)
core = {k: v for k, v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items()
        if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k, v in core.items())
previous = read(base / 'notefield-final-review.json')
native = [k for k in previous['native_controls_unchanged'] if k != 'crates/deadsync-song-lua/src/lua_util.rs']
assert all(current[k] == before[k] for k in native)
fixtures = {p: sha(root / p) for p in previous['fixtures']}
assert fixtures == previous['fixtures']
formatting = read(base / 'player-parts-format.json')
assert formatting['exit_code'] == 0
assert all(current[k] == v for k, v in formatting['files'].items())
baseline = read(base / 'player-parts-edit-baseline.json')
accessors = {}
for name in ['crates/deadsync-song-lua/src/compile.rs', 'crates/deadsync-song-lua/src/lua_util.rs']:
    old = baseline[name]['source'].encode()
    new = (root / name).read_bytes()
    assert new.startswith(old) and hashlib.sha256(old).hexdigest() == before[name]
    append = new[len(old):].decode()
    assert append.startswith('\n#[cfg(all(feature = "rust-engine", feature = "test-support"))]')
    assert subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout'],
                          input=append.lstrip(), text=True, capture_output=True,
                          check=True).stdout == append.lstrip()
    accessors[name] = {'unchanged_prefix_bytes': len(old), 'appended_bytes': len(new)-len(old),
                      'append_sha256': hashlib.sha256(append.encode()).hexdigest(), 'sha256': sha(root / name)}

def diagnostics(path):
    data = path.read_bytes()
    match = re.search(rb'(?m)^(?:warning|error):[^\n]*\n[ \t]+--> crates/deadsync-song-lua/', data)
    assert match
    return data[match.start():]

assert lint['exit_code'] == 101
assert diagnostics(base / 'notefield-lint-song-linux-debug/run.log') == diagnostics(lint_dir / 'run.log')
report = {'goal_complete': False,
          'scope': 'Player Init assembly: Combo, Judgment, per-column HoldJudgments and NoteField',
          'watched_sources': len(current), 'current_source_keys_and_hashes_match_test_and_lint': False,
          'current_adapter_sources_match_test_and_lint': True,
          'broad_test_wrapper_source_guard_passed': False,
          'changed_non_dependency_during_tests': changed_during_tests,
          'adapter_dependency_graph': dependencies,
          'rust_adapter_targets': targets, 'tests_passed': tests['tests_passed'],
          'tests_failed': tests['tests_failed'], 'test_log_sha256': tests['log_sha256'],
          'lint_log_sha256': lint['log_sha256'], 'lint_library_diagnostics_unchanged': True,
          'core_and_root_entries_unchanged': len(core), 'native_controls_unchanged': native,
          'native_prefixes_and_test_accessors': accessors, 'fixtures': fixtures, 'delta': delta,
          'preserved_concurrent_changes': external,
          'preserved_concurrent_additions': external_added,
          'remaining': 'Screen tree and later theme commands/milestones/transitions, compiler capture/baking/live orchestration, unconditional cutover and final application/dependency/fixture/performance gates'}
(base / 'player-parts-final-review.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['rust_adapter_targets','native_controls_unchanged','fixtures','preserved_concurrent_changes']}))
