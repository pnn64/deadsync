//! Shared owned-data track compilation for chronological song preparation.
use crate::{
    SongLuaCompileContext, SongLuaOverlayState, SongLuaOverlayUpdateSample,
    SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateValue,
    song_beat_at_elapsed_seconds,
};

/// Owned startup queue output shared by preparation and regression fixtures.
#[derive(Clone, Debug, PartialEq)]
pub struct StartupState {
    pub initial: crate::SongLuaOverlayState,
    pub blocks: Vec<crate::SongLuaOverlayCommandBlock>,
    pub steps: Vec<f32>,
    pub commands: Vec<Option<String>>,
    pub block_steps: Vec<Option<usize>>,
    pub clock_path: Vec<[f32; 2]>,
}

pub(crate) type PlayerProbe<'a> = &'a mut dyn FnMut() -> Result<bool, String>;
pub(crate) type CaptureVisitor<'a> = dyn FnMut(
        usize,
        &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
        &[ScheduledUpdate],
        &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
        bool,
        PlayerProbe<'_>,
    ) -> Result<(), String>
    + 'a;
pub(crate) trait CaptureSource {
    fn actor_count(&self) -> usize;
    fn chronological(&self) -> bool;
    fn drain(&mut self, visit: &mut CaptureVisitor<'_>) -> Result<(), String>;
    fn reset(&mut self, index: usize) -> Result<(), String>;
}

pub(crate) struct CaptureFrameArgs<'a, H: std::hash::BuildHasher> {
    pub context: &'a SongLuaCompileContext,
    pub baseline: &'a [SongLuaOverlayState],
    pub from_states: &'a [SongLuaOverlayState],
    pub update_states: &'a mut [SongLuaOverlayState],
    pub to_states: &'a mut [SongLuaOverlayState],
    pub restored_indices: &'a [usize],
    pub tracks: &'a mut Vec<SongLuaOverlayUpdateTrack>,
    pub track_indices:
        &'a mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize, H>,
    pub time: f32,
    pub next_time: f32,
    pub next_seconds: f64,
    pub scheduled_samples: &'a mut Vec<SongLuaScheduledOverlaySample>,
    pub scratch: &'a mut OverlaySampleScratch,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledUpdate {
    /// Immutable native time-left samples baked at load, shared by all writes
    /// to the queue's back tween. No sampling work or allocation during play.
    pub progress: Option<std::sync::Arc<[[f32; 2]]>>,
    /// Queued commands cannot render before their actual replay frame.
    pub dispatch_seconds: Option<f64>,
    pub frame_advance: f32,
    pub initial_value: Option<SongLuaOverlayUpdateValue>,
    pub delay_seconds: f32,
    pub duration_seconds: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub target: SongLuaOverlayUpdateTarget,
    pub value: SongLuaOverlayUpdateValue,
}

pub(crate) const PLAYER_TRANSFORM_TARGETS: [SongLuaOverlayUpdateTarget; 11] = {
    use SongLuaOverlayUpdateTarget as Target;
    [
        Target::X,
        Target::Y,
        Target::Z,
        Target::RotationX,
        Target::RotationZ,
        Target::RotationY,
        Target::ZoomX,
        Target::ZoomY,
        Target::ZoomZ,
        Target::SkewX,
        Target::SkewY,
    ]
};

// Close an idle interval before a command that has already changed the current
// state. Keep that command at its original frame rather than ramping across the
// idle interval or delaying it until the following capture frame.
pub(crate) fn push_overlay_gap(
    samples: &mut Vec<SongLuaOverlayUpdateSample>,
    time: f32,
    next_time: f32,
    current: impl FnOnce() -> SongLuaOverlayUpdateValue,
) {
    let Some(last) = samples.last() else {
        return;
    };
    if last.time >= time - f32::EPSILON {
        return;
    }
    let current = current();
    let hold_time = time - (next_time - time);
    if last.value != current && hold_time > last.time + f32::EPSILON {
        let value = last.value.clone();
        samples.push(SongLuaOverlayUpdateSample {
            time: hold_time,
            value,
        });
    }
    samples.push(SongLuaOverlayUpdateSample {
        time,
        value: current,
    });
}

pub(crate) fn push_update_overlay_value(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    overlay_index: usize,
    target: crate::SongLuaOverlayUpdateTarget,
    time: f32,
    current: crate::SongLuaOverlayUpdateValue,
    next_time: f32,
    next: crate::SongLuaOverlayUpdateValue,
) -> usize {
    let key = (overlay_index, target);
    let track_index = match track_indices.get(&key).copied() {
        Some(index) => index,
        None => {
            let index = tracks.len();
            track_indices.insert(key, index);
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index,
                target,
                // A target enters capture only when UpdateCommand writes it.
                // Do not invent a ramp from the actor default before that first
                // write; the runtime applies the first sampled value as a step.
                samples: vec![SongLuaOverlayUpdateSample {
                    time: next_time,
                    value: next,
                }],
            });
            return index;
        }
    };
    if current == next {
        return track_index;
    }
    let track = &mut tracks[track_index];
    push_overlay_gap(&mut track.samples, time, next_time, || current);
    track.samples.push(SongLuaOverlayUpdateSample {
        time: next_time,
        value: next,
    });
    track_index
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_captured_overlay_value(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    time: f32,
    current: &SongLuaOverlayState,
    next_time: f32,
    next: &SongLuaOverlayUpdateValue,
) -> usize {
    let key = (overlay_index, target);
    let track_index = match track_indices.get(&key).copied() {
        Some(index) => index,
        None => {
            let index = tracks.len();
            track_indices.insert(key, index);
            // The first write is a step. Its unused current value needs no
            // snapshot, including when that value contains vertex colors.
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index,
                target,
                samples: vec![SongLuaOverlayUpdateSample {
                    time: next_time,
                    value: next.clone(),
                }],
            });
            return index;
        }
    };
    if overlay_state_matches_update_value(current, target, next)
        && tracks[track_index]
            .samples
            .last()
            .is_some_and(|sample| &sample.value == next)
    {
        return track_index;
    }
    let track = &mut tracks[track_index];
    push_overlay_gap(&mut track.samples, time, next_time, || {
        overlay_state_update_value(current, target)
    });
    track.samples.push(SongLuaOverlayUpdateSample {
        time: next_time,
        value: next.clone(),
    });
    track_index
}

