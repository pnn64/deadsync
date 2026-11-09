exec((__import__('pathlib').Path(__file__).with_name('wire_song_observation_owners.py')).read_text().split('# Root-owned')[0])
edit('src/rust_engine/actors.rs','    observations: super::observations::Observations,','''    observations: super::observations::Observations,
    #[cfg(feature = "test-support")]
    audit_update_command: bool,
    #[cfg(feature = "test-support")]
    audit_queued_depth: usize,''')
edit('src/rust_engine/actors/song_messages.rs','    writes: BTreeMap<String, Vec<SongLuaStatefulMessageWrite>>,','''    writes: BTreeMap<String, Vec<SongLuaStatefulMessageWrite>>,
    #[cfg(feature = "test-support")]
    observed: BTreeMap<(usize, crate::SongLuaOverlayUpdateTarget), Vec<crate::SongLuaOverlayUpdateSample>>,''')
edit('src/rust_engine/actors/song_messages.rs','            writes: BTreeMap::new(),','''            writes: BTreeMap::new(),
            #[cfg(feature = "test-support")]
            observed: BTreeMap::new(),''')
edit('src/rust_engine/actors/song_messages.rs','    pub(super) fn write(','    #[cfg(feature = "test-support")]\n    pub(super) fn observe(&mut self, index: usize, beat: f32, target: crate::SongLuaOverlayUpdateTarget, value: &crate::SongLuaOverlayUpdateValue, scheduled: bool, eligible: bool) {\n        if !eligible { return; }\n        let samples = self.observed.entry((index, target)).or_default();\n        let sample = crate::SongLuaOverlayUpdateSample { time: beat, value: value.clone() };\n        if samples.last().is_some_and(|last| last.time == beat) {\n            if !scheduled { *samples.last_mut().unwrap() = sample; }\n        } else { samples.push(sample); }\n    }\n\n    pub(super) fn write(')
edit('src/rust_engine/actors/song_messages.rs','impl Actors {','''#[cfg(feature = "test-support")]
pub(super) fn observed_eligible(store: &Store) -> bool {
    store.active_message.is_none() && (store.audit_queued_depth == 0 || store.audit_update_command)
}
#[cfg(feature = "test-support")]
pub(super) fn update_command(store: &Shared, field: &[u8]) -> bool {
    let mut store = store.lock().unwrap();
    let prior = store.audit_update_command;
    if field == b"UpdateCommand" { store.audit_update_command = true; }
    prior
}
#[cfg(feature = "test-support")]
pub(super) fn restore_command(store: &Shared, prior: bool) {
    store.lock().unwrap().audit_update_command = prior;
}
#[cfg(feature = "test-support")]
pub(super) fn pose_write(call: &mut HostCall<'_>, store: &Shared, id: TableId, delta: &crate::SongLuaOverlayStateDelta) {
    {
        let guard = store.lock().unwrap();
        if song_pose::mode(&guard, id) || song_immediate::context(&guard, id).is_none() { return; }
    }
    let mut state = crate::SongLuaOverlayState::default();
    crate::apply_overlay_delta(&mut state, delta);
    for target in crate::tween_pose::TARGETS {
        if !delta.has_update_target(target) { continue; }
        let beat = effects::values(call, false).map_or(0., |values| values[0]);
        let value = crate::perframe::overlay_state_update_value(&state, target);
        let mut guard = store.lock().unwrap();
        let eligible = observed_eligible(&guard);
        let Some((owner, index)) = song_immediate::context(&guard, id) else { return; };
        let scheduled = !guard.actors[&id].queue.is_empty();
        guard.scenes.get_mut(&owner).unwrap().song_session.as_mut().unwrap().messages.as_mut().unwrap()
            .observe(index, beat, target, &value, scheduled, eligible);
    }
}

impl Actors {
    #[cfg(feature = "test-support")]
    pub(in crate::rust_engine) fn take_song_observed_writes(&self, screen: ActorScene) -> Vec<crate::SongLuaOverlayUpdateTrack> {
        let mut store = self.store.lock().unwrap();
        let Some(capture) = store.scenes.get_mut(&screen.id).and_then(|s| s.song_session.as_mut()).and_then(|s| s.messages.as_mut()) else { return Vec::new(); };
        std::mem::take(&mut capture.observed).into_iter().map(|((overlay_index, target), samples)| crate::SongLuaOverlayUpdateTrack { overlay_index, target, samples }).collect()
    }''')
edit('src/rust_engine/actors/capture.rs','''    if !song_pose::record(call, store, id, &delta) {
        song_messages::record(call, store, id, &delta);
    }''','''    #[cfg(feature = "test-support")]
    song_messages::pose_write(call, store, id, &delta);
    if !song_pose::record(call, store, id, &delta) {
        song_messages::record(call, store, id, &delta);
    }''')
