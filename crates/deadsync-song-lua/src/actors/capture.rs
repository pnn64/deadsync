//! Constructor and startup command capture, before compiler frame replay.
//! This is song-host scheduling state, not part of the Lua interpreter.
use super::*;
use crate::{SongLuaOverlayCommandBlock as Block, SongLuaOverlayStateDelta as Delta};

/// Owned command output and queue metadata for preparation/baking. These values
/// retain no Lua handles. Step zero denotes direct or immediate capture output.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorCommandCapture {
    pub blocks: Vec<Block>,
    pub block_steps: Vec<usize>,
    /// A command occupies a zero-duration queue state, represented by None.
    pub steps: Vec<Option<f32>>,
    pub queued_commands: Vec<(Vec<u8>, f32)>,
}

#[derive(Clone)]
struct Entry {
    changed: bool,
    block: Block,
    tween_step: usize,
}

#[derive(Clone)]
struct Spin {
    current: [f32; 3],
    queued: bool,
    queue: VecDeque<SpinTween>,
}

#[derive(Clone)]
struct SpinTween {
    duration: f32,
    left: f32,
    from: Option<[f32; 3]>,
    to: [f32; 3],
    easing: String,
}

#[derive(Clone, Default)]
pub(super) struct Capture {
    // Native preparation fields remain present after clearing capture blocks.
    // Cloning/restoring a capture also restores this semantic field presence.
    pub(super) transform_fields: u16,
    cursor: f32,
    pub(super) time_left: f32,
    current: Option<Entry>,
    blocks: Vec<Entry>,
    // Finish replaces the block list without replacing native's cached
    // immediate block. None as the index represents that detached block.
    immediate: Option<(f32, Option<usize>)>,
    steps: VecDeque<Option<f32>>,
    pending: VecDeque<(Vec<u8>, f32)>,
    /// A self-queue describes a repeating command; it must not recurse during
    /// discovery. Frame scheduling will consume this retained command later.
    pub(super) recurring: recurring::State,
    active_start: f32,
    active_exact: f64,
    spin: Option<Spin>,
    deferred: bool,
}

fn block(start: f32, duration: f32, easing: Option<String>, queued: bool) -> Block {
    Block {
        progress: None,
        queued,
        start,
        duration,
        easing,
        opt1: None,
        opt2: None,
        delta: Delta::default(),
    }
}

impl Capture {
    pub(super) fn begin_discovery(&mut self) {
        self.flush();
        self.blocks.clear();
        self.immediate = None;
    }

    pub(super) fn snapshot(&mut self) -> ActorCommandCapture {
        let blocks = self.blocks();
        ActorCommandCapture {
            blocks,
            block_steps: self.blocks.iter().map(|entry| entry.tween_step).collect(),
            steps: self.steps.iter().copied().collect(),
            queued_commands: self.pending.iter().cloned().collect(),
        }
    }

    pub(super) fn cursor(&self) -> f32 {
        self.cursor
    }

    pub(super) fn pending_steps(&self) -> usize {
        self.steps.len()
    }

    fn scheduled(&self) -> bool {
        self.cursor.max(0.) > f32::EPSILON
            || self
                .current
                .as_ref()
                .is_some_and(|entry| entry.block.duration.max(0.) > f32::EPSILON)
    }

    fn cancel(&mut self, finish: bool) {
        self.flush();
        if finish {
            let mut delta = Delta::default();
            for entry in &self.blocks {
                crate::merge_overlay_delta(&mut delta, &entry.block.delta);
            }
            self.blocks.clear();
            if !crate::overlay_delta_is_empty(&delta) {
                let mut final_block = block(0., 0., None, false);
                final_block.delta = delta;
                self.blocks.push(Entry {
                    changed: false,
                    block: final_block,
                    tween_step: 0,
                });
            }
            self.immediate = self.immediate.map(|(start, _)| (start, None));
        } else {
            self.blocks
                .retain(|entry| entry.tween_step == 0 && entry.block.duration == 0.);
            self.immediate = None;
        }
        self.cursor = 0.;
        self.time_left = 0.;
        self.steps.clear();
        self.pending.clear();
        self.recurring.command = None;
        self.recurring.remaining = None;
        self.deferred = false;
    }

    pub(super) fn hurry(&mut self, factor: f32) {
        if let Some(spin) = &mut self.spin {
            for tween in &mut spin.queue {
                tween.duration *= factor;
                tween.left *= factor;
            }
        }
        self.flush();
        for entry in &mut self.blocks {
            entry.block.start *= factor;
            entry.block.duration *= factor;
        }
        for seconds in self.steps.iter_mut().flatten() {
            *seconds *= factor;
        }
        self.cursor *= factor;
        self.time_left *= factor;
        self.active_exact *= f64::from(factor);
        if let Some(left) = &mut self.recurring.remaining {
            *left *= f64::from(factor);
        }
        if let Some((start, Some(_))) = &mut self.immediate {
            *start *= factor;
        }
        // Queue dispatch starts are separate from the scaled tween clocks;
        // the production constructor retains those original starts as well.
    }