pub(crate) fn overlay_state_update_value(
    state: &SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
) -> crate::SongLuaOverlayUpdateValue {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! value {
        ($variant:ident, $field:ident) => {
            Value::$variant(state.$field)
        };
    }
    macro_rules! option {
        ($variant:ident, $field:ident) => {
            state.$field.map(Value::$variant).unwrap_or(Value::None)
        };
    }
    match target {
        Target::Aux => value!(F32, aux),
        Target::X => value!(F32, x),
        Target::Y => value!(F32, y),
        Target::Z => value!(F32, z),
        Target::ZBias => value!(F32, z_bias),
        Target::DrawOrder => value!(I32, draw_order),
        Target::DrawByZPosition => value!(Bool, draw_by_z_position),
        Target::AftCreated => value!(Bool, aft_created),
        Target::AftPreserve => value!(Bool, aft_preserve),
        Target::HAlign => value!(F32, halign),
        Target::VAlign => value!(F32, valign),
        Target::TextAlign => value!(TextAlign, text_align),
        Target::Uppercase => value!(Bool, uppercase),
        Target::ShadowLen => value!(Vec2, shadow_len),
        Target::ShadowColor => value!(Vec4, shadow_color),
        Target::Glow => value!(Vec4, glow),
        Target::Fov => option!(F32, fov),
        Target::Vanishpoint => option!(Vec2, vanishpoint),
        Target::Diffuse => value!(Vec4, diffuse),
        Target::VertexColors => state
            .vertex_colors
            .map(|value| Value::VertexColors(std::sync::Arc::new(value)))
            .unwrap_or(Value::None),
        Target::Visible => value!(Bool, visible),
        Target::Hibernating => value!(Bool, hibernating),
        Target::CropLeft => value!(F32, cropleft),
        Target::CropRight => value!(F32, cropright),
        Target::CropTop => value!(F32, croptop),
        Target::CropBottom => value!(F32, cropbottom),
        Target::FadeLeft => value!(F32, fadeleft),
        Target::FadeRight => value!(F32, faderight),
        Target::FadeTop => value!(F32, fadetop),
        Target::FadeBottom => value!(F32, fadebottom),
        Target::MaskSource => value!(Bool, mask_source),
        Target::MaskDest => value!(Bool, mask_dest),
        Target::DepthTest => value!(Bool, depth_test),
        Target::DepthWrite => value!(Bool, depth_write),
        Target::DepthTestOnFail => value!(Bool, depth_test_on_fail),
        Target::CullMode => value!(CullMode, cull_mode),
        Target::Zoom => value!(F32, zoom),
        Target::ZoomX => value!(F32, zoom_x),
        Target::ZoomY => value!(F32, zoom_y),
        Target::ZoomZ => value!(F32, zoom_z),
        Target::BaseZoom => value!(F32, basezoom),
        Target::BaseZoomX => value!(F32, basezoom_x),
        Target::BaseZoomY => value!(F32, basezoom_y),
        Target::BaseZoomZ => value!(F32, basezoom_z),
        Target::RotationX => value!(F32, rot_x_deg),
        Target::RotationY => value!(F32, rot_y_deg),
        Target::RotationZ => value!(F32, rot_z_deg),
        Target::BaseRotation => value!(Vec3, base_rotation),
        Target::Quaternion => value!(Quaternion, quaternion),
        Target::BaseAlpha => value!(F32, base_alpha),
        Target::SkewX => value!(F32, skew_x),
        Target::SkewY => value!(F32, skew_y),
        Target::Blend => value!(Blend, blend),
        Target::Vibrate => value!(Bool, vibrate),
        Target::EffectMagnitude => value!(Vec3, effect_magnitude),
        Target::EffectClock => value!(EffectClock, effect_clock),
        Target::EffectTimer => value!(Bool, effect_timer),
        Target::EffectNoOffset => value!(Bool, effect_no_offset),
        Target::EffectMode => value!(EffectMode, effect_mode),
        Target::EffectColor1 => value!(Vec4, effect_color1),
        Target::EffectColor2 => value!(Vec4, effect_color2),
        Target::EffectPeriod => value!(F32, effect_period),
        Target::EffectOffset => value!(F32, effect_offset),
        Target::EffectTime => option!(Vec2, effect_time),
        Target::EffectTiming => option!(Vec5, effect_timing),
        Target::Rainbow => value!(Bool, rainbow),
        Target::RainbowScroll => value!(Bool, rainbow_scroll),
        Target::TextJitter => value!(Bool, text_jitter),
        Target::TextDistortion => value!(F32, text_distortion),
        Target::TextGlowMode => value!(TextGlowMode, text_glow_mode),
        Target::MultAttrsWithDiffuse => value!(Bool, mult_attrs_with_diffuse),
        Target::SpriteTexture => value!(Bool, sprite_texture),
        Target::SpriteAnimate => value!(Bool, sprite_animate),
        Target::SpriteLoop => value!(Bool, sprite_loop),
        Target::SpritePlaybackRate => value!(F32, sprite_playback_rate),
        Target::SpriteStateDelay => value!(F32, sprite_state_delay),
        Target::SpriteStateIndex => option!(U32, sprite_state_index),
        Target::VertSpacing => option!(I32, vert_spacing),
        Target::WrapWidthPixels => option!(I32, wrap_width_pixels),
        Target::MaxWidth => option!(F32, max_width),
        Target::MaxHeight => option!(F32, max_height),
        Target::MaxWPreZoom => value!(Bool, max_w_pre_zoom),
        Target::MaxHPreZoom => value!(Bool, max_h_pre_zoom),
        Target::MaxDimensionUsesZoom => value!(Bool, max_dimension_uses_zoom),
        Target::TextureFiltering => value!(Bool, texture_filtering),
        Target::TextureWrapping => value!(Bool, texture_wrapping),
        Target::TexcoordOffset => option!(Vec2, texcoord_offset),
        Target::CustomTextureRect => option!(Vec4, custom_texture_rect),
        Target::TexcoordVelocity => option!(Vec2, texcoord_velocity),
        Target::Size => option!(Vec2, size),
        Target::StretchRect => option!(Vec4, stretch_rect),
    }
}

