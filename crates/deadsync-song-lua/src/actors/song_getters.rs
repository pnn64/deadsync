//! Preparation's sampled getter handoff, including ordinary assignment hooks.
use super::*;
use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update};

pub(super) fn write(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (target, value) = call.function_data::<(Target, Update)>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
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
    ) && perframes::is_player(call, actor)?
    {
        return Ok(());
    }
    let key = crate::frame_capture::overlay_getter_scalar_key(target);
    match value {
        Update::F32(value) if !key.is_empty() => {
            let current = match target {
                Target::Aux => Some("__songlua_aux"),
                Target::X => Some("__songlua_current_x"),
                Target::Y => Some("__songlua_current_y"),
                Target::Z => Some("__songlua_current_z"),
                _ => None,
            };
            let key = current
                .map(str::to_owned)
                .unwrap_or_else(|| format!("__songlua_state_{key}"));
            assign(
                call,
                actor,
                &key,
                Value::Number(f64::from(value)),
                target,
                &Update::F32(value),
            )
        }
        Update::Bool(value) if target == Target::Hibernating => assign(
            call,
            actor,
            "__songlua_state_hibernating",
            Value::Boolean(value),
            target,
            &Update::Bool(value),
        ),
        Update::Bool(value) if target == Target::Visible => {
            assign(
                call,
                actor,
                "__songlua_visible",
                Value::Boolean(value),
                target,
                &Update::Bool(value),
            )?;
            assign(
                call,
                actor,
                "__songlua_state_visible",
                Value::Boolean(value),
                target,
                &Update::Bool(value),
            )
        }
        Update::Quaternion(value) => {
            let vector = sprites::vector_value(call, &value)?;
            song_replay::raw_set(call, actor, "__songlua_state_quaternion", vector)?;
            sync(
                call.store(),
                actor,
                Target::Quaternion,
                &Update::Quaternion(value),
            );
            Ok(())
        }
        Update::Vec4(value) if target == Target::Diffuse => {
            let vector = sprites::vector_value(call, &value)?;
            call.with_roots(&[vector], |call| {
                assign(
                    call,
                    actor,
                    "__songlua_state_diffuse",
                    vector,
                    target,
                    &Update::Vec4(value),
                )?;
                assign(
                    call,
                    actor,
                    "__songlua_diffuse",
                    vector,
                    target,
                    &Update::Vec4(value),
                )
            })
        }
        Update::Vec3(value) if target == Target::EffectMagnitude => {
            let vector = sprites::vector_value(call, &value)?;
            call.with_roots(&[vector], |call| {
                assign(
                    call,
                    actor,
                    "__songlua_state_effect_magnitude",
                    vector,
                    target,
                    &Update::Vec3(value),
                )
            })
        }
        _ => Ok(()),
    }
}
fn assign(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
    value: Value,
    target: Target,
    update: &Update,
) -> Result<(), Error> {
    let native_base = crate::base_transform::BaseField::from_key(
        crate::frame_capture::overlay_getter_scalar_key(target),
    )
    .is_some()
        && {
            let guard = call.store();
            guard.playback
                && guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
                && guard.actors.get(&actor).is_some_and(|a| !a.raw_live_epoch)
        };
    if !native_base {
        call.with_roots(&[value], |call| sprites::set(call, actor, key, value))?;
    }
    sync(call.store(), actor, target, update);
    Ok(())
}
fn sync(store: &mut Store, id: TableId, target: Target, update: &Update) {
    if crate::tween_pose::TARGETS.contains(&target) {
        song_pose::update_getter_state(store, id, target, update);
    }
    let store = &mut *store;
    let Some(actor) = store.actors.get_mut(&id) else {
        return;
    };
    crate::frame_capture::set_overlay_state_update_value(&mut actor.state.render, target, update);
    host_compat::update_base(actor, target, update);
    if let (Target::Visible, Update::Bool(visible)) = (target, update) {
        actor.state.visible = *visible;
    }
}
