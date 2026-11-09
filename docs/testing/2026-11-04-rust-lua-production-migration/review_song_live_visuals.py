"""Audit the visual publication port and final-source test evidence."""
import base64,difflib,hashlib,json,re,subprocess,sys,tomllib
from pathlib import Path

d=Path(__file__).resolve().parent;r=d.parents[2]
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=read(d/'song-live-visuals-before-sources.json');saved=read(d/'song-live-visuals-edit-baseline.json')
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed=sorted(p for p in before.keys()&current.keys() if before[p]!=current[p]);added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys();assert set(changed)==saved.keys(),changed
assert added==sorted('crates/deadsync-song-lua/'+p for p in ['src/rust_engine/actors/live_visuals.rs','src/rust_engine/live_scene/visuals.rs','tests/rust_live_visuals.rs']),added
for p,x in saved.items():assert hashlib.sha256(base64.b64decode(x['base64'])).hexdigest()==x['sha256']==before[p]
old=lambda p:base64.b64decode(saved['crates/deadsync-song-lua/'+p]['base64']).decode().replace('\r\n','\n')
source=lambda p:(r/'crates/deadsync-song-lua'/p).read_text()
fmt=lambda text:subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=text.encode()).decode()
assert all(current[p]==before[p] for p in before if p.startswith(('crates/deadlib-lua/','crates/deadsync-song-lua/tests/')))
for p in ['src/host.rs','src/live.rs','src/lib.rs','src/compile.rs','src/lua_util.rs','src/text_publication.rs','src/rust_engine/arrows.rs','src/rust_engine/arrows/math.rs']:
 assert current['crates/deadsync-song-lua/'+p]==before['crates/deadsync-song-lua/'+p]
a=tomllib.loads(old('Cargo.toml'));b=tomllib.loads(source('Cargo.toml'));tests=b.pop('test');prior=a.pop('test');assert a==b
assert tests==prior[:2]+[dict(name='rust_live_visuals',**{'required-features':['rust-engine','test-support']})]+prior[2:]
assert fmt(old('src/rust_engine/actors.rs').replace('mod live_sources;\n','mod live_sources;\nmod live_visuals;\n',1))==source('src/rust_engine/actors.rs')
scene=old('src/rust_engine/live_scene.rs').replace('mod gameplay;\n','mod gameplay;\nmod visuals;\n',1)
scene=scene.replace('    tracked_states: Vec<SongLuaOverlayState>,\n','    tracked_states: Vec<SongLuaOverlayState>,\n    targets: Vec<(usize, crate::SongLuaLiveTarget)>,\n    visual_targets: rustc_hash::FxHashMap<usize, crate::SongLuaLiveTarget>,\n',1)
scene=scene.replace('            tracked_states: Vec::new(),\n','            tracked_states: Vec::new(),\n            targets: Vec::new(),\n            visual_targets: Default::default(),\n',1)
assert fmt(scene)==source('src/rust_engine/live_scene.rs')

def replace_once(text,before,after):
 assert text.count(before)==1,(before,text.count(before));return text.replace(before,after,1)
