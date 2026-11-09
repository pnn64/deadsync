"""Review existing column outputs ported to the independent Rust song host."""
from pathlib import Path
import difflib, hashlib, json, re, subprocess, sys, tomllib

base = Path(__file__).resolve().parent
root = base.parents[2]
prefix = 'song-column-output'
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
reg, lint = sys.argv[1:]
assert sha(base/'objective.txt') == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
crates = ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love',
          'deadsync-assets','deadsync-shell','deadsync-notefield']
names = subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}',
    'ls-files','--cached','--others','--exclude-standard','-z','--',
    *[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths = {root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current = {p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
deps = read(base/'song-static-schedule-test-dependencies.json')
graph = subprocess.run(deps['command'],cwd=root,capture_output=True,check=True)
assert hashlib.sha256(graph.stdout).hexdigest() == deps['stdout_sha256']
(base/(prefix+'-current-test-dependencies.stdout')).write_bytes(graph.stdout)
def unused(path):
    p = Path(path).parts
    return len(p)>2 and p[0]=='crates' and p[1] not in deps['packages']
results = {s:read(base/s/'result.json') for s in [reg,lint]}
differences = {}
for stage,result in results.items():
    assert sha(base/stage/'run.log') == result['log_sha256']
    checked = read(base/stage/'sources.json')
    differences[stage] = sorted(k for k in current.keys()|checked.keys() if current.get(k)!=checked.get(k))
    assert all(unused(k) for k in differences[stage]),(stage,differences[stage])
assert results[reg]['exit_code']==0 and results[reg]['tests_passed']==220 and results[reg]['tests_failed']==0
assert len(results[reg]['test_results'])==2
assert any('212 passed; 0 failed;' in s for s in results[reg]['test_results'])
assert any('8 passed; 0 failed;' in s for s in results[reg]['test_results'])
snapshot = read(base/(prefix+'-edit-baseline.json'))
for value in snapshot.values():
    assert hashlib.sha256(value['text'].encode()).hexdigest()==value['sha256']
old = lambda p:snapshot[p]['text'].replace('\r\n','\n')
activation = 'crates/deadsync-song-lua/tests/rust_song_activation.rs'
assert current[activation] == snapshot[activation]['sha256']
assert len(re.findall(r'^#\[test\]',(root/activation).read_text(),re.M))==212
edited = sorted(p for p in snapshot if current[p]!=snapshot[p]['sha256'])
assert len(edited)==6
native = 'crates/deadsync-song-lua/src/perframe.rs'
raw = (root/native).read_bytes().decode()
raw = raw.replace('pub(crate) struct ColumnSplineCapture {','struct ColumnSplineCapture {')
raw = raw.replace('    pub(crate) fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {',
                  '    fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {')
raw = raw.replace('    pub(crate) fn finish(self, out: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>) {',
                  '    fn finish(self, out: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>) {')
assert raw == snapshot[native]['text']
helpers = 'crates/deadsync-song-lua/src/rust_engine/actors/song_columns.rs'
body = (root/helpers).read_text().replace('pub(super) enum Numeric {','enum Numeric {')
for name in ['numeric','text','point','note_field']:
    body = body.replace('pub(super) fn '+name+'(','fn '+name+'(')
body = body.replace('pub(super) fn with_table<T>(','fn with_table<T>(')
def formatted(text):
    return subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=text,
                          capture_output=True,text=True,check=True).stdout
assert formatted(body)==formatted(old(helpers))
runtime = 'crates/deadsync-song-lua/src/rust_engine.rs'
body = (root/runtime).read_text().replace('mod song_column_output;\n','').replace('pub use song_column_output::SongColumnSplineCapture;\n','')
assert body==old(runtime)
actors = 'crates/deadsync-song-lua/src/rust_engine/actors.rs'
assert (root/actors).read_text().replace('mod column_output;\n','')==old(actors)
support = 'crates/deadsync-song-lua/src/rust_engine/test_support.rs'
assert (root/support).read_text().startswith(old(support).rstrip()+'\n')
manifest = 'crates/deadsync-song-lua/Cargo.toml'
raw = (root/manifest).read_bytes().decode()
eol = '\r\n' if '\r\n' in snapshot[manifest]['text'] else '\n'
assert raw == snapshot[manifest]['text']+eol+eol.join(['[[test]]',
    'name = "rust_song_column_output"','required-features = ["rust-engine", "test-support"]',''])
parsed = tomllib.loads(raw)
assert parsed['test'].pop() == {'name':'rust_song_column_output','required-features':['rust-engine','test-support']}
assert parsed == tomllib.loads(snapshot[manifest]['text'])
added = ['crates/deadsync-song-lua/src/rust_engine/song_column_output.rs',
         'crates/deadsync-song-lua/src/rust_engine/actors/column_output.rs',
         'crates/deadsync-song-lua/tests/rust_song_column_output.rs']
text = (root/added[2]).read_text()
assert len(re.findall(r'^#\[test\]',text,re.M))==8 and '#[ignore' not in text
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',
    *[p for p in edited if p.endswith('.rs') and p!=native],*added],cwd=root,check=True,capture_output=True)
