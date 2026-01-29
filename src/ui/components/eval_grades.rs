use crate::act;
use crate::game::scores;
use crate::ui::actors::Actor;

const DEFAULT_EVAL_ZOOM: f32 = 0.4;
const LETTER_ZOOM: f32 = 0.85;

const STAR_PULSE_PERIOD_S: f32 = 0.80;
const STAR_PULSE_AMP: f32 = 0.06;

#[derive(Clone, Copy, Debug)]
pub struct EvalGradeParams {
    pub x: f32,
    pub y: f32,
    pub z: i16,
    pub zoom: f32,
    pub elapsed: f32,
}

impl Default for EvalGradeParams {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0,
            zoom: DEFAULT_EVAL_ZOOM,
            elapsed: 0.0,
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
fn letter_tex(grade: scores::Grade) -> &'static str {
    match grade {
        scores::Grade::Tier05 => "grades/s-plus.png",
        scores::Grade::Tier06 => "grades/s.png",
        scores::Grade::Tier07 => "grades/s-minus.png",
        scores::Grade::Tier08 => "grades/a-plus.png",
        scores::Grade::Tier09 => "grades/a.png",
        scores::Grade::Tier10 => "grades/a-minus.png",
        scores::Grade::Tier11 => "grades/b-plus.png",
        scores::Grade::Tier12 => "grades/b.png",
        scores::Grade::Tier13 => "grades/b-minus.png",
        scores::Grade::Tier14 => "grades/c-plus.png",
        scores::Grade::Tier15 => "grades/c.png",
        scores::Grade::Tier16 => "grades/c-minus.png",
        scores::Grade::Tier17 => "grades/d.png",
        scores::Grade::Failed => "grades/f.png",
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
fn star_actor(s: StarDef, p: EvalGradeParams) -> Actor {
    let x = p.x + s.dx * p.zoom;
    let y = p.y + s.dy * p.zoom;
    let base = s.zoom * p.zoom;
    let (sx, sy) = pulse_scales(base, p.elapsed, s.pulse_offset, s.mag_x, s.mag_y);
    let rot = spin_rot_deg(p.elapsed, s.spin_delay_s, s.spin_seed);

    act!(sprite("grades/star.png"):
        align(0.5, 0.5):
        xy(x, y):
        zoomx(sx):
        zoomy(sy):
        rotationz(rot):
        z(p.z)
    )
}

#[inline(always)]
fn stars_for(grade: scores::Grade) -> Option<&'static [StarDef]> {
    match grade {
        scores::Grade::Quint => Some(&STARS_QUINT),
        scores::Grade::Tier01 => Some(&STARS_TIER01),
        scores::Grade::Tier02 => Some(&STARS_TIER02),
        scores::Grade::Tier03 => Some(&STARS_TIER03),
        scores::Grade::Tier04 => Some(&STARS_TIER04),
        _ => None,
    }
}

pub fn actors(grade: scores::Grade, p: EvalGradeParams) -> Vec<Actor> {
    if let Some(stars) = stars_for(grade) {
        return stars.iter().copied().map(|s| star_actor(s, p)).collect();
    }

    let tex = letter_tex(grade);
    vec![act!(sprite(tex):
        align(0.5, 0.5):
        xy(p.x, p.y):
        zoom(p.zoom * LETTER_ZOOM):
        z(p.z)
    )]
}
