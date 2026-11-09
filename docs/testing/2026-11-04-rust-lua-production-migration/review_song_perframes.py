"""Review the standalone sampled-perframe compiler port against its phase baseline."""
from pathlib import Path
import difflib, hashlib, json, re, subprocess, sys

base = Path(__file__).resolve().parent
root = base.parents[2]
prefix = 'song-perframes'
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

# Revalidate the existing resolved graph; it still contains the native oracle
# and production legacy dependencies until the separate production cutover.
deps = read(base/'song-static-schedule-test-dependencies.json')
graph = subprocess.run(deps['command'], cwd=root, capture_output=True, check=True)
assert hashlib.sha256(graph.stdout).hexdigest() == deps['stdout_sha256']
(base/(prefix+'-current-test-dependencies.stdout')).write_bytes(graph.stdout)

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
assert results[reg]['tests_passed'] == 445 and results[reg]['tests_failed'] == 0
assert len(results[reg]['test_results']) == 24
assert any('212 passed; 0 failed;' in line for line in results[reg]['test_results'])
snapshot = read(base/(prefix+'-edit-baseline.json'))
for value in snapshot.values():
    assert hashlib.sha256(value['text'].encode()).hexdigest() == value['sha256']
edited = sorted(snapshot)
assert all(current[p] != snapshot[p]['sha256'] for p in edited)
old = lambda path: snapshot[path]['text'].replace('\r\n','\n')

native = 'crates/deadsync-song-lua/src/perframe.rs'
raw = (root/native).read_bytes().decode()
eol = '\r\n' if '\r\n' in snapshot[native]['text'] else '\n'
header = eol.join(['pub(crate) fn perframe_sample_iter(', '    start: f32,', '    end: f32,',
    ') -> impl Iterator<Item = SongLuaPerframeSample> {'])
original_header = 'fn perframe_sample_iter(start: f32, end: f32) -> impl Iterator<Item = SongLuaPerframeSample> {'
assert raw.replace(header, original_header) == snapshot[native]['text']
schedule = 'crates/deadsync-song-lua/src/rust_engine/song_static_schedule.rs'
body = (root/schedule).read_text().replace('pub(super) roots:', 'roots:').replace('pub(super) perframes:', 'perframes:')
assert body == old(schedule)
runtime = 'crates/deadsync-song-lua/src/rust_engine.rs'
assert (root/runtime).read_text().replace('mod song_perframes;\n','') == old(runtime)
actors = 'crates/deadsync-song-lua/src/rust_engine/actors.rs'
assert (root/actors).read_text().replace('pub(super) mod perframes;\n','') == old(actors)
capture = 'crates/deadsync-song-lua/src/rust_engine/actors/capture.rs'
body = (root/capture).read_text()
body = body.replace('    // Native preparation fields remain present after clearing capture blocks.\n'
    '    // Cloning/restoring a capture also restores this semantic field presence.\n'
    '    pub(super) transform_fields: u16,\n', '')
a = body.index('        for (index, value) in [', body.index('pub(super) fn write(&mut self, delta: Delta'))
z = body.index('        let start = scope.unwrap_or(0.);', a)
assert body[:a] + body[z:] == old(capture)
effects = 'crates/deadsync-song-lua/src/rust_engine/actors/effects.rs'
body = (root/effects).read_text().replace('    replay_epoch: Option<f32>,\n','')
a = body.index('pub(super) fn sync_preparation_motion(')
z = body.index('#[derive(Clone, Copy, Default)]\nstruct Motion', a)
assert body[:a] + body[z:] == old(effects)
support = 'crates/deadsync-song-lua/src/rust_engine/test_support.rs'
assert (root/support).read_text().startswith(old(support).rstrip()+'\n')
test = 'crates/deadsync-song-lua/tests/rust_song_activation.rs'
text = (root/test).read_text()
assert text.startswith(old(test).rstrip()+'\n')
assert len(re.findall(r'^#\[test\]', text, re.M)) == 212 and '#[ignore' not in text
assert len(re.findall(r'^#\[test\]\nfn sampled_perframes_', text, re.M)) == 5
added = ['crates/deadsync-song-lua/src/rust_engine/song_perframes.rs',
         'crates/deadsync-song-lua/src/rust_engine/actors/perframes.rs']
subprocess.run(['rustfmt', '--check', '--edition', '2024', '--config',
                'skip_children=true', *[p for p in edited if p != native], *added], cwd=root, check=True, capture_output=True)

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
assert all(unused(k) for k in other), other
for path in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']:
    assert current[path] == prior[path] == read(base/reg/'sources.json')[path] == read(base/lint/'sources.json')[path]
assert sha(base/'checks.py') == '05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'

diff = ''.join(''.join(difflib.unified_diff(
    old(p).splitlines(keepends=True), (root/p).read_text().splitlines(keepends=True),
    fromfile='before/'+p, tofile=p)) for p in edited)
diff += ''.join(''.join(difflib.unified_diff([], (root/p).read_text().splitlines(keepends=True),
    fromfile='/dev/null', tofile=p)) for p in added)
diff_path = base/(prefix+'-source-diff.diff')
diff_path.write_text(diff)
report = dict(goal_complete=False, production_cutover=False, final_application_validation=False,
    acceptance_scope='objective.txt', objective_sha256=objective_sha, source_count=len(current),
    adapter_changed=edited, added=added, native_perframe_only_shares_existing_sample_iterator=True,
    static_schedule_only_shares_private_keeper_fields=True,
    existing_actor_dispatch_unchanged=True, existing_effect_dispatch_unchanged=True,
    existing_capture_block_algorithm_unchanged=True, transform_field_presence_retained_in_capture=True,
    engine_and_manifests_unchanged_by_adapter=True, other_source_changes_preserved=other,
    watched_dependency_sources_match_checks=True,
    out_of_dependency_graph_changes_since_checks=differences,
    earlier_207_activation_tests_unchanged=True, new_cases=5,
    adapter_formatting_passed=True, song_clippy_baseline_unchanged=True,
    runner_unchanged=True, resolved_graph_sha256=hashlib.sha256(graph.stdout).hexdigest(),
    source_diff_sha256=sha(diff_path),
    results={s: {k: r[k] for k in ['exit_code', 'seconds', 'log_sha256', 'tests_passed',
              'tests_failed', 'sources_unchanged']} for s, r in results.items()})
(base/(prefix+'-reviewed-sources.json')).write_text(json.dumps(current, indent=2)+'\n')
(base/(prefix+'-review.json')).write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report))
