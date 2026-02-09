use crate::assets;
use image::image_dimensions;
use log::{info, warn};
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const NUM_QUANTIZATIONS: usize = 9;
const MINE_GRADIENT_SAMPLES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Quantization {
    Q4th = 0,
    Q8th,
    Q12th,
    Q16th,
    Q24th,
    Q32nd,
    Q48th,
    Q64th,
    Q192nd,
}

impl Quantization {
    pub const fn from_row(row: u32) -> Option<Self> {
        match row {
            4 => Some(Self::Q4th),
            8 => Some(Self::Q8th),
            12 => Some(Self::Q12th),
            16 => Some(Self::Q16th),
            24 => Some(Self::Q24th),
            32 => Some(Self::Q32nd),
            48 => Some(Self::Q48th),
            64 => Some(Self::Q64th),
            192 => Some(Self::Q192nd),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpriteDefinition {
    pub src: [i32; 2],
    pub size: [i32; 2],
    pub rotation_deg: i32,
    pub mirror_h: bool,
    pub mirror_v: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum AnimationRate {
    FramesPerSecond(f32),
    FramesPerBeat(f32),
}

#[derive(Debug)]
pub enum SpriteSource {
    Atlas {
        texture_key: String,
        tex_dims: (u32, u32),
    },
    Animated {
        texture_key: String,
        tex_dims: (u32, u32),
        frame_size: [i32; 2],
        grid: (usize, usize),
        frame_count: usize,
        rate: AnimationRate,
    },
}

impl SpriteSource {
    pub fn texture_key(&self) -> &str {
        match self {
            Self::Atlas { texture_key, .. } => texture_key,
            Self::Animated { texture_key, .. } => texture_key,
        }
    }

    pub const fn tex_dims(&self) -> (u32, u32) {
        match self {
            Self::Atlas { tex_dims, .. } => *tex_dims,
            Self::Animated { tex_dims, .. } => *tex_dims,
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            Self::Atlas { .. } => 1,
            Self::Animated { frame_count, .. } => (*frame_count).max(1),
        }
    }

    pub const fn frame_size(&self) -> Option<[i32; 2]> {
        match self {
            Self::Atlas { .. } => None,
            Self::Animated { frame_size, .. } => Some(*frame_size),
        }
    }

    pub const fn is_beat_based(&self) -> bool {
        matches!(
            self,
            Self::Animated {
                rate: AnimationRate::FramesPerBeat(_),
                ..
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct SpriteSlot {
    pub def: SpriteDefinition,
    pub source: Arc<SpriteSource>,
}

impl SpriteSlot {
    pub fn texture_key(&self) -> &str {
        self.source.texture_key()
    }

    pub const fn size(&self) -> [i32; 2] {
        self.def.size
    }

    pub fn frame_index(&self, time: f32, beat: f32) -> usize {
        let frames = self.source.frame_count();
        if frames <= 1 {
            return 0;
        }
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated { rate, .. } => {
                let frame = match rate {
                    AnimationRate::FramesPerSecond(fps) if *fps > 0.0 => {
                        (time * fps).floor() as isize
                    }
                    AnimationRate::FramesPerBeat(frames_per_beat) if *frames_per_beat > 0.0 => {
                        (beat * frames_per_beat).floor() as isize
                    }
                    _ => return 0,
                };
                ((frame % frames as isize) + frames as isize) as usize % frames
            }
        }
    }

    pub fn uv_for_frame(&self, frame_index: usize) -> [f32; 4] {
        match self.source.as_ref() {
            SpriteSource::Atlas { tex_dims, .. } => {
                let tw = tex_dims.0.max(1) as f32;
                let th = tex_dims.1.max(1) as f32;
                let src = self.def.src;
                let size = self.def.size;

                let mut u0 = src[0] as f32;
                let mut v0 = src[1] as f32;
                let mut u1 = (src[0] + size[0]) as f32;
                let mut v1 = (src[1] + size[1]) as f32;

                if size[0] > 0 {
                    u0 += 0.5;
                    u1 -= 0.5;
                }
                if size[1] > 0 {
                    v0 += 0.5;
                    v1 -= 0.5;
                }

                [u0 / tw, v0 / th, u1 / tw, v1 / th]
            }
            SpriteSource::Animated {
                tex_dims,
                frame_size,
                grid,
                frame_count,
                ..
            } => {
                let frames = (*frame_count).max(1);
                let idx = if frames > 0 { frame_index % frames } else { 0 };
                let cols = grid.0.max(1);
                let row = idx / cols;
                let col = idx % cols;
                let src_x = self.def.src[0] + (col as i32 * frame_size[0]);
                let src_y = self.def.src[1] + (row as i32 * frame_size[1]);
                let tw = tex_dims.0.max(1) as f32;
                let th = tex_dims.1.max(1) as f32;

                let mut u0 = src_x as f32;
                let mut v0 = src_y as f32;
                let mut u1 = (src_x + frame_size[0]) as f32;
                let mut v1 = (src_y + frame_size[1]) as f32;

                if frame_size[0] > 0 {
                    u0 += 0.5;
                    u1 -= 0.5;
                }
                if frame_size[1] > 0 {
                    v0 += 0.5;
                    v1 -= 0.5;
                }

                [u0 / tw, v0 / th, u1 / tw, v1 / th]
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TweenType {
    Linear,
    Accelerate,
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionState {
    pub zoom: f32,
    pub color: [f32; 4],
}

impl Default for ExplosionState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionSegment {
    pub duration: f32,
    pub tween: TweenType,
    pub start: ExplosionState,
    pub end_zoom: Option<f32>,
    pub end_color: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Copy)]
pub struct GlowEffect {
    pub period: f32,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
}

impl GlowEffect {
    fn color_at(&self, time: f32, base_alpha: f32) -> [f32; 4] {
        if self.period <= f32::EPSILON || base_alpha <= f32::EPSILON {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let phase = (time / self.period).rem_euclid(1.0);
        if !phase.is_finite() {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let percent_between = ((phase + 0.25) * std::f32::consts::TAU)
            .sin()
            .mul_add(0.5, 0.5);

        let mut color = [0.0; 4];
        for i in 0..4 {
            color[i] =
                self.color1[i].mul_add(percent_between, self.color2[i] * (1.0 - percent_between));
        }
        color[3] *= base_alpha;
        color
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionVisualState {
    pub zoom: f32,
    pub diffuse: [f32; 4],
    pub glow: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct ExplosionAnimation {
    pub initial: ExplosionState,
    pub segments: Vec<ExplosionSegment>,
    pub glow: Option<GlowEffect>,
}

impl Default for ExplosionAnimation {
    fn default() -> Self {
        Self {
            initial: ExplosionState {
                zoom: 1.0,
                color: [1.0, 1.0, 1.0, 1.0],
            },
            segments: vec![ExplosionSegment {
                duration: 0.3,
                tween: TweenType::Linear,
                start: ExplosionState {
                    zoom: 1.0,
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
            }],
            glow: None,
        }
    }
}

impl ExplosionAnimation {
    pub fn duration(&self) -> f32 {
        self.segments
            .iter()
            .map(|segment| segment.duration.max(0.0))
            .sum::<f32>()
            .max(0.0)
    }

    pub fn state_at(&self, time: f32) -> ExplosionVisualState {
        let mut elapsed = time;
        let mut current = self.initial;

        for segment in &self.segments {
            let duration = segment.duration.max(0.0);
            if duration <= 0.0 {
                if let Some(zoom) = segment.end_zoom {
                    current.zoom = zoom;
                }
                if let Some(color) = segment.end_color {
                    current.color = color;
                }
                continue;
            }

            if elapsed > duration {
                if let Some(zoom) = segment.end_zoom {
                    current.zoom = zoom;
                }
                if let Some(color) = segment.end_color {
                    current.color = color;
                }
                elapsed -= duration;
                continue;
            }

            let progress = (elapsed / duration).clamp(0.0, 1.0);
            let eased = match segment.tween {
                TweenType::Linear => progress,
                TweenType::Accelerate => progress * progress,
            };

            let mut zoom = current.zoom;
            if let Some(target_zoom) = segment.end_zoom {
                zoom = (target_zoom - segment.start.zoom).mul_add(eased, segment.start.zoom);
            }

            let mut color = current.color;
            if let Some(target_color) = segment.end_color {
                let mut interpolated = current.color;
                for i in 0..4 {
                    interpolated[i] = (target_color[i] - segment.start.color[i])
                        .mul_add(eased, segment.start.color[i]);
                }
                color = interpolated;
            }

            let diffuse = color;
            let glow = self
                .glow
                .map_or([0.0, 0.0, 0.0, 0.0], |g| g.color_at(time, diffuse[3]));

            return ExplosionVisualState {
                zoom,
                diffuse,
                glow,
            };
        }

        let diffuse = current.color;
        let glow = self
            .glow
            .map_or([0.0, 0.0, 0.0, 0.0], |g| g.color_at(time, diffuse[3]));

        ExplosionVisualState {
            zoom: current.zoom,
            diffuse,
            glow,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TapExplosion {
    pub slot: SpriteSlot,
    pub animation: ExplosionAnimation,
}

#[derive(Debug, Clone, Default)]
pub struct HoldVisuals {
    pub body_inactive: Option<SpriteSlot>,
    pub body_active: Option<SpriteSlot>,
    pub bottomcap_inactive: Option<SpriteSlot>,
    pub bottomcap_active: Option<SpriteSlot>,
    pub explosion: Option<SpriteSlot>,
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub num_cols: usize,
    pub num_players: usize,
}

#[derive(Debug)]
pub struct Noteskin {
    pub notes: Vec<SpriteSlot>,
    pub receptor_off: Vec<SpriteSlot>,
    pub receptor_glow: Vec<Option<SpriteSlot>>,
    pub mines: Vec<Option<SpriteSlot>>,
    pub mine_fill_gradients: Vec<Option<Vec<[f32; 4]>>>,
    pub mine_frames: Vec<Option<SpriteSlot>>,
    pub column_xs: Vec<i32>,
    pub tap_explosions: HashMap<String, TapExplosion>,
    pub receptor_pulse: ReceptorPulse,
    pub hold_let_go_gray_percent: f32,
    pub hold: HoldVisuals,
    pub roll: HoldVisuals,
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorPulse {
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub ramp_to_half: f32,
    pub hold_at_half: f32,
    pub ramp_to_full: f32,
    pub hold_at_full: f32,
    pub hold_at_zero: f32,
    pub effect_offset: f32,
}

impl ReceptorPulse {
    fn total_period(&self) -> f32 {
        let mut total = 0.0;
        total += self.ramp_to_half.max(0.0);
        total += self.hold_at_half.max(0.0);
        total += self.ramp_to_full.max(0.0);
        total += self.hold_at_full.max(0.0);
        total += self.hold_at_zero.max(0.0);
        total
    }

    pub fn color_for_beat(&self, beat: f32) -> [f32; 4] {
        let period = self.total_period();
        if period <= f32::EPSILON {
            return self.effect_color2;
        }

        let phase = (beat + self.effect_offset).rem_euclid(period);

        let ramp_to_half = self.ramp_to_half.max(0.0);
        let hold_at_half = self.hold_at_half.max(0.0);
        let ramp_to_full = self.ramp_to_full.max(0.0);
        let hold_at_full = self.hold_at_full.max(0.0);

        let ramp_and_hold_half = ramp_to_half + hold_at_half;
        let through_ramp_full = ramp_and_hold_half + ramp_to_full;
        let through_hold_full = through_ramp_full + hold_at_full;

        let percent = if ramp_to_half > 0.0 && phase < ramp_to_half {
            (phase / ramp_to_half) * 0.5
        } else if phase < ramp_and_hold_half {
            0.5
        } else if ramp_to_full > 0.0 && phase < through_ramp_full {
            ((phase - ramp_and_hold_half) / ramp_to_full).mul_add(0.5, 0.5)
        } else if phase < through_hold_full {
            1.0
        } else {
            0.0
        };

        let mut color = [0.0; 4];
        for i in 0..4 {
            color[i] =
                self.effect_color1[i].mul_add(percent, self.effect_color2[i] * (1.0 - percent));
        }
        color
    }
}

impl Default for ReceptorPulse {
    fn default() -> Self {
        Self {
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            ramp_to_half: 0.25,
            hold_at_half: 0.5,
            ramp_to_full: 0.0,
            hold_at_full: 0.0,
            hold_at_zero: 0.25,
            effect_offset: -0.25,
        }
    }
}

fn mine_fill_gradients(mines: &[Option<SpriteSlot>]) -> Vec<Option<Vec<[f32; 4]>>> {
    mines
        .iter()
        .map(|slot| slot.as_ref().and_then(load_mine_gradient_colors))
        .collect()
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let path = Path::new("assets").join(texture_key);
    let image = assets::open_image_fallback(&path).ok()?.to_rgba8();

    let mut width = slot.def.size[0];
    let mut height = slot.def.size[1];
    if (width <= 0 || height <= 0)
        && let Some(frame) = slot.source.frame_size()
    {
        width = frame[0];
        height = frame[1];
    }

    if width <= 0 || height <= 0 {
        warn!("Mine fill slot has invalid size for gradient sampling");
        return None;
    }

    let src_x = slot.def.src[0].max(0) as u32;
    let src_y = slot.def.src[1].max(0) as u32;
    let mut sample_width = width as u32;
    let mut sample_height = height as u32;

    if src_x >= image.width() || src_y >= image.height() {
        warn!("Mine fill region ({src_x}, {src_y}) is outside of texture {texture_key}");
        return None;
    }

    if src_x + sample_width > image.width() {
        sample_width = image.width().saturating_sub(src_x);
    }
    if src_y + sample_height > image.height() {
        sample_height = image.height().saturating_sub(src_y);
    }

    if sample_width == 0 || sample_height == 0 {
        warn!("Mine fill region has zero sample size for texture {texture_key}");
        return None;
    }

    let mut colors = Vec::with_capacity(sample_width as usize);
    for dx in 0..sample_width {
        let mut r = 0.0_f32;
        let mut g = 0.0_f32;
        let mut b = 0.0_f32;
        let mut alpha_weight = 0.0_f32;

        for dy in 0..sample_height {
            let pixel = image.get_pixel(src_x + dx, src_y + dy);
            let a = f32::from(pixel[3]) / 255.0;
            if a <= f32::EPSILON {
                continue;
            }
            r += f32::from(pixel[0]) * a;
            g += f32::from(pixel[1]) * a;
            b += f32::from(pixel[2]) * a;
            alpha_weight += a;
        }

        if alpha_weight <= f32::EPSILON {
            colors.push([0.0, 0.0, 0.0, 0.0]);
        } else {
            let inv = 1.0 / alpha_weight;
            colors.push([
                (r * inv) / 255.0,
                (g * inv) / 255.0,
                (b * inv) / 255.0,
                (alpha_weight / sample_height as f32).clamp(0.0, 1.0),
            ]);
        }
    }

    if colors.is_empty() {
        return None;
    }

    if colors.len() == 1 {
        let mut color = colors[0];
        color[3] = 1.0;
        return Some(vec![color; MINE_GRADIENT_SAMPLES.max(1)]);
    }

    let max_index = (colors.len() - 1) as f32;
    let mut samples = Vec::with_capacity(MINE_GRADIENT_SAMPLES);
    let divisor = (MINE_GRADIENT_SAMPLES.saturating_sub(1)).max(1) as f32;
    for i in 0..MINE_GRADIENT_SAMPLES {
        let t = i as f32 / divisor;
        let position = t * max_index;
        let base_index = position.floor() as usize;
        let next_index = (base_index + 1).min(colors.len() - 1);
        let frac = (position - base_index as f32).clamp(0.0, 1.0);

        let c0 = colors[base_index];
        let c1 = colors[next_index];
        let mut sampled = [
            (c1[0] - c0[0]).mul_add(frac, c0[0]),
            (c1[1] - c0[1]).mul_add(frac, c0[1]),
            (c1[2] - c0[2]).mul_add(frac, c0[2]),
            1.0,
        ];

        sampled[0] = sampled[0].clamp(0.0, 1.0);
        sampled[1] = sampled[1].clamp(0.0, 1.0);
        sampled[2] = sampled[2].clamp(0.0, 1.0);

        samples.push(sampled);
    }

    Some(samples)
}

#[derive(Clone, Default)]
struct SlotBuilder {
    def: SpriteDefinition,
    source: Option<Arc<SpriteSource>>,
}

impl SlotBuilder {
    fn set_source(&mut self, source: Arc<SpriteSource>) {
        self.source = Some(source);
    }
}

#[derive(Clone, Copy)]
enum HoldSpritePart {
    Body,
    Bottom,
    Explosion,
}

#[derive(Clone, Default)]
struct ExplosionBuilder {
    slot: Option<SlotBuilder>,
    animation: Option<ExplosionAnimation>,
}

struct NoteskinBuilder {
    notes: Vec<SlotBuilder>,
    receptor_off: Vec<SlotBuilder>,
    receptor_glow: Vec<SlotBuilder>,
    mines: Vec<SlotBuilder>,
    mine_frames: Vec<SlotBuilder>,
    column_xs: Vec<i32>,
    defaults: HashMap<String, SpriteDefinition>,
    default_sources: HashMap<String, Arc<SpriteSource>>,
    tap_explosions: HashMap<String, ExplosionBuilder>,
    receptor_pulse: ReceptorPulse,
    hold_let_go_gray_percent: f32,
    hold_body_inactive: Option<SlotBuilder>,
    hold_body_active: Option<SlotBuilder>,
    hold_bottomcap_inactive: Option<SlotBuilder>,
    hold_bottomcap_active: Option<SlotBuilder>,
    hold_explosion: Option<SlotBuilder>,
    roll_body_inactive: Option<SlotBuilder>,
    roll_body_active: Option<SlotBuilder>,
    roll_bottomcap_inactive: Option<SlotBuilder>,
    roll_bottomcap_active: Option<SlotBuilder>,
    roll_explosion: Option<SlotBuilder>,
}

impl NoteskinBuilder {
    fn new(style: &Style) -> Self {
        let note_slots = style.num_players * style.num_cols * NUM_QUANTIZATIONS;
        let mine_slots = style.num_players * style.num_cols;
        Self {
            notes: vec![SlotBuilder::default(); note_slots],
            receptor_off: vec![SlotBuilder::default(); style.num_cols],
            receptor_glow: vec![SlotBuilder::default(); style.num_cols],
            mines: vec![SlotBuilder::default(); mine_slots],
            mine_frames: vec![SlotBuilder::default(); mine_slots],
            column_xs: (0..style.num_cols)
                .map(|i| (i as i32 * 68) - ((style.num_cols - 1) as i32 * 34))
                .collect(),
            defaults: HashMap::new(),
            default_sources: HashMap::new(),
            tap_explosions: HashMap::new(),
            receptor_pulse: ReceptorPulse::default(),
            hold_let_go_gray_percent: 0.25,
            hold_body_inactive: None,
            hold_body_active: None,
            hold_bottomcap_inactive: None,
            hold_bottomcap_active: None,
            hold_explosion: None,
            roll_body_inactive: None,
            roll_body_active: None,
            roll_bottomcap_inactive: None,
            roll_bottomcap_active: None,
            roll_explosion: None,
        }
    }

    fn finalize(self) -> Result<Noteskin, String> {
        fn finalize_slots(
            slots: Vec<SlotBuilder>,
            default_source: Option<&Arc<SpriteSource>>,
            tag: &str,
        ) -> Result<Vec<SpriteSlot>, String> {
            let mut result = Vec::with_capacity(slots.len());
            for slot in slots {
                let source = match slot.source {
                    Some(src) => src,
                    None => match default_source {
                        Some(src) => src.clone(),
                        None => {
                            return Err(format!(
                                "Noteskin missing texture assignment for category '{tag}'."
                            ));
                        }
                    },
                };
                result.push(SpriteSlot {
                    def: slot.def,
                    source,
                });
            }
            Ok(result)
        }

        fn finalize_optional_slots(
            slots: Vec<SlotBuilder>,
            default_source: Option<&Arc<SpriteSource>>,
        ) -> Vec<Option<SpriteSlot>> {
            slots
                .into_iter()
                .map(|slot| {
                    slot.source
                        .or_else(|| default_source.cloned())
                        .map(|source| SpriteSlot {
                            def: slot.def,
                            source,
                        })
                })
                .collect()
        }

        fn finalize_single_slot(
            slot: Option<SlotBuilder>,
            default_source: Option<&Arc<SpriteSource>>,
            tag: &str,
        ) -> Option<SpriteSlot> {
            slot.and_then(|slot_builder| {
                let source = match slot_builder.source {
                    Some(src) => src,
                    None => match default_source {
                        Some(src) => src.clone(),
                        None => {
                            warn!("Noteskin missing texture assignment for component '{tag}'");
                            return None;
                        }
                    },
                };
                Some(SpriteSlot {
                    def: slot_builder.def,
                    source,
                })
            })
        }

        let notes = finalize_slots(self.notes, self.default_sources.get("Note"), "Note")?;
        let receptor_off = finalize_slots(
            self.receptor_off,
            self.default_sources.get("Receptor-off"),
            "Receptor-off",
        )?;
        let receptor_glow = finalize_optional_slots(
            self.receptor_glow,
            self.default_sources.get("Receptor-glow"),
        );

        let mines = finalize_optional_slots(self.mines, self.default_sources.get("Mine"));
        let mine_frames =
            finalize_optional_slots(self.mine_frames, self.default_sources.get("Mine-frame"));
        let mine_fill_gradients = mine_fill_gradients(&mines);

        let hold_body_inactive = finalize_single_slot(
            self.hold_body_inactive,
            self.default_sources.get("Hold-body"),
            "Hold-body",
        );
        let mut hold_body_active = finalize_single_slot(
            self.hold_body_active,
            self.default_sources
                .get("Hold-body-active")
                .or_else(|| self.default_sources.get("Hold-body")),
            "Hold-body-active",
        );
        if hold_body_active.is_none() {
            hold_body_active = hold_body_inactive.clone();
        }

        let hold_bottomcap_inactive = finalize_single_slot(
            self.hold_bottomcap_inactive,
            self.default_sources.get("Hold-tail"),
            "Hold-tail",
        );
        let mut hold_bottomcap_active = finalize_single_slot(
            self.hold_bottomcap_active,
            self.default_sources
                .get("Hold-bottomcap-active")
                .or_else(|| self.default_sources.get("Hold-tail")),
            "Hold-bottomcap-active",
        );
        if hold_bottomcap_active.is_none() {
            hold_bottomcap_active = hold_bottomcap_inactive.clone();
        }

        let hold_explosion = finalize_single_slot(
            self.hold_explosion,
            self.default_sources
                .get("Hold-explosion")
                .or_else(|| self.default_sources.get("Hold-body")),
            "Hold-explosion",
        );

        let roll_body_inactive = finalize_single_slot(
            self.roll_body_inactive,
            self.default_sources.get("Roll-body"),
            "Roll-body",
        )
        .or_else(|| hold_body_inactive.clone());
        let mut roll_body_active = finalize_single_slot(
            self.roll_body_active,
            self.default_sources
                .get("Roll-body-active")
                .or_else(|| self.default_sources.get("Roll-body")),
            "Roll-body-active",
        );
        if roll_body_active.is_none() {
            roll_body_active = roll_body_inactive.clone();
        }

        let roll_bottomcap_inactive = finalize_single_slot(
            self.roll_bottomcap_inactive,
            self.default_sources.get("Roll-tail"),
            "Roll-tail",
        )
        .or_else(|| hold_bottomcap_inactive.clone());
        let mut roll_bottomcap_active = finalize_single_slot(
            self.roll_bottomcap_active,
            self.default_sources
                .get("Roll-bottomcap-active")
                .or_else(|| self.default_sources.get("Roll-tail")),
            "Roll-bottomcap-active",
        );
        if roll_bottomcap_active.is_none() {
            roll_bottomcap_active = roll_bottomcap_inactive.clone();
        }

        let roll_explosion = finalize_single_slot(
            self.roll_explosion,
            self.default_sources
                .get("Roll-explosion")
                .or_else(|| self.default_sources.get("Hold-explosion")),
            "Roll-explosion",
        )
        .or_else(|| hold_explosion.clone());

        let hold_visuals = HoldVisuals {
            body_inactive: hold_body_inactive,
            body_active: hold_body_active,
            bottomcap_inactive: hold_bottomcap_inactive,
            bottomcap_active: hold_bottomcap_active,
            explosion: hold_explosion,
        };

        let roll_visuals = HoldVisuals {
            body_inactive: roll_body_inactive,
            body_active: roll_body_active,
            bottomcap_inactive: roll_bottomcap_inactive,
            bottomcap_active: roll_bottomcap_active,
            explosion: roll_explosion,
        };

        let tap_explosions = self
            .tap_explosions
            .into_iter()
            .filter_map(|(window, builder)| {
                let slot_builder = match builder.slot {
                    Some(slot) => slot,
                    None => {
                        warn!(
                            "Noteskin missing ExplosionSheet definition for tap window '{window}'"
                        );
                        return None;
                    }
                };

                let source = match slot_builder.source {
                    Some(src) => src,
                    None => {
                        warn!(
                            "Noteskin tap explosion '{window}' missing texture assignment"
                        );
                        return None;
                    }
                };

                let animation = builder.animation.unwrap_or_else(|| {
                    warn!(
                        "Noteskin tap explosion '{window}' missing command script; using default fade"
                    );
                    ExplosionAnimation::default()
                });

                Some((
                    window,
                    TapExplosion {
                        slot: SpriteSlot {
                            def: slot_builder.def,
                            source,
                        },
                        animation,
                    },
                ))
            })
            .collect();

        let column_xs = self.column_xs;
        Ok(Noteskin {
            notes,
            receptor_off,
            receptor_glow,
            mines,
            mine_fill_gradients,
            mine_frames,
            column_xs,
            tap_explosions,
            receptor_pulse: self.receptor_pulse,
            hold_let_go_gray_percent: self.hold_let_go_gray_percent,
            hold: hold_visuals,
            roll: roll_visuals,
        })
    }
}

pub fn load(path: &Path, style: &Style) -> Result<Noteskin, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = io::BufReader::new(file);
    let noteskin_dir = path
        .parent()
        .unwrap_or(Path::new(""))
        .strip_prefix("assets/")
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .to_string();

    let mut builder = NoteskinBuilder::new(style);

    for line_result in reader.lines() {
        let owned_line = line_result.unwrap_or_default();
        let line = owned_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((tag, rest)) = line.split_once('{') {
            if let Some(content) = rest.strip_suffix('}') {
                let tag = tag.trim();
                let props = parse_properties(content);

                match tag {
                    "NoteSheet" => parse_note_sheet(&noteskin_dir, &mut builder, style, &props),
                    "MineSheet" => parse_mine_sheet(&noteskin_dir, &mut builder, style, &props),
                    "ReceptorSheet" => {
                        parse_receptor_sheet(&noteskin_dir, &mut builder, style, &props);
                    }
                    "GlowSheet" => parse_glow_sheet(&noteskin_dir, &mut builder, style, &props),
                    "ExplosionSheet" => parse_explosion_sheet(&noteskin_dir, &mut builder, &props),
                    "ExplosionCommand" => parse_explosion_command(&mut builder, &props),
                    "ReceptorPulse" => parse_receptor_pulse(&mut builder, &props),
                    "HoldBody" | "Hold-body" | "HoldHead" | "HoldBodyActive"
                    | "HoldBodyInactive" => parse_hold_component(
                        &noteskin_dir,
                        &mut builder,
                        &props,
                        false,
                        HoldSpritePart::Body,
                        props
                            .get("state")
                            .map(|s| s.trim_matches('"').to_ascii_lowercase()),
                    ),
                    "HoldBottomCap"
                    | "Hold-tail"
                    | "HoldBottomCapActive"
                    | "HoldBottomCapInactive" => parse_hold_component(
                        &noteskin_dir,
                        &mut builder,
                        &props,
                        false,
                        HoldSpritePart::Bottom,
                        props
                            .get("state")
                            .map(|s| s.trim_matches('"').to_ascii_lowercase()),
                    ),
                    "HoldExplosion" => parse_hold_component(
                        &noteskin_dir,
                        &mut builder,
                        &props,
                        false,
                        HoldSpritePart::Explosion,
                        None,
                    ),
                    "RollBody" | "Roll-body" | "RollBodyActive" | "RollBodyInactive" => {
                        parse_hold_component(
                            &noteskin_dir,
                            &mut builder,
                            &props,
                            true,
                            HoldSpritePart::Body,
                            props
                                .get("state")
                                .map(|s| s.trim_matches('"').to_ascii_lowercase()),
                        );
                    }
                    "RollBottomCap"
                    | "Roll-tail"
                    | "RollBottomCapActive"
                    | "RollBottomCapInactive" => parse_hold_component(
                        &noteskin_dir,
                        &mut builder,
                        &props,
                        true,
                        HoldSpritePart::Bottom,
                        props
                            .get("state")
                            .map(|s| s.trim_matches('"').to_ascii_lowercase()),
                    ),
                    "RollExplosion" => parse_hold_component(
                        &noteskin_dir,
                        &mut builder,
                        &props,
                        true,
                        HoldSpritePart::Explosion,
                        None,
                    ),
                    _ => parse_sprite_rule(tag, &props, style, &mut builder),
                }
            }
        } else if let Some((tag, val)) = line.split_once('=') {
            let value = val.trim().trim_matches('"');
            match tag.trim() {
                "Texture-notes" => {
                    if let Some(src) = build_atlas_source(&noteskin_dir, value) {
                        builder
                            .default_sources
                            .insert("Note".to_string(), src.clone());
                        builder
                            .default_sources
                            .insert("Hold-body".to_string(), src.clone());
                        builder
                            .default_sources
                            .insert("Hold-tail".to_string(), src.clone());
                        builder
                            .default_sources
                            .insert("Roll-body".to_string(), src.clone());
                        builder.default_sources.insert("Roll-tail".to_string(), src);
                    }
                }
                "Texture-receptors" => {
                    if let Some(src) = build_atlas_source(&noteskin_dir, value) {
                        builder
                            .default_sources
                            .insert("Receptor-off".to_string(), src.clone());
                        builder
                            .default_sources
                            .insert("Receptor-on".to_string(), src);
                    }
                }
                "Texture-glow" => {
                    if let Some(src) = build_atlas_source(&noteskin_dir, value) {
                        builder
                            .default_sources
                            .insert("Receptor-glow".to_string(), src);
                    }
                }
                "HoldLetGoGrayPercent" => match value.parse::<f32>() {
                    Ok(parsed) => {
                        builder.hold_let_go_gray_percent = parsed.clamp(0.0, 1.0);
                    }
                    Err(e) => {
                        warn!("Failed to parse HoldLetGoGrayPercent '{value}': {e}");
                    }
                },
                _ => {}
            }
        }
    }

    let noteskin = builder.finalize()?;
    info!("Loaded noteskin from: {path:?}");
    Ok(noteskin)
}

fn parse_properties(content: &str) -> HashMap<&str, &str> {
    let mut props = HashMap::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut escape_next = false;

    for (idx, ch) in content.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => {
                escape_next = true;
            }
            '"' => {
                in_quotes = !in_quotes;
            }
            ';' if !in_quotes => {
                if let Some((key, value)) = content[start..idx].split_once('=') {
                    props.insert(key.trim(), value.trim());
                }
                start = idx + 1;
            }
            _ => {}
        }
    }

    if start < content.len()
        && let Some((key, value)) = content[start..].split_once('=')
    {
        props.insert(key.trim(), value.trim());
    }

    props
}

fn parse_note_sheet(
    noteskin_dir: &str,
    builder: &mut NoteskinBuilder,
    style: &Style,
    props: &HashMap<&str, &str>,
) {
    let Some(texture) = props.get("texture").map(|s| s.trim().trim_matches('"')) else {
        warn!("NoteSheet missing texture attribute");
        return;
    };

    let source = match build_sheet_source(noteskin_dir, texture, props, 30.0) {
        Some(src) => src,
        None => return,
    };

    let quants = parse_quant_list(props).into_iter().collect::<Vec<_>>();
    if quants.is_empty() {
        warn!("NoteSheet declared without quantization list");
        return;
    }

    let players = parse_index(props.get("player"), style.num_players as u32);
    let cols = parse_index(props.get("col"), style.num_cols as u32);

    let frame_size = source
        .frame_size()
        .unwrap_or_else(|| builder.defaults.get("Note").map_or([0, 0], |d| d.size));

    for p in &players {
        if (*p as usize) >= style.num_players {
            continue;
        }
        for c in &cols {
            if (*c as usize) >= style.num_cols {
                continue;
            }
            for q in &quants {
                let idx = ((*p as usize * style.num_cols) + *c as usize) * NUM_QUANTIZATIONS
                    + *q as usize;
                if let Some(slot) = builder.notes.get_mut(idx) {
                    slot.def.size = frame_size;
                    slot.def.src = parse_src_offset(props).unwrap_or([0, 0]);
                    slot.set_source(source.clone());
                }
            }
        }
    }
}

fn parse_mine_sheet(
    noteskin_dir: &str,
    builder: &mut NoteskinBuilder,
    style: &Style,
    props: &HashMap<&str, &str>,
) {
    let Some(texture) = props.get("texture").map(|s| s.trim().trim_matches('"')) else {
        warn!("MineSheet missing texture attribute");
        return;
    };

    let source = match build_sheet_source(noteskin_dir, texture, props, 24.0) {
        Some(src) => src,
        None => return,
    };

    let layer = props.get("layer").map_or_else(
        || "fill".to_string(),
        |s| s.trim_matches('"').to_ascii_lowercase(),
    );

    let (slots, default_key, default_tag) = match layer.as_str() {
        "frame" | "overlay" | "ring" => (&mut builder.mine_frames, "Mine-frame", "Mine-frame"),
        _ => (&mut builder.mines, "Mine", "Mine"),
    };

    let frame_size = source
        .frame_size()
        .unwrap_or_else(|| builder.defaults.get(default_tag).map_or([0, 0], |d| d.size));

    builder
        .default_sources
        .entry(default_key.to_string())
        .or_insert_with(|| source.clone());

    let players = parse_index(props.get("player"), style.num_players as u32);
    let cols = parse_index(props.get("col"), style.num_cols as u32);

    for p in players {
        for c in &cols {
            let idx = (p as usize * style.num_cols) + *c as usize;
            if idx >= slots.len() {
                continue;
            }

            if let Some(slot) = slots.get_mut(idx) {
                slot.def.size = frame_size;
                slot.def.src = parse_src_offset(props).unwrap_or([0, 0]);
                slot.set_source(source.clone());
            }
        }
    }
}

fn parse_receptor_sheet(
    noteskin_dir: &str,
    builder: &mut NoteskinBuilder,
    style: &Style,
    props: &HashMap<&str, &str>,
) {
    let Some(texture) = props.get("texture").map(|s| s.trim().trim_matches('"')) else {
        warn!("ReceptorSheet missing texture attribute");
        return;
    };

    let default_state = props
        .get("state")
        .map_or_else(|| "off".to_string(), |s| s.to_ascii_lowercase());
    let source = match build_sheet_source(noteskin_dir, texture, props, 20.0) {
        Some(src) => src,
        None => return,
    };
    let frame_size = source.frame_size().unwrap_or_else(|| {
        builder
            .defaults
            .get("Receptor-off")
            .map_or([0, 0], |d| d.size)
    });

    let cols = parse_index(props.get("col"), style.num_cols as u32);
    let target = match default_state.as_str() {
        "off" => Some(&mut builder.receptor_off),
        "glow" => Some(&mut builder.receptor_glow),
        _ => {
            warn!("ReceptorSheet state '{default_state}' is not supported");
            None
        }
    };

    if let Some(slots) = target {
        for c in cols {
            if (c as usize) >= slots.len() {
                continue;
            }
            if let Some(slot) = slots.get_mut(c as usize) {
                slot.def.size = frame_size;
                slot.def.src = parse_src_offset(props).unwrap_or([0, 0]);
                slot.set_source(source.clone());
            }
        }
    }
}

fn parse_hold_component(
    noteskin_dir: &str,
    builder: &mut NoteskinBuilder,
    props: &HashMap<&str, &str>,
    is_roll: bool,
    part: HoldSpritePart,
    state: Option<String>,
) {
    let state_ref = state.as_deref();
    let slot_option = match (is_roll, part) {
        (false, HoldSpritePart::Body) => {
            if matches!(state_ref, Some("active")) {
                &mut builder.hold_body_active
            } else {
                &mut builder.hold_body_inactive
            }
        }
        (false, HoldSpritePart::Bottom) => {
            if matches!(state_ref, Some("active")) {
                &mut builder.hold_bottomcap_active
            } else {
                &mut builder.hold_bottomcap_inactive
            }
        }
        (false, HoldSpritePart::Explosion) => &mut builder.hold_explosion,
        (true, HoldSpritePart::Body) => {
            if matches!(state_ref, Some("active")) {
                &mut builder.roll_body_active
            } else {
                &mut builder.roll_body_inactive
            }
        }
        (true, HoldSpritePart::Bottom) => {
            if matches!(state_ref, Some("active")) {
                &mut builder.roll_bottomcap_active
            } else {
                &mut builder.roll_bottomcap_inactive
            }
        }
        (true, HoldSpritePart::Explosion) => &mut builder.roll_explosion,
    };

    let slot = slot_option.get_or_insert_with(SlotBuilder::default);

    let base = if is_roll { "Roll" } else { "Hold" };
    let part_key = match part {
        HoldSpritePart::Body => "body",
        HoldSpritePart::Bottom => "tail",
        HoldSpritePart::Explosion => "explosion",
    };
    let mut default_key = format!("{base}-{part_key}");
    if matches!(part, HoldSpritePart::Body | HoldSpritePart::Bottom)
        && matches!(state_ref, Some("active"))
    {
        default_key.push_str("-active");
    }

    if let Some(default_def) = builder.defaults.get(&default_key).cloned() {
        slot.def = default_def;
    }

    apply_basic_sprite_properties(slot, props);
    builder
        .defaults
        .insert(default_key.clone(), slot.def.clone());

    if let Some(texture) = props.get("texture").map(|s| s.trim().trim_matches('"')) {
        if let Some(source) = build_sheet_source(noteskin_dir, texture, props, 30.0) {
            slot.set_source(source);
        } else {
            warn!("Failed to load texture '{texture}' for {default_key} component");
        }
    } else if slot.source.is_none()
        && let Some(source) = builder.default_sources.get(&default_key)
    {
        slot.set_source(source.clone());
    }

    if slot.def.size == [0, 0]
        && let Some(source) = slot.source.as_ref()
    {
        if let Some(size) = source.frame_size() {
            slot.def.size = size;
        } else {
            let dims = source.tex_dims();
            slot.def.size = [dims.0 as i32, dims.1 as i32];
        }
    }
}

fn parse_glow_sheet(
    noteskin_dir: &str,
    builder: &mut NoteskinBuilder,
    style: &Style,
    props: &HashMap<&str, &str>,
) {
    let Some(texture) = props.get("texture").map(|s| s.trim().trim_matches('"')) else {
        warn!("GlowSheet missing texture attribute");
        return;
    };

    let source = match build_sheet_source(noteskin_dir, texture, props, 30.0) {
        Some(src) => src,
        None => return,
    };
    let frame_size = source.frame_size().unwrap_or_else(|| {
        builder
            .defaults
            .get("Receptor-glow")
            .map_or([0, 0], |d| d.size)
    });

    let cols = parse_index(props.get("col"), style.num_cols as u32);
    for c in cols {
        if (c as usize) >= builder.receptor_glow.len() {
            continue;
        }
        if let Some(slot) = builder.receptor_glow.get_mut(c as usize) {
            slot.def.size = frame_size;
            slot.def.src = parse_src_offset(props).unwrap_or([0, 0]);
            slot.set_source(source.clone());
        }
    }
}

fn parse_explosion_sheet(
    noteskin_dir: &str,
    builder: &mut NoteskinBuilder,
    props: &HashMap<&str, &str>,
) {
    let Some(texture) = props.get("texture").map(|s| s.trim().trim_matches('"')) else {
        warn!("ExplosionSheet missing texture attribute");
        return;
    };
    let Some(window) = props.get("window").map(|s| s.trim().to_ascii_uppercase()) else {
        warn!("ExplosionSheet missing window attribute");
        return;
    };

    let source = match build_sheet_source(noteskin_dir, texture, props, 30.0) {
        Some(src) => src,
        None => return,
    };

    let mut slot = SlotBuilder::default();
    slot.def.size = source.frame_size().unwrap_or([0, 0]);
    slot.def.src = parse_src_offset(props).unwrap_or([0, 0]);
    slot.set_source(source);

    builder.tap_explosions.entry(window).or_default().slot = Some(slot);
}

fn parse_explosion_command(builder: &mut NoteskinBuilder, props: &HashMap<&str, &str>) {
    let Some(window) = props.get("window").map(|s| s.trim().to_ascii_uppercase()) else {
        warn!("ExplosionCommand missing window attribute");
        return;
    };

    let Some(commands) = props.get("commands").map(|s| s.trim().trim_matches('"')) else {
        warn!("ExplosionCommand missing commands attribute for window '{window}'");
        return;
    };

    let animation = parse_explosion_animation(commands);
    builder.tap_explosions.entry(window).or_default().animation = Some(animation);
}

struct PendingSegment {
    tween: TweenType,
    duration: f32,
    start: ExplosionState,
    target_zoom: Option<f32>,
    target_color: Option<[f32; 4]>,
}

fn parse_explosion_animation(script: &str) -> ExplosionAnimation {
    let mut animation = ExplosionAnimation {
        initial: ExplosionState::default(),
        segments: Vec::new(),
        glow: None,
    };

    let mut current_state = ExplosionState::default();
    let mut initial_locked = false;
    let mut pending: Option<PendingSegment> = None;

    let finish_pending = |pending: &mut Option<PendingSegment>,
                          animation: &mut ExplosionAnimation,
                          current_state: &mut ExplosionState| {
        if let Some(segment) = pending.take() {
            let mut end_state = segment.start;
            if let Some(z) = segment.target_zoom {
                end_state.zoom = z;
            }
            if let Some(color) = segment.target_color {
                end_state.color = color;
            }

            animation.segments.push(ExplosionSegment {
                duration: segment.duration.max(0.0),
                tween: segment.tween,
                start: segment.start,
                end_zoom: segment.target_zoom,
                end_color: segment.target_color,
            });

            *current_state = end_state;
        }
    };

    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }

        let mut parts = token.split(',').map(str::trim);
        let command = parts
            .next()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let args: Vec<&str> = parts.collect();

        match command.as_str() {
            "linear" | "accelerate" => {
                finish_pending(&mut pending, &mut animation, &mut current_state);
                if let Some(arg) = args.first() {
                    if let Ok(duration) = arg.parse::<f32>() {
                        pending = Some(PendingSegment {
                            tween: if command == "linear" {
                                TweenType::Linear
                            } else {
                                TweenType::Accelerate
                            },
                            duration: duration.max(0.0),
                            start: current_state,
                            target_zoom: None,
                            target_color: None,
                        });
                        if !initial_locked {
                            animation.initial = current_state;
                            initial_locked = true;
                        }
                    } else {
                        warn!("Failed to parse duration '{arg}' for explosion command '{command}'");
                    }
                } else {
                    warn!("Explosion command '{command}' missing duration argument");
                }
            }
            "diffusealpha" => {
                if let Some(arg) = args.first() {
                    if let Ok(value) = arg.parse::<f32>() {
                        if let Some(segment) = pending.as_mut() {
                            let mut target_color =
                                segment.target_color.unwrap_or(segment.start.color);
                            target_color[3] = value;
                            segment.target_color = Some(target_color);
                        } else {
                            current_state.color[3] = value;
                            if !initial_locked {
                                animation.initial = current_state;
                            }
                        }
                    } else {
                        warn!("Failed to parse diffusealpha value '{arg}' in explosion commands");
                    }
                }
            }
            "zoom" => {
                if let Some(arg) = args.first() {
                    if let Ok(value) = arg.parse::<f32>() {
                        if let Some(segment) = pending.as_mut() {
                            segment.target_zoom = Some(value);
                        } else {
                            current_state.zoom = value;
                            if !initial_locked {
                                animation.initial = current_state;
                            }
                        }
                    } else {
                        warn!("Failed to parse zoom value '{arg}' in explosion commands");
                    }
                }
            }
            "diffuse" => {
                if args.len() >= 3 {
                    let mut parsed = [0.0f32; 4];
                    let mut ok = true;
                    for i in 0..3 {
                        if let Ok(v) = args[i].parse::<f32>() {
                            parsed[i] = v
                        } else {
                            warn!(
                                "Failed to parse diffuse component '{}' in explosion commands",
                                args[i]
                            );
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        parsed[3] = if args.len() >= 4 {
                            args[3].parse::<f32>().unwrap_or(current_state.color[3])
                        } else {
                            current_state.color[3]
                        };

                        if let Some(segment) = pending.as_mut() {
                            segment.target_color = Some(parsed);
                        } else {
                            current_state.color = parsed;
                            if !initial_locked {
                                animation.initial = current_state;
                            }
                        }
                    }
                }
            }
            "glowshift" => {
                animation.glow.get_or_insert(GlowEffect {
                    period: 0.0,
                    color1: [1.0, 1.0, 1.0, 0.0],
                    color2: [1.0, 1.0, 1.0, 0.0],
                });
            }
            "effectperiod" => {
                if let Some(arg) = args.first()
                    && let Ok(period) = arg.parse::<f32>()
                {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.period = period.max(0.0);
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: period.max(0.0),
                            color1: [1.0, 1.0, 1.0, 0.0],
                            color2: [1.0, 1.0, 1.0, 0.0],
                        });
                    }
                }
            }
            "effectcolor1" => {
                if let Some(color) = parse_color4(&args) {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.color1 = color;
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: 0.0,
                            color1: color,
                            color2: color,
                        });
                    }
                }
            }
            "effectcolor2" => {
                if let Some(color) = parse_color4(&args) {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.color2 = color;
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: 0.0,
                            color1: color,
                            color2: color,
                        });
                    }
                }
            }
            other => {
                if !other.is_empty() {
                    warn!("Unhandled explosion command '{other}'.");
                }
            }
        }
    }

    finish_pending(&mut pending, &mut animation, &mut current_state);

    if !initial_locked {
        animation.initial = current_state;
    }

    if animation.segments.is_empty() {
        animation.segments.push(ExplosionSegment {
            duration: 0.3,
            tween: TweenType::Linear,
            start: animation.initial,
            end_zoom: Some(animation.initial.zoom),
            end_color: Some([
                animation.initial.color[0],
                animation.initial.color[1],
                animation.initial.color[2],
                0.0,
            ]),
        });
    }

    animation
}

fn parse_color4(args: &[&str]) -> Option<[f32; 4]> {
    if args.is_empty() {
        return None;
    }

    let mut values = [0.0; 4];
    for (i, arg) in args.iter().enumerate().take(4) {
        values[i] = arg.parse().ok()?;
    }

    if args.len() < 4 { None } else { Some(values) }
}

fn apply_basic_sprite_properties(slot: &mut SlotBuilder, props: &HashMap<&str, &str>) {
    if let Some(src_str) = props.get("src")
        && let Some((x_str, y_str)) = src_str.split_once(',')
    {
        slot.def.src = [x_str.parse().unwrap_or(0), y_str.parse().unwrap_or(0)];
    }
    if let Some(size_str) = props.get("size")
        && let Some((w_str, h_str)) = size_str.split_once(',')
    {
        slot.def.size = [w_str.parse().unwrap_or(0), h_str.parse().unwrap_or(0)];
    }
    if let Some(rot_str) = props.get("rot") {
        slot.def.rotation_deg = rot_str.parse().unwrap_or(0);
    }
    if let Some(mirror_str) = props.get("mirror") {
        slot.def.mirror_h = mirror_str.contains('h');
        slot.def.mirror_v = mirror_str.contains('v');
    }
}

fn parse_sprite_rule(
    tag: &str,
    props: &HashMap<&str, &str>,
    style: &Style,
    builder: &mut NoteskinBuilder,
) {
    let apply_properties = |slot: &mut SlotBuilder| apply_basic_sprite_properties(slot, props);

    let has_range_spec =
        props.contains_key("row") || props.contains_key("col") || props.contains_key("player");
    if !has_range_spec {
        let mut def = builder.defaults.get(tag).cloned().unwrap_or_default();
        if let Some(src_str) = props.get("src")
            && let Some((x_str, y_str)) = src_str.split_once(',')
        {
            def.src = [x_str.parse().unwrap_or(0), y_str.parse().unwrap_or(0)];
        }
        if let Some(size_str) = props.get("size")
            && let Some((w_str, h_str)) = size_str.split_once(',')
        {
            def.size = [w_str.parse().unwrap_or(0), h_str.parse().unwrap_or(0)];
        }
        if let Some(rot_str) = props.get("rot") {
            def.rotation_deg = rot_str.parse().unwrap_or(0);
        }
        if let Some(mirror_str) = props.get("mirror") {
            def.mirror_h = mirror_str.contains('h');
            def.mirror_v = mirror_str.contains('v');
        }
        if tag == "Mine" {
            builder
                .defaults
                .entry("Mine-frame".to_string())
                .or_insert_with(|| def.clone());
        }
        builder.defaults.insert(tag.to_string(), def);
    }

    let rows = props
        .get("row")
        .and_then(|s| s.parse().ok())
        .map_or_else(|| (0..=192).collect(), |r| vec![r]);
    let cols = parse_index(props.get("col"), style.num_cols as u32);
    let players = parse_index(props.get("player"), style.num_players as u32);

    for p in &players {
        for c in &cols {
            if (*p as usize) >= style.num_players || (*c as usize) >= style.num_cols {
                continue;
            }
            match tag {
                "Note" => {
                    for r in &rows {
                        if let Some(q) = Quantization::from_row(*r) {
                            let idx = ((*p as usize * style.num_cols) + *c as usize)
                                * NUM_QUANTIZATIONS
                                + q as usize;
                            if let Some(slot) = builder.notes.get_mut(idx) {
                                apply_properties(slot);
                            }
                        }
                    }
                }
                "Receptor-off" => {
                    if let Some(slot) = builder.receptor_off.get_mut(*c as usize) {
                        apply_properties(slot);
                    }
                }
                "Receptor-glow" => {
                    if let Some(slot) = builder.receptor_glow.get_mut(*c as usize) {
                        apply_properties(slot);
                    }
                }
                "Receptor" => {
                    if let Some(x_str) = props.get("x") {
                        builder.column_xs[*c as usize] = x_str.parse().unwrap_or(0);
                    }
                }
                "Mine" => {
                    let idx = (*p as usize * style.num_cols) + *c as usize;
                    if let Some(slot) = builder.mines.get_mut(idx) {
                        apply_properties(slot);
                    }
                    if let Some(slot) = builder.mine_frames.get_mut(idx) {
                        apply_properties(slot);
                    }
                }
                _ => {}
            }
        }
    }
}

fn parse_quant_list(props: &HashMap<&str, &str>) -> Vec<Quantization> {
    if let Some(list) = props.get("quants").or_else(|| props.get("quant")) {
        list.split([',', ' ', ';'])
            .filter_map(|part| part.trim().parse::<u32>().ok())
            .filter_map(Quantization::from_row)
            .collect()
    } else if let Some(row) = props.get("row").and_then(|s| s.parse::<u32>().ok()) {
        Quantization::from_row(row).into_iter().collect()
    } else {
        Vec::new()
    }
}

fn parse_index(spec: Option<&&str>, max: u32) -> Vec<u32> {
    match spec {
        Some(value) => value
            .split(',')
            .filter_map(|v| v.trim().parse::<u32>().ok())
            .collect::<Vec<_>>(),
        None => (0..max).collect(),
    }
}

fn parse_src_offset(props: &HashMap<&str, &str>) -> Option<[i32; 2]> {
    props.get("offset").and_then(|s| {
        s.split_once(',')
            .map(|(x, y)| [x.trim().parse().unwrap_or(0), y.trim().parse().unwrap_or(0)])
    })
}

fn build_atlas_source(noteskin_dir: &str, texture: &str) -> Option<Arc<SpriteSource>> {
    let key = resolve_texture_key(noteskin_dir, texture);
    let dims = texture_dimensions(&key)?;
    Some(Arc::new(SpriteSource::Atlas {
        texture_key: key,
        tex_dims: dims,
    }))
}

fn build_sheet_source(
    noteskin_dir: &str,
    texture: &str,
    props: &HashMap<&str, &str>,
    default_fps: f32,
) -> Option<Arc<SpriteSource>> {
    let key = resolve_texture_key(noteskin_dir, texture);
    let dims = texture_dimensions(&key)?;
    let fps = props
        .get("fps")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(default_fps);
    let grid = props
        .get("grid")
        .and_then(|g| parse_pair_usize(g))
        .unwrap_or_else(|| infer_grid(&key));
    let frame_size = props
        .get("frame_size")
        .and_then(|s| parse_pair_i32(s))
        .unwrap_or_else(|| infer_frame_size(dims, grid));
    let frames = props
        .get("frames")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| grid.0 * grid.1);
    let beats_per_loop = props
        .get("beats_per_loop")
        .or_else(|| props.get("loop_beats"))
        .or_else(|| props.get("beat_length"))
        .or_else(|| props.get("animation_length"))
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| *v > 0.0);
    let beat_based_flag = props
        .get("AnimationIsBeatBased")
        .or_else(|| props.get("animationisbeatbased"))
        .or_else(|| props.get("animation_is_beat_based"))
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "True" | "TRUE" | "yes" | "Yes"));
    let beat_based = beat_based_flag || beats_per_loop.is_some();
    let frame_count = frames.max(1);
    let rate = if beat_based {
        let frames_per_beat = beats_per_loop
            .map_or(fps, |beats| {
                if beats == 0.0 {
                    0.0
                } else {
                    frame_count as f32 / beats
                }
            })
            .max(0.0);
        AnimationRate::FramesPerBeat(frames_per_beat)
    } else {
        AnimationRate::FramesPerSecond(fps.max(0.0))
    };

    Some(Arc::new(SpriteSource::Animated {
        texture_key: key,
        tex_dims: dims,
        frame_size,
        grid,
        frame_count,
        rate,
    }))
}

fn resolve_texture_key(base: &str, texture: &str) -> String {
    let mut key = if texture.contains('/') {
        texture.to_string()
    } else if base.is_empty() {
        texture.to_string()
    } else {
        format!("{base}/{texture}")
    };
    key = key.replace('\\', "/");
    key
}

fn texture_dimensions(key: &str) -> Option<(u32, u32)> {
    if let Some(meta) = assets::texture_dims(key) {
        return Some((meta.w, meta.h));
    }
    let path = PathBuf::from("assets").join(key);
    image_dimensions(&path).ok()
}

fn parse_pair_usize(input: &str) -> Option<(usize, usize)> {
    input
        .split_once(',')
        .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))
}

fn parse_pair_i32(input: &str) -> Option<[i32; 2]> {
    input
        .split_once(',')
        .and_then(|(a, b)| Some([a.trim().parse().ok()?, b.trim().parse().ok()?]))
}

fn infer_grid(texture_key: &str) -> (usize, usize) {
    let (w, h) = assets::sprite_sheet_dims(texture_key);
    (w.max(1) as usize, h.max(1) as usize)
}

fn infer_frame_size(dims: (u32, u32), grid: (usize, usize)) -> [i32; 2] {
    let width = if grid.0 > 0 {
        (dims.0 / grid.0 as u32) as i32
    } else {
        dims.0 as i32
    };
    let height = if grid.1 > 0 {
        (dims.1 / grid.1 as u32) as i32
    } else {
        dims.1 as i32
    };
    [width.max(1), height.max(1)]
}

fn parse_color_rgba(input: &str) -> Option<[f32; 4]> {
    let mut components = input
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>();

    if components.is_empty() {
        return None;
    }

    if components.len() == 3 {
        components.push("1.0");
    }

    if components.len() != 4 {
        return None;
    }

    let mut color = [0.0; 4];
    for (i, component) in components.iter().enumerate().take(4) {
        color[i] = component.parse::<f32>().ok()?;
    }
    Some(color)
}

fn parse_receptor_pulse(builder: &mut NoteskinBuilder, props: &HashMap<&str, &str>) {
    fn parse_non_negative(value: Option<&&str>) -> Option<f32> {
        value
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v.max(0.0))
    }

    let mut pulse = builder.receptor_pulse;

    if let Some(color) = props
        .get("effect_color1")
        .or_else(|| props.get("base_color"))
        .or_else(|| props.get("base"))
        .and_then(|v| parse_color_rgba(v))
    {
        pulse.effect_color1 = color;
    }

    if let Some(color) = props
        .get("effect_color2")
        .or_else(|| props.get("beat_color"))
        .or_else(|| props.get("bright_color"))
        .and_then(|v| parse_color_rgba(v))
    {
        pulse.effect_color2 = color;
    }

    if let Some(offset) = props
        .get("effect_offset")
        .or_else(|| props.get("offset"))
        .and_then(|v| v.parse::<f32>().ok())
    {
        pulse.effect_offset = offset;
    }

    if let Some(timing) = props
        .get("effect_timing")
        .or_else(|| props.get("timing"))
        .and_then(|v| {
            let values: Vec<_> = v
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        })
    {
        if let Some(value) = timing.first() {
            pulse.ramp_to_half = value.max(0.0);
        }
        if let Some(value) = timing.get(1) {
            pulse.hold_at_half = value.max(0.0);
        }
        if let Some(value) = timing.get(2) {
            pulse.ramp_to_full = value.max(0.0);
        }
        if let Some(value) = timing.get(3) {
            pulse.hold_at_full = value.max(0.0);
        }
        if let Some(value) = timing.get(4) {
            pulse.hold_at_zero = value.max(0.0);
        }
    }

    if let Some(value) = parse_non_negative(props.get("ramp_to_half")) {
        pulse.ramp_to_half = value;
    }
    if let Some(value) = parse_non_negative(props.get("hold_at_half")) {
        pulse.hold_at_half = value;
    }
    if let Some(value) = parse_non_negative(props.get("ramp_to_full")) {
        pulse.ramp_to_full = value;
    }
    if let Some(value) = parse_non_negative(props.get("hold_at_full")) {
        pulse.hold_at_full = value;
    }
    if let Some(value) = parse_non_negative(props.get("hold_at_zero")) {
        pulse.hold_at_zero = value;
    }

    builder.receptor_pulse = pulse;
}
