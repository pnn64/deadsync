"""Verify immediate property capture regressions and preservation against recorded source."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
stages=sys.argv[1:]
assert len(stages)==2,"regression and lint stage directories required"
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
results={s:read(base/s/'result.json')for s in stages}
for stage,r in results.items():
 assert r['sources_unchanged'] and sha(base/stage/'run.log')==r['log_sha256']
 assert current==read(base/stage/'sources.json'),stage
regression=results[stages[0]]
assert (regression['exit_code'],regression['tests_passed'],regression['tests_failed'])==(101,128,1)
failed=re.findall(r'(?m)^    (immediate_capture_[a-z_]+)$',(base/stages[0]/'run.log').read_text())
assert failed==['immediate_capture_publishes_sprite_state_culling_and_framebuffer_flags'],failed
assert len(results[stages[0]]['test_results'])==8
prior=read(base/'song-message-provenance-focused-v5-song-linux-debug/sources.json')
snapshot=read(base/'song-immediate-capture-edit-baseline.json')
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert added==['crates/deadsync-song-lua/src/rust_engine/actors/song_immediate.rs'],added
assert set(changed)==snapshot.keys(),changed
for p,v in snapshot.items():assert prior[p]==v['sha256']
testpath='crates/deadsync-song-lua/tests/rust_song_activation.rs'
s=(root/testpath).read_text();assert s.startswith(snapshot[testpath]['text'].replace('\r\n','\n').rstrip())
assert len(re.findall(r'^#\[test\]',s,re.M))==39 and '#[ignore'not in s
for path,digest in prior.items():
 if path not in changed:assert current[path]==digest,path
for p,v in read(base/'gameplay-reload-review.json')['fixtures'].items():assert sha(root/p)==v
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*changed,*added],cwd=root,check=True,capture_output=True)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[stages[-1]]['exit_code']==101
assert diagnostics(base/stages[-1]/'run.log')==diagnostics(base/'song-message-provenance-lint-v1-song-linux-debug/run.log')
reused=read(base/'song-message-provenance-review.json')['reused_components']
for stage in reused:
 r=read(base/stage/'result.json');assert r['exit_code']==0 and r['sources_unchanged']
 assert sha(base/stage/'run.log')==r['log_sha256']
report=dict(goal_complete=False,regressions_passed=False,unresolved_regressions=failed,source_count=len(current),changed=changed,added=added,removed=[],unchanged=len(current)-len(changed)-len(added),normal_production_compiler_unchanged=True,prior_tests_and_fixtures_unchanged=True,formatting_passed=True,song_clippy_baseline_unchanged=True,new_cases=6,results={s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()},reused_components=reused)
(base/'song-immediate-capture-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
