"""Audit getter handoff and pose publication against the fixed edit baseline."""
from pathlib import Path
from collections import Counter
import base64,difflib,hashlib,json,re,subprocess,sys
base=Path(__file__).resolve().parent;root=base.parents[2];prefix='song-frame-handoff'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
saved=read(base/(prefix+'-edit-baseline.json'))
old={p:base64.b64decode(v['base64']).decode().replace('\r\n','\n') for p,v in saved.items()}
for p,v in saved.items():assert hashlib.sha256(base64.b64decode(v['base64'])).hexdigest()==v['sha256']
fmt=lambda s:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=s,text=True,capture_output=True,check=True).stdout
canonical=lambda s:'\n'.join(l for l in fmt(s).splitlines() if l.strip())
def span(text,marker):
    start=text.index(marker);opening=text.index('{',start);depth=1;end=opening+1
    while depth:depth+=(text[end]=='{')-(text[end]=='}');end+=1
    return start,end
def remove(text,marker):
    a,b=span(text,marker);return text[:a]+text[b:]
def fn(text,marker):
    a,b=span(text,marker);return text[a:b]
subprocess.run([sys.executable,str(base/'prepare_frame_handoff.py'),'--check'],cwd=root,check=True,capture_output=True)
proof=read(base/(prefix+'-extraction-proof.json'))
for p,h in proof['checked'].items():assert sha(root/p)==h
actors='crates/deadsync-song-lua/src/rust_engine/actors.rs'
assert canonical((root/actors).read_text().replace('mod song_getters;\n',''))==canonical(old[actors])
access='crates/deadsync-song-lua/src/rust_engine/actors/perframes.rs'
body=(root/access).read_text()
for marker in ['    pub fn update_getter(','    pub fn reset_frame(','pub(super) fn is_player(']:body=remove(body,marker)
body=body.replace(fn(body,'    pub fn is_player('),fn(old[access],'    pub fn is_player('))
assert canonical(body)==canonical(old[access])
buffer='crates/deadsync-song-lua/src/rust_engine/actors/song_immediate.rs'
assert canonical(remove((root/buffer).read_text(),'    pub(super) fn publish_pose('))==canonical(old[buffer])
replay='crates/deadsync-song-lua/src/rust_engine/actors/song_replay.rs'
assert canonical(remove((root/replay).read_text(),'    pub(in crate::rust_engine) fn publish_song_tween_frame('))==canonical(old[replay])
capture='crates/deadsync-song-lua/src/rust_engine/actors/function_capture.rs'
body=(root/capture).read_text()
fields=fn(body,'fn reset_fields(');fields=fields[fields.index('{')+1:-1].strip()
body=remove(body,'pub(super) fn reset_frame(');body=remove(body,'fn reset_fields(')
body=body.replace('    reset_fields(call, actor)',fields)
assert canonical(body)==canonical(old[capture]),'Existing probe reset changed'
runtime='crates/deadsync-song-lua/src/rust_engine/song_frame_capture.rs'
body=(root/runtime).read_text()
for marker in ['    pub fn publish_song_tween_frame(','    pub fn set_song_overlay_getter_values(','    pub fn sample_song_overlay_schedule(']:
    start,end=span(body,marker)
    # Remove the new method's documentation, leaving prior code intact.
    line_start=body.rfind('\n',0,start)+1
    while line_start>0:
        prev=body.rfind('\n',0,line_start-1)+1
        if body[prev:line_start].lstrip().startswith('///'):line_start=prev
        else:break
    body=body[:line_start]+body[end:]
body=body.replace('.reset_frame(&mut self.runtime.vm,','.reset(&mut self.runtime.vm,')
assert canonical(body)==canonical(old[runtime]),'Track consumer changed beyond queue-preserving reset'
support='crates/deadsync-song-lua/src/rust_engine/test_support.rs'
assert (root/support).read_text().startswith(old[support].rstrip()+'\n')
test='crates/deadsync-song-lua/tests/rust_song_frame_capture.rs'
text=(root/test).read_text();prior=old[test][old[test].index('#[test]'):].rstrip()
preserved=text[text.index('#[test]'):text.index('#[test]\nfn handoff_')].rstrip()
assert preserved==prior,'Existing eight test bodies changed'
assert len(re.findall(r'^#\[test\]',text,re.M))==16
assert len(re.findall(r'^fn handoff_',text,re.M))==8 and '#[ignore' not in text
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()};before=read(base/(prefix+'-before-sources.json'))
changed=sorted(p for p in current.keys()&before.keys() if current[p]!=before[p]);added=sorted(current.keys()-before.keys())
assert set(changed)==set(p for p in saved if p.startswith('crates/')),changed
assert added==['crates/deadsync-song-lua/src/rust_engine/actors/song_getters.rs'] and not before.keys()-current.keys()
format_paths=[p for p in changed+added if p.endswith('.rs') and p!='crates/deadsync-song-lua/src/lua_util.rs']
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*format_paths],cwd=root,check=True,capture_output=True)
if sys.argv[1:]==['--sources-only']:
    print(json.dumps(dict(source_count=len(current),changed=changed,added=added,extraction_verified=True,old_test_bodies_preserved=True,existing_probe_and_getter_behavior_preserved=True)));raise SystemExit(0)
reg,lint=sys.argv[1:];results={s:read(base/s/'result.json') for s in [reg,lint]}
for stage,r in results.items():
    assert sha(base/stage/'run.log')==r['log_sha256'] and r['sources_unchanged']
    assert read(base/stage/'sources.json')==current
assert results[reg]['exit_code']==0 and results[reg]['tests_passed']==1104 and results[reg]['tests_failed']==0
assert len(results[reg]['test_results'])==6
for line in results[reg]['test_results']:assert '0 failed; 0 ignored; 0 measured; 0 filtered out;' in line
assert any('849 passed;' in s for s in results[reg]['test_results'])
assert any('16 passed;' in s for s in results[reg]['test_results'])
def diagnostics(p):return Counter(re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text()))
assert results[lint]['exit_code']==101
assert diagnostics(base/lint/'run.log')==diagnostics(base/'song-frame-capture-lint-v1-song-linux-debug/run.log')
deps=read(base/'song-static-schedule-test-dependencies.json');graph=subprocess.run(deps['command'],cwd=root,capture_output=True,check=True)
assert hashlib.sha256(graph.stdout).hexdigest()==deps['stdout_sha256'];(base/(prefix+'-current-test-dependencies.stdout')).write_bytes(graph.stdout)
assert sha(base/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(base/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
diff=''.join(''.join(difflib.unified_diff(old.get(p,'').splitlines(keepends=True),(root/p).read_text().splitlines(keepends=True),fromfile='before/'+p if p in old else '/dev/null',tofile=p)) for p in changed+added)
diff_path=base/(prefix+'-source-diff.diff');diff_path.write_text(diff)
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,source_count=len(current),changed=changed,added=added,
    scalar_routing_and_pose_metadata_extraction_verified=True,existing_test_bodies_preserved=True,old_probe_and_fast_getter_behavior_preserved=True,
    queue_ownership_preserved_by_frame_reset=True,engine_and_manifests_unchanged=True,formatting_passed=True,
    clippy_diagnostics_match_prior_baseline=True,dependency_graph_unchanged=True,library_tests_passed=849,adapter_tests_passed=255,
    total_tests_passed=1104,test_targets=6,new_cases=8,objective_sha256=sha(base/'objective.txt'),source_diff_sha256=sha(diff_path),results=results)
(base/(prefix+'-reviewed-sources.json')).write_text(json.dumps(current,indent=2)+'\n');(base/(prefix+'-review.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
