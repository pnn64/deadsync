"""Audit arrow adapter scope, native-expression transfer and final-source evidence."""
import base64,difflib,hashlib,json,re,subprocess,sys,tomllib
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=read(d/'song-arrows-before-sources.json');saved=read(d/'song-arrows-edit-baseline.json')
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed=sorted(p for p in before.keys()&current.keys() if before[p]!=current[p]);added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys();assert set(changed)==saved.keys(),changed
assert added==sorted('crates/deadsync-song-lua/'+p for p in ['src/rust_engine/arrows.rs','src/rust_engine/arrows/math.rs','tests/rust_arrows.rs']),added
for p,x in saved.items():assert hashlib.sha256(base64.b64decode(x['base64'])).hexdigest()==x['sha256']==before[p]
old=lambda p:base64.b64decode(saved['crates/deadsync-song-lua/'+p]['base64']).decode().replace('\r\n','\n')
source=lambda p:(r/'crates/deadsync-song-lua'/p).read_text()
fmt=lambda text:subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=text.encode()).decode()
assert all(current[p]==before[p] for p in before if p.startswith(('crates/deadlib-lua/','crates/deadsync-song-lua/tests/')))
for p in ['src/host.rs','src/live.rs','src/lib.rs','src/compile.rs','src/lua_util.rs']:assert current['crates/deadsync-song-lua/'+p]==before['crates/deadsync-song-lua/'+p]
a=tomllib.loads(old('Cargo.toml'));b=tomllib.loads(source('Cargo.toml'));tests=b.pop('test');prior=a.pop('test');assert a==b
assert tests==prior[:1]+[dict(name='rust_arrows',**{'required-features':['rust-engine','test-support']})]+prior[1:]
shim='''pub fn update_legacy_arrow_effects(lua: &mlua::Lua) -> mlua::Result<()> {
    crate::host::update_native_arrow_effects(lua)
}
pub fn install_legacy_arrow_clock_inputs(lua: &mlua::Lua, context: &crate::SongLuaCompileContext) {
    crate::clock_inputs::install(lua, context);
}
'''
anchor='pub use super::managers::screen::expose_publish as expose_screen_publication;\n'
assert fmt(old('src/rust_engine/test_support.rs').replace(anchor,anchor+shim,1))==source('src/rust_engine/test_support.rs')
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in changed+added if p.endswith('.rs')]],check=True,capture_output=True)
subprocess.run([sys.executable,str(d/'port_song_arrow_math.py'),'--check'],check=True,capture_output=True)
for p,h in read(d/'song-arrows-reference-inputs.json').items():assert sha(r/p)==h,p
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
tests=source('tests/rust_arrows.rs');assert '#[ignore]' not in tests and '.eval()' not in tests
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,previous_goal_turn='progress: same-VM gameplay callbacks; 586 regression and 150 live library tests',current_goal_turn='progress: ArrowEffects bindings and post-health cache',source_count=len(current),changed=changed,added=added,engine_unchanged=True,native_algorithms_byte_unchanged=True,existing_tests_byte_unchanged=True,native_math_expression_transfer_verified=True,formatting_passed=True,arrow_cases=len(re.findall(r'(?m)^#\[test\]',tests)),captured_reference_cases=246)
if sys.argv[1:]==['--preflight']:print(json.dumps(report));sys.exit(0)
assert len(sys.argv[1:])==4
reg,focus,lint,lib=sys.argv[1:];results={}
for stage in [reg,focus,lint,lib]:
 x=read(d/stage/'result.json');assert x['sources_unchanged'] and read(d/stage/'sources.json')==current
 assert sha(d/stage/'run.log')==x['log_sha256'];results[stage]=x
for stage in [reg,focus,lib]:
 x=results[stage];assert x['exit_code']==0 and x['tests_failed']==0 and x['tests_passed']>0
 if stage!=lib:assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in s for s in x['test_results'])
prior=read(d/'song-live-gameplay-review.json')['results']['song-live-gameplay-regressions-v1-song-linux-debug']['command']
owners={'rust_catalog','rust_game_state','rust_player_options','rust_player_state'}
targets={'rust_arrows','rust_clock',*owners,*[prior[i+1] for i,v in enumerate(prior) if v=='--test']}
cmd=results[reg]['command'];assert {cmd[i+1] for i,v in enumerate(cmd) if v=='--test'}==targets;assert len(results[reg]['test_results'])==len(targets)==38
assert results[focus]['tests_passed']>=report['arrow_cases']
focus_command=results[focus]['command']
assert {focus_command[i+1] for i,v in enumerate(focus_command) if v=='--test'}=={'rust_arrows','rust_clock','rust_statistics',*owners}
failed_ownership=read(d/'song-arrows-regressions-v1-song-linux-debug/result.json')
assert failed_ownership['exit_code']==101 and failed_ownership['tests_passed']==606 and failed_ownership['tests_failed']==2 and failed_ownership['sources_unchanged']
assert sha(d/'song-arrows-regressions-v1-song-linux-debug/run.log')==failed_ownership['log_sha256']
pre_fix=read(d/'song-arrows-regressions-v1-song-linux-debug/sources.json')
assert pre_fix.keys()==current.keys()
assert [p for p in current if pre_fix[p]!=current[p]]==['crates/deadsync-song-lua/src/rust_engine/arrows.rs']
assert sha(d/'song-arrows-before-ownership-fix.rs.txt')==pre_fix['crates/deadsync-song-lua/src/rust_engine/arrows.rs']
failed_option_ownership=read(d/'song-arrows-option-ownership-v1-song-linux-debug/result.json')
assert failed_option_ownership['exit_code']==101 and failed_option_ownership['tests_passed']==0 and failed_option_ownership['tests_failed']==1 and failed_option_ownership['sources_unchanged']
assert sha(d/'song-arrows-option-ownership-v1-song-linux-debug/run.log')==failed_option_ownership['log_sha256']
pre_borrow=read(d/'song-arrows-option-ownership-v1-song-linux-debug/sources.json')
assert pre_borrow.keys()==current.keys()
assert [p for p in current if pre_borrow[p]!=current[p]]==['crates/deadsync-song-lua/src/rust_engine/arrows.rs']
assert sha(d/'song-arrows-before-borrowed-catalog-fix.rs.txt')==pre_borrow['crates/deadsync-song-lua/src/rust_engine/arrows.rs']
assert '--lib' in results[lib]['command'] and 'host::tests::arrow_' in results[lib]['command']
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
baseline=diagnostics((d/'song-live-gameplay-lint-v2-song-linux-debug/run.log').read_text(),False)
actual=diagnostics((d/lint/'run.log').read_text(),True);assert results[lint]['exit_code']==101
assert actual==baseline,{'added':[x for x in actual if x not in baseline],'removed':[x for x in baseline if x not in actual]}
cfg=read(d/'song-arrows-shipping-cfg-v3/result.json');assert cfg['exit_code']==0 and cfg['sources_unchanged'] and cfg['sources']==current
report.update(tests_passed=results[reg]['tests_passed'],test_targets=len(targets),library_tests=results[lib]['tests_passed'],focused_tests=results[focus]['tests_passed'],clippy_diagnostics_match_prior_baseline=True,without_test_support_compiles=True,ownership_regression_corrected=True,failed_ownership_run=failed_ownership,failed_option_ownership_run=failed_option_ownership,results=results)
(d/'song-arrows-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n');(d/'song-arrows-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
