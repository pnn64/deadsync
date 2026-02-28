use crate::act;
use crate::config;
use crate::core::gfx::{BlendMode, MeshMode, TexturedMeshVertex};
use crate::core::space::*;
use crate::game::gameplay::{
    COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind,
    HOLD_JUDGMENT_TOTAL_DURATION, MAX_COLS, RECEPTOR_Y_OFFSET_FROM_CENTER,
    RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, TRANSITION_IN_DURATION,
};
use crate::game::gameplay::{
    active_hold_is_engaged, effective_attack_mini_percent_delta_for_player,
    effective_scroll_speed_for_player, effective_visual_mask_for_player,
    receptor_glow_visual_for_col,
};
use crate::game::judgment::{HOLD_SCORE_HELD, JudgeGrade, TimingWindow};
use crate::game::note::{HoldResult, NoteType};
use crate::game::parsing::noteskin::{
    ModelDrawState, ModelEffectMode, ModelMesh, NUM_QUANTIZATIONS, NoteAnimPart, SpriteSlot,
};
use crate::game::{gameplay::PlayerRuntime, gameplay::State, profile, scroll::ScrollSpeedSetting};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use cgmath::{Deg, Matrix4, Point3, Vector3};
use rssp::streams::StreamSegment;
use std::array::from_fn;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use std::thread::LocalKey;
use twox_hash::XxHash64;

// --- CONSTANTS ---

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Match Simply Love's on-screen arrow height
const HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER: f32 = -90.0; // Mirrors Simply Love metrics for hold judgments
const HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR: f32 =
    HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER - RECEPTOR_Y_OFFSET_FROM_CENTER;
const TAP_JUDGMENT_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback JudgmentTransformCommand
const COMBO_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback ComboTransformCommand (non-centered)
const COLUMN_CUE_Y_OFFSET: f32 = 80.0;
const COLUMN_CUE_TEXT_NORMAL_Y: f32 = 80.0;
const COLUMN_CUE_TEXT_REVERSE_Y: f32 = 260.0;
const COLUMN_CUE_FADE_TIME: f32 = 0.15;
const COLUMN_CUE_BASE_ALPHA: f32 = 0.12;
const LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT: f32 = 140.0; // Each frame in Love 1x2 (doubleres).png is 140px tall
const HOLD_JUDGMENT_FINAL_HEIGHT: f32 = 32.0; // Matches Simply Love's final on-screen size
const HOLD_JUDGMENT_INITIAL_HEIGHT: f32 = HOLD_JUDGMENT_FINAL_HEIGHT * 0.8; // Mirrors 0.4->0.5 zoom ramp in metrics
const HOLD_JUDGMENT_FINAL_ZOOM: f32 =
    HOLD_JUDGMENT_FINAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const HOLD_JUDGMENT_INITIAL_ZOOM: f32 =
    HOLD_JUDGMENT_INITIAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const ERROR_BAR_JUDGMENT_HEIGHT: f32 = 40.0; // SL: judgmentHeight in SL-Layout.lua
const ERROR_BAR_OFFSET_FROM_JUDGMENT: f32 = ERROR_BAR_JUDGMENT_HEIGHT * 0.5 + 5.0; // SL: top/bottom +/-25px

const ERROR_BAR_WIDTH_COLORFUL: f32 = 160.0;
const ERROR_BAR_HEIGHT_COLORFUL: f32 = 10.0;
const ERROR_BAR_WIDTH_AVERAGE: f32 = 325.0;
const ERROR_BAR_HEIGHT_AVERAGE: f32 = 7.0;
const ERROR_BAR_WIDTH_MONOCHROME: f32 = 240.0;
const ERROR_BAR_TICK_WIDTH: f32 = 2.0;
const ERROR_BAR_TICK_DUR_COLORFUL: f32 = 0.5;
const ERROR_BAR_TICK_DUR_MONOCHROME: f32 = 0.75;
const ERROR_BAR_AVERAGE_Y_OFFSET: f32 = -70.0;
const ERROR_BAR_AVERAGE_TICK_EXTRA_H: f32 = 75.0;
const ERROR_BAR_SEG_ALPHA_BASE: f32 = 0.3;
const ERROR_BAR_MONO_BG_ALPHA: f32 = 0.5;
const ERROR_BAR_LINE_ALPHA: f32 = 0.3;
const ERROR_BAR_LINES_FADE_START_S: f32 = 2.5;
const ERROR_BAR_LINES_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_HOLD_S: f32 = 2.0;
const OFFSET_INDICATOR_DUR_S: f32 = 0.5;

const ERROR_BAR_COLORFUL_TICK_RGBA: [f32; 4] = color::rgba_hex("#b20000");
const TEXT_CACHE_LIMIT: usize = 4096;

// Visual Feedback
const SHOW_COMBO_AT: u32 = 4; // From Simply Love metrics

// Z-order layers for key gameplay visuals (higher draws on top)
const Z_RECEPTOR: i32 = 100;
const Z_HOLD_BODY: i32 = 110;
const Z_HOLD_CAP: i32 = 110;
// ITG draws GhostArrowRow after columns; keep hold/roll ghost arrows above note lanes.
const Z_HOLD_EXPLOSION: i32 = 145;
// ITG's Explosion actor declares hold/roll children before tap judgments, so taps render on top.
const Z_TAP_EXPLOSION: i32 = 150;
const Z_HOLD_GLOW: i32 = 130;
const Z_MINE_EXPLOSION: i32 = 101;
const Z_TAP_NOTE: i32 = 140;
const Z_COLUMN_CUE: i32 = 90;
const MINE_CORE_SIZE_RATIO: f32 = 0.45;
const Z_MEASURE_LINES: i32 = 80;

const VISUAL_MASK_DIZZY: u16 = 1 << 1;
const VISUAL_MASK_CONFUSION: u16 = 1 << 2;
const VISUAL_MASK_BIG: u16 = 1 << 3;

type TextCache<K> = HashMap<K, Arc<str>, BuildHasherDefault<XxHash64>>;

thread_local! {
    static FMT2_CACHE_F32: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static PERCENT2_CACHE_F64: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static SIGNED_PERCENT2_CACHE_F64: RefCell<TextCache<(u64, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(512, BuildHasherDefault::default()),
    );
    static NEG_INT_CACHE_U32: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        256,
        BuildHasherDefault::default(),
    ));
    static PAREN_INT_CACHE_I32: RefCell<TextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static INT_CACHE_I32: RefCell<TextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static RATIO_CACHE_I32: RefCell<TextCache<(i32, i32)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(1024, BuildHasherDefault::default()),
    );
    static OFFSET_MS_CACHE_F32: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static RUN_TIMER_CACHE: RefCell<TextCache<(i32, i32, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(1024, BuildHasherDefault::default()),
    );
}

#[inline(always)]
fn cached_text<K, F>(cache: &'static LocalKey<RefCell<TextCache<K>>>, key: K, build: F) -> Arc<str>
where
    K: Copy + Eq + std::hash::Hash,
    F: FnOnce() -> String,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&key) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(build());
        if cache.len() < TEXT_CACHE_LIMIT {
            cache.insert(key, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_fmt2_f32(value: f32) -> Arc<str> {
    cached_text(&FMT2_CACHE_F32, value.to_bits(), || format!("{value:.2}"))
}

#[inline(always)]
fn cached_percent2_f64(value: f64) -> Arc<str> {
    cached_text(&PERCENT2_CACHE_F64, value.to_bits(), || {
        format!("{value:.2}%")
    })
}

#[inline(always)]
fn cached_signed_percent2_f64(value: f64, neg: bool) -> Arc<str> {
    cached_text(&SIGNED_PERCENT2_CACHE_F64, (value.to_bits(), neg), || {
        if neg {
            format!("-{value:.2}%")
        } else {
            format!("+{value:.2}%")
        }
    })
}

#[inline(always)]
fn cached_neg_int_u32(value: u32) -> Arc<str> {
    cached_text(&NEG_INT_CACHE_U32, value, || format!("-{value}"))
}

#[inline(always)]
fn cached_paren_i32(value: i32) -> Arc<str> {
    cached_text(&PAREN_INT_CACHE_I32, value, || format!("({value})"))
}

#[inline(always)]
fn cached_int_i32(value: i32) -> Arc<str> {
    cached_text(&INT_CACHE_I32, value, || value.to_string())
}

#[inline(always)]
fn cached_ratio_i32(curr: i32, total: i32) -> Arc<str> {
    cached_text(&RATIO_CACHE_I32, (curr, total), || {
        format!("{curr}/{total}")
    })
}

#[inline(always)]
fn cached_offset_ms(value: f32) -> Arc<str> {
    cached_text(&OFFSET_MS_CACHE_F32, value.to_bits(), || {
        format!("{value:.2}ms")
    })
}

fn cached_run_timer(seconds: i32, minute_threshold: i32, trailing_space: bool) -> Arc<str> {
    let seconds = seconds.max(0);
    cached_text(
        &RUN_TIMER_CACHE,
        (seconds, minute_threshold, trailing_space),
        || {
            let mut s = if seconds < 10 {
                format!("0.0{seconds}")
            } else if seconds > minute_threshold {
                let minutes = seconds / 60;
                let secs = seconds % 60;
                format!("{minutes}.{secs:02}")
            } else {
                format!("0.{seconds}")
            };
            if trailing_space {
                s.push(' ');
            }
            s
        },
    )
}

#[derive(Clone, Copy, Debug)]
pub enum FieldPlacement {
    P1,
    P2,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ModelMeshCacheKey {
    slot: *const SpriteSlot,
    size: [u32; 2],
    rotation: u32,
    pos: [u32; 3],
    rot: [u32; 3],
    zoom: [u32; 3],
    vert_align: u32,
    tint: [u32; 4],
}

#[derive(Default)]
struct ModelMeshCache {
    entries: HashMap<ModelMeshCacheKey, Arc<[TexturedMeshVertex]>, BuildHasherDefault<XxHash64>>,
}

impl ModelMeshCache {
    #[inline(always)]
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default()),
        }
    }

    #[inline(always)]
    fn get_or_insert_with<F>(
        &mut self,
        key: ModelMeshCacheKey,
        build: F,
    ) -> Arc<[TexturedMeshVertex]>
    where
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        if let Some(vertices) = self.entries.get(&key) {
            return vertices.clone();
        }
        let vertices = build();
        self.entries.insert(key, vertices.clone());
        vertices
    }
}

#[inline(always)]
const fn norm_bits(v: f32) -> u32 {
    if v == 0.0 {
        0.0f32.to_bits()
    } else {
        v.to_bits()
    }
}

#[inline(always)]
fn model_cache_key(
    slot: &SpriteSlot,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
    tint: [f32; 4],
) -> ModelMeshCacheKey {
    ModelMeshCacheKey {
        slot: slot as *const SpriteSlot,
        size: [norm_bits(size[0]), norm_bits(size[1])],
        rotation: norm_bits(rotation_deg),
        pos: [
            norm_bits(draw.pos[0]),
            norm_bits(draw.pos[1]),
            norm_bits(draw.pos[2]),
        ],
        rot: [
            norm_bits(draw.rot[0]),
            norm_bits(draw.rot[1]),
            norm_bits(draw.rot[2]),
        ],
        zoom: [
            norm_bits(draw.zoom[0]),
            norm_bits(draw.zoom[1]),
            norm_bits(draw.zoom[2]),
        ],
        vert_align: norm_bits(draw.vert_align),
        tint: [
            norm_bits(tint[0]),
            norm_bits(tint[1]),
            norm_bits(tint[2]),
            norm_bits(tint[3]),
        ],
    }
}

#[inline(always)]
fn model_uv_params(slot: &SpriteSlot, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = match slot.source.as_ref() {
        crate::game::parsing::noteskin::SpriteSource::Atlas { tex_dims, .. } => {
            let tw = tex_dims.0.max(1) as f32;
            let th = tex_dims.1.max(1) as f32;
            let base_u0 = slot.def.src[0] as f32 / tw;
            let base_v0 = slot.def.src[1] as f32 / th;
            [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
        }
        crate::game::parsing::noteskin::SpriteSource::Animated { .. } => [0.0, 0.0],
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

#[inline(always)]
fn translated_uv_rect(mut uv: [f32; 4], translate: [f32; 2]) -> [f32; 4] {
    uv[0] += translate[0];
    uv[1] += translate[1];
    uv[2] += translate[0];
    uv[3] += translate[1];
    uv
}

#[inline(always)]
const fn tap_part_for_note_type(note_type: NoteType) -> NoteAnimPart {
    match note_type {
        NoteType::Fake => NoteAnimPart::Fake,
        _ => NoteAnimPart::Tap,
    }
}

#[inline(always)]
fn note_scale_height(slot: &SpriteSlot) -> f32 {
    if let Some(model) = slot.model.as_ref() {
        let model_h = model.size()[1];
        if model_h > f32::EPSILON {
            return model_h;
        }
    }
    slot.logical_size()[1].max(1.0)
}

#[inline(always)]
fn model_tint(color: [f32; 4], draw: ModelDrawState) -> [f32; 4] {
    [
        color[0] * draw.tint[0],
        color[1] * draw.tint[1],
        color[2] * draw.tint[2],
        color[3] * draw.tint[3],
    ]
}

#[inline(always)]
const fn model_blend(draw: ModelDrawState, blend: BlendMode) -> BlendMode {
    if draw.blend_add {
        BlendMode::Add
    } else {
        blend
    }
}

#[inline(always)]
fn with_sprite_local_offset(
    mut actor: Actor,
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
) -> Actor {
    if let Actor::Sprite {
        local_offset: actor_offset,
        local_offset_rot_sin_cos: actor_rot,
        ..
    } = &mut actor
    {
        *actor_offset = local_offset;
        *actor_rot = local_offset_rot_sin_cos;
    }
    actor
}

#[inline(always)]
fn build_model_vertices(
    slot: &SpriteSlot,
    model: &ModelMesh,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
    tint: [f32; 4],
) -> Arc<[TexturedMeshVertex]> {
    let model_size = model.size();
    let model_h = model_size[1];
    let scale = if model_h > f32::EPSILON && size[1] > f32::EPSILON {
        size[1] / model_h
    } else {
        1.0
    };
    let zoom = [
        draw.zoom[0].max(0.0),
        draw.zoom[1].max(0.0),
        draw.zoom[2].max(0.0),
    ];
    let local_scale = [scale * zoom[0], scale * zoom[1], scale * zoom[2]];
    let rx = draw.rot[0].to_radians();
    let ry = draw.rot[1].to_radians();
    let rz = (draw.rot[2] + rotation_deg).to_radians();
    let (sin_x, cos_x) = rx.sin_cos();
    let (sin_y, cos_y) = ry.sin_cos();
    let (sin_z, cos_z) = rz.sin_cos();
    let tx = draw.pos[0] * scale;
    let ty = draw.pos[1] * scale;
    let tz = draw.pos[2] * scale;
    let focal = model_size[0]
        .max(model_size[1])
        .mul_add(6.0, 0.0)
        .max(180.0);
    let align_y = (0.5 - draw.vert_align) * size[1];

    let mut vertices = Vec::with_capacity(model.vertices.len());
    for v in model.vertices.iter() {
        let mut lx = v.pos[0] * local_scale[0];
        let mut ly = v.pos[1] * local_scale[1] + align_y;
        let lz = v.pos[2] * local_scale[2];
        if slot.def.mirror_h {
            lx = -lx;
        }
        if slot.def.mirror_v {
            ly = -ly;
        }

        let x1 = lx;
        let y1 = ly.mul_add(cos_x, -lz * sin_x);
        let z1 = ly.mul_add(sin_x, lz * cos_x);

        let x2 = x1.mul_add(cos_y, z1 * sin_y);
        let y2 = y1;
        let z2 = z1.mul_add(cos_y, -x1 * sin_y);

        let x3 = x2.mul_add(cos_z, -y2 * sin_z) + tx;
        let y3 = x2.mul_add(sin_z, y2 * cos_z) + ty;
        let y_screen = -y3;
        let z3 = z2 + tz;
        let perspective = focal / (focal - z3).max(1.0);
        let u = if slot.def.mirror_h {
            1.0 - v.uv[0]
        } else {
            v.uv[0]
        };
        let v_tex = if slot.def.mirror_v {
            1.0 - v.uv[1]
        } else {
            v.uv[1]
        };

        vertices.push(TexturedMeshVertex {
            pos: [x3 * perspective, y_screen * perspective],
            uv: [u, v_tex],
            tex_matrix_scale: v.tex_matrix_scale,
            color: tint,
        });
    }
    Arc::from(vertices)
}

#[inline(always)]
fn noteskin_model_actor_from_vertices(
    slot: &SpriteSlot,
    xy: [f32; 2],
    vertices: Arc<[TexturedMeshVertex]>,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    blend: BlendMode,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: xy,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        texture: slot.texture_key().to_string(),
        vertices,
        mode: MeshMode::Triangles,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        visible: true,
        blend,
        z,
    }
}

#[inline(always)]
fn noteskin_model_actor_from_draw(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let blend = model_blend(draw, blend);
    let vertices = build_model_vertices(slot, model, size, rotation_deg, draw, tint);
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(noteskin_model_actor_from_vertices(
        slot,
        xy,
        vertices,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        blend,
        z,
    ))
}

#[inline(always)]
fn noteskin_model_actor_from_draw_cached(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
    cache: &mut ModelMeshCache,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let key = model_cache_key(slot, size, rotation_deg, draw, tint);
    let vertices = cache.get_or_insert_with(key, || {
        build_model_vertices(slot, model, size, rotation_deg, draw, tint)
    });
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(noteskin_model_actor_from_vertices(
        slot,
        xy,
        vertices,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        model_blend(draw, blend),
        z,
    ))
}

#[inline(always)]
pub(crate) fn noteskin_model_actor(
    slot: &SpriteSlot,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    elapsed: f32,
    beat: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let draw = slot.model_draw_at(elapsed, beat);
    noteskin_model_actor_from_draw(slot, draw, xy, size, uv_rect, rotation_deg, color, blend, z)
}

#[inline(always)]
fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

#[inline(always)]
fn calc_note_rotation_z(mask: u16, note_beat: f32, song_beat: f32, is_hold_head: bool) -> f32 {
    let mut r = 0.0;
    if (mask & VISUAL_MASK_CONFUSION) != 0 {
        let mut conf = song_beat;
        conf = conf.rem_euclid(2.0 * std::f32::consts::PI);
        r += conf * (-180.0 / std::f32::consts::PI);
    }
    if (mask & VISUAL_MASK_DIZZY) != 0 && !is_hold_head {
        let mut dizzy = note_beat - song_beat;
        dizzy = dizzy.rem_euclid(2.0 * std::f32::consts::PI);
        r += dizzy * (180.0 / std::f32::consts::PI);
    }
    r
}

#[inline(always)]
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline(always)]
fn effective_mini_value(
    profile: &profile::Profile,
    visual_mask: u16,
    attack_mini_percent_delta: f32,
) -> f32 {
    let attack_delta = if attack_mini_percent_delta.is_finite() {
        attack_mini_percent_delta
    } else {
        0.0
    };
    let mut mini = (profile.mini_percent as f32 + attack_delta).clamp(-100.0, 150.0);
    if (visual_mask & VISUAL_MASK_BIG) != 0 {
        // ITG _fallback/ArrowCloud map Effect Big to mod,-100% mini.
        mini -= 100.0;
    }
    mini / 100.0
}

#[inline(always)]
fn mini_judgment_zoom(mini: f32) -> f32 {
    0.5_f32.powf(mini).min(1.0)
}

#[inline(always)]
fn format_speed_mod_for_display(speed: ScrollSpeedSetting) -> String {
    let fmt_float = |v: f32| -> String {
        let s = cached_fmt2_f32(v);
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    };

    match speed {
        ScrollSpeedSetting::XMod(mult) => {
            if (mult - 1.0).abs() <= 0.000_1 {
                "1x".to_string()
            } else {
                let mut out = fmt_float(mult);
                out.push('x');
                out
            }
        }
        ScrollSpeedSetting::CMod(bpm) => {
            if (bpm - bpm.round()).abs() <= 0.000_1 {
                let mut out = String::from("C");
                out.push_str(&(bpm.round() as i32).to_string());
                out
            } else {
                let mut out = String::from("C");
                out.push_str(&fmt_float(bpm));
                out
            }
        }
        ScrollSpeedSetting::MMod(bpm) => {
            if (bpm - bpm.round()).abs() <= 0.000_1 {
                let mut out = String::from("M");
                out.push_str(&(bpm.round() as i32).to_string());
                out
            } else {
                let mut out = String::from("M");
                out.push_str(&fmt_float(bpm));
                out
            }
        }
    }
}

#[inline(always)]
fn gameplay_mods_text(scroll_speed: ScrollSpeedSetting, profile: &profile::Profile) -> String {
    let mut parts = Vec::with_capacity(10);
    parts.push(format_speed_mod_for_display(scroll_speed));
    let scroll = profile.scroll_option;
    if scroll.contains(profile::ScrollOption::Reverse) {
        parts.push("Reverse".to_string());
    }
    if scroll.contains(profile::ScrollOption::Split) {
        parts.push("Split".to_string());
    }
    if scroll.contains(profile::ScrollOption::Alternate) {
        parts.push("Alternate".to_string());
    }
    if scroll.contains(profile::ScrollOption::Cross) {
        parts.push("Cross".to_string());
    }
    if scroll.contains(profile::ScrollOption::Centered) {
        parts.push("Centered".to_string());
    }
    if !matches!(profile.turn_option, profile::TurnOption::None) {
        parts.push(profile.turn_option.to_string());
    }
    if profile.mini_percent != 0 {
        let mut part = profile.mini_percent.to_string();
        part.push_str("% Mini");
        parts.push(part);
    }
    parts.push(profile.perspective.to_string());
    if profile.visual_delay_ms != 0 {
        let mut part = profile.visual_delay_ms.to_string();
        part.push_str("ms VisualDelay");
        parts.push(part);
    }
    parts.join(", ")
}

#[inline(always)]
fn active_column_cue(
    cues: &[crate::game::gameplay::ColumnCue],
    current_time: f32,
) -> Option<&crate::game::gameplay::ColumnCue> {
    if cues.is_empty() {
        return None;
    }
    let idx = cues.partition_point(|cue| cue.start_time <= current_time);
    idx.checked_sub(1).and_then(|i| cues.get(i))
}

#[inline(always)]
fn column_cue_alpha(elapsed_real: f32, duration_real: f32) -> f32 {
    if !elapsed_real.is_finite() || !duration_real.is_finite() {
        return 0.0;
    }
    if elapsed_real < 0.0 || elapsed_real > duration_real {
        return 0.0;
    }
    if duration_real <= COLUMN_CUE_FADE_TIME * 2.0 {
        return 0.0;
    }
    if elapsed_real < COLUMN_CUE_FADE_TIME {
        let t = (elapsed_real / COLUMN_CUE_FADE_TIME).clamp(0.0, 1.0);
        return 1.0 - (1.0 - t) * (1.0 - t);
    }
    if elapsed_real > duration_real - COLUMN_CUE_FADE_TIME {
        let t = ((elapsed_real - (duration_real - COLUMN_CUE_FADE_TIME)) / COLUMN_CUE_FADE_TIME)
            .clamp(0.0, 1.0);
        return 1.0 - t * t;
    }
    1.0
}

#[inline(always)]
const fn timing_window_from_num(n: usize) -> TimingWindow {
    match n {
        0 => TimingWindow::W0,
        1 => TimingWindow::W1,
        2 => TimingWindow::W2,
        3 => TimingWindow::W3,
        4 => TimingWindow::W4,
        _ => TimingWindow::W5,
    }
}

#[inline(always)]
fn error_bar_color_for_window(window: TimingWindow, show_fa_plus_window: bool) -> [f32; 4] {
    match window {
        TimingWindow::W0 => color::JUDGMENT_RGBA[0],
        TimingWindow::W1 => {
            if show_fa_plus_window {
                color::JUDGMENT_FA_PLUS_WHITE_RGBA
            } else {
                color::JUDGMENT_RGBA[0]
            }
        }
        TimingWindow::W2 => color::JUDGMENT_RGBA[1],
        TimingWindow::W3 => color::JUDGMENT_RGBA[2],
        TimingWindow::W4 => color::JUDGMENT_RGBA[3],
        TimingWindow::W5 => color::JUDGMENT_RGBA[4],
    }
}

#[inline(always)]
fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if !age.is_finite() || age < 0.0 {
        return 0.0;
    }
    if multi_tick {
        if age < 0.03 {
            1.0
        } else if age < dur {
            1.0 - (age - 0.03) / (dur - 0.03).max(0.000_001)
        } else {
            0.0
        }
    } else if age < dur {
        1.0
    } else {
        0.0
    }
}

