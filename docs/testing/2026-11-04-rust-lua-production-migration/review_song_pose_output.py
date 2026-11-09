"""Review fallback pose output without changing the production migration scope."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
base=Path(__file__).resolve().parent;root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
stages=sys.argv[1:]
assert len(stages)==2
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}'for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p)for p in sorted(paths)if p.is_file()}
results={s:read(base/s/'result.json')for s in stages}
dependencies=read(base/'song-pose-output-test-dependencies.json')
assert dependencies['exit_code']==0
assert sha(base/'song-pose-output-test-dependencies.stdout')==dependencies['stdout_sha256']
packages=set(dependencies['packages'])
def unused(path):
 parts=Path(path).parts
 return len(parts)>2 and parts[0]=='crates' and parts[1]not in packages
source_changes={}
for stage,r in results.items():
 assert sha(base/stage/'run.log')==r['log_sha256']
 checked=read(base/stage/'sources.json')
 changed_since_check=sorted(k for k in current.keys()|checked.keys()if current.get(k)!=checked.get(k))
 assert all(unused(k)for k in changed_since_check),(stage,changed_since_check)
 source_changes[stage]=changed_since_check
regression=results[stages[0]]
assert regression['exit_code']==0 and regression['tests_passed']==162 and regression['tests_failed']==0
assert len(regression['test_results'])==9
prior=read(base/'song-fallback-execution-lint-v1-song-linux-debug/sources.json')
snapshot=read(base/'song-pose-output-edit-baseline.json')
external=read(base/'song-pose-output-external-source-changes.json')
external_added=read(base/'song-pose-output-external-source-additions.json')
our_added=['crates/deadsync-song-lua/src/rust_engine/actors/song_pose.rs']
changed=sorted(k for k in current.keys()&prior.keys()if current[k]!=prior[k]);added=sorted(current.keys()-prior.keys())
assert not prior.keys()-current.keys()
assert added==sorted(our_added+list(external_added)),added
for p,v in external_added.items():assert current[p]==v['current_sha256'],p
assert snapshot.keys()<=set(changed),changed
external_changed=sorted(set(changed)-snapshot.keys())
for p,v in snapshot.items():
 assert prior[p]==v['sha256']
 assert hashlib.sha256(v['text'].encode()).hexdigest()==v['sha256']
for p,v in external.items():
 assert prior[p]==v['expected_sha256']
 if not unused(p):assert current[p]==v['current_sha256'],p
observed_external={}
for p in external_changed:
 raw=(root/p).read_bytes()
 observed_external[p]=dict(prior_sha256=prior[p],current_sha256=current[p],crlf_only=hashlib.sha256(raw.replace(b'\r\n',b'\n')).hexdigest()==prior[p],outside_test_dependency_graph=unused(p))
s=(root/'crates/deadsync-song-lua/tests/rust_song_activation.rs').read_text()
old=snapshot['crates/deadsync-song-lua/tests/rust_song_activation.rs']['text'].replace('\r\n','\n')
assert s.startswith(old.rstrip()+'\n')
assert len(re.findall(r'^#\[test\]',s,re.M))==59 and '#[ignore'not in s
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*snapshot,*our_added],cwd=root,check=True,capture_output=True)
def diagnostics(p):return re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
assert results[stages[-1]]['exit_code']==101
assert diagnostics(base/stages[-1]/'run.log')==diagnostics(base/'song-fallback-execution-lint-v1-song-linux-debug/run.log')
report=dict(goal_complete=False,source_count=len(current),watched_dependency_sources_match_checks=True,out_of_dependency_graph_changes_since_checks=source_changes,adapter_changed=sorted(snapshot),adapter_added=our_added,external_additions_preserved=external_added,added=added,external_changes_preserved=len(observed_external),external_crlf_only=sum(v['crlf_only']for v in observed_external.values()),other_external_changes=sum(not v['crlf_only']for v in observed_external.values()),removed=[],unchanged_from_previous_checkpoint=len(current)-len(changed)-len(added),earlier_56_activation_tests_unchanged=True,new_cases=3,formatting_passed=True,song_clippy_baseline_unchanged=True,production_cutover=False,final_application_validation=False,results={s:{k:r[k]for k in ['exit_code','seconds','log_sha256','tests_passed','tests_failed','sources_unchanged']}for s,r in results.items()})
(base/'song-pose-output-reviewed-external-sources.json').write_text(json.dumps(observed_external,indent=2)+'\n')
(base/'song-pose-output-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n')
(base/'song-pose-output-review.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