pub(crate) fn overlay_state_matches_update_value(
    state: &SongLuaOverlayState,
    target: SongLuaOverlayUpdateTarget,
    value: &SongLuaOverlayUpdateValue,
) -> bool {
    if target == SongLuaOverlayUpdateTarget::VertexColors {
        // Compare the state's inline colors with the existing track's owned
        // colors. Only a changed track needs new owned samples.
        return match value {
            SongLuaOverlayUpdateValue::None => state.vertex_colors.is_none(),
            SongLuaOverlayUpdateValue::VertexColors(colors) => {
                state.vertex_colors.as_ref() == Some(colors.as_ref())
            }
            _ => false,
        };
    }
    overlay_state_update_value(state, target) == *value
}

pub(crate) fn set_overlay_state_update_value(
    state: &mut SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
    value: &crate::SongLuaOverlayUpdateValue,
) {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! set_value {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                if let Value::$variant(value) = value {
                    state.$field = *value;
                }
                return;
            }
        };
    }
    macro_rules! set_option {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                state.$field = match value {
                    Value::$variant(value) => Some(*value),
                    Value::None => None,
                    _ => state.$field,
                };
                return;
            }
        };
    }
    set_value!(Aux, F32, aux);
    set_value!(X, F32, x);
    set_value!(Y, F32, y);
    set_value!(Z, F32, z);
    set_value!(ZBias, F32, z_bias);
    set_value!(DrawOrder, I32, draw_order);
    set_value!(DrawByZPosition, Bool, draw_by_z_position);
    set_value!(AftCreated, Bool, aft_created);
    set_value!(AftPreserve, Bool, aft_preserve);
    set_value!(HAlign, F32, halign);
    set_value!(VAlign, F32, valign);
    set_value!(TextAlign, TextAlign, text_align);
    set_value!(Uppercase, Bool, uppercase);
    set_value!(ShadowLen, Vec2, shadow_len);
    set_value!(ShadowColor, Vec4, shadow_color);
    set_value!(Glow, Vec4, glow);
    set_option!(Fov, F32, fov);
    set_option!(Vanishpoint, Vec2, vanishpoint);
    set_value!(Diffuse, Vec4, diffuse);
    if target == Target::VertexColors {
        state.vertex_colors = match value {
            Value::VertexColors(value) => Some(**value),
            Value::None => None,
            _ => state.vertex_colors,
        };
        return;
    }
    set_value!(Visible, Bool, visible);
    set_value!(Hibernating, Bool, hibernating);
    set_value!(CropLeft, F32, cropleft);
    set_value!(CropRight, F32, cropright);
    set_value!(CropTop, F32, croptop);
    set_value!(CropBottom, F32, cropbottom);
    set_value!(FadeLeft, F32, fadeleft);
    set_value!(FadeRight, F32, faderight);
    set_value!(FadeTop, F32, fadetop);
    set_value!(FadeBottom, F32, fadebottom);
    set_value!(MaskSource, Bool, mask_source);
    set_value!(MaskDest, Bool, mask_dest);
    set_value!(DepthTest, Bool, depth_test);
    set_value!(DepthWrite, Bool, depth_write);
    set_value!(DepthTestOnFail, Bool, depth_test_on_fail);
    set_value!(CullMode, CullMode, cull_mode);
    set_value!(Zoom, F32, zoom);
    set_value!(ZoomX, F32, zoom_x);
    set_value!(ZoomY, F32, zoom_y);
    set_value!(ZoomZ, F32, zoom_z);
    set_value!(BaseZoom, F32, basezoom);
    set_value!(BaseZoomX, F32, basezoom_x);
    set_value!(BaseZoomY, F32, basezoom_y);
    set_value!(BaseZoomZ, F32, basezoom_z);
    set_value!(RotationX, F32, rot_x_deg);
    set_value!(RotationY, F32, rot_y_deg);
    set_value!(RotationZ, F32, rot_z_deg);
    set_value!(BaseRotation, Vec3, base_rotation);
    set_value!(Quaternion, Quaternion, quaternion);
    set_value!(BaseAlpha, F32, base_alpha);
    set_value!(SkewX, F32, skew_x);
    set_value!(SkewY, F32, skew_y);
    set_value!(Blend, Blend, blend);
    set_value!(Vibrate, Bool, vibrate);
    set_value!(EffectMagnitude, Vec3, effect_magnitude);
    set_value!(EffectClock, EffectClock, effect_clock);
    set_value!(EffectTimer, Bool, effect_timer);
    set_value!(EffectNoOffset, Bool, effect_no_offset);
    set_value!(EffectMode, EffectMode, effect_mode);
    set_value!(EffectColor1, Vec4, effect_color1);
    set_value!(EffectColor2, Vec4, effect_color2);
    set_value!(EffectPeriod, F32, effect_period);
    set_value!(EffectOffset, F32, effect_offset);
    set_option!(EffectTime, Vec2, effect_time);
    set_option!(EffectTiming, Vec5, effect_timing);
    set_value!(Rainbow, Bool, rainbow);
    set_value!(RainbowScroll, Bool, rainbow_scroll);
    set_value!(TextJitter, Bool, text_jitter);
    set_value!(TextDistortion, F32, text_distortion);
    set_value!(TextGlowMode, TextGlowMode, text_glow_mode);
    set_value!(MultAttrsWithDiffuse, Bool, mult_attrs_with_diffuse);
    set_value!(SpriteTexture, Bool, sprite_texture);
    set_value!(SpriteAnimate, Bool, sprite_animate);
    set_value!(SpriteLoop, Bool, sprite_loop);
    set_value!(SpritePlaybackRate, F32, sprite_playback_rate);
    set_value!(SpriteStateDelay, F32, sprite_state_delay);
    set_option!(SpriteStateIndex, U32, sprite_state_index);
    set_option!(VertSpacing, I32, vert_spacing);
    set_option!(WrapWidthPixels, I32, wrap_width_pixels);
    set_option!(MaxWidth, F32, max_width);
    set_option!(MaxHeight, F32, max_height);
    set_value!(MaxWPreZoom, Bool, max_w_pre_zoom);
    set_value!(MaxHPreZoom, Bool, max_h_pre_zoom);
    set_value!(MaxDimensionUsesZoom, Bool, max_dimension_uses_zoom);
    set_value!(TextureFiltering, Bool, texture_filtering);
    set_value!(TextureWrapping, Bool, texture_wrapping);
    set_option!(TexcoordOffset, Vec2, texcoord_offset);
    set_option!(CustomTextureRect, Vec4, custom_texture_rect);
    set_option!(TexcoordVelocity, Vec2, texcoord_velocity);
    set_option!(Size, Vec2, size);
    set_option!(StretchRect, Vec4, stretch_rect);
}