#[inline(always)]
fn error_bar_flash_alpha(now: f32, started_at: Option<f32>, dur: f32) -> f32 {
    let Some(t0) = started_at else {
        return ERROR_BAR_SEG_ALPHA_BASE;
    };
    let age = now - t0;
    if !age.is_finite() || age < 0.0 || age >= dur {
        return ERROR_BAR_SEG_ALPHA_BASE;
    }
    let t = (age / dur).clamp(0.0, 1.0);
    1.0 - (1.0 - ERROR_BAR_SEG_ALPHA_BASE) * t
}

#[inline(always)]
fn error_bar_trim_max_window_ix(trim: profile::ErrorBarTrim) -> usize {
    match trim {
        profile::ErrorBarTrim::Off => 4,       // W5
        profile::ErrorBarTrim::Fantastic => 0, // W1
        profile::ErrorBarTrim::Excellent => 1, // W2
        profile::ErrorBarTrim::Great => 2,     // W3
    }
}

#[inline(always)]
fn error_bar_boundaries_s(
    windows_s: [f32; 5],
    w0_s: Option<f32>,
    show_fa_plus_window: bool,
    trim: profile::ErrorBarTrim,
) -> ([f32; 6], usize) {
    let mut out = [0.0_f32; 6];
    let mut len: usize = 0;
    let base_end = error_bar_trim_max_window_ix(trim) + 1; // 1..=5
    for wi in 1..=base_end {
        if show_fa_plus_window && wi == 1 {
            if let Some(w0) = w0_s
                && len < out.len()
            {
                out[len] = w0;
                len += 1;
            }
            if len < out.len() {
                out[len] = windows_s[0];
                len += 1;
            }
        } else if len < out.len() {
            out[len] = windows_s[wi - 1];
            len += 1;
        }
    }
    (out, len)
}

#[derive(Clone, Copy, Debug)]
struct ZmodLayoutYs {
    combo_y: f32,
    measure_counter_y: Option<f32>,
    subtractive_scoring_y: f32,
}

#[inline(always)]
fn zmod_layout_ys(
    profile: &crate::game::profile::Profile,
    judgment_y: f32,
    combo_y_base: f32,
    reverse: bool,
) -> ZmodLayoutYs {
    let mut top_y = judgment_y - ERROR_BAR_JUDGMENT_HEIGHT * 0.5;
    let mut bottom_y = judgment_y + ERROR_BAR_JUDGMENT_HEIGHT * 0.5;

    // Zmod SL-Layout.lua: hasErrorBar checks multiple flags.
    let mut error_bar_mask = profile::normalize_error_bar_mask(profile.error_bar_active_mask);
    if error_bar_mask == 0 {
        error_bar_mask =
            profile::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let has_error_bar = error_bar_mask != 0;
    if has_error_bar {
        if matches!(
            profile.judgment_graphic,
            crate::game::profile::JudgmentGraphic::None
        ) {
            // Error bar replaces judgment; no top/bottom adjustment.
        } else if profile.error_bar_up {
            top_y -= 15.0;
        } else {
            bottom_y += 15.0;
        }
    }

    let mut measure_counter_y = None;
    let has_measure_counter = profile.measure_counter != crate::game::profile::MeasureCounter::None;
    if has_measure_counter {
        if profile.measure_counter_up {
            let mut y = top_y - 8.0;
            top_y -= 20.0;
            if profile.broken_run {
                y -= 16.0;
            }
            measure_counter_y = Some(y);
        } else {
            measure_counter_y = Some(bottom_y + 8.0);
            bottom_y += 21.0;
        }
    }

    // Zmod: HideLookahead is not implemented in deadsync, so we always take the normal branch.
    let subtractive_scoring_y = if has_measure_counter && profile.measure_counter_up {
        let y = bottom_y + 8.0;
        bottom_y += 16.0;
        y
    } else {
        let y = top_y - 8.0;
        top_y -= 16.0;
        y
    };

    let combo_y = if reverse {
        combo_y_base.min(top_y - 20.0)
    } else {
        combo_y_base.max(bottom_y + 20.0)
    };

    ZmodLayoutYs {
        combo_y,
        measure_counter_y,
        subtractive_scoring_y,
    }
}

#[inline(always)]
fn stream_segment_index_exclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure >= s.end as f32)
}

#[inline(always)]
fn stream_segment_index_inclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure > s.end as f32)
}

fn zmod_measure_counter_text(
    curr_beat_floor: f32,
    curr_measure: f32,
    segs: &[StreamSegment],
    stream_index_unshifted: usize,
    is_lookahead: bool,
    lookahead: u8,
    multiplier: f32,
) -> Option<Arc<str>> {
    if segs.is_empty() {
        return None;
    }

    let mut stream_index = stream_index_unshifted as isize;
    let beat_div4 = curr_beat_floor / 4.0;

    if curr_measure < 0.0 {
        if !is_lookahead {
            let first = segs[0];
            if !first.is_break {
                let v = ((beat_div4 * -1.0) + (1.0 * multiplier)).floor() as i32;
                return Some(cached_paren_i32(v));
            }
            let len = (first.end - first.start) as i32;
            let v_unscaled = (beat_div4 * -1.0).floor() as i32 + 1 + len;
            let v = ((v_unscaled as f32) * multiplier).floor() as i32;
            return Some(cached_paren_i32(v));
        }
        if !segs[0].is_break {
            stream_index -= 1;
        }
    }

    let Some(seg) = stream_index
        .try_into()
        .ok()
        .and_then(|i: usize| segs.get(i).copied())
    else {
        return None;
    };

    let segment_start = seg.start as f32;
    let segment_end = seg.end as f32;
    let seg_len = ((segment_end - segment_start) * multiplier).floor() as i32;
    let curr_count = (((beat_div4 - segment_start) * multiplier).floor() as i32) + 1;

    if seg.is_break {
        if lookahead == 0 {
            return None;
        }
        if is_lookahead {
            Some(cached_paren_i32(seg_len))
        } else {
            let remaining = seg_len - curr_count + 1;
            Some(cached_paren_i32(remaining))
        }
    } else if !is_lookahead && curr_count != 0 {
        Some(cached_ratio_i32(curr_count, seg_len))
    } else {
        Some(cached_int_i32(seg_len))
    }
}

fn zmod_broken_run_end(segs: &[StreamSegment], start_index: usize) -> (usize, bool) {
    let Some(first) = segs.get(start_index).copied() else {
        return (0, false);
    };
    if first.is_break {
        return (first.end, false);
    }

    let last_index = segs.len().saturating_sub(1);
    let mut end = first.end;
    let mut broken = false;

    for i in (start_index + 1)..segs.len() {
        let seg = segs[i];
        let len = seg.end - seg.start;
        if seg.is_break {
            if len < 4 && i != last_index {
                end += len;
                broken = true;
                continue;
            }
            break;
        }

        broken = true;
        end += len;
        if !segs[i - 1].is_break {
            end += 1;
        }
    }

    (end, broken)
}

fn zmod_broken_run_segment(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> Option<(usize, usize, bool)> {
    for (i, seg) in segs.iter().copied().enumerate() {
        if seg.is_break {
            if curr_measure < seg.end as f32 {
                return Some((i, seg.end, false));
            }
            continue;
        }
        let (end, broken) = zmod_broken_run_end(segs, i);
        if curr_measure < end as f32 {
            return Some((i, end, broken));
        }
    }
    None
}

fn zmod_run_timer_index(segs: &[StreamSegment], curr_measure: f32) -> Option<usize> {
    let i = stream_segment_index_inclusive_end(segs, curr_measure);
    if i < segs.len() { Some(i) } else { None }
}

#[inline(always)]
fn zmod_run_timer_fmt(seconds: i32, minute_threshold: i32, trailing_space: bool) -> Arc<str> {
    cached_run_timer(seconds, minute_threshold, trailing_space)
}

#[inline(always)]
fn zmod_small_combo_font(combo_font: profile::ComboFont) -> &'static str {
    match combo_font {
        profile::ComboFont::Wendy | profile::ComboFont::WendyCursed => "wendy",
        profile::ComboFont::ArialRounded => "combo_arial_rounded",
        profile::ComboFont::Asap => "combo_asap",
        profile::ComboFont::BebasNeue => "combo_bebas_neue",
        profile::ComboFont::SourceCode => "combo_source_code",
        profile::ComboFont::Work => "combo_work",
        profile::ComboFont::None => "wendy",
    }
}

#[inline(always)]
fn zmod_combo_font_name(combo_font: profile::ComboFont) -> Option<&'static str> {
    match combo_font {
        profile::ComboFont::Wendy => Some("wendy_combo"),
        profile::ComboFont::ArialRounded => Some("combo_arial_rounded"),
        profile::ComboFont::Asap => Some("combo_asap"),
        profile::ComboFont::BebasNeue => Some("combo_bebas_neue"),
        profile::ComboFont::SourceCode => Some("combo_source_code"),
        profile::ComboFont::Work => Some("combo_work"),
        profile::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
        profile::ComboFont::None => None,
    }
}

#[inline(always)]
fn zmod_combo_quint_active(state: &State, player_idx: usize, profile: &profile::Profile) -> bool {
    if !profile.show_fa_plus_window || player_idx >= state.num_players {
        return false;
    }
    let counts = state.live_window_counts[player_idx];
    counts.w0 > 0
        && counts.w1 == 0
        && counts.w2 == 0
        && counts.w3 == 0
        && counts.w4 == 0
        && counts.w5 == 0
        && counts.miss == 0
}

#[inline(always)]
fn zmod_combo_glow_pair(grade: JudgeGrade, quint: bool) -> ([f32; 4], [f32; 4]) {
    if quint && matches!(grade, JudgeGrade::Fantastic) {
        return (color::rgba_hex("#F7C0FE"), color::rgba_hex("#E928FF"));
    }
    match grade {
        JudgeGrade::Fantastic => (color::rgba_hex("#C8FFFF"), color::rgba_hex("#6BF0FF")),
        JudgeGrade::Excellent => (color::rgba_hex("#FDFFC9"), color::rgba_hex("#FDDB85")),
        JudgeGrade::Great => (color::rgba_hex("#C9FFC9"), color::rgba_hex("#94FEC1")),
        _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
    }
}

#[inline(always)]
fn zmod_combo_solid_color(grade: JudgeGrade, quint: bool) -> [f32; 4] {
    if quint && matches!(grade, JudgeGrade::Fantastic) {
        return color::rgba_hex("#E928FF");
    }
    match grade {
        JudgeGrade::Fantastic => color::rgba_hex("#21CCE8"),
        JudgeGrade::Excellent => color::rgba_hex("#E29C18"),
        JudgeGrade::Great => color::rgba_hex("#66C955"),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

#[inline(always)]
fn zmod_combo_glow_color(color1: [f32; 4], color2: [f32; 4], elapsed: f32) -> [f32; 4] {
    let effect_period = 0.8_f32;
    let through = (elapsed / effect_period).fract();
    let anim_t = ((through * 2.0 * std::f32::consts::PI).sin() + 1.0) * 0.5;
    [
        color1[0] + (color2[0] - color1[0]) * anim_t,
        color1[1] + (color2[1] - color1[1]) * anim_t,
        color1[2] + (color2[2] - color1[2]) * anim_t,
        1.0,
    ]
}

#[inline(always)]
fn zmod_combo_rainbow_color(elapsed: f32, scroll: bool, combo: u32) -> [f32; 4] {
    let speed = if scroll { 0.45 } else { 0.35 };
    let offset = if scroll { combo as f32 * 0.013 } else { 0.0 };
    let hue = (elapsed * speed + offset).fract();
    let h6 = hue * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let q = 1.0 - f;
    match i.rem_euclid(6) {
        0 => [1.0, f, 0.0, 1.0],
        1 => [q, 1.0, 0.0, 1.0],
        2 => [0.0, 1.0, f, 1.0],
        3 => [0.0, q, 1.0, 1.0],
        4 => [f, 0.0, 1.0, 1.0],
        _ => [1.0, 0.0, q, 1.0],
    }
}

#[inline(always)]
fn scoring_count(p: &PlayerRuntime, grade: JudgeGrade) -> u32 {
    p.scoring_counts[crate::game::judgment::judge_grade_ix(grade)]
}

#[derive(Clone, Copy, Debug)]
struct MiniIndicatorProgress {
    kept_percent: f64,
    lost_percent: f64,
    pace_percent: f64,
    current_possible_dp: i32,
    possible_dp: i32,
    actual_dp: i32,
    w2: u32,
    w3: u32,
    w4: u32,
    w5: u32,
    miss: u32,
    let_go: u32,
    mines_hit: u32,
    judged_any: bool,
}

fn zmod_mini_indicator_progress(
    state: &State,
    p: &PlayerRuntime,
    player_idx: usize,
) -> MiniIndicatorProgress {
    let w1 = scoring_count(p, JudgeGrade::Fantastic);
    let w2 = scoring_count(p, JudgeGrade::Excellent);
    let w3 = scoring_count(p, JudgeGrade::Great);
    let w4 = scoring_count(p, JudgeGrade::Decent);
    let w5 = scoring_count(p, JudgeGrade::WayOff);
    let miss = scoring_count(p, JudgeGrade::Miss);

    let let_go = p
        .holds_let_go_for_score
        .saturating_add(p.rolls_let_go_for_score);
    let mines_hit = p.mines_hit_for_score;
    let tap_rows = w1
        .saturating_add(w2)
        .saturating_add(w3)
        .saturating_add(w4)
        .saturating_add(w5)
        .saturating_add(miss);
    let resolved_holds = p
        .holds_held_for_score
        .saturating_add(p.holds_let_go_for_score);
    let resolved_rolls = p
        .rolls_held_for_score
        .saturating_add(p.rolls_let_go_for_score);
    let current_possible_dp = (tap_rows
        .saturating_add(resolved_holds)
        .saturating_add(resolved_rolls) as i32)
        .saturating_mul(HOLD_SCORE_HELD);

    let possible_dp = state.possible_grade_points[player_idx].max(1);
    let actual_dp = p.earned_grade_points.max(0);
    let dp_lost = current_possible_dp.saturating_sub(actual_dp);
    let kept_dp = possible_dp.saturating_sub(dp_lost).max(0);
    let kept_percent = ((f64::from(kept_dp) / f64::from(possible_dp)) * 10000.0).floor() / 100.0;
    let lost_percent = (100.0 - kept_percent).max(0.0);
    let pace_percent = if current_possible_dp > 0 {
        ((f64::from(actual_dp) / f64::from(current_possible_dp)) * 10000.0).floor() / 100.0
    } else {
        0.0
    };
    let judged_any = tap_rows > 0 || let_go > 0 || mines_hit > 0 || p.is_failing || p.life <= 0.0;
    MiniIndicatorProgress {
        kept_percent,
        lost_percent,
        pace_percent,
        current_possible_dp,
        possible_dp,
        actual_dp,
        w2,
        w3,
        w4,
        w5,
        miss,
        let_go,
        mines_hit,
        judged_any,
    }
}

#[inline(always)]
fn zmod_indicator_mode(profile: &profile::Profile) -> profile::MiniIndicator {
    if profile.mini_indicator != profile::MiniIndicator::None {
        return profile.mini_indicator;
    }
    if profile.subtractive_scoring {
        profile::MiniIndicator::SubtractiveScoring
    } else if profile.pacemaker {
        profile::MiniIndicator::Pacemaker
    } else {
        profile::MiniIndicator::None
    }
}

#[inline(always)]
fn zmod_indicator_default_color(score_percent: f64) -> [f32; 4] {
    if score_percent >= 96.0 {
        color::rgba_hex("#21CCE8")
    } else if score_percent >= 89.0 {
        color::rgba_hex("#e29c18")
    } else if score_percent >= 80.0 {
        color::rgba_hex("#66c955")
    } else if score_percent >= 68.0 {
        color::rgba_hex("#b45cff")
    } else {
        [1.0, 0.0, 0.0, 1.0]
    }
}

#[inline(always)]
fn zmod_rival_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace)).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