    fn current(&mut self, queued: bool) -> &mut Entry {
        self.current.get_or_insert_with(|| Entry {
            changed: false,
            block: block(self.cursor, 0., None, queued),
            tween_step: self.steps.len(),
        })
    }

    pub(super) fn mark_current_changed(&mut self, queued: bool) {
        self.current(queued).changed = true;
    }

    pub(super) fn flush(&mut self) {
        if let Some(entry) = self.current.take() {
            self.cursor = entry.block.start + entry.block.duration.max(0.);
            if entry.changed || !crate::overlay_delta_is_empty(&entry.block.delta) {
                self.blocks.push(entry);
            }
        }
    }

    pub(super) fn exact_step(&mut self, duration: f64) {
        self.active_exact += duration;
    }

    pub(super) fn reset_command(&mut self) {
        self.reset_blocks();
        self.steps.clear();
    }

    // Compiler probes replace capture history, preserving semantic queues.
    pub(super) fn reset_blocks(&mut self) {
        self.cursor = 0.;
        self.time_left = 0.;
        self.current = None;
        self.blocks.clear();
        self.immediate = None;
    }

    pub(super) fn begin_tween(&mut self, duration: f32, easing: &str, queued: bool) {
        self.flush();
        if let Some(spin) = &mut self.spin {
            spin.queued = true;
            spin.queue.push_back(SpinTween {
                duration: duration.max(0.),
                left: duration.max(0.),
                from: None,
                to: spin.queue.back().map_or(spin.current, |t| t.to),
                easing: easing.to_owned(),
            });
        }
        self.steps.push_back(Some(duration));
        self.time_left = self.cursor + duration;
        self.current = Some(Entry {
            changed: false,
            block: block(self.cursor, duration, Some(easing.to_owned()), queued),
            tween_step: self.steps.len(),
        });
    }

    pub(super) fn write(&mut self, delta: Delta, immediate: bool, scope: Option<f32>) {
        for (index, value) in [
            delta.x,
            delta.y,
            delta.z,
            delta.rot_x_deg,
            delta.rot_z_deg,
            delta.rot_y_deg,
            delta.zoom,
            delta.zoom_x,
            delta.zoom_y,
            delta.zoom_z,
            delta.skew_x,
            delta.skew_y,
        ]
        .into_iter()
        .enumerate()
        {
            if value.is_some() {
                self.transform_fields |= 1 << index;
            }
        }
        let start = scope.unwrap_or(0.);
        let direct = start == 0.
            && self.cursor == 0.
            && self.current.as_ref().is_none_or(|c| c.block.duration == 0.)
            && self.steps.is_empty();
        let target = if !immediate || direct {
            &mut self.current(scope.is_some()).block.delta
        } else {
            let index = if let Some((_, index)) = self
                .immediate
                .filter(|(cached, _)| (*cached - start).abs() <= f32::EPSILON)
            {
                let Some(index) = index else { return };
                index
            } else {
                let index = self
                    .blocks
                    .iter()
                    .position(|entry| {
                        entry.block.start > start
                            || (entry.block.start == start
                                && (entry.tween_step > 0 || entry.block.duration > 0.))
                    })
                    .unwrap_or(self.blocks.len());
                self.blocks.insert(
                    index,
                    Entry {
                        changed: false,
                        block: block(start, 0., None, scope.is_some()),
                        tween_step: 0,
                    },
                );
                self.immediate = Some((start, Some(index)));
                index
            };
            &mut self.blocks[index].block.delta
        };
        crate::merge_overlay_delta(target, &delta);
    }

    pub(super) fn blocks(&mut self) -> Vec<Block> {
        self.flush();
        self.blocks
            .iter()
            .map(|entry| {
                let mut block = entry.block.clone();
                block.duration = block.duration.max(0.);
                block
            })
            .collect()
    }
}

pub(super) fn write(call: &mut HostCall<'_>, id: TableId, delta: Delta, immediate: bool) {
    let guard = call.store();
    let scope = guard.capture_scope;
    let fallback = song_pose::mode(&guard, id);
    if let Some(actor) = guard.actors.get_mut(&id) {
        record(actor, &delta, immediate, scope, fallback);
    }
    #[cfg(feature = "test-support")]
    song_messages::pose_write(call, id, &delta);
    if !song_pose::record(call, id, &delta) {
        song_messages::record(call, id, &delta);
    }
    song_immediate::delta(call, id, &delta, immediate);
    song_media::delta(call, id, &delta);
}

