from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync')
def edit(name,old,new):
 p=r/'crates/deadsync-song-lua'/name;s=p.read_bytes();nl='\r\n' if b'\r\n' in s else '\n';old=old.replace('\n',nl).encode();new=new.replace('\n',nl).encode();assert s.count(old)==1,(name,s.count(old));p.write_bytes(s.replace(old,new))
edit('src/rust_engine/actors/diagnostics.rs','            Callback::Command(name) => String::from_utf8_lossy(name),','            Callback::Command(name) => format!("{}Command", String::from_utf8_lossy(name)).into(),')
edit('tests/rust_song_diagnostics.rs','.starts_with("Pulse for ActorFrame \'cycle\'")','.starts_with("PulseCommand for ActorFrame \'cycle\'")')
edit('src/rust_engine/compiler.rs','    let queued = runtime\n        .song_startup_queued_states(screen)', '''    // CALLBACK_ERROR_DEBUG_BEGIN
    #[cfg(feature = "test-support")]
    if std::env::var_os("DEADSYNC_CALLBACK_ERROR_DEBUG").is_some() {
        for (id, state) in &activation.tweens {
            if state.commands.iter().flatten().any(|name| name == "Pulse") {
                eprintln!("callback debug startup {id:?}: steps={:?} commands={:?}", state.steps, state.commands);
            }
        }
    }
    // CALLBACK_ERROR_DEBUG_END
    let queued = runtime
        .song_startup_queued_states(screen)''')
edit('src/rust_engine/actors/lifecycle.rs','        if let Some(command) = command {\n            dispatched += 1;', '''        if let Some(command) = command {
            // CALLBACK_ERROR_DEBUG_BEGIN
            #[cfg(feature = "test-support")]
            if command == b"Pulse" && std::env::var_os("DEADSYNC_CALLBACK_ERROR_DEBUG").is_some() {
                eprintln!("callback debug queue: remaining={remaining} capture={} replay={}", store.lock().unwrap().actors[&actor].capture.is_some(), store.lock().unwrap().actors[&actor].replay.is_some());
            }
            // CALLBACK_ERROR_DEBUG_END
            dispatched += 1;''')
edit('src/rust_engine/actors/recurring.rs','    let (prior, prior_clock) = {','''    // CALLBACK_ERROR_DEBUG_BEGIN
    #[cfg(feature = "test-support")]
    if command == b"Pulse" && std::env::var_os("DEADSYNC_CALLBACK_ERROR_DEBUG").is_some() {
        let guard = store.lock().unwrap();
        eprintln!("callback debug recurring: frame={:?} capture={} replay={}", guard.replay_frame, guard.actors[&actor].capture.is_some(), guard.actors[&actor].replay.is_some());
    }
    // CALLBACK_ERROR_DEBUG_END
    let (prior, prior_clock) = {''')
print('Corrected recurring label and added temporary targeted queue tracing')