//! Observations used by the production chronological compiler.
use super::*;
use crate::SongLuaOverlayState as State;

pub(super) struct WrapperCapture {
    start: usize,
    seen: usize,
    rows: Vec<(TableId, TableId, State)>,
}
#[derive(Clone, Copy)]
pub(crate) enum Query {
    Active,
    Motion([f32; 2], bool),
    Mask,
    Field([f32; 2]),
}
pub(crate) enum Answer {
    Active(bool),
    Motion(Option<[f32; 2]>, Option<[f32; 3]>),
    Mask(u16),
    Field(Option<Vec<State>>),
}
#[derive(Clone)]
struct Read {
    query: Query,
    output: Arc<Mutex<Option<Answer>>>,
}
impl Actors {
    pub(crate) fn apply_song_replay_pose(&self, vm: &Vm, value: Value, state: &mut State) -> bool {
        let Value::Table(id) = value else {
            return false;
        };
        let store = vm.store_ref();
        let Some(actor) = store
            .actors
            .get(&id)
            .filter(|a| a.capture.is_none() && a.replay.is_some())
        else {
            return false;
        };
        actor.current.write_to(state);
        true
    }
    pub(crate) fn wrapper_parent(&self, vm: &mut Vm, actor: TableId) -> Option<TableId> {
        vm.store().actors.get(&actor).and_then(|a| a.parent)
    }
    pub(crate) fn replace_song_capture_indices(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        values: &[Value],
    ) {
        let store = vm.store();
        let capture = store
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages
            .as_mut()
            .unwrap();
        capture.indices = values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                if let Value::Table(id) = v {
                    Some((*id, i))
                } else {
                    None
                }
            })
            .collect();
    }
    pub(crate) fn song_message_advances(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Vec<((usize, String), f32)> {
        vm.store().scenes[&screen.id]
            .song_session
            .as_ref()
            .unwrap()
            .messages
            .as_ref()
            .unwrap()
            .advances
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }
    pub(crate) fn chronological_read(
        &self,
        vm: &mut Vm,
        actor: Value,
        query: Query,
    ) -> Result<Answer, Error> {
        let output = Arc::new(Mutex::new(None));
        let function = vm.new_host_function_with_data(
            read,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            Read {
                query,
                output: output.clone(),
            },
        )?;
        vm.call(function, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
    pub(crate) fn models_active(&self, vm: &mut Vm) -> bool {
        vm.store()
            .actors
            .values()
            .any(|a| a.model_materials.iter().any(Option::is_some))
    }
    pub(crate) fn begin_song_wrapper_capture(&self, vm: &mut Vm, screen: ActorScene, start: usize) {
        let store = vm.store();
        let capture = store
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages
            .as_mut()
            .unwrap();
        capture.wrappers = Some(WrapperCapture {
            start,
            seen: 0,
            rows: Vec::new(),
        });
    }
    pub(crate) fn new_song_wrappers(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Vec<(TableId, TableId, State)> {
        let store = vm.store();
        let capture = store
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages
            .as_mut()
            .unwrap();
        let Some(wrappers) = capture.wrappers.as_mut() else {
            return Vec::new();
        };
        let result = wrappers.rows[wrappers.seen..].to_vec();
        wrappers.seen = wrappers.rows.len();
        result
    }
    pub(crate) fn finish_song_wrapper_capture(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Vec<(TableId, TableId, State)> {
        vm.store()
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages
            .as_mut()
            .unwrap()
            .wrappers
            .take()
            .map_or_else(Vec::new, |wrappers| wrappers.rows)
    }
    pub(crate) fn end_song_update_capture(&self, vm: &mut Vm, screen: ActorScene) {
        let store = vm.store();
        if store
            .replay_frame
            .is_some_and(|(owner, _)| owner == screen.id)
        {
            store.replay_frame = None;
            store.replay_clock = None;
            if let Some(session) = store
                .scenes
                .get_mut(&screen.id)
                .and_then(|s| s.song_session.as_mut())
            {
                session.messages = None;
            }
        }
    }
}
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Read>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let answer = match data.query {
        Query::Active => Answer::Active(active(call, actor)?),
        Query::Motion(clock, include_spin) => {
            let time = effect_time(call, actor, clock)?;
            let spin = if include_spin {
                render_spin(call, actor)?
            } else {
                None
            };
            Answer::Motion(time, spin)
        }
        Query::Mask => Answer::Mask(mask(call, actor)?),
        Query::Field(clock) => Answer::Field(field_wrappers(call, actor, clock)?),
    };
    *data.output.lock().unwrap() = Some(answer);
    Ok(())
}
fn boolean(call: &HostCall<'_>, value: Value) -> Result<Option<bool>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Boolean(b) => Ok(Some(b)),
        _ => Err(call.error(b"boolean expected")),
    }
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}
fn replay_pose(store: &mut Store, actor: TableId, state: &mut State) {
    let store = &mut *store;
    if let Some(actor) = store
        .actors
        .get(&actor)
        .filter(|a| a.capture.is_none() && a.replay.is_some())
    {
        actor.current.write_to(state);
    }
}
pub(super) fn ensure(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if call
        .store()
        .actors
        .get(&actor)
        .is_some_and(|a| a.capture.is_none() && a.replay.is_some())
    {
        return Ok(());
    }
    let state = initial_state::read(call, actor)?;
    let store = call.store();
    if !store.playback {
        return Ok(());
    }
    let live = store.clock.live.load(std::sync::atomic::Ordering::Relaxed);
    if let Some(actor) = store.actors.get_mut(&actor) {
        actor.current = state.into();
        if live {
            actor.state.render = state;
        }
        actor.queue.clear();
        actor.capture = None;
        actor.replay = Some(song_replay::State::default());
    }
    if let Some(clock) = store.replay_clock.as_mut() {
        clock.promote(actor);
    }
    if live {
        // A copied loading definition exports raw metadata. Once the native
        // replay owns those values, remove the mirrors just as initial live
        // promotion does; their presence must not override later native writes.
        for key in crate::render_flags::RenderFlag::ALL
            .into_iter()
            .map(crate::render_flags::RenderFlag::state_key)
            .chain(
                crate::base_transform::BaseField::ALL
                    .into_iter()
                    .map(crate::base_transform::BaseField::state_key),
            )
        {
            let key = call.try_intern(key.as_bytes())?;
            call.raw_table_set(actor, key, Value::Nil)?;
        }
    }
    Ok(())
}
pub(super) fn created(
    call: &mut HostCall<'_>,
    owner: TableId,
    wrapper: TableId,
) -> Result<(), Error> {
    ensure(call, wrapper)?;
    let context = {
        let store = call.store();
        store.replay_frame.and_then(|(scene, _)| {
            let capture = store
                .scenes
                .get(&scene)?
                .song_session
                .as_ref()?
                .messages
                .as_ref()?;
            (capture.wrappers.is_some() && capture.indices.contains_key(&owner)).then_some(scene)
        })
    };
    let Some(scene) = context else { return Ok(()) };
    let initial = initial_state::read(call, wrapper)?;
    let store = call.store();
    let capture = store
        .scenes
        .get_mut(&scene)
        .unwrap()
        .song_session
        .as_mut()
        .unwrap()
        .messages
        .as_mut()
        .unwrap();
    let wrappers = capture.wrappers.as_mut().unwrap();
    capture
        .indices
        .insert(wrapper, wrappers.start + wrappers.rows.len());
    wrappers.rows.push((owner, wrapper, initial));
    Ok(())
}
fn visit_wrappers(
    call: &mut HostCall<'_>,
    actor: TableId,
    ordinary: bool,
    reverse: bool,
    mut visit: impl FnMut(&mut HostCall<'_>, TableId) -> Result<bool, Error>,
) -> Result<bool, Error> {
    let value = if ordinary {
        sprites::field(call, actor, "__songlua_wrappers")?
    } else {
        get(call, actor, b"__songlua_wrappers")?
    };
    let array = optional_table(call, value)?;
    call.with_roots(&[value], |call| {
        let len = if let Some(array) = array {
            call.raw_table_len(array)?
        } else {
            call.store()
                .actors
                .get(&actor)
                .map_or(0, |a| a.wrappers.len())
        };
        let mut index = if reverse { len } else { 1 };
        loop {
            if reverse && index == 0 {
                return Ok(false);
            }
            let value = if let Some(array) = array {
                call.raw_table_get(array, Value::Number(index as f64))?
            } else {
                call.store()
                    .actors
                    .get(&actor)
                    .and_then(|a| a.wrappers.get(index - 1))
                    .copied()
                    .map_or(Value::Nil, Value::Table)
            };
            if value == Value::Nil && !reverse {
                return Ok(false);
            }
            let wrapper = table(call, value)?;
            if call.with_roots(&[value], |call| visit(call, wrapper))? {
                return Ok(true);
            }
            if reverse { index -= 1 } else { index += 1 }
        }
    })
}

fn active(call: &mut HostCall<'_>, actor: TableId) -> Result<bool, Error> {
    let sleep = get(call, actor, b"__songlua_hibernate_seconds")?;
    let sleeping = if sleep == Value::Nil {
        call.store()
            .actors
            .get(&actor)
            .is_some_and(|a| a.hibernate > 0.)
    } else {
        sprites::number(call, sleep)? as f32 > 0.
    };
    if sleeping {
        return Ok(true);
    }
    for key in ["__songlua_update_function", "__songlua_draw_function"] {
        let value = sprites::field(call, actor, key)?;
        if value != Value::Nil {
            if !matches!(value, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"function expected"));
            }
            return Ok(true);
        }
    }
    let recurring = sprites::field(call, actor, "__songlua_recurring_update_command")?;
    if if recurring == Value::Nil {
        capture::has_recurring(call.store(), actor)
    } else {
        initial_state::value_text(call, recurring)?.is_some()
    } {
        return Ok(true);
    }
    let clock = get(call, actor, b"__songlua_state_motion_clock")?;
    if optional_table(call, clock)?.is_some()
        || call
            .store()
            .actors
            .get(&actor)
            .is_some_and(effects::has_motion)
    {
        return Ok(true);
    }
    let deferred = get(call, actor, b"__songlua_command_queue_deferred")?;
    if if deferred == Value::Nil {
        capture::deferred(call.store(), actor)
    } else {
        !matches!(deferred, Value::Boolean(false))
    } {
        return Ok(true);
    }
    if visit_wrappers(call, actor, true, false, |call, row| active(call, row))? {
        return Ok(true);
    }
    let mut index = 1;
    loop {
        let value = call.raw_table_get(actor, Value::Number(index as f64))?;
        if value == Value::Nil {
            break;
        }
        if let Value::Table(child) = value
            && call.with_roots(&[value], |call| active(call, child))?
        {
            return Ok(true);
        }
        index += 1;
    }
    let stream = call
        .store()
        .actors
        .get(&actor)
        .filter(|a| a.state.class == "SongMeterDisplay")
        .and_then(|a| a.children.first())
        .copied();
    if let Some(stream) = stream {
        return active(call, stream);
    }
    Ok(false)
}
fn mask(call: &mut HostCall<'_>, actor: TableId) -> Result<u16, Error> {
    let mut mask = 0;
    let mut block = |call: &mut HostCall<'_>, block: TableId| -> Result<(), Error> {
        for (i, key) in crate::perframe::PLAYER_TRANSFORM_CAPTURE_KEYS
            .iter()
            .enumerate()
        {
            if get(call, block, key.as_bytes())? != Value::Nil {
                mask |= 1 << i;
            }
        }
        Ok(())
    };
    let value = sprites::field(call, actor, "__songlua_capture_block")?;
    if let Some(id) = optional_table(call, value)? {
        call.with_roots(&[value], |call| block(call, id))?;
    }
    let value = sprites::field(call, actor, "__songlua_capture_blocks")?;
    if let Some(id) = optional_table(call, value)? {
        call.with_roots(&[value], |call| {
            let mut i = 1;
            loop {
                let v = call.raw_table_get(id, Value::Number(i as f64))?;
                if v == Value::Nil {
                    break;
                }
                let id = table(call, v)?;
                call.with_roots(&[v], |call| block(call, id))?;
                i += 1;
            }
            Ok(())
        })?;
    }
    let guard = call.store();
    if let Some(a) = guard.actors.get(&actor) {
        let fields = a.capture.as_ref().map_or(0, |c| c.transform_fields);
        // Capture uses one extra slot for uniform zoom; the renderer has one
        // presence bit per zoom axis.
        for (dst, src) in [0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11].into_iter().enumerate() {
            if fields & (1 << src) != 0 {
                mask |= 1 << dst;
            }
        }
        if fields & (1 << 6) != 0 {
            mask |= (1 << 6) | (1 << 7) | (1 << 8);
        }
        let targets = a.replay.as_ref().map_or(0, |r| r.targets);
        for (i, t) in crate::perframe::PLAYER_TRANSFORM_TARGETS
            .into_iter()
            .enumerate()
        {
            if targets & (1u128 << t as usize) != 0 {
                mask |= 1 << i;
            }
        }
    }
    Ok(mask)
}
pub(super) fn render_spin(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<[f32; 3]>, Error> {
    let fallback = {
        let guard = call.store();
        guard
            .actors
            .get(&actor)
            .filter(|a| a.effects.spin_baked)
            .map(|a| {
                if song_pose::mode(&guard, actor)
                    && let Some(rotation) = capture::spin_rotation(a)
                {
                    return rotation;
                }
                [
                    a.current.rot_x_deg,
                    a.current.rot_y_deg,
                    a.current.rot_z_deg,
                ]
            })
    };
    let value = get(call, actor, b"__songlua_state_spin")?;
    let Some(pose) = optional_table(call, value)? else {
        return Ok(fallback);
    };
    call.with_roots(&[value], |call| {
        let value = sprites::field(call, pose, "baked")?;
        if !boolean(call, value)?.unwrap_or(false) {
            return Ok(None);
        }
        let value = sprites::field(call, pose, "rotation")?;
        let rotation = table(call, value)?;
        call.with_roots(&[value], |call| {
            let mut result = [0.; 3];
            for (i, out) in result.iter_mut().enumerate() {
                let value = call.raw_table_get(rotation, Value::Number((i + 1) as f64))?;
                *out = sprites::number(call, value)? as f32;
            }
            Ok(Some(result))
        })
    })
}
fn effect_time(
    call: &mut HostCall<'_>,
    actor: TableId,
    clock: [f32; 2],
) -> Result<Option<[f32; 2]>, Error> {
    let fallback = {
        let guard = call.store();
        guard.actors.get(&actor).map(|a| {
            let mut state = a.state.render;
            effects::render_time(a, clock, &mut state);
            state
        })
    };
    let value = get(call, actor, b"__songlua_state_rainbow")?;
    let rainbow = boolean(call, value)?.unwrap_or_else(|| fallback.is_some_and(|s| s.rainbow));
    if !rainbow {
        let value = get(call, actor, b"__songlua_state_effect_mode")?;
        let selected = if value == Value::Nil {
            fallback.is_some_and(|s| {
                !matches!(
                    s.effect_mode,
                    deadlib_present::anim::EffectMode::None
                        | deadlib_present::anim::EffectMode::Spin
                )
            })
        } else {
            initial_state::value_text(call, value)?.is_some_and(|s| {
                matches!(
                    s.as_str(),
                    "bob"
                        | "bounce"
                        | "wag"
                        | "pulse"
                        | "diffuseblink"
                        | "diffuseshift"
                        | "diffuseramp"
                        | "glowblink"
                        | "glowshift"
                        | "glowramp"
                )
            })
        };
        if !selected {
            return Ok(None);
        }
    }
    let value = get(call, actor, b"__songlua_state_motion_clock")?;
    let Some(motion) = optional_table(call, value)? else {
        return Ok(fallback.and_then(|s| s.effect_time));
    };
    call.with_roots(&[value], |call| {
        let value = get(call, actor, b"__songlua_state_effect_clock")?;
        let beat = initial_state::value_text(call, value)?
            .is_some_and(|s| matches!(s.as_str(), "beat" | "beatnooffset"));
        let value = sprites::field(call, motion, "units")?;
        let units = if value == Value::Nil {
            0.
        } else {
            sprites::number(call, value)? as f32
        };
        Ok(Some([clock[usize::from(beat)], units]))
    })
}

fn field_wrappers(
    call: &mut HostCall<'_>,
    player: TableId,
    clock: [f32; 2],
) -> Result<Option<Vec<State>>, Error> {
    let children = native_children::named_children(call, player)?;
    call.with_roots(&[children], |call| {
        let children = table(call, children)?;
        let value = get(call, children, b"NoteField")?;
        let Some(field) = optional_table(call, value)? else {
            return Ok(None);
        };
        call.with_roots(&[value], |call| {
            let mut states = Vec::new();
            visit_wrappers(call, field, false, true, |call, id| {
                ensure(call, id)?;
                let mut state = initial_state::read(call, id)?;
                replay_pose(call.store(), id, &mut state);
                if let Some(spin) = render_spin(call, id)? {
                    [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = spin;
                    state.spin_baked = true;
                }
                if let Some(time) = effect_time(call, id, clock)? {
                    state.effect_time = Some(time);
                }
                states.push(state);
                Ok(false)
            })?;
            Ok(Some(states))
        })
    })
}