/// Frame publication also supplies command blocks during construction/probes.
/// Capture::write already retains its delta, so its frame path skips this hook.
pub(super) fn retain_render_update(
    store: &mut Store,
    id: TableId,
    target: crate::SongLuaOverlayUpdateTarget,
    value: &crate::SongLuaOverlayUpdateValue,
    immediate: bool,
) {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update};
    let guard = &mut *store;
    if !guard.preparing || guard.replay_frame.is_some() {
        return;
    }
    let mut delta = Delta::default();
    match (target, value) {
        (Target::Visible, Update::Bool(value)) => delta.visible = Some(*value),
        (Target::Hibernating, Update::Bool(value)) => delta.hibernating = Some(*value),
        (Target::HAlign, Update::F32(value)) => delta.halign = Some(*value),
        (Target::VAlign, Update::F32(value)) => delta.valign = Some(*value),
        (Target::TextAlign, Update::TextAlign(value)) => delta.text_align = Some(*value),
        (Target::BaseZoom, Update::F32(value)) => delta.basezoom = Some(*value),
        (Target::BaseZoomX, Update::F32(value)) => delta.basezoom_x = Some(*value),
        (Target::BaseZoomY, Update::F32(value)) => delta.basezoom_y = Some(*value),
        (Target::BaseZoomZ, Update::F32(value)) => delta.basezoom_z = Some(*value),
        (Target::BaseAlpha, Update::F32(value)) => delta.base_alpha = Some(*value),
        (Target::BaseRotation, Update::Vec3(value)) => delta.base_rotation = Some(*value),
        (Target::ShadowLen, Update::Vec2(value)) => delta.shadow_len = Some(*value),
        (Target::ShadowColor, Update::Vec4(value)) => delta.shadow_color = Some(*value),
        (Target::Size, Update::Vec2(value)) => delta.size = Some(*value),
        (Target::SpriteTexture, Update::Bool(value)) => delta.sprite_texture = Some(*value),
        (Target::SpriteStateIndex, Update::U32(value)) => delta.sprite_state_index = Some(*value),
        (Target::Vibrate, Update::Bool(value)) => delta.vibrate = Some(*value),
        (Target::Rainbow, Update::Bool(value)) => delta.rainbow = Some(*value),
        (Target::EffectMode, Update::EffectMode(value)) => delta.effect_mode = Some(*value),
        (Target::EffectPeriod, Update::F32(value)) => delta.effect_period = Some(*value),
        (Target::EffectTiming, Update::Vec5(value)) => delta.effect_timing = Some(*value),
        (Target::EffectMagnitude, Update::Vec3(value)) => delta.effect_magnitude = Some(*value),
        (Target::EffectColor1, Update::Vec4(value)) => delta.effect_color1 = Some(*value),
        (Target::EffectColor2, Update::Vec4(value)) => delta.effect_color2 = Some(*value),
        (Target::EffectOffset, Update::F32(value)) => delta.effect_offset = Some(*value),
        (Target::EffectClock, Update::EffectClock(value)) => delta.effect_clock = Some(*value),
        (Target::EffectNoOffset, Update::Bool(value)) => delta.effect_no_offset = Some(*value),
        (Target::EffectTimer, Update::Bool(value)) => delta.effect_timer = Some(*value),
        (Target::CullMode, Update::CullMode(value)) => delta.cull_mode = Some(*value),
        (Target::AftPreserve, Update::Bool(value)) => delta.aft_preserve = Some(*value),
        (Target::AftCreated, Update::Bool(value)) => delta.aft_created = Some(*value),
        (Target::ZBias, Update::F32(value)) => delta.z_bias = Some(*value),
        (Target::DrawOrder, Update::I32(value)) => delta.draw_order = Some(*value),
        (Target::DrawByZPosition, Update::Bool(value)) => delta.draw_by_z_position = Some(*value),
        (Target::Fov, Update::F32(value)) => delta.fov = Some(*value),
        (Target::Vanishpoint, Update::Vec2(value)) => delta.vanishpoint = Some(*value),
        (Target::Blend, Update::Blend(value)) => delta.blend = Some(*value),
        (Target::DepthTest, Update::Bool(value)) => delta.depth_test = Some(*value),
        (Target::DepthTestOnFail, Update::Bool(value)) => delta.depth_test_on_fail = Some(*value),
        (Target::DepthWrite, Update::Bool(value)) => delta.depth_write = Some(*value),
        (Target::TextureFiltering, Update::Bool(value)) => delta.texture_filtering = Some(*value),
        _ => return,
    }
    let scope = guard.capture_scope;
    let fallback = song_pose::mode(&guard, id);
    if let Some(actor) = guard.actors.get_mut(&id) {
        record(actor, &delta, immediate, scope, fallback);
    }
}

