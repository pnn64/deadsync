//! Actor observations and getter writes used before chronological frame replay.
use super::*;
use crate::{SongLuaOverlayState as State, SongLuaPerframePlayerState as Player};

pub(crate) struct Access {
    registry: TableId,
    helpers: TableId,
}
type Output<T> = Arc<Mutex<Option<T>>>;
type Reader<T> = fn(&mut HostCall<'_>, TableId) -> Result<T, Error>;

pub(super) struct StateRead {
    callback: Root,
    result: Option<State>,
}

pub(super) struct StateWrite {
    callback: Root,
    pending: Option<(Value, State)>,
}

fn read_state(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    let state = initial_state::read(call, actor)?;
    call.store().perframe_state_reader.as_mut().unwrap().result = Some(state);
    Ok(())
}

impl Actors {
    pub(crate) fn perframe_access(&self) -> Access {
        Access {
            registry: self.registry,
            helpers: self.helpers,
        }
    }
}
impl Access {
    fn read<T: Send + 'static>(
        &self,
        vm: &mut Vm,
        actor: Value,
        reader: Reader<T>,
    ) -> Result<T, Error> {
        let output = Arc::new(Mutex::new(None));
        let callback = vm.new_host_function_with_data(
            |call| {
                let (reader, output) = call
                    .function_data::<(Reader<T>, Output<T>)>()
                    .unwrap()
                    .clone();
                let actor = table(call, call.captures()[2])?;
                let value = reader(call, actor)?;
                *output.lock().unwrap() = Some(value);
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            (reader, output.clone()),
        )?;
        vm.call(callback, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
    pub fn state(&self, vm: &mut Vm, actor: Value) -> Result<State, Error> {
        let callback = match &vm.store_ref().perframe_state_reader {
            Some(reader) => reader.callback,
            None => {
                let function = vm.new_host_function(
                    read_state,
                    &[Value::Table(self.registry), Value::Table(self.helpers)],
                )?;
                let callback = vm.pin(function)?;
                vm.store().perframe_state_reader = Some(StateRead {
                    callback,
                    result: None,
                });
                callback
            }
        };
        // The actor is rooted by the call arguments. The cached callback only
        // retains the shared registry and helpers between observations.
        let function = vm.rooted(callback)?;
        let result = vm.call(function, &[actor]).map(|_| ());
        let state = vm
            .store()
            .perframe_state_reader
            .as_mut()
            .unwrap()
            .result
            .take();
        result?;
        Ok(state.expect("state callback completed"))
    }
    pub fn player(&self, vm: &mut Vm, actor: Value) -> Result<Player, Error> {
        self.read(vm, actor, |call, actor| {
            let number = |call: &mut HostCall<'_>, key: &'static str, index: usize| {
                let raw = number(call, actor, key)?;
                let native = call.store().actors.get(&actor).and_then(|a| {
                    let mask = a.capture.as_ref().map_or(0, |c| c.transform_fields);
                    let pose = a.destination();
                    (mask & (1 << index) != 0).then_some(
                        [
                            pose.x,
                            pose.y,
                            pose.z,
                            pose.rot_x_deg,
                            pose.rot_z_deg,
                            pose.rot_y_deg,
                            pose.zoom,
                            pose.zoom_x,
                            pose.zoom_y,
                            pose.zoom_z,
                            pose.skew_x,
                            pose.skew_y,
                        ][index],
                    )
                });
                Ok::<_, Error>(raw.or(native))
            };
            let zoom = number(call, "__songlua_state_zoom", 6)?;
            Ok(Player {
                x: number(call, "__songlua_state_x", 0)?,
                y: number(call, "__songlua_state_y", 1)?,
                z: number(call, "__songlua_state_z", 2)?,
                rotation_x: number(call, "__songlua_state_rot_x_deg", 3)?,
                rotation_z: number(call, "__songlua_state_rot_z_deg", 4)?,
                rotation_y: number(call, "__songlua_state_rot_y_deg", 5)?,
                zoom_x: number(call, "__songlua_state_zoom_x", 7)?.or(zoom),
                zoom_y: number(call, "__songlua_state_zoom_y", 8)?.or(zoom),
                zoom_z: number(call, "__songlua_state_zoom_z", 9)?.or(zoom),
                skew_x: number(call, "__songlua_state_skew_x", 10)?,
                skew_y: number(call, "__songlua_state_skew_y", 11)?,
            })
        })
    }
    pub fn reset(&self, vm: &mut Vm, actor: Value) -> Result<(), Error> {
        self.read(vm, actor, function_capture::reset)
    }
    pub fn reset_frames(
        &self,
        vm: &mut Vm,
        actors: impl IntoIterator<Item = Value>,
    ) -> Result<(), Error> {
        // Baked replay only resets raw fields and replaces the blocks table.
        let baked = !vm
            .store_ref()
            .clock
            .live
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut reset = function_capture::BakedFrameReset::new();
        for actor in actors {
            if baked && let Value::Table(actor) = actor {
                reset.reset(vm, actor)?;
            } else {
                self.read(vm, actor, function_capture::reset_frame)?;
            }
        }
        Ok(())
    }
    pub fn is_player(
        &self,
        vm: &mut Vm,
        actor: Value,
        player_index_key: Value,
    ) -> Result<bool, Error> {
        if let Value::Table(id) = actor {
            if vm.raw_table_get(id, player_index_key)? == Value::Nil {
                return Ok(false);
            }
        }
        self.read(vm, actor, is_player)
    }
    pub fn update_getter(
        &self,
        vm: &mut Vm,
        actor: Value,
        target: crate::SongLuaOverlayUpdateTarget,
        value: &crate::SongLuaOverlayUpdateValue,
    ) -> Result<(), Error> {
        let callback = vm.new_host_function_with_data(
            super::song_getters::write,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            (target, value.clone()),
        )?;
        vm.call(callback, &[])?;
        Ok(())
    }
    /// The caller keeps the actors rooted for the whole batch. The callback
    /// consumes its writes before running any authored assignment hook.
    pub fn update_rooted_getters(
        &self,
        vm: &mut Vm,
        writes: Vec<super::song_getters::GetterWrite>,
    ) -> Result<(), Error> {
        if writes.is_empty() {
            return Ok(());
        }
        let callback = vm.new_host_function_with_data(
            super::song_getters::write_batch,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Mutex::new(writes),
        )?;
        vm.call(callback, &[]).map(|_| ())
    }
    pub fn write(&self, vm: &mut Vm, actor: Value, state: State) -> Result<(), Error> {
        let callback = vm.new_host_function_with_data(
            write,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            state,
        )?;
        vm.call(callback, &[]).map(|_| ())
    }
    pub fn write_frame(&self, vm: &mut Vm, actor: Value, state: State) -> Result<(), Error> {
        let callback = match &vm.store_ref().perframe_state_writer {
            Some(writer) => writer.callback,
            None => {
                let function = vm.new_host_function(
                    write_frame,
                    &[Value::Table(self.registry), Value::Table(self.helpers)],
                )?;
                let callback = vm.pin(function)?;
                vm.store().perframe_state_writer = Some(StateWrite {
                    callback,
                    pending: None,
                });
                callback
            }
        };
        let function = vm.rooted(callback)?;
        vm.store().perframe_state_writer.as_mut().unwrap().pending = Some((actor, state));
        let result = vm.call(function, &[actor]).map(|_| ());
        // A VM failure can occur before the callback consumes the state.
        vm.store().perframe_state_writer.as_mut().unwrap().pending = None;
        result
    }
    pub fn motion(&self, vm: &mut Vm, actor: Value, epoch: f32, seconds: f32) {
        if let Value::Table(id) = actor
            && let Some(actor) = vm.store().actors.get_mut(&id)
        {
            effects::sync_preparation_motion(actor, epoch, seconds);
        }
    }
}
pub(super) fn is_player(call: &mut HostCall<'_>, actor: TableId) -> Result<bool, Error> {
    let key = call.intern_static(b"__songlua_player_index")?;
    let value = call.raw_table_get(actor, key)?;
    if value == Value::Nil {
        return Ok(false);
    }
    song_columns::numeric(call, value, song_columns::Numeric::Integer)?;
    let key = call.intern_static(b"__songlua_player_child_name")?;
    let value = call.raw_table_get(actor, key)?;
    Ok(initial_state::value_text(call, value)?.is_none())
}
fn number(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &'static str,
) -> Result<Option<f32>, Error> {
    let value = sprites::field(call, actor, key)?;
    if value == Value::Nil {
        return Ok(None);
    }
    sprites::number(call, value).map(|n| Some(n as f32))
}
fn assign(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &'static str,
    value: Value,
    state: &State,
    frame: bool,
) -> Result<(), Error> {
    sprites::set_static(call, actor, key, value)?;
    let guard = call.store();
    let Some(actor) = guard.actors.get_mut(&actor) else {
        return Ok(());
    };
    match key {
        "__songlua_visible" => actor.state.visible = state.visible,
        "__songlua_aux" | "__songlua_current_x" | "__songlua_current_y" | "__songlua_current_z" => {
            let (axis, value) = match key {
                "__songlua_current_x" => (0, state.x),
                "__songlua_current_y" => (1, state.y),
                "__songlua_current_z" => (2, state.z),
                _ => (3, state.aux),
            };
            let current = actor.current;
            actor.preparation_getters.get_or_insert([
                current.x,
                current.y,
                current.z,
                current.aux,
            ])[axis] = value;
        }
        _ => {
            use crate::SongLuaOverlayUpdateTarget as T;
            let target = match key {
                "__songlua_state_aux" => T::Aux,
                "__songlua_state_x" => T::X,
                "__songlua_state_y" => T::Y,
                "__songlua_state_z" => T::Z,
                "__songlua_state_zoom" => T::Zoom,
                "__songlua_state_zoom_x" => T::ZoomX,
                "__songlua_state_zoom_y" => T::ZoomY,
                "__songlua_state_zoom_z" => T::ZoomZ,
                "__songlua_state_rot_x_deg" => T::RotationX,
                "__songlua_state_rot_y_deg" => T::RotationY,
                "__songlua_state_rot_z_deg" => T::RotationZ,
                "__songlua_state_quaternion" => T::Quaternion,
                "__songlua_state_diffuse" => T::Diffuse,
                "__songlua_state_glow" => T::Glow,
                "__songlua_state_basezoom" => T::BaseZoom,
                "__songlua_state_basezoom_x" => T::BaseZoomX,
                "__songlua_state_basezoom_y" => T::BaseZoomY,
                "__songlua_state_basezoom_z" => T::BaseZoomZ,
                "__songlua_state_base_alpha" => T::BaseAlpha,
                "__songlua_state_base_rotation" => T::BaseRotation,
                "__songlua_state_halign" => T::HAlign,
                "__songlua_state_valign" => T::VAlign,
                "__songlua_state_sprite_texture" => T::SpriteTexture,
                "__songlua_state_aft_preserve" => T::AftPreserve,
                "__songlua_state_aft_created" => T::AftCreated,
                "__songlua_state_effect_mode" => T::EffectMode,
                "__songlua_state_effect_clock" => T::EffectClock,
                "__songlua_state_effect_timer" => T::EffectTimer,
                "__songlua_state_effect_no_offset" => T::EffectNoOffset,
                "__songlua_state_effect_period" => T::EffectPeriod,
                "__songlua_state_effect_magnitude" => T::EffectMagnitude,
                "__songlua_state_size" => T::Size,
                "__songlua_state_stretch_rect" => T::StretchRect,
                _ => return Ok(()),
            };
            let value = crate::perframe::overlay_state_update_value(state, target);
            host_compat::update_base(actor, target, &value);
            crate::perframe::set_overlay_state_update_value(
                &mut actor.state.render,
                target,
                &value,
            );
            // The native frame handoff exports Lua getters without editing
            // ActorTweenReplay's current pose or any queued destination.
            if !(frame && actor.capture.is_none() && actor.replay.is_some()) {
                actor.destination_mut().set(target, &value);
            }
            match target {
                T::RotationX => actor.state.rotation[0] = state.rot_x_deg,
                T::RotationY => actor.state.rotation[1] = state.rot_y_deg,
                T::RotationZ => actor.state.rotation[2] = state.rot_z_deg,
                _ => {}
            }
        }
    }
    Ok(())
}
fn write(call: &mut HostCall<'_>) -> Result<(), Error> {
    let state = *call.function_data::<State>().unwrap();
    let actor = table(call, call.captures()[2])?;
    write_state(call, actor, state, false)
}

fn write_frame(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = arg(call, 0);
    let actor = table(call, value)?;
    let state = call
        .store()
        .perframe_state_writer
        .as_mut()
        .and_then(|writer| {
            if writer
                .pending
                .as_ref()
                .is_some_and(|(actor, _)| *actor == value)
            {
                writer.pending.take().map(|(_, state)| state)
            } else {
                None
            }
        })
        .ok_or_else(|| call.error(b"actor frame writer has no pending state"))?;
    // The actor remains rooted in the arguments while setters can reenter Lua.
    write_state(call, actor, state, true)
}

fn write_state(
    call: &mut HostCall<'_>,
    actor: TableId,
    s: State,
    frame: bool,
) -> Result<(), Error> {
    macro_rules! set {
        ($key:literal, $value:expr) => {
            assign(call, actor, $key, $value, &s, frame)?
        };
    }
    macro_rules! scalar {
        ($key:literal, $value:expr) => {
            set!($key, Value::Number(f64::from($value)))
        };
    }
    set!("__songlua_visible", Value::Boolean(s.visible));
    scalar!("__songlua_state_aux", s.aux);
    scalar!("__songlua_aux", s.aux);
    set!(
        "__songlua_state_sprite_texture",
        Value::Boolean(s.sprite_texture)
    );
    set!(
        "__songlua_state_aft_preserve",
        Value::Boolean(s.aft_preserve)
    );
    set!("__songlua_state_aft_created", Value::Boolean(s.aft_created));
    scalar!("__songlua_state_x", s.x);
    scalar!("__songlua_state_y", s.y);
    scalar!("__songlua_state_z", s.z);
    scalar!("__songlua_current_x", s.x);
    scalar!("__songlua_current_y", s.y);
    scalar!("__songlua_current_z", s.z);
    scalar!("__songlua_state_zoom", s.zoom);
    scalar!("__songlua_state_zoom_x", s.zoom_x);
    scalar!("__songlua_state_zoom_y", s.zoom_y);
    scalar!("__songlua_state_zoom_z", s.zoom_z);
    scalar!("__songlua_state_basezoom", s.basezoom);
    scalar!("__songlua_state_basezoom_x", s.basezoom_x);
    scalar!("__songlua_state_basezoom_y", s.basezoom_y);
    scalar!("__songlua_state_basezoom_z", s.basezoom_z);
    scalar!("__songlua_state_base_alpha", s.base_alpha);
    let value = sprites::vector_value(call, &s.quaternion)?;
    set!("__songlua_state_quaternion", value);
    scalar!("__songlua_state_rot_x_deg", s.rot_x_deg);
    scalar!("__songlua_state_rot_y_deg", s.rot_y_deg);
    scalar!("__songlua_state_rot_z_deg", s.rot_z_deg);
    let value = sprites::vector_value(call, &s.base_rotation)?;
    set!("__songlua_state_base_rotation", value);
    scalar!("__songlua_state_halign", s.halign);
    scalar!("__songlua_state_valign", s.valign);
    let value = call.intern_static(crate::effect_mode_label(s.effect_mode).as_bytes())?;
    set!("__songlua_state_effect_mode", value);
    let value = call.intern_static(crate::effect_clock_label(s.effect_clock).as_bytes())?;
    set!("__songlua_state_effect_clock", value);
    set!(
        "__songlua_state_effect_timer",
        Value::Boolean(s.effect_timer)
    );
    set!(
        "__songlua_state_effect_no_offset",
        Value::Boolean(s.effect_no_offset)
    );
    scalar!("__songlua_state_effect_period", s.effect_period);
    let value = sprites::vector_value(call, &s.diffuse)?;
    call.with_roots(&[value], |call| {
        assign(call, actor, "__songlua_state_diffuse", value, &s, frame)?;
        assign(call, actor, "__songlua_diffuse", value, &s, frame)
    })?;
    let value = sprites::vector_value(call, &s.glow)?;
    set!("__songlua_state_glow", value);
    let value = sprites::vector_value(call, &s.effect_magnitude)?;
    set!("__songlua_state_effect_magnitude", value);
    if let Some(size) = s.size {
        let value = sprites::vector_value(call, &size)?;
        set!("__songlua_state_size", value);
    }
    if let Some(rect) = s.stretch_rect {
        let value = sprites::vector_value(call, &rect)?;
        set!("__songlua_state_stretch_rect", value);
    }
    let known = call.store().actors.contains_key(&actor);
    if frame && known {
        song_replay::sync_frame_getters(call, actor)?;
    } else if known {
        song_replay::sync_getters(call, actor)?;
    }
    Ok(())
}

#[cfg(feature = "test-support")]
impl Access {
    pub fn fixture_children(&self, vm: &mut Vm, actor: Value) -> Result<Vec<Value>, Error> {
        self.read(vm, actor, |call, actor| {
            native_children::children(call, actor)
                .map(|children| children.into_iter().map(Value::Table).collect())
        })
    }
    pub fn fixture_sprite_states(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<Vec<crate::SongLuaSpriteState>, Error> {
        self.read(vm, actor, |call, actor| sprites::states(call, actor))
    }
    pub fn fixture_init_state(&self, vm: &mut Vm, actor: Value) -> Result<(), Error> {
        self.read(vm, actor, live_layers::reset_actor)
    }
}

#[cfg(feature = "test-support")]
impl Access {
    pub fn fixture_named_children(&self, vm: &mut Vm, actor: Value) -> Result<Root, Error> {
        let function = vm.new_host_function(
            |call| {
                let actor = table(call, call.captures()[2])?;
                let children = native_children::named_children(call, actor)?;
                call.push(children)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
        )?;
        let children = vm
            .call(function, &[])?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        vm.pin(children)
    }

    /// One snapshot entry through the production lookup, and whether that
    /// lookup answered without building the snapshot.
    pub fn fixture_named_child(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &[u8],
    ) -> Result<(Root, bool), Error> {
        let name = vm.try_intern(name)?;
        let function = vm.new_host_function(
            |call| {
                let actor = table(call, call.captures()[2])?;
                let name = bytes(call, call.captures()[3])?;
                let direct = native_children::find_named(call, actor, &name)?.is_some();
                let child = native_children::named_child(call, actor, &name)?;
                call.push(child)?;
                call.push(Value::Boolean(direct))
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
                name,
            ],
        )?;
        let results = vm.call(function, &[])?;
        let (child, direct) = (results[0], results[1] == Value::Boolean(true));
        Ok((vm.pin(child)?, direct))
    }

    /// A table's manual-draw snapshot fields, copied by the Lua helper
    /// (`helper`) or by the production path, and whether the production path
    /// answered without the helper.
    pub fn fixture_state_copy(
        &self,
        vm: &mut Vm,
        actor: Value,
        mutable: bool,
        helper: bool,
    ) -> Result<(Root, bool), Error> {
        let function = vm.new_host_function_with_data(
            |call| {
                let (mutable, helper) = *call.function_data::<(bool, bool)>().unwrap();
                let actor = table(call, call.captures()[2])?;
                let (copy, native) = if helper {
                    let copy = state_snapshot::copy_with_helper(call, actor, mutable)?;
                    (copy, false)
                } else {
                    let native = state_snapshot::copy_native(call, actor, mutable)?.is_some();
                    (state_snapshot::copy(call, actor, mutable)?, native)
                };
                call.push(Value::Table(copy))?;
                call.push(Value::Boolean(native))
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            (mutable, helper),
        )?;
        let results = vm.call(function, &[])?;
        let (copy, native) = (results[0], results[1] == Value::Boolean(true));
        Ok((vm.pin(copy)?, native))
    }
}
