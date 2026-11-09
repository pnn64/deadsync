"""Check scope and saved native algorithms for the chronological Rust driver."""
from pathlib import Path
import base64,hashlib,json,re,subprocess,sys
from collections import Counter
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
saved=read(base/'song-update-driver-edit-baseline.json')
for p,v in saved.items():assert hashlib.sha256(base64.b64decode(v['base64'])).hexdigest()==v['sha256']
native='crates/deadsync-song-lua/src/perframe.rs';expected=base64.b64decode(saved[native]['base64'])
for marker in [b'const PLAYER_TRANSFORM_CAPTURE_KEYS',b'fn sample_player_pose(',b'fn capture_field_wrappers(']:
    assert expected.count(marker)==1;expected=expected.replace(marker,b'pub(crate) '+marker)
assert (root/native).read_bytes()==expected,'Native compiler changed beyond declaration visibility'
native='crates/deadsync-song-lua/src/lua_util.rs';expected=base64.b64decode(saved[native]['base64'])
a=expected.index(b'pub struct SongLuaStartupState {');b=expected.index(b'\n}',a);chunk=expected[a:b]
for name in [b'steps',b'commands',b'block_steps',b'clock_path']:
    old=b'    '+name+b':';assert chunk.count(old)==1;chunk=chunk.replace(old,b'    pub(crate) '+name+b':')
assert (root/native).read_bytes()==expected[:a]+chunk+expected[b:],'Native runtime changed beyond owned startup visibility'
subprocess.run([sys.executable,str(base/'prepare_update_samples.py'),'--check'],cwd=root,check=True)
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()};before=read(base/'song-update-driver-before-sources.json')
changed=sorted(p for p in current.keys()&before.keys() if current[p]!=before[p]);added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys()
assert set(changed)<=set(saved)
assert added==sorted(['crates/deadsync-song-lua/src/rust_engine/actors/chronological.rs','crates/deadsync-song-lua/src/rust_engine/song_updates.rs','crates/deadsync-song-lua/src/rust_engine/test_support/update_driver.rs','crates/deadsync-song-lua/tests/rust_song_updates.rs'])
assert all(not p.startswith('crates/deadlib-lua/') for p in changed)
assert all(not p.startswith('crates/deadsync-song-lua/tests/') for p in changed)
assert all(current[p]==before[p] for p in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
test=root/'crates/deadsync-song-lua/tests/rust_song_updates.rs'
text=test.read_text();prior=(base/'song-update-driver-nine-cases.rs').read_text()
assert text[text.index('#[test]'):text.index('#[test]\nfn complete_driver_uses_exact_song_timing')].rstrip()==prior[prior.index('#[test]'):].rstrip()
assert len(re.findall(r'^#\[test\]',text,re.M))==10 and '#[ignore' not in text
format_paths=[p for p in changed+added if p.endswith('.rs') and p not in ['crates/deadsync-song-lua/src/lua_util.rs','crates/deadsync-song-lua/src/perframe.rs']]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*format_paths],cwd=root,check=True,capture_output=True)
assert sha(base/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(base/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,source_count=len(current),changed=changed,added=added,native_algorithms_byte_unchanged=True,engine_unchanged=True,existing_tests_byte_unchanged=True,formatting_passed=True)
if sys.argv[1:]==['--sources-only']:
    print(json.dumps(report));raise SystemExit(0)
assert len(sys.argv[1:])==2
for stage in sys.argv[1:]:
    result=read(base/stage/'result.json')
    assert result['sources_unchanged'] and read(base/stage/'sources.json')==current
    assert sha(base/stage/'run.log')==result['log_sha256']
    report.setdefault('results',{})[stage]=result
reg,lint=sys.argv[1:];a=report['results'][reg];b=report['results'][lint]
assert a['exit_code']==0 and a['tests_passed']==290 and a['tests_failed']==0
assert len(a['test_results'])==9
for line in a['test_results']:assert '0 failed; 0 ignored; 0 measured; 0 filtered out;' in line
def diagnostics(p):return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text()))
assert b['exit_code']==101
assert diagnostics(base/lint/'run.log')==diagnostics(base/'song-frame-handoff-lint-v1-song-linux-debug/run.log')
deps=read(base/'song-static-schedule-test-dependencies.json')
graph=subprocess.run(deps['command'],cwd=root,capture_output=True,check=True)
assert hashlib.sha256(graph.stdout).hexdigest()==deps['stdout_sha256']
(base/'song-update-driver-current-test-dependencies.stdout').write_bytes(graph.stdout)
report.update(tests_passed=290,test_targets=9,new_complete_driver_cases=10,zero_ignored_or_filtered_tests=True,clippy_diagnostics_match_prior_baseline=True,test_dependency_graph_unchanged=True)
(base/'song-update-driver-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n')
(base/'song-update-driver-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