fn record(actor: &mut Actor, delta: &Delta, immediate: bool, scope: Option<f32>, fallback: bool) {
    if let Some(replay) = &mut actor.replay {
        for target in crate::tween_pose::TARGETS {
            if delta.has_update_target(target) {
                replay.targets |= 1u128 << target as usize;
            }
        }
    }
    if let Some(pose) = actor
        .replay
        .as_mut()
        .and_then(|replay| replay.getters.as_mut())
    {
        let mut state = crate::SongLuaOverlayState::default();
        pose.write_to(&mut state);
        crate::apply_overlay_delta(&mut state, delta);
        *pose = state.into();
    }
    if actor.replay.is_some() && !fallback {
        let current = actor.current;
        if let Some(getters) = &mut actor.preparation_getters {
            for ((getter, changed), value) in getters
                .iter_mut()
                .zip([delta.x, delta.y, delta.z, delta.aux])
                .zip([current.x, current.y, current.z, current.aux])
            {
                if changed.is_some() {
                    *getter = value;
                }
            }
        }
    }
    let Some(capture) = &mut actor.capture else {
        return;
    };
    if let Some(spin) = &mut capture.spin {
        let rotation = if let Some(tween) = spin.queue.back_mut() {
            &mut tween.to
        } else {
            &mut spin.current
        };
        for (axis, value) in
            rotation
                .iter_mut()
                .zip([delta.rot_x_deg, delta.rot_y_deg, delta.rot_z_deg])
        {
            if let Some(value) = value {
                *axis = value;
            }
        }
    }
    // Zero-duration construction steps do not defer current-position getters.
    // These fields are independent of the retained destination/spin queue.
    if !capture.scheduled() && !fallback {
        let values = [delta.x, delta.y, delta.z, delta.aux];
        if values.iter().any(Option::is_some) {
            let current = actor.current;
            let getters = actor.preparation_getters.get_or_insert([
                current.x,
                current.y,
                current.z,
                current.aux,
            ]);
            for (getter, value) in getters.iter_mut().zip(values) {
                if let Some(value) = value {
                    *getter = value;
                }
            }
        }
    }
    capture.write(*delta, immediate, scope);
}

pub(super) fn cancel(actor: &mut Actor, finish: bool) -> bool {
    if actor.capture.is_none() {
        return false;
    }
    let current = actor.current;
    let getters =
        actor
            .preparation_getters
            .unwrap_or([current.x, current.y, current.z, current.aux]);
    let mut dest = *actor.destination();
    if !finish {
        [dest.x, dest.y, dest.z] = [getters[0], getters[1], getters[2]];
    }
    if let Some(spin) = &mut actor.capture.as_mut().unwrap().spin {
        if finish && let Some(tween) = spin.queue.back() {
            spin.current = tween.to;
        }
        [dest.rot_x_deg, dest.rot_y_deg, dest.rot_z_deg] = spin.current;
        spin.queued = false;
        spin.queue.clear();
    }
    actor.capture.as_mut().unwrap().cancel(finish);
    actor.current = dest;
    actor.queue.clear();
    actor.sync_rotation_getters();
    actor.preparation_getters = Some(getters);
    true
}

pub(super) fn start_spin(actor: &mut Actor) {
    let pose = *actor.destination();
    if let Some(capture) = &mut actor.capture {
        capture.spin.get_or_insert(Spin {
            current: [pose.rot_x_deg, pose.rot_y_deg, pose.rot_z_deg],
            queued: false,
            queue: VecDeque::new(),
        });
    }
}

/// Fallback preparation keeps rotation separate from exported destination
/// getters. Spin advances first; every tween then interpolates that complete
/// rotation, including a tween which only changes alpha.
pub(super) fn spin_rotation(actor: &Actor) -> Option<[f32; 3]> {
    Some(actor.capture.as_ref()?.spin.as_ref()?.current)
}

pub(super) fn advance_spin(actor: &mut Actor, elapsed: f32, delta: f32) {
    let spinning = actor.state.render.effect_mode == deadlib_present::anim::EffectMode::Spin;
    let magnitude = actor.state.render.effect_magnitude;
    let Some(spin) = actor.capture.as_mut().and_then(|c| c.spin.as_mut()) else {
        return;
    };
    if spinning {
        for (angle, speed) in spin.current.iter_mut().zip(magnitude) {
            *angle = (*angle + elapsed * speed).rem_euclid(360.);
        }
    }
    let mut remaining = delta.max(0.);
    while remaining > 0. {
        let Some(tween) = spin.queue.front_mut() else {
            break;
        };
        let from = *tween.from.get_or_insert(spin.current);
        let consumed = tween.left.min(remaining);
        tween.left -= consumed;
        remaining -= consumed;
        if tween.left == 0. {
            spin.current = tween.to;
            spin.queue.pop_front();
        } else {
            let factor = crate::overlay_command_ease_factor(
                Some(&tween.easing),
                1. - tween.left / tween.duration,
                None,
            );
            for ((angle, from), to) in spin.current.iter_mut().zip(from).zip(tween.to) {
                *angle = (1. - factor) * from + factor * to;
            }
        }
    }
    spin.queued = !spin.queue.is_empty();
    let rotation = spin.current;
    let empty = !spin.queued;
    [
        actor.current.rot_x_deg,
        actor.current.rot_y_deg,
        actor.current.rot_z_deg,
    ] = rotation;
    if empty {
        let destination = actor.destination_mut();
        [
            destination.rot_x_deg,
            destination.rot_y_deg,
            destination.rot_z_deg,
        ] = rotation;
        actor.state.rotation = rotation;
    }
}

