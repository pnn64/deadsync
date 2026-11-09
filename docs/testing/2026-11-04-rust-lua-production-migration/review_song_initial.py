"""Verify the final initial-state integration sources and isolated oracle."""
from pathlib import Path
import hashlib,json,re,subprocess,tomllib
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
song='song-initial-adapters-v1-song-linux-debug';lint='song-initial-lint-v1-song-linux-debug'
results={s:read(base/s/'result.json')for s in [song,lint]}
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
for stage,r in results.items():
    assert r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
    assert current==read(base/stage/'sources.json')
prior=read(base/'song-layers-adapters-v1-song-linux-debug/sources.json')
snapshot=read(base/'song-initial-edit-baseline.json')
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert changed==sorted(snapshot)
assert added==['crates/deadsync-song-lua/src/rust_engine/actors/initial_state.rs']
for p,v in snapshot.items():assert prior[p]==v['sha256']
compiler='crates/deadsync-song-lua/src/compile.rs'
original=snapshot[compiler]['text'];now=(root/compiler).read_bytes().decode()
marker='\r\n#[cfg(all(feature = "test-support", feature = "rust-engine"))]\r\n#[derive(Clone)]\r\npub(crate) struct MigrationInitialStates'
start=now.index(marker);prefix=now[:start];bridge=now[start:]
for spaces in [8,12]:
    indent=' '*spaces
    # The original compiler body uses LF; its previously appended oracle uses
    # CRLF. Match the two new body hooks without normalizing original bytes.
    hook='\n'+indent+'#[cfg(all(feature = "test-support", feature = "rust-engine"))]\n'+indent+'migration_record_initial(&lua, &initial_actor_states, &screen_layer_states);\n'
    assert prefix.count(hook)==1
    prefix=prefix.replace(hook,'\n')
assert prefix==original
assert bridge.count('#[cfg(all(feature = "test-support", feature = "rust-engine"))]')==2
assert bridge.count('fn migration_record_initial')==1
for p,v in read(base/'gameplay-reload-review.json')['fixtures'].items():assert sha(root/p)==v
oldtests=snapshot['crates/deadsync-song-lua/tests/rust_song_layers.rs']['text'].encode()
newtests=(root/'crates/deadsync-song-lua/tests/rust_song_layers.rs').read_bytes()
assert newtests.startswith(oldtests) and b'#[ignore' not in newtests
assert len(re.findall(rb'^#\[test\]',newtests,re.M))==10
expected=sorted(t['name']for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test']if t['name'].startswith('rust_'))
command=results[song]['command'];actual=sorted(command[i+1]for i,a in enumerate(command)if a=='--test')
assert actual==expected and len(actual)==55
assert results[song]['exit_code']==0 and results[song]['tests_failed']==0 and results[song]['tests_passed']==566
formatted=[p for p in changed+added if p.endswith('.rs') and p!=compiler]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True)
f=subprocess.check_output(['rustfmt','--edition','2024','--emit','stdout'],input=bridge.encode()).decode().strip()
assert f==bridge.replace('\r\n','\n').strip()
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[lint]['exit_code']==101
assert diagnostics(base/lint/'run.log')==diagnostics(base/'song-layers-lint-v1-song-linux-debug/run.log')
assert '[cfg(feature = "rust-engine")]\npub mod rust_engine;'in(root/'crates/deadsync-song-lua/src/lib.rs').read_text()
# All changes are inside the optional adapter, its tests, or the proven gated
# observer above. Manifests and the prior feature-tree inputs are unchanged.
reused=read(base/'song-layers-review.json')['reused_component_results']
for stage in reused:
    report=read(base/stage/'result.json')
    assert report['exit_code']==0 and report['sources_unchanged']
    assert sha(base/stage/'run.log')==report['log_sha256']
report={'goal_complete':False,'song_tests_passed':566,'song_targets':55,'new_cases':3,'source_count':len(current),'changed':changed,'added':added,'removed':[],'unchanged':len(current)-len(changed)-len(added),'normal_production_compiler_unchanged':True,'prior_tests_and_fixtures_unchanged':True,'formatting_passed':True,'song_clippy_baseline_unchanged':True,'results':{s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()},'reused_components':list(reused),'remaining':'Complete startup/navigation, prepared/live outputs, fallback/cache, unconditional production song cutover and final fixtures/application/render/dependency/performance evidence'}
(base/'song-initial-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
