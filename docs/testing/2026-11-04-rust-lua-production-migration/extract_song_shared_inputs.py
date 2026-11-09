"""Move shared DTOs and four pure helpers out of the native bindings unchanged."""
import base64
import hashlib
import json
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
song = r / 'crates/deadsync-song-lua'
out = d / 'song-shared-extraction-edit-baseline.json'
assert not out.exists()
names = ['crates/deadsync-song-lua/' + name for name in [
    'src/input.rs', 'src/actor_input.rs', 'src/lib.rs', 'src/compile.rs',
    'src/legacy.rs', 'src/rust_engine/input.rs', 'src/rust_engine/compiler.rs',
    'src/rust_engine/compiler/live.rs']]
names.append('crates/deadsync-theme-simply-love/src/screens/gameplay.rs')
before = json.loads((d / 'production-song-routing-contract-v1-song-windows-debug/sources.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert all(sha(r / name) == before[name] for name in names)
saved = {name: dict(sha256=sha(r / name), base64=base64.b64encode((r / name).read_bytes()).decode()) for name in names}
out.write_text(json.dumps(saved, indent=2) + '\n')
inventory = {file['file']: file for file in json.loads((d / 'song-native-item-inventory-v1.json').read_text())}
def item(file, kind, name):
    found = [value for value in inventory[file]['items'] if value['kind'] == kind and value['name'] == name]
    assert len(found) == 1, (file, kind, name)
    return found[0]
def extract(file, selected, comment_start=None):
    path = song / 'src' / file
    source = path.read_bytes()
    assert len(source) == inventory[file]['bytes']
    ranges = []
    for kind, name in selected:
        node = item(file, kind, name)
        start, end = node['start'], node['end']
        if comment_start and name in comment_start:
            marker = comment_start[name].encode()
            assert source.count(marker) == 1
            start = source.index(marker)
        body = source[start:end]
        assert name.encode() in body
        ranges.append((start, end, body))
    remaining = source
    for start, end, _ in sorted(ranges, reverse=True):
        remaining = remaining[:start] + remaining[end:]
    return path, remaining, [body for _, _, body in sorted(ranges)]
input_path, input_remaining, input_items = extract('input.rs', [
    ('enum', 'SongLuaInputType'), ('impl', 'SongLuaInputType'),
    ('struct', 'SongLuaInputEvent'), ('const', 'RUNTIME_REQUIRED')])
action_path, action_remaining, action_items = extract('actor_input.rs', [
    ('enum', 'SongLuaActorInputAction'), ('enum', 'SongLuaInputScalar')])
target = song / 'src/input_types.rs'
assert not target.exists()
data = b'//! Engine-independent screen input data and authored actor writes.\n\n' + b'\n\n'.join(input_items + action_items) + b'\n'
assert data.count(b'fn native_name(') == 1
data = data.replace(b'fn native_name(', b'pub(crate) fn native_name(', 1)
target.write_bytes(data)
input_path.write_bytes(input_remaining.replace(b'use mlua::{Function, Lua, Table, Value};',
    b'use mlua::{Function, Lua, Table, Value};\npub use crate::input_types::{SongLuaInputEvent, SongLuaInputType};\npub(crate) use crate::input_types::RUNTIME_REQUIRED;', 1))
action_path.write_bytes(action_remaining.replace(b'pub use update::SongLuaCompiledActorInteractions;',
    b'pub use update::SongLuaCompiledActorInteractions;\npub use crate::input_types::{SongLuaActorInputAction, SongLuaInputScalar};', 1))
compile_path, compile_remaining, helpers = extract('compile.rs', [
    ('fn', 'merge_runtime_messages'), ('fn', 'merge_runtime_messages_linear'),
    ('fn', 'wrapper_overlay_order'), ('fn', 'split_compiled_song_lua')],
    {'wrapper_overlay_order': "// Preserve an actor's original sibling position"})
target = song / 'src/compiled_layers.rs'
assert not target.exists()
header = b'''//! Shared message merging and compiled layer ownership, independent of a VM.
use crate::{CompiledSongLuaData, SongLuaMessageEvent, SongLuaOverlayActor, SongLuaOverlayKind};
use deadlib_present::actors::TextAttribute;
type DefaultCompiledSongLua<S,V,L> = CompiledSongLuaData<SongLuaOverlayActor<SongLuaOverlayKind<S,V,TextAttribute>>,L>;

'''
target.write_bytes(header + b'\n\n'.join(helpers) + b'\n')
compile_path.write_bytes(b'pub(crate) use crate::compiled_layers::{merge_runtime_messages, split_compiled_song_lua};\n' + compile_remaining)
for name in ['src/rust_engine/compiler.rs', 'src/rust_engine/compiler/live.rs']:
    path = song / name
    path.write_text(path.read_text().replace('crate::compile::merge_runtime_messages', 'crate::compiled_layers::merge_runtime_messages').replace('crate::compile::split_compiled_song_lua', 'crate::compiled_layers::split_compiled_song_lua'))
path = song / 'src/rust_engine/input.rs'
text = path.read_text()
assert text.count('crate::input::RUNTIME_REQUIRED') == 1
path.write_text(text.replace('crate::input::RUNTIME_REQUIRED', 'crate::input_types::RUNTIME_REQUIRED'))
print('Extracted six shared input items and four unchanged VM-independent helper bodies.', flush=True)
