// Frozen method from 91d50c0d3; self renamed to capture for test access.
use super::*;

pub(super) fn record_stateful_message(
    capture: &mut SongLuaOverlayUpdateCapture,
    actor: &Table,
    beat: f32,
    target: SongLuaOverlayUpdateTarget,
    value: SongLuaOverlayUpdateValue,
    delay_seconds: f32,
    duration_seconds: f32,
    easing: Option<String>,
    opt1: Option<f32>,
) {
    let Some(message) = capture.active_broadcast.clone() else {
        return;
    };
    let Some(index) = capture.touch(actor) else {
        return;
    };
    capture
        .stateful_messages
        .entry(message.clone())
        .or_default()
        .entry(index)
        .or_default()
        .insert(target);
    capture
        .stateful_writes
        .entry(message)
        .or_default()
        .push(SongLuaStatefulMessageWrite {
            overlay_index: index,
            beat,
            target,
            value,
            delay_seconds,
            duration_seconds,
            easing,
            opt1,
        });
}
