"""Verify production preparation loaders without declaring production cutover."""
from pathlib import Path
import argparse, hashlib, json, re, subprocess, tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--focus', required=True)
p.add_argument('--lint', required=True)
p.add_argument('--tests', required=True)
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
assert (focus['tests_passed'], focus['tests_failed']) == (69, 0)
assert (tests['tests_passed'], tests['tests_failed']) == (481, 0)
crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root / n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
assert all(current == read(d / 'sources.json') for d in directories)
before = read(base / 'offline-constructor-adapters-song-linux-debug/sources.json')
previous = read(base / 'offline-constructor-review.json')
delta = {
    'added': sorted(current.keys() - before.keys()),
    'removed': sorted(before.keys() - current.keys()),
    'changed': sorted(k for k in current.keys() & before.keys() if current[k] != before[k]),
    'unchanged': sum(current[k] == before[k] for k in current.keys() & before.keys()),
}
assert delta['added'] == ['crates/deadsync-song-lua/src/rust_engine/actors/loading.rs']
assert delta['changed'] == sorted([
    'crates/deadsync-song-lua/src/rust_engine.rs',
    'crates/deadsync-song-lua/src/rust_engine/definitions.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors.lua',
    'crates/deadsync-song-lua/src/rust_engine/actors/offline.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors/text.rs',
    'crates/deadsync-song-lua/src/rust_engine/test_support.rs',
    'crates/deadsync-song-lua/tests/rust_preparation.rs',
])
assert not delta['removed']
baseline = read(base / 'preparation-loader-edit-baseline.json')
assert all(k in baseline for k in delta['changed'])
for k, v in baseline.items():
    assert hashlib.sha256(v['source'].encode()).hexdigest() == v['sha256']
    if k in before:
        assert v['sha256'] == before[k]
targets = sorted(t['name'] for t in tomllib.loads((root / 'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets == sorted(tests['command'][i+1] for i, a in enumerate(tests['command']) if a == '--test')
assert len(targets) == len(tests['test_results']) == 46
core = {k:v for k,v in read(base / 'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(current[k] == v for k,v in core.items())
native=sorted(previous['native_controls_unchanged']+[previous['native_test_bridge_only']])
assert all(current[k] == before[k] for k in native)
# Preserve all three original failing regressions and their existing surrounding
# controls. Later tests only append new cases to that exact source prefix.
failed_sources=read(base / 'preparation-loader-baseline-song-linux-debug/sources.json')
failed=read(base / 'preparation-loader-baseline-song-linux-debug/result.json')
assert (failed['tests_passed'],failed['tests_failed'],failed['exit_code'])==(0,3,101)
test_file='crates/deadsync-song-lua/tests/rust_preparation.rs'
prefix=(root / test_file).read_text().split('#[test]\nfn preparation_loader_saved_callbacks_')[0].rstrip()+'\n'
assert hashlib.sha256(prefix.encode()).hexdigest()==failed_sources[test_file]
assert sorted(focus['command'][i+1] for i,a in enumerate(focus['command']) if a=='--test') == sorted([
    'rust_preparation','rust_loader','rust_definitions','rust_bitmap_text','rust_sprites','rust_noteskin'])
assert {k:sha(root / k) for k in previous['fixtures']} == previous['fixtures']
changed_rs = sorted(k for k in delta['added'] + delta['changed'] if k.endswith('.rs'))
command = ['rustfmt', '--check', '--edition', '2024', '--config', 'skip_children=true', *changed_rs]
formatting = subprocess.run(command, cwd=root, capture_output=True, text=True)
assert formatting.returncode == 0, formatting.stderr + formatting.stdout
def diagnostics(path):
    return [d for d in re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text()) if d[2].startswith('crates/deadsync-song-lua/')]
old_diagnostics = diagnostics(base / 'offline-constructor-lint-song-linux-debug/run.log')
new_diagnostics = diagnostics(directories[1] / 'run.log')
assert lint['exit_code'] == 101
assert new_diagnostics == old_diagnostics, [d for d in new_diagnostics if d not in old_diagnostics]
report = {
    'goal_complete': False,
    'verification': 'Production preparation file/font/media loading, caller scope, ownership, error/limit cleanup and entry-API behavior verified; production migration incomplete',
    'tests_passed': tests['tests_passed'], 'tests_failed': tests['tests_failed'],
    'focus_tests_passed': focus['tests_passed'], 'rust_adapter_targets': targets,
    'source_count': len(current), 'sources_match_focus_tests_and_lint': True, 'delta': delta,
    'log_sha256': {d.name:r['log_sha256'] for d,r in zip(directories, [focus, lint, tests])},
    'core_and_root_entries_unchanged': len(core),
    'native_controls_unchanged': native, 'original_failing_coverage_unchanged': True, 'fixtures': previous['fixtures'],
    'format_command': command, 'formatted_rust_files': len(changed_rs),
    'lint_diagnostic_messages_and_files_unchanged': True,
    'remaining': 'Full compiler startup orchestration/state rewind, frame scheduling, remaining capture channels and baking/live integration; remaining screen trees and transition roles; unconditional production cutover; final fixture, application/rendering, dependency and performance checks.',
}
(base / 'preparation-loader-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['native_controls_unchanged', 'fixtures', 'rust_adapter_targets', 'format_command']}))
