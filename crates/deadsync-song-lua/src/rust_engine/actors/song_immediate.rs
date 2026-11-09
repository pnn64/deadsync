//! Compiler buffers shared by immediate and scheduled render setters.
use super::super::{SongReplayImmediateUpdates, SongReplayScheduledUpdate};
use super::*;
use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as UpdateValue};

// The order within compound setters matters (notably basezoom and Load's size
// and texture writes). Effects emit their own ordered sequence of writes.
pub(crate) const TARGETS: &[Target] = &[
    Target::Visible,
    Target::Hibernating,
    Target::HAlign,
    Target::VAlign,
    Target::TextAlign,
    Target::BaseZoom,
    Target::BaseZoomX,
    Target::BaseZoomY,
    Target::BaseZoomZ,
    Target::BaseAlpha,
    Target::BaseRotation,
    Target::ShadowLen,
    Target::ShadowColor,
    Target::Size,
    Target::SpriteTexture,
    Target::SpriteStateIndex,
    Target::Vibrate,
    Target::Rainbow,
    Target::EffectMode,
    Target::EffectPeriod,
    Target::EffectTiming,
    Target::EffectMagnitude,
    Target::EffectColor1,
    Target::EffectColor2,
    Target::EffectOffset,
    Target::EffectClock,
    Target::EffectNoOffset,
    Target::EffectTimer,
    Target::CullMode,
    Target::AftPreserve,
    Target::AftCreated,
];

#[derive(Clone, Copy)]
pub(super) struct Dispatch {
    pub(super) seconds: Option<f64>,
    pub(super) delay: f32,
}

#[derive(Default)]
pub(super) struct Capture {
    touched: Vec<usize>,
    pub(super) frames: HashMap<usize, SongReplayImmediateUpdates>,
    pending: HashMap<usize, Vec<(Target, UpdateValue)>>,
    pub(super) prior_positions: HashMap<usize, [f32; 3]>,
}

fn replace(values: &mut Vec<(Target, UpdateValue)>, target: Target, value: UpdateValue) {
    if let Some((_, old)) = values.iter_mut().find(|(key, _)| *key == target) {
        *old = value;
    } else {
        values.push((target, value));
    }
}

impl Capture {
    pub(super) fn publish_pose(
        &mut self,
        index: usize,
        targets: u128,
        pose: crate::tween_pose::TweenPose,
    ) {
        let frame = self.touch(index);
        for target in crate::tween_pose::TARGETS {
            if targets & (1_u128 << target as usize) != 0 {
                let value = pose.get(target);
                replace(&mut frame.values, target, value.clone());
                replace(&mut frame.final_values, target, value);
            }
        }
    }
    pub(super) fn touch(&mut self, index: usize) -> &mut SongReplayImmediateUpdates {
        self.frames.entry(index).or_insert_with(|| {
            self.touched.push(index);
            SongReplayImmediateUpdates {
                overlay_index: index,
                ..Default::default()
            }
        })
    }

    pub(super) fn record(
        &mut self,
        index: usize,
        target: Target,
        value: UpdateValue,
        scheduled: Option<SongReplayScheduledUpdate>,
    ) {
        let pending = self
            .pending
            .get(&index)
            .and_then(|values| values.iter().find(|(key, _)| *key == target))
            .map(|(_, value)| value.clone());
        let frame = self.touch(index);
        replace(&mut frame.final_values, target, value.clone());
        if let Some(mut update) = scheduled {
            if let Some(previous) = frame.scheduled.iter_mut().find(|prior| {
                prior.target == target
                    && prior.dispatch_seconds == update.dispatch_seconds
                    && prior.frame_advance == update.frame_advance
                    && prior.delay_seconds == update.delay_seconds
                    && prior.duration_seconds == update.duration_seconds
                    && prior.easing == update.easing
                    && prior.opt1 == update.opt1
            }) {
                previous.value = value;
            } else {
                update.initial_value = frame
                    .values
                    .iter()
                    .find(|(key, _)| *key == target)
                    .map(|(_, value)| value.clone())
                    .or(pending);
                frame.scheduled.push(update);
            }
        } else {
            replace(&mut frame.values, target, value);
        }
    }

    fn drain(&mut self, include_empty: bool) -> Vec<SongReplayImmediateUpdates> {
        self.touched
            .drain(..)
            .map(|index| self.frames.remove(&index).unwrap())
            .filter(|frame| {
                include_empty
                    || frame.tween_reset
                    || !frame.values.is_empty()
                    || !frame.scheduled.is_empty()
                    || !frame.final_values.is_empty()
            })
            .collect()
    }
}

impl Actors {
    pub(in crate::rust_engine) fn set_song_pending_overlay_updates(
        &self,
        screen: ActorScene,
        values: &[(usize, Target, UpdateValue)],
    ) -> bool {
        self.set_pending_updates(screen, values, false)
    }

    pub(in crate::rust_engine) fn set_song_pending_render_updates(
        &self,
        screen: ActorScene,
        values: &[(usize, Target, UpdateValue)],
    ) -> bool {
        self.set_pending_updates(screen, values, true)
    }

