"""Verify extraction, preserved host code, tests and final-source diagnostics."""
from pathlib import Path
from collections import Counter
import base64, difflib, hashlib, json, re, subprocess, sys, tomllib
base=Path(__file__).resolve().parent
root=base.parents[2]
prefix='song-frame-capture'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
saved=read(base/(prefix+'-edit-baseline.json'))
old={p:base64.b64decode(v['base64']) for p,v in saved.items()}
for p,data in old.items():assert hashlib.sha256(data).hexdigest()==saved[p]['sha256']
fmt=lambda s:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=s,text=True,capture_output=True,check=True).stdout
canonical=lambda s:'\n'.join(line for line in fmt(s).splitlines() if line.strip())
subprocess.run([sys.executable,str(base/'extract_frame_capture.py'),'--check'],cwd=root,check=True,capture_output=True)
proof=read(base/(prefix+'-extraction-proof.json'))
for p,h in proof['checked'].items():assert sha(root/p)==h,p
def remove_fn(text,marker):
    start=text.index(marker);opening=text.index('{',start);depth=1;end=opening+1
    while depth:
        depth+=(text[end]=='{')-(text[end]=='}');end+=1
    return text[:start]+text[end:]
runtime='crates/deadsync-song-lua/src/rust_engine.rs'
body=(root/runtime).read_text().replace('mod song_frame_capture;\n','')
body=re.sub(r'pub use song_frame_capture::\{.*?\};\n','',body,flags=re.S)
assert canonical(body)==canonical(old[runtime].decode())
access='crates/deadsync-song-lua/src/rust_engine/actors/perframes.rs'
assert canonical(remove_fn((root/access).read_text(),'    pub fn is_player('))==canonical(old[access].decode())
buffer='crates/deadsync-song-lua/src/rust_engine/actors/song_immediate.rs'
body=(root/buffer).read_text()
body=remove_fn(body,'    pub(in crate::rust_engine) fn restore_song_overlay_updates(')
# Remove the documentation belonging to the new restore hook as well.
body=body.replace('    /// Metadata classification performs no authored calls. If it fails, retain\n    /// the complete publication batch as the native visit-before-clear reader did.\n','')
body=remove_fn(body,'    pub(in crate::rust_engine) fn chronological_song_capture(')
body=remove_fn(body,'    pub(in crate::rust_engine) fn drain_song_overlay_updates(')
body=body.replace('fn take_song_overlay_frame_updates(','fn drain_song_overlay_updates(')
body=body.replace('        include_empty: bool,\n','').replace('.drain(include_empty)','.drain()')
body=body.replace('fn drain(&mut self, include_empty: bool)','fn drain(&mut self)')
body=body.replace('include_empty\n                    || frame.tween_reset','frame.tween_reset').replace('include_empty || frame.tween_reset','frame.tween_reset')
assert canonical(body)==canonical(old[buffer].decode()),'Existing publication buffer behavior changed'
support='crates/deadsync-song-lua/src/rust_engine/test_support.rs'
assert (root/support).read_text().startswith(old[support].decode().replace('\r\n','\n').rstrip()+'\n')
manifest='crates/deadsync-song-lua/Cargo.toml'
parsed=tomllib.loads((root/manifest).read_text())
assert parsed['test'].pop()=={'name':'rust_song_frame_capture','required-features':['rust-engine','test-support']}
assert parsed==tomllib.loads(old[manifest].decode())
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
before=read(base/(prefix+'-before-sources.json'))
changed=sorted(p for p in current.keys()&before.keys() if current[p]!=before[p])
added=sorted(current.keys()-before.keys())
assert set(changed)==set(p for p in saved if p.startswith('crates/')),changed
assert added==['crates/deadsync-song-lua/src/frame_capture.rs','crates/deadsync-song-lua/src/rust_engine/song_frame_capture.rs','crates/deadsync-song-lua/tests/rust_song_frame_capture.rs'],added
assert not before.keys()-current.keys()
for p in before.keys()&current.keys():
    if p not in changed:assert current[p]==before[p]
