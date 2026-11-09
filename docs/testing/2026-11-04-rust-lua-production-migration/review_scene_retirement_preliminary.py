"""Audit private host table state and outgoing actor lifetime evidence."""
from pathlib import Path
import json,hashlib,re,subprocess,tomllib
base=Path(__file__).resolve().parent
root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
stages={'core':'scene-retirement-core-v1-core-linux-debug','song':'scene-retirement-adapters-v1-song-linux-debug','core_lint':'scene-retirement-core-lint-v1-core-linux-debug','song_lint':'scene-retirement-lint-v1-song-linux-debug','noteskin':'scene-retirement-noteskin-v1-noteskin-linux-debug','gifs':'scene-retirement-gifs-v1-gifs-linux-debug'}
results={k:read(base/v/'result.json') for k,v in stages.items()}
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
for k,d in results.items():
    assert d['sources_unchanged'] and sha(base/stages[k]/'run.log')==d['log_sha256']
    assert current==read(base/stages[k]/'sources.json')
for k in ['core','song','core_lint','noteskin','gifs']:assert results[k]['exit_code']==0
for k in ['noteskin','gifs']:assert results[k]['tests_failed']==0
assert results['noteskin']['tests_passed']==read(base/'noteskin-callbacks-complete-noteskin-linux-debug/result.json')['tests_passed']
assert results['gifs']['tests_passed']==sum(len(re.findall(r'#\[test\]',p.read_text())) for p in (root/'crates/deadsync-theme-simply-love/src').rglob('step_stats_gifs.rs'))
assert results['core']['tests_failed']==results['song']['tests_failed']==0
before=read(base/'gameplay-reload-adapters-v1-song-linux-debug/sources.json')
previous=read(base/'gameplay-reload-review.json')
snapshot=read(base/'scene-retirement-edit-baseline.json')
changed=sorted(k for k in current.keys()&before.keys() if current[k]!=before[k])
added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys()
assert changed==sorted(snapshot)
assert added==sorted(['crates/deadlib-lua/tests/host_table_state.rs','crates/deadsync-song-lua/src/rust_engine/actors/retirement.rs','crates/deadsync-song-lua/tests/rust_scene_retirement.rs'])
for k,v in snapshot.items():assert hashlib.sha256(v['text'].encode()).hexdigest()==v['sha256']==before[k]
assert all(current[k]==v for k,v in before.items() if '/tests/' in k)
native=[*previous['native_controls_unchanged'],'crates/deadsync-song-lua/src/compile.rs']
assert all(current[k]==before[k] for k in native)
assert all(sha(root/k)==v for k,v in previous['fixtures'].items())
expected=sorted(t['name'] for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
command=results['song']['command'];actual=sorted(command[i+1] for i,a in enumerate(command) if a=='--test')
assert actual==expected and len(expected)==len(results['song']['test_results'])
log=(base/stages['song']/'run.log').read_text();block=log.split('Running tests/rust_scene_retirement.rs',1)[1].split('Running tests/',1)[0]
newcases=len(re.findall(r'^#\[test\]',(root/'crates/deadsync-song-lua/tests/rust_scene_retirement.rs').read_text(),re.M))
assert f'test result: ok. {newcases} passed; 0 failed; 0 ignored;' in block
assert results['song']['tests_passed']==previous['tests_passed']+newcases
priorcore=read(base/'actor-meshes-final-core-core-linux-debug/result.json')
assert results['core']['tests_passed']==priorcore['tests_passed']+5
corelog=(base/stages['core']/'run.log').read_text()
assert 'private_host_state_write_traces_white_graph_from_a_black_owner ... ok' in corelog
assert 'Running tests/host_table_state.rs' in corelog
for p in added:
    if '/tests/' in p:assert '#[ignore' not in (root/p).read_text()
formatted=read(base/'scene-retirement-formatted.json')
assert sorted(formatted)==sorted(k for k in changed+added if k.endswith('.rs'))
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True,text=True)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results['song_lint']['exit_code']==101
assert diagnostics(base/stages['song_lint']/'run.log')==diagnostics(base/'gameplay-reload-lint-v1-song-linux-debug/run.log')
report={'goal_complete':False,'core_tests_passed':results['core']['tests_passed'],'song_tests_passed':results['song']['tests_passed'],'noteskin_tests_passed':results['noteskin']['tests_passed'],'gif_tests_passed':results['gifs']['tests_passed'],'song_targets':len(expected),'new_retirement_cases':newcases,'source_count':len(current),'final_sources_match_all_runs':True,'changed':changed,'added':added,'prior_tests_and_native_controls_unchanged':True,'formatting_passed':True,'core_clippy_passed':True,'song_lint_messages_files_unchanged':True,'log_sha256':{stages[k]:d['log_sha256'] for k,d in results.items()},'remaining':'Compiler/session integration and automatic retirement; compiled/live output, fallback/cache, unconditional production cutover, final fixture/application/render/dependency/performance validation'}
(base/'scene-retirement-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