    fn set_pending_updates(
        &self,
        screen: ActorScene,
        values: &[(usize, Target, UpdateValue)],
        render_only: bool,
    ) -> bool {
        let mut store = self.store.lock().unwrap();
        if store.replay_frame.map(|f| f.0) != Some(screen.id) {
            return false;
        }
        let Some(capture) = store
            .scenes
            .get_mut(&screen.id)
            .and_then(|s| s.song_session.as_mut())
            .and_then(|s| s.messages.as_mut())
        else {
            return false;
        };
        if values.iter().any(|(index, target, _)| {
            !capture.indices.values().any(|i| i == index)
                || if render_only {
                    !song_properties::TARGETS.contains(target)
                } else {
                    !song_pose::supported(*target)
                }
        }) {
            return false;
        }
        capture.immediate.pending.clear();
        for (index, target, value) in values {
            replace(
                capture.immediate.pending.entry(*index).or_default(),
                *target,
                value.clone(),
            );
        }
        true
    }

    pub(in crate::rust_engine) fn drain_song_immediate_updates(
        &self,
        screen: ActorScene,
    ) -> Option<Vec<SongReplayImmediateUpdates>> {
        let mut frames = self.drain_song_property_updates(screen)?;
        for frame in &mut frames {
            frame.values.retain(|(target, _)| TARGETS.contains(target));
            frame
                .final_values
                .retain(|(target, _)| TARGETS.contains(target));
            frame
                .scheduled
                .retain(|update| TARGETS.contains(&update.target));
        }
        frames.retain(|f| {
            f.tween_reset
                || !f.values.is_empty()
                || !f.final_values.is_empty()
                || !f.scheduled.is_empty()
        });
        Some(frames)
    }

    pub(in crate::rust_engine) fn drain_song_property_updates(
        &self,
        screen: ActorScene,
    ) -> Option<Vec<SongReplayImmediateUpdates>> {
        let mut frames = self.drain_song_overlay_updates(screen)?;
        for frame in &mut frames {
            frame.values.retain(|(target, _)| {
                TARGETS.contains(target) || song_properties::TARGETS.contains(target)
            });
            frame.final_values.retain(|(target, _)| {
                TARGETS.contains(target) || song_properties::TARGETS.contains(target)
            });
            frame.scheduled.retain(|u| {
                TARGETS.contains(&u.target) || song_properties::TARGETS.contains(&u.target)
            });
        }
        frames.retain(|f| {
            f.tween_reset
                || !f.values.is_empty()
                || !f.final_values.is_empty()
                || !f.scheduled.is_empty()
        });
        Some(frames)
    }

    pub(in crate::rust_engine) fn drain_song_overlay_updates(
        &self,
        screen: ActorScene,
    ) -> Option<Vec<SongReplayImmediateUpdates>> {
        self.take_song_overlay_frame_updates(screen, false)
    }

    pub(in crate::rust_engine) fn take_song_overlay_frame_updates(
        &self,
        screen: ActorScene,
        include_empty: bool,
    ) -> Option<Vec<SongReplayImmediateUpdates>> {
        let mut store = self.store.lock().unwrap();
        if store.replay_frame?.0 != screen.id {
            return None;
        }
        Some(
            store
                .scenes
                .get_mut(&screen.id)?
                .song_session
                .as_mut()?
                .messages
                .as_mut()?
                .immediate
                .drain(include_empty),
        )
    }

    pub(in crate::rust_engine) fn chronological_song_capture(&self) -> bool {
        self.store.lock().unwrap().replay_clock.is_some()
    }

    /// Metadata classification performs no authored calls. If it fails, retain
    /// the complete publication batch as the native visit-before-clear reader did.
    pub(in crate::rust_engine) fn restore_song_overlay_updates(
        &self,
        screen: ActorScene,
        frames: Vec<SongReplayImmediateUpdates>,
    ) {
        let mut store = self.store.lock().unwrap();
        let capture = &mut store
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages
            .as_mut()
            .unwrap()
            .immediate;
        debug_assert!(capture.touched.is_empty() && capture.frames.is_empty());
        for frame in frames {
            capture.touched.push(frame.overlay_index);
            capture.frames.insert(frame.overlay_index, frame);
        }
    }
}

// A capture's actor IDs are traced by its enclosing scene. Lookup after each Lua
// access also tolerates authored reentry without holding a Store lock across it.
pub(super) fn context(store: &Store, id: TableId) -> Option<(u64, usize)> {
    if store.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    let (owner, _) = store.replay_frame?;
    if store.retired_scenes.contains_key(&owner) {
        return None;
    }
    let capture = store
        .scenes
        .get(&owner)?
        .song_session
        .as_ref()?
        .messages
        .as_ref()?;
    Some((owner, *capture.indices.get(&id)?))
}

