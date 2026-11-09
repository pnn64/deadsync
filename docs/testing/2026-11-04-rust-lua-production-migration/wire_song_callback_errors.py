from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync')
def edit(name,old,new):
 p=r/'crates/deadsync-song-lua'/name;s=p.read_bytes();nl='\r\n' if b'\r\n' in s else '\n';old=old.replace('\n',nl).encode();new=new.replace('\n',nl).encode();assert s.count(old)==1,(name,s.count(old));p.write_bytes(s.replace(old,new))
edit('src/rust_engine/actors.rs','mod draw;','mod draw;\nmod diagnostics;')
edit('src/rust_engine/actors.rs','    diagnostics: Vec<String>,','    diagnostics: Vec<String>,\n    callback_errors: Vec<String>,')
edit('src/rust_engine/compiler.rs','    for detail in runtime.take_actor_diagnostics() {','    for detail in runtime.actors.take_callback_errors() {')
edit('src/rust_engine/actors/compiler.rs','                draw::scene(call, &read.store, root)?;','                if let Err(error) = draw::scene(call, &read.store, root) {\n                    diagnostics::report(call, &read.store, root, diagnostics::Callback::Draw, error)?;\n                }')
# The standalone draw API still returns traversal errors; the compiler wrapper
# above is the native run_actor_draw_functions_for_table catch/report boundary.
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/draw.rs';s=p.read_text();a=s.index('    if error.kind() != ErrorKind::Runtime {',s.index('fn report('));b=s.index('\n}\nfn draw(',a)
old=s[a:b];edit('src/rust_engine/actors/draw.rs',old,'    diagnostics::report(call, store, actor, diagnostics::Callback::Draw, error)')
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/song_activation.rs';s=p.read_text();a=s.index('            match result {\n                Err(error) if error.kind() == ErrorKind::Runtime => {',s.index('fn zero_node('));b=s.index('            drain?;',a)+len('            drain?;')
edit('src/rust_engine/actors/song_activation.rs',s[a:b],'''            if let Err(error) = result {
                diagnostics::report(call, store, actor, diagnostics::Callback::Update, error)?;
            }
            if let Err(error) = drain {
                diagnostics::report(call, store, actor, diagnostics::Callback::Queue, error)?;
            }''')
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/song_multitap.rs';s=p.read_text();a=s.index('    for result in [result, drain] {',s.index('fn callback('));b=s.index('    Ok(())',a)
edit('src/rust_engine/actors/song_multitap.rs',s[a:b],'''    if let Err(error) = result {
        diagnostics::report(call, store, actor, diagnostics::Callback::Update, error)?;
    }
    if let Err(error) = drain {
        diagnostics::report(call, store, actor, diagnostics::Callback::Queue, error)?;
    }
''')
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/lifecycle.rs';s=p.read_text();a=s.index('        match result {\n            Err(error) if error.kind() == ErrorKind::Runtime => {',s.index('    enter_phase(store, node.callback_order, delta);'));b=s.index('\n    }\n    // NoteField',a)
edit('src/rust_engine/actors/lifecycle.rs',s[a:b],'''        let drain = capture::drain_commands(call, store, actor);
        if let Err(error) = result {
            diagnostics::report(call, store, actor, diagnostics::Callback::Update, error)?;
        }
        if let Err(error) = drain {
            diagnostics::report(call, store, actor, diagnostics::Callback::Queue, error)?;
        }''')
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/capture.rs';s=p.read_text();a=s.index('    match drain_commands(call, store, actor) {',s.index('pub(super) fn advance_ready('));b=s.index('\n}\n\npub(super) fn drain_commands(',a)
edit('src/rust_engine/actors/capture.rs',s[a:b],'''    match drain_commands(call, store, actor) {
        Err(error) => diagnostics::report(call, store, actor, diagnostics::Callback::Queue, error),
        Ok(()) => Ok(()),
    }''')
edit('src/rust_engine/actors/capture.rs','    queue_error_reported: bool,\n','')
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/recurring.rs';s=p.read_text();a=s.index('    match result {');b=s.index('\n}\n\npub(super) fn advance(',a)
edit('src/rust_engine/actors/recurring.rs',s[a:b],'''    drop(guard);
    match result {
        Err(error) => diagnostics::report(call, store, actor, diagnostics::Callback::Command(command), error),
        Ok(()) => Ok(()),
    }''')
edit('src/rust_engine/actors/recurring.rs','    error_reported: bool,\n','')
print('Wired categorized callback errors without changing the existing native compiler or tests')