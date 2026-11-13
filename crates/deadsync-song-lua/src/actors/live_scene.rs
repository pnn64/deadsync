//! Ordered host operations used by the retained Rust live scene.
use super::*;

#[derive(Clone)]
pub(crate) enum Phase {
    States,
    Off,
    ClearHistory,
    EndSounds,
    Health(Error),
}
#[derive(Clone)]
struct Run {
    store: Shared,
    scene: u64,
    phase: Phase,
    output: Arc<Mutex<Vec<crate::SongLuaOverlayState>>>,
}
impl Actors {
    pub(crate) fn suppress_load_sound_retirement(&self, scene: ActorScene) {
        if let Some(scene) = self.store.lock().unwrap().scenes.get_mut(&scene.id) {
            scene.sounds_retired = true;
        }
    }
    pub(crate) fn reset_live_playback(&self, vm: &mut Vm) -> Result<(), Error> {
        let f = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                lifecycle::invalidate_plans(call, &store)?;
                let mut s = store.lock().unwrap();
                s.playback = false;
                s.replay_frame = None;
                s.replay_dispatch = None;
                s.replay_advancing = false;
                s.replay_clock = None;
                for actor in s.actors.values_mut() {
                    actor.raw_live_epoch = true;
                    actor.queue.clear();
                    actor.replay = None;
                    actor.preparation_getters = None;
                }
                Ok(())
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            self.store.clone(),
        )?;
        vm.call(f, &[]).map(|_| ())
    }
    pub(crate) fn begin_live_load(&self) {
        let mut s = self.store.lock().unwrap();
        s.trace.scene = None;
        s.trace.generation += 1;
        s.live_sources = Some(Vec::new());
        if let Some(b) = &mut s.song_broadcasts {
            b.events.clear();
        }
    }
    pub(crate) fn has_live_load(&self) -> bool {
        self.store.lock().unwrap().live_sources.is_some()
    }
    pub(crate) fn live_phase(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        batch: Value,
        phase: Phase,
    ) -> Result<Vec<crate::SongLuaOverlayState>, Error> {
        let output = Arc::new(Mutex::new(Vec::new()));
        let error = if let Phase::Health(e) = &phase {
            e.value().unwrap_or(Value::Nil)
        } else {
            Value::Nil
        };
        let f = vm.new_host_function_with_data(
            run,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                batch,
                error,
            ],
            Run {
                store: self.store.clone(),
                scene: scene.id,
                phase,
                output: output.clone(),
            },
        )?;
        let first = self.store.lock().unwrap().next;
        let result = vm.call(f, &[]).map(|_| ());
        song_activation::adopt_created(&self.store, scene.id, first);
        result?;
        Ok(std::mem::take(&mut *output.lock().unwrap()))
    }
    pub(crate) fn take_live_sources(&self) -> Vec<crate::SongLuaLiveBroadcast> {
        self.store
            .lock()
            .unwrap()
            .live_sources
            .as_mut()
            .map_or_else(Vec::new, std::mem::take)
    }
}
fn run(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let batch = table(call, call.captures()[2])?;
    match data.phase {
        Phase::States => {
            for slot in [2., 3.] {
                let list = call.raw_table_get(batch, Value::Number(slot))?;
                let list = table(call, list)?;
                for i in 1..=call.raw_table_len(list)? {
                    let actor = table(call, call.raw_table_get(list, Value::Number(i as f64))?)?;
                    let state = initial_state::read_live(call, &data.store, actor)?;
                    data.output.lock().unwrap().push(state);
                }
            }
        }
        Phase::ClearHistory => {
            let globals = table(call, helper(call, 5)?)?;
            let key = call.try_intern(crate::SONG_LUA_BROADCASTS_KEY.as_bytes())?;
            let history = call.raw_table_get(globals, key)?;
            if history != Value::Nil {
                let history = table(call, history)?;
                let clear = call.new_host_function(
                    |call| {
                        let history = call.captures()[0];
                        let mut s = call.native_stack()?;
                        let base = s.top();
                        s.push(history)?;
                        let index = s.top_offset();
                        s.push(Value::Nil)?;
                        let mut keys = Vec::new();
                        while s.next_table(index)? {
                            keys.push(s.value(-2).unwrap());
                            s.set_top(s.top() - 1)?;
                        }
                        for key in keys {
                            s.push(key)?;
                            s.push(Value::Nil)?;
                            s.set_table(index, true)?;
                        }
                        s.set_top(base)?;
                        s.finish(0)
                    },
                    &[Value::Table(history)],
                )?;
                call.call(clear, &[])?;
            }
        }
        Phase::Health(error) => {
            let screen = top(call)?;
            diagnostics::report(
                call,
                &data.store,
                screen,
                diagnostics::Callback::Health,
                error,
            )?;
        }
        Phase::Off => {
            let screen = top(call)?;
            fn visit(call: &mut HostCall<'_>, s: &Shared, actor: TableId) -> Result<(), Error> {
                native_children::run_command_with_drain(call, s, actor, b"Off", None, true)?;
                native_children::with_children(call, actor, |call, c| visit(call, s, c))
            }
            native_children::with_children(call, screen, |call, c| visit(call, &data.store, c))?;
            let children = native_children::named_children(call, screen)?;
            call.with_roots(&[children], |call| {
                let key = call.try_intern(b"Out")?;
                let out = call.raw_table_get(table(call, children)?, key)?;
                if out != Value::Nil {
                    transitions::start(call, &data.store, table(call, out)?)?;
                }
                Ok(())
            })?;
        }
        Phase::EndSounds => {
            let list = table(call, call.raw_table_get(batch, Value::Number(2.))?)?;
            for i in 1..=call.raw_table_len(list)? {
                let actor = table(call, call.raw_table_get(list, Value::Number(i as f64))?)?;
                if assets::field(call, actor, b"__songlua_actor_type")?
                    .is_some_and(|s| s.eq_ignore_ascii_case("Sound"))
                {
                    let owner = call.object_pointer(Value::Table(actor)).unwrap() as u64;
                    data.store
                        .lock()
                        .unwrap()
                        .sounds
                        .push(crate::SongLuaActorSound::Stop { owner });
                }
            }
            if let Some(scene) = data.store.lock().unwrap().scenes.get_mut(&data.scene) {
                scene.sounds_retired = true;
            }
        }
    }
    Ok(())
}
fn top(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let key = call.try_intern(b"__songlua_top_screen")?;
    table(call, call.raw_table_get(globals, key)?)
}

