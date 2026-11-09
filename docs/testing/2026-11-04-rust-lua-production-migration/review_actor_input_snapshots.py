"""Check the snapshot boundary and preserve independent native controls."""
import base64,hashlib,json,subprocess,tomllib
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2];prefix='crates/deadsync-song-lua/'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((d/'actor-input-snapshots-before-sources.json').read_text())
saved=json.loads((d/'actor-input-snapshots-edit-baseline.json').read_text())
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/name for name in names if name and Path(name).suffix in {'.rs','.lua','.toml'}}
paths.update(r/name for name in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
changed={p for p in before if p in current and before[p]!=current[p]}
added=current.keys()-before.keys()
assert not before.keys()-current.keys()
assert changed==saved.keys(),changed
assert added=={prefix+name for name in ['src/rust_engine/actors/input_snapshot.rs',
    'src/rust_engine/actors/input_snapshot/tables.rs','src/rust_engine/actors/input_snapshot/update.rs',
    'src/rust_engine/actors/lifecycle/snapshot.rs','src/rust_engine/live_scene/interactions.rs',
    'tests/rust_actor_inputs.rs','tests/rust_actor_input_contract.rs']},added
for p,data in saved.items():assert hashlib.sha256(base64.b64decode(data['base64'])).hexdigest()==data['sha256']==before[p]
assert all(current[p]==before[p] for p in before if p.startswith(('crates/deadlib-lua/',prefix+'tests/',prefix+'src/rust_engine/input/program/')))
fmt=lambda text:subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=text.encode()).decode()
native=(r/prefix/'src/actor_input.rs').read_text()
start=native.index('pub struct SongLuaCompiledActorInputs');end=native.index('pub(crate) fn compile(',start)
expected=native[start:end].replace('SongLuaCompiledActorInputs','CompiledActorInputs').replace('crate::input::compiled::Frame','super::super::input::Frame')
source=(r/prefix/'src/rust_engine/actors/input_snapshot.rs').read_text()
actual=source[source.index('pub struct CompiledActorInputs'):source.index('fn raw(')]
actual=actual[:actual.rfind('\n}')+2]
assert fmt(expected)==fmt(actual),'input execution owner differs from existing native owner'
native=(r/prefix/'src/actor_input/update.rs').read_text()
start=native.index('struct Node {');end=native.index('pub(crate) fn compile(',start)
expected=native[start:end].replace('SongLuaCompiledActorInputs','CompiledActorInputs').replace('SongLuaCompiledActorInteractions','CompiledActorInteractions')
source=(r/prefix/'src/rust_engine/actors/input_snapshot/update.rs').read_text()
actual=source[source.index('struct Node {'):source.index('fn optional_number(')]
actual=actual[:actual.rfind('\n}')+2]
assert fmt(expected)==fmt(actual),'combined execution owner differs from existing native owner'
original=base64.b64decode(saved[prefix+'src/rust_engine/actors/lifecycle.rs']['base64']).decode().replace('\r\n','\n')
source=(r/prefix/'src/rust_engine/actors/lifecycle.rs').read_text()
start=original.index('fn advance_node(');end=original.index('/// Even an actor paused',start)
first=source.index('fn advance_node(');last=source.index('/// Even an actor paused',first)
assert fmt(original[start:end])==fmt(source[first:last]),'existing actor update algorithm changed'
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in sorted(changed|added) if p.endswith('.rs')]],check=True,capture_output=True)
assert '#[ignore]' not in (r/prefix/'tests/rust_actor_inputs.rs').read_text()
native=(r/prefix/'tests/actor_input.rs').read_text()
expected=native.replace('use deadsync_song_lua::*;', '''use deadsync_song_lua::*;
use deadsync_song_lua::rust_engine::{SongLiveScene as SongLuaLiveScene,
    CompiledActorInputs as SongLuaCompiledActorInputs,
    CompiledActorInteractions as SongLuaCompiledActorInteractions};''',1)
assert expected.count('compile_song_lua_layers_with_default_host(')==1
expected=expected.replace('compile_song_lua_layers_with_default_host(',
                         'deadsync_song_lua::rust_engine::compile_song_lua_layers(',1)
assert fmt(expected)==(r/prefix/'tests/rust_actor_input_contract.rs').read_text(), 'native contract assertions changed'
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
print(json.dumps({'sources':len(current),'changed':sorted(changed),'added':sorted(added),
                  'interpreter_unchanged':True,'existing_tests_unchanged':True,
                  'runtime_owners_match_native':True,'existing_actor_update_unchanged':True}),flush=True)
