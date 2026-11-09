"""Audit authored construction, production-oracle isolation and component reuse."""
from pathlib import Path
import hashlib,json,re,subprocess,tomllib
base=Path(__file__).resolve().parent
root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
song='song-layers-adapters-v1-song-linux-debug';lint='song-layers-lint-v1-song-linux-debug'
results={s:read(base/s/'result.json')for s in [song,lint]}
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
for stage,r in results.items():
    assert r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
    assert current==read(base/stage/'sources.json')
prior=read(base/'scene-retirement-adapters-v2-song-linux-debug/sources.json')
snapshot=read(base/'song-layers-edit-baseline.json')
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k])
added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert changed==sorted(snapshot)
assert added==sorted('crates/deadsync-song-lua/'+s for s in ['src/rust_engine/song_layers.rs','src/rust_engine/compiler_debug.rs','src/rust_engine/actors/song_layers.rs','tests/rust_song_layers.rs'])
for k,v in snapshot.items():assert hashlib.sha256(v['text'].encode()).hexdigest()==v['sha256']==prior[k]
assert all(current[k]==v for k,v in prior.items()if '/tests/' in k)
# The sole change to the production compiler is an appended cfg-isolated oracle.
compiler='crates/deadsync-song-lua/src/compile.rs'
original=snapshot[compiler]['text'].encode();now=(root/compiler).read_bytes()
assert now.startswith(original)
bridge=now[len(original):].decode().strip()
assert bridge.startswith('// Migration oracle: execute the production compiler')
assert bridge.count('#[cfg(all(feature = "test-support", feature = "rust-engine"))]')==1
assert bridge.count('pub(crate) fn migration_compile_song_layers')==1
for p,v in read(base/'gameplay-reload-review.json')['fixtures'].items():assert sha(root/p)==v
expected=sorted(t['name']for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test']if t['name'].startswith('rust_'))
command=results[song]['command'];actual=sorted(command[i+1]for i,a in enumerate(command)if a=='--test')
assert actual==expected and len(actual)==55
new=(root/'crates/deadsync-song-lua/tests/rust_song_layers.rs').read_text()
cases=len(re.findall(r'^#\[test\]',new,re.M));assert cases==7 and '#[ignore' not in new
assert results[song]['exit_code']==0 and results[song]['tests_failed']==0
assert results[song]['tests_passed']==read(base/'scene-retirement-review.json')['song_tests_passed']+cases
log=(base/song/'run.log').read_text();block=log.split('Running tests/rust_song_layers.rs',1)[1].split('Running tests/',1)[0]
assert f'test result: ok. {cases} passed; 0 failed; 0 ignored;'in block
formatted=read(base/'song-layers-formatted.json')
assert sorted(formatted+[compiler])==sorted(p for p in changed+added if p.endswith('.rs'))
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True)
f=subprocess.check_output(['rustfmt','--edition','2024','--emit','stdout'],input=bridge.encode()).decode().strip()
assert f==bridge.replace('\r\n','\n')
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[lint]['exit_code']==101
assert diagnostics(base/lint/'run.log')==diagnostics(base/'scene-retirement-lint-v2-song-linux-debug/run.log')
# Core/noteskin have no song dependency. GIF normal dependencies exclude the
# optional adapter, the new test target, and the appended feature-gated bridge.
feature_command=['cargo','tree','--locked','-p','deadsync-theme-simply-love','-e','features','--prefix','none','--format','{p} features={f}']
feature_text=subprocess.check_output(feature_command,cwd=root,text=True)
lines=[line for line in feature_text.splitlines()if line.startswith('deadsync-song-lua v')]
assert lines and all('rust-engine'not in line for line in lines)
(base/'song-layers-gif-features.json').write_text(json.dumps({'command':feature_command,'song_dependency_lines':lines,'rust_engine_enabled':False},indent=2)+'\n')
manifest='crates/deadsync-song-lua/Cargo.toml'
m=tomllib.loads((root/manifest).read_text());old=tomllib.loads(snapshot[manifest]['text'])
assert {k:v for k,v in m.items()if k!='test'}=={k:v for k,v in old.items()if k!='test'}
assert '[cfg(feature = "rust-engine")]\npub mod rust_engine;'in(root/'crates/deadsync-song-lua/src/lib.rs').read_text()
reused={}
for stage in ['scene-retirement-core-v1-core-linux-debug','scene-retirement-core-lint-v1-core-linux-debug','scene-retirement-noteskin-v1-noteskin-linux-debug','scene-retirement-gifs-v1-gifs-linux-debug']:
    r=read(base/stage/'result.json');previous=read(base/stage/'sources.json')
    assert r['exit_code']==0 and r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
    assert not previous.keys()-current.keys()
    diff={p for p in current if p not in previous or current[p]!=previous[p]}
    for p in diff:
        assert p in {compiler,manifest,'crates/deadsync-song-lua/src/rust_engine.rs'}or p.startswith(('crates/deadsync-song-lua/src/rust_engine/','crates/deadsync-song-lua/tests/rust_'))
    assert previous[compiler]==snapshot[compiler]['sha256']
    assert previous[manifest]==snapshot[manifest]['sha256']
    reused[stage]={'tests_passed':r['tests_passed'],'log_sha256':r['log_sha256'],'changed_excluded_song_files':sorted(diff)}
report={'goal_complete':False,'song_tests_passed':results[song]['tests_passed'],'song_targets':len(expected),'new_cases':cases,'source_count':len(current),'final_song_sources_match':True,'changed':changed,'added':added,'production_compiler_prefix_unchanged':True,'previous_tests_and_fixtures_unchanged':True,'formatting_passed':True,'song_clippy_baseline_unchanged':True,'results':{s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()},'reused_component_results':reused,'remaining':'Full startup/navigation, initial capture and prepared/live outputs, fallback/cache, unconditional production song cutover and final fixture/application/render/dependency/performance evidence'}
(base/'song-layers-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='reused_component_results'}))
