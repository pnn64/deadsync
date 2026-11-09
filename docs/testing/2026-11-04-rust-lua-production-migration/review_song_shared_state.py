"""Audit shared-state discovery against the preceding source checkpoint."""
from pathlib import Path
import hashlib,json,re,subprocess
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
stages=['song-shared-state-focused-v4-song-linux-debug','song-shared-state-lint-v2-song-linux-debug']
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
results={s:read(base/s/'result.json')for s in stages}
for stage,r in results.items():
 assert r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
 assert current==read(base/stage/'sources.json'),stage
assert results[stages[0]]['exit_code']==0 and results[stages[0]]['tests_failed']==0
prior=read(base/'song-startup-capture-focused-v4-song-linux-debug/sources.json')
snapshot=read(base/'song-shared-state-edit-baseline.json')
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert set(changed)==snapshot.keys(),changed
assert added==['crates/deadsync-song-lua/src/rust_engine/function_snapshot.rs','crates/deadsync-song-lua/src/rust_engine/song_discovery.rs']
for p,v in snapshot.items():assert prior[p]==v['sha256']
utility='crates/deadsync-song-lua/src/lua_util.rs'
s=(root/utility).read_bytes().decode();old=snapshot[utility]['text'];assert s.startswith(old)
suffix=s[len(old):]
assert '#[cfg(all(feature = "test-support", feature = "rust-engine"))]'in suffix
assert subprocess.check_output(['rustfmt','--edition','2024','--emit','stdout'],input=suffix.encode()).decode().strip()==suffix.replace('\r\n','\n').strip()
testpath='crates/deadsync-song-lua/tests/rust_song_activation.rs'
s=(root/testpath).read_text();assert s.startswith(snapshot[testpath]['text'].replace('\r\n','\n'))
assert len(re.findall(r'^#\[test\]',s,re.M))==18 and '#[ignore'not in s
for path,digest in prior.items():
 if path.startswith('crates/deadsync-song-lua/tests/') and path!=testpath:assert current[path]==digest,path
for p,v in read(base/'gameplay-reload-review.json')['fixtures'].items():assert sha(root/p)==v
formatted=[p for p in changed+added if p.endswith('.rs') and p!=utility]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[stages[-1]]['exit_code']==101
assert diagnostics(base/stages[-1]/'run.log')==diagnostics(base/'song-startup-capture-lint-v2-song-linux-debug/run.log')
for path in prior:
 if not path.startswith('crates/deadsync-song-lua/'):assert current[path]==prior[path]
for path in ['crates/deadsync-song-lua/Cargo.toml','crates/deadsync-song-lua/src/compile.rs']:
 assert current[path]==prior[path]
reused=read(base/'song-startup-capture-review.json')['reused_components']
for stage in reused:
 r=read(base/stage/'result.json');assert r['exit_code']==0 and r['sources_unchanged']
 assert sha(base/stage/'run.log')==r['log_sha256']
report=dict(goal_complete=False,source_count=len(current),changed=changed,added=added,removed=[],unchanged=len(current)-len(changed)-len(added),normal_production_compiler_unchanged=True,prior_tests_and_fixtures_unchanged=True,formatting_passed=True,song_clippy_baseline_unchanged=True,new_cases=6,results={s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()},reused_components=reused,remaining='Queued actor-state/sprite capture and replay setup, prepared/live output integration, reload orchestration, fallback/cache, unconditional cutover and final fixture/app/render/dependency/performance evidence')
(base/'song-shared-state-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