pub(super) fn record(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    target: Target,
    value: UpdateValue,
) {
    capture::retain_render_update(store, id, target, &value, true);
    record_frame(call, store, id, target, value);
}
fn record_frame(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    target: Target,
    value: UpdateValue,
) {
    if context(&store.lock().unwrap(), id).is_none() {
        return;
    }
    let beat = effects::values(call, false).map_or(0., |values| values[0]);
    let mut guard = store.lock().unwrap();
    let Some((owner, index)) = context(&guard, id) else {
        return;
    };
    let dispatch = guard.replay_dispatch;
    let frame_advance = guard
        .replay_clock
        .as_ref()
        .map_or(0., |clock| clock.immediate_advance(id));
    let message = guard.active_message.clone();
    let message_advance = guard
        .replay_clock
        .as_ref()
        .map_or(0., |clock| clock.actor_frame_advance(id));
    #[cfg(feature = "test-support")]
    let observed_eligible = song_messages::observed_eligible(&guard);
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
                delay_seconds: dispatch.map_or(0., |scope| scope.delay),
                duration_seconds: 0.,
                easing: None,
                opt1: None,
            },
            message_advance,
        );
    }
    let scheduled = dispatch.map(|dispatch| SongReplayScheduledUpdate {
        progress: None,
        dispatch_seconds: dispatch.seconds,
        frame_advance,
        initial_value: None,
        delay_seconds: dispatch.delay,
        duration_seconds: 0.,
        easing: None,
        opt1: None,
        target,
        value: value.clone(),
    });
    #[cfg(feature = "test-support")]
    capture.observe(index, beat, target, &value, false, observed_eligible);
    capture.immediate.record(index, target, value, scheduled);
}

pub(super) fn delta(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    delta: &crate::SongLuaOverlayStateDelta,
    immediate: bool,
) {
    if !immediate || context(&store.lock().unwrap(), id).is_none() {
        return;
    }
    let mut state = crate::SongLuaOverlayState::default();
    crate::apply_overlay_delta(&mut state, delta);
    for &target in TARGETS {
        if delta.has_update_target(target) {
            record_frame(
                call,
                store,
                id,
                target,
                crate::perframe::overlay_state_update_value(&state, target),
            );
        }
    }
}

pub(super) fn cancel(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    finish: bool,
) -> Result<(), Error> {
    if context(&store.lock().unwrap(), id).is_none() {
        return Ok(());
    }
    let beat = effects::values(call, false).map_or(0., |values| values[0]);
    let (owner, index, message, values) = {
        let mut guard = store.lock().unwrap();
        let Some((owner, index)) = context(&guard, id) else {
            return Ok(());
        };
        let actor = &guard.actors[&id];
        let cur = actor.current;
        let position = actor
            .preparation_getters
            .unwrap_or([cur.x, cur.y, cur.z, cur.aux]);
        let message = guard.active_message.clone();
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
        let pending = capture.immediate.pending.remove(&index).unwrap_or_default();
        let frame = capture.immediate.touch(index);
        frame.tween_reset = true;
        if !finish {
            let targets = frame
                .scheduled
                .iter()
                .filter(|u| crate::tween_pose::TARGETS.contains(&u.target))
                .fold(0u128, |mask, u| mask | (1u128 << u.target as usize));
            frame
                .final_values
                .retain(|(target, _)| targets & (1u128 << *target as usize) == 0);
            frame
                .scheduled
                .retain(|u| !crate::tween_pose::TARGETS.contains(&u.target));
            for (target, value) in [Target::X, Target::Y, Target::Z].into_iter().zip(position) {
                replace(&mut frame.values, target, UpdateValue::F32(value));
                replace(&mut frame.final_values, target, UpdateValue::F32(value));
            }
            return Ok(());
        }
        let mut values = pending;
        for update in std::mem::take(&mut frame.scheduled) {
            replace(&mut values, update.target, update.value);
        }
        (owner, index, message, values)
    };
    for (target, value) in values {
        if crate::tween_pose::TARGETS.contains(&target) {
            song_pose::set_getter(call, store, id, target, &value)?;
        }
        let mut guard = store.lock().unwrap();
        if song_properties::TARGETS.contains(&target) {
            let actor = guard.actors.get_mut(&id).unwrap();
            crate::perframe::set_overlay_state_update_value(
                &mut actor.state.render,
                target,
                &value,
            );
            if let (Target::DrawOrder, UpdateValue::I32(order)) = (target, &value) {
                actor.state.draw_order = *order;
            }
            if target == Target::Fov {
                actor.state.render.fov = actor.state.render.fov.filter(|fov| *fov != -1.);
            }
        }
        let advance = guard
            .replay_clock
            .as_ref()
            .map_or(0., |clock| clock.immediate_advance(id));
        let Some(capture) = guard
            .scenes
            .get_mut(&owner)
            .and_then(|s| s.song_session.as_mut())
            .and_then(|s| s.messages.as_mut())
        else {
            return Ok(());
        };
        capture.immediate.record(index, target, value.clone(), None);
        if let Some(message) = &message {
            capture.write(
                message.clone(),
                crate::SongLuaStatefulMessageWrite {
                    overlay_index: index,
                    beat,
                    target,
                    value,
                    delay_seconds: 0.,
                    duration_seconds: 0.,
                    easing: None,
                    opt1: None,
                },
                advance,
            );
        }
    }
    Ok(())
}