// Scratch belongs to one compilation and retains only its high-water storage.
// Track indices stay stable because update tracks are append-only during capture.
#[derive(Default)]
pub(crate) struct OverlaySampleScratch {
    pub(crate) completed: Vec<SongLuaScheduledOverlaySample>,
    pub(crate) blocked_actors: Vec<bool>,
    pub(crate) reset_indices: Vec<usize>,
    pub(crate) stopped_indices: Vec<usize>,
    pub(crate) captured_tracks: Vec<bool>,
    pub(crate) message_targets: Vec<(
        usize,
        SongLuaOverlayUpdateTarget,
        SongLuaOverlayUpdateValue,
        SongLuaOverlayUpdateValue,
    )>,
    pub(crate) retargeted_states:
        Vec<(usize, SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>,
}

pub(crate) fn retarget_actor_tween(
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    value: &SongLuaOverlayUpdateValue,
    current: &SongLuaOverlayState,
    seconds: f64,
) -> Option<SongLuaOverlayUpdateValue> {
    // Keep current-state sampling, back-tween mutation and inherited axes in
    // one state transition: a setter must never restart the active tween.
    if !matches!(
        target,
        SongLuaOverlayUpdateTarget::Zoom | SongLuaOverlayUpdateTarget::Diffuse
    ) && !PLAYER_TRANSFORM_TARGETS.contains(&target)
    {
        return None;
    }
    let pending = scheduled.iter().rposition(|sample| {
        sample.overlay_index == overlay_index
            && sample.end_seconds > scheduled_overlay_clock(sample, seconds)
    })?;
    let (start, end) = (
        scheduled[pending].start_seconds,
        scheduled[pending].end_seconds,
    );
    let rendered = scheduled
        .iter()
        .rev()
        .find(|sample| {
            sample.overlay_index == overlay_index
                && sample.target == target
                && sample
                    .dispatch_seconds
                    .is_none_or(|dispatch| dispatch <= seconds + 1.0e-7)
                && sample.start_seconds <= scheduled_overlay_clock(sample, seconds)
                && sample.end_seconds > scheduled_overlay_clock(sample, seconds)
        })
        .map_or_else(
            || overlay_state_update_value(current, target),
            |sample| {
                lerp_scheduled_value(
                    &sample.from,
                    // A setter changes the back tween. Earlier active tweens
                    // retain their own destinations on this frame.
                    if sample.frame_advance > 0.0
                        && sample.start_seconds == start
                        && sample.end_seconds == end
                    {
                        value
                    } else {
                        &sample.value
                    },
                    scheduled_overlay_factor(sample, scheduled_overlay_clock(sample, seconds)),
                )
            },
        );
    if let Some(sample) = scheduled.iter_mut().rev().find(|sample| {
        sample.overlay_index == overlay_index
            && sample.target == target
            && sample.start_seconds == start
            && sample.end_seconds == end
    }) {
        sample.value = value.clone();
        return Some(rendered);
    }
    let pending = &scheduled[pending];
    let from = scheduled
        .iter()
        .rev()
        .find(|sample| {
            sample.overlay_index == overlay_index
                && sample.target == target
                && sample.end_seconds <= pending.start_seconds
        })
        .map_or_else(
            || overlay_state_update_value(current, target),
            |sample| sample.value.clone(),
        );
    let replacement = SongLuaScheduledOverlaySample {
        progress: pending.progress.clone(),
        duration: pending.duration,
        dispatch_seconds: pending.dispatch_seconds,
        frame_advance: pending.frame_advance,
        overlay_index,
        target,
        start_seconds: pending.start_seconds,
        end_seconds: pending.end_seconds,
        start_beat: pending.start_beat,
        end_beat: pending.end_beat,
        easing: pending.easing.clone(),
        opt1: pending.opt1,
        from,
        value: value.clone(),
    };
    scheduled.push(replacement);
    if scheduled.last().is_some_and(|sample| {
        sample.frame_advance > 0.0
            && sample.start_seconds <= scheduled_overlay_clock(sample, seconds)
    }) {
        let sample = scheduled.last().expect("replacement appended");
        return Some(lerp_scheduled_value(
            &sample.from,
            &sample.value,
            scheduled_overlay_factor(sample, scheduled_overlay_clock(sample, seconds)),
        ));
    }
    Some(rendered)
}

pub(crate) fn append_scheduled_overlay_updates(
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    context: &SongLuaCompileContext,
    update_states: &[SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[ScheduledUpdate],
    message_seconds: f64,
) {
    if scheduled.is_empty() {
        return;
    }
    // Include immediate AFT/hibernate properties after the tween pose targets.
    // Borrow prior writes; only emitted samples need owned values.
    let mut scheduled_values: [Option<&SongLuaOverlayUpdateValue>;
        SongLuaOverlayUpdateTarget::Hibernating as usize + 1] =
        [None; SongLuaOverlayUpdateTarget::Hibernating as usize + 1];
    let mut previous_times: Option<((u32, u32), (f32, f32))> = None;
    let reuse_times = scheduled.len() > 1;
    for update in scheduled {
        let (start_seconds, end_seconds) = if let Some(samples) = &update.progress {
            let start = samples.first().expect("baked tween has frames")[0];
            let end = samples.last().expect("baked tween has frames");
            (f64::from(start), f64::from(end[0] + end[1]))
        } else {
            let start = message_seconds + f64::from(update.delay_seconds);
            (start, start + f64::from(update.duration_seconds))
        };
        let advance = if update.progress.is_some() {
            0.0
        } else {
            update.frame_advance
        };
        let start = (start_seconds - f64::from(advance)) as f32;
        let end = (end_seconds - f64::from(advance)) as f32;
        let time_bits = (start.to_bits(), end.to_bits());
        // Adjacent properties in one tween share their time bounds. Keep only
        // the preceding pair, keyed by bits to retain signed zero and NaNs.
        let (start_beat, end_beat) = if reuse_times {
            match previous_times {
                Some((bits, beats)) if bits == time_bits => beats,
                _ => {
                    let beats = (
                        song_beat_at_elapsed_seconds(start, context),
                        song_beat_at_elapsed_seconds(end, context),
                    );
                    previous_times = Some((time_bits, beats));
                    beats
                }
            }
        } else {
            (
                song_beat_at_elapsed_seconds(start, context),
                song_beat_at_elapsed_seconds(end, context),
            )
        };
        let from = scheduled_values[update.target as usize]
            .cloned()
            .or_else(|| update.initial_value.clone())
            .or_else(|| {
                update_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, update.target))
            })
            .unwrap_or(SongLuaOverlayUpdateValue::None);
        scheduled_samples.push(SongLuaScheduledOverlaySample {
            progress: update.progress.clone(),
            duration: update.duration_seconds,
            dispatch_seconds: update.dispatch_seconds,
            frame_advance: update.frame_advance,
            overlay_index,
            target: update.target,
            start_seconds,
            end_seconds,
            start_beat,
            end_beat,
            easing: update.easing.clone(),
            opt1: update.opt1,
            from,
            value: update.value.clone(),
        });
        scheduled_values[update.target as usize] = Some(&update.value);
    }
}

