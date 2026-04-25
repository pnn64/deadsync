use crate::engine::gfx::TexturedMeshVertex;
use crate::engine::present::actors::{TextAlign, TextAttribute};
use crate::engine::present::anim::{EffectClock, EffectMode};
use std::path::PathBuf;
use std::sync::Arc;

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
    Multiply,
    Subtract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaTextGlowMode {
    Inner,
    Stroke,
    Both,
}

#[derive(Debug, Clone)]
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
    Sound {
        sound_path: PathBuf,
    },
    BitmapText {
        font_name: &'static str,
        font_path: PathBuf,
        text: Arc<str>,
        stroke_color: Option<[f32; 4]>,
        attributes: Arc<[TextAttribute]>,
    },
    ActorMultiVertex {
        vertices: Arc<[SongLuaOverlayMeshVertex]>,
        texture_path: Option<PathBuf>,
    },
    Model {
        layers: Arc<[SongLuaOverlayModelLayer]>,
    },
    SongMeterDisplay {
        stream_width: f32,
        stream_state: SongLuaOverlayState,
        music_length_seconds: f32,
    },
    GraphDisplay {
        size: [f32; 2],
        body_values: Arc<[f32]>,
        body_state: SongLuaOverlayState,
        line_state: SongLuaOverlayState,
    },
    Quad,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayMeshVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayModelDraw {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub zoom: [f32; 3],
    pub tint: [f32; 4],
    pub vert_align: f32,
    pub blend_add: bool,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct SongLuaOverlayModelLayer {
    pub texture_key: Arc<str>,
    pub vertices: Arc<[TexturedMeshVertex]>,
    pub model_size: [f32; 2],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_tex_shift: [f32; 2],
    pub draw: SongLuaOverlayModelDraw,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayState {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub draw_order: i32,
    pub halign: f32,
    pub valign: f32,
    pub text_align: TextAlign,
    pub uppercase: bool,
    pub shadow_len: [f32; 2],
    pub shadow_color: [f32; 4],
    pub glow: [f32; 4],
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: [f32; 4],
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub visible: bool,
    pub cropleft: f32,
    pub cropright: f32,
    pub croptop: f32,
    pub cropbottom: f32,
    pub fadeleft: f32,
    pub faderight: f32,
    pub fadetop: f32,
    pub fadebottom: f32,
    pub mask_source: bool,
    pub mask_dest: bool,
    pub depth_test: bool,
    pub zoom: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
    pub basezoom: f32,
    pub basezoom_x: f32,
    pub basezoom_y: f32,
    pub rot_x_deg: f32,
    pub rot_y_deg: f32,
    pub rot_z_deg: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub blend: SongLuaOverlayBlendMode,
    pub vibrate: bool,
    pub effect_magnitude: [f32; 3],
    pub effect_clock: EffectClock,
    pub effect_mode: EffectMode,
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub effect_offset: f32,
    pub effect_timing: Option<[f32; 5]>,
    pub rainbow: bool,
    pub rainbow_scroll: bool,
    pub text_jitter: bool,
    pub text_distortion: f32,
    pub text_glow_mode: SongLuaTextGlowMode,
    pub mult_attrs_with_diffuse: bool,
    pub sprite_animate: bool,
    pub sprite_loop: bool,
    pub sprite_playback_rate: f32,
    pub sprite_state_delay: f32,
    pub sprite_state_index: Option<u32>,
    pub decode_movie: bool,
    pub vert_spacing: Option<i32>,
    pub wrap_width_pixels: Option<i32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_w_pre_zoom: bool,
    pub max_h_pre_zoom: bool,
    pub max_dimension_uses_zoom: bool,
    pub texture_filtering: bool,
    pub texture_wrapping: bool,
    pub texcoord_offset: Option<[f32; 2]>,
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
            z: 0.0,
            draw_order: 0,
            halign: 0.5,
            valign: 0.5,
            text_align: TextAlign::Center,
            uppercase: false,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            glow: [0.0, 0.0, 0.0, 0.0],
            fov: None,
            vanishpoint: None,
            diffuse: [1.0, 1.0, 1.0, 1.0],
            vertex_colors: None,
            visible: true,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: 0.0,
            faderight: 0.0,
            fadetop: 0.0,
            fadebottom: 0.0,
            mask_source: false,
            mask_dest: false,
            depth_test: false,
            zoom: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
            basezoom: 1.0,
            basezoom_x: 1.0,
            basezoom_y: 1.0,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            blend: SongLuaOverlayBlendMode::Alpha,
            vibrate: false,
            effect_magnitude: [0.0, 0.0, 0.0],
            effect_clock: EffectClock::Time,
            effect_mode: EffectMode::None,
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            effect_offset: 0.0,
            effect_timing: None,
            rainbow: false,
            rainbow_scroll: false,
            text_jitter: false,
            text_distortion: 0.0,
            text_glow_mode: SongLuaTextGlowMode::Both,
            mult_attrs_with_diffuse: false,
            sprite_animate: false,
            sprite_loop: true,
            sprite_playback_rate: 1.0,
            sprite_state_delay: 0.1,
            sprite_state_index: None,
            decode_movie: false,
            vert_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            max_dimension_uses_zoom: false,
            texture_filtering: true,
            texture_wrapping: false,
            texcoord_offset: None,
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
    pub z: Option<f32>,
    pub draw_order: Option<i32>,
    pub halign: Option<f32>,
    pub valign: Option<f32>,
    pub text_align: Option<TextAlign>,
    pub uppercase: Option<bool>,
    pub shadow_len: Option<[f32; 2]>,
    pub shadow_color: Option<[f32; 4]>,
    pub glow: Option<[f32; 4]>,
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: Option<[f32; 4]>,
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub visible: Option<bool>,
    pub cropleft: Option<f32>,
    pub cropright: Option<f32>,
    pub croptop: Option<f32>,
    pub cropbottom: Option<f32>,
    pub fadeleft: Option<f32>,
    pub faderight: Option<f32>,
    pub fadetop: Option<f32>,
    pub fadebottom: Option<f32>,
    pub mask_source: Option<bool>,
    pub mask_dest: Option<bool>,
    pub depth_test: Option<bool>,
    pub zoom: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub basezoom: Option<f32>,
    pub basezoom_x: Option<f32>,
    pub basezoom_y: Option<f32>,
    pub rot_x_deg: Option<f32>,
    pub rot_y_deg: Option<f32>,
    pub rot_z_deg: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
    pub blend: Option<SongLuaOverlayBlendMode>,
    pub vibrate: Option<bool>,
    pub effect_magnitude: Option<[f32; 3]>,
    pub effect_clock: Option<EffectClock>,
    pub effect_mode: Option<EffectMode>,
    pub effect_color1: Option<[f32; 4]>,
    pub effect_color2: Option<[f32; 4]>,
    pub effect_period: Option<f32>,
    pub effect_offset: Option<f32>,
    pub effect_timing: Option<[f32; 5]>,
    pub rainbow: Option<bool>,
    pub rainbow_scroll: Option<bool>,
    pub text_jitter: Option<bool>,
    pub text_distortion: Option<f32>,
    pub text_glow_mode: Option<SongLuaTextGlowMode>,
    pub mult_attrs_with_diffuse: Option<bool>,
    pub sprite_animate: Option<bool>,
    pub sprite_loop: Option<bool>,
    pub sprite_playback_rate: Option<f32>,
    pub sprite_state_delay: Option<f32>,
    pub sprite_state_index: Option<u32>,
    pub vert_spacing: Option<i32>,
    pub wrap_width_pixels: Option<i32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_w_pre_zoom: Option<bool>,
    pub max_h_pre_zoom: Option<bool>,
    pub max_dimension_uses_zoom: Option<bool>,
    pub texture_filtering: Option<bool>,
    pub texture_wrapping: Option<bool>,
    pub texcoord_offset: Option<[f32; 2]>,
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

#[derive(Debug, Clone)]
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
    if let Some(value) = delta.z {
        state.z = value;
    }
    if let Some(value) = delta.draw_order {
        state.draw_order = value;
    }
    if let Some(value) = delta.halign {
        state.halign = value;
    }
    if let Some(value) = delta.valign {
        state.valign = value;
    }
    if let Some(value) = delta.text_align {
        state.text_align = value;
    }
    if let Some(value) = delta.uppercase {
        state.uppercase = value;
    }
    if let Some(value) = delta.shadow_len {
        state.shadow_len = value;
    }
    if let Some(value) = delta.shadow_color {
        state.shadow_color = value;
    }
    if let Some(value) = delta.glow {
        state.glow = value;
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
    if let Some(value) = delta.vertex_colors {
        state.vertex_colors = Some(value);
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
    if let Some(value) = delta.fadeleft {
        state.fadeleft = value;
    }
    if let Some(value) = delta.faderight {
        state.faderight = value;
    }
    if let Some(value) = delta.fadetop {
        state.fadetop = value;
    }
    if let Some(value) = delta.fadebottom {
        state.fadebottom = value;
    }
    if let Some(value) = delta.mask_source {
        state.mask_source = value;
    }
    if let Some(value) = delta.mask_dest {
        state.mask_dest = value;
    }
    if let Some(value) = delta.depth_test {
        state.depth_test = value;
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
    if let Some(value) = delta.zoom_z {
        state.zoom_z = value;
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
    if let Some(value) = delta.skew_x {
        state.skew_x = value;
    }
    if let Some(value) = delta.skew_y {
        state.skew_y = value;
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
    if let Some(value) = delta.effect_clock {
        state.effect_clock = value;
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
    if let Some(value) = delta.effect_offset {
        state.effect_offset = value;
    }
    if let Some(value) = delta.effect_timing {
        state.effect_timing = Some(value);
    }
    if let Some(value) = delta.rainbow {
        state.rainbow = value;
    }
    if let Some(value) = delta.rainbow_scroll {
        state.rainbow_scroll = value;
    }
    if let Some(value) = delta.text_jitter {
        state.text_jitter = value;
    }
    if let Some(value) = delta.text_distortion {
        state.text_distortion = value;
    }
    if let Some(value) = delta.text_glow_mode {
        state.text_glow_mode = value;
    }
    if let Some(value) = delta.mult_attrs_with_diffuse {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = delta.sprite_animate {
        state.sprite_animate = value;
    }
    if let Some(value) = delta.sprite_loop {
        state.sprite_loop = value;
    }
    if let Some(value) = delta.sprite_playback_rate {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = delta.sprite_state_delay {
        state.sprite_state_delay = value;
    }
    if let Some(value) = delta.sprite_state_index {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = delta.vert_spacing {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = delta.wrap_width_pixels {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = delta.max_width {
        state.max_width = Some(value);
    }
    if let Some(value) = delta.max_height {
        state.max_height = Some(value);
    }
    if let Some(value) = delta.max_w_pre_zoom {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = delta.max_h_pre_zoom {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = delta.max_dimension_uses_zoom {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = delta.texture_filtering {
        state.texture_filtering = value;
    }
    if let Some(value) = delta.texture_wrapping {
        state.texture_wrapping = value;
    }
    if let Some(value) = delta.texcoord_offset {
        state.texcoord_offset = Some(value);
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
    if delta.z.is_some() {
        from.z = (to.z - from.z).mul_add(t, from.z);
    }
    if delta.draw_order.is_some() && t >= 1.0 - f32::EPSILON {
        from.draw_order = to.draw_order;
    }
    if delta.halign.is_some() {
        from.halign = (to.halign - from.halign).mul_add(t, from.halign);
    }
    if delta.valign.is_some() {
        from.valign = (to.valign - from.valign).mul_add(t, from.valign);
    }
    if delta.text_align.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_align = to.text_align;
    }
    if delta.uppercase.is_some() && t >= 1.0 - f32::EPSILON {
        from.uppercase = to.uppercase;
    }
    if delta.shadow_len.is_some() {
        from.shadow_len = [
            (to.shadow_len[0] - from.shadow_len[0]).mul_add(t, from.shadow_len[0]),
            (to.shadow_len[1] - from.shadow_len[1]).mul_add(t, from.shadow_len[1]),
        ];
    }
    if delta.shadow_color.is_some() {
        for i in 0..4 {
            from.shadow_color[i] =
                (to.shadow_color[i] - from.shadow_color[i]).mul_add(t, from.shadow_color[i]);
        }
    }
    if delta.glow.is_some() {
        for i in 0..4 {
            from.glow[i] = (to.glow[i] - from.glow[i]).mul_add(t, from.glow[i]);
        }
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
    if delta.vertex_colors.is_some() {
        let mut from_colors = from.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        let to_colors = to.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        for corner in 0..4 {
            for channel in 0..4 {
                from_colors[corner][channel] = (to_colors[corner][channel]
                    - from_colors[corner][channel])
                    .mul_add(t, from_colors[corner][channel]);
            }
        }
        from.vertex_colors = Some(from_colors);
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
    if delta.fadeleft.is_some() {
        from.fadeleft = (to.fadeleft - from.fadeleft).mul_add(t, from.fadeleft);
    }
    if delta.faderight.is_some() {
        from.faderight = (to.faderight - from.faderight).mul_add(t, from.faderight);
    }
    if delta.fadetop.is_some() {
        from.fadetop = (to.fadetop - from.fadetop).mul_add(t, from.fadetop);
    }
    if delta.fadebottom.is_some() {
        from.fadebottom = (to.fadebottom - from.fadebottom).mul_add(t, from.fadebottom);
    }
    if delta.mask_source.is_some() && t >= 1.0 - f32::EPSILON {
        from.mask_source = to.mask_source;
    }
    if delta.mask_dest.is_some() && t >= 1.0 - f32::EPSILON {
        from.mask_dest = to.mask_dest;
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
    if delta.zoom_z.is_some() {
        from.zoom_z = (to.zoom_z - from.zoom_z).mul_add(t, from.zoom_z);
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
    if delta.skew_x.is_some() {
        from.skew_x = (to.skew_x - from.skew_x).mul_add(t, from.skew_x);
    }
    if delta.skew_y.is_some() {
        from.skew_y = (to.skew_y - from.skew_y).mul_add(t, from.skew_y);
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
    if delta.effect_offset.is_some() {
        from.effect_offset = (to.effect_offset - from.effect_offset).mul_add(t, from.effect_offset);
    }
    if delta.effect_timing.is_some()
        && let (Some(from_timing), Some(to_timing)) = (from.effect_timing, to.effect_timing)
    {
        from.effect_timing = Some([
            (to_timing[0] - from_timing[0]).mul_add(t, from_timing[0]),
            (to_timing[1] - from_timing[1]).mul_add(t, from_timing[1]),
            (to_timing[2] - from_timing[2]).mul_add(t, from_timing[2]),
            (to_timing[3] - from_timing[3]).mul_add(t, from_timing[3]),
            (to_timing[4] - from_timing[4]).mul_add(t, from_timing[4]),
        ]);
    }
    if delta.sprite_playback_rate.is_some() {
        from.sprite_playback_rate = (to.sprite_playback_rate - from.sprite_playback_rate)
            .mul_add(t, from.sprite_playback_rate);
    }
    if delta.sprite_state_delay.is_some() {
        from.sprite_state_delay =
            (to.sprite_state_delay - from.sprite_state_delay).mul_add(t, from.sprite_state_delay);
    }
    if delta.sprite_state_index.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_state_index = to.sprite_state_index;
    }
    if delta.vert_spacing.is_some() && t >= 1.0 - f32::EPSILON {
        from.vert_spacing = to.vert_spacing;
    }
    if delta.wrap_width_pixels.is_some() && t >= 1.0 - f32::EPSILON {
        from.wrap_width_pixels = to.wrap_width_pixels;
    }
    if delta.max_width.is_some()
        && let (Some(from_width), Some(to_width)) = (from.max_width, to.max_width)
    {
        from.max_width = Some((to_width - from_width).mul_add(t, from_width));
    }
    if delta.max_height.is_some()
        && let (Some(from_height), Some(to_height)) = (from.max_height, to.max_height)
    {
        from.max_height = Some((to_height - from_height).mul_add(t, from_height));
    }
    if delta.max_w_pre_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_w_pre_zoom = to.max_w_pre_zoom;
    }
    if delta.max_h_pre_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_h_pre_zoom = to.max_h_pre_zoom;
    }
    if delta.max_dimension_uses_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_dimension_uses_zoom = to.max_dimension_uses_zoom;
    }
    if delta.texcoord_offset.is_some()
        && let (Some(from_offset), Some(to_offset)) = (from.texcoord_offset, to.texcoord_offset)
    {
        from.texcoord_offset = Some([
            (to_offset[0] - from_offset[0]).mul_add(t, from_offset[0]),
            (to_offset[1] - from_offset[1]).mul_add(t, from_offset[1]),
        ]);
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
    if delta.effect_clock.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_clock = to.effect_clock;
    }
    if delta.effect_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_mode = to.effect_mode;
    }
    if delta.rainbow.is_some() && t >= 1.0 - f32::EPSILON {
        from.rainbow = to.rainbow;
    }
    if delta.rainbow_scroll.is_some() && t >= 1.0 - f32::EPSILON {
        from.rainbow_scroll = to.rainbow_scroll;
    }
    if delta.text_jitter.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_jitter = to.text_jitter;
    }
    if delta.text_distortion.is_some() {
        from.text_distortion =
            (to.text_distortion - from.text_distortion).mul_add(t, from.text_distortion);
    }
    if delta.text_glow_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_glow_mode = to.text_glow_mode;
    }
    if delta.mult_attrs_with_diffuse.is_some() && t >= 1.0 - f32::EPSILON {
        from.mult_attrs_with_diffuse = to.mult_attrs_with_diffuse;
    }
    if delta.sprite_animate.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_animate = to.sprite_animate;
    }
    if delta.sprite_loop.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_loop = to.sprite_loop;
    }
    if delta.texture_wrapping.is_some() && t >= 1.0 - f32::EPSILON {
        from.texture_wrapping = to.texture_wrapping;
    }
    if delta.texture_filtering.is_some() && t >= 1.0 - f32::EPSILON {
        from.texture_filtering = to.texture_filtering;
    }
    if delta.depth_test.is_some() && t >= 1.0 - f32::EPSILON {
        from.depth_test = to.depth_test;
    }
    from
}

pub(super) fn parse_overlay_blend_mode(raw: &str) -> Option<SongLuaOverlayBlendMode> {
    if raw.eq_ignore_ascii_case("add") || raw.eq_ignore_ascii_case("blendmode_add") {
        Some(SongLuaOverlayBlendMode::Add)
    } else if raw.eq_ignore_ascii_case("multiply") || raw.eq_ignore_ascii_case("blendmode_multiply")
    {
        Some(SongLuaOverlayBlendMode::Multiply)
    } else if raw.eq_ignore_ascii_case("subtract") || raw.eq_ignore_ascii_case("blendmode_subtract")
    {
        Some(SongLuaOverlayBlendMode::Subtract)
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
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Some(EffectMode::None),
        "diffuseramp" => Some(EffectMode::DiffuseRamp),
        "diffuseshift" => Some(EffectMode::DiffuseShift),
        "glowshift" => Some(EffectMode::GlowShift),
        "pulse" => Some(EffectMode::Pulse),
        "bob" => Some(EffectMode::Bob),
        "bounce" => Some(EffectMode::Bounce),
        "wag" => Some(EffectMode::Wag),
        "spin" => Some(EffectMode::Spin),
        _ => None,
    }
}

pub(super) fn parse_overlay_effect_clock(raw: &str) -> Option<EffectClock> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "beat" | "beatnooffset" | "bgm" => Some(EffectClock::Beat),
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            Some(EffectClock::Time)
        }
        _ if lower.contains("beat") => Some(EffectClock::Beat),
        _ if !lower.is_empty() => Some(EffectClock::Time),
        _ => None,
    }
}

pub(super) fn parse_overlay_text_align(raw: &str) -> Option<TextAlign> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "left" | "horizalign_left" => Some(TextAlign::Left),
        "center" | "middle" | "horizalign_center" | "horizalign_middle" => Some(TextAlign::Center),
        "right" | "horizalign_right" => Some(TextAlign::Right),
        _ => None,
    }
}

pub(super) fn parse_overlay_text_glow_mode(raw: &str) -> Option<SongLuaTextGlowMode> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "inner" | "textglowmode_inner" => Some(SongLuaTextGlowMode::Inner),
        "stroke" | "textglowmode_stroke" => Some(SongLuaTextGlowMode::Stroke),
        "both" | "textglowmode_both" => Some(SongLuaTextGlowMode::Both),
        _ => None,
    }
}

fn overlay_delta_is_empty(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.x.is_none()
        && delta.y.is_none()
        && delta.z.is_none()
        && delta.draw_order.is_none()
        && delta.halign.is_none()
        && delta.valign.is_none()
        && delta.text_align.is_none()
        && delta.uppercase.is_none()
        && delta.shadow_len.is_none()
        && delta.shadow_color.is_none()
        && delta.glow.is_none()
        && delta.fov.is_none()
        && delta.vanishpoint.is_none()
        && delta.diffuse.is_none()
        && delta.vertex_colors.is_none()
        && delta.visible.is_none()
        && delta.cropleft.is_none()
        && delta.cropright.is_none()
        && delta.croptop.is_none()
        && delta.cropbottom.is_none()
        && delta.fadeleft.is_none()
        && delta.faderight.is_none()
        && delta.fadetop.is_none()
        && delta.fadebottom.is_none()
        && delta.mask_source.is_none()
        && delta.mask_dest.is_none()
        && delta.depth_test.is_none()
        && delta.zoom.is_none()
        && delta.zoom_x.is_none()
        && delta.zoom_y.is_none()
        && delta.zoom_z.is_none()
        && delta.basezoom.is_none()
        && delta.basezoom_x.is_none()
        && delta.basezoom_y.is_none()
        && delta.rot_x_deg.is_none()
        && delta.rot_y_deg.is_none()
        && delta.rot_z_deg.is_none()
        && delta.skew_x.is_none()
        && delta.skew_y.is_none()
        && delta.blend.is_none()
        && delta.vibrate.is_none()
        && delta.effect_magnitude.is_none()
        && delta.effect_clock.is_none()
        && delta.effect_mode.is_none()
        && delta.effect_color1.is_none()
        && delta.effect_color2.is_none()
        && delta.effect_period.is_none()
        && delta.effect_offset.is_none()
        && delta.effect_timing.is_none()
        && delta.rainbow.is_none()
        && delta.rainbow_scroll.is_none()
        && delta.text_jitter.is_none()
        && delta.text_distortion.is_none()
        && delta.text_glow_mode.is_none()
        && delta.mult_attrs_with_diffuse.is_none()
        && delta.sprite_animate.is_none()
        && delta.sprite_loop.is_none()
        && delta.sprite_playback_rate.is_none()
        && delta.sprite_state_delay.is_none()
        && delta.sprite_state_index.is_none()
        && delta.vert_spacing.is_none()
        && delta.wrap_width_pixels.is_none()
        && delta.max_width.is_none()
        && delta.max_height.is_none()
        && delta.max_w_pre_zoom.is_none()
        && delta.max_h_pre_zoom.is_none()
        && delta.max_dimension_uses_zoom.is_none()
        && delta.texture_filtering.is_none()
        && delta.texture_wrapping.is_none()
        && delta.texcoord_offset.is_none()
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
    if from.z.is_some() {
        into.z = from.z;
    }
    if from.draw_order.is_some() {
        into.draw_order = from.draw_order;
    }
    if from.halign.is_some() {
        into.halign = from.halign;
    }
    if from.valign.is_some() {
        into.valign = from.valign;
    }
    if from.text_align.is_some() {
        into.text_align = from.text_align;
    }
    if from.uppercase.is_some() {
        into.uppercase = from.uppercase;
    }
    if from.shadow_len.is_some() {
        into.shadow_len = from.shadow_len;
    }
    if from.shadow_color.is_some() {
        into.shadow_color = from.shadow_color;
    }
    if from.glow.is_some() {
        into.glow = from.glow;
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
    if from.fadeleft.is_some() {
        into.fadeleft = from.fadeleft;
    }
    if from.faderight.is_some() {
        into.faderight = from.faderight;
    }
    if from.fadetop.is_some() {
        into.fadetop = from.fadetop;
    }
    if from.fadebottom.is_some() {
        into.fadebottom = from.fadebottom;
    }
    if from.mask_source.is_some() {
        into.mask_source = from.mask_source;
    }
    if from.mask_dest.is_some() {
        into.mask_dest = from.mask_dest;
    }
    if from.depth_test.is_some() {
        into.depth_test = from.depth_test;
    }
    if from.halign.is_some() {
        into.halign = from.halign;
    }
    if from.valign.is_some() {
        into.valign = from.valign;
    }
    if from.text_align.is_some() {
        into.text_align = from.text_align;
    }
    if from.shadow_len.is_some() {
        into.shadow_len = from.shadow_len;
    }
    if from.shadow_color.is_some() {
        into.shadow_color = from.shadow_color;
    }
    if from.glow.is_some() {
        into.glow = from.glow;
    }
    if from.vertex_colors.is_some() {
        into.vertex_colors = from.vertex_colors;
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
    if from.zoom_z.is_some() {
        into.zoom_z = from.zoom_z;
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
    if from.skew_x.is_some() {
        into.skew_x = from.skew_x;
    }
    if from.skew_y.is_some() {
        into.skew_y = from.skew_y;
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
    if from.effect_clock.is_some() {
        into.effect_clock = from.effect_clock;
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
    if from.effect_offset.is_some() {
        into.effect_offset = from.effect_offset;
    }
    if from.effect_timing.is_some() {
        into.effect_timing = from.effect_timing;
    }
    if from.rainbow.is_some() {
        into.rainbow = from.rainbow;
    }
    if from.rainbow_scroll.is_some() {
        into.rainbow_scroll = from.rainbow_scroll;
    }
    if from.text_jitter.is_some() {
        into.text_jitter = from.text_jitter;
    }
    if from.text_distortion.is_some() {
        into.text_distortion = from.text_distortion;
    }
    if from.text_glow_mode.is_some() {
        into.text_glow_mode = from.text_glow_mode;
    }
    if from.mult_attrs_with_diffuse.is_some() {
        into.mult_attrs_with_diffuse = from.mult_attrs_with_diffuse;
    }
    if from.sprite_animate.is_some() {
        into.sprite_animate = from.sprite_animate;
    }
    if from.sprite_loop.is_some() {
        into.sprite_loop = from.sprite_loop;
    }
    if from.sprite_playback_rate.is_some() {
        into.sprite_playback_rate = from.sprite_playback_rate;
    }
    if from.sprite_state_delay.is_some() {
        into.sprite_state_delay = from.sprite_state_delay;
    }
    if from.sprite_state_index.is_some() {
        into.sprite_state_index = from.sprite_state_index;
    }
    if from.vert_spacing.is_some() {
        into.vert_spacing = from.vert_spacing;
    }
    if from.wrap_width_pixels.is_some() {
        into.wrap_width_pixels = from.wrap_width_pixels;
    }
    if from.max_width.is_some() {
        into.max_width = from.max_width;
    }
    if from.max_height.is_some() {
        into.max_height = from.max_height;
    }
    if from.max_w_pre_zoom.is_some() {
        into.max_w_pre_zoom = from.max_w_pre_zoom;
    }
    if from.max_h_pre_zoom.is_some() {
        into.max_h_pre_zoom = from.max_h_pre_zoom;
    }
    if from.max_dimension_uses_zoom.is_some() {
        into.max_dimension_uses_zoom = from.max_dimension_uses_zoom;
    }
    if from.texture_filtering.is_some() {
        into.texture_filtering = from.texture_filtering;
    }
    if from.texture_wrapping.is_some() {
        into.texture_wrapping = from.texture_wrapping;
    }
    if from.texcoord_offset.is_some() {
        into.texcoord_offset = from.texcoord_offset;
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
    copy_pair!(z);
    copy_pair!(draw_order);
    copy_pair!(halign);
    copy_pair!(valign);
    copy_pair!(text_align);
    copy_pair!(uppercase);
    copy_pair!(shadow_len);
    copy_pair!(shadow_color);
    copy_pair!(glow);
    copy_pair!(fov);
    copy_pair!(vanishpoint);
    copy_pair!(diffuse);
    copy_pair!(vertex_colors);
    copy_pair!(visible);
    copy_pair!(cropleft);
    copy_pair!(cropright);
    copy_pair!(croptop);
    copy_pair!(cropbottom);
    copy_pair!(fadeleft);
    copy_pair!(faderight);
    copy_pair!(fadetop);
    copy_pair!(fadebottom);
    copy_pair!(mask_source);
    copy_pair!(mask_dest);
    copy_pair!(depth_test);
    copy_pair!(zoom);
    copy_pair!(zoom_x);
    copy_pair!(zoom_y);
    copy_pair!(zoom_z);
    copy_pair!(basezoom);
    copy_pair!(basezoom_x);
    copy_pair!(basezoom_y);
    copy_pair!(rot_x_deg);
    copy_pair!(rot_y_deg);
    copy_pair!(rot_z_deg);
    copy_pair!(skew_x);
    copy_pair!(skew_y);
    copy_pair!(blend);
    copy_pair!(vibrate);
    copy_pair!(effect_magnitude);
    copy_pair!(effect_clock);
    copy_pair!(effect_mode);
    copy_pair!(effect_color1);
    copy_pair!(effect_color2);
    copy_pair!(effect_period);
    copy_pair!(effect_offset);
    copy_pair!(effect_timing);
    copy_pair!(rainbow);
    copy_pair!(rainbow_scroll);
    copy_pair!(text_jitter);
    copy_pair!(text_distortion);
    copy_pair!(text_glow_mode);
    copy_pair!(mult_attrs_with_diffuse);
    copy_pair!(sprite_animate);
    copy_pair!(sprite_loop);
    copy_pair!(sprite_playback_rate);
    copy_pair!(sprite_state_delay);
    copy_pair!(sprite_state_index);
    copy_pair!(vert_spacing);
    copy_pair!(wrap_width_pixels);
    copy_pair!(max_width);
    copy_pair!(max_height);
    copy_pair!(max_w_pre_zoom);
    copy_pair!(max_h_pre_zoom);
    copy_pair!(max_dimension_uses_zoom);
    copy_pair!(texture_filtering);
    copy_pair!(texture_wrapping);
    copy_pair!(texcoord_offset);
    copy_pair!(custom_texture_rect);
    copy_pair!(texcoord_velocity);
    copy_pair!(size);
    copy_pair!(stretch_rect);
    (!overlay_delta_is_empty(&out_from)).then_some((out_from, out_to))
}

#[cfg(test)]
mod tests {
    use super::{
        SongLuaOverlayBlendMode, parse_overlay_blend_mode, parse_overlay_effect_clock,
        parse_overlay_effect_mode,
    };
    use crate::engine::present::anim::{EffectClock, EffectMode};

    #[test]
    fn parse_overlay_blend_mode_accepts_stepmania_add_name() {
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Add"),
            Some(SongLuaOverlayBlendMode::Add)
        );
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Multiply"),
            Some(SongLuaOverlayBlendMode::Multiply)
        );
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Subtract"),
            Some(SongLuaOverlayBlendMode::Subtract)
        );
    }

    #[test]
    fn parse_overlay_effect_mode_accepts_song_lua_effect_names() {
        assert_eq!(
            parse_overlay_effect_mode("DiffuseRamp"),
            Some(EffectMode::DiffuseRamp)
        );
        assert_eq!(
            parse_overlay_effect_mode("glowshift"),
            Some(EffectMode::GlowShift)
        );
        assert_eq!(
            parse_overlay_effect_mode("bounce"),
            Some(EffectMode::Bounce)
        );
        assert_eq!(parse_overlay_effect_mode("wag"), Some(EffectMode::Wag));
    }

    #[test]
    fn parse_overlay_effect_clock_accepts_music_and_bgm_aliases() {
        assert_eq!(parse_overlay_effect_clock("beat"), Some(EffectClock::Beat));
        assert_eq!(parse_overlay_effect_clock("bgm"), Some(EffectClock::Beat));
        assert_eq!(parse_overlay_effect_clock("music"), Some(EffectClock::Time));
    }
}