#[cfg(feature = "test-support")]
impl Actors {
    pub(crate) fn fixture_live_registry(&self) -> Vec<Value> {
        self.store
            .lock()
            .unwrap()
            .actor_order
            .iter()
            .copied()
            .map(Value::Table)
            .collect()
    }
    pub(crate) fn fixture_draw_storage(&self) -> (usize, usize, usize) {
        self.store
            .lock()
            .unwrap()
            .draw_capture
            .as_ref()
            .map_or((0, 0, 0), |capture| {
                (
                    capture.frames.len(),
                    capture.frames.iter().map(|frame| frame.calls.len()).sum(),
                    capture.bytes,
                )
            })
    }
    pub(crate) fn fixture_command_depth(&self) -> usize {
        self.store.lock().unwrap().song_callback_depth
    }
    pub(crate) fn fixture_set_command_depth(&self, depth: usize) {
        self.store.lock().unwrap().song_callback_depth = depth;
    }
    pub(crate) fn fixture_advance_actor(
        &self,
        vm: &mut Vm,
        actor: Value,
        delta: f32,
    ) -> Result<(), Error> {
        let callback = vm.new_host_function_with_data(
            |call| {
                let (store, delta) = call.function_data::<(Shared, f32)>().unwrap().clone();
                let actor = table(call, call.captures()[2])?;
                lifecycle::advance(call, &store, actor, delta)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            (self.store.clone(), delta),
        )?;
        vm.call(callback, &[]).map(|_| ())
    }
    pub(crate) fn fixture_command_depth_function(&self, vm: &mut Vm) -> Result<(), Error> {
        let callback = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap();
                let depth = store.lock().unwrap().song_callback_depth;
                call.push(Value::Number(depth as f64))
            },
            &[],
            self.store.clone(),
        )?;
        vm.set_global(b"CommandDepth", callback)
    }
}

#[cfg(feature = "test-support")]
impl Actors {
    pub(crate) fn fixture_has_motion(&self, value: Value) -> bool {
        let Value::Table(id) = value else {
            return false;
        };
        self.store
            .lock()
            .unwrap()
            .actors
            .get(&id)
            .is_some_and(effects::has_motion)
    }
}
