"""Verify the StepsDisplay migration against the final saved source set."""
from pathlib import Path
import hashlib, json, re, subprocess, tomllib
base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
td, ld = base/'steps-display-final-adapters-song-linux-debug', base/'steps-display-final-lint-song-linux-debug'
fd, pd = base/'steps-display-focused-v4-song-linux-debug', base/'steps-display-gc-probe-song-linux-debug'
tests, lint, focused, probe = [read(p/'result.json') for p in [td,ld,fd,pd]]
assert tests['exit_code']==focused['exit_code']==0 and tests['tests_failed']==focused['tests_failed']==0
assert focused['tests_passed']==8 and tests['tests_passed']==430+focused['tests_passed']
assert probe['exit_code']==101 and probe['tests_failed']==1 and probe['sources_unchanged']
for directory, result in [(td,tests),(ld,lint),(fd,focused),(pd,probe)]:
    assert sha(directory/'run.log')==result['log_sha256']
    assert result['sources_unchanged']
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
assert current==read(td/'sources.json')==read(ld/'sources.json')==read(fd/'sources.json')
before=read(base/'screen-parts-verified-adapters-song-linux-debug/sources.json')
previous=read(base/'screen-parts-final-review.json')
delta={'added':sorted(current.keys()-before.keys()),'removed':sorted(before.keys()-current.keys()),
       'changed':sorted(k for k in current.keys()&before.keys() if current[k]!=before[k]),
       'unchanged':sum(current[k]==before[k] for k in current.keys()&before.keys())}
assert delta['added']==['crates/deadsync-song-lua/src/rust_engine/actors/steps_displays.rs','crates/deadsync-song-lua/tests/rust_steps_displays.rs']
assert delta['removed']==[]
assert delta['changed']==['crates/deadsync-song-lua/Cargo.toml','crates/deadsync-song-lua/src/lua_util.rs','crates/deadsync-song-lua/src/rust_engine/actors.lua','crates/deadsync-song-lua/src/rust_engine/actors.rs','crates/deadsync-song-lua/src/rust_engine/actors/screen_parts.rs','crates/deadsync-song-lua/src/rust_engine/managers.rs','crates/deadsync-song-lua/src/rust_engine/test_support.rs']
targets=sorted(t['name'] for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets==sorted(tests['command'][i+1] for i,a in enumerate(tests['command']) if a=='--test')
assert len(tests['test_results'])==len(targets)==44
core={k:v for k,v in read(base/'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml','Cargo.lock','.cargo/config.toml']}
assert all(current[k]==v for k,v in core.items())
native=sorted((set(previous['native_controls_unchanged'])-{'crates/deadsync-song-lua/src/lua_util.rs'})|{'crates/deadsync-song-lua/src/compile.rs'})
assert all(current[k]==before[k] for k in native)
fixtures={k:sha(root/k) for k in previous['fixtures']};assert fixtures==previous['fixtures']
formatting=read(base/'steps-display-final-format.json');assert formatting['exit_code']==0
assert all(current[k]==v for k,v in formatting['files'].items())
native_name='crates/deadsync-song-lua/src/lua_util.rs'
old=read(base/'steps-display-edit-baseline.json')[native_name]['source'].encode()
new=(root/native_name).read_bytes();assert new.startswith(old)
assert hashlib.sha256(old).hexdigest()==before[native_name]
append=new[len(old):].decode();assert append.startswith('\n#[cfg(all(feature = "rust-engine", feature = "test-support"))]')
assert subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=append.lstrip(),text=True,capture_output=True,check=True).stdout==append.lstrip()
control=(root/native_name).read_text();i=control.index('return {\n    FrameLoadCommand');j=control.index('\n}\n"#',i)+2
metrics=control[i:j];assert (root/'crates/deadsync-song-lua/src/rust_engine/actors/steps_displays.rs').read_text().count(metrics)==1
def diagnostics(path):
    data=path.read_bytes();m=re.search(rb'(?m)^(?:warning|error):[^\n]*\n[ \t]+--> crates/deadsync-song-lua/',data);assert m;return data[m.start():]
assert lint['exit_code']==101
assert diagnostics(base/'screen-parts-verified-lint-song-linux-debug/run.log')==diagnostics(ld/'run.log')
cancelled=read(base/'steps-display-adapters-song-linux-debug/result.json')
assert cancelled['exit_code']==-15 and cancelled['sources_unchanged']
assert sha(base/'steps-display-adapters-song-linux-debug/run.log')==cancelled['log_sha256']
report={'goal_complete':False,'scope':'StepsDisplay constructor, setters and gameplay metric initialization',
        'watched_sources':len(current),'current_source_keys_and_hashes_match_tests_focus_and_lint':True,
        'rust_adapter_targets':targets,'tests_passed':tests['tests_passed'],'tests_failed':tests['tests_failed'],
        'test_log_sha256':tests['log_sha256'],'focused_log_sha256':focused['log_sha256'],
        'gc_probe_log_sha256':probe['log_sha256'],'lint_log_sha256':lint['log_sha256'],
        'lint_library_diagnostics_unchanged':True,'core_and_root_entries_unchanged':len(core),
        'native_controls_unchanged':native,'fixtures':fixtures,'native_lua_util_prefix_bytes':len(old),
        'native_test_accessor_bytes':len(new)-len(old),'native_lua_util_sha256':sha(root/native_name),
        'metric_commands_verbatim_sha256':hashlib.sha256(metrics.encode()).hexdigest(),'delta':delta,
        'cancelled_incomplete_run':{'exit_code':cancelled['exit_code'],'log_sha256':cancelled['log_sha256']},
        'remaining':'Transition actors and loaded theme trees/full screen construction; compiler capture, existing baking/live orchestration, unconditional cutover and final application/dependency/fixture/performance validation'}
(base/'steps-display-final-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['rust_adapter_targets','native_controls_unchanged','fixtures']}))