pub(crate) fn apply_captured_final_values(
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[ScheduledUpdate],
    final_values: &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
    restored_indices: &[usize],
) {
    // Small batches retain their original short scan. Wide batches reconcile
    // target membership once, without heap storage or a branch per target.
    if scheduled.len() < 16 || final_values.len() < 16 {
        for (target, value) in final_values {
            if scheduled.iter().any(|update| update.target == *target) {
                continue;
            }
            if let Some(state) = update_states.get_mut(overlay_index) {
                set_overlay_state_update_value(state, *target, value);
            }
            if restored_indices.binary_search(&overlay_index).is_err()
                && let Some(state) = to_states.get_mut(overlay_index)
            {
                set_overlay_state_update_value(state, *target, value);
            }
        }
        return;
    }
    const _: () = assert!((SongLuaOverlayUpdateTarget::Hibernating as usize) < 128);
    let scheduled_targets = scheduled.iter().fold(0_u128, |mask, update| {
        mask | (1_u128 << update.target as usize)
    });
    let restored = restored_indices.binary_search(&overlay_index).is_ok();
    for (target, value) in final_values {
        if scheduled_targets & (1_u128 << *target as usize) != 0 {
            continue;
        }
        if let Some(state) = update_states.get_mut(overlay_index) {
            set_overlay_state_update_value(state, *target, value);
        }
        if !restored && let Some(state) = to_states.get_mut(overlay_index) {
            set_overlay_state_update_value(state, *target, value);
        }
    }
}

