"""Audit generic payload changes, unchanged algorithms and final-source checks."""
import ast,base64,difflib,hashlib,json,re,subprocess,sys,tomllib
from pathlib import Path
d=Path(__file__).parent;r=d.resolve().parents[2]
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=read(d/'song-live-compiler-before-sources.json');saved=read(d/'song-live-compiler-edit-baseline.json');saved.update(read(d/'song-live-compiler-epoch-edit-baseline.json'))
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}};paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed=sorted(p for p in current.keys()&before.keys() if current[p]!=before[p]);added=sorted(current.keys()-before.keys())
assert not before.keys()-current.keys();assert set(changed)==saved.keys(),changed
assert added==sorted('crates/deadsync-song-lua/'+p for p in ['src/rust_engine/compiler/live.rs','src/rust_engine/actors/live_epoch.rs','tests/rust_live_compiler.rs']),added
for p,x in saved.items():assert hashlib.sha256(base64.b64decode(x['base64'])).hexdigest()==x['sha256']==before[p]
assert all(current[p]==before[p] for p in before if p.startswith(('crates/deadlib-lua/','crates/deadsync-song-lua/tests/')))
old=lambda p:base64.b64decode(saved['crates/deadsync-song-lua/'+p]['base64']).decode().replace('\r\n','\n')
source=lambda p:(r/'crates/deadsync-song-lua'/p).read_text()
module=ast.parse((d/'refactor_live_compiler_types.py').read_text());scope={'re':re}
exec(compile(ast.Module(body=[n for n in module.body if isinstance(n,ast.FunctionDef) and n.name in {'lib','native','rust'}],type_ignores=[]),'<pure type transformations>','exec'),scope)
fmt=lambda text:subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=text.encode()).decode()
expected=scope['lib'](old('src/lib.rs'))
for start,end in [('pub type CompiledSongLua<OverlayActor>','impl<OverlayActor, LiveScene> CompiledSongLuaData<OverlayActor, LiveScene> {'),('pub fn sort_compiled_song_lua<OverlayActor, LiveScene>(', '\npub fn runtime_static_overlay_index_by_path')]:
 a=expected.index(start);b=expected.index(end,a);expected=expected[:a]+fmt(expected[a:b])+'\n'+expected[b:]
assert expected==source('src/lib.rs'),'shared payload changed beyond generic owner and equivalent Debug'
assert scope['native'](old('src/compile.rs'))==source('src/compile.rs'),'native compile algorithms changed'
expected=scope['rust'](old('src/rust_engine/compiler.rs')).replace('CompiledSongLua, ','',1)
assert fmt(expected)==source('src/rust_engine/compiler.rs'),'finite compiler changed beyond type generalization and wrapper'
p='Cargo.toml';a=tomllib.loads(old(p));b=tomllib.loads(source(p))
assert b.pop('test')==a.pop('test')+[dict(name='rust_live_compiler',**{'required-features':['rust-engine','test-support']})];assert a==b
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in changed+added if p.endswith('.rs') and p not in ['crates/deadsync-song-lua/src/lib.rs','crates/deadsync-song-lua/src/compile.rs']]],check=True,capture_output=True)
subprocess.run([sys.executable,str(d/'format_live_compiler_shared.py'),'--check'],check=True,capture_output=True)
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
tests=source('tests/rust_live_compiler.rs');assert '#[ignore]' not in tests and '.eval()' not in tests
report=dict(goal_complete=False,production_cutover=False,final_application_validation=False,previous_goal_turn='progress: retained Rust live-scene coordination; 561 tests',current_goal_turn='progress: unified Rust compilation and retained-VM live reload',source_count=len(current),changed=changed,added=added,existing_tests_byte_unchanged=True,engine_unchanged=True,native_algorithms_unchanged=True,finite_compiler_algorithms_unchanged=True,shared_payload_fields_and_debug_preserved=True,formatting_passed=True,live_compiler_cases=len(re.findall(r'(?m)^#\[test\]',tests)))
if sys.argv[1:]==['--preflight']:print(json.dumps(report));sys.exit(0)
assert len(sys.argv[1:])==4
reg,focus,lint,lib=sys.argv[1:];results={}
for stage in [reg,focus,lint,lib]:
 x=read(d/stage/'result.json');assert x['sources_unchanged'] and read(d/stage/'sources.json')==current
 assert sha(d/stage/'run.log')==x['log_sha256'];results[stage]=x
for stage in [reg,focus,lib]:
 x=results[stage];assert x['exit_code']==0 and x['tests_failed']==0 and x['tests_passed']>0
 if stage!=lib:assert all('0 failed; 0 ignored; 0 measured; 0 filtered out;' in s for s in x['test_results'])
module=ast.parse((d/'run_live_compiler_regressions.py').read_text());targets=next(ast.literal_eval(n.value) for n in module.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='targets' for t in n.targets))
cmd=results[reg]['command'];assert {cmd[i+1] for i,v in enumerate(cmd) if v=='--test'}==set(targets);assert len(results[reg]['test_results'])==len(targets)
assert results[focus]['tests_passed']>=report['live_compiler_cases']
# Existing diagnostics move with shared declarations. Compare lint identity and
# unchanged source locations after mapping line shifts back to the baseline.
line_maps={}
for p in saved:
 old_lines=base64.b64decode(saved[p]['base64']).decode().splitlines();new_lines=(r/p).read_text().splitlines()
 line_maps[p]={j+k+1:i+k+1 for i,j,n in difflib.SequenceMatcher(None,old_lines,new_lines,autojunk=False).get_matching_blocks() for k in range(n)}
def diagnostics(text,remap):
 out=[]
 for block in re.split(r'(?m)(?=^(?:warning|error)(?:\[.*?\])?:)',text):
  message=block.splitlines()[0] if block else ''
  loc=re.search(r'-->\s+([^\s:]+):(\d+):(\d+)',block)
  if not loc:continue
  p,line,col=loc.group(1),int(loc.group(2)),int(loc.group(3))
  if remap:line=line_maps.get(p,{}).get(line,line if p not in line_maps else -line)
  out.append((message,p,line,col,tuple(re.findall(r'index.html#([a-z_]+)',block))))
 return sorted(out)
baseline=diagnostics((d/'song-live-scene-lint-v2-song-linux-debug/run.log').read_text(),False)
actual=diagnostics((d/lint/'run.log').read_text(),True)
assert results[lint]['exit_code']==101
assert actual==baseline,{'added':[x for x in actual if x not in baseline],'removed':[x for x in baseline if x not in actual]}
cfg=read(d/'song-live-compiler-shipping-cfg/result.json');assert cfg['exit_code']==0 and cfg['sources_unchanged'] and cfg['sources']==current
report.update(tests_passed=results[reg]['tests_passed'],test_targets=len(targets),library_compile_tests=results[lib]['tests_passed'],focused_tests=results[focus]['tests_passed'],clippy_diagnostics_match_prior_baseline=True,without_test_support_compiles=True,results=results)
(d/'song-live-compiler-reviewed-sources.json').write_text(json.dumps(current,indent=2)+'\n');(d/'song-live-compiler-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='results'}))