assert len(re.findall(r'^#\[test\]',(root/added[-1]).read_text(),re.M))==8
assert '#[ignore' not in (root/added[-1]).read_text()
assert 'mlua' not in (root/added[0]).read_text() and 'lua_util' not in (root/added[0]).read_text()
format_paths=[p for p in changed+added if p.endswith('.rs') and p not in {
    'crates/deadsync-song-lua/src/lib.rs','crates/deadsync-song-lua/src/lua_util.rs','crates/deadsync-song-lua/src/perframe.rs'}]
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*format_paths],cwd=root,check=True,capture_output=True)
if sys.argv[1:]==['--sources-only']:
    print(json.dumps(dict(source_count=len(current),changed=changed,added=added,extraction_verified=True,existing_host_code_preserved=True,formatting_passed=True)));raise SystemExit(0)
reg,lint=sys.argv[1:]
results={s:read(base/s/'result.json') for s in [reg,lint]}
for stage,result in results.items():
    assert sha(base/stage/'run.log')==result['log_sha256']
    assert result['sources_unchanged'] and read(base/stage/'sources.json')==current,stage
assert results[reg]['exit_code']==0 and results[reg]['tests_failed']==0
log=(base/reg/'run.log').read_text()
parts=re.split(r'(?m)^\s*Running (unittests [^\n]+|tests/[^\n]+)',log)
sections={}
for header,body in zip(parts[1::2],parts[2::2]):
    result=re.search(r'test result: ok\. (\d+) passed; (\d+) failed;',body)
    assert result,header
    sections[header]=int(result[1]);assert int(result[2])==0
assert len(sections)==6,sections
library_header=next(h for h in sections if h.startswith('unittests '))
library_count=sections[library_header]
assert library_count>0
assert sum(n for h,n in sections.items() if h!=library_header)==247
assert len(results[reg]['test_results'])==6
assert results[reg]['tests_passed']==247+library_count
assert 'test perframe::tests::scheduled_native_lerp ... ok' in log
assert 'test perframe::tests::completed_update_tweens_persist_into_following_state ... ok' in log
def diagnostics(p):
    entries=re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',p.read_text())
    return Counter((kind,message,path.replace('/frame_capture.rs','/perframe.rs')) for kind,message,path in entries)
assert results[lint]['exit_code']==101
assert diagnostics(base/lint/'run.log')==diagnostics(base/'song-command-locals-lint-v1-song-linux-debug/run.log')
deps=read(base/'song-static-schedule-test-dependencies.json')
graph=subprocess.run(deps['command'],cwd=root,capture_output=True,check=True)
assert hashlib.sha256(graph.stdout).hexdigest()==deps['stdout_sha256']
(base/(prefix+'-current-test-dependencies.stdout')).write_bytes(graph.stdout)
assert sha(base/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(base/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
diff=''
for p in changed+added:
    previous=old[p].decode().replace('\r\n','\n') if p in old else ''
    diff+=''.join(difflib.unified_diff(previous.splitlines(keepends=True),(root/p).read_text().splitlines(keepends=True),fromfile='before/'+p if p in old else '/dev/null',tofile=p))
diff_path=base/(prefix+'-source-diff.diff');diff_path.write_text(diff)
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,
    source_count=len(current),changed=changed,added=added,source_diff_sha256=sha(diff_path),
    saved_algorithms_extraction_verified=True,existing_host_code_preserved=True,
    existing_tests_byte_unchanged=True,engine_sources_unchanged=True,formatting_passed=True,
    clippy_baseline_preserved_with_diagnostics_moved_to_shared_module=True,
    dependency_graph_unchanged=True,adapter_tests_passed=247,adapter_targets=5,
    library_tests_passed=library_count,new_cases=8,test_sections=sections,
    objective_sha256=sha(base/'objective.txt'),results=results)
(base/(prefix+'-reviewed-sources.json')).write_text(json.dumps(current,indent=2)+'\n')
(base/(prefix+'-review.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
