"""Audit the Rust static update compiler against its exact edit baseline."""
from pathlib import Path
import base64, difflib, hashlib, json, re, subprocess, sys, tomllib

base = Path(__file__).resolve().parent
root = base.parents[2]
prefix = 'song-static-update'
read = lambda p: json.loads(p.read_text(encoding='utf-8-sig'))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
sources_only = sys.argv[1:] == ['--sources-only']
reg, lint = (None, None) if sources_only else sys.argv[1:]
before = read(base/(prefix+'-before-sources.json'))
saved = read(base/(prefix+'-edit-baseline.json'))
old = {p:base64.b64decode(s['base64']) for p,s in saved.items()}
for p, data in old.items():
    assert hashlib.sha256(data).hexdigest() == saved[p]['sha256']
assert sha(base/'objective.txt') == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
crates = ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love',
          'deadsync-assets','deadsync-shell','deadsync-notefield']
names = subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}',
    'ls-files','--cached','--others','--exclude-standard','-z','--',
    *[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths = {root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current = {p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed = sorted(p for p in current.keys()&before.keys() if current[p]!=before[p])
added = sorted(current.keys()-before.keys())
removed = sorted(before.keys()-current.keys())
expected = [
    'crates/deadsync-song-lua/Cargo.toml',
    'crates/deadsync-song-lua/src/perframe.rs',
    'crates/deadsync-song-lua/src/rust_engine.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors.rs',
    'crates/deadsync-song-lua/src/rust_engine/test_support.rs',
]
assert changed==expected,changed
assert added==[
    'crates/deadsync-song-lua/src/rust_engine/actors/update_options.rs',
    'crates/deadsync-song-lua/src/rust_engine/song_update_output.rs',
    'crates/deadsync-song-lua/tests/rust_song_update_output.rs',
],added
assert not removed,removed
native = expected[1]
raw = (root/native).read_bytes().decode()
for declaration in ['trait ModState {','struct ModSnapshot(', 'struct ModSnapshotScratch {',
                    'fn push_update_mod_targets_with_key<S: ModState>(', 'fn states(', 'fn player_speeds(']:
    assert 'pub(crate) '+declaration in raw,declaration
    raw = raw.replace('pub(crate) '+declaration,declaration)
assert raw.encode()==old[native], 'Native compiler changed beyond shared visibility'
runtime, actors, support = expected[2:]
text = (root/runtime).read_text().replace('mod song_update_output;\n','').replace('pub use song_update_output::SongStaticUpdateOutput;\n','')
assert text==old[runtime].decode().replace('\r\n','\n')
assert (root/actors).read_text().replace('pub(super) mod update_options;\n','')==old[actors].decode().replace('\r\n','\n')
assert (root/support).read_text().startswith(old[support].decode().replace('\r\n','\n').rstrip()+'\n')
manifest = expected[0]
parsed = tomllib.loads((root/manifest).read_text())
assert parsed['test'].pop()=={'name':'rust_song_update_output','required-features':['rust-engine','test-support']}
assert parsed==tomllib.loads(old[manifest].decode())
for p in ['crates/deadsync-song-lua/tests/rust_song_activation.rs','crates/deadsync-song-lua/tests/rust_song_column_output.rs','crates/deadsync-song-lua/tests/rust_player_options.rs']:
    assert current[p]==before[p],p
assert len(re.findall(r'^#\[test\]',(root/added[-1]).read_text(),re.M))==8
assert '#[ignore' not in (root/added[-1]).read_text()
format_paths=[p for p in changed+added if p.endswith('.rs') and p!=native]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*format_paths],cwd=root,check=True,capture_output=True)
if sources_only:
    print(json.dumps(dict(source_count=len(current),changed=changed,added=added,
        native_compiler_only_shares_visibility=True,existing_tests_byte_unchanged=True,
        engine_sources_unchanged=True,formatting_passed=True)))
    raise SystemExit(0)
results = {s:read(base/s/'result.json') for s in [reg,lint]}
for stage,result in results.items():
    assert sha(base/stage/'run.log')==result['log_sha256']
    assert result['sources_unchanged']
    assert read(base/stage/'sources.json')==current,stage
assert results[reg]['exit_code']==0 and results[reg]['tests_passed']==239 and results[reg]['tests_failed']==0
assert len(results[reg]['test_results'])==4
assert results[lint]['exit_code']==101
diagnostics = lambda p:re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert diagnostics(base/lint/'run.log')==diagnostics(base/'song-command-locals-lint-v1-song-linux-debug/run.log')
deps = read(base/'song-static-schedule-test-dependencies.json')
graph = subprocess.run(deps['command'],cwd=root,capture_output=True,check=True)
assert hashlib.sha256(graph.stdout).hexdigest()==deps['stdout_sha256']
(base/(prefix+'-current-test-dependencies.stdout')).write_bytes(graph.stdout)
assert sha(base/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
diff=''
for p in changed+added:
    previous=old[p].decode().replace('\r\n','\n') if p in old else ''
    diff+=''.join(difflib.unified_diff(previous.splitlines(keepends=True),
        (root/p).read_text().splitlines(keepends=True),fromfile='before/'+p if p in old else '/dev/null',tofile=p))
diff_path=base/(prefix+'-source-diff.diff')
diff_path.write_text(diff)
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,
    source_count=len(current),changed=changed,added=added,removed=removed,
    native_compiler_only_shares_visibility=True,existing_tests_byte_unchanged=True,
    original_212_activation_cases_preserved=True,new_cases=8,adapter_tests_passed=239,
    adapter_targets=4,formatting_passed=True,clippy_diagnostics_match_preexisting_baseline=True,
    dependency_graph_unchanged=True,engine_sources_unchanged=True,runner_unchanged=True,
    objective_sha256=sha(base/'objective.txt'),source_diff_sha256=sha(diff_path),
    results={s:{k:r[k] for k in ['exit_code','seconds','tests_passed','tests_failed','log_sha256','sources_unchanged']} for s,r in results.items()})
(base/(prefix+'-reviewed-sources.json')).write_text(json.dumps(current,indent=2)+'\n')
(base/(prefix+'-review.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
