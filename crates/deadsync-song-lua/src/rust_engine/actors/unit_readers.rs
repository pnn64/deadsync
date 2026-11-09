//! Fixture entry points to actual host readers; callbacks keep their bindings
//! outside allocation measurements and use the production captured primitives.
use super::*;
use deadlib_present::actors::TextAttribute;
impl Actors {
    /// The old size-getter fixtures mutate loading metadata between calls.
    /// Read that preparation representation before observing the real native
    /// getters, as the compiler does before publishing owned actor state.
    pub(in crate::rust_engine) fn fixture_loading_size_getter(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &'static str,
    ) -> Result<deadlib_lua::Root, Error> {
        self.fixture_reader(
            vm,
            actor,
            |call| {
                let (store, name) = call
                    .function_data::<(Shared, &'static str)>()
                    .unwrap()
                    .clone();
                let actor = table(call, call.captures()[2])?;
                let state = initial_state::read(call, &store, actor)?;
                {
                    let mut store = store.lock().unwrap();
                    let actor = store.actors.get_mut(&actor).unwrap();
                    actor.state.render = state;
                    actor.current = state.into();
                }
                if name == "GetNumStates" {
                    sprites::method(call, &store, actor, name)
                } else {
                    get_scalar(call, &store, actor, name)
                }
            },
            (self.store.clone(), name),
        )
    }
    pub(in crate::rust_engine) fn fixture_reader<T: std::any::Any + Send + Sync>(
        &self,
        vm: &mut Vm,
        actor: Value,
        callback: deadlib_lua::HostFunction,
        data: T,
    ) -> Result<deadlib_lua::Root, Error> {
        let function = vm.new_host_function_with_data(
            callback,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            data,
        )?;
        vm.pin(function)
    }
}
pub fn fixture_text_content(call: &mut HostCall<'_>, actor: TableId) -> Result<String, Error> {
    text::content(call, actor)
}
pub fn fixture_text_value(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    text::text_value(call, value)
}
pub fn fixture_text_attributes(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Arc<[TextAttribute]>, Error> {
    text::attributes(call, actor)
}
pub fn fixture_sync_text(
    call: &mut HostCall<'_>,
    actor: TableId,
    current: &mut Arc<str>,
) -> Result<(), Error> {
    live_visuals::sync_text(call, actor, current)
}
pub fn fixture_sync_text_attributes(
    call: &mut HostCall<'_>,
    actor: TableId,
    current: &mut Arc<[TextAttribute]>,
) -> Result<(), Error> {
    live_visuals::sync_attributes(call, actor, current)
}
pub fn fixture_vector_components<const N: usize>(
    call: &mut HostCall<'_>,
    value: TableId,
) -> Result<Option<[f32; N]>, Error> {
    initial_state::components(call, value)
}

pub fn fixture_vertex_colors(
    call: &mut HostCall<'_>,
    value: TableId,
) -> Result<Option<[[f32; 4]; 4]>, Error> {
    initial_state::vertex_colors(call, value)
}

pub type FixtureStateOutput = Arc<Mutex<Option<Result<crate::SongLuaOverlayState, String>>>>;
struct FixtureStateRead {
    store: Shared,
    output: FixtureStateOutput,
    live: bool,
}
impl Actors {
    pub(in crate::rust_engine) fn fixture_state_reader(
        &self,
        vm: &mut Vm,
        actor: Value,
        live: bool,
    ) -> Result<(deadlib_lua::Root, FixtureStateOutput), Error> {
        let output = Arc::new(Mutex::new(None));
        let function = vm.new_host_function_with_data(
            |call| {
                let data = call.function_data::<FixtureStateRead>().unwrap();
                let store = data.store.clone();
                let output = data.output.clone();
                let live = data.live;
                let actor = table(call, call.captures()[2])?;
                let result = if live {
                    initial_state::read_live(call, &store, actor)
                } else {
                    initial_state::read(call, &store, actor)
                };
                let result = match result {
                    Err(error)
                        if matches!(
                            error.kind(),
                            deadlib_lua::ErrorKind::Memory | deadlib_lua::ErrorKind::Limit
                        ) =>
                    {
                        return Err(error);
                    }
                    result => result,
                };
                *output.lock().unwrap() = Some(result.map_err(|error| error.to_string()));
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            FixtureStateRead {
                store: self.store.clone(),
                output: output.clone(),
                live,
            },
        )?;
        Ok((vm.pin(function)?, output))
    }
}
pub fn fixture_field_text(call: &mut HostCall<'_>, value: Value) -> Result<Option<String>, Error> {
    initial_state::value_text(call, value)
}

impl Actors {
    pub(in crate::rust_engine) fn fixture_effect_clock(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<deadlib_lua::Root, Error> {
        let function = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let actor = table(call, call.captures()[2])?;
                let Value::Number(delta) = call.arguments()[0] else {
                    return Err(call.error(b"number expected"));
                };
                effects::advance(call, &store, actor, delta as f32)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            self.store.clone(),
        )?;
        vm.pin(function)
    }
}

pub fn fixture_tap_note(call: &mut HostCall<'_>, params: TableId) -> Result<Value, Error> {
    let note = call.try_new_table()?;
    call.with_roots(&[Value::Table(note)], |call| {
        messages::judgment::normalize_note(call, params, note)?;
        Ok(Value::Table(note))
    })
}

impl Actors {
    pub(in crate::rust_engine) fn fixture_base_values(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<[Option<f32>; 6], Error> {
        let output: Arc<Mutex<Option<[Option<f32>; 6]>>> = Arc::new(Mutex::new(None));
        let function = vm.new_host_function_with_data(
            |call| {
                let (store, output) = call
                    .function_data::<(Shared, Arc<Mutex<Option<[Option<f32>; 6]>>>)>()
                    .unwrap()
                    .clone();
                let actor = table(call, call.captures()[2])?;
                host_compat::initialize_base(call, &store, actor)?;
                host_compat::initialize_pose(call, &store, actor)?;
                let base = store.lock().unwrap().actors[&actor].base_transform;
                *output.lock().unwrap() =
                    Some(crate::base_transform::BaseField::ALL.map(|field| base.get(field)));
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            (self.store.clone(), output.clone()),
        )?;
        vm.call(function, &[])?;
        let values = output.lock().unwrap().take().unwrap();
        Ok(values)
    }
}

impl Actors {
    pub(in crate::rust_engine) fn fixture_capture_reset(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<deadlib_lua::Root, Error> {
        let f = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let actor = table(call, call.captures()[2])?;
                function_capture::reset(call, &store, actor)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            self.store.clone(),
        )?;
        vm.pin(f)
    }
    pub(in crate::rust_engine) fn fixture_invalidate_plans(
        &self,
        vm: &mut Vm,
    ) -> Result<(), Error> {
        let f = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                lifecycle::invalidate_plans(call, &store)
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            self.store.clone(),
        )?;
        vm.call(f, &[]).map(|_| ())
    }
}
pub fn fixture_mesh_points(
    call: &mut HostCall<'_>,
    actor: TableId,
    vertices: TableId,
    packed: bool,
) -> Result<Vec<crate::SongLuaActorMultiVertexPoint>, Error> {
    if !packed {
        return meshes::points(call, vertices);
    }
    meshes::upload(call, actor, vertices)?;
    let value = get(call, actor, b"__songlua_vertices")?;
    Ok(meshes::source(call, value)?.unwrap())
}

pub fn fixture_vector_value<const N: usize>(
    call: &mut HostCall<'_>,
    value: Value,
) -> Result<Option<[f32; N]>, Error> {
    let t = table(call, value)?;
    initial_state::components(call, t)
}

/// The current compiler saves native values alongside the raw Lua snapshot.
/// Keep both parts for the original native-vector independence fixture.
pub struct FixtureNativeSnapshot {
    saved: Option<function_capture::Saved>,
    raw: deadlib_lua::Root,
    actor: Value,
}

impl Actors {
    pub(in crate::rust_engine) fn fixture_ensure_native_replay(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<(), Error> {
        let f = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let actor = table(call, call.captures()[2])?;
                chronological::ensure(call, &store, actor)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            self.store.clone(),
        )?;
        vm.call(f, &[]).map(|_| ())
    }
    pub(in crate::rust_engine) fn fixture_begin_pose_observation(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<(), Error> {
        let f = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let actor = table(call, call.captures()[2])?;
                chronological::ensure(call, &store, actor)?;
                store.lock().unwrap().fixture_pose_observer =
                    Some(song_messages::Capture::new([(actor, 0)]));
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            self.store.clone(),
        )?;
        vm.call(f, &[]).map(|_| ())
    }
    pub(in crate::rust_engine) fn fixture_pose_sample(
        &self,
    ) -> (usize, Option<Arc<[[f32; 4]; 4]>>) {
        self.store
            .lock()
            .unwrap()
            .fixture_pose_observer
            .as_ref()
            .unwrap()
            .fixture_vertex_sample()
    }
    pub(in crate::rust_engine) fn fixture_pose_scope(
        &self,
        queued: usize,
        update: bool,
        message: Option<&str>,
    ) {
        let mut store = self.store.lock().unwrap();
        store.audit_queued_depth = queued;
        store.audit_update_command = update;
        store.active_message = message.map(str::to_owned);
    }
}
impl Actors {
    pub(in crate::rust_engine) fn fixture_save_native(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<FixtureNativeSnapshot, Error> {
        let raw = self.fixture_export_live_state(vm, actor, true)?;
        let Value::Table(raw_id) = vm.rooted(raw)? else {
            unreachable!()
        };
        let Value::Table(id) = actor else {
            unreachable!()
        };
        let saved =
            function_capture::Saved::read(&self.store, id, raw_id).expect("owned native fixture");
        Ok(FixtureNativeSnapshot {
            saved: Some(saved),
            raw,
            actor,
        })
    }
    pub(in crate::rust_engine) fn fixture_restore_native(
        &self,
        vm: &mut Vm,
        mut snapshot: FixtureNativeSnapshot,
    ) -> Result<(), Error> {
        let raw = vm.rooted(snapshot.raw)?;
        let result = self.fixture_restore_live_state(vm, snapshot.actor, raw);
        snapshot.saved.take().unwrap().restore_mutable(&self.store);
        vm.unpin(snapshot.raw)?;
        result
    }
    pub(in crate::rust_engine) fn fixture_shadow_storage(&self, actor: Value) -> (usize, [f32; 2]) {
        let Value::Table(id) = actor else {
            panic!("actor table")
        };
        let store = self.store.lock().unwrap();
        let field = &store.actors[&id].state.render.shadow_len;
        (field.as_ptr() as usize, *field)
    }
}
