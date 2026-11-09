"""Verify the existing file/font bindings and restored complete-player fixture."""
from pathlib import Path
import hashlib, json, re, subprocess, tomllib

base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
stages = ['file-font-helpers-focused-v2', 'file-font-helpers-lint-v1', 'file-font-helpers-adapters-v1']
directories = [base / (s + '-song-linux-debug') for s in stages]
focus, lint, tests = results = [read(d / 'result.json') for d in directories]
for d, result in zip(directories, results):
    assert result['sources_unchanged'] and sha(d / 'run.log') == result['log_sha256']
assert focus['exit_code'] == tests['exit_code'] == 0
assert (focus['tests_passed'], focus['tests_failed']) == (23, 0)
assert (tests['tests_passed'], tests['tests_failed']) == (497, 0)

crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert all(current == read(d / 'sources.json') for d in directories)
before = read(base / 'top-screen-controls-adapters-song-linux-debug/sources.json')
previous = read(base / 'top-screen-controls-review.json')
delta = {
    'added': sorted(current.keys() - before.keys()),
    'removed': sorted(before.keys() - current.keys()),
    'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
    'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys()),
}
assert delta['added'] == [
    'crates/deadsync-song-lua/src/rust_engine/managers/files.rs',
    'crates/deadsync-song-lua/src/rust_engine/managers/get_combo_fonts.lua',
    'crates/deadsync-song-lua/tests/rust_files.rs',
]
assert delta['changed'] == sorted([
    'crates/deadsync-song-lua/Cargo.toml',
    'crates/deadsync-song-lua/src/rust_engine/managers.rs',
    'crates/deadsync-song-lua/src/rust_engine/test_support.rs',
    'crates/deadsync-song-lua/tests/rust_top_screen.rs',
])
assert not delta['removed']

def targets(result):
    return sorted(result['command'][i+1] for i, arg in enumerate(result['command']) if arg == '--test')
expected = sorted(t['name'] for t in tomllib.loads((root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets(tests) == expected and len(expected) == len(tests['test_results']) == 48
assert targets(focus) == ['rust_files', 'rust_player_parts', 'rust_top_screen']
baseline = read(base / 'file-font-helpers-edit-baseline.json')
for name, record in baseline.items():
    assert hashlib.sha256(record['source'].encode()).hexdigest() == record['sha256']
    if name in before:
        assert record['sha256'] == before[name]
fixture = 'crates/deadsync-song-lua/src/rust_engine/test_support.rs'
old = baseline[fixture]['source']
start = old.index('pub fn top_screen_controls(')
end = old.index('\npub fn screen_part(', start)
restored = old[:start] + old[start:end].replace('gameplay_player_base(runtime, context, index)?', 'gameplay_player(runtime, context, index)?') + old[end:]
assert (root / fixture).read_bytes().decode() == restored

checkpoint = read(base / 'file-font-helpers-v1-checkpoint.json')
top_tests = 'crates/deadsync-song-lua/tests/rust_top_screen.rs'
top_prefix = (root / top_tests).read_text().split('#[test]\nfn default_screen_fixture_constructs_complete_players')[0].rstrip()
assert top_prefix == checkpoint[top_tests]['source'].replace('\r\n', '\n').rstrip()
file_tests = 'crates/deadsync-song-lua/tests/rust_files.rs'
split = '#[test]\nfn combo_font_helper_resolves_current_lua_callbacks_during_collection_and_reentry'
assert (root / file_tests).read_text().split(split)[0] == checkpoint[file_tests]['source'].replace('\r\n', '\n').split(split)[0]

legacy = (root / 'crates/deadsync-song-lua/src/compat.rs').read_text()
start = legacy.index('lua.load(r#"local FilenameIsMultiFrameSprite') + len('lua.load(r#"')
end = legacy.index('"#).set_name("Simply Love/Scripts/SL-Helpers.lua:GetComboFonts")', start)
assert (root / 'crates/deadsync-song-lua/src/rust_engine/managers/get_combo_fonts.lua').read_text() == legacy[start:end]
core = {k:v for k,v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k,v in core.items())
native = sorted(set(previous['native_controls_unchanged'] + [previous['native_test_bridge_only']]))
assert all(current[k] == before[k] for k in native)
assert {k:sha(root / k) for k in previous['fixtures']} == previous['fixtures']
formatted = sorted(k for k in delta['added'] + delta['changed'] if k.endswith('.rs'))
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true', *formatted], cwd=root, check=True, capture_output=True, text=True)

def diagnostics(path):
    return [d for d in re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text()) if d[2].startswith('crates/deadsync-song-lua/')]
assert lint['exit_code'] == 101
assert diagnostics(directories[1] / 'run.log') == diagnostics(base / 'top-screen-controls-lint-v1-song-linux-debug/run.log')
report = {
    'goal_complete': False,
    'verification': 'Existing FILEMAN and font helpers; full player fixture restored; complete screen assembly and production cutover remain pending',
    'tests_passed': tests['tests_passed'], 'tests_failed': tests['tests_failed'],
    'focus_tests_passed': focus['tests_passed'], 'rust_adapter_targets': expected,
    'source_count': len(current), 'sources_match_focus_tests_and_lint': True, 'delta': delta,
    'log_sha256': {d.name:r['log_sha256'] for d,r in zip(directories, results)},
    'complete_player_fixture_restored': True, 'previous_nine_screen_tests_unchanged': True,
    'font_scanner_source_matches_production': True, 'first_five_file_tests_unchanged_since_v1': True,
    'core_and_root_entries_unchanged': len(core), 'native_controls_unchanged': native,
    'fixtures': previous['fixtures'], 'formatted_rust_files': formatted,
    'lint_diagnostic_messages_and_files_unchanged': True,
    'remaining': previous['remaining'],
}
(base / 'file-font-helpers-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['native_controls_unchanged', 'fixtures', 'rust_adapter_targets', 'formatted_rust_files']}))
