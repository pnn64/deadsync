"""Verify bounded compiler observation wiring without changing old coverage."""
import base64,hashlib,json,re,subprocess,sys,tomllib
from collections import Counter
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
b=read(d/'song-observations-before-sources.json');saved=read(d/'song-observations-edit-baseline.json')
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed=sorted(p for p in b.keys()&current.keys() if b[p]!=current[p]);added=sorted(current.keys()-b.keys())
assert not b.keys()-current.keys()
assert set(changed)<=saved.keys(),set(changed)-saved.keys()
assert added==sorted('crates/deadsync-song-lua/'+n for n in ['src/rust_engine/observations.rs','tests/rust_song_observations.rs'])
for name,row in saved.items():
 raw=base64.b64decode(row['base64']);assert hashlib.sha256(raw).hexdigest()==row['sha256']==b[name],name
assert all(p=='crates/deadsync-song-lua/Cargo.toml' or p=='crates/deadsync-song-lua/src/rust_engine.rs' or p.startswith('crates/deadsync-song-lua/src/rust_engine/') for p in changed)
assert all(current[p]==b[p] for p in b if p.startswith('crates/deadsync-song-lua/tests/'))
manifest='crates/deadsync-song-lua/Cargo.toml';old=base64.b64decode(saved[manifest]['base64'])
assert (r/manifest).read_bytes().startswith(old)
a=tomllib.loads(old.decode('utf-8-sig'));c=tomllib.loads((r/manifest).read_text(encoding='utf-8-sig'))
assert c.pop('test')==a.pop('test')+[dict(name='rust_song_observations',**{'required-features':['rust-engine','test-support']})]
assert a==c
tests=(r/'crates/deadsync-song-lua/tests/rust_song_observations.rs').read_text()
prior=(d/'song-observations-tests-before-v3.rs').read_text()
def cases(s):
 ms=list(re.finditer(r'(?m)^#\[test\]\nfn (\w+)\(\) \{',s))
 return {m[1]:s[m.start():ms[i+1].start() if i+1<len(ms) else len(s)].strip() for i,m in enumerate(ms)}
before=cases(prior);after=cases(tests)
assert len(before)==7 and len(after)>=9
repaired={'chronological_direct_queued_update_and_message_writes_match',
 'chronological_option_setters_and_gc_match', 'multilayer_screen_and_player_write_indices_match',
 'nested_manager_calls_and_reload_history_match', 'startup_queue_capture_excludes_noteskin_discovery_writes'}
assert all(after[k]==v for k,v in before.items() if k not in repaired)
original=read(d/'song-observations-original-fixtures.json')
assert len(original)==9 and len(after)>=14
assert all(source in tests for sources in original.values() for source in sources)
for field in ['boolean_writes','speed_writes','noteskin_writes','state_calls','overlay_writes']:
 assert re.search(r'^        '+field+r'[,\n]',tests,re.M),field
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in changed+added if p.endswith('.rs')]],check=True,capture_output=True)
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,source_count=len(current),changed=changed,added=added,existing_tests_byte_unchanged=True,native_algorithms_byte_unchanged=True,engine_unchanged=True,formatting_passed=True,observation_cases=len(after),all_five_fields_compared=True,live_session_integration_pending=True)
if sys.argv[1:]==['--preflight']:
 print(json.dumps(report));sys.exit(0)
assert len(sys.argv[1:])==3
reg,focus,lint=sys.argv[1:];results={}
for stage in [reg,focus,lint]:
 x=read(d/stage/'result.json');assert x['sources_unchanged'] and read(d/stage/'sources.json')==current
 assert sha(d/stage/'run.log')==x['log_sha256'];results[stage]=x
for stage in [reg,focus]:
 x=results[stage];assert x['exit_code']==0 and x['tests_failed']==0 and x['tests_passed']>0
 assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in s for s in x['test_results'])
assert any(s.startswith('test result: ok. 212 passed;') for s in results[reg]['test_results'])
def targets(command):return [command[i+1] for i,v in enumerate(command) if v=='--test']
expected=set(targets(read(d/'song-callback-errors-regressions-v1-song-linux-debug/result.json')['command']))
expected.update(['rust_song_observations','rust_game_state','rust_player_state','rust_screen_manager','rust_utility_managers','rust_song_options','rust_top_screen'])
assert set(targets(results[reg]['command']))==expected
assert len(expected)==len(results[reg]['test_results'])==28
assert results[focus]['tests_passed']==23+len(after)
assert set(targets(results[focus]['command']))=={'rust_song_compiler','rust_song_diagnostics','rust_song_observations'}
def diagnostics(p):return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text()))
assert results[lint]['exit_code']==101
assert diagnostics(d/lint/'run.log')==diagnostics(d/'song-callback-errors-lint-v3-song-linux-debug/run.log')
cfg=read(d/'song-observations-shipping-cfg-v2/result.json')
assert cfg['exit_code']==0 and cfg['sources_unchanged'] and cfg['sources']==current
report.update(tests_passed=results[reg]['tests_passed'],test_targets=len(results[reg]['test_results']),focused_tests=results[focus]['tests_passed'],clippy_diagnostics_match_prior_baseline=True,without_test_support_compiles=True,results=results)
(d/'song-observations-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n')
(d/'song-observations-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
