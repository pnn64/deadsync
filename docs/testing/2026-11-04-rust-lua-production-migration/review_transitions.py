"""Audit transition progress, explicitly preserving its failing preparation gate."""
from pathlib import Path
import hashlib,json,re,subprocess,tomllib
base=Path(__file__).resolve().parent
root=base.parents[2]
read=lambda p:json.loads(p.read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
td=base/'transitions-adapters-song-linux-debug'
fd=base/'transitions-focused-v5-song-linux-debug'
ld=base/'transitions-lint-song-linux-debug'
tests,focus,lint=[read(p/'result.json') for p in [td,fd,ld]]
assert tests['exit_code']==focus['exit_code']==101
assert (focus['tests_passed'],focus['tests_failed'])==(11,1)
assert (tests['tests_passed'],tests['tests_failed'])==(438+11,1)
assert '--no-fail-fast' in tests['command']
for directory,result in [(td,tests),(fd,focus),(ld,lint)]:
    assert sha(directory/'run.log')==result['log_sha256'] and result['sources_unchanged']
for directory in [td,fd]:
    log=(directory/'run.log').read_text()
    failed=re.findall(r'failures:\n\nfailures:\n(.*?)\n\ntest result:',log,re.S)
    assert len(failed)==1 and failed[0].strip()=='preparation_constructors_keep_captured_queue_effects_and_shared_state'
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={root.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=root).decode().split('\0')
paths={root/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(root/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(root).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
assert current==read(td/'sources.json')==read(fd/'sources.json')==read(ld/'sources.json')
before=read(base/'steps-display-final-adapters-song-linux-debug/sources.json')
previous=read(base/'steps-display-final-review.json')
delta={'added':sorted(current.keys()-before.keys()),'removed':sorted(before.keys()-current.keys()),'changed':sorted(k for k in current.keys()&before.keys() if current[k]!=before[k]),'unchanged':sum(current[k]==before[k] for k in current.keys()&before.keys())}
assert delta['added']==['crates/deadsync-song-lua/'+p for p in ['src/rust_engine/actors/gameplay_bitmaps.rs','src/rust_engine/actors/transitions.rs','src/rust_engine/managers/utility/colors.rs','tests/rust_transitions.rs']]
assert delta['removed']==[]
assert delta['changed']==['crates/deadsync-song-lua/'+p for p in ['Cargo.toml','src/rust_engine/actors.rs','src/rust_engine/actors/lifecycle.rs','src/rust_engine/actors/native_children.rs','src/rust_engine/actors/render.rs','src/rust_engine/actors/screen_parts.rs','src/rust_engine/actors/steps_displays.rs','src/rust_engine/managers/utility.rs','src/rust_engine/test_support.rs']]
for k,v in read(base/'transitions-edit-baseline.json').items():
    assert hashlib.sha256(v['source'].encode()).hexdigest()==v['sha256']
    if k in before:assert v['sha256']==before[k]
targets=sorted(t['name'] for t in tomllib.loads((root/'crates/deadsync-song-lua/Cargo.toml').read_text())['test'] if t['name'].startswith('rust_'))
assert targets==sorted(tests['command'][i+1] for i,a in enumerate(tests['command']) if a=='--test')
assert len(tests['test_results'])==len(targets)==45
core={k:v for k,v in read(base/'actor-meshes-final-core-core-linux-debug/sources.json').items() if k.startswith('crates/deadlib-lua/') or k in ['Cargo.toml','Cargo.lock','.cargo/config.toml']}
assert all(current[k]==v for k,v in core.items())
native=sorted(set(previous['native_controls_unchanged'])|{'crates/deadsync-song-lua/src/lua_util.rs'})
assert all(current[k]==before[k] for k in native)
fixtures={k:sha(root/k) for k in previous['fixtures']};assert fixtures==previous['fixtures']
formatting=read(base/'transitions-format.json');assert formatting['exit_code']==0
assert all(current[k]==v for k,v in formatting['files'].items())
control=(root/'crates/deadsync-song-lua/src/lua_util.rs').read_text()
port=(root/'crates/deadsync-song-lua/src/rust_engine/actors/transitions.rs').read_text()
a=control.index('pub fn create_top_screen_theme_actor(');b=control.index('\nfn create_native_song_background',a)
blocks=re.findall(r'lua.load\(r#"(.*?)"#\).set_name\("([^"]+)"\)',control[a:b],re.S)
assert len(blocks)==3 and all(body in port for body,_ in blocks)
bitmap_pattern=r'key:\s*"([^"]+)",\s*filename:\s*"([^"]+)",\s*bytes:\s*include_bytes!\(\s*"(?:\.\./)+assets/([^"]+)"\s*\)'
bitmaps=re.findall(bitmap_pattern,(root/'crates/deadsync-song-lua/src/rust_engine/actors/gameplay_bitmaps.rs').read_text())
assert len(bitmaps)==57 and bitmaps==re.findall(bitmap_pattern,control)
a=control.index('pub fn create_color_constants_table');b=control.index('    ] {',a)
colors=re.findall(r'\("([^"]+)", "([^"]+)"\)',control[a:b])
assert len(colors)==21
assert colors==re.findall(r'\("([^"]+)", "([^"]+)"\)',(root/'crates/deadsync-song-lua/src/rust_engine/managers/utility/colors.rs').read_text())
def diagnostics(path):
    return [d for d in re.findall(r'(?m)^(warning|error): ([^\n]+)\n[ \t]+--> ([^:\n]+):',path.read_text()) if d[2].startswith('crates/deadsync-song-lua/')]
old_diagnostics=diagnostics(base/'steps-display-final-lint-song-linux-debug/run.log')
new_diagnostics=diagnostics(ld/'run.log')
assert lint['exit_code']==101
assert new_diagnostics==old_diagnostics
report={'goal_complete':False,'stage_complete':False,'verification':'verified incomplete; preparation regression remains failing',
 'scope':'In/Out/Cancel construction and lifecycle; geometry and existing command helpers',
 'tests_passed':tests['tests_passed'],'tests_failed':tests['tests_failed'],'rust_adapter_targets':targets,
 'known_failure':'preparation_constructors_keep_captured_queue_effects_and_shared_state',
 'source_count':len(current),'sources_match_focus_tests_and_lint':True,'delta':delta,
 'test_log_sha256':tests['log_sha256'],'focused_log_sha256':focus['log_sha256'],'lint_log_sha256':lint['log_sha256'],
 'lint_diagnostic_messages_and_files_unchanged':True,'core_and_root_entries_unchanged':len(core),
 'native_controls_unchanged':native,'fixtures':fixtures,'formatted_rust_files':len(formatting['files']),
 'native_lua_util_sha256':sha(root/'crates/deadsync-song-lua/src/lua_util.rs'),
 'verbatim_command_blocks':len(blocks),'bitmap_records':len(bitmaps),'color_constants':len(colors),
 'remaining':'Constructor preparation capture; unnamed transition roles/theme trees/full screen; compiler capture and existing baking/live orchestration; unconditional cutover; final application/rendering/dependency/fixture/performance checks.'}
(base/'transitions-review.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k not in ['fixtures','native_controls_unchanged','rust_adapter_targets']}))