#[inline(always)]
fn zmod_pacemaker_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace) / 100.0).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

fn zmod_stream_prog_completion(state: &State, player_idx: usize) -> Option<f64> {
    let total_stream = state.mini_indicator_total_stream_measures[player_idx] as f64;
    if total_stream <= 0.0 {
        return None;
    }
    let segs = &state.mini_indicator_stream_segments[player_idx];
    if segs.is_empty() {
        return None;
    }

    let beat_floor = state.current_beat_visible[player_idx].floor();
    if !beat_floor.is_finite() {
        return Some(0.0);
    }
    let upper_beat = (beat_floor as i32).saturating_add(1).max(0);
    if upper_beat <= 0 {
        return Some(0.0);
    }
    let mut completed_stream_beats: i64 = 0;
    for seg in segs {
        let start_beat = (seg.start as i32).saturating_mul(4);
        if start_beat >= upper_beat {
            break;
        }
        if seg.is_break {
            continue;
        }
        let end_beat = (seg.end as i32).saturating_mul(4);
        let lo = start_beat.max(0);
        let hi = upper_beat.min(end_beat);
        if hi > lo {
            completed_stream_beats += i64::from(hi - lo);
        }
    }
    let completed_stream_measures = (completed_stream_beats as f64) / 4.0;
    Some((completed_stream_measures / total_stream).clamp(0.0, 1.0))
}

fn zmod_mini_indicator_text(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile::Profile,
    player_idx: usize,
) -> Option<(Arc<str>, [f32; 4])> {
    let mode = zmod_indicator_mode(profile);
    if mode == profile::MiniIndicator::None {
        return None;
    }

    let progress = zmod_mini_indicator_progress(state, p, player_idx);
    if !progress.judged_any {
        return None;
    }

    match mode {
        profile::MiniIndicator::SubtractiveScoring => {
            let entered_percent_mode = progress.w3 > 0
                || progress.w4 > 0
                || progress.w5 > 0
                || progress.miss > 0
                || progress.let_go > 0
                || progress.mines_hit > 0
                || p.is_failing
                || p.life <= 0.0
                || progress.w2 > 10;
            if !entered_percent_mode && progress.w2 > 0 {
                return Some((cached_neg_int_u32(progress.w2), color::rgba_hex("#ff55cc")));
            }

            let score = progress.kept_percent.clamp(0.0, 100.0);
            Some((
                cached_signed_percent2_f64(progress.lost_percent.clamp(0.0, 100.0), true),
                zmod_indicator_default_color(score),
            ))
        }
        profile::MiniIndicator::PredictiveScoring => {
            let score = progress.kept_percent.clamp(0.0, 100.0);
            Some((
                cached_percent2_f64(score),
                zmod_indicator_default_color(score),
            ))
        }
        profile::MiniIndicator::PaceScoring => {
            let pace = progress.pace_percent.clamp(0.0, 100.0);
            Some((
                cached_percent2_f64(pace),
                zmod_indicator_default_color(pace),
            ))
        }
        profile::MiniIndicator::RivalScoring => {
            let possible = f64::from(progress.possible_dp.max(1));
            let current_possible = f64::from(progress.current_possible_dp.max(0));
            let actual = f64::from(progress.actual_dp.max(0));
            let pace = ((actual / possible) * 10000.0).floor() / 100.0;
            let rival_score =
                state.mini_indicator_rival_score_percent[player_idx].clamp(0.0, 100.0);
            let rival_pace =
                ((current_possible / possible) * 10000.0 * rival_score).floor() / 10000.0;
            let diff = (pace - rival_pace).abs();
            let text = cached_signed_percent2_f64(diff, pace < rival_pace);
            Some((text, zmod_rival_color(pace, rival_pace)))
        }
        profile::MiniIndicator::Pacemaker => {
            let possible = f64::from(progress.possible_dp.max(1));
            let current_possible = f64::from(progress.current_possible_dp.max(0));
            let actual = f64::from(progress.actual_dp.max(0));
            let pace = (actual / possible * 10000.0).floor();
            let target_ratio =
                (state.mini_indicator_target_score_percent[player_idx] / 100.0).clamp(0.0, 1.0);
            let rival_pace =
                ((current_possible / possible) * 1_000_000.0 * target_ratio).floor() / 100.0;

            let text = if pace < rival_pace {
                let diff = ((rival_pace - pace).floor() / 100.0).max(0.0);
                cached_signed_percent2_f64(diff, true)
            } else {
                let diff = ((pace - rival_pace).floor() / 100.0).max(0.0);
                cached_signed_percent2_f64(diff, false)
            };
            Some((text, zmod_pacemaker_color(pace, rival_pace)))
        }
        profile::MiniIndicator::StreamProg => {
            let completion = zmod_stream_prog_completion(state, player_idx)?;
            let rgba = if completion >= 0.9 {
                [
                    0.0,
                    1.0,
                    ((completion - 0.9) * 10.0).clamp(0.0, 1.0) as f32,
                    1.0,
                ]
            } else if completion >= 0.5 {
                [
                    ((0.9 - completion) * 10.0 / 4.0).clamp(0.0, 1.0) as f32,
                    1.0,
                    0.0,
                    1.0,
                ]
            } else {
                [
                    1.0,
                    ((completion - 0.2) * 10.0 / 3.0).clamp(0.0, 1.0) as f32,
                    0.0,
                    1.0,
                ]
            };
            Some((
                cached_percent2_f64((completion * 100.0).clamp(0.0, 100.0)),
                rgba,
            ))
        }
        profile::MiniIndicator::None => None,
    }
}

#[inline(always)]
fn rage_frustum(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32) -> Matrix4<f32> {
    let a = (r + l) / (r - l);
    let bb = (t + b) / (t - b);
    let c = -(zf + zn) / (zf - zn);
    let d = -(2.0 * zf * zn) / (zf - zn);
    // Match ITGmania's RageDisplay::GetFrustumMatrix (OpenGL-style frustum matrix).
    //
    // Note: cgmath::Matrix4::new takes elements in column-major order.
    Matrix4::new(
        // column 0
        2.0 * zn / (r - l),
        0.0,
        0.0,
        0.0,
        // column 1
        0.0,
        2.0 * zn / (t - b),
        0.0,
        0.0,
        // column 2
        a,
        bb,
        c,
        -1.0,
        // column 3
        0.0,
        0.0,
        d,
        0.0,
    )
}

fn notefield_view_proj(
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    center_y: f32,
    tilt: f32,
    skew: f32,
    reverse: bool,
) -> Option<Matrix4<f32>> {
    if !screen_w.is_finite() || !screen_h.is_finite() || screen_w <= 0.0 || screen_h <= 0.0 {
        return None;
    }

    let half_w = 0.5 * screen_w;
    let half_h = 0.5 * screen_h;

    // ITGmania: Player::PushPlayerMatrix -> LoadMenuPerspective(45, w, h, vanish_x, center_y)
    let fov_deg = 45.0_f32;
    let theta = (0.5 * fov_deg).to_radians();
    let tan_theta = theta.tan();
    if !tan_theta.is_finite() || tan_theta.abs() < 1e-6 {
        return None;
    }
    let dist = half_w / tan_theta;
    if !dist.is_finite() || dist <= 0.0 {
        return None;
    }

    let vanish_x = sm_scale(skew, 0.1, 1.0, playfield_center_x, half_w);
    let vanish_y = center_y;

    let near = 1.0_f32;
    let far = dist + 1000.0_f32;

    // Match RageDisplay::LoadMenuPerspective exactly (ITGmania).
    let mut vp_x = sm_scale(vanish_x, 0.0, screen_w, screen_w, 0.0);
    let mut vp_y = sm_scale(vanish_y, 0.0, screen_h, screen_h, 0.0);
    vp_x -= half_w;
    vp_y -= half_h;
    let l = (vp_x - half_w) / dist;
    let r = (vp_x + half_w) / dist;
    let b = (vp_y + half_h) / dist;
    let t = (vp_y - half_h) / dist;
    let proj = rage_frustum(l, r, b, t, near, far);

    let eye = Point3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Point3::new(-vp_x + half_w, -vp_y + half_h, 0.0);
    let view = Matrix4::look_at_rh(eye, at, Vector3::unit_y());

    // ITGmania: PlayerNoteFieldPositioner applies tilt/zoom/y_offset on the NoteField actor.
    let reverse_mult = if reverse { -1.0 } else { 1.0 };
    let tilt = tilt.clamp(-1.0, 1.0);
    let tilt_deg = (-30.0 * tilt) * reverse_mult;
    let tilt_abs = tilt.abs();
    let tilt_scale = 1.0 - 0.1 * tilt_abs;
    let y_offset_screen = if tilt > 0.0 {
        -45.0 * tilt
    } else {
        20.0 * tilt
    } * reverse_mult;
    // Screen y-down to world y-up.
    let y_offset_world = -y_offset_screen;

    let pivot_x = playfield_center_x - half_w;
    let pivot_y = half_h - center_y;
    // Convert our world coords (centered, y-up) back into the SM-style screen
    // coords (top-left, y-down) expected by the menu perspective camera.
    let world_to_screen = Matrix4::new(
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half_w, half_h, 0.0, 1.0,
    );
    let field = Matrix4::from_translation(Vector3::new(0.0, y_offset_world, 0.0))
        * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
        * Matrix4::from_angle_x(Deg(tilt_deg))
        * Matrix4::from_nonuniform_scale(tilt_scale, tilt_scale, 1.0)
        * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0));

    Some((proj * view) * world_to_screen * field)
}