# Non-pose frame setters already read the per-write clock. Reuse that value,
# preserving successful equal writes and the immediate/scheduled distinction.
for name in ['song_immediate.rs','song_properties.rs']:
 path='src/rust_engine/actors/'+name
 old='    let message = guard.active_message.clone();'
 if name=='song_immediate.rs':
  p=r/path;s=p.read_text();a=s.index('fn record_frame(');b=s.index('pub(super) fn delta(',a);block=s[a:b]
  assert block.count(old)==1
  new=old+'\n    #[cfg(feature = "test-support")]\n    let observed_eligible = song_messages::observed_eligible(&guard);'
  block=block.replace(old,new)
  old2='    capture.immediate.record(index, target, value, scheduled);'
  block=block.replace(old2,'    #[cfg(feature = "test-support")]\n    capture.observe(index, beat, target, &value, scheduled.is_some(), observed_eligible);\n'+old2)
  edit(path,s[a:b],block)
 else:
  edit(path,old,old+'\n    #[cfg(feature = "test-support")]\n    let observed_eligible = song_messages::observed_eligible(&guard);')
  edit(path,'    let timed = scheduled.is_some();','    let timed = scheduled.is_some();\n    #[cfg(feature = "test-support")]\n    capture.observe(index, beat, target, &value, timed, observed_eligible);')
# Preserve dynamic command scope even on failure; queue drains happen after the
# UpdateCommand scope is restored, exactly as in the baseline.
edit('src/rust_engine/actors.rs','    let previous_capture_start = capture::command_start(store, actor);','    let previous_capture_start = capture::command_start(store, actor);\n    #[cfg(feature = "test-support")]\n    let observed_command = song_messages::update_command(store, &field);')
edit('src/rust_engine/actors.rs','    let result = result.and_then(|()| {','    #[cfg(feature = "test-support")]\n    song_messages::restore_command(store, observed_command);\n    let result = result.and_then(|()| {')
edit('src/rust_engine/actors/native_children.rs','''                    call.table_set(active, key, Value::Boolean(true))?;
                    let result = (|| {''','''                    call.table_set(active, key, Value::Boolean(true))?;
                    #[cfg(feature = "test-support")]
                    let observed_command = song_messages::update_command(store, &command_field);
                    let result = (|| {''')
edit('src/rust_engine/actors/native_children.rs','''                    call.table_set(active, key, Value::Nil)?;
                    result''','''                    #[cfg(feature = "test-support")]
                    song_messages::restore_command(store, observed_command);
                    call.table_set(active, key, Value::Nil)?;
                    result''')
edit('src/rust_engine/actors/lifecycle.rs','            let result = if let Some(message) = command.strip_prefix(b"!") {','            #[cfg(feature = "test-support")]\n            { store.lock().unwrap().audit_queued_depth += 1; }\n            let result = if let Some(message) = command.strip_prefix(b"!") {')
edit('src/rust_engine/actors/lifecycle.rs','            store.lock().unwrap().replay_dispatch = previous;','            #[cfg(feature = "test-support")]\n            { store.lock().unwrap().audit_queued_depth -= 1; }\n            store.lock().unwrap().replay_dispatch = previous;')
edit('src/rust_engine/actors/capture.rs','''        // The registry and this lexical root retain the dispatch target even
        // when its command edits the scene and forces a collection.
        let result''','''        // The registry and this lexical root retain the dispatch target even
        // when its command edits the scene and forces a collection.
        #[cfg(feature = "test-support")]
        { store.lock().unwrap().audit_queued_depth += 1; }
        let result''')
edit('src/rust_engine/actors/capture.rs','''            guard.capture_scope = prior;
            guard.replay_dispatch = previous_dispatch;
            if let Some(clock) = &mut guard.replay_clock {
                clock.dispatch = previous_clock;
            }
        }
        result?;''','''            guard.capture_scope = prior;
            guard.replay_dispatch = previous_dispatch;
            #[cfg(feature = "test-support")]
            { guard.audit_queued_depth -= 1; }
            if let Some(clock) = &mut guard.replay_clock {
                clock.dispatch = previous_clock;
            }
        }
        result?;''')
edit('src/rust_engine/song_updates.rs','    pub overlay_tracks: Vec<crate::SongLuaOverlayUpdateTrack>,','    pub overlay_tracks: Vec<crate::SongLuaOverlayUpdateTrack>,\n    #[cfg(feature = "test-support")]\n    pub overlay_writes: Vec<crate::SongLuaOverlayUpdateTrack>,')
edit('src/rust_engine/song_updates.rs','        roots.clear(&mut self.vm)?;','        #[cfg(feature = "test-support")]\n        self.observations.stop();\n        roots.clear(&mut self.vm)?;')
edit('src/rust_engine/song_updates.rs','            let wrapper_start = values.len();','            #[cfg(feature = "test-support")]\n            self.observations.start();\n            let wrapper_start = values.len();')
edit('src/rust_engine/song_updates.rs','            self.actors.end_song_update_capture(screen);','            #[cfg(feature = "test-support")]\n            let overlay_writes = self.actors.take_song_observed_writes(screen);\n            self.actors.end_song_update_capture(screen);')
edit('src/rust_engine/song_updates.rs','                overlay_tracks: tracks,','                overlay_tracks: tracks,\n                #[cfg(feature = "test-support")]\n                overlay_writes,')
edit('src/rust_engine/compiler.rs','    out.eases.extend(update.eases);','    #[cfg(feature = "test-support")]\n    {\n        runtime.observations.take().apply(&mut out);\n        out.overlay_writes = update.overlay_writes;\n    }\n    out.eases.extend(update.eases);')
print('Wired overlay setter history and chronological observation boundaries')
