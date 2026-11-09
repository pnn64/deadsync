"""Verify retained selection and screen reconstruction bindings, not production cutover."""
from pathlib import Path
import argparse, hashlib, json, re, subprocess, tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--full', required=True)
p.add_argument('--lint', required=True)
args = p.parse_args()
base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
stages = [args.full, args.lint]
directories = [base / (s + '-song-linux-debug') for s in stages]
full, lint = results = [read(d / 'result.json') for d in directories]
for d, result in zip(directories, results):
    assert result['sources_unchanged'] and sha(d / 'run.log') == result['log_sha256']
assert full['exit_code'] == 0
assert full['tests_failed'] == 0

crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert all(current == read(d / 'sources.json') for d in directories)
before = read(base / 'theme-overlay-adapters-v1-song-linux-debug/sources.json')
previous = read(base / 'theme-overlay-review.json')
delta = {
    'added': sorted(current.keys() - before.keys()),
    'removed': sorted(before.keys() - current.keys()),
    'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
    'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys()),
}
prefix = 'crates/deadsync-song-lua/'
assert delta['added'] == sorted(prefix + p for p in [
    'src/rust_engine/gameplay.rs', 'src/rust_engine/managers/game_state/scene.rs',
    'tests/rust_gameplay_reload.rs',
])
assert delta['changed'] == sorted(prefix + p for p in [
    'Cargo.toml', 'src/compile.rs', 'src/rust_engine.rs', 'src/rust_engine/actors.rs',
    'src/rust_engine/managers.rs', 'src/rust_engine/managers/game_state.rs',
    'src/rust_engine/managers/game_state/selection.rs', 'src/rust_engine/test_support.rs',
])
assert not delta['removed']
baseline = read(base / 'gameplay-reload-edit-baseline.json')
for name, record in baseline.items():
    assert hashlib.sha256(record['text'].encode()).hexdigest() == record['sha256']
    if name in before:
        assert record['sha256'] == before[name]
bridge_path = prefix + 'src/compile.rs'
native = (root / bridge_path).read_bytes().decode()
start = native.index('/// Actual retained-VM screen replacement, used by migration comparisons.')
end = native.index('/// Full production host, used as the screen-controls migration oracle.', start)
assert native[:start] + native[end:] == baseline[bridge_path]['text']
bridge = native[start:end]
assert bridge.count('#[cfg(all(feature = "rust-engine", feature = "test-support"))]') == 1
formatted_bridge = subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout'], input=bridge, capture_output=True, text=True, check=True).stdout
assert formatted_bridge.rstrip() == bridge.replace('\r\n', '\n').rstrip()

def targets(result):
    return sorted(result['command'][i + 1] for i, arg in enumerate(result['command']) if arg == '--test')

expected = sorted(t['name'] for t in tomllib.loads((root / prefix / 'Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets(full) == expected and len(expected) == len(full['test_results']) == 53
theme_log = (directories[0] / 'run.log').read_text().split('Running tests/rust_gameplay_reload.rs', 1)[1].split('Running tests/', 1)[0]
theme_result = re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', theme_log)
assert theme_result
focus_passed = int(theme_result.group(1))
fixture = (root / prefix / 'tests/rust_gameplay_reload.rs').read_text()
cases = len(re.findall(r'^#\[test\]', fixture, re.M))
assert '#[ignore' not in fixture
assert focus_passed == cases and full['tests_passed'] == previous['tests_passed'] + cases
assert all(current[k] == v for k, v in before.items() if k.startswith(prefix + 'tests/'))
for source in ['src/rust_engine/gameplay.rs', 'src/rust_engine/managers/game_state/scene.rs']:
    assert 'eprintln!' not in (root / prefix / source).read_text()
core = {k:v for k,v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k,v in core.items())
native_controls = previous['native_controls_unchanged']
assert all(current[k] == before[k] for k in native_controls)
assert {k:sha(root / k) for k in previous['fixtures']} == previous['fixtures']
formatted = sorted(k for k in delta['added'] + delta['changed'] if k.endswith('.rs') and k != bridge_path)
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true', *formatted], cwd=root, check=True, capture_output=True, text=True)

def diagnostics(path):
    return [d for d in re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text()) if d[2].startswith('crates/deadsync-song-lua/')]

assert lint['exit_code'] == 101
assert diagnostics(directories[1] / 'run.log') == diagnostics(base / 'theme-overlay-lint-v1-song-linux-debug/run.log')
report = {
    'goal_complete': False,
    'verification': 'Retained chart selection and initial/replacement screen phase ordering; complete compiler/session integration remains incomplete',
    'tests_passed': full['tests_passed'], 'tests_failed': full['tests_failed'], 'focus_tests_passed': focus_passed, 'focus_verification': 'rust_gameplay_reload block in the full regression log',
    'rust_adapter_targets': expected, 'source_count': len(current), 'sources_match_full_tests_and_lint': True,
    'delta': delta, 'log_sha256': {d.name:r['log_sha256'] for d,r in zip(directories, results)},
    'prior_adapter_test_sources_unchanged': True, 'new_gameplay_tests': cases,
    'core_and_root_entries_unchanged': len(core), 'native_controls_unchanged': native_controls,
    'native_test_bridge_only': bridge_path, 'fixtures': previous['fixtures'],
    'formatted_rust_files': formatted, 'native_bridge_formatted_in_isolation': True,
    'lint_diagnostic_messages_and_files_unchanged': True,
    'remaining': 'Complete compiler/session startup, reload/end ownership and output; fallback/cache; unconditional cutover and final fixture/application/rendering/dependency/performance checks',
}
(base / 'gameplay-reload-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['native_controls_unchanged', 'fixtures', 'rust_adapter_targets', 'formatted_rust_files']}))
