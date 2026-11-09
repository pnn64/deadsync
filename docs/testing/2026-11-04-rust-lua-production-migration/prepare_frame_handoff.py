"""Extract the existing getter key map and pose metadata transition exactly."""
from pathlib import Path
import base64, hashlib, json, subprocess, sys
base=Path(__file__).resolve().parent
root=base.parents[2]
saved=json.loads((base/'song-frame-handoff-edit-baseline.json').read_text(encoding='utf-8-sig'))
old=lambda p:base64.b64decode(saved[p]['base64']).decode()
fmt=lambda s:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=s,text=True,capture_output=True,check=True).stdout
check='--apply' not in sys.argv
checked={}
def write(path,text,formatted=False):
    if formatted:text=fmt(text)
    if check:
        actual=(root/path).read_bytes().decode()
        assert (fmt(actual)==fmt(text) if formatted else actual==text),path
        checked[path]=hashlib.sha256(actual.encode()).hexdigest()
    else:
        if (root/path).exists():
            current=(root/path).read_bytes().decode()
            assert current==old(path) or fmt(current)==fmt(text),f'Other changes present: {path}'
        (root/path).write_bytes(text.encode())
native='crates/deadsync-song-lua/src/lua_util.rs'
text=old(native)
start=text.index('    let key = match target {',text.index('pub fn set_actor_overlay_update_getter_value('))
end=text.index('\n    match value {',start)
block=text[start:end].replace('\r\n','\n').rstrip('\r')
assert block.endswith('    };')
keys=block.replace('    let key = match target {','    match target {',1)[:-len('    };')]+'    }\n'
core='crates/deadsync-song-lua/src/frame_capture.rs'
write(core,old(core)+'''\n/// Field routing of the existing preparation getter handoff.
pub(crate) fn overlay_getter_scalar_key(target: SongLuaOverlayUpdateTarget) -> &'static str {
    use SongLuaOverlayUpdateTarget as Target;
'''+keys+'}\n',True)
eol_prefix='\r' if text[end-1]=='\r' else ''
write(native,text[:start]+'    let key = crate::frame_capture::overlay_getter_scalar_key(target);'+eol_prefix+text[end:])
pose='crates/deadsync-song-lua/src/rust_engine/actors/song_pose.rs'
text=old(pose).replace('\r\n','\n')
start=text.index('    {\n        let mut guard = store.lock().unwrap();',text.index('pub(super) fn set_getter('))
end=text.index('    match value {',start)
block=text[start:end]
body=block[len('    {\n'):].removesuffix('    }\n')
body='\n'.join(line[4:] if line.startswith('    ') else line for line in body.split('\n'))
body=body.replace('let actor = guard.actors.get_mut(&id).unwrap();','let Some(actor) = guard.actors.get_mut(&id) else { return; };')
text=text[:start]+'    update_getter_state(store, id, target, value);\n'+text[end:]
insert=text.index('fn apply_getters(')
helper='''/// Metadata half of the getter handoff. Lua assignments stay in their
/// caller's order, and no actor-store lock survives those assignments.
pub(super) fn update_getter_state(store: &Shared, id: TableId, target: Target, value: &Update) {
'''+body+'}\n\n'
text=text[:insert]+helper+text[insert:]
write(pose,text,True)
if check:
    (base/'song-frame-handoff-extraction-proof.json').write_text(json.dumps(dict(
        getter_routing_and_pose_metadata_match_saved_source=True,checked=checked),indent=2)+'\n')
    print(json.dumps(checked))
