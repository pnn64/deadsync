"""Review live host-phase implementation and its exact final-source checks."""
import base64,hashlib,json,re,subprocess,sys,tomllib
from collections import Counter
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=read(d/'song-live-frames-before-sources.json');saved=read(d/'song-live-frames-edit-baseline.json')
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed=sorted(p for p in before.keys()&current.keys() if before[p]!=current[p]);added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys()
assert set(changed)==saved.keys(),(changed,list(saved))
assert added==sorted('crates/deadsync-song-lua/'+n for n in ['src/rust_engine/live_frames.rs','src/rust_engine/actors/live_frames.rs','tests/rust_live_frames.rs'])
for n,v in saved.items():
 raw=base64.b64decode(v['base64']);assert hashlib.sha256(raw).hexdigest()==v['sha256']==before[n]
assert all(current[p]==before[p] for p in before if p.startswith('crates/deadsync-song-lua/tests/'))
native='crates/deadsync-song-lua/src/live.rs';old=base64.b64decode(saved[native]['base64'])
assert (r/native).read_bytes().startswith(old)
suffix=(r/native).read_bytes()[len(old):].decode().strip()
assert suffix=='''#[cfg(all(feature = "test-support", feature = "rust-engine"))]
pub(crate) fn migration_finish_health_update(lua: &Lua) -> mlua::Result<()> {
    finish_health_update(lua)
}'''
helper='crates/deadsync-song-lua/src/rust_engine/test_support.rs'
old=base64.b64decode(saved[helper]['base64']).decode().replace('\r\n','\n').strip()
assert (r/helper).read_text().startswith(old)
manifest='crates/deadsync-song-lua/Cargo.toml';old=base64.b64decode(saved[manifest]['base64'])
assert (r/manifest).read_bytes().startswith(old)
a=tomllib.loads(old.decode('utf-8-sig'));b=tomllib.loads((r/manifest).read_text(encoding='utf-8-sig'))
assert b.pop('test')==a.pop('test')+[dict(name='rust_live_frames',**{'required-features':['rust-engine','test-support']})]
assert a==b
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in changed+added if p.endswith('.rs') and p!=native]],check=True,capture_output=True)
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
def cases(text):
 ms=list(re.finditer(r'(?m)^#\[test\]\nfn (\w+)\(\) \{',text))
 return {m[1]:text[m.start():ms[i+1].start() if i+1<len(ms) else len(text)].strip() for i,m in enumerate(ms)}
tests=(r/'crates/deadsync-song-lua/tests/rust_live_frames.rs').read_text()
original=cases((d/'song-live-frames-tests-before-v3.rs').read_text());final=cases(tests)
assert len(original)==12 and len(final)==17
for n,v in original.items():
 if n=='disabled_player_position_stays_owned_while_statistics_still_feed':
  v=v.replace("GetChild('LifeP2')", "GetChild('PlayerP2')")
 if n!='health_thresholds_and_fail_off_match_native_phase':assert final[n]==v,n
assert 'original_life_meter_call_keeps_prior_health_event_and_reports_unsupported_method' in final
assert 'disabled_following_player_keeps_health_fallback_after_nested_life_write' in final
assert '.call(())' in tests and '.eval()' not in tests
assert all(marker not in tests for marker in ['#[ignore]','LIVE_FRAME_DEBUG'])
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,previous_goal_turn='progress: five compiler observation fields wired and verified',source_count=len(current),changed=changed,added=added,existing_tests_byte_unchanged=True,native_algorithms_byte_unchanged=True,engine_unchanged=True,formatting_passed=True,live_phase_cases=len(final),live_session_coordinator_and_cutover_pending=True)
if sys.argv[1:]==['--preflight']:print(json.dumps(report));sys.exit(0)
assert len(sys.argv[1:])==3
reg,focus,lint=sys.argv[1:];results={}
for stage in [reg,focus,lint]:
 x=read(d/stage/'result.json');assert x['sources_unchanged'] and read(d/stage/'sources.json')==current
 assert sha(d/stage/'run.log')==x['log_sha256'];results[stage]=x
for stage in [reg,focus]:
 x=results[stage];assert x['exit_code']==0 and x['tests_failed']==0 and x['tests_passed']>0
 assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in s for s in x['test_results'])
expected={'rust_live_frames','rust_clock','rust_statistics','rust_player_state','rust_song_options','rust_game_state','rust_screen_manager','rust_messages','rust_player_parts','rust_generated_players','rust_gameplay_underlay','rust_gameplay_reload','rust_scene_retirement','rust_song_observations'}
cmd=results[reg]['command'];assert {cmd[i+1] for i,v in enumerate(cmd) if v=='--test'}==expected
assert len(results[reg]['test_results'])==len(expected)==14
assert results[focus]['tests_passed']==17
def diagnostics(p):return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text()))
assert results[lint]['exit_code']==101
assert diagnostics(d/lint/'run.log')==diagnostics(d/'song-observations-lint-v2-song-linux-debug/run.log')
cfg=read(d/'song-live-frames-shipping-cfg/result.json');assert cfg['exit_code']==0 and cfg['sources_unchanged'] and cfg['sources']==current
report.update(tests_passed=results[reg]['tests_passed'],test_targets=len(expected),focused_tests=17,clippy_diagnostics_match_prior_baseline=True,without_test_support_compiles=True,results=results)
(d/'song-live-frames-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n')
(d/'song-live-frames-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
