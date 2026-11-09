"""Audit activation sources, compiler observer isolation and focused regressions."""
from pathlib import Path
import hashlib,json,re,subprocess
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
stages=['song-activation-focused-v5-song-linux-debug','song-activation-shared-v1-song-linux-debug','song-activation-lint-v1-song-linux-debug']
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
results={stage:read(base/stage/'result.json')for stage in stages}
for stage,r in results.items():
    assert r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
    assert current==read(base/stage/'sources.json'),stage
for stage in stages[:2]:assert results[stage]['exit_code']==0 and results[stage]['tests_failed']==0
prior=read(base/'song-initial-adapters-v1-song-linux-debug/sources.json')
snapshot=read(base/'song-activation-edit-baseline.json')
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert set(changed)<=snapshot.keys(),changed
assert added==['crates/deadsync-song-lua/src/rust_engine/actors/song_activation.rs','crates/deadsync-song-lua/src/rust_engine/song_activation.rs','crates/deadsync-song-lua/tests/rust_song_activation.rs']
for p,v in snapshot.items():assert prior[p]==v['sha256']
compiler='crates/deadsync-song-lua/src/compile.rs';utility='crates/deadsync-song-lua/src/lua_util.rs'
def remove_block(s,start,end):
    a=s.index(start);z=s.index(end,a)+len(end);return s[:a]+s[z:]
s=(root/compiler).read_bytes().decode()
s=remove_block(s,'    // The oracle supplies an empty VM','        lua.remove_app_data::<MigrationFreshCompile>().is_some());\n')
s=remove_block(s,'        #[cfg(all(feature = "test-support", feature = "rust-engine"))]\n        if let Some(mut probe)', '        }\n')
marker='\n#[cfg(all(feature = "test-support", feature = "rust-engine"))]\n#[derive(Default)]\npub(crate) struct MigrationActivationProbe'
suffix=s[s.index(marker):];s=s[:s.index(marker)]
assert s==snapshot[compiler]['text'],'non-observer compiler change'
assert suffix.count('#[cfg(all(feature = "test-support", feature = "rust-engine"))]')==3
s=(root/utility).read_bytes().decode()
s=remove_block(s,'    #[cfg(all(feature = "test-support", feature = "rust-engine"))]\n    if let Some(probe) = lua.remove_app_data::<crate::compile::MigrationActivationProbe>()', '    }\n')
assert s==snapshot[utility]['text'],'non-observer startup change'
manifest='crates/deadsync-song-lua/Cargo.toml'
assert (root/manifest).read_bytes()==snapshot[manifest]['text'].encode()+b'\n[[test]]\nname = "rust_song_activation"\nrequired-features = ["rust-engine", "test-support"]\n'
for path,digest in prior.items():
    if path.startswith('crates/deadsync-song-lua/tests/'):assert current[path]==digest,path
for p,v in read(base/'gameplay-reload-review.json')['fixtures'].items():assert sha(root/p)==v
test=(root/added[-1]).read_bytes();assert len(re.findall(rb'^#\[test\]',test,re.M))==9 and b'#[ignore' not in test
formatted=[p for p in changed+added if p.endswith('.rs') and p not in [compiler,utility]]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True)
assert subprocess.check_output(['rustfmt','--edition','2024','--emit','stdout'],input=suffix.encode()).decode().strip()==suffix.replace('\r\n','\n').strip()
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[stages[-1]]['exit_code']==101
assert diagnostics(base/stages[-1]/'run.log')==diagnostics(base/'song-initial-lint-v1-song-linux-debug/run.log')
# Shared command entry points retain their previous default draining behavior;
# only the activation On traversal selects the new no-final-drain variant.
assert 'run_named_with_drain(call, store, actor, name, params, true)'in(root/'crates/deadsync-song-lua/src/rust_engine/actors.rs').read_text()
assert current['crates/deadsync-song-lua/src/rust_engine/actors/lifecycle.rs']==prior['crates/deadsync-song-lua/src/rust_engine/actors/lifecycle.rs']
reused=read(base/'song-initial-review.json')['reused_components']
for stage in reused:
    r=read(base/stage/'result.json');assert r['exit_code']==0 and r['sources_unchanged']
    assert sha(base/stage/'run.log')==r['log_sha256']
for path in prior:
    if not path.startswith('crates/deadsync-song-lua/'):
        assert current[path]==prior[path]
report=dict(goal_complete=False,source_count=len(current),changed=changed,added=added,removed=[],unchanged=len(current)-len(changed)-len(added),normal_production_compiler_unchanged=True,prior_tests_and_fixtures_unchanged=True,formatting_passed=True,song_clippy_baseline_unchanged=True,new_cases=9,results={s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()},reused_components=reused,remaining='Initial-update/wrapper and broadcast capture outputs, shared-state queued discovery/rewind, prepared/live output, reload orchestration, fallback/cache, unconditional cutover and final fixture/app/render/dependency/performance evidence')
(base/'song-activation-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