def diagnostics(path):
    return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',path.read_text())
assert results[lint]['exit_code']==101
assert diagnostics(base/lint/'run.log')==diagnostics(base/'song-command-locals-lint-v1-song-linux-debug/run.log')
prior = read(base/(prefix+'-before-sources.json'))
changed = sorted(k for k in current.keys()&prior.keys() if current[k]!=prior[k])
new = sorted(current.keys()-prior.keys())
removed = sorted(prior.keys()-current.keys())
assert set(edited)<=set(changed) and set(added)<=set(new)
other = sorted(k for k in changed+new+removed if k not in edited and k not in added)
assert all(unused(k) for k in other),other
for path in ['Cargo.toml','Cargo.lock','.cargo/config.toml']:
    assert current[path]==prior[path]==read(base/reg/'sources.json')[path]==read(base/lint/'sources.json')[path]
assert sha(base/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
diff = ''.join(''.join(difflib.unified_diff(old(p).splitlines(keepends=True),
    (root/p).read_text().splitlines(keepends=True),fromfile='before/'+p,tofile=p)) for p in edited)
diff += ''.join(''.join(difflib.unified_diff([],(root/p).read_text().splitlines(keepends=True),
    fromfile='/dev/null',tofile=p)) for p in added)
diff_path = base/(prefix+'-source-diff.diff')
diff_path.write_text(diff)
report = dict(goal_complete=False,production_cutover=False,final_application_validation=False,
    acceptance_scope='objective.txt',objective_sha256=sha(base/'objective.txt'),source_count=len(current),
    adapter_changed=edited,added=added,native_accumulator_only_shares_visibility=True,
    existing_column_reader_only_shares_private_helpers_and_formats_signatures=True,
    existing_actor_dispatch_unchanged=True,engine_unchanged=True,production_dependencies_unchanged=True,
    manifest_only_registers_new_test_feature_requirements=True,
    original_212_activation_tests_byte_unchanged=True,new_cases=8,adapter_formatting_passed=True,
    song_clippy_baseline_unchanged=True,runner_unchanged=True,
    watched_dependency_sources_match_checks=True,out_of_dependency_graph_changes_since_checks=differences,
    other_source_changes_preserved=other,resolved_graph_sha256=hashlib.sha256(graph.stdout).hexdigest(),
    source_diff_sha256=sha(diff_path),results={s:{k:r[k] for k in ['exit_code','seconds','log_sha256',
        'tests_passed','tests_failed','sources_unchanged']} for s,r in results.items()})
(base/(prefix+'-reviewed-sources.json')).write_text(json.dumps(current,indent=2)+'\n')
(base/(prefix+'-review.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
