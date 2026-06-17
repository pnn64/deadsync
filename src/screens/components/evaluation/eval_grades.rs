use crate::act;
use crate::assets;
use deadlib_platform::dirs;
use deadlib_present::actors::Actor;
use deadlib_render::SamplerDesc;
use deadsync_rules::judgment::{self, JudgeGrade};
use deadsync_score as score_data;
use image::{Rgba, RgbaImage};
use std::path::Path;

const DEFAULT_EVAL_ZOOM: f32 = 0.4;
const LETTER_ZOOM: f32 = 0.85;
const STAR_TEX: &str = "grades/star.png";
const AFFLUENT_TEX: &str = "grades/affluent.png";
const GOLDSTAR_TEX: &str = "grades/goldstar (stretch).png";

// Evaluation-only generated texture cache. The global asset registry owns uploads;
// this path lazily builds at most 180 two-degree face-rotation buckets from bundled
// grade art, outside gameplay, and does not evict during the process lifetime.
const AFFLUENT_CLIP_KEY_PREFIX: &str = "__eval_grade_affluent_star_clip";

const STAR_PULSE_PERIOD_S: f32 = 0.80;
const STAR_PULSE_AMP: f32 = 0.06;
const STAR_RAINBOW_DELAY_S: f32 = 2.0;
const TAUNT_APPEAR_DELAY_S: f32 = 9.0;
const AFFLUENT_FADE_S: f32 = 3.0;
const AFFLUENT_ALPHA_MAX: f32 = 0.7;
const AFFLUENT_OFFSET_Y: f32 = 10.0;
const AFFLUENT_ZOOM: f32 = 1.2;
const AFFLUENT_ROT_BUCKET_DEG: f32 = 2.0;
const AFFLUENT_ROT_BUCKETS: u32 = (360.0 / AFFLUENT_ROT_BUCKET_DEG) as u32;
const AFFLUENT_SPIN_DELAY_S: f32 = 14.0;
const GOLDSTAR_ANIM_DELAY_S: f32 = 2.0;
const GOLDSTAR_ZOOM_OUT_S: f32 = 3.0;
const GOLDSTAR_ZOOM_BACK_S: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GradeStarTaunt {
    excellent: u32,
    great: u32,
    decent: u32,
    way_off: u32,
    miss: u32,
}