pub(crate) fn capture_update_overlay_samples(
    source: &mut impl CaptureSource,
    args: CaptureFrameArgs<'_, impl std::hash::BuildHasher>,
) -> Result<(), String> {
    let CaptureFrameArgs {
        context,
        baseline,
        from_states,
        update_states,
        to_states,
        restored_indices,
        tracks,
        track_indices,
        time,
        next_time,
        next_seconds,
        scheduled_samples,
        scratch,
    } = args;

    let source_len = source.actor_count();
    let chronological = source.chronological();
    if chronological {
        scratch.completed.clear();
        scratch.blocked_actors.resize(source_len, false);
        scratch.blocked_actors.fill(false);
        scratch
            .completed
            .extend(scheduled_samples.extract_if(.., |sample| {
                // A later zero-time state cannot pass its pending actor tween
                // when cursor rounding puts that state's endpoint first.
                // Actor.h stores immediate fields outside TweenState. A queued
                // effect runs at dispatch, independently of later pose tweens.
                let tweened = crate::tween_pose::TARGETS.contains(&sample.target);
                if tweened && scratch.blocked_actors[sample.overlay_index] {
                    return false;
                }
                let complete = sample
                    .dispatch_seconds
                    .is_none_or(|dispatch| dispatch <= next_seconds + 1.0e-7)
                    && sample.end_seconds <= scheduled_overlay_clock(sample, next_seconds);
                if tweened {
                    scratch.blocked_actors[sample.overlay_index] = !complete;
                }
                complete
            }));
        // Actor queues are captured in enqueue order. Rounded cursor times
        // can put a trailing zero-time state just before its preceding tween;
        // sorting completed endpoints would then restore the wrong state.
        for sample in &scratch.completed {
            set_overlay_state_update_value(
                &mut update_states[sample.overlay_index],
                sample.target,
                &sample.value,
            );
            set_overlay_state_update_value(
                &mut to_states[sample.overlay_index],
                sample.target,
                &sample.value,
            );
            push_captured_overlay_value(
                tracks,
                track_indices,
                sample.overlay_index,
                sample.target,
                time,
                &from_states[sample.overlay_index],
                next_time,
                &sample.value,
            );
        }
    } else {
        merge_completed_scheduled_overlay_samples_into(
            tracks,
            track_indices,
            baseline,
            update_states,
            to_states,
            scheduled_samples,
            next_seconds,
            &mut scratch.completed,
        );
    }
    scratch.reset_indices.clear();
    scratch.stopped_indices.clear();
    scratch.captured_tracks.clear();
    scratch.captured_tracks.resize(tracks.len(), false);
    scratch.message_targets.clear();
    scratch.retargeted_states.clear();
    let OverlaySampleScratch {
        reset_indices,
        stopped_indices,
        captured_tracks,
        message_targets,
        retargeted_states,
        ..
    } = scratch;
    source.drain(
        &mut |overlay_index, values, scheduled, final_values, tween_reset, player| {
            let Some(baseline) = baseline.get(overlay_index) else {
                return Ok(());
            };
            debug_assert!(overlay_index < source_len);
            reset_indices.push(overlay_index);
            if tween_reset {
                stopped_indices.push(overlay_index);
                for sample in scheduled_samples
                    .iter()
                    .filter(|sample| sample.overlay_index == overlay_index)
                {
                    let clock = scheduled_overlay_clock(sample, next_seconds);
                    let current = if clock >= sample.start_seconds
                        && sample
                            .dispatch_seconds
                            .is_none_or(|dispatch| dispatch <= next_seconds + 1.0e-7)
                    {
                        lerp_scheduled_value(
                            &sample.from,
                            &sample.value,
                            scheduled_overlay_factor(sample, clock),
                        )
                    } else {
                        overlay_state_update_value(&from_states[overlay_index], sample.target)
                    };
                    set_overlay_state_update_value(
                        &mut update_states[overlay_index],
                        sample.target,
                        &current,
                    );
                    set_overlay_state_update_value(
                        &mut to_states[overlay_index],
                        sample.target,
                        &current,
                    );
                    push_captured_overlay_value(
                        tracks,
                        track_indices,
                        overlay_index,
                        sample.target,
                        time,
                        &from_states[overlay_index],
                        next_time,
                        &current,
                    );
                }
                scheduled_samples.retain(|sample| sample.overlay_index != overlay_index);
            }
            let player = player()?;
            if player || chronological {
                // Player Lua getters retain destinations. Their render state
                // must also retain immediate writes before a delayed return.
                for (target, value) in values {
                    if let Some(rendered) = retarget_actor_tween(
                        scheduled_samples,
                        overlay_index,
                        *target,
                        value,
                        from_states.get(overlay_index).unwrap_or(baseline),
                        next_seconds,
                    ) {
                        // Actor setters modify the back tween's destination.
                        // This frame has already advanced its current state.
                        retargeted_states.push((overlay_index, *target, rendered));
                    }
                    if let Some(state) = update_states.get_mut(overlay_index) {
                        set_overlay_state_update_value(state, *target, value);
                    }
                    if let Some(state) = to_states.get_mut(overlay_index) {
                        set_overlay_state_update_value(state, *target, value);
                    }
                }
            }
            apply_captured_final_values(
                update_states,
                to_states,
                overlay_index,
                scheduled,
                final_values,
                restored_indices,
            );
            for (target, next) in values {
                let current = from_states.get(overlay_index).unwrap_or(baseline);
                let rendered = retargeted_states
                    .iter()
                    .rev()
                    .find(|(index, property, _)| *index == overlay_index && *property == *target)
                    .map(|(_, _, value)| value);
                let next = rendered.unwrap_or(next);
                let track_index = push_captured_overlay_value(
                    tracks,
                    track_indices,
                    overlay_index,
                    *target,
                    time,
                    current,
                    next_time,
                    next,
                );
                // Tracks only append. Mark unchanged writes too: they still
                // take precedence over a restored message on this tick.
                if track_index >= captured_tracks.len() {
                    captured_tracks.resize(track_index + 1, false);
                }
                captured_tracks[track_index] = true;
            }
            append_scheduled_overlay_updates(
                scheduled_samples,
                context,
                update_states,
                overlay_index,
                scheduled,
                next_seconds,
            );
            Ok(())
        },
    )?;
    message_targets.extend(
        track_indices
            .iter()
            .filter(|(_, index)| !captured_tracks[**index])
            .filter_map(|(&(overlay_index, target), &track_index)| {
                let current_state = from_states.get(overlay_index)?;
                let message_state = to_states.get(overlay_index)?;
                let tracked = tracks
                    .get(track_index)
                    .and_then(|track| track.samples.last())
                    .map(|sample| &sample.value)?;
                if overlay_state_matches_update_value(message_state, target, tracked) {
                    return None;
                }
                Some((
                    overlay_index,
                    target,
                    overlay_state_update_value(current_state, target),
                    overlay_state_update_value(message_state, target),
                ))
            }),
    );
    // Runtime update tracks are applied after message commands. Keep an existing
    // track synchronized when a message changes its target, otherwise a stale
    // sampled value can overwrite a later persistent action (notably Player
    // ActorProxy visibility in Step Your Game Up).
    for (overlay_index, target, current, message) in message_targets.drain(..) {
        push_update_overlay_value(
            tracks,
            track_indices,
            overlay_index,
            target,
            time,
            current,
            next_time,
            message,
        );
    }
    for &overlay_index in reset_indices.iter() {
        source.reset(overlay_index)?;
    }
    Ok(())
}

#[cfg_attr(test, derive(Clone))]
pub(crate) struct SongLuaScheduledOverlaySample {
    pub(crate) progress: Option<std::sync::Arc<[[f32; 2]]>>,
    pub(crate) duration: f32,
    pub(crate) dispatch_seconds: Option<f64>,
    pub(crate) frame_advance: f32,
    pub(crate) overlay_index: usize,
    pub(crate) target: SongLuaOverlayUpdateTarget,
    pub(crate) start_seconds: f64,
    pub(crate) end_seconds: f64,
    pub(crate) start_beat: f32,
    pub(crate) end_beat: f32,
    pub(crate) easing: Option<String>,
    pub(crate) opt1: Option<f32>,
    pub(crate) from: SongLuaOverlayUpdateValue,
    pub(crate) value: SongLuaOverlayUpdateValue,
}

