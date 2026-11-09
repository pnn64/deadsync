"""Install the input-only draft; combined traversal remains staged separately."""
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
actor=r/'crates/deadsync-song-lua/src/rust_engine/actors'
owner=(d/'draft-actor-input-snapshot-owner.rs.txt').read_text()
owner=owner.replace('pub(super) mod update;\npub use update::CompiledActorInteractions;\n','')
text=owner+'\n'+(d/'draft-actor-input-snapshot-inspection.rs.txt').read_text()
text+='\n'+(d/'draft-actor-input-snapshot-compiler.rs.txt').read_text()
text+='''
fn position_ready(stack: &mut NativeStack<'_, '_>, store: &Shared, actor: TableId) -> Result<bool, Error> {
    {
        let guard = store.lock().unwrap();
        let Some(actor) = guard.actors.get(&actor) else { return Ok(false); };
        if guard.ease_probe_active || actor.raw_live_epoch || !actor.queue.is_empty()
            || !effects::snapshot_position_ready(actor) { return Ok(false); }
    }
    for key in [b"__songlua_command_queue".as_slice(), b"__songlua_state_spin"] {
        match raw(stack, actor, key)? {
            Value::Nil => {},
            Value::Table(queue) if key == b"__songlua_command_queue" && raw_len(stack, queue)? == 0 => {},
            _ => return Ok(false),
        }
    }
    let mode = raw(stack, actor, b"__songlua_state_effect_mode")?;
    if mode != Value::Nil && byte_string(stack, mode)?.as_deref() != Some(b"none") { return Ok(false); }
    Ok(!raw(stack, actor, b"__songlua_state_rainbow")?.is_truthy())
}

impl Actors {
    pub(in crate::rust_engine) fn compile_input_snapshot(
        &self, vm: &mut Vm, functions: &[Value], actors: &[Value], states: &[SongLuaOverlayState],
        tracked: &[(SongLuaTrackedActorTarget, Value, SongLuaOverlayState)],
    ) -> Option<CompiledActorInputs> {
        {
            let guard = self.store.lock().unwrap();
            if !guard.playback || guard.action_capture.is_some() { return None; }
        }
        if log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) { return None; }
        let actor_table = |value| if let Value::Table(table) = value { Some(table) } else { None };
        let actors = actors.iter().map(|value| actor_table(*value)).collect::<Option<Vec<_>>>()?;
        let tracked = tracked.iter().map(|(target, value, state)|
            Some((*target, actor_table(*value)?, *state))).collect::<Option<Vec<_>>>()?;
        vm.with_native_stack(|stack| {
            let top = stack.top();
            let result = if stack.hook(stack.thread_id())?.function != deadlib_lua::NativeHook::None { None }
                else { compile_with_updates(stack, &self.store, functions, &actors, states, &tracked, &[], &[]) };
            stack.set_top(top)?;
            Ok(result)
        }).ok().flatten()
    }
}
'''
paths={actor/'input_snapshot.rs':text,
       actor/'input_snapshot/tables.rs':(d/'draft-actor-input-snapshot-tables.rs.txt').read_text()}
for p,data in paths.items():
    assert not p.exists(),p;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(data)
print('Installed input-only snapshot owner, raw inspector, schema capture and bytecode lowering.')