/// Record an actual successful write, including writes equal to the old value.
/// Comparing snapshots would lose those writes and change later composition.
pub(super) fn property(call: &mut HostCall<'_>, id: TableId, name: &str) {
    let guard = call.store();
    let fallback = song_pose::mode(&guard, id);
    let scope = guard.capture_scope;
    let immediate_visibility = scope.is_some()
        || guard.startup_queues.is_some()
        || guard.startup_discovery
        || guard.replay_frame.is_some();
    let actor = guard.actors.get_mut(&id).unwrap();
    if matches!(
        name,
        "x" | "y"
            | "z"
            | "addx"
            | "addy"
            | "addz"
            | "Center"
            | "CenterX"
            | "CenterY"
            | "aux"
            | "zoom"
            | "zoomx"
            | "zoomy"
            | "zoomz"
            | "zoomto"
            | "zoomtowidth"
            | "zoomtoheight"
            | "scaletofit"
            | "scaletocover"
            | "stretchto"
            | "rotationx"
            | "rotationy"
            | "rotationz"
            | "diffuse"
            | "diffusealpha"
            | "glow"
            | "vertex_colors"
            | "skewx"
            | "skewy"
            | "cropleft"
            | "cropright"
            | "croptop"
            | "cropbottom"
            | "fadeleft"
            | "faderight"
            | "fadetop"
            | "fadebottom"
            | "heading"
            | "pitch"
            | "roll"
    ) {
        actor.pose_exported = true;
    }
    if actor.capture.is_none() && actor.replay.is_none() {
        return;
    }
    let pose = actor.destination();
    let state = &actor.state;
    let render = &state.render;
    let mut delta = Delta::default();
    let mut immediate = false;
    match name {
        "x" | "addx" | "CenterX" => delta.x = Some(pose.x),
        "y" | "addy" | "CenterY" => delta.y = Some(pose.y),
        "z" | "addz" => delta.z = Some(pose.z),
        "Center" => {
            delta.x = Some(pose.x);
            delta.y = Some(pose.y);
        }
        "zoom" => {
            delta.zoom = Some(pose.zoom);
            delta.zoom_x = Some(pose.zoom_x);
            delta.zoom_y = Some(pose.zoom_y);
            delta.zoom_z = Some(pose.zoom_z);
        }
        "zoomx" | "zoomtowidth" => delta.zoom_x = Some(pose.zoom_x),
        "zoomy" | "zoomtoheight" => delta.zoom_y = Some(pose.zoom_y),
        "zoomz" => delta.zoom_z = Some(pose.zoom_z),
        "zoomto" => {
            delta.zoom_x = Some(pose.zoom_x);
            delta.zoom_y = Some(pose.zoom_y);
        }
        "scaletofit" | "scaletocover" => {
            delta.x = Some(pose.x);
            delta.y = Some(pose.y);
            delta.zoom = Some(pose.zoom);
            delta.zoom_x = Some(pose.zoom_x);
            delta.zoom_y = Some(pose.zoom_y);
            delta.zoom_z = Some(pose.zoom_z);
        }
        "stretchto" => {
            delta.x = Some(pose.x);
            delta.y = Some(pose.y);
            delta.zoom_x = Some(pose.zoom_x);
            delta.zoom_y = Some(pose.zoom_y);
        }
        "rotationx" | "addrotationx" => delta.rot_x_deg = Some(pose.rot_x_deg),
        "rotationy" | "addrotationy" => delta.rot_y_deg = Some(pose.rot_y_deg),
        "rotationz" | "addrotationz" => delta.rot_z_deg = Some(pose.rot_z_deg),
        "aux" => delta.aux = Some(pose.aux),
        "visible" => {
            delta.visible = Some(state.visible);
            immediate = immediate_visibility;
        }
        "draworder" => delta.draw_order = Some(state.draw_order),
        "size" | "SetWidth" | "SetHeight" | "setsize" => {
            delta.size = Some(state.size);
            immediate = true;
        }
        "diffuse" | "diffusecolor" | "diffusealpha" => delta.diffuse = Some(pose.diffuse),
        "glow" => delta.glow = Some(pose.glow),
        "vertex_colors" => delta.vertex_colors = pose.vertex_colors,
        "cropleft" => delta.cropleft = Some(pose.cropleft),
        "cropright" => delta.cropright = Some(pose.cropright),
        "croptop" => delta.croptop = Some(pose.croptop),
        "cropbottom" => delta.cropbottom = Some(pose.cropbottom),
        "fadeleft" => delta.fadeleft = Some(pose.fadeleft),
        "faderight" => delta.faderight = Some(pose.faderight),
        "fadetop" => delta.fadetop = Some(pose.fadetop),
        "fadebottom" => delta.fadebottom = Some(pose.fadebottom),
        "skewx" => delta.skew_x = Some(pose.skew_x),
        "skewy" => delta.skew_y = Some(pose.skew_y),
        "heading" | "pitch" | "roll" => delta.quaternion = Some(pose.quaternion),
        "halign" => {
            delta.halign = Some(render.halign);
            immediate = true;
        }
        "valign" => {
            delta.valign = Some(render.valign);
            immediate = true;
        }
        // Immediate renderer/base-transform properties do not join a tween.
        "basealpha" => {
            delta.base_alpha = Some(render.base_alpha);
            immediate = true;
        }
        "basezoom" => {
            delta.basezoom = Some(render.basezoom);
            delta.basezoom_x = Some(render.basezoom_x);
            delta.basezoom_y = Some(render.basezoom_y);
            delta.basezoom_z = Some(render.basezoom_z);
            immediate = true;
        }
        "basezoomx" => {
            delta.basezoom_x = Some(render.basezoom_x);
            immediate = true;
        }
        "basezoomy" => {
            delta.basezoom_y = Some(render.basezoom_y);
            immediate = true;
        }
        "basezoomz" => {
            delta.basezoom_z = Some(render.basezoom_z);
            immediate = true;
        }
        "baserotationx" | "baserotationy" | "baserotationz" => {
            delta.base_rotation = Some(render.base_rotation);
            immediate = true;
        }
        "shadowlength" | "shadowlengthx" | "shadowlengthy" => {
            delta.shadow_len = Some(render.shadow_len);
            immediate = true;
        }
        "shadowcolor" => {
            delta.shadow_color = Some(render.shadow_color);
            immediate = true;
        }
        "zbias" => delta.z_bias = Some(render.z_bias),
        "fov" => delta.fov = render.fov,
        _ => return,
    }
    record(actor, &delta, immediate, scope, fallback);
    #[cfg(feature = "test-support")]
    song_messages::pose_write(call, id, &delta);
    if !song_pose::record(call, id, &delta) {
        song_messages::record(call, id, &delta);
    }
    song_immediate::delta(call, id, &delta, immediate);
    song_media::delta(call, id, &delta);
}

