"""Review callback-table discovery and the existing static mod-table formats."""
from pathlib import Path
import difflib, hashlib, json, re, subprocess, sys

base = Path(__file__).resolve().parent
root = base.parents[2]
prefix = 'song-callback-tables'
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
reg, lint = sys.argv[1:]
objective_sha = sha(base/'objective.txt')
assert objective_sha == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
crates = ['deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
          'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield']
names = subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}',
    'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
    *[f'crates/{c}' for c in crates]], cwd=root).decode().split('\0')
paths = {root/n for n in names if n and Path(n).suffix in {'.rs', '.lua', '.toml'}}
paths.update(root/n for n in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
current = {p.relative_to(root).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}
deps = read(base/(prefix+'-test-dependencies.json'))
assert deps['exit_code'] == 0
assert sha(base/(prefix+'-test-dependencies.stdout')) == deps['stdout_sha256']
graph = subprocess.run(deps['command'], cwd=root, capture_output=True, check=True)
assert hashlib.sha256(graph.stdout).hexdigest() == deps['stdout_sha256']

def unused(path):
    p = Path(path).parts
    return len(p) > 2 and p[0] == 'crates' and p[1] not in deps['packages']

results = {s: read(base/s/'result.json') for s in [reg, lint]}
differences = {}
for stage, result in results.items():
    assert sha(base/stage/'run.log') == result['log_sha256']
    checked = read(base/stage/'sources.json')
    differences[stage] = sorted(k for k in current.keys() | checked.keys()
                               if current.get(k) != checked.get(k))
    assert all(unused(k) for k in differences[stage]), (stage, differences[stage])
assert results[reg]['exit_code'] == 0
assert results[reg]['tests_passed'] == 200 and results[reg]['tests_failed'] == 0
assert len(results[reg]['test_results']) == 1
snapshot = read(base/(prefix+'-edit-baseline.json'))
for path, value in snapshot.items():
    assert hashlib.sha256(value['text'].encode()).hexdigest() == value['sha256']
edited = sorted(snapshot)
assert all(current[p] != snapshot[p]['sha256'] for p in edited)
actor = 'crates/deadsync-song-lua/src/rust_engine/actors.rs'
assert (root/actor).read_text().replace('pub(super) mod compiler_tables;\n', '') == snapshot[actor]['text'].replace('\r\n', '\n')
reader = 'crates/deadsync-song-lua/src/rust_engine/song_easing_reader.rs'
assert (root/reader).read_text().replace('pub(super) fn song_easing_registry', 'fn song_easing_registry').replace('pub(super) fn song_easing_name', 'fn song_easing_name') == snapshot[reader]['text'].replace('\r\n', '\n')
added = ['crates/deadsync-song-lua/src/rust_engine/actors/compiler_tables.rs',
         'crates/deadsync-song-lua/src/rust_engine/song_callback_tables.rs',
         'crates/deadsync-song-lua/src/rust_engine/song_mod_tables.rs']
test = 'crates/deadsync-song-lua/tests/rust_song_activation.rs'
text = (root/test).read_text()
assert text.startswith(snapshot[test]['text'].replace('\r\n', '\n').rstrip()+'\n')
assert len(re.findall(r'^#\[test\]', text, re.M)) == 200 and '#[ignore' not in text
assert len(re.findall(r'^#\[test\]\nfn callback_tables_', text, re.M)) == 7
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config',
                'skip_children=true', *edited, *added], cwd=root, check=True, capture_output=True)

def diagnostics(path):
    return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):', path.read_text())

assert results[lint]['exit_code'] == 101
assert diagnostics(base/lint/'run.log') == diagnostics(base/'song-command-locals-lint-v1-song-linux-debug/run.log')
prior = read(base/(prefix+'-before-sources.json'))
changed = sorted(k for k in current.keys() & prior.keys() if current[k] != prior[k])
new = sorted(current.keys() - prior.keys())
removed = sorted(prior.keys() - current.keys())
assert set(edited) <= set(changed) and set(added) <= set(new)
other = sorted(k for k in changed+new+removed if k not in edited and k not in added)
shared = ['Cargo.toml', 'Cargo.lock']
for path in shared:
    assert current[path] == read(base/reg/'sources.json')[path] == read(base/lint/'sources.json')[path]
assert all(unused(k) or k in shared for k in other), other
assert sha(base/'checks.py') == read(base/'song-ease-probes-runner-change.json')['after_sha256']
diff = ''.join(''.join(difflib.unified_diff(
    snapshot[p]['text'].replace('\r\n', '\n').splitlines(keepends=True),
    (root/p).read_text().splitlines(keepends=True), fromfile='before/'+p, tofile=p)) for p in edited)
diff += ''.join(''.join(difflib.unified_diff([], (root/p).read_text().splitlines(keepends=True),
    fromfile='/dev/null', tofile=p)) for p in added)
diff_path = base/(prefix+'-source-diff.diff')
diff_path.write_text(diff)
report = dict(goal_complete=False, production_cutover=False, final_application_validation=False,
    acceptance_scope='objective.txt', objective_sha256=objective_sha, source_count=len(current),
    adapter_changed=edited, added=added,
    native_production_source_engine_and_manifests_unchanged_by_adapter=True,
    existing_actor_dispatch_unchanged=True, existing_easing_reader_only_shares_private_helpers=True,
    other_source_changes_preserved=other,
    concurrent_workspace_manifests_match_both_final_checks=True,
    watched_dependency_sources_match_checks=True,
    out_of_dependency_graph_changes_since_checks=differences,
    earlier_193_activation_tests_unchanged=True, new_cases=7,
    adapter_formatting_passed=True, song_clippy_baseline_unchanged=True,
    runner_unchanged_from_ease_probe_checkpoint=True, source_diff_sha256=sha(diff_path),
    results={s: {k: r[k] for k in ['exit_code', 'seconds', 'log_sha256', 'tests_passed',
              'tests_failed', 'sources_unchanged']} for s, r in results.items()})
(base/(prefix+'-reviewed-sources.json')).write_text(json.dumps(current, indent=2)+'\n')
(base/(prefix+'-review.json')).write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report))
