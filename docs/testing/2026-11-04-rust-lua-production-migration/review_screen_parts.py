"""Audit generated screen display constructors against the final adapter sources."""
from pathlib import Path
import hashlib, json, re, subprocess, tomllib
base = Path(__file__).resolve().parent
root = base.parents[2]
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
td, ld = base / 'screen-parts-verified-adapters-song-linux-debug', base / 'screen-parts-verified-lint-song-linux-debug'
tests, lint = read(td / 'result.json'), read(ld / 'result.json')
assert tests['exit_code'] == 0 and tests['tests_failed'] == 0
assert tests['sources_unchanged'] and lint['sources_unchanged']
for directory, result in [(td, tests), (ld, lint)]:
    assert sha(directory / 'run.log') == result['log_sha256']
crates = ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names = subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths = {root / n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root / n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current = {p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
assert current == read(td / 'sources.json') == read(ld / 'sources.json')
previous = read(base / 'player-parts-final-review.json')
before = read(base / 'player-parts-adapters-song-linux-debug/sources.json')
# This prior concurrent edit is already recorded in the preceding stage audit.
for change in previous['changed_non_dependency_during_tests']:
    assert before[change['path']] == change['before']
    before[change['path']] = change['after']
delta = {'added':sorted(current.keys()-before.keys()),'removed':sorted(before.keys()-current.keys()),
         'changed':sorted(k for k in current.keys()&before.keys() if current[k]!=before[k]),
         'unchanged':sum(current[k]==before[k] for k in current.keys()&before.keys())}
assert delta['added'] == ['crates/deadsync-song-lua/src/rust_engine/actors/screen_parts.rs','crates/deadsync-song-lua/tests/rust_screen_parts.rs']
assert delta['removed'] == []
assert delta['changed'] == ['crates/deadsync-song-lua/Cargo.toml','crates/deadsync-song-lua/src/compile.rs','crates/deadsync-song-lua/src/rust_engine/actors.lua','crates/deadsync-song-lua/src/rust_engine/actors.rs','crates/deadsync-song-lua/src/rust_engine/actors/native_children.lua','crates/deadsync-song-lua/src/rust_engine/actors/textures.rs','crates/deadsync-song-lua/src/rust_engine/test_support.rs']
targets = sorted(t['name'] for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets == sorted(tests['command'][i+1] for i,a in enumerate(tests['command']) if a=='--test')
assert len(tests['test_results']) == len(targets)
core = {k:v for k,v in read(base/'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml','Cargo.lock','.cargo/config.toml']}
assert all(current[k]==v for k,v in core.items())
native = sorted(set(previous['native_controls_unchanged']+['crates/deadsync-song-lua/src/lua_util.rs']))
assert all(current[k]==before[k] for k in native)
fixtures = {k:sha(root/k) for k in previous['fixtures']};assert fixtures==previous['fixtures']
formatting = read(base/'screen-parts-texture-format.json');assert formatting['exit_code']==0
assert all(current[k]==v for k,v in formatting['files'].items())
probe_dir=base/'screen-parts-texture-probe-song-linux-debug';probe=read(probe_dir/'result.json')
assert probe['exit_code']==101 and probe['tests_failed']==2 and probe['sources_unchanged']
assert sha(probe_dir/'run.log')==probe['log_sha256']
fix_dir=base/'screen-parts-texture-fix-song-linux-debug';fix=read(fix_dir/'result.json')
assert fix['exit_code']==0 and fix['tests_passed']==7 and fix['tests_failed']==0 and fix['sources_unchanged']
assert current==read(fix_dir/'sources.json') and sha(fix_dir/'run.log')==fix['log_sha256']
compile_name='crates/deadsync-song-lua/src/compile.rs'
old=read(base/'screen-parts-edit-baseline.json')[compile_name]['source'].encode()
new=(root/compile_name).read_bytes();assert new.startswith(old)
assert hashlib.sha256(old).hexdigest()==before[compile_name]
append=new[len(old):].decode()
assert append.startswith('\n#[cfg(all(feature = "rust-engine", feature = "test-support"))]')
assert subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=append.lstrip(),text=True,capture_output=True,check=True).stdout==append.lstrip()
control=(root/'crates/deadsync-song-lua/src/actor_classes.rs').read_text()
i=control.index('function Actor:LyricCommand(side)');j=control.index('\n"##,',i)
lyric=control[i:j]
assert (root/'crates/deadsync-song-lua/src/rust_engine/actors.lua').read_text().count(lyric)==1

def diagnostics(path):
    data=path.read_bytes()
    match=re.search(rb'(?m)^(?:warning|error):[^\n]*\n[ \t]+--> crates/deadsync-song-lua/',data)
    assert match;return data[match.start():]
assert lint['exit_code']==101
assert diagnostics(base/'player-parts-lint-song-linux-debug/run.log')==diagnostics(ld/'run.log')
report={'goal_complete':False,'scope':'Generated timer, score, lyrics, background and simple screen displays',
        'watched_sources':len(current),'current_source_keys_and_hashes_match_test_and_lint':True,
        'rust_adapter_targets':targets,'tests_passed':tests['tests_passed'],'tests_failed':tests['tests_failed'],
        'test_log_sha256':tests['log_sha256'],'lint_log_sha256':lint['log_sha256'],'lint_library_diagnostics_unchanged':True,
        'texture_method_regression_probe':{'failed':probe['tests_failed'],'log_sha256':probe['log_sha256']},
        'texture_method_regression_fixed':{'passed':fix['tests_passed'],'log_sha256':fix['log_sha256']},
        'core_and_root_entries_unchanged':len(core),'native_controls_unchanged':native,'fixtures':fixtures,
        'native_compile_prefix_bytes':len(old),'native_test_accessor_bytes':len(new)-len(old),'native_compile_sha256':sha(root/compile_name),
        'lyric_helper_verbatim_sha256':hashlib.sha256(lyric.encode()).hexdigest(),'delta':delta,
        'remaining':'StepsDisplay, transitions and loaded theme trees/full screen assembly; compiler capture, baking/live orchestration, unconditional cutover and final application/dependency/fixture/performance validation'}
(base/'screen-parts-final-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['rust_adapter_targets','native_controls_unchanged','fixtures']}))
