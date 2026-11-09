"""Verify the screen-control stage without claiming complete screen assembly."""
from pathlib import Path
import argparse, hashlib, json, re, subprocess, tempfile, tomllib

p = argparse.ArgumentParser(description=__doc__)
for name in ['focus', 'lint', 'tests']:
    p.add_argument('--' + name, required=True)
args = p.parse_args()
base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda path: json.loads(path.read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
directories = [base / (name + '-song-linux-debug') for name in [args.focus, args.lint, args.tests]]
focus, lint, tests = [read(d / 'result.json') for d in directories]
for directory, result in zip(directories, [focus, lint, tests]):
    assert result['sources_unchanged']
    assert sha(directory / 'run.log') == result['log_sha256']
assert focus['exit_code'] == tests['exit_code'] == 0
assert (tests['tests_passed'], tests['tests_failed']) == (490, 0)
crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert all(current == read(d / 'sources.json') for d in directories)
before = read(base / 'preparation-loader-adapters-v2-song-linux-debug/sources.json')
previous = read(base / 'preparation-loader-review.json')
delta = {
    'added': sorted(current.keys() - before.keys()),
    'removed': sorted(before.keys() - current.keys()),
    'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
    'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys()),
}
assert delta['added'] == [
    'crates/deadsync-song-lua/src/rust_engine/actors/top_screen.rs',
    'crates/deadsync-song-lua/tests/rust_top_screen.rs',
]
assert delta['changed'] == sorted([
    'crates/deadsync-song-lua/Cargo.toml',
    'crates/deadsync-song-lua/src/compile.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors.lua',
    'crates/deadsync-song-lua/src/rust_engine/managers/screen.rs',
    'crates/deadsync-song-lua/src/rust_engine/test_support.rs',
])
assert not delta['removed']
baseline = read(base / 'top-screen-controls-edit-baseline.json')
for k, v in baseline.items():
    assert hashlib.sha256(v['source'].encode()).hexdigest() == v['sha256']
    if k in before:
        assert v['sha256'] == before[k]
targets = sorted(t['name'] for t in tomllib.loads((root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
def test_targets(result):
    return [result['command'][i+1] for i, a in enumerate(result['command']) if a == '--test']
assert targets == sorted(test_targets(tests))
assert len(targets) == len(tests['test_results']) == 47
assert sorted(test_targets(focus)) == ['rust_generated_players', 'rust_screen_manager', 'rust_top_screen', 'rust_transitions']
assert len(focus['test_results']) == 4 and focus['tests_failed'] == 0
old_tests = read(base / 'preparation-loader-adapters-v2-song-linux-debug/result.json')
counts = dict(zip(test_targets(old_tests), [int(re.search(r'(\d+) passed', r).group(1)) for r in old_tests['test_results']]))
assert focus['tests_passed'] == 9 + sum(counts[t] for t in test_targets(focus) if t != 'rust_top_screen')
core = {k:v for k,v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k,v in core.items())
bridge_path = 'crates/deadsync-song-lua/src/compile.rs'
native = sorted(k for k in previous['native_controls_unchanged'] if k != bridge_path)
assert all(current[k] == before[k] for k in native)
source = (root / bridge_path).read_bytes().decode()
start = source.index('/// Full production host, used as the screen-controls migration oracle.')
end = source.index('\npub fn compile_song_lua_with_actors', start)
bridge = source[start:end]
assert source[:start] + source[end+1:] == baseline[bridge_path]['source']
assert '#[cfg(all(feature = "rust-engine", feature = "test-support"))]' in bridge
assert {k:sha(root / k) for k in previous['fixtures']} == previous['fixtures']
v3 = read(base / 'top-screen-controls-focused-v3-song-linux-debug/result.json')
assert (v3['tests_passed'], v3['tests_failed']) == (7, 1)
test_file = 'crates/deadsync-song-lua/tests/rust_top_screen.rs'
prefix = (root / test_file).read_text().split('#[test]\nfn preparation_row_capture_')[0].rstrip() + '\n'
assert hashlib.sha256(prefix.encode()).hexdigest() == read(base / 'top-screen-controls-focused-v3-song-linux-debug/sources.json')[test_file]
changed_rs = sorted(k for k in delta['added'] + delta['changed'] if k.endswith('.rs') and k != bridge_path)
format_command = ['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true', *changed_rs]
subprocess.run(format_command, cwd=root, check=True, capture_output=True, text=True)
with tempfile.TemporaryDirectory(prefix='deadsync-screen-bridge-') as temp:
    path = Path(temp) / 'bridge.rs'
    path.write_text(bridge.rstrip() + '\n')
    subprocess.run(['rustfmt', '--check', '--edition', '2024', str(path)], check=True, capture_output=True, text=True)
def diagnostics(path):
    return [d for d in re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text()) if d[2].startswith('crates/deadsync-song-lua/')]
assert lint['exit_code'] == 101
assert diagnostics(directories[1] / 'run.log') == diagnostics(base / 'preparation-loader-lint-v2-song-linux-debug/run.log')
report = {
    'goal_complete': False,
    'verification': 'TopScreen controls, OptionRow construction/capture, MusicWheel backing, transitions and retained ownership; full screen assembly and production migration remain incomplete',
    'tests_passed': tests['tests_passed'], 'tests_failed': tests['tests_failed'],
    'focus_tests_passed': focus['tests_passed'], 'rust_adapter_targets': targets,
    'source_count': len(current), 'sources_match_focus_tests_and_lint': True, 'delta': delta,
    'log_sha256': {d.name:r['log_sha256'] for d,r in zip(directories, [focus, lint, tests])},
    'core_and_root_entries_unchanged': len(core), 'native_controls_unchanged': native,
    'native_test_bridge_only': bridge_path, 'original_failing_coverage_unchanged_since_v3': True,
    'fixtures': previous['fixtures'], 'format_command': format_command,
    'formatted_rust_files': len(changed_rs), 'isolated_bridge_formatting': True,
    'lint_diagnostic_messages_and_files_unchanged': True,
    'remaining': 'Complete gameplay screen/theme assembly and pending host helpers; compiler startup/state rewind/frame scheduling/capture/baking/live integration; unconditional cutover; final fixture, application/rendering, dependency and performance checks.',
}
(base / 'top-screen-controls-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['native_controls_unchanged', 'fixtures', 'rust_adapter_targets', 'format_command']}))
