from pathlib import Path
import base64,hashlib,json,re
r=Path('/mnt/c/GitHub/deadsync');d=r/'docs/testing/2026-11-04-rust-lua-production-migration';bp=d/'song-callback-errors-edit-baseline.json';b=json.loads(bp.read_text());name='crates/deadsync-song-lua/src/rust_engine/actors/chronological.rs';raw=(r/name).read_bytes();assert name not in b;b[name]={'sha256':hashlib.sha256(raw).hexdigest(),'base64':base64.b64encode(raw).decode()};before=json.loads((d/'song-callback-errors-before-sources.json').read_text(encoding='utf-8-sig'));assert b[name]['sha256']==before[name];bp.write_text(json.dumps(b,indent=2)+'\n')
debug={}
for name in ['src/rust_engine/compiler.rs','src/rust_engine/actors/lifecycle.rs','src/rust_engine/actors/recurring.rs']:
 p=r/'crates/deadsync-song-lua'/name;raw=p.read_bytes();debug[name]={'sha256':hashlib.sha256(raw).hexdigest(),'base64':base64.b64encode(raw).decode()};s=raw.decode();s,n=re.subn(r'(?m)^([ \t]*)// CALLBACK_ERROR_DEBUG_BEGIN\r?\n[\s\S]*?^[ \t]*// CALLBACK_ERROR_DEBUG_END\r?\n','',s);assert n==1,name;p.write_bytes(s.encode())
(d/'song-callback-errors-debug-sources.json').write_text(json.dumps(debug,indent=2)+'\n')
def edit(name,old,new):
 p=r/'crates/deadsync-song-lua'/name;s=p.read_bytes();nl='\r\n' if b'\r\n' in s else '\n';old=old.replace('\n',nl).encode();new=new.replace('\n',nl).encode();assert s.count(old)==1,(name,s.count(old));p.write_bytes(s.replace(old,new))
edit('src/rust_engine/actors/capture.rs','pub(super) fn deferred(store: &Shared, actor: TableId) -> bool {','''pub(super) fn has_recurring(store: &Shared, actor: TableId) -> bool {
    store.lock().unwrap().actors.get(&actor)
        .and_then(|actor| actor.capture.as_ref())
        .is_some_and(|capture| capture.recurring.command.is_some())
}

pub(super) fn deferred(store: &Shared, actor: TableId) -> bool {''')
edit('src/rust_engine/actors/chronological.rs','    if initial_state::value_text(call, recurring)?.is_some() {','''    if if recurring == Value::Nil {
        capture::has_recurring(store, actor)
    } else {
        initial_state::value_text(call, recurring)?.is_some()
    } {''')
edit('src/rust_engine/actors/song_multitap.rs','    let recurring = initial_state::value_text(call, recurring)?.is_some();','''    let recurring = if recurring == Value::Nil {
        capture::has_recurring(store, actor)
    } else {
        initial_state::value_text(call, recurring)?.is_some()
    };''')
edit('tests/rust_song_diagnostics.rs','    assert!(native.unwrap_err().contains("cycle marker"));\n    assert!(rust.unwrap_err().contains("cycle marker"));','''    assert!(native.is_err(), "native must reject the startup queue failure");
    assert!(rust.is_err(), "Rust must reject the startup queue failure");
    assert!(native.unwrap_err().contains("cycle marker"));
    assert!(rust.unwrap_err().contains("cycle marker"));''')
print('Fixed metadata-owned recurrence in eligibility and traversal; removed all temporary tracing')