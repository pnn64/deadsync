"""Audit the existing ScreenGameplay base constructor migration, not cutover."""
from pathlib import Path
import hashlib, json, re, subprocess, tomllib

base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
stages = ['screen-assembly-focused-v8', 'screen-assembly-lint-v1', 'screen-assembly-adapters-v1']
directories = [base / (s + '-song-linux-debug') for s in stages]
focus, lint, tests = results = [read(d / 'result.json') for d in directories]
for d, result in zip(directories, results):
    assert result['sources_unchanged'] and sha(d / 'run.log') == result['log_sha256']
assert focus['exit_code'] == tests['exit_code'] == 0
assert focus['tests_passed'] == 61 and focus['tests_failed'] == 0
assert tests['tests_passed'] == 505 and tests['tests_failed'] == 0

crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert all(current == read(d / 'sources.json') for d in directories)
before = read(base / 'file-font-helpers-adapters-v1-song-linux-debug/sources.json')
previous = read(base / 'file-font-helpers-review.json')
delta = {
    'added': sorted(current.keys() - before.keys()),
    'removed': sorted(before.keys() - current.keys()),
    'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
    'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys()),
}
prefix = 'crates/deadsync-song-lua/'
assert delta['added'] == sorted(prefix + p for p in [
    'src/rust_engine/actors/screen_assembly.rs', 'src/rust_engine/actors/underlay.rs',
    *['src/rust_engine/actors/underlay/' + s + '.lua' for s in ['header', 'filter', 'difficulty', 'life', 'bpm']],
    'src/rust_engine/managers/gameplay_helpers.rs',
    'src/rust_engine/managers/is_routine.lua', 'src/rust_engine/managers/is_spooky.lua',
    'tests/rust_screen_assembly.rs',
])
assert delta['changed'] == sorted(prefix + p for p in [
    'Cargo.toml', 'src/compile.rs', 'src/rust_engine/actors.rs',
    'src/rust_engine/actors/generated_players.rs', 'src/rust_engine/actors/top_screen.rs',
    'src/rust_engine/managers.rs', 'src/rust_engine/test_support.rs',
])
assert not delta['removed']
baseline = read(base / 'screen-assembly-edit-baseline.json')
for name, record in baseline.items():
    assert hashlib.sha256(record['source'].encode()).hexdigest() == record['sha256']
    if name in before:
        assert record['sha256'] == before[name]
bridge_path = prefix + 'src/compile.rs'
native = (root / bridge_path).read_bytes().decode()
start = native.index('/// Actual production base constructor, excluding subsequent theme-file loads.')
end = native.index('/// Full production host, used as the screen-controls migration oracle.', start)
assert native[:start] + native[end:] == baseline[bridge_path]['source']
bridge = native[start:end]
assert '#[cfg(all(feature = "rust-engine", feature = "test-support"))]' in bridge
formatted_bridge = subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout'], input=bridge, capture_output=True, text=True, check=True).stdout
assert formatted_bridge.rstrip() == bridge.replace('\r\n', '\n').rstrip()

def targets(result):
    return sorted(result['command'][i+1] for i, arg in enumerate(result['command']) if arg == '--test')
expected = sorted(t['name'] for t in tomllib.loads((root / prefix / 'Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets(tests) == expected and len(expected) == len(tests['test_results']) == 49
assert targets(focus) == sorted(['rust_screen_assembly', 'rust_generated_players', 'rust_player_parts', 'rust_top_screen', 'rust_screen_parts', 'rust_transitions'])
assert focus['tests_failed'] == 0
assert all(current[k] == v for k, v in before.items() if k.startswith(prefix + 'tests/'))
fixture = (root / prefix / 'tests/rust_screen_assembly.rs').read_text()
assert len(re.findall(r'^#\[test\]', fixture, re.M)) == 8
assert '#[ignore' not in fixture
for source in ['src/rust_engine/actors/screen_assembly.rs', 'src/rust_engine/actors/underlay.rs']:
    assert 'eprintln!' not in (root / prefix / source).read_text()

native_source = (root / prefix / 'src/lua_util.rs').read_text()
verbatim = []
for name in ['header', 'filter', 'difficulty', 'life', 'bpm']:
    path = prefix + 'src/rust_engine/actors/underlay/' + name + '.lua'
    assert (root / path).read_text() in native_source
    verbatim.append(path)
compat = (root / prefix / 'src/compat.rs').read_text()
for name in ['is_routine', 'is_spooky']:
    path = prefix + 'src/rust_engine/managers/' + name + '.lua'
    assert (root / path).read_text() in compat
    verbatim.append(path)
core = {k:v for k,v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k,v in core.items())
native_controls = [k for k in previous['native_controls_unchanged'] if k != bridge_path]
assert all(current[k] == before[k] for k in native_controls)
assert {k:sha(root / k) for k in previous['fixtures']} == previous['fixtures']
formatted = sorted(k for k in delta['added'] + delta['changed'] if k.endswith('.rs') and k != bridge_path)
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true', *formatted], cwd=root, check=True, capture_output=True, text=True)

def diagnostics(path):
    return [d for d in re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text()) if d[2].startswith('crates/deadsync-song-lua/')]
assert lint['exit_code'] == 101
assert diagnostics(directories[1] / 'run.log') == diagnostics(base / 'file-font-helpers-lint-v1-song-linux-debug/run.log')
report = {
    'goal_complete': False,
    'verification': 'ScreenGameplay base constructor, native underlay snippets and ownership; later theme files, compiler/live orchestration and production cutover remain incomplete',
    'tests_passed': tests['tests_passed'], 'tests_failed': tests['tests_failed'],
    'focus_tests_passed': focus['tests_passed'], 'rust_adapter_targets': expected,
    'source_count': len(current), 'sources_match_focus_tests_and_lint': True, 'delta': delta,
    'log_sha256': {d.name:r['log_sha256'] for d,r in zip(directories, results)},
    'prior_adapter_test_sources_unchanged': True, 'screen_tests': 8,
    'verbatim_production_lua': verbatim,
    'core_and_root_entries_unchanged': len(core), 'native_controls_unchanged': native_controls,
    'native_test_bridge_only': bridge_path, 'fixtures': previous['fixtures'],
    'formatted_rust_files': formatted, 'native_bridge_formatted_in_isolation': True,
    'lint_diagnostic_messages_and_files_unchanged': True,
    'remaining': 'Original loaded combo/underlay/overlay/decorations/transitions; selection/reload; compiler/live output and fallback; unconditional cutover and final fixture/application/rendering/dependency/performance checks',
}
(base / 'screen-assembly-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['native_controls_unchanged', 'fixtures', 'rust_adapter_targets', 'formatted_rust_files']}))
