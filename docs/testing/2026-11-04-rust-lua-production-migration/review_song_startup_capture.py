"""Verify startup capture integration and the isolated production oracle."""
from pathlib import Path
import hashlib,json,re,subprocess
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
stages=['song-startup-capture-focused-v3-song-linux-debug','song-startup-capture-focused-v4-song-linux-debug','song-startup-capture-lint-v2-song-linux-debug']
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
results={stage:read(base/stage/'result.json')for stage in stages}
for stage,r in results.items():
    assert r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
    sources=read(base/stage/'sources.json')
    if stage==stages[0]:
        style=read(base/'song-startup-capture-lint-style-before.json')
        assert sources[style['path']]==style['sha256']
        assert set(sources)==set(current)
        assert [p for p in sources if sources[p]!=current[p]]==[style['path']]
        old=style['text'].replace('\r\n','\n')
        old=old.replace('            states\n                .entry(wrapper)\n                .or_insert_with(SongLuaOverlayState::default);','            states.entry(wrapper).or_default();')
        assert old==(root/style['path']).read_text(),'non-style change after broad checks'
    else:assert current==sources,stage
for stage in stages[:2]:assert results[stage]['exit_code']==0 and results[stage]['tests_failed']==0
prior=read(base/'song-activation-focused-v5-song-linux-debug/sources.json')
snapshot=read(base/'song-startup-capture-edit-baseline.json')
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert set(changed)==snapshot.keys(),changed
assert added==['crates/deadsync-song-lua/src/rust_engine/actors/song_capture.rs']
for p,v in snapshot.items():assert prior[p]==v['sha256']
compiler='crates/deadsync-song-lua/src/compile.rs';utility='crates/deadsync-song-lua/src/lua_util.rs'
s=(root/compiler).read_bytes().decode()
for line in [
'    pub initial_states: Vec<(Table, crate::SongLuaOverlayState)>,\n',
'    pub updates: std::collections::HashMap<usize, crate::rust_engine::SongStartupState>,\n',
'    pub broadcasts: Vec<(f32, String, bool)>,\n']:
    assert s.count(line)==1;s=s.replace(line,'')
assert s==snapshot[compiler]['text'],'non-observer compiler change'
def without_observer(s):
    a=s.index('    #[cfg(all(feature = "test-support", feature = "rust-engine"))]\n    if let Some(probe) = lua.remove_app_data::<crate::compile::MigrationActivationProbe>()')
    z=s.index('\n    }',a)+len('\n    }\n')
    return s[:a]+s[z:]
s=(root/utility).read_bytes().decode()
marker='\n#[cfg(all(feature = "test-support", feature = "rust-engine"))]\npub(crate) fn migration_startup_tween_capture'
suffix=s[s.index(marker):];s=s[:s.index(marker)]
assert without_observer(s)==without_observer(snapshot[utility]['text']),'non-observer startup utility change'
# New comparisons precede the old harness; all old tests and fixtures are intact.
testpath='crates/deadsync-song-lua/tests/rust_song_activation.rs'
s=(root/testpath).read_text();a=s.index('\nfn saved_pair(');z=s.index('\nimpl Pair {',a)
recovered=(s[:a]+s[z:]).replace('    context: SongLuaCompileContext,\n}\n\nimpl Pair {','    context: SongLuaCompileContext,\n}\nimpl Pair {',1)
assert recovered==snapshot[testpath]['text'].replace('\r\n','\n')
assert len(re.findall(r'^#\[test\]',s,re.M))==12 and '#[ignore'not in s
for path,digest in prior.items():
    if path.startswith('crates/deadsync-song-lua/tests/') and path!=testpath:assert current[path]==digest,path
for p,v in read(base/'gameplay-reload-review.json')['fixtures'].items():assert sha(root/p)==v
formatted=[p for p in changed+added if p.endswith('.rs') and p not in [compiler,utility]]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True)
assert subprocess.check_output(['rustfmt','--edition','2024','--emit','stdout'],input=suffix.encode()).decode().strip()==suffix.replace('\r\n','\n').strip()
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[stages[-1]]['exit_code']==101
assert diagnostics(base/stages[-1]/'run.log')==diagnostics(base/'song-activation-lint-v1-song-linux-debug/run.log')
for path in prior:
    if not path.startswith('crates/deadsync-song-lua/'):assert current[path]==prior[path]
assert current['crates/deadsync-song-lua/Cargo.toml']==prior['crates/deadsync-song-lua/Cargo.toml']
reused=read(base/'song-activation-review.json')['reused_components']
for stage in reused:
    r=read(base/stage/'result.json');assert r['exit_code']==0 and r['sources_unchanged']
    assert sha(base/stage/'run.log')==r['log_sha256']
report=dict(goal_complete=False,source_count=len(current),changed=changed,added=added,removed=[],unchanged=len(current)-len(changed)-len(added),normal_production_compiler_unchanged=True,prior_tests_and_fixtures_unchanged=True,formatting_passed=True,song_clippy_baseline_unchanged=True,new_cases=3,final_activation_recheck=12,broad_regressions_before_style_cleanup=90,only_or_default_cleanup_after_broad_run=True,results={s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()},reused_components=reused,remaining='Shared-state queued discovery/rewind, prepared/live output integration, reload orchestration, fallback/cache, unconditional cutover and final fixture/app/render/dependency/performance evidence')
(base/'song-startup-capture-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