#[inline(always)]
pub fn grade_star_taunt_from_counts(counts: judgment::JudgeCounts) -> GradeStarTaunt {
    GradeStarTaunt {
        excellent: counts[judgment::judge_grade_ix(JudgeGrade::Excellent)],
        great: counts[judgment::judge_grade_ix(JudgeGrade::Great)],
        decent: counts[judgment::judge_grade_ix(JudgeGrade::Decent)],
        way_off: counts[judgment::judge_grade_ix(JudgeGrade::WayOff)],
        miss: counts[judgment::judge_grade_ix(JudgeGrade::Miss)],
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EvalGradeParams {
    pub x: f32,
    pub y: f32,
    pub z: i16,
    pub zoom: f32,
    pub elapsed: f32,
    pub taunt: GradeStarTaunt,
    pub easter_eggs: bool,
}

impl Default for EvalGradeParams {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0,
            zoom: DEFAULT_EVAL_ZOOM,
            elapsed: 0.0,
            taunt: GradeStarTaunt::default(),
            easter_eggs: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct StarDef {
    dx: f32,
    dy: f32,
    zoom: f32,
    pulse_offset: f32,
    mag_x: f32,
    mag_y: f32,
    spin_delay_s: f32,
    spin_seed: u32,
}

const NO_SPIN_DELAY_S: f32 = 1.0e9;

// Ported from:
// `itgmania/Themes/Simply Love/Graphics/_grades/Grade_Tier00.lua`
const STARS_QUINT: [StarDef; 5] = [
    StarDef {
        dx: 0.0,
        dy: -54.0,
        zoom: 0.35,
        pulse_offset: 0.0,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: 5.0,
        spin_seed: 0x51A7_0000,
    },
    StarDef {
        dx: -52.0,
        dy: -16.0,
        zoom: 0.35,
        pulse_offset: 0.0,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: 60.0,
        spin_seed: 0x51A7_0001,
    },
    StarDef {
        dx: 52.0,
        dy: -16.0,
        zoom: 0.35,
        pulse_offset: 0.2,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: 3.0,
        spin_seed: 0x51A7_0002,
    },
    StarDef {
        dx: -32.0,
        dy: 50.0,
        zoom: 0.35,
        pulse_offset: 0.4,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: 11.0,
        spin_seed: 0x51A7_0003,
    },
    StarDef {
        dx: 32.0,
        dy: 50.0,
        zoom: 0.35,
        pulse_offset: 0.6,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: 48.0,
        spin_seed: 0x51A7_0004,
    },
];

// Ported from:
// `itgmania/Themes/Simply Love/Graphics/_grades/Grade_Tier01.lua`
const STARS_TIER01: [StarDef; 4] = [
    StarDef {
        dx: -46.0,
        dy: -46.0,
        zoom: 0.5,
        pulse_offset: 0.0,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: 60.0,
        spin_seed: 0x57A2_0000,
    },
    StarDef {
        dx: 46.0,
        dy: -46.0,
        zoom: 0.5,
        pulse_offset: 0.2,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: 3.0,
        spin_seed: 0x57A2_0001,
    },
    StarDef {
        dx: -46.0,
        dy: 46.0,
        zoom: 0.5,
        pulse_offset: 0.4,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: 11.0,
        spin_seed: 0x57A2_0002,
    },
    StarDef {
        dx: 46.0,
        dy: 46.0,
        zoom: 0.5,
        pulse_offset: 0.6,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: 48.0,
        spin_seed: 0x57A2_0003,
    },
];

// Ported from:
// `itgmania/Themes/Simply Love/Graphics/_grades/Grade_Tier02.lua`
const STARS_TIER02: [StarDef; 3] = [
    StarDef {
        dx: -45.0,
        dy: 40.0,
        zoom: 0.5,
        pulse_offset: 0.0,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: NO_SPIN_DELAY_S,
        spin_seed: 0,
    },
    StarDef {
        dx: 0.0,
        dy: -40.0,
        zoom: 0.5,
        pulse_offset: 0.2,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: NO_SPIN_DELAY_S,
        spin_seed: 0,
    },
    StarDef {
        dx: 45.0,
        dy: 40.0,
        zoom: 0.5,
        pulse_offset: 0.4,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: NO_SPIN_DELAY_S,
        spin_seed: 0,
    },
];

// Ported from:
// `itgmania/Themes/Simply Love/Graphics/_grades/Grade_Tier03.lua`
const STARS_TIER03: [StarDef; 2] = [
    StarDef {
        dx: -39.0,
        dy: 40.0,
        zoom: 0.6,
        pulse_offset: 0.0,
        mag_x: 1.0,
        mag_y: 0.9,
        spin_delay_s: NO_SPIN_DELAY_S,
        spin_seed: 0,
    },
    StarDef {
        dx: 39.0,
        dy: -40.0,
        zoom: 0.6,
        pulse_offset: 0.2,
        mag_x: 0.9,
        mag_y: 1.0,
        spin_delay_s: NO_SPIN_DELAY_S,
        spin_seed: 0,
    },
];

// Ported from:
// `itgmania/Themes/Simply Love/Graphics/_grades/Grade_Tier04.lua`
const STARS_TIER04: [StarDef; 1] = [StarDef {
    dx: 0.0,
    dy: 0.0,
    zoom: 0.8,
    pulse_offset: 0.0,
    mag_x: 1.0,
    mag_y: 0.9,
    spin_delay_s: NO_SPIN_DELAY_S,
    spin_seed: 0,
}];

#[inline(always)]
fn letter_tex(grade: score_data::Grade) -> &'static str {
    match grade {
        score_data::Grade::Tier05 => "grades/s-plus.png",
        score_data::Grade::Tier06 => "grades/s.png",
        score_data::Grade::Tier07 => "grades/s-minus.png",
        score_data::Grade::Tier08 => "grades/a-plus.png",
        score_data::Grade::Tier09 => "grades/a.png",
        score_data::Grade::Tier10 => "grades/a-minus.png",
        score_data::Grade::Tier11 => "grades/b-plus.png",
        score_data::Grade::Tier12 => "grades/b.png",
        score_data::Grade::Tier13 => "grades/b-minus.png",
        score_data::Grade::Tier14 => "grades/c-plus.png",
        score_data::Grade::Tier15 => "grades/c.png",
        score_data::Grade::Tier16 => "grades/c-minus.png",
        score_data::Grade::Tier17 => "grades/d.png",
        score_data::Grade::Failed => "grades/f.png",
        _ => "grades/f.png",
    }
}

#[inline(always)]
fn pulse_scales(base: f32, elapsed: f32, offset: f32, mx: f32, my: f32) -> (f32, f32) {
    let t = if STAR_PULSE_PERIOD_S > 0.0 {
        elapsed / STAR_PULSE_PERIOD_S
    } else {
        0.0
    };
    let phase = (t + offset) * std::f32::consts::TAU;
    let s = phase.sin();
    (
        base * (1.0 + STAR_PULSE_AMP * mx * s),
        base * (1.0 + STAR_PULSE_AMP * my * s),
    )
}

#[inline(always)]
fn mix_u32(mut x: u32) -> u32 {
    // MurmurHash3 finalizer-ish; deterministic and fast.
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

#[inline(always)]
fn spin_cycle(seed: u32, cycle: u32) -> (u32, f32) {
    let a = mix_u32(seed ^ cycle.wrapping_mul(0x9E37_79B9));
    let b = mix_u32(seed ^ cycle.wrapping_mul(0x85EB_CA6B) ^ 0xC2B2_AE35);

    // Mirror the Lua-ish distribution: r = min(random(3..51), 36).
    let raw = 3 + (a % 49); // [3..51]
    let r = raw.min(36);

    // sleep = random()*7 + 1  => (1..8)
    let frac = b as f32 / u32::MAX as f32;
    let sleep_s = 7.0_f32.mul_add(frac, 1.0);
    (r, sleep_s)
}

#[inline(always)]
fn spin_rot_deg(elapsed: f32, first_delay_s: f32, seed: u32) -> f32 {
    if first_delay_s >= NO_SPIN_DELAY_S {
        return 0.0;
    }
    let mut t = elapsed - first_delay_s;
    if t <= 0.0 {
        return 0.0;
    }

    // In Simply Love's `star.lua`, `Spin()` stores a 0..36 counter in the actor's Z,
    // then rotates by `z*10` degrees. We emulate that with a deterministic schedule.
    let mut z_steps: u32 = 0;
    for cycle in 0..256u32 {
        if z_steps >= 36 {
            z_steps -= 36;
        }
        let (r, sleep_s) = spin_cycle(seed, cycle);
        let spin_dur = r as f32 / 36.0;

        if t <= spin_dur {
            let start = z_steps as f32 * 10.0;
            let end = (z_steps + r) as f32 * 10.0;
            let p = (t / spin_dur).clamp(0.0, 1.0);
            return (end - start).mul_add(p, start);
        }
        t -= spin_dur;
        z_steps = z_steps.saturating_add(r);

        if t <= sleep_s {
            return z_steps as f32 * 10.0;
        }
        t -= sleep_s;
    }

    z_steps as f32 * 10.0
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn one_w2_taunt(t: GradeStarTaunt) -> bool {
    t.miss == 0 && t.way_off == 0 && t.decent == 0 && t.great == 0 && t.excellent == 1
}

#[inline(always)]
fn one_w3_flag(t: GradeStarTaunt) -> bool {
    t.miss == 0 && t.way_off == 0 && t.decent == 0 && t.great == 1
}

#[inline(always)]
fn star_seed(s: StarDef) -> u32 {
    if s.spin_seed != 0 {
        s.spin_seed
    } else {
        mix_u32(s.dx.to_bits() ^ s.dy.to_bits().rotate_left(11) ^ s.zoom.to_bits().rotate_left(21))
    }
}

#[inline(always)]
fn unit_rand(seed: u32, salt: u32) -> f32 {
    mix_u32(seed ^ salt) as f32 / u32::MAX as f32
}

#[inline(always)]
fn rainbow_rgba(elapsed: f32, seed: u32) -> [f32; 4] {
    let period = 2.0 + 2.0 * unit_rand(seed, 0xA11C_0DE1);
    let pct = ((elapsed - STAR_RAINBOW_DELAY_S) / period).rem_euclid(1.0);
    let between = ((pct + 0.25) * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    let phase = between * std::f32::consts::TAU;
    [
        phase.cos() * 0.5 + 0.5,
        (phase + std::f32::consts::TAU / 3.0).cos() * 0.5 + 0.5,
        (phase + 2.0 * std::f32::consts::TAU / 3.0).cos() * 0.5 + 0.5,
        1.0,
    ]
}

#[inline(always)]
fn accel(t: f32) -> f32 {
    t * t
}

#[inline(always)]
fn decel(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

#[inline(always)]
fn affluent_rot_deg(elapsed: f32, seed: u32) -> f32 {
    let start = unit_rand(seed, 0xAFF1_0001) * 360.0;
    if elapsed < AFFLUENT_SPIN_DELAY_S {
        return start;
    }

    let half_s = 0.6 + 0.4 * unit_rand(seed, 0xAFF1_0002);
    let cycle_s = half_s * 2.0;
    let t = (elapsed - AFFLUENT_SPIN_DELAY_S).rem_euclid(cycle_s);
    if t < half_s {
        start - 180.0 * accel(t / half_s)
    } else {
        start - 180.0 - 180.0 * decel((t - half_s) / half_s)
    }
}

#[inline(always)]
fn goldstar_zoom(elapsed: f32) -> f32 {
    let t = elapsed - GOLDSTAR_ANIM_DELAY_S;
    if t <= 0.0 {
        1.0
    } else if t <= GOLDSTAR_ZOOM_OUT_S {
        lerp(1.0, 5.0, t / GOLDSTAR_ZOOM_OUT_S)
    } else if t <= GOLDSTAR_ZOOM_OUT_S + GOLDSTAR_ZOOM_BACK_S {
        lerp(5.0, 1.6, (t - GOLDSTAR_ZOOM_OUT_S) / GOLDSTAR_ZOOM_BACK_S)
    } else {
        1.6
    }
}

#[inline(always)]
fn goldstar_wag_deg(elapsed: f32) -> f32 {
    let t = elapsed - GOLDSTAR_ANIM_DELAY_S;
    if t <= 0.0 {
        0.0
    } else {
        ((t / 2.0).rem_euclid(1.0) * std::f32::consts::TAU).sin() * 20.0
    }
}

#[derive(Clone, Copy)]
struct StarTransform {
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    rot: f32,
}

#[inline(always)]
fn star_transform(s: StarDef, p: EvalGradeParams) -> StarTransform {
    let x = p.x + s.dx * p.zoom;
    let y = p.y + s.dy * p.zoom;
    let base = s.zoom * p.zoom;
    let (sx, sy) = pulse_scales(base, p.elapsed, s.pulse_offset, s.mag_x, s.mag_y);
    let rot = spin_rot_deg(p.elapsed, s.spin_delay_s, s.spin_seed);
    StarTransform { x, y, sx, sy, rot }
}

#[inline(always)]
fn child_xy(st: StarTransform, x: f32, y: f32) -> (f32, f32) {
    let r = st.rot.to_radians();
    let lx = x * st.sx;
    let ly = y * st.sy;
    (
        st.x + lx * r.cos() - ly * r.sin(),
        st.y + lx * r.sin() + ly * r.cos(),
    )
}

#[inline(always)]
fn base_star_actor(st: StarTransform, p: EvalGradeParams, seed: u32) -> Actor {
    let tint = if one_w2_taunt(p.taunt) && p.elapsed >= STAR_RAINBOW_DELAY_S {
        rainbow_rgba(p.elapsed, seed)
    } else {
        [1.0, 1.0, 1.0, 1.0]
    };

    act!(sprite_static(STAR_TEX):
        align(0.5, 0.5):
        xy(st.x, st.y):
        zoomx(st.sx):
        zoomy(st.sy):
        rotationz(st.rot):
        z(p.z):
        diffuse(tint[0], tint[1], tint[2], tint[3])
    )
}

#[inline(always)]
fn affluent_actor(st: StarTransform, p: EvalGradeParams, seed: u32) -> Option<Actor> {
    if !p.easter_eggs || !one_w2_taunt(p.taunt) || p.elapsed < TAUNT_APPEAR_DELAY_S {
        return None;
    }

    let alpha =
        ((p.elapsed - TAUNT_APPEAR_DELAY_S) / AFFLUENT_FADE_S).clamp(0.0, 1.0) * AFFLUENT_ALPHA_MAX;
    let face_rot = affluent_rot_deg(p.elapsed, seed);
    if p.elapsed < TAUNT_APPEAR_DELAY_S + AFFLUENT_FADE_S {
        return Some(raw_affluent_actor(st, p.z, alpha, face_rot));
    }

    if let Some(key) = clipped_affluent_texture(face_rot) {
        Some(act!(sprite(key):
            align(0.5, 0.5):
            xy(st.x, st.y):
            zoomx(st.sx):
            zoomy(st.sy):
            rotationz(st.rot):
            z(p.z):
            diffuse(1.0, 1.0, 1.0, alpha)
        ))
    } else {
        Some(raw_affluent_actor(st, p.z, alpha, face_rot))
    }
}

fn raw_affluent_actor(st: StarTransform, z: i16, alpha: f32, face_rot: f32) -> Actor {
    let rot = st.rot + face_rot;
    let (x, y) = child_xy(st, 0.0, AFFLUENT_OFFSET_Y);
    act!(sprite_static(AFFLUENT_TEX):
        align(0.5, 0.5):
        xy(x, y):
        zoomx(st.sx * AFFLUENT_ZOOM):
        zoomy(st.sy * AFFLUENT_ZOOM):
        rotationz(rot):
        z(z):
        diffuse(1.0, 1.0, 1.0, alpha)
    )
}

fn clipped_affluent_texture(rot_deg: f32) -> Option<String> {
    let bucket = affluent_rot_bucket(rot_deg);
    let key = format!("{AFFLUENT_CLIP_KEY_PREFIX}_{bucket:03}");
    if assets::texture_dims(&key).is_some() {
        return Some(key);
    }

    let rot = bucket as f32 * AFFLUENT_ROT_BUCKET_DEG;
    let image = build_clipped_affluent_texture(rot)?;
    assets::register_generated_texture(&key, image, SamplerDesc::default());
    Some(key)
}

#[inline(always)]
fn affluent_rot_bucket(rot_deg: f32) -> u32 {
    let bucket = (rot_deg.rem_euclid(360.0) / AFFLUENT_ROT_BUCKET_DEG).round() as u32;
    bucket % AFFLUENT_ROT_BUCKETS
}

fn load_grade_rgba(key: &str) -> Option<RgbaImage> {
    let path = Path::new("assets").join("graphics").join(key);
    let path = dirs::app_dirs().resolve_asset_path(&path.to_string_lossy());
    assets::open_image_fallback(&path)
        .map(|img| img.to_rgba8())
        .ok()
}

fn build_clipped_affluent_texture(rot_deg: f32) -> Option<RgbaImage> {
    let star = load_grade_rgba(STAR_TEX)?;
    let face = load_grade_rgba(AFFLUENT_TEX)?;
    let (w, h) = (star.width(), star.height());
    let mut out = RgbaImage::new(w, h);
    let face_cx = face.width() as f32 * 0.5;
    let face_cy = face.height() as f32 * 0.5;
    let out_cx = w as f32 * 0.5;
    let out_cy = h as f32 * 0.5;
    let rot = -rot_deg.to_radians();
    let (sin, cos) = rot.sin_cos();

    for y in 0..h {
        for x in 0..w {
            let mask_a = star.get_pixel(x, y)[3];
            if mask_a == 0 {
                continue;
            }

            let lx = x as f32 + 0.5 - out_cx;
            let ly = y as f32 + 0.5 - out_cy - AFFLUENT_OFFSET_Y;
            let rx = (lx * cos - ly * sin) / AFFLUENT_ZOOM + face_cx;
            let ry = (lx * sin + ly * cos) / AFFLUENT_ZOOM + face_cy;
            let Some(src) = sample_rgba_bilinear(&face, rx, ry) else {
                continue;
            };

            let a = ((u16::from(src[3]) * u16::from(mask_a)) / 255) as u8;
            out.put_pixel(x, y, Rgba([src[0], src[1], src[2], a]));
        }
    }
    Some(out)
}

fn sample_rgba_bilinear(img: &RgbaImage, x: f32, y: f32) -> Option<[u8; 4]> {
    if x < 0.0
        || y < 0.0
        || x > img.width().saturating_sub(1) as f32
        || y > img.height().saturating_sub(1) as f32
    {
        return None;
    }

    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(img.width() - 1);
    let y1 = (y0 + 1).min(img.height() - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = img.get_pixel(x0, y0);
    let p10 = img.get_pixel(x1, y0);
    let p01 = img.get_pixel(x0, y1);
    let p11 = img.get_pixel(x1, y1);
    let mut out = [0; 4];

    for i in 0..4 {
        let top = (p10[i] as f32 - p00[i] as f32).mul_add(fx, p00[i] as f32);
        let bottom = (p11[i] as f32 - p01[i] as f32).mul_add(fx, p01[i] as f32);
        out[i] = (bottom - top).mul_add(fy, top).round().clamp(0.0, 255.0) as u8;
    }
    Some(out)
}

#[inline(always)]
fn goldstar_actor(st: StarTransform, p: EvalGradeParams) -> Option<Actor> {
    if !p.easter_eggs
        || p.taunt.miss > 0
        || p.taunt.way_off > 0
        || p.taunt.decent > 0
        || p.taunt.great > 1
    {
        return None;
    }

    let one_w2 = p.taunt.great == 0 && p.taunt.excellent == 1;
    if one_w3_flag(p.taunt) {
        return Some(act!(sprite(GOLDSTAR_TEX):
            align(0.5, 0.5):
            xy(st.x, st.y):
            zoomx(st.sx):
            zoomy(st.sy):
            rotationz(st.rot):
            z(p.z.saturating_add(1)):
            diffuse(0.0, 0.0, 0.0, 1.0)
        ));
    }
    if !one_w2 {
        return None;
    }

    let zoom = goldstar_zoom(p.elapsed);
    let wag = goldstar_wag_deg(p.elapsed);
    if p.elapsed >= GOLDSTAR_ANIM_DELAY_S + GOLDSTAR_ZOOM_OUT_S + GOLDSTAR_ZOOM_BACK_S {
        Some(act!(sprite(GOLDSTAR_TEX):
            align(0.5, 0.5):
            xy(st.x, st.y):
            zoomx(st.sx * zoom):
            zoomy(st.sy * zoom):
            rotationz(st.rot + wag):
            z(p.z.saturating_add(1)):
            diffuse(1.0, 1.0, 1.0, 1.0):
            texcoordvelocity(1.0, 0.0)
        ))
    } else if p.elapsed >= GOLDSTAR_ANIM_DELAY_S {
        Some(act!(sprite(GOLDSTAR_TEX):
            align(0.5, 0.5):
            xy(st.x, st.y):
            zoomx(st.sx * zoom):
            zoomy(st.sy * zoom):
            rotationz(st.rot + wag):
            z(p.z.saturating_add(1)):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ))
    } else {
        Some(act!(sprite(GOLDSTAR_TEX):
            align(0.5, 0.5):
            xy(st.x, st.y):
            zoomx(st.sx):
            zoomy(st.sy):
            rotationz(st.rot):
            z(p.z.saturating_add(1)):
            diffuse(0.0, 0.0, 0.0, 1.0)
        ))
    }
}

#[inline(always)]
fn star_actors(s: StarDef, p: EvalGradeParams) -> Vec<Actor> {
    let st = star_transform(s, p);
    let seed = star_seed(s);
    let mut out = Vec::with_capacity(4);
    out.push(base_star_actor(st, p, seed));
    if let Some(actor) = affluent_actor(st, p, seed) {
        out.push(actor);
    }
    if let Some(actor) = goldstar_actor(st, p) {
        out.push(actor);
    }
    out
}

#[inline(always)]
fn stars_for(grade: score_data::Grade) -> Option<&'static [StarDef]> {
    match grade {
        score_data::Grade::Quint => Some(&STARS_QUINT),
        score_data::Grade::Tier01 => Some(&STARS_TIER01),
        score_data::Grade::Tier02 => Some(&STARS_TIER02),
        score_data::Grade::Tier03 => Some(&STARS_TIER03),
        score_data::Grade::Tier04 => Some(&STARS_TIER04),
        _ => None,
    }
}

pub fn actors(grade: score_data::Grade, p: EvalGradeParams) -> Vec<Actor> {
    if let Some(stars) = stars_for(grade) {
        return stars
            .iter()
            .copied()
            .flat_map(|s| star_actors(s, p))
            .collect();
    }

    let tex = letter_tex(grade);
    vec![act!(sprite_static(tex):
        align(0.5, 0.5):
        xy(p.x, p.y):
        zoom(p.zoom * LETTER_ZOOM):
        z(p.z)
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(
        excellent: u32,
        great: u32,
        decent: u32,
        way_off: u32,
        miss: u32,
    ) -> judgment::JudgeCounts {
        let mut counts = [0; judgment::JUDGE_GRADE_COUNT];
        counts[judgment::judge_grade_ix(JudgeGrade::Excellent)] = excellent;
        counts[judgment::judge_grade_ix(JudgeGrade::Great)] = great;
        counts[judgment::judge_grade_ix(JudgeGrade::Decent)] = decent;
        counts[judgment::judge_grade_ix(JudgeGrade::WayOff)] = way_off;
        counts[judgment::judge_grade_ix(JudgeGrade::Miss)] = miss;
        counts
    }

    fn texture_count(actors: &[Actor], key: &str) -> usize {
        actors
            .iter()
            .filter(|actor| match actor {
                Actor::Sprite { source, .. } => source.texture_key() == Some(key),
                _ => false,
            })
            .count()
    }

    fn texture_prefix_count(actors: &[Actor], prefix: &str) -> usize {
        actors
            .iter()
            .filter(|actor| match actor {
                Actor::Sprite { source, .. } => source
                    .texture_key()
                    .is_some_and(|key| key.starts_with(prefix)),
                _ => false,
            })
            .count()
    }

    fn first_tint(actors: &[Actor], key: &str) -> Option<[f32; 4]> {
        actors.iter().find_map(|actor| match actor {
            Actor::Sprite { source, tint, .. } if source.texture_key() == Some(key) => Some(*tint),
            _ => None,
        })
    }

    #[test]
    fn exact_one_w2_adds_late_taunt_actors() {
        let actors = actors(
            score_data::Grade::Tier04,
            EvalGradeParams {
                elapsed: 10.0,
                taunt: grade_star_taunt_from_counts(counts(1, 0, 0, 0, 0)),
                ..Default::default()
            },
        );

        assert_eq!(texture_count(&actors, STAR_TEX), 1);
        assert_eq!(texture_count(&actors, AFFLUENT_TEX), 1);
        assert_eq!(texture_prefix_count(&actors, AFFLUENT_CLIP_KEY_PREFIX), 0);
        assert_eq!(texture_count(&actors, GOLDSTAR_TEX), 1);
    }

    #[test]
    fn exact_one_w2_uses_clipped_affluent_after_fade() {
        let actors = actors(
            score_data::Grade::Tier04,
            EvalGradeParams {
                elapsed: 12.0,
                taunt: grade_star_taunt_from_counts(counts(1, 0, 0, 0, 0)),
                ..Default::default()
            },
        );

        assert_eq!(texture_count(&actors, STAR_TEX), 1);
        assert_eq!(texture_count(&actors, AFFLUENT_TEX), 0);
        assert_eq!(texture_prefix_count(&actors, AFFLUENT_CLIP_KEY_PREFIX), 1);
        assert_eq!(texture_count(&actors, GOLDSTAR_TEX), 1);
    }

    #[test]
    fn exact_one_w2_goldstar_starts_black() {
        let actors = actors(
            score_data::Grade::Tier04,
            EvalGradeParams {
                elapsed: 1.0,
                taunt: grade_star_taunt_from_counts(counts(1, 0, 0, 0, 0)),
                ..Default::default()
            },
        );

        assert_eq!(texture_count(&actors, STAR_TEX), 1);
        assert_eq!(texture_count(&actors, AFFLUENT_TEX), 0);
        assert_eq!(
            first_tint(&actors, GOLDSTAR_TEX),
            Some([0.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn one_w3_adds_black_flag_only() {
        let actors = actors(
            score_data::Grade::Tier04,
            EvalGradeParams {
                elapsed: 10.0,
                taunt: grade_star_taunt_from_counts(counts(7, 1, 0, 0, 0)),
                ..Default::default()
            },
        );

        assert_eq!(texture_count(&actors, STAR_TEX), 1);
        assert_eq!(texture_count(&actors, AFFLUENT_TEX), 0);
        assert_eq!(
            first_tint(&actors, GOLDSTAR_TEX),
            Some([0.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn worse_than_one_w3_gets_no_taunt() {
        let actors = actors(
            score_data::Grade::Tier04,
            EvalGradeParams {
                elapsed: 10.0,
                taunt: grade_star_taunt_from_counts(counts(1, 2, 0, 0, 0)),
                ..Default::default()
            },
        );

        assert_eq!(texture_count(&actors, STAR_TEX), 1);
        assert_eq!(texture_count(&actors, AFFLUENT_TEX), 0);
        assert_eq!(texture_count(&actors, GOLDSTAR_TEX), 0);
    }
}