pub(crate) fn lerp_scheduled_value(
    from: &SongLuaOverlayUpdateValue,
    to: &SongLuaOverlayUpdateValue,
    factor: f32,
) -> SongLuaOverlayUpdateValue {
    use SongLuaOverlayUpdateValue as Value;
    let lerp = |from: f32, to: f32| crate::actor_lerp(from, to, factor);
    match (from, to) {
        (Value::F32(from), Value::F32(to)) => Value::F32(lerp(*from, *to)),
        (Value::Vec2(from), Value::Vec2(to)) => {
            Value::Vec2([lerp(from[0], to[0]), lerp(from[1], to[1])])
        }
        (Value::Vec3(from), Value::Vec3(to)) => Value::Vec3([
            lerp(from[0], to[0]),
            lerp(from[1], to[1]),
            lerp(from[2], to[2]),
        ]),
        (Value::Quaternion(from), Value::Quaternion(to)) => {
            Value::Quaternion(crate::actor_quaternion::slerp(*from, *to, factor))
        }
        (Value::Vec4(from), Value::Vec4(to)) => Value::Vec4([
            lerp(from[0], to[0]),
            lerp(from[1], to[1]),
            lerp(from[2], to[2]),
            lerp(from[3], to[3]),
        ]),
        _ => {
            if factor >= 1.0 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}

#[inline(always)]
pub(crate) fn scheduled_overlay_factor(
    sample: &SongLuaScheduledOverlaySample,
    seconds: f64,
) -> f32 {
    let linear_factor = if let Some(frames) = &sample.progress {
        crate::tween_frame_factor(frames, sample.duration, seconds as f32).unwrap_or(0.0)
    } else if sample.end_seconds <= sample.start_seconds + f64::EPSILON {
        1.0
    } else {
        ((seconds - sample.start_seconds) / (sample.end_seconds - sample.start_seconds))
            .clamp(0.0, 1.0) as f32
    };
    crate::overlay_command_ease_factor(sample.easing.as_deref(), linear_factor, sample.opt1)
}

pub(crate) fn scheduled_overlay_clock(sample: &SongLuaScheduledOverlaySample, seconds: f64) -> f64 {
    if sample.progress.is_some() {
        // Native frame samples and Actor::Update use float timestamps. Compare
        // on that clock too, so a rounded-up zero-time state is not deferred.
        f64::from(seconds as f32)
    } else if sample.frame_advance == 0.0 {
        seconds
    } else {
        seconds + f64::from(sample.frame_advance)
    }
}

pub(crate) fn scheduled_overlay_value(
    sample: &SongLuaScheduledOverlaySample,
    factor: f32,
) -> SongLuaOverlayUpdateValue {
    if sample.duration <= 0.0 {
        // Zero-time destinations are copied, not interpolated. Even at 1.0,
        // subtraction and addition can change a timer period's float bits.
        sample.value.clone()
    } else {
        lerp_scheduled_value(&sample.from, &sample.value, factor)
    }
}

pub(crate) fn scheduled_clocks_match(
    left: &SongLuaScheduledOverlaySample,
    right: &SongLuaScheduledOverlaySample,
) -> bool {
    left.start_seconds.to_bits() == right.start_seconds.to_bits()
        && left.end_seconds.to_bits() == right.end_seconds.to_bits()
        && left.duration.to_bits() == right.duration.to_bits()
        && left.frame_advance == right.frame_advance
        && match (&left.progress, &right.progress) {
            (Some(left), Some(right)) => std::sync::Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        }
}

pub(crate) fn apply_scheduled_overlay_states_uncached(
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
    mut write: impl FnMut(
        usize,
        SongLuaOverlayUpdateTarget,
        &SongLuaOverlayUpdateValue,
    ) -> Result<(), String>,
) -> Result<(), String> {
    for sample in scheduled {
        let clock = scheduled_overlay_clock(sample, seconds);
        if sample
            .dispatch_seconds
            .is_some_and(|dispatch| seconds + 1.0e-7 < dispatch)
            || clock + f64::EPSILON < sample.start_seconds
        {
            continue;
        }
        let factor = scheduled_overlay_factor(sample, clock);
        let Some(state) = states.get_mut(sample.overlay_index) else {
            continue;
        };
        let value = scheduled_overlay_value(sample, factor);
        set_overlay_state_update_value(state, sample.target, &value);
        write(sample.overlay_index, sample.target, &value)?;
    }
    Ok(())
}

pub(crate) fn apply_scheduled_overlay_states(
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
    mut write: impl FnMut(
        usize,
        SongLuaOverlayUpdateTarget,
        &SongLuaOverlayUpdateValue,
    ) -> Result<(), String>,
) -> Result<(), String> {
    // Probe the first pair once. Homogeneous scalar/unique-timing batches
    // keep the original loop; grouped expensive tweens amortize the cache.
    let reuse_factors = if let [first, second, ..] = scheduled {
        matches!(first.easing.as_deref(), Some("spring" | "outElastic"))
            && scheduled_clocks_match(first, second)
            && first.easing == second.easing
            && (first.easing.as_deref() == Some("spring")
                || first.opt1.map(f32::to_bits) == second.opt1.map(f32::to_bits))
    } else {
        false
    };
    if !reuse_factors {
        return apply_scheduled_overlay_states_uncached(states, scheduled, seconds, write);
    }
    let mut last_factor: Option<(&SongLuaScheduledOverlaySample, u8, f32)> = None;
    for sample in scheduled {
        let clock = scheduled_overlay_clock(sample, seconds);
        if sample
            .dispatch_seconds
            .is_some_and(|dispatch| seconds + 1.0e-7 < dispatch)
            || clock + f64::EPSILON < sample.start_seconds
        {
            continue;
        }
        // Spring/elastic curves use transcendental functions. Adjacent
        // properties of one tween share their factor; keep only the last one.
        let curve = match sample.easing.as_deref() {
            Some("spring") => 1,
            Some("outElastic") => 2,
            _ => 0,
        };
        let factor = if curve != 0 {
            if let Some((previous, previous_curve, factor)) = last_factor
                && scheduled_clocks_match(previous, sample)
                && previous_curve == curve
                && (curve == 1 || previous.opt1.map(f32::to_bits) == sample.opt1.map(f32::to_bits))
            {
                factor
            } else {
                let factor = scheduled_overlay_factor(sample, clock);
                last_factor = Some((sample, curve, factor));
                factor
            }
        } else {
            scheduled_overlay_factor(sample, clock)
        };
        let Some(state) = states.get_mut(sample.overlay_index) else {
            continue;
        };
        let value = scheduled_overlay_value(sample, factor);
        set_overlay_state_update_value(state, sample.target, &value);
        write(sample.overlay_index, sample.target, &value)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn merge_completed_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    beat: f32,
) {
    merge_completed_scheduled_overlay_samples_into(
        tracks,
        track_indices,
        baseline,
        update_states,
        to_states,
        scheduled,
        f64::from(beat),
        &mut Vec::new(),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn merge_completed_scheduled_overlay_samples_into(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    seconds: f64,
    completed: &mut Vec<SongLuaScheduledOverlaySample>,
) {
    completed.clear();
    if scheduled.is_empty() {
        return;
    }
    // Keep pending tweens (and their capacity) in place across sample ticks.
    // The common case where none have completed does not allocate or move them.
    // Beats do not advance during a pause. Actor delays and tweens still do.
    completed.extend(scheduled.extract_if(.., |sample| {
        // Progress samples use a float song clock. Complete on that same
        // clock; a double comparison can leave a rounded-up empty tail alive
        // for one more frame and overwrite the next recurring tween.
        sample.end_seconds <= scheduled_overlay_clock(sample, seconds)
            && sample
                .dispatch_seconds
                .is_none_or(|dispatch| dispatch <= seconds + 1.0e-7)
    }));
    if !completed.is_empty() {
        completed.sort_by(|left, right| left.end_seconds.total_cmp(&right.end_seconds));
        for sample in completed.iter() {
            if let Some(state) = update_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
            if let Some(state) = to_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
        }
        merge_scheduled_overlay_samples_from_buffer(tracks, track_indices, baseline, completed);
    }
}

pub(crate) fn sort_overlay_update_samples(samples: &mut Vec<SongLuaOverlayUpdateSample>) {
    // Sample tracks are normally already ordered. Avoid the stable sort's
    // temporary allocation in that case; preserve stable ties when sorting.
    if !samples.is_sorted_by(|left, right| left.time.total_cmp(&right.time).is_le()) {
        samples.sort_by(|left, right| left.time.total_cmp(&right.time));
    }
    samples.dedup_by(|next, previous| {
        if (previous.time - next.time).abs() <= f32::EPSILON {
            // Keep the last value AND timestamp so epsilon-connected runs
            // collapse exactly as they do when replacing the last output item.
            std::mem::swap(previous, next);
            true
        } else {
            false
        }
    });
}

pub(crate) fn append_ordered_overlay_sample(
    samples: &mut Vec<SongLuaOverlayUpdateSample>,
    sample: SongLuaOverlayUpdateSample,
) {
    if let Some(last) = samples.last_mut()
        && (last.time - sample.time).abs() <= f32::EPSILON
    {
        *last = sample;
    } else {
        samples.push(sample);
    }
}

// The caller keeps samples in total_cmp order. Negative NaNs form a leading
// prefix; include them in the search partition, then reject them as values.
// Numeric comparison includes both signs of zero, matching the reverse scan.
pub(crate) fn overlay_sample_at_or_before(
    samples: &[SongLuaOverlayUpdateSample],
    time: f32,
) -> Option<&SongLuaOverlayUpdateSample> {
    let last = samples.last()?;
    if last.time <= time {
        return Some(last);
    }
    let earlier = &samples[..samples.len() - 1];
    // Short reverse scans time the binary-search setup. The last sample was
    // already rejected, so neither path needs to compare it again.
    if earlier.len() < 32 {
        return earlier.iter().rev().find(|sample| sample.time <= time);
    }
    let end = earlier.partition_point(|sample| {
        sample.time <= time || (sample.time.is_nan() && sample.time.is_sign_negative())
    });
    end.checked_sub(1)
        .and_then(|index| earlier.get(index))
        .filter(|sample| sample.time <= time)
}

pub(crate) fn merge_scheduled_overlay_samples_from_buffer(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
) {
    for track in tracks.iter_mut() {
        sort_overlay_update_samples(&mut track.samples);
    }
    scheduled.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
    for sample in scheduled.drain(..) {
        let track_index = *track_indices
            .entry((sample.overlay_index, sample.target))
            .or_insert_with(|| {
                let index = tracks.len();
                tracks.push(SongLuaOverlayUpdateTrack {
                    overlay_index: sample.overlay_index,
                    target: sample.target,
                    samples: vec![SongLuaOverlayUpdateSample {
                        time: 0.0,
                        value: overlay_state_update_value(
                            &baseline[sample.overlay_index],
                            sample.target,
                        ),
                    }],
                });
                index
            });
        let track = &mut tracks[track_index];
        let has_start = sample.end_beat > sample.start_beat + f32::EPSILON;
        // Step writes emit only their end value, so their start snapshot is
        // unused. Clone or construct one only when an anchor will own it.
        let current = has_start.then(|| {
            overlay_sample_at_or_before(&track.samples, sample.start_beat + f32::EPSILON)
                .map(|current| current.value.clone())
                .unwrap_or_else(|| {
                    overlay_state_update_value(&baseline[sample.overlay_index], sample.target)
                })
        });
        let first_beat = if has_start {
            sample.start_beat
        } else {
            sample.end_beat
        };
        let ordered = track
            .samples
            .last()
            .is_none_or(|last| last.time.total_cmp(&first_beat).is_le());
        if ordered {
            // The existing prefix is sorted and compacted. An ordered append
            // can only merge with its last sample, preserving last-write wins.
            if let Some(current) = current {
                append_ordered_overlay_sample(
                    &mut track.samples,
                    SongLuaOverlayUpdateSample {
                        time: sample.start_beat,
                        value: current,
                    },
                );
            }
            append_ordered_overlay_sample(
                &mut track.samples,
                SongLuaOverlayUpdateSample {
                    time: sample.end_beat,
                    value: sample.value,
                },
            );
            continue;
        }
        if let Some(current) = current {
            track.samples.push(SongLuaOverlayUpdateSample {
                time: sample.start_beat,
                value: current,
            });
        }
        track.samples.push(SongLuaOverlayUpdateSample {
            time: sample.end_beat,
            value: sample.value,
        });
        sort_overlay_update_samples(&mut track.samples);
    }
    // Every track was canonicalized above. Ordered appends keep that order
    // and compact ties; out-of-order appends canonicalize their track locally.
}

/// Field routing of the existing preparation getter handoff.
pub(crate) fn overlay_getter_scalar_key(target: SongLuaOverlayUpdateTarget) -> &'static str {
    use SongLuaOverlayUpdateTarget as Target;
    match target {
        Target::Aux => "aux",
        Target::X => "x",
        Target::Y => "y",
        Target::Z => "z",
        Target::Zoom => "zoom",
        Target::ZoomX => "zoom_x",
        Target::ZoomY => "zoom_y",
        Target::ZoomZ => "zoom_z",
        Target::BaseZoom => "basezoom",
        Target::BaseZoomX => "basezoom_x",
        Target::BaseZoomY => "basezoom_y",
        Target::BaseZoomZ => "basezoom_z",
        Target::RotationX => "rot_x_deg",
        Target::RotationY => "rot_y_deg",
        Target::RotationZ => "rot_z_deg",
        Target::BaseRotation => "base_rotation",
        Target::Quaternion => "quaternion",
        Target::BaseAlpha => "base_alpha",
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
        Target::EffectPeriod => "effect_period",
        Target::EffectOffset => "effect_offset",
        _ => "",
    }
}