text=old('src/rust_engine/actors/text.rs');start=text.index('pub(super) fn attributes(');end=text.index('pub(super) fn stroke(',start)
body=text[start:end]
body=replace_once(body,'pub(super) fn attributes(','pub(super) fn visit_attributes(')
body=replace_once(body,'    actor: TableId,\n) -> Result<Arc<[TextAttribute]>, Error> {','    actor: TableId,\n    mut visit: impl FnMut(TextAttribute),\n) -> Result<(), Error> {')
body=replace_once(body,'return Ok(Arc::from([]));','return Ok(());')
body=replace_once(body,'        let mut out = Vec::new();\n','')
body=replace_once(body,'out.push(TextAttribute {','visit(TextAttribute {')
body=replace_once(body,'Ok(out.into())','Ok(())')
wrapper='''pub(super) fn attributes(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Arc<[TextAttribute]>, Error> {
    let mut out = Vec::new();
    visit_attributes(call, actor, |attribute| out.push(attribute))?;
    Ok(out.into())
}

'''
assert fmt(text[:start]+wrapper+body+text[end:])==source('src/rust_engine/actors/text.rs')
text=old('src/rust_engine/actors/sprites.rs');start=text.index('pub(super) fn states(');end=text.index('fn capture(',start)
body=text[start:end]
body=replace_once(body,'pub(super) fn states(','pub(super) fn visit_states(')
body=replace_once(body,'    actor: TableId,\n) -> Result<Vec<crate::SongLuaSpriteState>, Error> {','    actor: TableId,\n    mut visit: impl FnMut(crate::SongLuaSpriteState),\n) -> Result<(), Error> {')
body=replace_once(body,'return Ok(Vec::new());','return Ok(());')
body=replace_once(body,'    let mut result = Vec::new();\n','')
assert body.count('result.push(crate::SongLuaSpriteState { frame, delay });')==2
body=body.replace('result.push(crate::SongLuaSpriteState { frame, delay });','visit(crate::SongLuaSpriteState { frame, delay });')
body=replace_once(body,'    Ok(result)\n','    Ok(())\n')
wrapper='''pub(super) fn states(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Vec<crate::SongLuaSpriteState>, Error> {
    let mut result = Vec::new();
    visit_states(call, actor, |state| result.push(state))?;
    Ok(result)
}

'''
assert fmt(text[:start]+wrapper+body+text[end:])==source('src/rust_engine/actors/sprites.rs')
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in changed+added if p.endswith('.rs')]],check=True,capture_output=True)
for p,h in read(d/'song-arrows-reference-inputs.json').items():assert sha(r/p)==h,p
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
tests=source('tests/rust_live_visuals.rs');assert '#[ignore]' not in tests and '.eval()' not in tests
pre=read(d/'song-live-visuals-before-reentry-sources.json')
assert pre.keys()==current.keys()
assert sorted(p for p in current if pre[p]!=current[p])==['crates/deadsync-song-lua/src/rust_engine/actors/live_visuals.rs','crates/deadsync-song-lua/tests/rust_live_visuals.rs']
assert sha(d/'song-live-visuals-before-reentry-actor.rs.txt')==pre['crates/deadsync-song-lua/src/rust_engine/actors/live_visuals.rs']
assert sha(d/'song-live-visuals-before-reentry-tests.rs.txt')==pre['crates/deadsync-song-lua/tests/rust_live_visuals.rs']
assert tests.startswith((d/'song-live-visuals-before-reentry-tests.rs.txt').read_text().rstrip()+'\n\n')
assert 'fn single_use_publication_callbacks_reject_recursive_and_saved_lua_calls()' in tests
previous_review=read(d/'song-live-visuals-before-reentry-review.json')
assert previous_review['tests_passed']==683 and previous_review['visual_cases']==9
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,previous_goal_turn='progress: ArrowEffects, post-health cache and ownership; 658 tests across 38 targets',current_goal_turn='progress: live visual/content publication',source_count=len(current),changed=changed,added=added,engine_unchanged=True,native_algorithms_byte_unchanged=True,existing_tests_byte_unchanged=True,reader_refactors_verified=True,formatting_passed=True,visual_cases=len(re.findall(r'(?m)^#\[test\]',tests)))
if sys.argv[1:]==['--preflight']:print(json.dumps(report));sys.exit(0)
assert len(sys.argv[1:])==4
reg,focus,lint,lib=sys.argv[1:];results={}
for stage in [reg,focus,lint,lib]:
 x=read(d/stage/'result.json');assert x['sources_unchanged'] and read(d/stage/'sources.json')==current
 assert sha(d/stage/'run.log')==x['log_sha256'];results[stage]=x
for stage in [reg,focus,lib]:
 x=results[stage];assert x['exit_code']==0 and x['tests_failed']==0 and x['tests_passed']>0
 if stage!=lib:assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in s for s in x['test_results'])
prior=read(d/'song-arrows-review.json')['results']['song-arrows-regressions-v3-song-linux-debug']['command']
targets={'rust_live_visuals','rust_meshes','rust_proxies',*[prior[i+1] for i,v in enumerate(prior) if v=='--test']}
cmd=results[reg]['command'];assert {cmd[i+1] for i,v in enumerate(cmd) if v=='--test'}==targets
assert len(results[reg]['test_results'])==len(targets)==41
assert results[focus]['tests_passed']>=report['visual_cases']
assert '--lib' in results[lib]['command'] and 'publication' in results[lib]['command']
line_maps={}
for p in saved:
 old_lines=base64.b64decode(saved[p]['base64']).decode().splitlines();new_lines=(r/p).read_text().splitlines()
 line_maps[p]={j+k+1:i+k+1 for i,j,n in difflib.SequenceMatcher(None,old_lines,new_lines,autojunk=False).get_matching_blocks() for k in range(n)}
def diagnostics(text,remap):
 out=[]
 for block in re.split(r'(?m)(?=^(?:warning|error)(?:\[.*?\])?:)',text):
  message=block.splitlines()[0] if block else '';loc=re.search(r'-->\s+([^\s:]+):(\d+):(\d+)',block)
  if not loc:continue
  p,line,col=loc.group(1),int(loc.group(2)),int(loc.group(3))
  if remap:line=line_maps.get(p,{}).get(line,line if p not in line_maps else -line)
  out.append((message,p,line,col,tuple(re.findall(r'index.html#([a-z_]+)',block))))
 return sorted(out)
baseline=diagnostics((d/'song-arrows-lint-v5-song-linux-debug/run.log').read_text(),False)
actual=diagnostics((d/lint/'run.log').read_text(),True);assert results[lint]['exit_code']==101
assert actual==baseline,{'added':[x for x in actual if x not in baseline],'removed':[x for x in baseline if x not in actual]}
cfg=read(d/'song-live-visuals-shipping-cfg-v2/result.json');assert cfg['exit_code']==0 and cfg['sources_unchanged'] and cfg['sources']==current
report.update(tests_passed=results[reg]['tests_passed'],test_targets=len(targets),library_tests=results[lib]['tests_passed'],focused_tests=results[focus]['tests_passed'],clippy_diagnostics_match_prior_baseline=True,without_test_support_compiles=True,reentry_guard_verified=True,pre_reentry_regression_count=683,results=results)
(d/'song-live-visuals-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n')
(d/'song-live-visuals-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
