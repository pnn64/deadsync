//! Fallback pose publications and the compiler's sampled getter handoff.
use super::*;
use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update};

pub(super) fn supported(target: Target) -> bool {
    crate::tween_pose::TARGETS.contains(&target)
        || song_immediate::TARGETS.contains(&target)
        || song_properties::TARGETS.contains(&target)
        || song_media::TARGETS.contains(&target)
}
pub(super) fn mode(store: &Store, id: TableId) -> bool {
    !store.ease_probe_active
        && song_immediate::context(store, id).is_some()
        && store.replay_clock.as_ref().is_some_and(|c| c.fallback(id))
}
pub(super) fn prior_position(store: &Store, id: TableId) -> Option<[f32; 3]> {
    if !store.replay_clock.as_ref()?.later(id) {
        return None;
    }
    let (owner, index) = song_immediate::context(store, id)?;
    store
        .scenes
        .get(&owner)?
        .song_session
        .as_ref()?
        .messages
        .as_ref()?
        .immediate
        .prior_positions
        .get(&index)
        .copied()
}
pub(super) fn record(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    delta: &crate::SongLuaOverlayStateDelta,
) -> bool {
    if !mode(&store.lock().unwrap(), id) {
        return false;
    }
    let mut state = crate::SongLuaOverlayState::default();
    crate::apply_overlay_delta(&mut state, delta);
    for target in crate::tween_pose::TARGETS {
        if !delta.has_update_target(target) {
            continue;
        }
        let value = crate::perframe::overlay_state_update_value(&state, target);
        let scheduled = song_properties::record_inner(call, store, id, target, value.clone());
        if scheduled == Some(false)
            && let Update::F32(value) = value
        {
            let axis = match target {
                Target::X => Some(0),
                Target::Y => Some(1),
                Target::Z => Some(2),
                Target::Aux => Some(3),
                _ => None,
            };
            if let Some(axis) = axis {
                let mut guard = store.lock().unwrap();
                let actor = guard.actors.get_mut(&id).unwrap();
                actor.current.set(target, &Update::F32(value));
                let current = actor.current;
                actor.preparation_getters.get_or_insert([
                    current.x,
                    current.y,
                    current.z,
                    current.aux,
                ])[axis] = value;
                if axis < 3 {
                    let (owner, index) = song_immediate::context(&guard, id).unwrap();
                    if let Some(position) = guard
                        .scenes
                        .get_mut(&owner)
                        .and_then(|scene| scene.song_session.as_mut())
                        .and_then(|session| session.messages.as_mut())
                        .and_then(|capture| capture.immediate.prior_positions.get_mut(&index))
                    {
                        position[axis] = value;
                    }
                }
            }
        }
    }
    true
}

pub(super) fn advance_position(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
) -> Result<(), Error> {
    let position = {
        let mut guard = store.lock().unwrap();
        if !mode(&guard, id) {
            return Ok(());
        }
        let (owner, index) = song_immediate::context(&guard, id).unwrap();
        let actor = &guard.actors[&id];
        let current = actor.current;
        let mut position =
            actor
                .preparation_getters
                .unwrap_or([current.x, current.y, current.z, current.aux]);
        let Some(frame) = guard.scenes[&owner]
            .song_session
            .as_ref()
            .unwrap()
            .messages
            .as_ref()
            .unwrap()
            .immediate
            .frames
            .get(&index)
        else {
            return Ok(());
        };
        let mut destinations = [None; 3];
        for update in &frame.scheduled {
            let axis = match update.target {
                Target::X => 0,
                Target::Y => 1,
                Target::Z => 2,
                _ => continue,
            };
            let Update::F32(to) = update.value else {
                continue;
            };
            let from = destinations[axis]
                .or(match update.initial_value {
                    Some(Update::F32(value)) => Some(value),
                    _ => None,
                })
                .unwrap_or(position[axis]);
            destinations[axis] = Some(to);
            if update.frame_advance + 1.0e-7 < update.delay_seconds {
                continue;
            }
            let factor = if update.duration_seconds <= f32::EPSILON {
                1.
            } else {
                ((update.frame_advance - update.delay_seconds) / update.duration_seconds)
                    .clamp(0., 1.)
            };
            let factor =
                crate::overlay_command_ease_factor(update.easing.as_deref(), factor, update.opt1);
            position[axis] = (to - from).mul_add(factor, from);
        }
        let actor = guard.actors.get_mut(&id).unwrap();
        actor.preparation_getters = Some(position);
        [actor.current.x, actor.current.y, actor.current.z] =
            [position[0], position[1], position[2]];
        position
    };
    for (key, value) in [
        "__songlua_current_x",
        "__songlua_current_y",
        "__songlua_current_z",
    ]
    .into_iter()
    .zip(position)
    {
        song_replay::raw_set(call, id, key, Value::Number(f64::from(value)))?;
    }
    Ok(())
}

