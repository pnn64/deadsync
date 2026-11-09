//! Scheduled renderer properties outside the native interpolated tween pose.
use super::*;
use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update};

pub(crate) const TARGETS: &[Target] = &[
    Target::ZBias,
    Target::DrawOrder,
    Target::DrawByZPosition,
    Target::Fov,
    Target::Vanishpoint,
    Target::Blend,
    Target::DepthTest,
    Target::DepthTestOnFail,
    Target::DepthWrite,
    Target::TextureFiltering,
];

pub(super) fn record(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    target: Target,
    value: Update,
) {
    capture::retain_render_update(store, id, target, &value, false);
    let _ = record_inner(call, store, id, target, value);
}

pub(super) fn record_inner(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    target: Target,
    value: Update,
) -> Option<bool> {
    song_immediate::context(&store.lock().unwrap(), id)?;
    let beat = effects::values(call, false).map_or(0., |values| values[0]);
    let duration = get(call, id, b"__songlua_capture_duration")
        .ok()
        .and_then(|v| float_scalar(call, v, true))
        .unwrap_or(0.)
        .max(0.);
    let easing = get(call, id, b"__songlua_capture_easing")
        .ok()
        .and_then(|v| render::text(call, v).map(str::to_owned));
    let opt1 = get(call, id, b"__songlua_capture_opt1")
        .ok()
        .and_then(|v| float_scalar(call, v, true));
    let mut guard = store.lock().unwrap();
    let (owner, index) = song_immediate::context(&guard, id)?;
    let actor = &guard.actors[&id];
    let mut delay = actor.capture.as_ref().map_or_else(
        || lifecycle::message_delay(actor, duration),
        |capture| capture.cursor().max(0.),
    );
    let (progress, frame_advance) = if let Some(publication) = guard
        .replay_clock
        .as_mut()
        .map(|clock| clock.publication(id, delay > f32::EPSILON || duration > f32::EPSILON))
    {
        delay = publication.cursor.unwrap_or(delay);
        (publication.progress, publication.advance)
    } else {
        (None, 0.)
    };
    let dispatch_seconds = guard.replay_dispatch.and_then(|scope| scope.seconds);
    let message = guard.active_message.clone();
    #[cfg(feature = "test-support")]
    let observed_eligible = song_messages::observed_eligible(&guard);
    let message_advance = guard
        .replay_clock
        .as_ref()
        .map_or(0., |clock| clock.actor_frame_advance(id));
    let capture = guard
        .scenes
        .get_mut(&owner)
        .unwrap()
        .song_session
        .as_mut()
        .unwrap()
        .messages
        .as_mut()
        .unwrap();
    if let Some(message) = message {
        capture.write(
            message,
            crate::SongLuaStatefulMessageWrite {
                overlay_index: index,
                beat,
                target,
                value: value.clone(),
                delay_seconds: delay,
                duration_seconds: duration,
                easing: easing.clone(),
                opt1,
            },
            message_advance,
        );
    }
    let scheduled =
        (delay > f32::EPSILON || duration > f32::EPSILON || progress.is_some()).then(|| {
            super::super::SongReplayScheduledUpdate {
                progress,
                dispatch_seconds,
                frame_advance,
                initial_value: None,
                delay_seconds: delay,
                duration_seconds: duration,
                easing,
                opt1,
                target,
                value: value.clone(),
            }
        });
    let timed = scheduled.is_some();
    #[cfg(feature = "test-support")]
    capture.observe(index, beat, target, &value, timed, observed_eligible);
    capture.immediate.record(index, target, value, scheduled);
    Some(timed)
}