pub fn build(
    state: &State,
    profile: &profile::Profile,
    placement: FieldPlacement,
) -> (Vec<Actor>, f32) {
    let mut actors = Vec::new();
    let mut hud_actors: Vec<Actor> = Vec::new();
    let mut model_cache = ModelMeshCache::with_capacity(96);
    let hold_judgment_texture: Option<&str> = match profile.hold_judgment_graphic {
        profile::HoldJudgmentGraphic::Love => Some("hold_judgements/Love 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::Mute => Some("hold_judgements/mute 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::ITG2 => Some("hold_judgements/ITG2 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::None => None,
    };

    // --- Playfield Positioning (1:1 with Simply Love) ---
    // In P2-only single-player, we still have a single player runtime (index 0),
    // but need to place the notefield on the P2 side of the screen.
    let player_idx = if state.num_players == 1 {
        0
    } else {
        match placement {
            FieldPlacement::P1 => 0,
            FieldPlacement::P2 => 1,
        }
    };
    if player_idx >= state.num_players {
        return (Vec::new(), screen_center_x());
    }
    // Use the cached field_zoom from gameplay state so visual layout and
    // scroll math share the exact same scaling as gameplay.
    let field_zoom = state.field_zoom[player_idx];
    let draw_distance_before_targets = state.draw_distance_before_targets[player_idx];
    let draw_distance_after_targets = state.draw_distance_after_targets[player_idx];
    let scroll_speed = effective_scroll_speed_for_player(state, player_idx);
    let col_start = player_idx * state.cols_per_player;
    let col_end = (col_start + state.cols_per_player)
        .min(state.num_cols)
        .min(MAX_COLS);
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return (Vec::new(), screen_center_x());
    }
    let p = &state.players[player_idx];

    // NoteFieldOffsetX is stored as a non-negative magnitude; for a single P1-style field,
    // apply the player-side sign flip used by Simply Love (P1=-, P2=+).
    let offset_sign = match placement {
        FieldPlacement::P1 => -1.0,
        FieldPlacement::P2 => 1.0,
    };
    let notefield_offset_x = offset_sign * (profile.note_field_offset_x.clamp(0, 50) as f32);
    let notefield_offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let play_style = profile::get_session_play_style();
    let center_1player = config::get().center_1player_notefield;
    let centered_one_side =
        state.num_players == 1 && play_style == profile::PlayStyle::Single && center_1player;
    let centered_both_sides = state.num_players == 1 && play_style == profile::PlayStyle::Double;
    let base_playfield_center_x = if state.num_players == 2 {
        match placement {
            FieldPlacement::P1 => screen_center_x() - (clamped_width * 0.25),
            FieldPlacement::P2 => screen_center_x() + (clamped_width * 0.25),
        }
    } else if centered_both_sides || centered_one_side {
        screen_center_x()
    } else {
        match placement {
            FieldPlacement::P1 => screen_center_x() - (clamped_width * 0.25),
            FieldPlacement::P2 => screen_center_x() + (clamped_width * 0.25),
        }
    };
    let playfield_center_x = base_playfield_center_x + notefield_offset_x;
    // Simply Love's GetNotefieldX helper reports base center for centered one-player fields,
    // ignoring NoteFieldOffsetX for layout decisions.
    let layout_center_x = if state.num_players == 1 && (centered_both_sides || centered_one_side) {
        screen_center_x()
    } else {
        playfield_center_x
    };
    let receptor_y_normal = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + notefield_offset_y;
    let receptor_y_reverse =
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + notefield_offset_y;

    let is_centered = profile
        .scroll_option
        .contains(profile::ScrollOption::Centered);
    let receptor_y_centered = screen_center_y() + notefield_offset_y;
    let column_dirs: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return 1.0;
        }
        state.column_scroll_dirs[col_start + i]
    });
    let column_receptor_ys: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return receptor_y_normal;
        }
        if is_centered {
            receptor_y_centered
        } else if column_dirs[i] >= 0.0 {
            receptor_y_normal
        } else {
            receptor_y_reverse
        }
    });

    let elapsed_screen = state.total_elapsed_in_screen;
    let visual_mask = effective_visual_mask_for_player(state, player_idx);
    let attack_mini_delta = effective_attack_mini_percent_delta_for_player(state, player_idx);
    let mini = effective_mini_value(profile, visual_mask, attack_mini_delta);
    let reverse_scroll = state.reverse_scroll[player_idx];
    let judgment_y = if is_centered {
        receptor_y_centered + 95.0
    } else if reverse_scroll {
        screen_center_y() + TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y
    } else {
        screen_center_y() - TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y
    };
    let combo_y_base = if is_centered {
        receptor_y_centered + 155.0
    } else if reverse_scroll {
        screen_center_y() - COMBO_OFFSET_FROM_CENTER + notefield_offset_y
    } else {
        screen_center_y() + COMBO_OFFSET_FROM_CENTER + notefield_offset_y
    };
    let zmod_layout = zmod_layout_ys(profile, judgment_y, combo_y_base, reverse_scroll);
    let mc_font_name = zmod_small_combo_font(profile.combo_font);
    // ITGmania Player::Update: min(pow(0.5, mini + tiny), 1.0); deadsync currently supports Mini.
    let judgment_zoom_mod = mini_judgment_zoom(mini);

    if let Some(ns) = &state.noteskin[player_idx] {
        let timing = &state.timing_players[player_idx];
        let target_arrow_px = TARGET_ARROW_PIXEL_SIZE * field_zoom;
        let scale_sprite = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || target_arrow_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_arrow_px / height;
                [width * scale, target_arrow_px]
            }
        };
        let scale_mine_slot = |slot: &SpriteSlot| -> [f32; 2] {
            // ITG NoteDisplay::DrawTap uses SetPRZForActor zoom for TapMine and does not
            // normalize Def.Model mine meshes to an arrow texture target size. Preserve
            // native model geometry scale here; keep sprite mines on texture-size scaling.
            if let Some(model) = slot.model.as_ref() {
                let model_size = model.size();
                if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
                    return [model_size[0] * field_zoom, model_size[1] * field_zoom];
                }
            }
            scale_sprite(slot.size())
        };
        let scale_cap = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if width <= 0.0 || target_arrow_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_arrow_px / width;
                [target_arrow_px, height * scale]
            }
        };
        let logical_slot_size = |slot: &SpriteSlot| -> [f32; 2] { slot.logical_size() };
        let scaled_note_slot_size = |slot: &SpriteSlot, note_scale: f32| -> [f32; 2] {
            if let Some(model) = slot.model.as_ref() {
                let model_size = model.size();
                if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
                    return [model_size[0] * note_scale, model_size[1] * note_scale];
                }
            }
            let logical = logical_slot_size(slot);
            [logical[0] * note_scale, logical[1] * note_scale]
        };
        let scale_explosion = |logical_size: [f32; 2]| -> [f32; 2] {
            [logical_size[0] * field_zoom, logical_size[1] * field_zoom]
        };
        let scale_hold_explosion = |slot: &SpriteSlot| -> [f32; 2] {
            // Match ITG ghost arrow behavior: hold/roll explosions use actor asset size
            // (including double-res handling) instead of being normalized to arrow size.
            let logical = logical_slot_size(slot);
            [logical[0] * field_zoom, logical[1] * field_zoom]
        };
        let current_time = state.current_music_time_visible[player_idx];
        let current_beat = state.current_beat_visible[player_idx];
        let confusion_receptor_rot = if (visual_mask & VISUAL_MASK_CONFUSION) != 0 {
            let beat = current_beat.rem_euclid(2.0 * std::f32::consts::PI);
            beat * (-180.0 / std::f32::consts::PI)
        } else {
            0.0
        };
        // ITG NoteField currently advances NoteDisplay resources twice per frame for
        // the master field (and once per additional field), so model/tween time in
        // NoteDisplay actors runs faster than wall-clock elapsed.
        let note_display_time_scale = state.num_players as f32 + 1.0;
        // Precompute per-frame values used for converting beat/time to Y positions
        let (rate, cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                    state.music_rate
                } else {
                    1.0
                };
                (rate, Some(pps), 0.0, 0.0)
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let curr_disp = timing.get_displayed_beat(state.current_beat_visible[player_idx]);
                let speed_multiplier = timing
                    .get_speed_multiplier(state.current_beat_visible[player_idx], current_time);
                let player_multiplier =
                    scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * speed_multiplier;
                (1.0, None, curr_disp, final_multiplier)
            }
        };
        let travel_offset_for_cached_note = |note_index: usize, use_hold_end: bool| -> f32 {
            match scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    let note_time = if use_hold_end {
                        state.hold_end_time_cache[note_index]
                            .unwrap_or(state.note_time_cache[note_index])
                    } else {
                        state.note_time_cache[note_index]
                    };
                    let time_diff_real = (note_time - current_time) / rate;
                    time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp = if use_hold_end {
                        state.hold_end_display_beat_cache[note_index]
                            .unwrap_or(state.note_display_beat_cache[note_index])
                    } else {
                        state.note_display_beat_cache[note_index]
                    };
                    let beat_diff_disp = note_disp - curr_disp_beat;
                    beat_diff_disp
                        * ScrollSpeedSetting::ARROW_SPACING
                        * field_zoom
                        * beatmod_multiplier
                }
            }
        };
        let adjusted_travel_offset = |travel_offset: f32| -> f32 { travel_offset };
        let lane_y_from_travel =
            |local_col: usize, receptor_y_lane: f32, dir: f32, travel_offset: f32| -> f32 {
                let dir = if dir >= 0.0 { 1.0 } else { -1.0 };
                let _ = local_col;
                receptor_y_lane + dir * adjusted_travel_offset(travel_offset)
            };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat.
        // Direction and receptor row are per-lane: upwards lanes anchor to the normal receptor row,
        // downwards lanes anchor to the reverse row.
        let compute_lane_y_dynamic =
            |local_col: usize, beat: f32, receptor_y_lane: f32, dir: f32| -> f32 {
                let travel_offset = match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = timing.get_time_for_beat(beat);
                        let time_diff_real = (note_time_chart - current_time) / rate;
                        time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = timing.get_displayed_beat(beat);
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        beat_diff_disp
                            * ScrollSpeedSetting::ARROW_SPACING
                            * field_zoom
                            * beatmod_multiplier
                    }
                };
                lane_y_from_travel(local_col, receptor_y_lane, dir, travel_offset)
            };
        // Measure Lines (Zmod parity: NoteField:SetBeatBarsAlpha)
        if !matches!(
            profile.measure_lines,
            crate::game::profile::MeasureLines::Off
        ) {
            let (alpha_measure, alpha_quarter, alpha_eighth) = match profile.measure_lines {
                crate::game::profile::MeasureLines::Off => (0.0, 0.0, 0.0),
                crate::game::profile::MeasureLines::Measure => (0.75, 0.0, 0.0),
                crate::game::profile::MeasureLines::Quarter => (0.75, 0.5, 0.0),
                crate::game::profile::MeasureLines::Eighth => (0.75, 0.5, 0.125),
            };

            let mut pos_min_x: f32 = f32::INFINITY;
            let mut pos_max_x: f32 = f32::NEG_INFINITY;
            let mut pos_receptor_y: f32 = 0.0;
            let mut pos_any = false;

            let mut neg_min_x: f32 = f32::INFINITY;
            let mut neg_max_x: f32 = f32::NEG_INFINITY;
            let mut neg_receptor_y: f32 = 0.0;
            let mut neg_any = false;

            for i in 0..num_cols {
                let x = ns.column_xs[i] as f32;
                if column_dirs[i] >= 0.0 {
                    if !pos_any {
                        pos_any = true;
                        pos_receptor_y = column_receptor_ys[i];
                        pos_min_x = x;
                        pos_max_x = x;
                    } else {
                        pos_min_x = pos_min_x.min(x);
                        pos_max_x = pos_max_x.max(x);
                    }
                } else if !neg_any {
                    neg_any = true;
                    neg_receptor_y = column_receptor_ys[i];
                    neg_min_x = x;
                    neg_max_x = x;
                } else {
                    neg_min_x = neg_min_x.min(x);
                    neg_max_x = neg_max_x.max(x);
                }
            }

            let beat_units_start = (current_beat * 2.0).floor() as i64;
            let thickness = (2.0 * field_zoom).max(1.0);
            let y_min = -400.0;
            let y_max = screen_height() + 400.0;

            let mut draw_group = |min_x: f32, max_x: f32, receptor_y: f32, dir: f32| {
                let center_x_offset = 0.5 * (min_x + max_x) * field_zoom;
                let w = ((max_x - min_x) + ScrollSpeedSetting::ARROW_SPACING) * field_zoom;
                if !w.is_finite() || w <= 0.0 {
                    return;
                }

                let x_center = playfield_center_x + center_x_offset;

                // Walk backward from current beat.
                let mut u = beat_units_start;
                let mut iters = 0;
                while iters < 2000 {
                    let alpha = if u.rem_euclid(8) == 0 {
                        alpha_measure
                    } else if u.rem_euclid(2) == 0 {
                        alpha_quarter
                    } else {
                        alpha_eighth
                    };

                    let beat = (u as f32) * 0.5;
                    let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y < y_min) || (dir < 0.0 && y > y_max) {
                        break;
                    }
                    if alpha > 0.0 && y >= y_min && y <= y_max {
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x_center, y):
                            zoomto(w, thickness):
                            diffuse(1.0, 1.0, 1.0, alpha):
                            z(Z_MEASURE_LINES)
                        ));
                    }
                    u -= 1;
                    iters += 1;
                }

                // Walk forward from next half-beat to avoid duplicating the start line.
                let mut u = beat_units_start + 1;
                let mut iters = 0;
                while iters < 2000 {
                    let alpha = if u.rem_euclid(8) == 0 {
                        alpha_measure
                    } else if u.rem_euclid(2) == 0 {
                        alpha_quarter
                    } else {
                        alpha_eighth
                    };

                    let beat = (u as f32) * 0.5;
                    let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y > y_max) || (dir < 0.0 && y < y_min) {
                        break;
                    }
                    if alpha > 0.0 && y >= y_min && y <= y_max {
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x_center, y):
                            zoomto(w, thickness):
                            diffuse(1.0, 1.0, 1.0, alpha):
                            z(Z_MEASURE_LINES)
                        ));
                    }
                    u += 1;
                    iters += 1;
                }
            };

            if pos_any {
                draw_group(pos_min_x, pos_max_x, pos_receptor_y, 1.0);
            }
            if neg_any {
                draw_group(neg_min_x, neg_max_x, neg_receptor_y, -1.0);
            }
        }

        if profile.column_cues {
            let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                state.music_rate
            } else {
                1.0
            };
            let current_time = state.current_music_time_visible[player_idx];
            if let Some(cue) = active_column_cue(&state.column_cues[player_idx], current_time) {
                let duration_real = cue.duration / rate;
                let elapsed_real = (current_time - cue.start_time) / rate;
                let alpha_mul = column_cue_alpha(elapsed_real, duration_real);
                if alpha_mul > 0.0 {
                    let lane_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                    let cue_height = (screen_height() - COLUMN_CUE_Y_OFFSET).max(0.0);
                    let mut countdown_text: Option<(f32, f32, i32)> = None;

                    if duration_real >= 5.0 {
                        let remaining = duration_real - elapsed_real;
                        if remaining > 0.5
                            && let Some(last_col) = cue.columns.last()
                        {
                            let local_col = last_col.column.saturating_sub(col_start);
                            if local_col < num_cols {
                                let x = playfield_center_x
                                    + ns.column_xs[local_col] as f32 * field_zoom;
                                let y = if column_dirs[local_col] < 0.0 {
                                    COLUMN_CUE_TEXT_REVERSE_Y + notefield_offset_y
                                } else {
                                    COLUMN_CUE_TEXT_NORMAL_Y + notefield_offset_y
                                };
                                countdown_text = Some((x, y, remaining.round() as i32));
                            }
                        }
                    }

                    for col_cue in &cue.columns {
                        let local_col = col_cue.column.saturating_sub(col_start);
                        if local_col >= num_cols {
                            continue;
                        }
                        let x = playfield_center_x + ns.column_xs[local_col] as f32 * field_zoom;
                        let alpha = COLUMN_CUE_BASE_ALPHA * alpha_mul;
                        let color = if col_cue.is_mine {
                            [1.0, 0.0, 0.0, alpha]
                        } else {
                            [0.3, 1.0, 1.0, alpha]
                        };
                        if column_dirs[local_col] < 0.0 {
                            let reverse_y = COLUMN_CUE_Y_OFFSET * 2.0
                                + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE
                                + lane_width * 0.5
                                + notefield_offset_y;
                            actors.push(act!(quad:
                                align(0.5, 0.0):
                                xy(x, reverse_y):
                                zoomto(lane_width, cue_height):
                                fadebottom(0.333):
                                rotationz(180):
                                diffuse(color[0], color[1], color[2], color[3]):
                                z(Z_COLUMN_CUE)
                            ));
                        } else {
                            actors.push(act!(quad:
                                align(0.5, 0.0):
                                xy(x, COLUMN_CUE_Y_OFFSET + notefield_offset_y):
                                zoomto(lane_width, cue_height):
                                fadebottom(0.333):
                                diffuse(color[0], color[1], color[2], color[3]):
                                z(Z_COLUMN_CUE)
                            ));
                        }
                    }

                    if let Some((x, y, value)) = countdown_text {
                        hud_actors.push(act!(text:
                            font(mc_font_name):
                            settext(value.to_string()):
                            align(0.5, 0.5):
                            xy(x, y):
                            zoom(0.5):
                            z(200):
                            diffuse(1.0, 1.0, 1.0, alpha_mul)
                        ));
                    }
                }
            }
        }

        // Receptors + glow
        for i in 0..num_cols {
            let col = col_start + i;
            let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
            let receptor_y_lane = column_receptor_ys[i];
            if !profile.hide_targets {
                let bop_timer = state.receptor_bop_timers[col];
                let bop_zoom = if bop_timer > 0.0 {
                    let t = (0.11 - bop_timer) / 0.11;
                    0.75 + (1.0 - 0.75) * t
                } else {
                    1.0
                };
                let receptor_slot = &ns.receptor_off[i];
                let receptor_frame =
                    receptor_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                let receptor_uv =
                    receptor_slot.uv_for_frame_at(receptor_frame, state.total_elapsed_in_screen);
                // ITG Sprite::SetTexture uses source-frame dimensions for draw size,
                // so receptor and overlay keep their authored ratio (e.g. 64 vs 74 in
                // dance/default) instead of being normalized to arrow height.
                let receptor_size = scale_explosion(logical_slot_size(receptor_slot));
                let receptor_color = ns.receptor_pulse.color_for_beat(current_beat);
                actors.push(act!(sprite(receptor_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset, receptor_y_lane):
                    setsize(receptor_size[0], receptor_size[1]):
                    zoom(bop_zoom):
                    diffuse(
                        receptor_color[0],
                        receptor_color[1],
                        receptor_color[2],
                        receptor_color[3]
                    ):
                    rotationz(-receptor_slot.def.rotation_deg as f32 + confusion_receptor_rot):
                    customtexturerect(
                        receptor_uv[0],
                        receptor_uv[1],
                        receptor_uv[2],
                        receptor_uv[3]
                    ):
                    z(Z_RECEPTOR)
                ));
            }
            let hold_slot = if let Some(active) = state.active_holds[col]
                .as_ref()
                .filter(|active| active_hold_is_engaged(active))
            {
                let note_type = &state.notes[active.note_index].note_type;
                let visuals = ns.hold_visuals_for_col(i, matches!(note_type, NoteType::Roll));
                if let Some(slot) = visuals.explosion.as_ref() {
                    Some(slot)
                } else if let Some(slot) = ns.hold.explosion.as_ref() {
                    Some(slot)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(hold_slot) = hold_slot {
                let draw = hold_slot.model_draw_at(state.total_elapsed_in_screen, current_beat);
                let hold_frame = hold_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                let hold_uv = hold_slot.uv_for_frame_at(hold_frame, state.total_elapsed_in_screen);
                let base_size = scale_hold_explosion(hold_slot);
                let hold_size = [
                    base_size[0] * draw.zoom[0].max(0.0),
                    base_size[1] * draw.zoom[1].max(0.0),
                ];
                if hold_size[0] <= f32::EPSILON || hold_size[1] <= f32::EPSILON {
                    continue;
                }
                let receptor_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let base_rotation = hold_slot.def.rotation_deg as f32;
                let final_rotation =
                    base_rotation + receptor_rotation - draw.rot[2] - confusion_receptor_rot;
                let center = [playfield_center_x + col_x_offset, receptor_y_lane];
                let color = draw.tint;
                let glow =
                    hold_slot.model_glow_at(state.total_elapsed_in_screen, current_beat, color[3]);
                let blend = if draw.blend_add {
                    BlendMode::Add
                } else {
                    BlendMode::Alpha
                };
                if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                    hold_slot,
                    draw,
                    center,
                    hold_size,
                    hold_uv,
                    -final_rotation,
                    color,
                    blend,
                    Z_HOLD_EXPLOSION as i16,
                    &mut model_cache,
                ) {
                    actors.push(model_actor);
                    if let Some(glow_color) = glow
                        && let Some(glow_actor) = noteskin_model_actor_from_draw_cached(
                            hold_slot,
                            draw,
                            center,
                            hold_size,
                            hold_uv,
                            -final_rotation,
                            glow_color,
                            BlendMode::Add,
                            Z_HOLD_EXPLOSION as i16,
                            &mut model_cache,
                        )
                    {
                        actors.push(glow_actor);
                    }
                } else if draw.blend_add {
                    actors.push(act!(sprite(hold_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(center[0], center[1]):
                        setsize(hold_size[0], hold_size[1]):
                        rotationz(-final_rotation):
                        customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                        diffuse(color[0], color[1], color[2], color[3]):
                        blend(add):
                        z(Z_HOLD_EXPLOSION)
                    ));
                    if let Some(glow_color) = glow {
                        actors.push(act!(sprite(hold_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(center[0], center[1]):
                            setsize(hold_size[0], hold_size[1]):
                            rotationz(-final_rotation):
                            customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                            diffuse(glow_color[0], glow_color[1], glow_color[2], glow_color[3]):
                            blend(add):
                            z(Z_HOLD_EXPLOSION)
                        ));
                    }
                } else {
                    actors.push(act!(sprite(hold_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(center[0], center[1]):
                        setsize(hold_size[0], hold_size[1]):
                        rotationz(-final_rotation):
                        customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                        diffuse(color[0], color[1], color[2], color[3]):
                        blend(normal):
                        z(Z_HOLD_EXPLOSION)
                    ));
                    if let Some(glow_color) = glow {
                        actors.push(act!(sprite(hold_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(center[0], center[1]):
                            setsize(hold_size[0], hold_size[1]):
                            rotationz(-final_rotation):
                            customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                            diffuse(glow_color[0], glow_color[1], glow_color[2], glow_color[3]):
                            blend(add):
                            z(Z_HOLD_EXPLOSION)
                        ));
                    }
                }
            }
            if !profile.hide_targets {
                if let Some((alpha, zoom)) = receptor_glow_visual_for_col(state, col)
                    && let Some(glow_slot) = ns.receptor_glow.get(i).and_then(|slot| slot.as_ref())
                {
                    let glow_frame =
                        glow_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                    let glow_uv =
                        glow_slot.uv_for_frame_at(glow_frame, state.total_elapsed_in_screen);
                    let glow_size = scale_explosion(logical_slot_size(glow_slot));
                    let behavior = ns.receptor_glow_behavior;
                    if alpha > f32::EPSILON {
                        let width = glow_size[0] * zoom;
                        let height = glow_size[1] * zoom;
                        if behavior.blend_add {
                            actors.push(act!(sprite(glow_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset, receptor_y_lane):
                                setsize(width, height):
                                rotationz(-glow_slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(add):
                                z(Z_HOLD_GLOW)
                            ));
                        } else {
                            actors.push(act!(sprite(glow_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset, receptor_y_lane):
                                setsize(width, height):
                                rotationz(-glow_slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(normal):
                                z(Z_HOLD_GLOW)
                            ));
                        }
                    }
                }
            }
        }
        // Tap explosions
        if !profile.hide_combo_explosions {
            for i in 0..num_cols {
                let col = col_start + i;
                if let Some(active) = state.tap_explosions[col].as_ref()
                    && let Some(explosion) = ns.tap_explosions.get(&active.window)
                {
                    let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
                    let receptor_y_lane = column_receptor_ys[i];
                    let anim_time = active.elapsed;
                    let slot = &explosion.slot;
                    let beat_for_anim = if slot.source.is_beat_based() {
                        (state.current_beat - active.start_beat).max(0.0)
                    } else {
                        state.current_beat
                    };
                    let frame = slot.frame_index(anim_time, beat_for_anim);
                    let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen);
                    let size = scale_explosion(logical_slot_size(slot));
                    let visual = explosion.animation.state_at(active.elapsed);
                    if !visual.visible {
                        continue;
                    }
                    let rotation_deg = ns
                        .receptor_off
                        .get(i)
                        .map(|slot| slot.def.rotation_deg)
                        .unwrap_or(0);
                    actors.push(act!(sprite(slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset, receptor_y_lane):
                        setsize(size[0], size[1]):
                        zoom(visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        diffuse(
                            visual.diffuse[0],
                            visual.diffuse[1],
                            visual.diffuse[2],
                            visual.diffuse[3]
                        ):
                        rotationz(-(rotation_deg as f32) + confusion_receptor_rot):
                        blend(normal):
                        z(Z_TAP_EXPLOSION)
                    ));
                    let glow = visual.glow;
                    let glow_strength =
                        glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset, receptor_y_lane):
                            setsize(size[0], size[1]):
                            zoom(visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            rotationz(-(rotation_deg as f32) + confusion_receptor_rot):
                            blend(add):
                            z(Z_TAP_EXPLOSION)
                        ));
                    }
                }
            }
        }
        // Mine explosions
        for i in 0..num_cols {
            let col = col_start + i;
            let Some(active) = state.mine_explosions[col].as_ref() else {
                continue;
            };
            let Some(explosion) = ns.mine_hit_explosion.as_ref() else {
                continue;
            };
            let slot = &explosion.slot;
            let visual = explosion.animation.state_at(active.elapsed);
            if !visual.visible {
                continue;
            }
            let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
            let receptor_y_lane = column_receptor_ys[i];
            let frame = slot.frame_index(active.elapsed, current_beat);
            let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen);
            let size = scale_explosion(logical_slot_size(slot));
            actors.push(act!(sprite(slot.texture_key().to_string()):
                align(0.5, 0.5):
                xy(playfield_center_x + col_x_offset, receptor_y_lane):
                setsize(size[0], size[1]):
                zoom(visual.zoom):
                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                rotationz(-visual.rotation_z):
                diffuse(
                    visual.diffuse[0],
                    visual.diffuse[1],
                    visual.diffuse[2],
                    visual.diffuse[3]
                ):
                blend(add):
                z(Z_MINE_EXPLOSION)
            ));
            let glow = visual.glow;
            let glow_strength = glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
            if glow_strength > f32::EPSILON {
                actors.push(act!(sprite(slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset, receptor_y_lane):
                    setsize(size[0], size[1]):
                    zoom(visual.zoom):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    rotationz(-visual.rotation_z):
                    diffuse(glow[0], glow[1], glow[2], glow[3]):
                    blend(add):
                    z(Z_MINE_EXPLOSION)
                ));
            }
        }
        // Only consider notes that are currently in or near the lookahead window.
        let notes_len = state.notes.len();
        let (note_start, note_end) = state.note_ranges[player_idx];
        let min_visible_index = state.arrows[col_start..col_end]
            .iter()
            .filter_map(|v| v.first())
            .map(|a| a.note_index)
            .min()
            .unwrap_or(note_start);
        let max_visible_index = state.note_spawn_cursor[player_idx]
            .clamp(note_start, note_end)
            .min(notes_len);
        let extra_hold_indices = state
            .active_holds
            .iter()
            .filter_map(|a| a.as_ref().map(|h| h.note_index))
            .chain(state.decaying_hold_indices.iter().copied())
            .filter(|&idx| {
                idx >= note_start
                    && idx < note_end
                    && (idx < min_visible_index || idx >= max_visible_index)
            });

        // Render holds in the visible window, plus any active/decaying holds outside it.
        // This avoids per-frame allocations and hashing for deduping.
        for note_index in (min_visible_index..max_visible_index).chain(extra_hold_indices) {
            let note = &state.notes[note_index];
            if note.column < col_start || note.column >= col_end {
                continue;
            }
            let local_col = note.column - col_start;
            if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let Some(hold) = &note.hold else {
                continue;
            };
            if matches!(hold.result, Some(HoldResult::Held)) {
                continue;
            }

            // Prepare static/dynamic Y positions for the hold body
            // Head Y: dynamic if actively held or let go, otherwise static cache
            let mut head_beat = note.beat;
            let is_head_dynamic =
                hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo);

            if is_head_dynamic {
                head_beat = hold.last_held_beat.clamp(note.beat, hold.end_beat);
            }

            let col_dir = column_dirs[local_col];
            let dir = col_dir;
            let lane_receptor_y = column_receptor_ys[local_col];

            let head_travel_offset = if is_head_dynamic {
                match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = timing.get_time_for_beat(head_beat);
                        (note_time_chart - current_time) / rate * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = timing.get_displayed_beat(head_beat);
                        (note_disp_beat - curr_disp_beat)
                            * ScrollSpeedSetting::ARROW_SPACING
                            * field_zoom
                            * beatmod_multiplier
                    }
                }
            } else {
                travel_offset_for_cached_note(note_index, false)
            };
            let tail_travel_offset = travel_offset_for_cached_note(note_index, true);
            let head_y = lane_y_from_travel(local_col, lane_receptor_y, dir, head_travel_offset);
            let tail_y = lane_y_from_travel(local_col, lane_receptor_y, dir, tail_travel_offset);
            let note_display = ns.note_display_metrics;
            let visual_reverse = head_y > tail_y;
            let body_flipped = visual_reverse && note_display.flip_hold_body_when_reverse;
            let (body_head_y, body_tail_y) = if body_flipped {
                (
                    head_y - note_display.stop_drawing_hold_body_offset_from_tail,
                    tail_y - note_display.start_drawing_hold_body_offset_from_head,
                )
            } else {
                (
                    head_y + note_display.start_drawing_hold_body_offset_from_head,
                    tail_y + note_display.stop_drawing_hold_body_offset_from_tail,
                )
            };
            let head_is_top = body_head_y <= body_tail_y;
            let mut top = body_head_y.min(body_tail_y);
            let mut bottom = body_head_y.max(body_tail_y);
            if bottom < -200.0 || top > screen_height() + 200.0 {
                continue;
            }
            top = top.max(-400.0);
            bottom = bottom.min(screen_height() + 400.0);
            if bottom <= top {
                continue;
            }
            let col_x_offset = ns.column_xs[local_col] as f32 * field_zoom;

            let active_state = state.active_holds[note.column]
                .as_ref()
                .filter(|h| h.note_index == note_index);
            let engaged = active_state.map(active_hold_is_engaged).unwrap_or(false);
            let use_active = active_state
                .map(|h| h.is_pressed && !h.let_go)
                .unwrap_or(false);
            let let_go_gray = ns.hold_let_go_gray_percent.clamp(0.0, 1.0);
            let hold_life = hold.life.clamp(0.0, 1.0);
            let hold_color_scale = let_go_gray + (1.0 - let_go_gray) * hold_life;
            let hold_diffuse = [hold_color_scale, hold_color_scale, hold_color_scale, 1.0];
            let use_tail_for_head_anchor =
                visual_reverse && !note_display.flip_head_and_tail_when_reverse;
            let head_anchor_y = if use_tail_for_head_anchor {
                tail_y
            } else {
                head_y
            };
            if engaged {
                if head_is_top {
                    top = top.max(lane_receptor_y);
                } else {
                    bottom = bottom.min(lane_receptor_y);
                }
            }
            if bottom <= top {
                continue;
            }
            let visuals =
                ns.hold_visuals_for_col(local_col, matches!(note.note_type, NoteType::Roll));
            let hold_head_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollHead
            } else {
                NoteAnimPart::HoldHead
            };
            let hold_body_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollBody
            } else {
                NoteAnimPart::HoldBody
            };
            let hold_bottomcap_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollBottomCap
            } else {
                NoteAnimPart::HoldBottomCap
            };
            let hold_part_phase = ns.part_uv_phase(
                hold_head_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let hold_body_phase = ns.part_uv_phase(
                hold_body_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let hold_cap_phase = ns.part_uv_phase(
                hold_bottomcap_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let tail_slot = if use_active {
                visuals
                    .bottomcap_active
                    .as_ref()
                    .or(visuals.bottomcap_inactive.as_ref())
            } else {
                visuals
                    .bottomcap_inactive
                    .as_ref()
                    .or(visuals.bottomcap_active.as_ref())
            };
            // Prepare clipped body extents that respect the tail cap on the side
            // where the tail visually exists. For normal orientation (head above
            // tail), we clip the body against the tail cap at the bottom. For
            // reverse orientation (head below tail), we clip the body against the
            // tail cap at the top.
            let mut body_top = top;
            let mut body_bottom = bottom;
            if let Some(cap_slot) = tail_slot {
                let cap_size = scale_cap(cap_slot.size());
                let cap_height = cap_size[1];
                if cap_height > f32::EPSILON {
                    // ITGmania joins hold body to cap at the tail edge (with a tiny overlap),
                    // not at the cap midpoint. Keep the body clipped to that join line.
                    if head_is_top {
                        body_bottom = body_bottom.min(body_tail_y + 1.0);
                        if body_bottom >= body_tail_y - 1.0 {
                            body_bottom = body_tail_y + 1.0;
                        }
                    } else {
                        body_top = body_top.max(body_tail_y - 1.0);
                        if body_top <= body_tail_y + 1.0 {
                            body_top = body_tail_y - 1.0;
                        }
                    }
                }
            }
            // Track the actual drawn body extents to decide whether the tail cap
            // should be rendered (prevents floating caps when no body segments were drawn).
            let mut rendered_body_top: Option<f32> = None;
            let mut rendered_body_bottom: Option<f32> = None;
            if body_bottom > body_top
                && let Some(body_slot) = if use_active {
                    visuals
                        .body_active
                        .as_ref()
                        .or(visuals.body_inactive.as_ref())
                } else {
                    visuals
                        .body_inactive
                        .as_ref()
                        .or(visuals.body_active.as_ref())
                }
            {
                let texture_size = body_slot.size();
                let texture_width = texture_size[0].max(1) as f32;
                let texture_height = texture_size[1].max(1) as f32;
                if texture_width > f32::EPSILON && texture_height > f32::EPSILON {
                    let body_frame = body_slot.frame_index_from_phase(hold_body_phase);
                    let body_width = TARGET_ARROW_PIXEL_SIZE * field_zoom;
                    let scale = body_width / texture_width;
                    let segment_height = (texture_height * scale).max(f32::EPSILON);
                    let body_uv_elapsed = if body_slot.model.is_some() {
                        hold_body_phase
                    } else {
                        state.total_elapsed_in_screen
                    };
                    let body_uv = translated_uv_rect(
                        body_slot.uv_for_frame_at(body_frame, body_uv_elapsed),
                        ns.part_uv_translation(hold_body_part, note.beat, false),
                    );
                    let u0 = body_uv[0];
                    let u1 = body_uv[2];
                    let v_top = body_uv[1];
                    let v_bottom = body_uv[3];
                    let v_range = v_bottom - v_top;
                    let natural_top = if head_is_top {
                        body_head_y
                    } else {
                        body_tail_y
                    };
                    let natural_bottom = if head_is_top {
                        body_tail_y
                    } else {
                        body_head_y
                    };
                    let hold_length = (natural_bottom - natural_top).abs();
                    const SEGMENT_PHASE_EPS: f32 = 1e-4;
                    let max_segments = 2048;
                    let receptor = lane_receptor_y;

                    // Unified segmentation path for both normal and reverse scroll.
                    // For reverse scroll, we work in "forward space" by mirroring coordinates,
                    // run the same segmentation logic, then mirror back to screen space.

                    // Transform to "forward space" if reverse scroll (mirror around receptor)
                    let (eff_head_y, eff_tail_y, eff_body_top, eff_body_bottom) = if visual_reverse
                    {
                        (
                            2.0 * receptor - body_head_y,
                            2.0 * receptor - body_tail_y,
                            2.0 * receptor - body_bottom,
                            2.0 * receptor - body_top,
                        )
                    } else {
                        (body_head_y, body_tail_y, body_top, body_bottom)
                    };

                    let eff_head_is_top = eff_head_y <= eff_tail_y;
                    let eff_natural_top = if eff_head_is_top {
                        eff_head_y
                    } else {
                        eff_tail_y
                    };
                    let eff_natural_bottom = if eff_head_is_top {
                        eff_tail_y
                    } else {
                        eff_head_y
                    };

                    // Skip if hold has no effective length
                    if hold_length > f32::EPSILON {
                        // Calculate visible distances in forward space
                        let visible_top_distance = if eff_head_is_top {
                            (eff_body_top - eff_natural_top).clamp(0.0, hold_length)
                        } else {
                            (eff_natural_bottom - eff_body_top).clamp(0.0, hold_length)
                        };
                        let visible_bottom_distance = if eff_head_is_top {
                            (eff_body_bottom - eff_natural_top).clamp(0.0, hold_length)
                        } else {
                            (eff_natural_bottom - eff_body_bottom).clamp(0.0, hold_length)
                        };

                        // Phase offset: shifts fractional remainder to first segment so the
                        // final segment aligns with the tail cap. Only applies when head is on top.
                        let anchor_to_top =
                            visual_reverse && note_display.top_hold_anchor_when_reverse;
                        let phase_offset = if eff_head_is_top && !anchor_to_top {
                            let total_phase = hold_length / segment_height;
                            if total_phase >= 1.0 + SEGMENT_PHASE_EPS {
                                let fractional = total_phase.fract();
                                if fractional > SEGMENT_PHASE_EPS
                                    && (1.0 - fractional) > SEGMENT_PHASE_EPS
                                {
                                    1.0 - fractional
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };

                        let mut phase = visible_top_distance / segment_height + phase_offset;
                        let phase_end_adjusted =
                            visible_bottom_distance / segment_height + phase_offset;
                        let mut emitted = 0;

                        while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                            && emitted < max_segments
                        {
                            let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                            if next_phase - phase < SEGMENT_PHASE_EPS {
                                next_phase = phase_end_adjusted;
                            }
                            if next_phase - phase < SEGMENT_PHASE_EPS {
                                break;
                            }

                            let distance_start = (phase - phase_offset) * segment_height;
                            let distance_end = (next_phase - phase_offset) * segment_height;
                            let y_start = eff_natural_top + distance_start;
                            let y_end = eff_natural_top + distance_end;
                            let segment_top_eff = y_start.max(eff_body_top);
                            let segment_bottom_eff = y_end.min(eff_body_bottom);

                            if segment_bottom_eff - segment_top_eff <= f32::EPSILON {
                                phase = next_phase;
                                continue;
                            }

                            // UV calculations
                            let base_floor = phase.floor();
                            let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                            let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                            let mut v0 = v_top + v_range * start_fraction;
                            let mut v1 = v_top + v_range * end_fraction;

                            let segment_size_eff = segment_bottom_eff - segment_top_eff;
                            let portion = (segment_size_eff / segment_height).clamp(0.0, 1.0);

                            // Tail UV snapping should only happen when this drawn chunk
                            // actually reaches the tail side. If the tail is offscreen,
                            // snapping the viewport cut to v_bottom creates a darker seam.
                            let tail_gap = (eff_natural_bottom - eff_body_bottom).max(0.0);
                            let body_reaches_tail =
                                eff_head_is_top && tail_gap <= segment_height + 1.0;
                            let is_last_visible_segment =
                                (eff_body_bottom - segment_bottom_eff).abs() <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                            if body_reaches_tail && is_last_visible_segment {
                                if v_range >= 0.0 {
                                    v1 = v_bottom;
                                    v0 = v_bottom - v_range.abs() * portion;
                                } else {
                                    v1 = v_bottom;
                                    v0 = v_bottom + v_range.abs() * portion;
                                }
                            }

                            // Transform back to screen space if reverse scroll
                            let (
                                segment_center_screen,
                                segment_size_screen,
                                seg_top_screen,
                                seg_bottom_screen,
                            ) = if visual_reverse {
                                let top_scr = 2.0 * receptor - segment_bottom_eff;
                                let bottom_scr = 2.0 * receptor - segment_top_eff;
                                (
                                    (top_scr + bottom_scr) * 0.5,
                                    bottom_scr - top_scr,
                                    top_scr,
                                    bottom_scr,
                                )
                            } else {
                                (
                                    (segment_top_eff + segment_bottom_eff) * 0.5,
                                    segment_size_eff,
                                    segment_top_eff,
                                    segment_bottom_eff,
                                )
                            };

                            let rotation = if visual_reverse { 180.0 } else { 0.0 };

                            // Track rendered bounds in screen space
                            rendered_body_top = Some(match rendered_body_top {
                                None => seg_top_screen,
                                Some(v) => v.min(seg_top_screen),
                            });
                            rendered_body_bottom = Some(match rendered_body_bottom {
                                None => seg_bottom_screen,
                                Some(v) => v.max(seg_bottom_screen),
                            });

                            actors.push(act!(sprite(body_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset, segment_center_screen):
                                setsize(body_width, segment_size_screen):
                                rotationz(rotation):
                                customtexturerect(u0, v0, u1, v1):
                                diffuse(
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3]
                                ):
                                z(Z_HOLD_BODY)
                            ));

                            phase = next_phase;
                            emitted += 1;
                        }
                    }
                }
            }
            if let Some(cap_slot) = tail_slot {
                let tail_position = body_tail_y;
                if tail_position > -400.0 && tail_position < screen_height() + 400.0 {
                    let cap_frame = cap_slot.frame_index_from_phase(hold_cap_phase);
                    let cap_uv_elapsed = if cap_slot.model.is_some() {
                        hold_cap_phase
                    } else {
                        state.total_elapsed_in_screen
                    };
                    let cap_uv = translated_uv_rect(
                        cap_slot.uv_for_frame_at(cap_frame, cap_uv_elapsed),
                        ns.part_uv_translation(hold_bottomcap_part, note.beat, false),
                    );
                    let cap_size = scale_cap(cap_slot.size());
                    let cap_width = cap_size[0];
                    let mut cap_height = cap_size[1];
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let mut v0 = cap_uv[1];
                    let mut v1 = cap_uv[3];
                    let body_anchor = match (rendered_body_top, rendered_body_bottom) {
                        (Some(t), Some(b)) if b > t + 0.5 => Some((t, b)),
                        _ => None,
                    };
                    // ITG always draws the tail cap path (DrawHoldBodyInternal -> hpt_bottom),
                    // even when no body strip vertices are emitted. Keep body-edge anchoring
                    // when available, but fall back to tail-based placement otherwise so
                    // very short holds remain visible.
                    let (mut cap_top, mut cap_bottom) = if let Some((rt, rb)) = body_anchor {
                        let cap_adjacent_ok = if head_is_top {
                            // Tail visually below; ensure the drawn body bottom is near the tail.
                            let dist = body_tail_y - rb;
                            dist >= -2.0 && dist <= cap_height + 2.0
                        } else {
                            // Tail visually above; ensure the drawn body top is near the tail.
                            let dist = rt - body_tail_y;
                            dist >= -2.0 && dist <= cap_height + 2.0
                        };
                        if !cap_adjacent_ok {
                            continue;
                        }
                        // Anchor cap join to the actual rendered body edge. This avoids
                        // sub-pixel drift leaving a 1px seam at the body/cap boundary.
                        if head_is_top {
                            (rb, rb + cap_height)
                        } else {
                            (rt - cap_height, rt)
                        }
                    } else if head_is_top {
                        (body_tail_y, body_tail_y + cap_height)
                    } else {
                        (body_tail_y - cap_height, body_tail_y)
                    };
                    let mut cap_center = (cap_top + cap_bottom) * 0.5;
                    if cap_height > f32::EPSILON {
                        let v_span = v1 - v0;
                        if head_is_top {
                            let head_limit = top;
                            if head_limit > cap_top {
                                let trimmed = (head_limit - cap_top).clamp(0.0, cap_height);
                                if trimmed >= cap_height - f32::EPSILON {
                                    cap_height = 0.0;
                                } else if trimmed > f32::EPSILON {
                                    let fraction = trimmed / cap_height;
                                    v0 += v_span * fraction;
                                    cap_top += trimmed;
                                    cap_center = (cap_top + cap_bottom) * 0.5;
                                    cap_height = cap_bottom - cap_top;
                                }
                            }
                        } else {
                            let head_limit = bottom;
                            if head_limit < cap_bottom {
                                let trimmed = (cap_bottom - head_limit).clamp(0.0, cap_height);
                                if trimmed >= cap_height - f32::EPSILON {
                                    cap_height = 0.0;
                                } else if trimmed > f32::EPSILON {
                                    let fraction = trimmed / cap_height;
                                    v1 -= v_span * fraction;
                                    cap_bottom -= trimmed;
                                    cap_center = (cap_top + cap_bottom) * 0.5;
                                    cap_height = cap_bottom - cap_top;
                                }
                            }
                        }
                    }
                    if cap_height > f32::EPSILON {
                        actors.push(act!(sprite(cap_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset, cap_center):
                            setsize(cap_width, cap_height):
                            customtexturerect(u0, v0, u1, v1):
                            diffuse(
                                hold_diffuse[0],
                                hold_diffuse[1],
                                hold_diffuse[2],
                                hold_diffuse[3]
                            ):
                            rotationz(if visual_reverse { 180.0 } else { 0.0 }):
                            z(Z_HOLD_CAP)
                        ));
                    }
                }
            }
            let should_draw_hold_head = true;
            let head_draw_y = if engaged {
                lane_receptor_y
            } else {
                head_anchor_y
            };
            let head_draw_delta = (head_draw_y - lane_receptor_y) * dir;
            if should_draw_hold_head
                && head_draw_delta >= -draw_distance_after_targets
                && head_draw_delta <= draw_distance_before_targets
            {
                let head_alpha = 1.0;
                let hold_head_rot =
                    calc_note_rotation_z(visual_mask, note.beat, current_beat, true);
                let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                let head_center = [playfield_center_x + col_x_offset, head_draw_y];
                let elapsed = state.total_elapsed_in_screen;
                let head_slot = if use_active {
                    visuals
                        .head_active
                        .as_ref()
                        .or(visuals.head_inactive.as_ref())
                } else {
                    visuals
                        .head_inactive
                        .as_ref()
                        .or(visuals.head_active.as_ref())
                };
                if let Some(head_slot) = head_slot {
                    let draw = head_slot.model_draw_at(elapsed, current_beat);
                    if !draw.visible {
                        continue;
                    }
                    let frame = head_slot.frame_index_from_phase(hold_part_phase);
                    let uv_elapsed = if head_slot.model.is_some() {
                        hold_part_phase
                    } else {
                        elapsed
                    };
                    let uv = translated_uv_rect(
                        head_slot.uv_for_frame_at(frame, uv_elapsed),
                        ns.part_uv_translation(hold_head_part, note.beat, false),
                    );
                    let h = note_scale_height(head_slot);
                    let note_scale = if h > f32::EPSILON {
                        target_arrow_px / h
                    } else {
                        1.0
                    };
                    let base_size = scaled_note_slot_size(head_slot, note_scale);
                    let local_offset = [draw.pos[0] * note_scale, draw.pos[1] * note_scale];
                    let local_offset_rot_sin_cos = head_slot.base_rot_sin_cos();
                    let model_center = if head_slot.model.is_some() {
                        let [sin_r, cos_r] = local_offset_rot_sin_cos;
                        let offset = [
                            local_offset[0] * cos_r - local_offset[1] * sin_r,
                            local_offset[0] * sin_r + local_offset[1] * cos_r,
                        ];
                        [head_center[0] + offset[0], head_center[1] + offset[1]]
                    } else {
                        head_center
                    };
                    let size = [
                        base_size[0] * draw.zoom[0].max(0.0),
                        base_size[1] * draw.zoom[1].max(0.0),
                    ];
                    if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                        continue;
                    }
                    let color = [
                        draw.tint[0] * hold_diffuse[0],
                        draw.tint[1] * hold_diffuse[1],
                        draw.tint[2] * hold_diffuse[2],
                        draw.tint[3] * hold_diffuse[3] * head_alpha,
                    ];
                    let blend = if draw.blend_add {
                        BlendMode::Add
                    } else {
                        BlendMode::Alpha
                    };
                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                        head_slot,
                        draw,
                        model_center,
                        size,
                        uv,
                        -head_slot.def.rotation_deg as f32 + hold_head_rot,
                        color,
                        blend,
                        Z_TAP_NOTE as i16,
                        &mut model_cache,
                    ) {
                        actors.push(model_actor);
                    } else if draw.blend_add {
                        actors.push(with_sprite_local_offset(
                            act!(sprite(head_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(head_center[0], head_center[1]):
                                setsize(size[0], size[1]):
                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + hold_head_rot):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(add):
                                z(Z_TAP_NOTE)
                            ),
                            local_offset,
                            local_offset_rot_sin_cos,
                        ));
                    } else {
                        actors.push(with_sprite_local_offset(
                            act!(sprite(head_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(head_center[0], head_center[1]):
                                setsize(size[0], size[1]):
                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + hold_head_rot):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(normal):
                                z(Z_TAP_NOTE)
                            ),
                            local_offset,
                            local_offset_rot_sin_cos,
                        ));
                    }
                } else if let Some(note_slots) = ns.note_layers.get(note_idx) {
                    let primary_h = note_slots
                        .first()
                        .map(note_scale_height)
                        .unwrap_or(1.0);
                    let note_scale = if primary_h > f32::EPSILON {
                        target_arrow_px / primary_h
                    } else {
                        1.0
                    };
                    for note_slot in note_slots.iter() {
                        let draw = note_slot.model_draw_at(elapsed, current_beat);
                        if !draw.visible {
                            continue;
                        }
                        let frame = note_slot.frame_index_from_phase(hold_part_phase);
                        let uv_elapsed = if note_slot.model.is_some() {
                            hold_part_phase
                        } else {
                            elapsed
                        };
                        let uv = translated_uv_rect(
                            note_slot.uv_for_frame_at(frame, uv_elapsed),
                            ns.part_uv_translation(hold_head_part, note.beat, false),
                        );
                        let base_size = scaled_note_slot_size(note_slot, note_scale);
                        let offset_scale = note_scale;
                        let local_offset = [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
                        let local_offset_rot_sin_cos = note_slot.base_rot_sin_cos();
                        let model_center = if note_slot.model.is_some() {
                            let [sin_r, cos_r] = local_offset_rot_sin_cos;
                            let offset = [
                                local_offset[0] * cos_r - local_offset[1] * sin_r,
                                local_offset[0] * sin_r + local_offset[1] * cos_r,
                            ];
                            [head_center[0] + offset[0], head_center[1] + offset[1]]
                        } else {
                            head_center
                        };
                        let size = [
                            base_size[0] * draw.zoom[0].max(0.0),
                            base_size[1] * draw.zoom[1].max(0.0),
                        ];
                        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                            continue;
                        }
                        let color = [
                            draw.tint[0] * hold_diffuse[0],
                            draw.tint[1] * hold_diffuse[1],
                            draw.tint[2] * hold_diffuse[2],
                            draw.tint[3] * hold_diffuse[3] * head_alpha,
                        ];
                        let layer_z = Z_TAP_NOTE;
                        let blend = if draw.blend_add {
                            BlendMode::Add
                        } else {
                            BlendMode::Alpha
                        };
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            note_slot,
                            draw,
                            model_center,
                            size,
                            uv,
                            -note_slot.def.rotation_deg as f32 + hold_head_rot,
                            color,
                            blend,
                            layer_z as i16,
                            &mut model_cache,
                        ) {
                            actors.push(model_actor);
                        } else if draw.blend_add {
                            actors.push(with_sprite_local_offset(
                                act!(sprite(note_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(head_center[0], head_center[1]):
                                    setsize(size[0], size[1]):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + hold_head_rot):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(layer_z)
                                ),
                                local_offset,
                                local_offset_rot_sin_cos,
                            ));
                        } else {
                            actors.push(with_sprite_local_offset(
                                act!(sprite(note_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(head_center[0], head_center[1]):
                                    setsize(size[0], size[1]):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + hold_head_rot):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(layer_z)
                                ),
                                local_offset,
                                local_offset_rot_sin_cos,
                            ));
                        }
                    }
                } else if let Some(note_slot) = ns.notes.get(note_idx) {
                    let frame = note_slot.frame_index_from_phase(hold_part_phase);
                    let uv_elapsed = if note_slot.model.is_some() {
                        hold_part_phase
                    } else {
                        elapsed
                    };
                    let uv = translated_uv_rect(
                        note_slot.uv_for_frame_at(frame, uv_elapsed),
                        ns.part_uv_translation(hold_head_part, note.beat, false),
                    );
                    let size = scale_sprite(note_slot.size());
                    let draw = note_slot.model_draw_at(elapsed, current_beat);
                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                        note_slot,
                        draw,
                        head_center,
                        size,
                        uv,
                        -note_slot.def.rotation_deg as f32 + hold_head_rot,
                        [
                            hold_diffuse[0],
                            hold_diffuse[1],
                            hold_diffuse[2],
                            hold_diffuse[3] * head_alpha,
                        ],
                        BlendMode::Alpha,
                        Z_TAP_NOTE as i16,
                        &mut model_cache,
                    ) {
                        actors.push(model_actor);
                    } else {
                        actors.push(act!(sprite(note_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(head_center[0], head_center[1]):
                            setsize(size[0], size[1]):
                            rotationz(-note_slot.def.rotation_deg as f32 + hold_head_rot):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(
                                hold_diffuse[0],
                                hold_diffuse[1],
                                hold_diffuse[2],
                                hold_diffuse[3] * head_alpha
                            ):
                            z(Z_TAP_NOTE)
                        ));
                    }
                }
            }
        }
        // Active arrows
        for col_idx in 0..num_cols {
            let col = col_start + col_idx;
            let column_arrows = &state.arrows[col];
            let dir = column_dirs[col_idx];
            let receptor_y_lane = column_receptor_ys[col_idx];
            for arrow in column_arrows {
                let travel_offset = match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = state.note_time_cache[arrow.note_index];
                        (note_time_chart - current_time) / rate * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                        (note_disp_beat - curr_disp_beat)
                            * ScrollSpeedSetting::ARROW_SPACING
                            * field_zoom
                            * beatmod_multiplier
                    }
                };
                let y_pos = lane_y_from_travel(col_idx, receptor_y_lane, dir, travel_offset);
                let delta = (y_pos - receptor_y_lane) * dir;
                if delta < -draw_distance_after_targets || delta > draw_distance_before_targets {
                    continue;
                }
                let note_alpha = 1.0;
                let col_x_offset = ns.column_xs[col_idx] as f32 * field_zoom;
                if matches!(arrow.note_type, NoteType::Hold | NoteType::Roll) {
                    continue;
                }
                let note = &state.notes[arrow.note_index];
                let note_rot = calc_note_rotation_z(visual_mask, note.beat, current_beat, false);
                if matches!(arrow.note_type, NoteType::Mine) {
                    let fill_slot = ns.mines.get(col_idx).and_then(|slot| slot.as_ref());
                    let fill_gradient_slot = ns
                        .mine_fill_slots
                        .get(col_idx)
                        .and_then(|slot| slot.as_ref());
                    let frame_slot = ns.mine_frames.get(col_idx).and_then(|slot| slot.as_ref());
                    if fill_slot.is_none() && frame_slot.is_none() {
                        continue;
                    }
                    let phase_time = state.total_elapsed_in_screen;
                    let note_display_time = phase_time * note_display_time_scale;
                    let beat = current_beat;
                    let mine_note_beat = note.beat;
                    let mine_uv_phase = ns.tap_mine_uv_phase(phase_time, beat, mine_note_beat);
                    let circle_reference = frame_slot
                        .map(scale_mine_slot)
                        .or_else(|| fill_slot.map(scale_mine_slot))
                        .unwrap_or([
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                        ]);
                    if let Some(slot) = fill_slot {
                        if frame_slot.is_some()
                            && slot.model.is_none()
                            && slot.source.frame_count() <= 1
                            && let Some(gradient_slot) = fill_gradient_slot
                        {
                            let width = circle_reference[0] * MINE_CORE_SIZE_RATIO;
                            let height = circle_reference[1] * MINE_CORE_SIZE_RATIO;
                            if width > 0.0 && height > 0.0 {
                                let fill_phase = current_beat.rem_euclid(1.0);
                                let frame = gradient_slot.frame_index_from_phase(fill_phase);
                                let uv = gradient_slot.uv_for_frame_at(frame, phase_time);
                                actors.push(act!(sprite(gradient_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(playfield_center_x + col_x_offset, y_pos):
                                    setsize(width, height):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(1.0, 1.0, 1.0, note_alpha):
                                    z(Z_TAP_NOTE - 2)
                                ));
                            }
                        } else {
                            let draw = slot.model_draw_at(note_display_time, beat);
                            if draw.visible {
                                let frame = slot.frame_index_from_phase(mine_uv_phase);
                                let uv_elapsed = if slot.model.is_some() {
                                    mine_uv_phase
                                } else {
                                    phase_time
                                };
                                let uv = translated_uv_rect(
                                    slot.uv_for_frame_at(frame, uv_elapsed),
                                    ns.part_uv_translation(
                                        NoteAnimPart::Mine,
                                        mine_note_beat,
                                        false,
                                    ),
                                );
                                let size = scale_mine_slot(slot);
                                let width = size[0];
                                let height = size[1];
                                let base_rotation = -slot.def.rotation_deg as f32;
                                let has_scripted_rot =
                                    matches!(slot.model_effect.mode, ModelEffectMode::Spin)
                                        || slot.model_auto_rot_total_frames > f32::EPSILON
                                        || draw.rot[2].abs() > f32::EPSILON;
                                let legacy_rot = if has_scripted_rot {
                                    0.0
                                } else {
                                    -note_display_time * 45.0
                                };
                                let sprite_rotation =
                                    base_rotation + legacy_rot + draw.rot[2] + note_rot;
                                let center = [playfield_center_x + col_x_offset, y_pos];
                                if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                    slot,
                                    draw,
                                    center,
                                    [width, height],
                                    uv,
                                    base_rotation + legacy_rot + note_rot,
                                    [1.0, 1.0, 1.0, 0.9 * note_alpha],
                                    BlendMode::Alpha,
                                    (Z_TAP_NOTE - 1) as i16,
                                    &mut model_cache,
                                ) {
                                    actors.push(model_actor);
                                } else {
                                    actors.push(act!(sprite(slot.texture_key().to_string()):
                                        align(0.5, 0.5):
                                        xy(center[0], center[1]):
                                        setsize(width, height):
                                        rotationz(sprite_rotation):
                                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                        diffuse(1.0, 1.0, 1.0, 0.9 * note_alpha):
                                        z(Z_TAP_NOTE - 1)
                                    ));
                                }
                            }
                        }
                    }
                    if let Some(slot) = frame_slot {
                        let draw = slot.model_draw_at(note_display_time, beat);
                        if !draw.visible {
                            continue;
                        }
                        let frame = slot.frame_index_from_phase(mine_uv_phase);
                        let uv_elapsed = if slot.model.is_some() {
                            mine_uv_phase
                        } else {
                            phase_time
                        };
                        let uv = translated_uv_rect(
                            slot.uv_for_frame_at(frame, uv_elapsed),
                            ns.part_uv_translation(NoteAnimPart::Mine, mine_note_beat, false),
                        );
                        let size = scale_mine_slot(slot);
                        let base_rotation = -slot.def.rotation_deg as f32;
                        let has_scripted_rot =
                            matches!(slot.model_effect.mode, ModelEffectMode::Spin)
                                || slot.model_auto_rot_total_frames > f32::EPSILON
                                || draw.rot[2].abs() > f32::EPSILON;
                        let legacy_rot = if has_scripted_rot {
                            0.0
                        } else {
                            note_display_time * 120.0
                        };
                        let sprite_rotation = base_rotation + legacy_rot + draw.rot[2] + note_rot;
                        let center = [playfield_center_x + col_x_offset, y_pos];
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            slot,
                            draw,
                            center,
                            size,
                            uv,
                            base_rotation + legacy_rot + note_rot,
                            [1.0, 1.0, 1.0, note_alpha],
                            BlendMode::Alpha,
                            Z_TAP_NOTE as i16,
                            &mut model_cache,
                        ) {
                            actors.push(model_actor);
                        } else {
                            actors.push(act!(sprite(slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(center[0], center[1]):
                                setsize(size[0], size[1]):
                                rotationz(sprite_rotation):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(1.0, 1.0, 1.0, note_alpha):
                                z(Z_TAP_NOTE)
                            ));
                        }
                    }
                    continue;
                }
                let tap_note_part = tap_part_for_note_type(note.note_type);
                let tap_row_flags = state
                    .tap_row_hold_roll_flags
                    .get(arrow.note_index)
                    .copied()
                    .unwrap_or(0);
                let tap_replacement_roll = if note.note_type == NoteType::Tap {
                    let same_row_has_hold = tap_row_flags & 0b01 != 0;
                    let same_row_has_roll = tap_row_flags & 0b10 != 0;
                    let draw_hold = ns.note_display_metrics.draw_hold_head_for_taps_on_same_row;
                    let draw_roll = ns.note_display_metrics.draw_roll_head_for_taps_on_same_row;
                    if same_row_has_hold && same_row_has_roll {
                        if draw_hold && draw_roll {
                            Some(!ns.note_display_metrics.tap_hold_roll_on_row_means_hold)
                        } else if draw_hold {
                            Some(false)
                        } else if draw_roll {
                            Some(true)
                        } else {
                            None
                        }
                    } else if same_row_has_hold && draw_hold {
                        Some(false)
                    } else if same_row_has_roll && draw_roll {
                        Some(true)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(use_roll_head) = tap_replacement_roll {
                    let visuals = ns.hold_visuals_for_col(col_idx, use_roll_head);
                    if let Some(head_slot) = visuals
                        .head_inactive
                        .as_ref()
                        .or(visuals.head_active.as_ref())
                    {
                        let elapsed = state.total_elapsed_in_screen;
                        let part = if use_roll_head {
                            NoteAnimPart::RollHead
                        } else {
                            NoteAnimPart::HoldHead
                        };
                        let head_phase = ns.part_uv_phase(part, elapsed, current_beat, note.beat);
                        let note_frame = head_slot.frame_index_from_phase(head_phase);
                        let uv_elapsed = if head_slot.model.is_some() {
                            head_phase
                        } else {
                            elapsed
                        };
                        let note_uv = translated_uv_rect(
                            head_slot.uv_for_frame_at(note_frame, uv_elapsed),
                            ns.part_uv_translation(part, note.beat, false),
                        );
                        let h = note_scale_height(head_slot);
                        let note_scale = if h > f32::EPSILON {
                            target_arrow_px / h
                        } else {
                            1.0
                        };
                        let note_size = scaled_note_slot_size(head_slot, note_scale);
                        let center = [playfield_center_x + col_x_offset, y_pos];
                        let draw = head_slot.model_draw_at(elapsed, current_beat);
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            head_slot,
                            draw,
                            center,
                            note_size,
                            note_uv,
                            -head_slot.def.rotation_deg as f32 + note_rot,
                            [1.0, 1.0, 1.0, note_alpha],
                            BlendMode::Alpha,
                            Z_TAP_NOTE as i16,
                            &mut model_cache,
                        ) {
                            actors.push(model_actor);
                        } else {
                            actors.push(act!(sprite(head_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(center[0], center[1]):
                                setsize(note_size[0], note_size[1]):
                                rotationz(-head_slot.def.rotation_deg as f32 + note_rot):
                                customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                diffuse(1.0, 1.0, 1.0, note_alpha):
                                z(Z_TAP_NOTE)
                            ));
                        }
                        continue;
                    }
                }
                let note_idx = col_idx * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                if let Some(note_slots) = ns.note_layers.get(note_idx) {
                    let note_center = [playfield_center_x + col_x_offset, y_pos];
                    let elapsed = state.total_elapsed_in_screen;
                    let note_uv_phase =
                        ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                    let primary_h = note_slots
                        .first()
                        .map(note_scale_height)
                        .unwrap_or(1.0);
                    let note_scale = if primary_h > f32::EPSILON {
                        target_arrow_px / primary_h
                    } else {
                        1.0
                    };
                    for note_slot in note_slots.iter() {
                        let draw = note_slot.model_draw_at(elapsed, current_beat);
                        if !draw.visible {
                            continue;
                        }
                        let note_frame = note_slot.frame_index_from_phase(note_uv_phase);
                        let uv_elapsed = if note_slot.model.is_some() {
                            note_uv_phase
                        } else {
                            elapsed
                        };
                        let note_uv = translated_uv_rect(
                            note_slot.uv_for_frame_at(note_frame, uv_elapsed),
                            ns.part_uv_translation(tap_note_part, note.beat, false),
                        );
                        let base_size = scaled_note_slot_size(note_slot, note_scale);
                        let offset_scale = note_scale;
                        let local_offset = [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
                        let local_offset_rot_sin_cos = note_slot.base_rot_sin_cos();
                        let model_center = if note_slot.model.is_some() {
                            let [sin_r, cos_r] = local_offset_rot_sin_cos;
                            let offset = [
                                local_offset[0] * cos_r - local_offset[1] * sin_r,
                                local_offset[0] * sin_r + local_offset[1] * cos_r,
                            ];
                            [note_center[0] + offset[0], note_center[1] + offset[1]]
                        } else {
                            note_center
                        };
                        let note_size = [
                            base_size[0] * draw.zoom[0].max(0.0),
                            base_size[1] * draw.zoom[1].max(0.0),
                        ];
                        if note_size[0] <= f32::EPSILON || note_size[1] <= f32::EPSILON {
                            continue;
                        }
                        let layer_z = Z_TAP_NOTE;
                        let blend = if draw.blend_add {
                            BlendMode::Add
                        } else {
                            BlendMode::Alpha
                        };
                        let color = [
                            draw.tint[0],
                            draw.tint[1],
                            draw.tint[2],
                            draw.tint[3] * note_alpha,
                        ];
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            note_slot,
                            draw,
                            model_center,
                            note_size,
                            note_uv,
                            -note_slot.def.rotation_deg as f32 + note_rot,
                            color,
                            blend,
                            layer_z as i16,
                            &mut model_cache,
                        ) {
                            actors.push(model_actor);
                        } else {
                            if draw.blend_add {
                                actors.push(with_sprite_local_offset(act!(sprite(note_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(note_center[0], note_center[1]):
                                    setsize(note_size[0], note_size[1]):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + note_rot):
                                    customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(layer_z)
                                ), local_offset, local_offset_rot_sin_cos));
                            } else {
                                actors.push(with_sprite_local_offset(act!(sprite(note_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(note_center[0], note_center[1]):
                                    setsize(note_size[0], note_size[1]):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + note_rot):
                                    customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(layer_z)
                                ), local_offset, local_offset_rot_sin_cos));
                            }
                        }
                    }
                } else if let Some(note_slot) = ns.notes.get(note_idx) {
                    let elapsed = state.total_elapsed_in_screen;
                    let note_uv_phase =
                        ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                    let note_frame = note_slot.frame_index_from_phase(note_uv_phase);
                    let uv_elapsed = if note_slot.model.is_some() {
                        note_uv_phase
                    } else {
                        elapsed
                    };
                    let note_uv = translated_uv_rect(
                        note_slot.uv_for_frame_at(note_frame, uv_elapsed),
                        ns.part_uv_translation(tap_note_part, note.beat, false),
                    );
                    let note_size = scale_sprite(note_slot.size());
                    let center = [playfield_center_x + col_x_offset, y_pos];
                    let draw = note_slot.model_draw_at(elapsed, current_beat);
                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                        note_slot,
                        draw,
                        center,
                        note_size,
                        note_uv,
                        -note_slot.def.rotation_deg as f32 + note_rot,
                        [1.0, 1.0, 1.0, note_alpha],
                        BlendMode::Alpha,
                        Z_TAP_NOTE as i16,
                        &mut model_cache,
                    ) {
                        actors.push(model_actor);
                    } else {
                        actors.push(act!(sprite(note_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(center[0], center[1]):
                            setsize(note_size[0], note_size[1]):
                            rotationz(-note_slot.def.rotation_deg as f32 + note_rot):
                            customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                            diffuse(1.0, 1.0, 1.0, note_alpha):
                            z(Z_TAP_NOTE)
                        ));
                    }
                }
            }
        }
    }
    // Simply Love: ScreenGameplay underlay/PerPlayer/NoteField/DisplayMods.lua
    // shows the current mod string for 5s, then decelerates out over 0.5s.
    {
        // Simply Love DisplayMods.lua uses sleep(5), but ScreenGameplay in/default.lua
        // keeps a full-screen intro cover up for 2.0s. Since deadsync's gameplay
        // in-transition cover is shorter, subtract the exact missing cover time so
        // the *visible* mods duration matches ITG/SL.
        const SL_DISPLAY_MODS_HOLD_S: f32 = 5.0;
        const SL_GAMEPLAY_IN_COVER_S: f32 = 2.0;
        const MODS_FADE_S: f32 = 0.5;
        let hold_adjust = (SL_GAMEPLAY_IN_COVER_S - TRANSITION_IN_DURATION).max(0.0);
        let mods_hold_s = (SL_DISPLAY_MODS_HOLD_S - hold_adjust).max(0.0);

        let alpha = if elapsed_screen <= mods_hold_s {
            1.0
        } else if elapsed_screen < mods_hold_s + MODS_FADE_S {
            let t = ((elapsed_screen - mods_hold_s) / MODS_FADE_S).clamp(0.0, 1.0);
            let decelerate = 1.0 - (1.0 - t) * (1.0 - t);
            1.0 - decelerate
        } else {
            0.0
        };

        if alpha > 0.0 {
            let mods_text = gameplay_mods_text(state.scroll_speed[player_idx], profile);
            if !mods_text.is_empty() {
                let y = screen_height() * 0.25 * 1.3 + 15.0 + notefield_offset_y;
                hud_actors.push(act!(text:
                    font("miso"): settext(mods_text):
                    align(0.5, 0.5): xy(playfield_center_x, y):
                    zoom(0.8): maxwidth(125.0):
                    shadowcolor(0.0, 0.0, 0.0, 1.0):
                    shadowlength(1.0):
                    diffuse(1.0, 1.0, 1.0, alpha):
                    z(84)
                ));
            }
        }
    }

    // Combo Milestone Explosions (100 / 1000 combo)
    if !profile.hide_combo && !profile.hide_combo_explosions && !p.combo_milestones.is_empty() {
        let combo_center_x = playfield_center_x;
        let combo_center_y = zmod_layout.combo_y;
        let player_color = state.player_color;
        let ease_out_quad = |t: f32| -> f32 {
            let t = t.clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(2)
        };
        for milestone in &p.combo_milestones {
            match milestone.kind {
                ComboMilestoneKind::Hundred => {
                    let elapsed = milestone.elapsed;
                    let explosion_duration = 0.5_f32;
                    if elapsed <= explosion_duration {
                        let progress = (elapsed / explosion_duration).clamp(0.0, 1.0);
                        let zoom = (2.0 - progress) * judgment_zoom_mod;
                        let alpha = (0.5 * (1.0 - progress)).max(0.0);
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let rotation = 90.0 * direction * progress;
                            hud_actors.push(act!(sprite("combo_explosion.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(zoom):
                                rotationz(rotation):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                    if elapsed <= COMBO_HUNDRED_MILESTONE_DURATION {
                        let progress = (elapsed / COMBO_HUNDRED_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let eased = ease_out_quad(progress);
                        let zoom = (0.25 + (2.0 - 0.25) * eased) * judgment_zoom_mod;
                        let alpha = (0.6 * (1.0 - eased)).max(0.0);
                        let rotation = 10.0 + (0.0 - 10.0) * eased;
                        hud_actors.push(act!(sprite("combo_100milestone_splode.png"):
                            align(0.5, 0.5):
                            xy(combo_center_x, combo_center_y):
                            zoom(zoom):
                            rotationz(rotation):
                            diffuse(player_color[0], player_color[1], player_color[2], alpha):
                            blend(add):
                            z(89)
                        ));
                        let mini_duration = 0.4_f32;
                        if elapsed <= mini_duration {
                            let mini_progress = (elapsed / mini_duration).clamp(0.0, 1.0);
                            let mini_zoom =
                                (0.25 + (1.8 - 0.25) * mini_progress) * judgment_zoom_mod;
                            let mini_alpha = (1.0 - mini_progress).max(0.0);
                            let mini_rotation = 10.0 + (0.0 - 10.0) * mini_progress;
                            hud_actors.push(act!(sprite("combo_100milestone_minisplode.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(mini_zoom):
                                rotationz(mini_rotation):
                                diffuse(player_color[0], player_color[1], player_color[2], mini_alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
                ComboMilestoneKind::Thousand => {
                    let elapsed = milestone.elapsed;
                    if elapsed <= COMBO_THOUSAND_MILESTONE_DURATION {
                        let progress =
                            (elapsed / COMBO_THOUSAND_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let zoom = (0.25 + (3.0 - 0.25) * progress) * judgment_zoom_mod;
                        let alpha = (0.7 * (1.0 - progress)).max(0.0);
                        let x_offset = 100.0 * progress * judgment_zoom_mod;
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let final_x = combo_center_x + x_offset * direction;
                            hud_actors.push(act!(sprite("combo_1000milestone_swoosh.png"):
                                align(0.5, 0.5):
                                xy(final_x, combo_center_y):
                                zoom(zoom):
                                zoomx(zoom * direction):
                                diffuse(player_color[0], player_color[1], player_color[2], alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
            }
        }
    }
    // Combo
    if !profile.hide_combo {
        let combo_y = zmod_layout.combo_y;
        let combo_font_name = zmod_combo_font_name(profile.combo_font);
        if p.miss_combo >= SHOW_COMBO_AT {
            if let Some(font_name) = combo_font_name {
                hud_actors.push(act!(text:
                    font(font_name): settext(p.miss_combo.to_string()):
                    align(0.5, 0.5): xy(playfield_center_x, combo_y):
                    zoom(0.75 * judgment_zoom_mod): horizalign(center): shadowlength(1.0):
                    diffuse(1.0, 0.0, 0.0, 1.0):
                    z(90)
                ));
            }
        } else if p.combo >= SHOW_COMBO_AT {
            let quint_active = zmod_combo_quint_active(state, player_idx, profile);
            let final_color = match profile.combo_colors {
                profile::ComboColors::None => [1.0, 1.0, 1.0, 1.0],
                profile::ComboColors::Rainbow => {
                    if profile.combo_mode == profile::ComboMode::FullCombo {
                        if matches!(
                            p.full_combo_grade,
                            Some(JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great)
                        ) {
                            zmod_combo_rainbow_color(state.total_elapsed_in_screen, false, p.combo)
                        } else {
                            [1.0, 1.0, 1.0, 1.0]
                        }
                    } else {
                        zmod_combo_rainbow_color(state.total_elapsed_in_screen, false, p.combo)
                    }
                }
                profile::ComboColors::RainbowScroll => {
                    if profile.combo_mode == profile::ComboMode::FullCombo {
                        if matches!(
                            p.full_combo_grade,
                            Some(JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great)
                        ) {
                            zmod_combo_rainbow_color(state.total_elapsed_in_screen, true, p.combo)
                        } else {
                            [1.0, 1.0, 1.0, 1.0]
                        }
                    } else {
                        zmod_combo_rainbow_color(state.total_elapsed_in_screen, true, p.combo)
                    }
                }
                profile::ComboColors::Glow => {
                    let combo_grade = if profile.combo_mode == profile::ComboMode::FullCombo {
                        p.full_combo_grade
                    } else {
                        p.current_combo_grade
                    };
                    if let Some(grade) = combo_grade {
                        let (color1, color2) = zmod_combo_glow_pair(
                            grade,
                            quint_active && grade == JudgeGrade::Fantastic,
                        );
                        zmod_combo_glow_color(color1, color2, state.total_elapsed_in_screen)
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    }
                }
                profile::ComboColors::Solid => {
                    let combo_grade = if profile.combo_mode == profile::ComboMode::FullCombo {
                        p.full_combo_grade
                    } else {
                        p.current_combo_grade
                    };
                    if let Some(grade) = combo_grade {
                        zmod_combo_solid_color(
                            grade,
                            quint_active && grade == JudgeGrade::Fantastic,
                        )
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    }
                }
            };
            if let Some(font_name) = combo_font_name {
                hud_actors.push(act!(text:
                    font(font_name): settext(p.combo.to_string()):
                    align(0.5, 0.5): xy(playfield_center_x, combo_y):
                    zoom(0.75 * judgment_zoom_mod): horizalign(center): shadowlength(1.0):
                    diffuse(final_color[0], final_color[1], final_color[2], final_color[3]):
                    z(90)
                ));
            }
        }
    }

    let mut error_bar_mask = profile::normalize_error_bar_mask(profile.error_bar_active_mask);
    if error_bar_mask == 0 {
        error_bar_mask =
            profile::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let show_error_bar_colorful = (error_bar_mask & profile::ERROR_BAR_BIT_COLORFUL) != 0;
    let show_error_bar_monochrome = (error_bar_mask & profile::ERROR_BAR_BIT_MONOCHROME) != 0;
    let show_error_bar_text = (error_bar_mask & profile::ERROR_BAR_BIT_TEXT) != 0;
    let show_error_bar_highlight = (error_bar_mask & profile::ERROR_BAR_BIT_HIGHLIGHT) != 0;
    let show_error_bar_average = (error_bar_mask & profile::ERROR_BAR_BIT_AVERAGE) != 0;
    let show_error_bar = error_bar_mask != 0;
    let (error_bar_y, error_bar_max_h) = if matches!(
        profile.judgment_graphic,
        crate::game::profile::JudgmentGraphic::None
    ) {
        (judgment_y, 30.0_f32)
    } else if profile.error_bar_up {
        (judgment_y - ERROR_BAR_OFFSET_FROM_JUDGMENT, 10.0_f32)
    } else {
        (judgment_y + ERROR_BAR_OFFSET_FROM_JUDGMENT, 10.0_f32)
    };

    // zmod ExtraAesthetics: offset indicator text (ErrorMSDisplay).
    if profile.error_ms_display
        && let Some(text) = p.offset_indicator_text
    {
        let age = elapsed_screen - text.started_at;
        if age >= 0.0 && age < OFFSET_INDICATOR_DUR_S {
            let mut offset_y = screen_center_y() + notefield_offset_y;
            if show_error_bar {
                let min_sep = error_bar_max_h * 0.5 + 6.0;
                if (offset_y - error_bar_y).abs() < min_sep {
                    offset_y = error_bar_y + min_sep;
                }
            }
            let c = error_bar_color_for_window(text.window, profile.show_fa_plus_window);
            hud_actors.push(act!(text:
                font("wendy"): settext(cached_offset_ms(text.offset_ms)):
                align(0.5, 0.5): xy(playfield_center_x, offset_y):
                zoom(0.25): shadowlength(1.0):
                diffuse(c[0], c[1], c[2], 1.0):
                z(184)
            ));
        }
    }

    // Error Bar (Simply Love parity)
    if show_error_bar {
        let mut styles = [profile::ErrorBarStyle::None; 4];
        let mut style_count = 0usize;
        if show_error_bar_colorful {
            styles[style_count] = profile::ErrorBarStyle::Colorful;
            style_count += 1;
        }
        if show_error_bar_monochrome {
            styles[style_count] = profile::ErrorBarStyle::Monochrome;
            style_count += 1;
        }
        if show_error_bar_highlight {
            styles[style_count] = profile::ErrorBarStyle::Highlight;
            style_count += 1;
        }
        if show_error_bar_average {
            styles[style_count] = profile::ErrorBarStyle::Average;
            style_count += 1;
        }
        let fa_plus_window_s = Some(crate::game::gameplay::player_fa_plus_window_s(
            state, player_idx,
        ));

        for style in styles.into_iter().take(style_count) {
            match style {
                crate::game::profile::ErrorBarStyle::Monochrome => {
                    let bar_h = error_bar_max_h;
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_MONOCHROME * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile.windows_s,
                        fa_plus_window_s,
                        profile.show_fa_plus_window,
                        profile.error_bar_trim,
                    );

                    let bg_alpha = if matches!(
                        profile.background_filter,
                        crate::game::profile::BackgroundFilter::Off
                    ) {
                        ERROR_BAR_MONO_BG_ALPHA
                    } else {
                        0.0
                    };
                    if bg_alpha > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_MONOCHROME + 2.0, bar_h + 2.0):
                            diffuse(0.0, 0.0, 0.0, bg_alpha):
                            z(180)
                        ));
                    }

                    hud_actors.push(act!(quad:
                        align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                        zoomto(2.0, bar_h):
                        diffuse(0.5, 0.5, 0.5, 1.0):
                        z(181)
                    ));

                    let line_alpha = if elapsed_screen < ERROR_BAR_LINES_FADE_START_S {
                        0.0
                    } else if elapsed_screen
                        < ERROR_BAR_LINES_FADE_START_S + ERROR_BAR_LINES_FADE_DUR_S
                    {
                        let t = (elapsed_screen - ERROR_BAR_LINES_FADE_START_S)
                            / ERROR_BAR_LINES_FADE_DUR_S;
                        ERROR_BAR_LINE_ALPHA * smoothstep01(t)
                    } else {
                        ERROR_BAR_LINE_ALPHA
                    };
                    if line_alpha > 0.0 && wscale.is_finite() && wscale > 0.0 {
                        for i in 0..bounds_len {
                            let offset = bounds_s[i] * wscale;
                            if !offset.is_finite() {
                                continue;
                            }
                            for sx in [-1.0_f32, 1.0_f32] {
                                hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + sx * offset, error_bar_y):
                                zoomto(1.0, bar_h):
                                diffuse(1.0, 1.0, 1.0, line_alpha):
                                z(182)
                            ));
                            }
                        }
                    }

                    let label_fade_out_start_s =
                        ERROR_BAR_LABEL_FADE_DUR_S + ERROR_BAR_LABEL_HOLD_S;
                    let label_alpha = if elapsed_screen < ERROR_BAR_LABEL_FADE_DUR_S {
                        smoothstep01(elapsed_screen / ERROR_BAR_LABEL_FADE_DUR_S)
                    } else if elapsed_screen < label_fade_out_start_s {
                        1.0
                    } else if elapsed_screen < label_fade_out_start_s + ERROR_BAR_LABEL_FADE_DUR_S {
                        1.0 - smoothstep01(
                            (elapsed_screen - label_fade_out_start_s) / ERROR_BAR_LABEL_FADE_DUR_S,
                        )
                    } else {
                        0.0
                    };
                    if label_alpha > 0.0 {
                        let x_off = ERROR_BAR_WIDTH_MONOCHROME * 0.25;
                        hud_actors.push(act!(text:
                            font("game"): settext("Early"):
                            align(0.5, 0.5): xy(playfield_center_x - x_off, error_bar_y):
                            zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                            z(184)
                        ));
                        hud_actors.push(act!(text:
                            font("game"): settext("Late"):
                            align(0.5, 0.5): xy(playfield_center_x + x_off, error_bar_y):
                            zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                            z(184)
                        ));
                    }

                    if wscale.is_finite() && wscale > 0.0 {
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_mono_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_MONOCHROME,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            let c = error_bar_color_for_window(
                                tick.window,
                                profile.show_fa_plus_window,
                            );
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                                zoomto(ERROR_BAR_TICK_WIDTH, bar_h):
                                diffuse(c[0], c[1], c[2], alpha):
                                z(183)
                            ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Colorful => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile.windows_s,
                        fa_plus_window_s,
                        profile.show_fa_plus_window,
                        profile.error_bar_trim,
                    );

                    let bar_visible = p
                        .error_bar_color_bar_started_at
                        .map(|t0| {
                            let age = elapsed_screen - t0;
                            age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
                        })
                        .unwrap_or(false);

                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_COLORFUL + 4.0, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(180)
                        ));

                        let base = if profile.show_fa_plus_window {
                            0usize
                        } else {
                            1usize
                        };
                        let mut lastx = 0.0_f32;
                        for i in 0..bounds_len {
                            let x = bounds_s[i] * wscale;
                            let width = x - lastx;
                            if !x.is_finite() || !width.is_finite() || width <= 0.0 {
                                lastx = x;
                                continue;
                            }
                            let window = timing_window_from_num(base + i);
                            let c = error_bar_color_for_window(window, profile.show_fa_plus_window);

                            let cx_early = -0.5 * (lastx + x);
                            let cx_late = 0.5 * (lastx + x);
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + cx_early, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], 1.0):
                                z(181)
                            ));
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + cx_late, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], 1.0):
                                z(181)
                            ));

                            lastx = x;
                        }
                    }

                    if wscale.is_finite() && wscale > 0.0 {
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_color_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_COLORFUL,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(182)
                        ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Highlight => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile.windows_s,
                        fa_plus_window_s,
                        profile.show_fa_plus_window,
                        profile.error_bar_trim,
                    );

                    let bar_visible = p
                        .error_bar_color_bar_started_at
                        .map(|t0| {
                            let age = elapsed_screen - t0;
                            age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
                        })
                        .unwrap_or(false);

                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_COLORFUL + 4.0, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(180)
                        ));

                        let base = if profile.show_fa_plus_window {
                            0usize
                        } else {
                            1usize
                        };
                        let mut lastx = 0.0_f32;
                        for i in 0..bounds_len {
                            let x = bounds_s[i] * wscale;
                            let width = x - lastx;
                            if !x.is_finite() || !width.is_finite() || width <= 0.0 {
                                lastx = x;
                                continue;
                            }
                            let window_num = base + i;
                            let window = timing_window_from_num(window_num);
                            let wi = window_num.min(5);
                            let c = error_bar_color_for_window(window, profile.show_fa_plus_window);
                            let early_a = error_bar_flash_alpha(
                                elapsed_screen,
                                p.error_bar_color_flash_early[wi],
                                ERROR_BAR_TICK_DUR_COLORFUL,
                            );
                            let late_a = error_bar_flash_alpha(
                                elapsed_screen,
                                p.error_bar_color_flash_late[wi],
                                ERROR_BAR_TICK_DUR_COLORFUL,
                            );

                            let cx_early = -0.5 * (lastx + x);
                            let cx_late = 0.5 * (lastx + x);
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + cx_early, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], early_a):
                                z(181)
                            ));
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + cx_late, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], late_a):
                                z(181)
                            ));

                            lastx = x;
                        }
                    }

                    if wscale.is_finite() && wscale > 0.0 {
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_color_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_COLORFUL,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(182)
                        ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Average => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_AVERAGE * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let avg_y = error_bar_y + ERROR_BAR_AVERAGE_Y_OFFSET;
                    let bar_visible = p
                        .error_bar_avg_bar_started_at
                        .map(|t0| {
                            let age = elapsed_screen - t0;
                            age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
                        })
                        .unwrap_or(false);
                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        let tick_h =
                            ERROR_BAR_HEIGHT_AVERAGE + 4.0 + ERROR_BAR_AVERAGE_TICK_EXTRA_H;
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_avg_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_COLORFUL,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x + x, avg_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, tick_h):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(182)
                        ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Text => {}
                crate::game::profile::ErrorBarStyle::None => {}
            }
        }
        if show_error_bar_text && let Some(text) = p.error_bar_text {
            let age = elapsed_screen - text.started_at;
            if age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL {
                let x = if text.early { -40.0 } else { 40.0 };
                let s = if text.early { "EARLY" } else { "LATE" };
                hud_actors.push(act!(text:
                    font("wendy"): settext(s):
                    align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                    zoom(0.25): shadowlength(1.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(184)
                ));
            }
        }
    }

    // Measure Counter / Measure Breakdown (Zmod parity)
    if profile.measure_counter != crate::game::profile::MeasureCounter::None {
        let segs: &[StreamSegment] = &state.measure_counter_segments[player_idx];
        if !segs.is_empty() {
            let lookahead: u8 = profile.measure_counter_lookahead.min(4);
            let multiplier = profile.measure_counter.multiplier();

            let beat_floor = state.current_beat_visible[player_idx].floor();
            let curr_measure = beat_floor / 4.0;
            let base_index = stream_segment_index_exclusive_end(segs, curr_measure);

            let mut column_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
            if profile.measure_counter_left {
                column_width *= 4.0 / 3.0;
            }

            if let Some(measure_counter_y) = zmod_layout.measure_counter_y {
                for j in (0..=lookahead).rev() {
                    let seg_index_unshifted = base_index + j as usize;
                    if seg_index_unshifted >= segs.len() {
                        continue;
                    }

                    let is_lookahead = j != 0;
                    let text = zmod_measure_counter_text(
                        beat_floor,
                        curr_measure,
                        segs,
                        seg_index_unshifted,
                        is_lookahead,
                        lookahead,
                        multiplier,
                    );
                    let Some(text) = text else { continue };

                    let seg_unshifted = segs[seg_index_unshifted];
                    let rgba = if seg_unshifted.is_break {
                        if is_lookahead {
                            [0.4, 0.4, 0.4, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        }
                    } else if is_lookahead {
                        [0.45, 0.45, 0.45, 1.0]
                    } else if text.contains('/') {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.5, 0.5, 0.5, 1.0]
                    };

                    let zoom = 0.35 - 0.05 * (j as f32);
                    let mut x = playfield_center_x;
                    let mut y = measure_counter_y;

                    if profile.measure_counter_vert {
                        y += 20.0 * (j as f32);
                    } else {
                        let denom = if lookahead == 0 {
                            1.0
                        } else {
                            lookahead as f32
                        };
                        x += (column_width / denom) * 2.0 * (j as f32);
                    }
                    if profile.measure_counter_left {
                        x -= column_width;
                    }

                    hud_actors.push(act!(text:
                        font(mc_font_name): settext(text):
                        align(0.5, 0.5): xy(x, y):
                        zoom(zoom): horizalign(center): shadowlength(1.0):
                        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                        z(85)
                    ));
                }

                // Broken Run Total (Zmod BrokenRunCounter.lua)
                if profile.broken_run
                    && let Some((broken_index, broken_end, is_broken)) =
                        zmod_broken_run_segment(segs, curr_measure)
                {
                    let seg0 = segs[broken_index];
                    if !seg0.is_break && is_broken {
                        let curr_count = (curr_measure - (seg0.start as f32)).floor() as i32 + 1;
                        let len = (broken_end - seg0.start) as i32;
                        let text = if curr_measure < 0.0 {
                            // BrokenRunCounter.lua special-cases negative time.
                            let first = segs[0];
                            if !first.is_break {
                                let v = (curr_measure * -1.0).floor() as i32 + 1;
                                cached_paren_i32(v)
                            } else {
                                let first_len = (first.end - first.start) as i32;
                                let v = (curr_measure * -1.0).floor() as i32 + 1 + first_len;
                                cached_paren_i32(v)
                            }
                        } else if curr_count != 0 {
                            cached_ratio_i32(curr_count, len)
                        } else {
                            cached_int_i32(len)
                        };

                        if text.contains('/') {
                            let mut x = playfield_center_x;
                            let mut y = measure_counter_y + 15.0;
                            if profile.measure_counter_vert {
                                y -= 15.0;
                                x += column_width * (4.0 / 3.0);
                            }
                            if profile.measure_counter_left {
                                x -= column_width;
                            }

                            hud_actors.push(act!(text:
                                font(mc_font_name): settext(text):
                                align(0.5, 0.5): xy(x, y):
                                zoom(0.35): horizalign(center): shadowlength(1.0):
                                diffuse(1.0, 1.0, 1.0, 0.7):
                                z(85)
                            ));
                        }
                    }
                }
            }

            // Run Timer (Zmod RunTimer.lua: TimerMode=Time only)
            if profile.run_timer
                && let Some(stream_index) = zmod_run_timer_index(segs, curr_measure)
            {
                let seg = segs[stream_index];
                if !seg.is_break {
                    let cur_bps = state.timing.get_bpm_for_beat(state.current_beat) / 60.0;
                    let rate = state.music_rate;
                    if cur_bps.is_finite() && cur_bps > 0.0 && rate.is_finite() && rate > 0.0 {
                        let measure_seconds = 4.0 / (cur_bps * rate);
                        let curr_time = state.current_beat / (cur_bps * rate);

                        let seg_len_s =
                            (((seg.end - seg.start) as f32) * measure_seconds).ceil() as i32;
                        let total = zmod_run_timer_fmt(seg_len_s, 60, false);

                        let remaining_s =
                            (((seg.end as f32) * measure_seconds) - curr_time).ceil() as i32;
                        let remaining_s = remaining_s.max(0);

                        let text = if remaining_s > seg_len_s {
                            total
                        } else if remaining_s < 1 {
                            zmod_run_timer_fmt(0, 59, true)
                        } else {
                            zmod_run_timer_fmt(remaining_s, 59, true)
                        };

                        let active = text.contains(' ');
                        let rgba = if active {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        };

                        let mut x = playfield_center_x;
                        if profile.measure_counter_left {
                            x -= column_width;
                        }
                        let y = zmod_layout.subtractive_scoring_y;

                        hud_actors.push(act!(text:
                            font(mc_font_name): settext(text):
                            align(0.5, 0.5): xy(x, y):
                            zoom(0.35): horizalign(center): shadowlength(1.0):
                            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                            z(85)
                        ));
                    }
                }
            }
        }
    }

    // Mini Indicator (zmod SubtractiveScoring.lua parity).
    if let Some((text, rgba)) = zmod_mini_indicator_text(state, p, profile, player_idx) {
        let column_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
        let mut x = playfield_center_x + column_width;
        let mut h_align = 0.5;
        if !profile.measure_counter_left {
            h_align = 0.0;
            x -= 12.0;
        }

        hud_actors.push(act!(text:
            font(mc_font_name): settext(text):
            align(h_align, 0.5): xy(x, zmod_layout.subtractive_scoring_y):
            zoom(0.35): shadowlength(1.0):
            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
            z(85)
        ));
    }

    // Judgment Sprite (tap judgments)
    if let Some(render_info) = &p.last_judgment {
        if matches!(profile.judgment_graphic, profile::JudgmentGraphic::None) {
            // Player chose to hide tap judgment graphics.
            // Still keep life/score effects; only suppress the visual sprite.
        } else {
            let judgment = &render_info.judgment;
            let elapsed = render_info.judged_at.elapsed().as_secs_f32();
            if elapsed < 0.9 {
                let zoom = if elapsed < 0.1 {
                    let t = elapsed / 0.1;
                    let ease_t = 1.0 - (1.0 - t).powi(2);
                    0.8 + (0.75 - 0.8) * ease_t
                } else if elapsed < 0.7 {
                    0.75
                } else {
                    let t = (elapsed - 0.7) / 0.2;
                    let ease_t = t.powi(2);
                    0.75 * (1.0 - ease_t)
                } * judgment_zoom_mod;
                let offset_sec = judgment.time_error_ms / 1000.0;
                let use_fa_plus_window = profile.show_fa_plus_window;
                // Map JudgeGrade + TimingWindow to a row index in the 7-row sheet:
                //  row 0: FA+ Fantastic (W0)
                //  row 1: regular Fantastic (W1)
                //  row 2..6: Excellent..Miss, matching our existing layout.
                let frame_row = match judgment.grade {
                    JudgeGrade::Fantastic => {
                        if use_fa_plus_window {
                            match judgment.window {
                                Some(TimingWindow::W0) => 0,
                                _ => 1,
                            }
                        } else {
                            0
                        }
                    }
                    JudgeGrade::Excellent => 2,
                    JudgeGrade::Great => 3,
                    JudgeGrade::Decent => 4,
                    JudgeGrade::WayOff => 5,
                    JudgeGrade::Miss => 6,
                };
                let frame_offset = if offset_sec < 0.0 { 0 } else { 1 };
                let columns = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Censored => 1,
                    _ => 2,
                };
                let col_index = if columns > 1 { frame_offset } else { 0 };
                let linear_index = (frame_row * columns + col_index) as u32;
                let judgment_texture = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Bebas => "judgements/Bebas 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Censored => "judgements/Censored 1x7 (doubleres).png",
                    profile::JudgmentGraphic::Chromatic => {
                        "judgements/Chromatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Code => "judgements/Code 2x7 (doubleres).png",
                    profile::JudgmentGraphic::ComicSans => {
                        "judgements/Comic Sans 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Emoticon => "judgements/Emoticon 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Focus => "judgements/Focus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Grammar => "judgements/Grammar 2x7 (doubleres).png",
                    profile::JudgmentGraphic::GrooveNights => {
                        "judgements/GrooveNights 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::ITG2 => "judgements/ITG2 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Love => "judgements/Love 2x7 (doubleres).png",
                    profile::JudgmentGraphic::LoveChroma => {
                        "judgements/Love Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Miso => "judgements/Miso 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Papyrus => "judgements/Papyrus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Rainbowmatic => {
                        "judgements/Rainbowmatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Roboto => "judgements/Roboto 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Shift => "judgements/Shift 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Tactics => "judgements/Tactics 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Wendy => "judgements/Wendy 2x7 (doubleres).png",
                    profile::JudgmentGraphic::WendyChroma => {
                        "judgements/Wendy Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::None => {
                        unreachable!("JudgmentGraphic::None is filtered above")
                    }
                };
                let rot_deg = if profile.judgment_tilt && judgment.grade != JudgeGrade::Miss {
                    let abs_sec = offset_sec.abs().min(0.050);
                    let dir = if offset_sec < 0.0 { 1.0 } else { -1.0 };
                    dir * abs_sec * 300.0 * profile.tilt_multiplier
                } else {
                    0.0
                };
                hud_actors.push(act!(sprite(judgment_texture):
                    align(0.5, 0.5): xy(playfield_center_x, judgment_y):
                    z(200): rotationz(rot_deg): setsize(0.0, 76.0): setstate(linear_index): zoom(zoom)
                ));
            }
        }
    }
    for i in 0..num_cols {
        let col = col_start + i;
        let Some(render_info) = state.hold_judgments[col].as_ref() else {
            continue;
        };
        let elapsed = render_info.triggered_at.elapsed().as_secs_f32();
        if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
            continue;
        }
        let zoom = if elapsed < 0.3 {
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            (HOLD_JUDGMENT_INITIAL_ZOOM
                + progress * (HOLD_JUDGMENT_FINAL_ZOOM - HOLD_JUDGMENT_INITIAL_ZOOM))
                * judgment_zoom_mod
        } else {
            HOLD_JUDGMENT_FINAL_ZOOM * judgment_zoom_mod
        };
        let frame_index = match render_info.result {
            HoldResult::Held => 0,
            HoldResult::LetGo => 1,
        } as u32;
        if let Some(texture) = hold_judgment_texture {
            let dir = column_dirs[i];
            let receptor_y_lane = column_receptor_ys[i];
            let hold_judgment_y = if dir >= 0.0 {
                // Non-reverse lane: match Simply Love's baseline offset below receptors.
                receptor_y_lane + HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR
            } else {
                // Reverse lane: mirror around the receptor so the hold judgment
                // appears just above the receptors instead of near screen center.
                receptor_y_lane - HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR
            };
            let column_offset = state.noteskin[player_idx]
                .as_ref()
                .and_then(|ns| ns.column_xs.get(i))
                .map(|&x| x as f32)
                .unwrap_or_else(|| ((i as f32) - 1.5) * TARGET_ARROW_PIXEL_SIZE * field_zoom);
            hud_actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(playfield_center_x + column_offset, hold_judgment_y):
                z(195):
                setstate(frame_index):
                zoom(zoom):
                diffusealpha(1.0)
            ));
        }
    }

    let (tilt, skew) = profile.perspective.tilt_skew();
    if (tilt != 0.0 || skew != 0.0) && !actors.is_empty() {
        let center_y = 0.5 * (receptor_y_normal + receptor_y_reverse);
        let reverse = column_dirs[0] < 0.0;
        if let Some(view_proj) = notefield_view_proj(
            screen_width(),
            screen_height(),
            playfield_center_x,
            center_y,
            tilt,
            skew,
            reverse,
        ) {
            actors = vec![Actor::Camera {
                view_proj,
                children: actors,
            }];
        }
    }

    if hud_actors.is_empty() {
        return (actors, layout_center_x);
    }
    let mut out: Vec<Actor> = Vec::with_capacity(hud_actors.len() + actors.len());
    out.extend(hud_actors);
    out.extend(actors);
    (out, layout_center_x)
}

#[cfg(test)]
mod tests {
    use super::note_scale_height;
    use crate::game::parsing::noteskin::{Style, load_itg_skin};

    #[test]
    fn cyber_model_tap_scale_uses_model_height_not_logical_height() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cyber")
            .expect("dance/cyber should load from assets/noteskins");
        let slot = ns
            .note_layers
            .first()
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("cyber should expose model-backed tap-note layer for 4th notes");

        let logical_h = slot.logical_size()[1].max(1.0);
        let model_h = slot
            .model
            .as_ref()
            .map(|model| model.size()[1])
            .expect("cyber tap slot should be model-backed");
        assert!(
            model_h > f32::EPSILON,
            "cyber model-backed tap slot should have positive model height"
        );
        assert!(
            logical_h / model_h > 1.5,
            "regression guard: cyber logical height must stay larger than model height so this test catches logical-height scaling; logical={logical_h}, model={model_h}"
        );
        let scale_h = note_scale_height(slot);
        assert!(
            (scale_h - model_h).abs() <= 1e-4,
            "model-backed tap notes must scale by model height; got scale_h={scale_h}, model_h={model_h}"
        );
    }
}