fn scalar_key(target: Target) -> Option<&'static str> {
    Some(match target {
        Target::Aux => "aux",
        Target::X => "x",
        Target::Y => "y",
        Target::Z => "z",
        Target::Zoom => "zoom",
        Target::ZoomX => "zoom_x",
        Target::ZoomY => "zoom_y",
        Target::ZoomZ => "zoom_z",
        Target::RotationX => "rot_x_deg",
        Target::RotationY => "rot_y_deg",
        Target::RotationZ => "rot_z_deg",
        Target::SkewX => "skew_x",
        Target::SkewY => "skew_y",
        Target::CropLeft => "cropleft",
        Target::CropRight => "cropright",
        Target::CropTop => "croptop",
        Target::CropBottom => "cropbottom",
        Target::FadeLeft => "fadeleft",
        Target::FadeRight => "faderight",
        Target::FadeTop => "fadetop",
        Target::FadeBottom => "fadebottom",
        _ => return None,
    })
}

pub(super) fn set_getter(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    target: Target,
    value: &Update,
) -> Result<(), Error> {
    if matches!(
        target,
        Target::Zoom
            | Target::ZoomX
            | Target::ZoomY
            | Target::ZoomZ
            | Target::RotationX
            | Target::RotationY
            | Target::RotationZ
            | Target::SkewX
            | Target::SkewY
    ) {
        let key = call.try_intern(b"__songlua_player_index")?;
        let player = call.raw_table_get(id, key)?;
        let key = call.try_intern(b"__songlua_player_child_name")?;
        let child = call.raw_table_get(id, key)?;
        if player != Value::Nil && child == Value::Nil {
            return Ok(());
        }
    }
    let scalar = scalar_key(target);
    if !matches!(value,Update::F32(_) if scalar.is_some())
        && !matches!(
            (target, value),
            (Target::Diffuse, Update::Vec4(_)) | (_, Update::Quaternion(_))
        )
    {
        return Ok(());
    }
    update_getter_state(store, id, target, value);
    match value {
        Update::F32(value) => {
            let key = match target {
                Target::X => "__songlua_current_x".to_owned(),
                Target::Y => "__songlua_current_y".to_owned(),
                Target::Z => "__songlua_current_z".to_owned(),
                Target::Aux => "__songlua_aux".to_owned(),
                _ => format!("__songlua_state_{}", scalar.unwrap()),
            };
            song_replay::raw_set(call, id, &key, Value::Number(f64::from(*value)))?;
        }
        Update::Vec4(value) => {
            let value = sprites::vector_value(call, value)?;
            song_replay::raw_set(call, id, "__songlua_state_diffuse", value)?;
            song_replay::raw_set(call, id, "__songlua_diffuse", value)?;
        }
        Update::Quaternion(value) => {
            let value = sprites::vector_value(call, value)?;
            song_replay::raw_set(call, id, "__songlua_state_quaternion", value)?;
        }
        _ => {}
    }
    Ok(())
}