// Once a fallback actor queues during an update, that queue belongs to frame
// advancement. Captured commands outside the update instead drain synchronously.
pub(super) fn enter_replay_queue(store: &mut Store, actor: TableId) {
    let guard = &mut *store;
    // Full-timeline preparation retains its fallback capture and command queue.
    // Streamed preparation continues to use the ordinary queue below.
    if guard.replay_clock.is_some() {
        return;
    }
    if guard.replay_frame.is_some() && guard.replay_advancing {
        let actor = guard.actors.get_mut(&actor).unwrap();
        if actor
            .capture
            .as_ref()
            .is_some_and(|c| c.recurring.command.is_some() || c.recurring.running)
        {
            return;
        }
        if actor.capture.take().is_some() {
            // Captured steps have already emitted their timed publications.
            // The first update-created queue starts its own clock; those old
            // durations must not postpone commands queued by this callback.
            actor.queue.clear();
        }
    }
}

pub(super) fn queue(call: &mut HostCall<'_>, actor: TableId, name: Vec<u8>) -> Result<bool, Error> {
    action_capture::prepare(call, actor)?;
    enter_replay_queue(call.store(), actor);
    let mut command_field = name.clone();
    command_field.extend_from_slice(b"Command");
    let overflow = {
        let guard = call.store();
        let target = &guard.actors[&actor];
        !target.active_commands.contains(&command_field)
            && target.capture.as_ref().is_some_and(|capture| {
                guard
                    .replay_clock
                    .as_ref()
                    .and_then(|clock| clock.queue_len(actor))
                    .unwrap_or_else(|| capture.pending_steps())
                    > 50
            })
    };
    if overflow {
        lifecycle::finish(call, actor)?;
    }
    let full_capture = {
        let guard = call.store();
        guard.replay_clock.is_some() && guard.actors[&actor].capture.is_some()
    };
    if full_capture {
        song_messages::timing(call, actor, 0., None)?;
    }
    let drain = {
        let guard = call.store();
        let startup = guard.startup_queues.is_some();
        let self_queue = guard.actors[&actor]
            .active_commands
            .contains(&command_field);
        if guard.actors[&actor].capture.is_some()
            && let Some(clock) = &mut guard.replay_clock
        {
            if self_queue {
                clock.flush(actor);
            } else {
                clock.tween(actor, None);
            }
        }
        let actor = guard.actors.get_mut(&actor).unwrap();
        let Some(capture) = &mut actor.capture else {
            return Ok(false);
        };
        capture.flush();
        let mut field = name.clone();
        field.extend_from_slice(b"Command");
        // Startup collection must retain the first self-queued command for
        // chronological replay. Its capture is replaced when replay begins,
        // so converting it to a recurring capture here would lose the queue.
        if !startup && actor.active_commands.contains(&field) {
            let interval = if capture.active_exact > f64::EPSILON {
                capture.active_exact
            } else {
                f64::from((capture.cursor - capture.active_start).max(0.))
            };
            capture.recurring.command = Some(name);
            if interval > f64::EPSILON {
                capture.recurring.interval = interval;
            }
            return Ok(true);
        }
        capture.steps.push_back(None);
        capture.pending.push_back((name, capture.cursor));
        startup || actor.active_commands.is_empty()
    };
    if call.store().replay_frame.is_some() {
        song_messages::timing(call, actor, 0., None)?;
    }
    if drain {
        drain_commands(call, actor)?;
    }
    Ok(true)
}

