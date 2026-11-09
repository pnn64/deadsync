"""Verify live-layer changes against preserved sources and exact final checks."""
import base64,hashlib,json,re,subprocess,sys,tomllib
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=read(d/'song-live-layers-before-sources.json');saved=read(d/'song-live-layers-edit-baseline.json')
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed=sorted(p for p in current.keys()&before.keys() if current[p]!=before[p]);added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys();assert set(changed)==saved.keys(),changed
assert added==sorted('crates/deadsync-song-lua/'+p for p in ['src/rust_engine/live_layers.rs','src/rust_engine/actors/live_layers.rs','tests/rust_live_layers.rs']),added
for p,x in saved.items():assert hashlib.sha256(base64.b64decode(x['base64'])).hexdigest()==x['sha256']==before[p]
assert all(current[p]==before[p] for p in before if p.startswith('crates/deadsync-song-lua/tests/'))
assert all(current[p]==before[p] for p in before if p.startswith('crates/deadlib-lua/'))
assert all(current[p]==before[p] for p in before if p in ['crates/deadsync-song-lua/src/live.rs','crates/deadsync-song-lua/src/lua_util.rs','crates/deadsync-song-lua/src/compile.rs'])
p='crates/deadsync-song-lua/src/rust_engine/actors/song_activation.rs'
old=base64.b64decode(saved[p]['base64']).decode().replace('\r\n','\n');new=(r/p).read_text()
signature="fn startup(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {"
replacement="pub(super) fn startup(\n    call: &mut HostCall<'_>,\n    store: &Shared,\n    actor: TableId,\n) -> Result<(), Error> {"
assert new==old.replace(signature,replacement)
p='crates/deadsync-song-lua/src/rust_engine/actors/initial_state.rs'
old=base64.b64decode(saved[p]['base64']).decode().replace('\r\n','\n');new=(r/p).read_text()
marker=') -> Result<SongLuaOverlayState, Error> {'
old=old[old.index(marker)+len(marker):]
new=new[new.index('fn read_with_metadata('):];new=new[new.index(marker)+len(marker):]
a=new.index('    if !metadata {');b=new.index('    let class =',a);new=new[:a]+new[b:]
new=new.replace('if !metadata || !store.lock().unwrap().actors.contains_key(&actor)', 'if !store.lock().unwrap().actors.contains_key(&actor)')
assert re.sub(r'\s+','',new)==re.sub(r'\s+','',old),'existing reader changed beyond its explicit reset branch'
p='crates/deadsync-song-lua/Cargo.toml';a=tomllib.loads(base64.b64decode(saved[p]['base64']).decode());b=tomllib.loads((r/p).read_text())
assert b.pop('test')==a.pop('test')+[dict(name='rust_live_layers',**{'required-features':['rust-engine','test-support']})];assert a==b
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in changed+added if p.endswith('.rs')]],check=True,capture_output=True)
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
tests=(r/'crates/deadsync-song-lua/tests/rust_live_layers.rs').read_text();assert '#[ignore]' not in tests and '.eval()' not in tests
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,previous_goal_turn='progress: live actor traversal and player feedback; 204 tests',current_goal_turn='progress: Rust live background and foreground scheduling',source_count=len(current),changed=changed,added=added,existing_tests_byte_unchanged=True,native_algorithms_byte_unchanged=True,engine_unchanged=True,existing_reader_semantics_unchanged=True,startup_body_unchanged=True,formatting_passed=True,live_layer_cases=len(re.findall(r'(?m)^#\[test\]',tests)),full_live_coordinator_and_cutover_pending=True)
if sys.argv[1:]==['--preflight']:print(json.dumps(report));sys.exit(0)
assert len(sys.argv[1:])==3
reg,focus,lint=sys.argv[1:];results={}
for stage in [reg,focus,lint]:
 x=read(d/stage/'result.json');assert x['sources_unchanged'] and read(d/stage/'sources.json')==current
 assert sha(d/stage/'run.log')==x['log_sha256'];results[stage]=x
for stage in [reg,focus]:
 x=results[stage];assert x['exit_code']==0 and x['tests_failed']==0 and x['tests_passed']>0
 assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in s for s in x['test_results'])
expected={'rust_live_layers','rust_live_actors','rust_live_frames','rust_song_layers','rust_song_activation','rust_song_multitap','rust_song_updates','rust_song_update_output','rust_song_compiler','rust_song_observations','rust_song_diagnostics','rust_actors','rust_player_parts','rust_generated_players','rust_notefields','rust_wrappers','rust_transitions','rust_scene_retirement','rust_statistics','rust_render','rust_effects','rust_sprites','rust_models','rust_bitmap_text'}
cmd=results[reg]['command'];assert {cmd[i+1] for i,v in enumerate(cmd) if v=='--test'}==expected;assert len(results[reg]['test_results'])==len(expected)
assert results[focus]['tests_passed']>=report['live_layer_cases']
assert results[lint]['exit_code']==101
assert (d/lint/'run.log').read_bytes()==(d/'song-live-actors-lint-v1-song-linux-debug/run.log').read_bytes()
cfg=read(d/'song-live-layers-shipping-cfg/result.json');assert cfg['exit_code']==0 and cfg['sources_unchanged'] and cfg['sources']==current
report.update(tests_passed=results[reg]['tests_passed'],test_targets=len(expected),focused_tests=results[focus]['tests_passed'],clippy_log_matches_prior_baseline=True,without_test_support_compiles=True,results=results)
(d/'song-live-layers-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n');(d/'song-live-layers-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