/// Metadata half of the getter handoff. Lua assignments stay in their
/// caller's order, and no actor-store lock survives those assignments.
pub(super) fn update_getter_state(store: &Shared, id: TableId, target: Target, value: &Update) {
    let mut guard = store.lock().unwrap();
    let fallback = mode(&guard, id);
    let Some(actor) = guard.actors.get_mut(&id) else {
        return;
    };
    let dest = *actor.getter_destination();
    let replay = actor.replay.get_or_insert_with(song_replay::State::default);
    let getters = replay.getters.get_or_insert(dest);
    if !matches!(target, Target::X | Target::Y | Target::Z | Target::Aux) {
        getters.set(target, value);
    }
    if fallback {
        actor.current.set(target, value);
        // Non-position samples replace the native raw destination. Relative
        // setters must read that sample even while a captured queue exists.
        if !matches!(target, Target::X | Target::Y | Target::Z | Target::Aux) {
            actor.destination_mut().set(target, value);
        }
    }
    if let Update::F32(v) = value {
        let axis = match target {
            Target::X => Some(0),
            Target::Y => Some(1),
            Target::Z => Some(2),
            Target::Aux => Some(3),
            _ => None,
        };
        if let Some(axis) = axis {
            let cur = actor.current;
            actor
                .preparation_getters
                .get_or_insert([cur.x, cur.y, cur.z, cur.aux])[axis] = *v;
        }
        match target {
            Target::RotationX => actor.state.rotation[0] = *v,
            Target::RotationY => actor.state.rotation[1] = *v,
            Target::RotationZ => actor.state.rotation[2] = *v,
            _ => {}
        }
    }
}

fn apply_getters(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (store, values) = call
        .function_data::<(Shared, Vec<(TableId, Target, Update)>)>()
        .unwrap()
        .clone();
    for (id, target, value) in values {
        set_getter(call, &store, id, target, &value)?;
    }
    Ok(())
}

impl Actors {
    pub(crate) fn set_song_replay_prior_positions(
        &self,
        screen: ActorScene,
        states: &[crate::SongLuaOverlayState],
    ) -> bool {
        let mut guard = self.store.lock().unwrap();
        if guard.replay_frame.map(|f| f.0) != Some(screen.id) {
            return false;
        }
        let Some(capture) = guard
            .scenes
            .get_mut(&screen.id)
            .and_then(|s| s.song_session.as_mut())
            .and_then(|s| s.messages.as_mut())
        else {
            return false;
        };
        let count = capture.indices.values().max().map_or(0, |i| i + 1);
        for (index, state) in states.iter().take(count).enumerate() {
            capture
                .immediate
                .prior_positions
                .insert(index, [state.x, state.y, state.z]);
        }
        true
    }
    pub(crate) fn set_song_replay_pose_getters(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        values: &[(usize, Target, Update)],
    ) -> Result<bool, Error> {
        let Some(values) = (|| {
            let guard = self.store.lock().unwrap();
            if guard.replay_frame?.0 != screen.id {
                return None;
            }
            let capture = guard
                .scenes
                .get(&screen.id)?
                .song_session
                .as_ref()?
                .messages
                .as_ref()?;
            values
                .iter()
                .map(|(index, target, value)| {
                    if !crate::tween_pose::TARGETS.contains(target) {
                        return None;
                    }
                    let id = capture
                        .indices
                        .iter()
                        .find_map(|(id, i)| (*i == *index).then_some(*id))?;
                    Some((id, *target, value.clone()))
                })
                .collect::<Option<Vec<_>>>()
        })() else {
            return Ok(false);
        };
        let (roots, pin) = retained::values(vm, values.iter().map(|(id, _, _)| Value::Table(*id)))?;
        let function = vm.new_host_function_with_data(
            apply_getters,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(roots),
            ],
            (self.store.clone(), values),
        );
        vm.unpin(pin)?;
        let function = function?;
        vm.call(function, &[])?;
        Ok(true)
    }
}