pub(super) fn advance_ready(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let ready = {
        let guard = call.store();
        guard.actors[&actor].registered
            && guard
                .replay_clock
                .as_ref()
                .is_some_and(|clock| clock.ready(actor))
    };
    if !ready {
        return Ok(());
    }
    match drain_commands(call, actor) {
        Err(error) => diagnostics::report(call, actor, diagnostics::Callback::Queue, error),
        Ok(()) => Ok(()),
    }
}

pub(super) fn drain_commands(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    {
        let guard = call.store();
        let pending = guard
            .actors
            .get(&actor)
            .and_then(|actor| actor.capture.as_ref())
            .is_some_and(|capture| !capture.pending.is_empty());
        if let Some(queued) = &mut guard.startup_queues {
            if pending && !queued.contains(&actor) {
                queued.push(actor);
            }
            return Ok(());
        }
    }
    loop {
        let (next, clock_time) = {
            let guard = call.store();
            if !guard
                .actors
                .get(&actor)
                .and_then(|a| a.capture.as_ref())
                .is_some_and(|c| !c.pending.is_empty())
            {
                return Ok(());
            }
            let mut time = None;
            if let Some(clock) = &mut guard.replay_clock {
                clock.prime(actor);
                if clock.waiting(actor) {
                    return Ok(());
                }
                time = clock.pending_dispatch(actor);
            }
            let discovery = guard.startup_discovery;
            let full = guard.replay_clock.is_some();
            let capture = guard
                .actors
                .get_mut(&actor)
                .unwrap()
                .capture
                .as_mut()
                .unwrap();
            let (name, start) = capture.pending.front().unwrap();
            if !full && (discovery || capture.deferred) && (name.starts_with(b"!") || *start > 0.) {
                capture.deferred = true;
                return Ok(());
            }
            let next = capture.pending.pop_front().unwrap();
            while let Some(step) = capture.steps.pop_front() {
                if step.is_none() {
                    break;
                }
            }
            (next, time)
        };
        let (name, captured_start) = next;
        let start = if clock_time.is_some() {
            0.
        } else {
            captured_start
        };
        let (prior, previous_dispatch, previous_clock, was_recurring, remainder, cursor) = {
            let guard = call.store();
            let mut previous_clock = guard.replay_clock.as_ref().and_then(|c| c.dispatch);
            let mut seconds = None;
            let mut cursor = None;
            if let Some(clock) = &mut guard.replay_clock {
                if clock_time.is_some() {
                    previous_clock = clock.dispatched(actor, 0.);
                    seconds = clock.dispatch.map(|scope| clock.time(scope.frame));
                    cursor = Some(clock.cursor(actor));
                } else {
                    clock.dispatch = None;
                }
            }
            let capture = guard
                .actors
                .get_mut(&actor)
                .unwrap()
                .capture
                .as_mut()
                .unwrap();
            if let Some(cursor) = cursor {
                capture.flush();
                capture.cursor = cursor;
            }
            let was_recurring = capture.recurring.command.is_some();
            let prior = guard.capture_scope.replace(start);
            let dispatch = guard.replay_dispatch;
            if guard.replay_frame.is_some() {
                guard.replay_dispatch = Some(song_immediate::Dispatch {
                    seconds,
                    delay: start,
                });
            }
            (
                prior,
                dispatch,
                previous_clock,
                was_recurring,
                clock_time.map(|t| t.1 as f32),
                cursor,
            )
        };
        if let Some(cursor) = cursor {
            let result = (|| {
                song_messages::timing(call, actor, 0., None)?;
                song_replay::raw_set(
                    call,
                    actor,
                    "__songlua_capture_cursor",
                    Value::Number(f64::from(cursor)),
                )
            })();
            if let Err(error) = result {
                let guard = call.store();
                guard.capture_scope = prior;
                guard.replay_dispatch = previous_dispatch;
                if let Some(clock) = &mut guard.replay_clock {
                    clock.dispatch = previous_clock;
                }
                return Err(error);
            }
        }
        // The registry and this lexical root retain the dispatch target even
        // when its command edits the scene and forces a collection.
        #[cfg(feature = "test-support")]
        {
            call.store().audit_queued_depth += 1;
        }
        let result = call.with_roots(&[Value::Table(actor)], |call| {
            if let Some(message) = name.strip_prefix(b"!") {
                messages::queued(call, message)
            } else {
                let mut field = name.clone();
                field.extend_from_slice(b"Command");
                let previous = call
                    .store()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .active_commands
                    .remove(&field);
                let result = dispatch(call, actor, &name);
                if previous {
                    call.store()
                        .actors
                        .get_mut(&actor)
                        .unwrap()
                        .active_commands
                        .insert(field);
                }
                result
            }
        });
        {
            let guard = call.store();
            guard.capture_scope = prior;
            guard.replay_dispatch = previous_dispatch;
            #[cfg(feature = "test-support")]
            {
                guard.audit_queued_depth -= 1;
            }
            if let Some(clock) = &mut guard.replay_clock {
                clock.dispatch = previous_clock;
            }
        }
        result?;
        song_pose::advance_position(call, actor)?;
        let recurring = call
            .store()
            .actors
            .get(&actor)
            .and_then(|a| a.capture.as_ref())
            .is_some_and(|c| c.recurring.command.is_some());
        if !was_recurring
            && recurring
            && let Some(remainder) = remainder
        {
            recurring::advance(call, actor, remainder)?;
            if let Some(clock) = &mut call.store().replay_clock {
                clock.mark_recurring(actor);
            }
        }
    }
}

