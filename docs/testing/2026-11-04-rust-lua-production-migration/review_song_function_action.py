"""Verify full function-action probes against the fixed migration scope."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
reg,lint=sys.argv[1:]
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
deps=read(base/'song-function-action-test-dependencies.json');assert deps['exit_code']==0 and sha(base/'song-function-action-test-dependencies.stdout')==deps['stdout_sha256']
def unused(path):
 p=Path(path).parts
 return len(p)>2 and p[0]=='crates'and p[1]not in deps['packages']
results={s:read(base/s/'result.json')for s in [reg,lint]};differences={}
for s,r in results.items():
 assert sha(base/s/'run.log')==r['log_sha256']
 checked=read(base/s/'sources.json');differences[s]=sorted(k for k in current.keys()|checked.keys()if current.get(k)!=checked.get(k))
 assert all(unused(k)for k in differences[s]),(s,differences[s])
assert results[reg]['exit_code']==0 and results[reg]['tests_passed']==350 and results[reg]['tests_failed']==0 and len(results[reg]['test_results'])==24
snapshot=read(base/'song-function-action-edit-baseline.json')
for n,v in snapshot.items():assert hashlib.sha256(v['text'].encode()).hexdigest()==v['sha256']
test='crates/deadsync-song-lua/tests/rust_song_activation.rs';text=(root/test).read_text();assert text.startswith(snapshot[test]['text'].replace('\r\n','\n').rstrip()+'\n');assert len(re.findall(r'^#\[test\]',text,re.M))==117 and '#[ignore'not in text
added=['crates/deadsync-song-lua/src/rust_engine/actors/function_action.rs','crates/deadsync-song-lua/src/rust_engine/song_action.rs','crates/deadsync-song-lua/src/rust_engine/actors/sound_manager.rs']
formatted=list(snapshot)+added
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*formatted],cwd=root,check=True,capture_output=True)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[lint]['exit_code']==101 and diagnostics(base/lint/'run.log')==diagnostics(base/'song-command-locals-lint-v1-song-linux-debug/run.log')
prior=read(base/'song-message-probe-reviewed-sources.json');changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);new=sorted(current.keys()-prior.keys());removed=sorted(prior.keys()-current.keys())
assert set(snapshot)<=set(changed)and set(added)<=set(new)
assert all(unused(k)for k in changed+new+removed if k not in snapshot and k not in added)
report=dict(goal_complete=False,source_count=len(current),adapter_changed=sorted(snapshot),native_change="no native production or oracle implementation changed; test-support delegates to existing public native helper",added=added,other_source_changes_preserved=sorted(k for k in changed+new+removed if k not in snapshot and k not in added),watched_dependency_sources_match_checks=True,out_of_dependency_graph_changes_since_checks=differences,earlier_109_activation_tests_unchanged=True,new_cases=8,adapter_formatting_passed=True,song_clippy_baseline_unchanged=True,production_cutover=False,final_application_validation=False,results={s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()})
(base/'song-function-action-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n');(base/'song-function-action-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
