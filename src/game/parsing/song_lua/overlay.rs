use crate::engine::present::anim::EffectMode;
use std::path::PathBuf;

use super::{SongLuaSpanMode, SongLuaTimeUnit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaProxyTarget {
    Player { player_index: usize },
    NoteField { player_index: usize },
    Judgment { player_index: usize },
    Combo { player_index: usize },
    Underlay,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaOverlayBlendMode {
    Alpha,
    Add,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SongLuaOverlayKind {
    ActorFrame,
    ActorFrameTexture,
    ActorProxy {
        target: SongLuaProxyTarget,
    },
    AftSprite {
        capture_name: String,
    },
    Sprite {
        texture_path: PathBuf,
    },
    BitmapText {
        font_name: &'static str,
        font_path: PathBuf,
        text: String,
        stroke_color: Option<[f32; 4]>,
    },
    Quad,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayState {
    pub x: f32,
    pub y: f32,
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: [f32; 4],
    pub visible: bool,
    pub cropleft: f32,
    pub cropright: f32,
    pub croptop: f32,
    pub cropbottom: f32,
    pub zoom: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub basezoom: f32,
    pub basezoom_x: f32,
    pub basezoom_y: f32,
    pub rot_x_deg: f32,
    pub rot_y_deg: f32,
    pub rot_z_deg: f32,
    pub blend: SongLuaOverlayBlendMode,
    pub vibrate: bool,
    pub effect_magnitude: [f32; 3],
    pub effect_mode: EffectMode,
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
}

impl Default for SongLuaOverlayState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            fov: None,
            vanishpoint: None,
            diffuse: [1.0, 1.0, 1.0, 1.0],
            visible: true,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            zoom: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            basezoom: 1.0,
            basezoom_x: 1.0,
            basezoom_y: 1.0,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            blend: SongLuaOverlayBlendMode::Alpha,
            vibrate: false,
            effect_magnitude: [0.0, 0.0, 0.0],
            effect_mode: EffectMode::None,
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            custom_texture_rect: None,
            texcoord_velocity: None,
            size: None,
            stretch_rect: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaOverlayStateDelta {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: Option<[f32; 4]>,
    pub visible: Option<bool>,
    pub cropleft: Option<f32>,
    pub cropright: Option<f32>,
    pub croptop: Option<f32>,
    pub cropbottom: Option<f32>,
    pub zoom: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub basezoom: Option<f32>,
    pub basezoom_x: Option<f32>,
    pub basezoom_y: Option<f32>,
    pub rot_x_deg: Option<f32>,
    pub rot_y_deg: Option<f32>,
    pub rot_z_deg: Option<f32>,
    pub blend: Option<SongLuaOverlayBlendMode>,
    pub vibrate: Option<bool>,
    pub effect_magnitude: Option<[f32; 3]>,
    pub effect_mode: Option<EffectMode>,
    pub effect_color1: Option<[f32; 4]>,
    pub effect_color2: Option<[f32; 4]>,
    pub effect_period: Option<f32>,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayCommandBlock {
    pub start: f32,
    pub duration: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
    pub delta: SongLuaOverlayStateDelta,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayMessageCommand {
    pub message: String,
    pub blocks: Vec<SongLuaOverlayCommandBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayActor {
    pub kind: SongLuaOverlayKind,
    pub name: Option<String>,
    pub parent_index: Option<usize>,
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayEase {
    pub overlay_index: usize,
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub from: SongLuaOverlayStateDelta,
    pub to: SongLuaOverlayStateDelta,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

pub(super) fn overlay_state_after_blocks(
    mut state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    for block in blocks {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_overlay_delta(&mut state, &block.delta);
            continue;
        }
        let target = overlay_state_with_delta(state, &block.delta);
        return overlay_state_lerp(
            state,
            target,
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            &block.delta,
        );
    }
    state
}

fn overlay_state_with_delta(
    mut state: SongLuaOverlayState,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    apply_overlay_delta(&mut state, delta);
    state
}

fn apply_overlay_delta(state: &mut SongLuaOverlayState, delta: &SongLuaOverlayStateDelta) {
    if let Some(value) = delta.x {
        state.x = value;
    }
    if let Some(value) = delta.y {
        state.y = value;
    }
    if let Some(value) = delta.fov {
        state.fov = Some(value);
    }
    if let Some(value) = delta.vanishpoint {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = delta.diffuse {
        state.diffuse = value;
    }
    if let Some(value) = delta.visible {
        state.visible = value;
    }
    if let Some(value) = delta.cropleft {
        state.cropleft = value;
    }
    if let Some(value) = delta.cropright {
        state.cropright = value;
    }
    if let Some(value) = delta.croptop {
        state.croptop = value;
    }
    if let Some(value) = delta.cropbottom {
        state.cropbottom = value;
    }
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.zoom_x {
        state.zoom_x = value;
    }
    if let Some(value) = delta.zoom_y {
        state.zoom_y = value;
    }
    if let Some(value) = delta.basezoom {
        state.basezoom = value;
    }
    if let Some(value) = delta.basezoom_x {
        state.basezoom_x = value;
    }
    if let Some(value) = delta.basezoom_y {
        state.basezoom_y = value;
    }
    if let Some(value) = delta.rot_x_deg {
        state.rot_x_deg = value;
    }
    if let Some(value) = delta.rot_y_deg {
        state.rot_y_deg = value;
    }
    if let Some(value) = delta.rot_z_deg {
        state.rot_z_deg = value;
    }
    if let Some(value) = delta.blend {
        state.blend = value;
    }
    if let Some(value) = delta.vibrate {
        state.vibrate = value;
    }
    if let Some(value) = delta.effect_magnitude {
        state.effect_magnitude = value;
    }
    if let Some(value) = delta.effect_mode {
        state.effect_mode = value;
    }
    if let Some(value) = delta.effect_color1 {
        state.effect_color1 = value;
    }
    if let Some(value) = delta.effect_color2 {
        state.effect_color2 = value;
    }
    if let Some(value) = delta.effect_period {
        state.effect_period = value;
    }
    if let Some(value) = delta.custom_texture_rect {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = delta.texcoord_velocity {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = delta.size {
        state.size = Some(value);
    }
    if let Some(value) = delta.stretch_rect {
        state.stretch_rect = Some(value);
    }
}

fn overlay_state_lerp(
    mut from: SongLuaOverlayState,
    to: SongLuaOverlayState,
    t: f32,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    if delta.x.is_some() {
        from.x = (to.x - from.x).mul_add(t, from.x);
    }
    if delta.y.is_some() {
        from.y = (to.y - from.y).mul_add(t, from.y);
    }
    if delta.fov.is_some()
        && let (Some(from_fov), Some(to_fov)) = (from.fov, to.fov)
    {
        from.fov = Some((to_fov - from_fov).mul_add(t, from_fov));
    }
    if delta.vanishpoint.is_some()
        && let (Some(from_vanish), Some(to_vanish)) = (from.vanishpoint, to.vanishpoint)
    {
        from.vanishpoint = Some([
            (to_vanish[0] - from_vanish[0]).mul_add(t, from_vanish[0]),
            (to_vanish[1] - from_vanish[1]).mul_add(t, from_vanish[1]),
        ]);
    }
    if delta.diffuse.is_some() {
        for i in 0..4 {
            from.diffuse[i] = (to.diffuse[i] - from.diffuse[i]).mul_add(t, from.diffuse[i]);
        }
    }
    if delta.cropleft.is_some() {
        from.cropleft = (to.cropleft - from.cropleft).mul_add(t, from.cropleft);
    }
    if delta.cropright.is_some() {
        from.cropright = (to.cropright - from.cropright).mul_add(t, from.cropright);
    }
    if delta.croptop.is_some() {
        from.croptop = (to.croptop - from.croptop).mul_add(t, from.croptop);
    }
    if delta.cropbottom.is_some() {
        from.cropbottom = (to.cropbottom - from.cropbottom).mul_add(t, from.cropbottom);
    }
    if delta.zoom.is_some() {
        from.zoom = (to.zoom - from.zoom).mul_add(t, from.zoom);
    }
    if delta.zoom_x.is_some() {
        from.zoom_x = (to.zoom_x - from.zoom_x).mul_add(t, from.zoom_x);
    }
    if delta.zoom_y.is_some() {
        from.zoom_y = (to.zoom_y - from.zoom_y).mul_add(t, from.zoom_y);
    }
    if delta.basezoom.is_some() {
        from.basezoom = (to.basezoom - from.basezoom).mul_add(t, from.basezoom);
    }
    if delta.basezoom_x.is_some() {
        from.basezoom_x = (to.basezoom_x - from.basezoom_x).mul_add(t, from.basezoom_x);
    }
    if delta.basezoom_y.is_some() {
        from.basezoom_y = (to.basezoom_y - from.basezoom_y).mul_add(t, from.basezoom_y);
    }
    if delta.rot_x_deg.is_some() {
        from.rot_x_deg = (to.rot_x_deg - from.rot_x_deg).mul_add(t, from.rot_x_deg);
    }
    if delta.rot_y_deg.is_some() {
        from.rot_y_deg = (to.rot_y_deg - from.rot_y_deg).mul_add(t, from.rot_y_deg);
    }
    if delta.rot_z_deg.is_some() {
        from.rot_z_deg = (to.rot_z_deg - from.rot_z_deg).mul_add(t, from.rot_z_deg);
    }
    if delta.effect_magnitude.is_some() {
        for i in 0..3 {
            from.effect_magnitude[i] = (to.effect_magnitude[i] - from.effect_magnitude[i])
                .mul_add(t, from.effect_magnitude[i]);
        }
    }
    if delta.effect_color1.is_some() {
        for i in 0..4 {
            from.effect_color1[i] =
                (to.effect_color1[i] - from.effect_color1[i]).mul_add(t, from.effect_color1[i]);
        }
    }
    if delta.effect_color2.is_some() {
        for i in 0..4 {
            from.effect_color2[i] =
                (to.effect_color2[i] - from.effect_color2[i]).mul_add(t, from.effect_color2[i]);
        }
    }
    if delta.effect_period.is_some() {
        from.effect_period = (to.effect_period - from.effect_period).mul_add(t, from.effect_period);
    }
    if delta.custom_texture_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.custom_texture_rect, to.custom_texture_rect)
    {
        from.custom_texture_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.texcoord_velocity.is_some()
        && let (Some(from_vel), Some(to_vel)) = (from.texcoord_velocity, to.texcoord_velocity)
    {
        from.texcoord_velocity = Some([
            (to_vel[0] - from_vel[0]).mul_add(t, from_vel[0]),
            (to_vel[1] - from_vel[1]).mul_add(t, from_vel[1]),
        ]);
    }
    if delta.size.is_some()
        && let (Some(from_size), Some(to_size)) = (from.size, to.size)
    {
        from.size = Some([
            (to_size[0] - from_size[0]).mul_add(t, from_size[0]),
            (to_size[1] - from_size[1]).mul_add(t, from_size[1]),
        ]);
    }
    if delta.stretch_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.stretch_rect, to.stretch_rect)
    {
        from.stretch_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.visible.is_some() && t >= 1.0 - f32::EPSILON {
        from.visible = to.visible;
    }
    if delta.blend.is_some() && t >= 1.0 - f32::EPSILON {
        from.blend = to.blend;
    }
    if delta.vibrate.is_some() && t >= 1.0 - f32::EPSILON {
        from.vibrate = to.vibrate;
    }
    if delta.effect_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_mode = to.effect_mode;
    }
    from
}

pub(super) fn parse_overlay_blend_mode(raw: &str) -> Option<SongLuaOverlayBlendMode> {
    if raw.eq_ignore_ascii_case("add") {
        Some(SongLuaOverlayBlendMode::Add)
    } else if raw.eq_ignore_ascii_case("alpha")
        || raw.eq_ignore_ascii_case("normal")
        || raw.eq_ignore_ascii_case("blendmode_normal")
    {
        Some(SongLuaOverlayBlendMode::Alpha)
    } else {
        None
    }
}

pub(super) fn parse_overlay_effect_mode(raw: &str) -> Option<EffectMode> {
    match raw {
        "none" => Some(EffectMode::None),
        "diffuseshift" => Some(EffectMode::DiffuseShift),
        "spin" => Some(EffectMode::Spin),
        _ => None,
    }
}

fn overlay_delta_is_empty(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.x.is_none()
        && delta.y.is_none()
        && delta.fov.is_none()
        && delta.vanishpoint.is_none()
        && delta.diffuse.is_none()
        && delta.visible.is_none()
        && delta.cropleft.is_none()
        && delta.cropright.is_none()
        && delta.croptop.is_none()
        && delta.cropbottom.is_none()
        && delta.zoom.is_none()
        && delta.zoom_x.is_none()
        && delta.zoom_y.is_none()
        && delta.basezoom.is_none()
        && delta.basezoom_x.is_none()
        && delta.basezoom_y.is_none()
        && delta.rot_x_deg.is_none()
        && delta.rot_y_deg.is_none()
        && delta.rot_z_deg.is_none()
        && delta.blend.is_none()
        && delta.vibrate.is_none()
        && delta.effect_magnitude.is_none()
        && delta.effect_mode.is_none()
        && delta.effect_color1.is_none()
        && delta.effect_color2.is_none()
        && delta.effect_period.is_none()
        && delta.custom_texture_rect.is_none()
        && delta.texcoord_velocity.is_none()
        && delta.size.is_none()
        && delta.stretch_rect.is_none()
}

fn merge_overlay_delta(into: &mut SongLuaOverlayStateDelta, from: &SongLuaOverlayStateDelta) {
    if from.x.is_some() {
        into.x = from.x;
    }
    if from.y.is_some() {
        into.y = from.y;
    }
    if from.fov.is_some() {
        into.fov = from.fov;
    }
    if from.vanishpoint.is_some() {
        into.vanishpoint = from.vanishpoint;
    }
    if from.diffuse.is_some() {
        into.diffuse = from.diffuse;
    }
    if from.visible.is_some() {
        into.visible = from.visible;
    }
    if from.cropleft.is_some() {
        into.cropleft = from.cropleft;
    }
    if from.cropright.is_some() {
        into.cropright = from.cropright;
    }
    if from.croptop.is_some() {
        into.croptop = from.croptop;
    }
    if from.cropbottom.is_some() {
        into.cropbottom = from.cropbottom;
    }
    if from.zoom.is_some() {
        into.zoom = from.zoom;
    }
    if from.zoom_x.is_some() {
        into.zoom_x = from.zoom_x;
    }
    if from.zoom_y.is_some() {
        into.zoom_y = from.zoom_y;
    }
    if from.basezoom.is_some() {
        into.basezoom = from.basezoom;
    }
    if from.basezoom_x.is_some() {
        into.basezoom_x = from.basezoom_x;
    }
    if from.basezoom_y.is_some() {
        into.basezoom_y = from.basezoom_y;
    }
    if from.rot_x_deg.is_some() {
        into.rot_x_deg = from.rot_x_deg;
    }
    if from.rot_y_deg.is_some() {
        into.rot_y_deg = from.rot_y_deg;
    }
    if from.rot_z_deg.is_some() {
        into.rot_z_deg = from.rot_z_deg;
    }
    if from.blend.is_some() {
        into.blend = from.blend;
    }
    if from.vibrate.is_some() {
        into.vibrate = from.vibrate;
    }
    if from.effect_magnitude.is_some() {
        into.effect_magnitude = from.effect_magnitude;
    }
    if from.effect_mode.is_some() {
        into.effect_mode = from.effect_mode;
    }
    if from.effect_color1.is_some() {
        into.effect_color1 = from.effect_color1;
    }
    if from.effect_color2.is_some() {
        into.effect_color2 = from.effect_color2;
    }
    if from.effect_period.is_some() {
        into.effect_period = from.effect_period;
    }
    if from.custom_texture_rect.is_some() {
        into.custom_texture_rect = from.custom_texture_rect;
    }
    if from.texcoord_velocity.is_some() {
        into.texcoord_velocity = from.texcoord_velocity;
    }
    if from.size.is_some() {
        into.size = from.size;
    }
    if from.stretch_rect.is_some() {
        into.stretch_rect = from.stretch_rect;
    }
}

pub(super) fn overlay_delta_from_blocks(
    blocks: &[SongLuaOverlayCommandBlock],
) -> Option<SongLuaOverlayStateDelta> {
    let mut delta = SongLuaOverlayStateDelta::default();
    for block in blocks {
        merge_overlay_delta(&mut delta, &block.delta);
    }
    (!overlay_delta_is_empty(&delta)).then_some(delta)
}

pub(super) fn overlay_delta_intersection(
    from: &SongLuaOverlayStateDelta,
    to: &SongLuaOverlayStateDelta,
) -> Option<(SongLuaOverlayStateDelta, SongLuaOverlayStateDelta)> {
    let mut out_from = SongLuaOverlayStateDelta::default();
    let mut out_to = SongLuaOverlayStateDelta::default();
    macro_rules! copy_pair {
        ($field:ident) => {
            if let (Some(from_value), Some(to_value)) = (from.$field, to.$field) {
                out_from.$field = Some(from_value);
                out_to.$field = Some(to_value);
            }
        };
    }
    copy_pair!(x);
    copy_pair!(y);
    copy_pair!(fov);
    copy_pair!(vanishpoint);
    copy_pair!(diffuse);
    copy_pair!(visible);
    copy_pair!(cropleft);
    copy_pair!(cropright);
    copy_pair!(croptop);
    copy_pair!(cropbottom);
    copy_pair!(zoom);
    copy_pair!(zoom_x);
    copy_pair!(zoom_y);
    copy_pair!(basezoom);
    copy_pair!(basezoom_x);
    copy_pair!(basezoom_y);
    copy_pair!(rot_x_deg);
    copy_pair!(rot_y_deg);
    copy_pair!(rot_z_deg);
    copy_pair!(blend);
    copy_pair!(vibrate);
    copy_pair!(effect_magnitude);
    copy_pair!(effect_mode);
    copy_pair!(effect_color1);
    copy_pair!(effect_color2);
    copy_pair!(effect_period);
    copy_pair!(custom_texture_rect);
    copy_pair!(texcoord_velocity);
    copy_pair!(size);
    copy_pair!(stretch_rect);
    (!overlay_delta_is_empty(&out_from)).then_some((out_from, out_to))
}