fn dispatch(call: &mut HostCall<'_>, actor: TableId, name: &[u8]) -> Result<(), Error> {
    {
        let guard = call.store();
        let start = guard.capture_scope.unwrap_or(0.);
        if let Some(capture) = guard
            .actors
            .get_mut(&actor)
            .and_then(|a| a.capture.as_mut())
            && capture.cursor < start
        {
            capture.flush();
            capture.cursor = start;
        }
    }
    native_children::run_command(call, actor, name, None)?;
    let kind = sprites::field(call, actor, "__songlua_actor_type")?;
    if matches!(kind, Value::String(id) if matches!(call.string_bytes(id), Some(b"ActorFrame" | b"ActorFrameTexture" | b"TopScreen")))
    {
        native_children::with_children(call, actor, |call, child| dispatch(call, child, name))?;
    }
    Ok(())
}

pub(super) fn command_start(store: &mut Store, actor: TableId) -> Option<(f32, f64)> {
    let guard = &mut *store;
    let capture = guard.actors.get_mut(&actor)?.capture.as_mut()?;
    Some((
        std::mem::replace(&mut capture.active_start, capture.cursor),
        std::mem::take(&mut capture.active_exact),
    ))
}

pub(super) fn command_end(store: &mut Store, actor: TableId, previous: Option<(f32, f64)>) {
    if let Some(previous) = previous
        && let Some(capture) = store
            .actors
            .get_mut(&actor)
            .and_then(|a| a.capture.as_mut())
    {
        (capture.active_start, capture.active_exact) = previous;
    }
}

impl Actors {
    pub(crate) fn begin_preparation(&self, vm: &mut Vm) -> Result<bool, Error> {
        {
            let guard = vm.store();
            if guard.preparing
                || guard.playback
                || guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
            {
                return Ok(false);
            }
        }
        let function = vm.rooted(self.prepare_definitions)?;
        vm.call(function, &[])?;
        let guard = vm.store();
        guard.preparing = true;
        guard.constructing_capture = true;
        Ok(true)
    }

    pub(crate) fn discover_startup(&self, vm: &mut Vm) -> Result<(), Error> {
        let function = vm.rooted(self.discover_startup)?;
        vm.call(function, &[])?;
        Ok(())
    }

    pub(super) fn discover_queued(&self, vm: &mut Vm, queued: Vec<TableId>) -> Result<(), Error> {
        let (roots, pin) = retained::values(vm, queued.iter().copied().map(Value::Table))?;
        let function = vm.new_host_function_with_data(
            |call| {
                let queued = call.function_data::<Vec<TableId>>().unwrap().clone();
                run_discovery(call, queued)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(roots),
            ],
            queued,
        );
        vm.unpin(pin)?;
        let function = function?;
        vm.call(function, &[])?;
        Ok(())
    }

    pub(crate) fn command_capture(&self, vm: &mut Vm, actor: Value) -> Option<ActorCommandCapture> {
        let Value::Table(actor) = actor else {
            return None;
        };
        let guard = vm.store();
        let capture = guard.actors.get_mut(&actor)?.capture.as_mut()?;
        Some(capture.snapshot())
    }

    pub(crate) fn captured_blocks(&self, vm: &mut Vm, actor: Value) -> Option<Vec<Block>> {
        let Value::Table(actor) = actor else {
            return None;
        };
        let guard = vm.store();
        Some(guard.actors.get_mut(&actor)?.capture.as_mut()?.blocks())
    }
}

pub(super) fn discover_startup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let queued = call.store().startup_queues.take();
    let Some(queued) = queued else {
        return Ok(());
    };
    run_discovery(call, queued)
}

fn run_discovery(call: &mut HostCall<'_>, queued: Vec<TableId>) -> Result<(), Error> {
    let previous = std::mem::replace(&mut call.store().startup_discovery, true);
    let roots = queued.iter().copied().map(Value::Table).collect::<Vec<_>>();
    let result = call.with_roots(&roots, |call| {
        for actor in queued {
            drain_commands(call, actor)?;
        }
        Ok(())
    });
    call.store().startup_discovery = previous;
    result
}

pub(super) fn has_recurring(store: &mut Store, actor: TableId) -> bool {
    store
        .actors
        .get(&actor)
        .and_then(|actor| actor.capture.as_ref())
        .is_some_and(|capture| capture.recurring.command.is_some())
}

pub(super) fn deferred(store: &mut Store, actor: TableId) -> bool {
    store
        .actors
        .get(&actor)
        .and_then(|a| a.capture.as_ref())
        .is_some_and(|c| c.deferred)
}
