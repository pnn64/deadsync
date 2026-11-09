"""Run every existing snapshot contract assertion against the Rust scene too."""
import hashlib,json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
source=r/'crates/deadsync-song-lua/tests/actor_input.rs'
native=source.read_text()
text=native.replace('use deadsync_song_lua::*;', '''use deadsync_song_lua::*;
use deadsync_song_lua::rust_engine::{SongLiveScene as SongLuaLiveScene,
    CompiledActorInputs as SongLuaCompiledActorInputs,
    CompiledActorInteractions as SongLuaCompiledActorInteractions};''',1)
assert text.count('compile_song_lua_layers_with_default_host(')==1
text=text.replace('compile_song_lua_layers_with_default_host(',
                  'deadsync_song_lua::rust_engine::compile_song_lua_layers(',1)
target=source.with_name('rust_actor_input_contract.rs');assert not target.exists()
target.write_text(text)
out=d/'actor-input-snapshot-contract-origin.json';assert not out.exists()
out.write_text(json.dumps({'native_source':source.relative_to(r).as_posix(),
    'native_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
    'mirror':target.relative_to(r).as_posix(),
    'changes':['Only scene/owner type aliases and compiler selection. All native assertions and fixture bodies are preserved.']},indent=2)+'\n')
print('Generated the complete existing snapshot contract for the Rust backend without changing assertions.')
