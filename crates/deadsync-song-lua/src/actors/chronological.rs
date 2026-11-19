//! Observations used by the production chronological compiler.
use super::fields::Field;
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
    Hud([f32; 2]),
}
pub(crate) enum Answer {
    Active(bool),
    Motion(Option<[f32; 2]>, Option<[f32; 3]>),
    Mask(u16),
    /// NoteField wrappers (`None` without a NoteField), then the Player's own
    /// wrappers. Both are ordered outermost first.
    Field(Option<Vec<State>>, Vec<State>),
    /// Authored Judgment/Combo wrappers followed by the child's resolved pose;
    /// empty when the theme still owns the untouched child's placement.
    Hud(Vec<State>),
}
#[derive(Clone)]
struct Read {
    query: Query,
    output: Arc<Mutex<Vec<Answer>>>,
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
        let mut output = Vec::new();
        self.chronological_read_many(vm, &[actor], query, &mut output)?;
        Ok(output.pop().unwrap())
    }
    /// Keep actor order and authored getter behavior while sharing rooted host
    /// calls across a frame's observations. Reuse the caller's result buffer.
    pub(crate) fn chronological_read_many(
        &self,
        vm: &mut Vm,
        actors: &[Value],
        query: Query,
        answers: &mut Vec<Answer>,
    ) -> Result<(), Error> {
        answers.clear();
        let output = Arc::new(Mutex::new(std::mem::take(answers)));
        // Lua closures allow 255 captures; the first two belong to the host.
        let mut captures = Vec::with_capacity(actors.len().min(253) + 2);
        captures.extend([Value::Table(self.registry), Value::Table(self.helpers)]);
        let result = (|| {
            for actors in actors.chunks(253) {
                captures.truncate(2);
                captures.extend_from_slice(actors);
                let function = vm.new_host_function_with_data(
                    read,
                    &captures,
                    Read {
                        query,
                        output: output.clone(),
                    },
                )?;
                vm.call(function, &[])?;
            }
            Ok(())
        })();
        *answers = std::mem::take(&mut *output.lock().unwrap());
        result
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
    // A getter may call back into Lua; never hold the output lock across it.
    let mut answers = std::mem::take(&mut *data.output.lock().unwrap());
    for index in 2..call.captures().len() {
        let actor = table(call, call.captures()[index])?;
        answers.push(read_actor(call, actor, data.query)?);
    }
    *data.output.lock().unwrap() = answers;
    Ok(())
}
fn read_actor(call: &mut HostCall<'_>, actor: TableId, query: Query) -> Result<Answer, Error> {
    Ok(match query {
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
        Query::Field(clock) => Answer::Field(
            field_wrappers(call, actor, clock)?,
            wrapper_states(call, actor, clock)?,
        ),
        Query::Hud(clock) => {
            let mut states = wrapper_states(call, actor, clock)?;
            if !states.is_empty() || call.store().actors[&actor].authored_transform {
                states.push(render_state(call, actor, clock)?);
            }
            // These are native poses with absolute axis scales. Preparation's
            // redundant uniform aliases must not replace an explicit unit axis.
            for state in &mut states {
                initial_state::normalize_live_scale(state);
            }
            Answer::Hud(states)
        }
    })
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
        get_static(call, actor, b"__songlua_wrappers")?
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
    // A movie advances in Sprite::Update even without an authored callback.
    // Retain paused movies too: queued commands/messages can resume them later.
    if call
        .store()
        .actors
        .get(&actor)
        .and_then(|a| a.sprite_animation.as_ref())
        .is_some_and(sprite_animation::State::is_movie)
    {
        return Ok(true);
    }
    let sleep = get_static(call, actor, b"__songlua_hibernate_seconds")?;
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
    let clock = get_static(call, actor, b"__songlua_state_motion_clock")?;
    if optional_table(call, clock)?.is_some()
        || call
            .store()
            .actors
            .get(&actor)
            .is_some_and(effects::has_motion)
    {
        return Ok(true);
    }
    let deferred = get_static(call, actor, b"__songlua_command_queue_deferred")?;
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
    let value = fields::get(call, actor, fields::Field::STATE_SPIN)?;
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
            let state = &a.state.render;
            (
                state.rainbow,
                state.effect_mode,
                effects::render_effect_time(a, clock, state),
            )
        })
    };
    let [value, mode] = fields::get_all(
        call,
        actor,
        [Field::STATE_RAINBOW, Field::STATE_EFFECT_MODE],
    )?;
    let rainbow = boolean(call, value)?.unwrap_or_else(|| fallback.is_some_and(|s| s.0));
    if !rainbow {
        let value = mode;
        let selected = if value == Value::Nil {
            fallback.is_some_and(|s| {
                !matches!(
                    s.1,
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
    let value = fields::get(call, actor, Field::STATE_MOTION_CLOCK)?;
    let Some(motion) = optional_table(call, value)? else {
        return Ok(fallback.and_then(|s| s.2));
    };
    call.with_roots(&[value], |call| {
        let value = fields::get(call, actor, Field::STATE_EFFECT_CLOCK)?;
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
    let value = native_children::named_child(call, player, b"NoteField")?;
    let Some(field) = optional_table(call, value)? else {
        return Ok(None);
    };
    call.with_roots(&[value], |call| wrapper_states(call, field, clock).map(Some))
}

/// The current render pose of each wrapper state of `actor`, outermost first.
fn wrapper_states(
    call: &mut HostCall<'_>,
    actor: TableId,
    clock: [f32; 2],
) -> Result<Vec<State>, Error> {
    let mut states = Vec::new();
    visit_wrappers(call, actor, false, true, |call, id| {
        states.push(render_state(call, id, clock)?);
        Ok(false)
    })?;
    Ok(states)
}

fn render_state(call: &mut HostCall<'_>, actor: TableId, clock: [f32; 2]) -> Result<State, Error> {
    if call
        .store_ref()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        // Live actors already own their current pose. Promoting a wrapper from
        // preparation metadata here would mutate it while publishing the frame.
        return initial_state::read_live(call, actor);
    }
    ensure(call, actor)?;
    let mut state = initial_state::read(call, actor)?;
    replay_pose(call.store(), actor, &mut state);
    if let Some(spin) = render_spin(call, actor)? {
        [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = spin;
        state.spin_baked = true;
    }
    if let Some(time) = effect_time(call, actor, clock)? {
        state.effect_time = Some(time);
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_motion_reads_observe_raw_and_nested_mutations() {
        fn execute(runtime: &mut crate::SongRuntime, source: &str) -> Value {
            runtime
                .vm
                .execute(
                    &deadlib_lua::compile_named(source.as_bytes(), b"=motion mutation").unwrap(),
                )
                .unwrap()
                .first()
                .copied()
                .unwrap_or(Value::Nil)
        }
        let mut runtime =
            crate::SongRuntime::new(&crate::SongLuaCompileContext::new(".", "motion mutation"))
                .unwrap();
        let declaration = execute(&mut runtime, "return Def.Quad{}");
        let scene = runtime
            .actors
            .instantiate(&mut runtime.vm, declaration)
            .unwrap();
        let actor = runtime.actors.root(&runtime.vm, scene).unwrap();
        runtime.vm.set_global(b"actor", actor).unwrap();
        let Value::Table(id) = actor else {
            unreachable!()
        };
        assert!(runtime.vm.store_ref().actors.contains_key(&id));
        let cases = [
            ("", None),
            (
                "actor.__songlua_state_rainbow=true; actor.__songlua_state_motion_clock={units=2}",
                Some([10., 2.]),
            ),
            ("actor.__songlua_state_effect_clock='beat'", Some([20., 2.])),
            (
                "actor.__songlua_state_motion_clock.units=3",
                Some([20., 3.]),
            ),
            ("actor.__songlua_state_effect_clock=nil", Some([10., 3.])),
            ("actor.__songlua_state_rainbow=nil", None),
            ("actor.__songlua_state_effect_mode='bob'", Some([10., 3.])),
            (
                "actor.__songlua_state_motion_clock={units=4}",
                Some([10., 4.]),
            ),
            (
                "actor.__songlua_state_motion_clock=setmetatable({}, {__index=function() actor.__songlua_state_effect_clock='beat'; collectgarbage('collect'); return 5 end})",
                Some([10., 5.]),
            ),
            ("", Some([20., 5.])),
            ("actor.__songlua_state_effect_mode=nil", None),
        ];
        for (source, expected) in cases {
            execute(&mut runtime, source);
            // Read twice so each mutation is also exercised with a warm field cache.
            for _ in 0..2 {
                let Answer::Motion(time, _) = runtime
                    .actors
                    .chronological_read(&mut runtime.vm, actor, Query::Motion([10., 20.], false))
                    .unwrap()
                else {
                    panic!("motion answer")
                };
                assert_eq!(time, expected, "{source}");
                // The authored getter changes the next read's clock.
                if source.contains("__index") {
                    break;
                }
            }
        }
        for (source, expected_error) in [
            ("actor.__songlua_state_rainbow={}", "boolean expected"),
            (
                "actor.__songlua_state_rainbow=false; actor.__songlua_state_effect_mode={}",
                "string expected",
            ),
            (
                "actor.__songlua_state_effect_mode='bob'; actor.__songlua_state_motion_clock=false",
                "table expected",
            ),
            (
                "actor.__songlua_state_motion_clock={units={}}",
                "number expected",
            ),
        ] {
            execute(&mut runtime, source);
            let error = runtime
                .actors
                .chronological_read(&mut runtime.vm, actor, Query::Motion([10., 20.], false))
                .err()
                .expect("invalid motion field");
            assert!(error.to_string().contains(expected_error), "{error}");
        }
        execute(&mut runtime, "actor.__songlua_state_motion_clock={units=6}");
        let Answer::Motion(time, _) = runtime
            .actors
            .chronological_read(&mut runtime.vm, actor, Query::Motion([10., 20.], false))
            .unwrap()
        else {
            panic!("motion answer")
        };
        assert_eq!(time, Some([20., 6.]));
        execute(
            &mut runtime,
            "actor.__songlua_state_rainbow=nil; actor.__songlua_state_effect_mode=nil; actor.__songlua_state_motion_clock=nil",
        );
        // Native motion can advance without changing the actor's raw fields.
        for expected in [None, Some([11., 7.]), Some([12., 8.]), None] {
            let state = &mut runtime.vm.store().actors.get_mut(&id).unwrap().state.render;
            state.rainbow = expected.is_some();
            state.effect_time = expected;
            let Answer::Motion(time, _) = runtime
                .actors
                .chronological_read(&mut runtime.vm, actor, Query::Motion([10., 20.], false))
                .unwrap()
            else {
                panic!("motion answer")
            };
            assert_eq!(time, expected);
        }
        runtime.actors.release(&mut runtime.vm, scene).unwrap();
    }

    #[test]
    fn batched_motion_reads_preserve_order_roots_and_getter_errors() {
        let mut runtime =
            crate::SongRuntime::new(&crate::SongLuaCompileContext::new(".", "motion reads"))
                .unwrap();
        let source = br#"
            last = 0
            fail = false
            local actors = {}
            for i = 1, 600 do
                actors[i] = {
                    __songlua_state_rainbow = true,
                    __songlua_state_effect_clock = 'beat',
                    __songlua_state_motion_clock = setmetatable({}, {
                        __index = function(_, key)
                            assert(key == 'units' and last + 1 == i)
                            last = i
                            if i == 253 or i == 254 then collectgarbage('collect') end
                            if fail and i == 254 then return {} end
                            return i
                        end,
                    }),
                    __songlua_state_spin = { baked = true, rotation = {i, i+1, i+2} },
                }
            end
            return actors
        "#;
        let value = runtime
            .vm
            .execute(&deadlib_lua::compile_named(source, b"=motion reads").unwrap())
            .unwrap()[0];
        let root = runtime.vm.pin(value).unwrap();
        let Value::Table(table) = value else {
            unreachable!()
        };
        let actors = (1..=600)
            .map(|i| {
                runtime
                    .vm
                    .raw_table_get(table, Value::Number(i as f64))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let mut answers = Vec::new();
        for failed in [false, true, false] {
            let setup = if failed {
                b"last=0; fail=true".as_slice()
            } else {
                b"last=0; fail=false".as_slice()
            };
            runtime
                .vm
                .execute(&deadlib_lua::compile_named(setup, b"=reset motion reads").unwrap())
                .unwrap();
            let result = runtime.actors.chronological_read_many(
                &mut runtime.vm,
                &actors,
                Query::Motion([10., 20.], true),
                &mut answers,
            );
            if failed {
                assert!(result.unwrap_err().to_string().contains("number expected"));
                assert_eq!(runtime.vm.get_global(b"last").unwrap(), Value::Number(254.));
                continue;
            }
            result.unwrap();
            assert_eq!(answers.len(), actors.len());
            for (i, answer) in answers.iter().enumerate() {
                let &Answer::Motion(time, spin) = answer else {
                    panic!("motion answer")
                };
                let n = (i + 1) as f32;
                assert_eq!(time, Some([20., n]));
                assert_eq!(spin, Some([n, n + 1., n + 2.]));
            }
        }
        runtime.vm.unpin(root).unwrap();
    }
}
