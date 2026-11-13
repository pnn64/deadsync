//! Chronological message provenance for properties owned by the tween queue.
use super::song_replay::raw_set;
use super::*;
use crate::{SongLuaStatefulMessageCapture, SongLuaStatefulMessageWrite};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Capture {
    // Identities are owned by this song session's scene, never by this map.
    pub(super) indices: HashMap<TableId, usize>,
    pub(super) immediate: song_immediate::Capture,
    writes: BTreeMap<String, Vec<SongLuaStatefulMessageWrite>>,
    #[cfg(feature = "test-support")]
    observed: BTreeMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        Vec<crate::SongLuaOverlayUpdateSample>,
    >,
    pub(super) advances: BTreeMap<(usize, String), f32>,
    pub(super) wrappers: Option<chronological::WrapperCapture>,
}

impl Capture {
    pub(super) fn new(indices: impl IntoIterator<Item = (TableId, usize)>) -> Self {
        Self {
            indices: indices.into_iter().collect(),
            immediate: Default::default(),
            writes: BTreeMap::new(),
            #[cfg(feature = "test-support")]
            observed: BTreeMap::new(),
            advances: BTreeMap::new(),
            wrappers: None,
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn observe(
        &mut self,
        index: usize,
        beat: f32,
        target: crate::SongLuaOverlayUpdateTarget,
        value: &crate::SongLuaOverlayUpdateValue,
        scheduled: bool,
        eligible: bool,
    ) {
        if !eligible {
            return;
        }
        let samples = self.observed.entry((index, target)).or_default();
        let sample = crate::SongLuaOverlayUpdateSample {
            time: beat,
            value: value.clone(),
        };
        if samples.last().is_some_and(|last| last.time == beat) {
            if !scheduled {
                *samples.last_mut().unwrap() = sample;
            }
        } else {
            samples.push(sample);
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn fixture_vertex_sample(&self) -> (usize, Option<Arc<[[f32; 4]; 4]>>) {
        let Some(samples) = self
            .observed
            .get(&(0, crate::SongLuaOverlayUpdateTarget::VertexColors))
        else {
            return (0, None);
        };
        let value = samples.last().and_then(|sample| match &sample.value {
            crate::SongLuaOverlayUpdateValue::VertexColors(colors) => Some(colors.clone()),
            _ => None,
        });
        (samples.len(), value)
    }

    pub(super) fn write(
        &mut self,
        message: String,
        write: SongLuaStatefulMessageWrite,
        advance: f32,
    ) {
        // A pose-only message write still establishes this actor's place in
        // the frame buffer if an immediate property follows later.
        self.immediate.touch(write.overlay_index);
        self.advances
            .insert((write.overlay_index, message.clone()), advance);
        self.writes.entry(message).or_default().push(write);
    }

    pub(super) fn output(&self) -> Vec<SongLuaStatefulMessageCapture> {
        self.writes
            .iter()
            .map(|(message, writes)| {
                let mut targets: BTreeMap<usize, BTreeSet<_>> = BTreeMap::new();
                for write in writes {
                    targets
                        .entry(write.overlay_index)
                        .or_default()
                        .insert(write.target);
                }
                SongLuaStatefulMessageCapture {
                    message: message.clone(),
                    overlay_targets: targets
                        .into_iter()
                        .map(|(index, targets)| (index, targets.into_iter().collect()))
                        .collect(),
                    writes: writes.clone(),
                }
            })
            .collect()
    }
}

#[cfg(feature = "test-support")]
pub(super) fn observed_eligible(store: &Store) -> bool {
    store.active_message.is_none() && (store.audit_queued_depth == 0 || store.audit_update_command)
}
#[cfg(feature = "test-support")]
pub(super) fn update_command(store: &mut Store, field: &[u8]) -> bool {
    let store = &mut *store;
    let prior = store.audit_update_command;
    if field == b"UpdateCommand" {
        store.audit_update_command = true;
    }
    prior
}
#[cfg(feature = "test-support")]
pub(super) fn restore_command(store: &mut Store, prior: bool) {
    store.audit_update_command = prior;
}
#[cfg(feature = "test-support")]
pub(super) fn pose_write(
    call: &mut HostCall<'_>,
    id: TableId,
    delta: &crate::SongLuaOverlayStateDelta,
) {
    {
        let guard = call.store();
        if guard.fixture_pose_observer.is_none()
            && (song_pose::mode(&guard, id) || song_immediate::context(&guard, id).is_none())
        {
            return;
        }
    }
    let mut state = crate::SongLuaOverlayState::default();
    crate::apply_overlay_delta(&mut state, delta);
    for target in crate::tween_pose::TARGETS {
        if !delta.has_update_target(target) {
            continue;
        }
        let beat = effects::values(call, false).map_or(0., |values| values[0]);
        let value = crate::perframe::overlay_state_update_value(&state, target);
        let guard = call.store();
        let eligible = observed_eligible(&guard);
        let scheduled = !guard.actors[&id].queue.is_empty();
        if let Some(capture) = &mut guard.fixture_pose_observer {
            if let Some(&index) = capture.indices.get(&id) {
                capture.observe(index, beat, target, &value, scheduled, eligible);
            }
            continue;
        }
        let Some((owner, index)) = song_immediate::context(&guard, id) else {
            return;
        };
        guard
            .scenes
            .get_mut(&owner)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages
            .as_mut()
            .unwrap()
            .observe(index, beat, target, &value, scheduled, eligible);
    }
}

impl Actors {
    #[cfg(feature = "test-support")]
    pub(crate) fn take_song_observed_writes(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Vec<crate::SongLuaOverlayUpdateTrack> {
        let store = vm.store();
        let Some(capture) = store
            .scenes
            .get_mut(&screen.id)
            .and_then(|s| s.song_session.as_mut())
            .and_then(|s| s.messages.as_mut())
        else {
            return Vec::new();
        };
        std::mem::take(&mut capture.observed)
            .into_iter()
            .map(
                |((overlay_index, target), samples)| crate::SongLuaOverlayUpdateTrack {
                    overlay_index,
                    target,
                    samples,
                },
            )
            .collect()
    }
    pub(crate) fn song_replay_messages(
        &self,
        vm: &Vm,
        screen: ActorScene,
    ) -> Option<Vec<SongLuaStatefulMessageCapture>> {
        let store = vm.store_ref();
        if store.replay_frame?.0 != screen.id {
            return None;
        }
        Some(
            store
                .scenes
                .get(&screen.id)?
                .song_session
                .as_ref()?
                .messages
                .as_ref()?
                .output(),
        )
    }

    pub(crate) fn song_replay_tween_messages(
        &self,
        vm: &Vm,
        screen: ActorScene,
    ) -> Option<Vec<SongLuaStatefulMessageCapture>> {
        let store = vm.store_ref();
        if store.replay_frame?.0 != screen.id {
            return None;
        }
        Some(
            store
                .scenes
                .get(&screen.id)?
                .song_session
                .as_ref()?
                .messages
                .as_ref()?
                .output()
                .into_iter()
                .filter_map(|mut capture| {
                    capture
                        .writes
                        .retain(|w| crate::tween_pose::TARGETS.contains(&w.target));
                    for (_, targets) in &mut capture.overlay_targets {
                        targets.retain(|t| crate::tween_pose::TARGETS.contains(t));
                    }
                    capture.overlay_targets.retain(|(_, t)| !t.is_empty());
                    (!capture.writes.is_empty()).then_some(capture)
                })
                .collect(),
        )
    }
}

pub(super) fn timing(
    call: &mut HostCall<'_>,
    actor: TableId,
    duration: f32,
    easing: Option<&str>,
) -> Result<(), Error> {
    raw_set(
        call,
        actor,
        "__songlua_capture_duration",
        Value::Number(f64::from(duration)),
    )?;
    let easing = easing
        .map(|text| call.try_intern(text.as_bytes()))
        .transpose()?
        .unwrap_or(Value::Nil);
    raw_set(call, actor, "__songlua_capture_easing", easing)?;
    raw_set(call, actor, "__songlua_capture_opt1", Value::Nil)
}

pub(super) fn reset_timing(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let replaying = {
        let guard = call.store();
        let replay = guard.actors.get_mut(&actor).unwrap().replay.as_mut();
        if let Some(replay) = replay {
            replay.capture_open = false;
            true
        } else {
            false
        }
    };
    if replaying {
        timing(call, actor, 0., None)?;
    }
    Ok(())
}

pub(super) fn hurry_timing(
    call: &mut HostCall<'_>,
    actor: TableId,
    factor: f32,
) -> Result<(), Error> {
    let (replaying, flush, children) = {
        let guard = call.store();
        if guard.replay_frame.is_none() {
            return Ok(());
        }
        let actor = guard.actors.get_mut(&actor).unwrap();
        let flush = actor
            .replay
            .as_mut()
            .is_some_and(|r| std::mem::take(&mut r.capture_open));
        (actor.replay.is_some(), flush, actor.children.clone())
    };
    // Hurry flushes an open command block before scaling the retained queue.
    // Later pose writes still change that queue, but have no active block timing.
    if flush {
        timing(call, actor, 0., None)?;
    }
    if replaying {
        let duration = get(call, actor, b"__songlua_capture_duration")?;
        if let Some(duration) = float_scalar(call, duration, true) {
            raw_set(
                call,
                actor,
                "__songlua_capture_duration",
                Value::Number(f64::from(duration * factor)),
            )?;
        }
    }
    for child in children {
        hurry_timing(call, child, factor)?;
    }
    Ok(())
}

pub(super) fn record(
    call: &mut HostCall<'_>,
    id: TableId,
    delta: &crate::SongLuaOverlayStateDelta,
) {
    if !crate::tween_pose::TARGETS
        .iter()
        .any(|&target| delta.has_update_target(target))
    {
        return;
    }
    let context = {
        let guard = call.store();
        if guard.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let Some(message) = guard.active_message.as_ref() else {
            return;
        };
        let Some((owner, _)) = guard.replay_frame else {
            return;
        };
        if guard.retired_scenes.contains_key(&owner) {
            return;
        }
        let Some(capture) = guard
            .scenes
            .get(&owner)
            .and_then(|s| s.song_session.as_ref())
            .and_then(|s| s.messages.as_ref())
        else {
            return;
        };
        let Some(&index) = capture.indices.get(&id) else {
            return;
        };
        (owner, index, message.clone())
    };
    // Read the same per-write clock and command metadata as preparation. These
    // accesses can enter Lua, so no actor metadata borrow crosses them.
    let beat = effects::values(call, false).map_or(0., |values| values[0]);
    let duration = get(call, id, b"__songlua_capture_duration")
        .ok()
        .and_then(|v| float_scalar(call, v, true))
        .unwrap_or(0.);
    let easing = get(call, id, b"__songlua_capture_easing")
        .ok()
        .and_then(|v| render::text(call, v).map(str::to_owned));
    let opt1 = get(call, id, b"__songlua_capture_opt1")
        .ok()
        .and_then(|v| float_scalar(call, v, true));
    let guard = call.store();
    let Some(actor) = guard.actors.get(&id) else {
        return;
    };
    let delay = actor.capture.as_ref().map_or_else(
        || lifecycle::message_delay(actor, duration),
        |capture| capture.cursor().max(0.),
    );
    if guard.retired_scenes.contains_key(&context.0) {
        return;
    }
    let advance = guard
        .replay_clock
        .as_ref()
        .map_or(0., |clock| clock.actor_frame_advance(id));
    let Some(capture) = guard
        .scenes
        .get_mut(&context.0)
        .and_then(|s| s.song_session.as_mut())
        .and_then(|s| s.messages.as_mut())
    else {
        return;
    };
    let mut state = crate::SongLuaOverlayState::default();
    crate::apply_overlay_delta(&mut state, delta);
    let pose = TweenPose::from(state);
    for target in crate::tween_pose::TARGETS {
        if delta.has_update_target(target) {
            capture.write(
                context.2.clone(),
                SongLuaStatefulMessageWrite {
                    overlay_index: context.1,
                    beat,
                    target,
                    value: pose.get(target),
                    delay_seconds: delay,
                    duration_seconds: duration,
                    easing: easing.clone(),
                    opt1,
                },
                advance,
            );
        }
    }
}
