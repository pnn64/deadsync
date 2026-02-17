use crate::assets;
use crate::core::gfx::BlendMode;
use crate::ui::actors::{Actor, SizeSpec, SpriteSource, TextAlign, TextContent};
use crate::ui::{anim, font, runtime};

// PARITY COMMENT STANDARD:
// PARITY[<Source>]: <mirrored behavior>. Ref: <file/symbol> when known.

#[allow(dead_code)]
#[derive(Clone)]
pub enum Mod<'a> {
    // position
    Xy(f32, f32),
    SetX(f32),
    SetY(f32),
    AddX(f32),
    AddY(f32),

    // pivot inside the rect (0..1)
    Align(f32, f32),
    HAlign(f32),
    VAlign(f32),

    // draw order & color
    Z(i16),
    Tint([f32; 4]),
    Alpha(f32),
    Glow([f32; 4]),
    StrokeColor([f32; 4]),
    Blend(BlendMode),

    // absolute size (pre-zoom) in SM TL space
    SizePx(f32, f32),

    // PARITY[StepMania Actor]: zoom/zoomx/zoomy mutate scale factors.
    Zoom(f32),
    ZoomX(f32),
    ZoomY(f32),
    AddZoomX(f32),
    AddZoomY(f32),
    /// `zoomto(w,h)` — sets zoom based on current unzoomed width/height.
    ZoomToPx(f32, f32),

    // helpers that set one axis and preserve aspect
    ZoomToWidth(f32),
    ZoomToHeight(f32),

    // NEW: max constraints (only shrink, preserve aspect)
    MaxWidth(f32),
    MaxHeight(f32),

    // cropping (fractions 0..1)
    CropLeft(f32),
    CropRight(f32),
    CropTop(f32),
    CropBottom(f32),

    // NEW: edge fades
    FadeLeft(f32),
    FadeRight(f32),
    FadeTop(f32),
    FadeBottom(f32),
    MaskSource,
    MaskDest,

    // texture scroll (kept)
    TexVel([f32; 2]),

    // text
    Font(&'static str),
    Content(TextContent),
    TAlign(TextAlign),

    // visibility + rotation
    Visible(bool),
    RotX(f32),
    RotY(f32),
    RotZ(f32),
    AddRotX(f32),
    AddRotY(f32),
    AddRotZ(f32),

    // PARITY[StepMania/ITGmania Sprite]: state/frame and UV controls.
    /// `setstate(i)` — linear state index (row-major); grid inferred from filename `_CxR`.
    State(u32),
    /// `SetAllStateDelays(seconds)` — uniform delay for each state while animating.
    StateDelay(f32),
    /// `animate(true/false)` — toggles auto state advance.
    Animate(bool),
    /// `customtexturerect(u0,v0,u1,v1)` — normalized UVs, top-left origin.
    UvRect([f32; 4]),

    // runtime/tween plumbing
    Tween(&'a [anim::Step]),

    // --- Shadow (SM-compatible) ---
    /// `shadowlength(f)` — sets both X and Y to f.
    ShadowLenBoth(f32),
    /// `shadowlengthx(f)` — sets horizontal shadow length.
    ShadowLenX(f32),
    /// `shadowlengthy(f)` — sets vertical shadow length.
    ShadowLenY(f32),
    /// `shadowcolor(r,g,b,a)` — sets shadow tint; default is (0,0,0,0.5).
    ShadowColor([f32; 4]),

    // PARITY[StepMania/ITGmania Actor]: effect command family.
    EffectClock(anim::EffectClock),
    EffectMode(anim::EffectMode),
    EffectColor1([f32; 4]),
    EffectColor2([f32; 4]),
    EffectPeriod(f32),
    EffectOffset(f32),
    EffectTiming([f32; 5]),
    EffectMagnitude([f32; 3]),
}

#[doc(hidden)]
#[inline(always)]
pub fn __dsl_parse_effect_clock(raw: &str) -> anim::EffectClock {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        // ITGmania Actor::SetEffectClockString()
        "beat" | "beatnooffset" | "bgm" => anim::EffectClock::Beat,
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            anim::EffectClock::Time
        }
        _ if lower.contains("beat") => anim::EffectClock::Beat,
        _ => anim::EffectClock::Time,
    }
}

/* ===================== small hashing helpers ===================== */
/* Fix for clippy::items_after_statements: no nested fn items mid-block */

#[inline(always)]
fn mix_u64(h: &mut u64, v: u64) {
    *h ^= v.wrapping_mul(0x9E3779B97F4A7C15);
    *h = h.rotate_left(27) ^ (*h >> 33);
}

#[inline(always)]
fn f32b_u64(f: f32) -> u64 {
    f.to_bits() as u64
}

#[inline(always)]
fn hash_bytes64(bs: &[u8]) -> u64 {
    let mut x = 0xcbf29ce484222325u64;
    for &b in bs {
        x ^= b as u64;
        x = x.wrapping_mul(0x100000001b3);
    }
    x
}

#[inline(always)]
fn sprite_native_dims(
    source: &SpriteSource,
    uv: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
) -> (f32, f32) {
    match source {
        SpriteSource::Solid => (1.0, 1.0),
        SpriteSource::Texture(key) => {
            let Some(meta) = assets::texture_dims(key) else {
                return (0.0, 0.0);
            };
            let (mut tw, mut th) = (meta.w as f32, meta.h as f32);

            if let Some([u0, v0, u1, v1]) = uv {
                tw *= (u1 - u0).abs().max(1e-6);
                th *= (v1 - v0).abs().max(1e-6);
                return (tw, th);
            }

            // Match compose: if the texture looks like a sheet and no cell is specified,
            // default to cell 0 for sizing (per-frame dimensions).
            let effective_cell = if cell.is_some() {
                cell
            } else {
                let (gc, gr) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(key));
                if gc.saturating_mul(gr) > 1 {
                    Some((0, u32::MAX))
                } else {
                    None
                }
            };

            if effective_cell.is_some() {
                let (gc, gr) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(key));
                let cols = gc.max(1);
                let rows = gr.max(1);
                tw /= cols as f32;
                th /= rows as f32;
            }

            (tw, th)
        }
    }
}

/* ======================== SPRITE/QUAD CORE ======================== */

#[inline(always)]
fn build_sprite_like<'a>(
    source: SpriteSource,
    mods: &[Mod<'a>],
    file: &'static str,
    line: u32,
    col: u32,
) -> Actor {
    // defaults
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    let (mut hx, mut vy) = (0.5, 0.5);
    let mut tint = [1.0, 1.0, 1.0, 1.0];
    let mut glow = [1.0, 1.0, 1.0, 0.0];
    let mut z: i16 = 0;
    let (mut vis, mut fx, mut fy) = (true, false, false);
    let (mut cl, mut cr, mut ct, mut cb) = (0.0, 0.0, 0.0, 0.0);
    let (mut fl, mut fr, mut ft, mut fb) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    let mut blend = BlendMode::Alpha;
    let mut mask_source = false;
    let mut mask_dest = false;
    let mut rot_x = 0.0_f32;
    let mut rot_y = 0.0_f32;
    let mut rot_z = 0.0_f32;
    let mut uv: Option<[f32; 4]> = None;
    let mut cell: Option<(u32, u32)> = None;
    let mut grid: Option<(u32, u32)> = None;
    let mut texv: Option<[f32; 2]> = None;
    // animation
    let mut anim_enable = false;
    let mut state_delay = 0.1_f32;
    let (mut tw, _site_ignored): (Option<&[anim::Step]>, u64) = (None, 0);
    let mut effect = anim::EffectState::default();

    // PARITY[StepMania Actor]: keep zoom signs until final flip folding.
    let (mut sx, mut sy) = (1.0_f32, 1.0_f32);

    // PARITY[StepMania Actor]: shadow defaults are length=0, color=(0,0,0,0.5).
    let (mut shx, mut shy) = (0.0_f32, 0.0_f32);
    let mut shc = [0.0_f32, 0.0_f32, 0.0_f32, 0.5_f32];

    // fold mods in order
    for m in mods {
        match m {
            Mod::Xy(a, b) => {
                x = *a;
                y = *b;
            }
            Mod::SetX(a) => {
                x = *a;
            }
            Mod::SetY(b) => {
                y = *b;
            }
            Mod::AddX(a) => {
                x += *a;
            }
            Mod::AddY(b) => {
                y += *b;
            }

            Mod::HAlign(a) => {
                hx = *a;
            }
            Mod::VAlign(b) => {
                vy = *b;
            }
            Mod::Align(a, b) => {
                hx = *a;
                vy = *b;
            }

            Mod::Z(v) => {
                z = *v;
            }
            Mod::Tint(rgba) => {
                tint = *rgba;
            }
            Mod::Alpha(a) => {
                tint[3] = *a;
            }
            Mod::Glow(rgba) => {
                glow = *rgba;
            }
            Mod::StrokeColor(_) => {}
            Mod::Blend(bm) => {
                blend = *bm;
            }
            Mod::MaskSource => {
                mask_source = true;
            }
            Mod::MaskDest => {
                mask_dest = true;
            }

            Mod::SizePx(a, b) => {
                w = *a;
                h = *b;
            }

            // PARITY[StepMania Actor]: zoom commands mutate scale factors.
            Mod::Zoom(f) => {
                sx = *f;
                sy = *f;
            }
            Mod::ZoomX(a) => {
                sx = *a;
            }
            Mod::ZoomY(b) => {
                sy = *b;
            }
            Mod::AddZoomX(a) => {
                sx += *a;
            }
            Mod::AddZoomY(b) => {
                sy += *b;
            }
            Mod::ZoomToPx(tw, th) => {
                let (nw, nh) = sprite_native_dims(&source, uv, cell, grid);
                let base_w = if w != 0.0 { w } else { nw };
                let base_h = if h != 0.0 { h } else { nh };
                sx = if base_w != 0.0 { *tw / base_w } else { 0.0 };
                sy = if base_h != 0.0 { *th / base_h } else { 0.0 };
            }

            // aspect-preserving absolute sizes
            Mod::ZoomToWidth(new_w) => {
                if w > 0.0 && h > 0.0 {
                    let aspect = h / w;
                    w = *new_w;
                    h = w * aspect;
                } else {
                    w = *new_w;
                }
            }
            Mod::ZoomToHeight(new_h) => {
                if w > 0.0 && h > 0.0 {
                    let aspect = w / h;
                    h = *new_h;
                    w = h * aspect;
                } else {
                    h = *new_h;
                }
            }

            Mod::CropLeft(v) => {
                cl = *v;
            }
            Mod::CropRight(v) => {
                cr = *v;
            }
            Mod::CropTop(v) => {
                ct = *v;
            }
            Mod::CropBottom(v) => {
                cb = *v;
            }

            Mod::FadeLeft(v) => {
                fl = *v;
            }
            Mod::FadeRight(v) => {
                fr = *v;
            }
            Mod::FadeTop(v) => {
                ft = *v;
            }
            Mod::FadeBottom(v) => {
                fb = *v;
            }

            Mod::TexVel(v) => {
                texv = Some(*v);
            }

            Mod::Visible(v) => {
                vis = *v;
            }
            Mod::RotX(d) => {
                rot_x = *d;
            }
            Mod::RotY(d) => {
                rot_y = *d;
            }
            Mod::RotZ(d) => {
                rot_z = *d;
            }
            Mod::AddRotX(dd) => {
                rot_x += *dd;
            }
            Mod::AddRotY(dd) => {
                rot_y += *dd;
            }
            Mod::AddRotZ(dd) => {
                rot_z += *dd;
            }

            // text-only mods ignored here
            Mod::Font(_)
            | Mod::Content(_)
            | Mod::TAlign(_)
            | Mod::MaxWidth(_)
            | Mod::MaxHeight(_) => {}
            Mod::Tween(steps) => {
                tw = Some(steps);
            }
            Mod::State(i) => {
                cell = Some((*i, u32::MAX));
                grid = None;
                uv = None;
            }
            Mod::UvRect(r) => {
                uv = Some(*r);
                cell = None;
                grid = None;
            }
            Mod::Animate(v) => {
                anim_enable = *v;
            }
            Mod::StateDelay(s) => {
                state_delay = (*s).max(0.0);
            }

            // PARITY[StepMania Actor]: +Y is down; flip Y in our +Y-up space for matching shadows.
            Mod::ShadowLenBoth(v) => {
                shx = *v;
                shy = -*v;
            }
            Mod::ShadowLenX(v) => {
                shx = *v;
            }
            Mod::ShadowLenY(v) => {
                shy = -*v;
            }
            Mod::ShadowColor(c) => {
                shc = *c;
            }

            Mod::EffectClock(clock) => {
                effect.clock = *clock;
            }
            Mod::EffectMode(mode) => {
                effect.mode = *mode;
            }
            Mod::EffectColor1(color) => {
                effect.color1 = *color;
            }
            Mod::EffectColor2(color) => {
                effect.color2 = *color;
            }
            Mod::EffectPeriod(v) => {
                if *v > 0.0 {
                    effect.period = *v;
                    effect.timing = [*v * 0.5, 0.0, *v * 0.5, 0.0, 0.0];
                }
            }
            Mod::EffectOffset(v) => {
                effect.offset = *v;
            }
            Mod::EffectTiming(v) => {
                let timing = [
                    v[0].max(0.0),
                    v[1].max(0.0),
                    v[2].max(0.0),
                    v[3].max(0.0),
                    v[4].max(0.0),
                ];
                let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
                if total > 0.0 {
                    effect.timing = timing;
                    effect.period = total;
                }
            }
            Mod::EffectMagnitude(v) => {
                effect.magnitude = *v;
            }
        }
    }

    // PARITY[StepMania Actor]: `zoomto()` is computed from unzoomed actor size.
    // If size isn't explicitly set, use native texture size (or 1x1 for quads).
    if tw.is_some() && w == 0.0 && h == 0.0 {
        let (nw, nh) = sprite_native_dims(&source, uv, cell, grid);
        w = nw;
        h = nh;
    }

    if let Some(steps) = tw {
        let mut init = anim::TweenState::default();
        init.x = x;
        init.y = y;
        init.w = w;
        init.h = h;
        init.hx = hx;
        init.vy = vy;
        init.tint = tint;
        init.glow = glow;
        init.visible = vis;
        init.flip_x = fx;
        init.flip_y = fy;
        init.rot_x = rot_x;
        init.rot_y = rot_y;
        init.rot_z = rot_z;
        init.fade_l = fl;
        init.fade_r = fr;
        init.fade_t = ft;
        init.fade_b = fb;
        init.crop_l = cl;
        init.crop_r = cr;
        init.crop_t = ct;
        init.crop_b = cb;
        init.scale = [sx, sy];

        #[inline(always)]
        fn auto_salt(src: &SpriteSource, init: &anim::TweenState, steps: &[anim::Step]) -> u64 {
            let mut h = 0xcbf29ce484222325u64;

            match src {
                SpriteSource::Texture(key) => {
                    mix_u64(&mut h, 0x54455854);
                    mix_u64(&mut h, hash_bytes64(key.as_bytes()));
                }
                SpriteSource::Solid => {
                    mix_u64(&mut h, 0x534F4C49);
                }
            }

            mix_u64(&mut h, f32b_u64(init.x));
            mix_u64(&mut h, f32b_u64(init.y));
            mix_u64(&mut h, f32b_u64(init.w));
            mix_u64(&mut h, f32b_u64(init.h));
            mix_u64(&mut h, f32b_u64(init.hx));
            mix_u64(&mut h, f32b_u64(init.vy));
            mix_u64(&mut h, f32b_u64(init.rot_x));
            mix_u64(&mut h, f32b_u64(init.rot_y));
            mix_u64(&mut h, f32b_u64(init.rot_z));
            for c in init.tint {
                mix_u64(&mut h, f32b_u64(c));
            }
            for c in init.glow {
                mix_u64(&mut h, f32b_u64(c));
            }
            mix_u64(&mut h, u64::from(init.visible));
            mix_u64(&mut h, u64::from(init.flip_x));
            mix_u64(&mut h, u64::from(init.flip_y));
            mix_u64(&mut h, f32b_u64(init.fade_l));
            mix_u64(&mut h, f32b_u64(init.fade_r));
            mix_u64(&mut h, f32b_u64(init.fade_t));
            mix_u64(&mut h, f32b_u64(init.fade_b));
            mix_u64(&mut h, f32b_u64(init.crop_l));
            mix_u64(&mut h, f32b_u64(init.crop_r));
            mix_u64(&mut h, f32b_u64(init.crop_t));
            mix_u64(&mut h, f32b_u64(init.crop_b));
            mix_u64(&mut h, f32b_u64(init.scale[0]));
            mix_u64(&mut h, f32b_u64(init.scale[1]));
            for s in steps {
                mix_u64(&mut h, s.fingerprint64());
            }
            h
        }

        let salt = auto_salt(&source, &init, steps);
        let sid = runtime::site_id(file, line, col, salt);
        let s = runtime::materialize(sid, init, steps);

        x = s.x;
        y = s.y;
        w = s.w;
        h = s.h;
        hx = s.hx;
        vy = s.vy;
        tint = s.tint;
        glow = s.glow;
        vis = s.visible;
        fx = s.flip_x;
        fy = s.flip_y;
        rot_x = s.rot_x;
        rot_y = s.rot_y;
        rot_z = s.rot_z;
        fl = s.fade_l;
        fr = s.fade_r;
        ft = s.fade_t;
        fb = s.fade_b;
        cl = s.crop_l;
        cr = s.crop_r;
        ct = s.crop_t;
        cb = s.crop_b;
        sx = s.scale[0];
        sy = s.scale[1];
    }

    // PARITY[StepMania Actor]: negative zoom flips; magnitudes stay positive.
    if sx < 0.0 {
        fx = !fx;
        sx = -sx;
    }
    if sy < 0.0 {
        fy = !fy;
        sy = -sy;
    }

    // If size is already known, apply zoom now. Else, carry to compose.
    //
    // IMPORTANT: `SizeSpec::Px(0,0)` is used in compose as a sentinel meaning
    // "use native texture dims". If a sprite had an explicit size and is then
    // scaled to exactly 0 (e.g. `zoom(0)`), we must not let it fall back to
    // native size on the final frame.
    let scale_carry = if w != 0.0 || h != 0.0 {
        w *= sx;
        h *= sy;
        if w == 0.0 && h == 0.0 {
            [0.0, 0.0]
        } else {
            [1.0, 1.0]
        }
    } else {
        [sx, sy]
    };

    let base = Actor::Sprite {
        align: [hx, vy],
        offset: [x, y],
        size: [SizeSpec::Px(w), SizeSpec::Px(h)],
        source,
        tint,
        glow,
        z,
        cell,
        grid,
        uv_rect: uv,
        visible: vis,
        flip_x: fx,
        flip_y: fy,
        cropleft: cl,
        cropright: cr,
        croptop: ct,
        cropbottom: cb,
        fadeleft: fl,
        faderight: fr,
        fadetop: ft,
        fadebottom: fb,
        blend,
        mask_source,
        mask_dest,
        rot_x_deg: rot_x,
        rot_y_deg: rot_y,
        rot_z_deg: rot_z,
        texcoordvelocity: texv,
        animate: anim_enable,
        state_delay,
        scale: scale_carry, // NEW
        effect,
    };

    if shx != 0.0 || shy != 0.0 {
        Actor::Shadow {
            len: [shx, shy],
            color: shc,
            child: Box::new(base),
        }
    } else {
        base
    }
}

#[inline(always)]
pub fn sprite<'a>(tex: String, mods: &[Mod<'a>], f: &'static str, l: u32, c: u32) -> Actor {
    build_sprite_like(SpriteSource::Texture(tex), mods, f, l, c)
}

#[inline(always)]
pub fn quad<'a>(mods: &[Mod<'a>], f: &'static str, l: u32, c: u32) -> Actor {
    build_sprite_like(SpriteSource::Solid, mods, f, l, c)
}

/* ============================== TEXT =============================== */

#[inline(always)]
pub fn text<'a>(mods: &[Mod<'a>], file: &'static str, line: u32, col: u32) -> Actor {
    let (mut x, mut y) = (0.0, 0.0);
    let (mut hx, mut vy) = (0.5, 0.5);
    let mut color = [1.0, 1.0, 1.0, 1.0];
    let mut glow = [1.0, 1.0, 1.0, 0.0];
    let mut stroke_color: Option<[f32; 4]> = None;
    let mut font: &'static str = "miso";
    let mut content: TextContent = TextContent::default();
    let mut talign = TextAlign::Left;
    let mut z: i16 = 0;

    // zoom + optional fit targets
    let (mut sx, mut sy) = (1.0_f32, 1.0_f32);
    let (mut fit_w, mut fit_h): (Option<f32>, Option<f32>) = (None, None);
    let (mut max_w, mut max_h): (Option<f32>, Option<f32>) = (None, None);

    // PARITY[StepMania Actor]: track command order for maxwidth/maxheight vs zoom.
    let (mut saw_max_w, mut saw_max_h) = (false, false);
    let (mut max_w_pre_zoom, mut max_h_pre_zoom) = (false, false);

    // text respects blend mode
    let mut blend = BlendMode::Alpha;
    let mut tw: Option<&[anim::Step]> = None;
    let mut effect = anim::EffectState::default();

    // PARITY[StepMania Actor]: shadow defaults are length=0, color=(0,0,0,0.5).
    let (mut shx, mut shy) = (0.0_f32, 0.0_f32);
    let mut shc = [0.0_f32, 0.0_f32, 0.0_f32, 0.5_f32];

    for m in mods {
        match m {
            // position & alignment
            Mod::Xy(a, b) => {
                x = *a;
                y = *b;
            }
            Mod::SetX(a) => {
                x = *a;
            }
            Mod::SetY(b) => {
                y = *b;
            }
            Mod::AddX(a) => {
                x += *a;
            }
            Mod::AddY(b) => {
                y += *b;
            }

            Mod::HAlign(a) => {
                hx = *a;
            }
            Mod::VAlign(b) => {
                vy = *b;
            }
            Mod::Align(a, b) => {
                hx = *a;
                vy = *b;
            }

            // color/font/text/align
            Mod::Tint(r) => {
                color = *r;
            }
            Mod::Alpha(a) => {
                color[3] = *a;
            }
            Mod::Glow(r) => {
                glow = *r;
            }
            Mod::StrokeColor(r) => {
                stroke_color = Some(*r);
            }
            Mod::Font(f) => {
                font = *f;
            }
            Mod::Content(s) => {
                content = s.clone();
            }
            Mod::TAlign(a) => {
                talign = *a;
            }
            Mod::Z(v) => {
                z = *v;
            }

            // zooms — if they occur after a max* for that axis, mark pre-zoom clamp
            Mod::Zoom(f) => {
                sx = *f;
                sy = *f;
                if saw_max_w {
                    max_w_pre_zoom = true;
                }
                if saw_max_h {
                    max_h_pre_zoom = true;
                }
            }
            Mod::ZoomX(a) => {
                sx = *a;
                if saw_max_w {
                    max_w_pre_zoom = true;
                }
            }
            Mod::ZoomY(b) => {
                sy = *b;
                if saw_max_h {
                    max_h_pre_zoom = true;
                }
            }
            Mod::AddZoomX(a) => {
                sx += *a;
                if saw_max_w {
                    max_w_pre_zoom = true;
                }
            }
            Mod::AddZoomY(b) => {
                sy += *b;
                if saw_max_h {
                    max_h_pre_zoom = true;
                }
            }

            // fit targets (applied later with metrics)
            Mod::ZoomToWidth(w) => {
                fit_w = Some(*w);
            }
            Mod::ZoomToHeight(h) => {
                fit_h = Some(*h);
            }

            // max constraints — reset the pre/post decision window
            Mod::MaxWidth(w) => {
                max_w = Some(*w);
                saw_max_w = true;
                max_w_pre_zoom = false; // a new max resets the boundary
            }
            Mod::MaxHeight(h) => {
                max_h = Some(*h);
                saw_max_h = true;
                max_h_pre_zoom = false; // a new max resets the boundary
            }

            // blend mode
            Mod::Blend(bm) => {
                blend = *bm;
            }
            Mod::Tween(steps) => {
                tw = Some(steps);
            }

            // PARITY[StepMania Actor]: +Y is down; flip Y in our +Y-up space for matching shadows.
            Mod::ShadowLenBoth(v) => {
                shx = *v;
                shy = -*v;
            }
            Mod::ShadowLenX(v) => {
                shx = *v;
            }
            Mod::ShadowLenY(v) => {
                shy = -*v;
            }
            Mod::ShadowColor(c) => {
                shc = *c;
            }

            Mod::EffectClock(clock) => {
                effect.clock = *clock;
            }
            Mod::EffectMode(mode) => {
                effect.mode = *mode;
            }
            Mod::EffectColor1(color1) => {
                effect.color1 = *color1;
            }
            Mod::EffectColor2(color2) => {
                effect.color2 = *color2;
            }
            Mod::EffectPeriod(v) => {
                if *v > 0.0 {
                    effect.period = *v;
                    effect.timing = [*v * 0.5, 0.0, *v * 0.5, 0.0, 0.0];
                }
            }
            Mod::EffectOffset(v) => {
                effect.offset = *v;
            }
            Mod::EffectTiming(v) => {
                let timing = [
                    v[0].max(0.0),
                    v[1].max(0.0),
                    v[2].max(0.0),
                    v[3].max(0.0),
                    v[4].max(0.0),
                ];
                let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
                if total > 0.0 {
                    effect.timing = timing;
                    effect.period = total;
                }
            }
            Mod::EffectMagnitude(v) => {
                effect.magnitude = *v;
            }

            // ignore sprite-only/text-irrelevant
            _ => {}
        }
    }

    if let std::borrow::Cow::Owned(s) = font::replace_markers(content.as_str()) {
        content = TextContent::Owned(s);
    }

    if let Some(steps) = tw {
        let mut init = anim::TweenState::default();
        init.x = x;
        init.y = y;
        init.tint = color;
        init.glow = glow;
        init.scale = [sx, sy];

        // Create a salt for the tween state based on the text content to ensure
        // different text gets a different animation state.
        let salt = {
            let mut h = 0xcbf29ce484222325u64;
            for &b in content.as_str().as_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h
        };

        let sid = runtime::site_id(file, line, col, salt);
        let s = runtime::materialize(sid, init, steps);

        // Apply tweened state
        x = s.x;
        y = s.y;
        color = s.tint;
        glow = s.glow;
        sx = s.scale[0];
        sy = s.scale[1];
    }

    let base = Actor::Text {
        align: [hx, vy],
        offset: [x, y],
        color,
        stroke_color,
        glow,
        font,
        content,
        align_text: talign,
        z,
        scale: [sx, sy],
        fit_width: fit_w,
        fit_height: fit_h,
        max_width: max_w,
        max_height: max_h,
        max_w_pre_zoom,
        max_h_pre_zoom,
        clip: None,
        blend,
        effect,
    };

    if shx != 0.0 || shy != 0.0 {
        Actor::Shadow {
            len: [shx, shy],
            color: shc,
            child: Box::new(base),
        }
    } else {
        base
    }
}

// ... act! and helper macros ...
#[macro_export]
macro_rules! __ui_textalign_from_ident {
    (left) => {
        $crate::ui::actors::TextAlign::Left
    };
    (center) => {
        $crate::ui::actors::TextAlign::Center
    };
    (right) => {
        $crate::ui::actors::TextAlign::Right
    };
    ($other:ident) => {
        compile_error!(concat!(
            "horizalign expects left|center|right, got: ",
            stringify!($other)
        ));
    };
}

#[macro_export]
macro_rules! __ui_halign_from_ident {
    (left) => {
        0.0f32
    };
    (center) => {
        0.5f32
    };
    (right) => {
        1.0f32
    };
    ($other:ident) => {
        compile_error!(concat!(
            "halign expects left|center|right, got: ",
            stringify!($other)
        ));
    };
}

#[macro_export]
macro_rules! __ui_valign_from_ident {
    (top) => {
        0.0f32
    };
    (middle) => {
        0.5f32
    };
    (center) => {
        0.5f32
    };
    (bottom) => {
        1.0f32
    };
    ($other:ident) => {
        compile_error!(concat!(
            "valign expects top|middle|center|bottom, got: ",
            stringify!($other)
        ));
    };
}

#[macro_export]
macro_rules! act {
    (sprite($tex:expr): $($tail:tt)+) => {{
        let mut __mods = ::std::vec::Vec::new();
        let mut __tw = ::std::vec::Vec::new();
        let mut __cur: ::core::option::Option<$crate::ui::anim::SegmentBuilder> = None;
        $crate::__dsl_apply!( ($($tail)+) __mods __tw __cur _dummy_site );
        if let ::core::option::Option::Some(seg)=__cur.take(){__tw.push(seg.build());}
        if !__tw.is_empty(){ __mods.push($crate::ui::dsl::Mod::Tween(&__tw)); }
        $crate::ui::dsl::sprite(($tex).to_string(), &__mods, file!(), line!(), column!())
    }};
    (quad: $($tail:tt)+) => {{
        let mut __mods = ::std::vec::Vec::new();
        let mut __tw = ::std::vec::Vec::new();
        let mut __cur: ::core::option::Option<$crate::ui::anim::SegmentBuilder> = None;
        $crate::__dsl_apply!( ($($tail)+) __mods __tw __cur _dummy_site );
        if let ::core::option::Option::Some(seg)=__cur.take(){__tw.push(seg.build());}
        if !__tw.is_empty(){ __mods.push($crate::ui::dsl::Mod::Tween(&__tw)); }
        $crate::ui::dsl::quad(&__mods, file!(), line!(), column!())
    }};
    (text: $($tail:tt)+) => {{
        let mut __mods = ::std::vec::Vec::new();
        let mut __tw = ::std::vec::Vec::new();
        let mut __cur: ::core::option::Option<$crate::ui::anim::SegmentBuilder> = None;
        $crate::__dsl_apply!( ($($tail)+) __mods __tw __cur _dummy_site );
        if let ::core::option::Option::Some(seg)=__cur.take(){__tw.push(seg.build());}
        if !__tw.is_empty(){ __mods.push($crate::ui::dsl::Mod::Tween(&__tw)); }
        $crate::ui::dsl::text(&__mods, file!(), line!(), column!())
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply {
    ( () $mods:ident $tw:ident $cur:ident $site:ident ) => { () };
    ( ($cmd:ident ( $($args:tt)* ) : $($rest:tt)* ) $mods:ident $tw:ident $cur:ident $site:ident ) => {{
        $crate::__dsl_apply_one!{ $cmd ( $($args)* ) $mods $tw $cur $site }
        $crate::__dsl_apply!( ($($rest)*) $mods $tw $cur $site );
    }};
    ( ($cmd:ident ( $($args:tt)* ) ) $mods:ident $tw:ident $cur:ident $site:ident ) => {{
        $crate::__dsl_apply_one!{ $cmd ( $($args)* ) $mods $tw $cur $site }
        $crate::__dsl_apply!( () $mods $tw $cur $site );
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply_one {
    // --- segment controls ---
    (linear ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $cur = ::core::option::Option::Some($crate::ui::anim::linear(($d) as f32));
    }};
    (accelerate ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $cur = ::core::option::Option::Some($crate::ui::anim::accelerate(($d) as f32));
    }};
    (decelerate ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $cur = ::core::option::Option::Some($crate::ui::anim::decelerate(($d) as f32));
    }};
    (ease ($d:expr, $f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg) = $cur.take() { $tw.push(seg.build()); }
        $cur = ::core::option::Option::Some($crate::ui::anim::ease(($d) as f32, ($f) as f32));
    }};
    (smooth ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg) = $cur.take() { $tw.push(seg.build()); }
        $cur = ::core::option::Option::Some($crate::ui::anim::smooth(($d) as f32));
    }};
    (sleep ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $tw.push($crate::ui::anim::sleep(($d) as f32));
    }};

    // --- tweenable props ---
    (xy ($x:expr, $y:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.x(($x) as f32).y(($y) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::Xy(($x) as f32, ($y) as f32)); }
    }};
    (x ($x:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.x(($x) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::SetX(($x) as f32)); }
    }};
    (y ($y:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.y(($y) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::SetY(($y) as f32)); }
    }};
    (addx ($dx:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addx(($dx) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddX(($dx) as f32)); }
    }};
    (addy ($dy:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addy(($dy) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddY(($dy) as f32)); }
    }};

    // PARITY[StepMania Actor]: Center/CenterX/CenterY map to screen center globals.
    (Center () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let cx = $crate::core::space::globals::screen_center_x();
        let cy = $crate::core::space::globals::screen_center_y();
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.xy(cx, cy);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::Xy(cx, cy));
        }
    }};
    (CenterX () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let cx = $crate::core::space::globals::screen_center_x();
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.x(cx);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::SetX(cx));
        }
    }};
    (CenterY () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let cy = $crate::core::space::globals::screen_center_y();
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.y(cy);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::SetY(cy));
        }
    }};

    // Lowercase aliases (so both Center() and center() work)
    (center ()  $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $crate::__dsl_apply_one!(Center() $mods $tw $cur $site)
    }};
    (centerx () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $crate::__dsl_apply_one!(CenterX() $mods $tw $cur $site)
    }};
    (centery () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $crate::__dsl_apply_one!(CenterY() $mods $tw $cur $site)
    }};

    // --- color (present both for sprite & text) ---
    (diffuse ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.diffuse(($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::Tint([($r) as f32,($g) as f32,($b) as f32,($a) as f32]));
        }
    }};
    (alpha ($a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.alpha(($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::Alpha(($a) as f32));
        }
    }};
    (diffusealpha ($a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.alpha(($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::Alpha(($a) as f32));
        }
    }};

    // PARITY[StepMania/ITGmania Actor]: glow and stroke color commands.
    (glow ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.glow(($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.push($crate::ui::dsl::Mod::Glow([($r) as f32,($g) as f32,($b) as f32,($a) as f32]));
        }
    }};
    (strokecolor ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::StrokeColor([($r) as f32,($g) as f32,($b) as f32,($a) as f32]));
    }};

    // PARITY[StepMania Actor]: shadowlength/shadowcolor command behavior.
    (shadowlength ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::ShadowLenBoth(($v) as f32));
    }};
    (shadowlengthx ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::ShadowLenX(($v) as f32));
    }};
    (shadowlengthy ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::ShadowLenY(($v) as f32));
    }};
    (shadowcolor ($r:expr, $g:expr, $b:expr, $a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::ShadowColor([($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32]));
    }};

    // PARITY[StepMania/ITGmania Actor]: effect command behavior.
    (effectclock (beat) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectClock($crate::ui::anim::EffectClock::Beat));
    }};
    (effectclock (time) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectClock($crate::ui::anim::EffectClock::Time));
    }};
    (effectclock (music) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectClock($crate::ui::anim::EffectClock::Time));
    }};
    (effectclock (seconds) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectClock($crate::ui::anim::EffectClock::Time));
    }};
    (effectclock ($raw:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let __clock_raw = ::std::format!("{}", $raw);
        let __clock = $crate::ui::dsl::__dsl_parse_effect_clock(__clock_raw.as_str());
        $mods.push($crate::ui::dsl::Mod::EffectClock(__clock));
    }};

    (diffuseramp () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMode($crate::ui::anim::EffectMode::DiffuseRamp));
        $mods.push($crate::ui::dsl::Mod::EffectPeriod(1.0));
        $mods.push($crate::ui::dsl::Mod::EffectColor1([0.0, 0.0, 0.0, 1.0]));
        $mods.push($crate::ui::dsl::Mod::EffectColor2([1.0, 1.0, 1.0, 1.0]));
    }};
    (diffuseshift () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMode($crate::ui::anim::EffectMode::DiffuseShift));
        $mods.push($crate::ui::dsl::Mod::EffectPeriod(1.0));
        $mods.push($crate::ui::dsl::Mod::EffectColor1([0.0, 0.0, 0.0, 1.0]));
        $mods.push($crate::ui::dsl::Mod::EffectColor2([1.0, 1.0, 1.0, 1.0]));
    }};
    (glowshift () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMode($crate::ui::anim::EffectMode::GlowShift));
        $mods.push($crate::ui::dsl::Mod::EffectPeriod(1.0));
        $mods.push($crate::ui::dsl::Mod::EffectColor1([1.0, 1.0, 1.0, 0.2]));
        $mods.push($crate::ui::dsl::Mod::EffectColor2([1.0, 1.0, 1.0, 0.8]));
    }};
    (pulse () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMode($crate::ui::anim::EffectMode::Pulse));
        $mods.push($crate::ui::dsl::Mod::EffectPeriod(2.0));
        $mods.push($crate::ui::dsl::Mod::EffectMagnitude([0.5, 1.0, 0.0]));
    }};
    (spin () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMode($crate::ui::anim::EffectMode::Spin));
        $mods.push($crate::ui::dsl::Mod::EffectMagnitude([0.0, 0.0, 180.0]));
    }};
    (stopeffect () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMode($crate::ui::anim::EffectMode::None));
    }};

    (effectcolor1 ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectColor1([($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32]));
    }};
    (effectcolor2 ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectColor2([($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32]));
    }};
    (effectperiod ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectPeriod(($v) as f32));
    }};
    (effectoffset ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectOffset(($v) as f32));
    }};
    (effecttiming ($a:expr,$b:expr,$c:expr,$d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        // ITGmania compatibility: 4-arg call is (ramp_to_half, hold_at_half, ramp_to_full, hold_at_zero).
        $mods.push($crate::ui::dsl::Mod::EffectTiming([($a) as f32, ($b) as f32, ($c) as f32, 0.0, ($d) as f32]));
    }};
    (effecttiming ($a:expr,$b:expr,$c:expr,$d:expr,$e:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        // ITGmania compatibility: 5-arg call is (ramp_to_half, hold_at_half, ramp_to_full, hold_at_zero, hold_at_full).
        $mods.push($crate::ui::dsl::Mod::EffectTiming([($a) as f32, ($b) as f32, ($c) as f32, ($e) as f32, ($d) as f32]));
    }};
    (effectmagnitude ($x:expr,$y:expr,$z:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::EffectMagnitude([($x) as f32, ($y) as f32, ($z) as f32]));
    }};

    // PARITY[StepMania Actor]: zoom* commands mutate scale factors.
    (zoom ($f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let f=($f) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoom(f,f); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::Zoom(f)); }
    }};
    (zoomx ($f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let f=($f) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoomx(f); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::ZoomX(f)); }
    }};
    (zoomy ($f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let f=($f) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoomy(f); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::ZoomY(f)); }
    }};
    (addzoomx ($df:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let df=($df) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addzoomx(df); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddZoomX(df)); }
    }};
    (addzoomy ($df:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let df=($df) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addzoomy(df); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddZoomY(df)); }
    }};

    // PARITY[StepMania Actor]: `zoomto` works from unzoomed size; `setsize` sets unzoomed size.
    (zoomto ($w:expr, $h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let ww = ($w) as f32;
        let hh = ($h) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoomto(ww, hh); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::ZoomToPx(ww, hh)); }
    }};
    (setsize ($w:expr, $h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let ww = ($w) as f32;
        let hh = ($h) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.size(ww, hh); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::SizePx(ww, hh)); }
    }};

    // --- absolute size helpers preserving aspect ---------------------
    (zoomtowidth ($w:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::ZoomToWidth(($w) as f32));
    }};
    (zoomtoheight ($h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::ZoomToHeight(($h) as f32));
    }};
    // --- NEW: max constraints for text -------------------------------
    (maxwidth ($w:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::MaxWidth(($w) as f32));
    }};
    (maxheight ($h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::MaxHeight(($h) as f32));
    }};

    // static sprite bits / cropping / uv / blend ---------------------
    (align ($h:expr,$v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Align(($h) as f32, ($v) as f32));
    }};
    (halign ($dir:ident) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::HAlign($crate::__ui_halign_from_ident!($dir)));
    }};
    (halign ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::HAlign(($v) as f32));
    }};
    (valign ($dir:ident) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::VAlign($crate::__ui_valign_from_ident!($dir)));
    }};
    (valign ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::VAlign(($v) as f32));
    }};

    (z ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{ $mods.push($crate::ui::dsl::Mod::Z(($v) as i16)); }};
    (texcoordvelocity ($vx:expr,$vy:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::TexVel([($vx) as f32, ($vy) as f32]));
    }};
    (cropleft ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.cropleft(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::CropLeft(($v) as f32)); }
    }};
    (cropright ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.cropright(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::CropRight(($v) as f32)); }
    }};
    (croptop ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.croptop(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::CropTop(($v) as f32)); }
    }};
    (cropbottom ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.cropbottom(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::CropBottom(($v) as f32)); }
    }};
    // edge fades (0..1 of visible width/height)
    (fadeleft ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.fadeleft(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::FadeLeft(vv)); }
    }};
    (faderight ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.faderight(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::FadeRight(vv)); }
    }};
    (fadetop ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.fadetop(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::FadeTop(vv)); }
    }};
    (fadebottom ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.fadebottom(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::FadeBottom(vv)); }
    }};
    // PARITY[StepMania/ITGmania Sprite]: `setstate(i)` chooses row-major frame index.
    (setstate ($i:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::State(($i) as u32));
    }};
    // animation control
    (animate ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Animate(($v) as bool));
    }};
    (setallstatedelays ($s:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::StateDelay(($s) as f32));
    }};
    // PARITY[StepMania/ITGmania Sprite]: `customtexturerect` uses normalized top-left UVs.
    (customtexturerect ($u0:expr, $v0:expr, $u1:expr, $v1:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::UvRect([($u0) as f32, ($v0) as f32, ($u1) as f32, ($v1) as f32]));
    }};

    // --- visibility (immediate or inside a tween) ---
    (visible ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.set_visible(($v) as bool); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::Visible(($v) as bool)); }
    }};

    // --- rotation (degrees) ---
    (rotationx ($deg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let d=($deg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.rotationx(d); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::RotX(d)); }
    }};

    (rotationy ($deg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let d=($deg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.rotationy(d); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::RotY(d)); }
    }};

    (rotationz ($deg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let d=($deg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.rotationz(d); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::RotZ(d)); }
    }};

    (addrotationx ($ddeg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let dd=($ddeg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addrotationx(dd); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddRotX(dd)); }
    }};

    (addrotationy ($ddeg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let dd=($ddeg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addrotationy(dd); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddRotY(dd)); }
    }};

    (addrotationz ($ddeg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let dd=($ddeg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addrotationz(dd); $cur=::core::option::Option::Some(seg); }
        else { $mods.push($crate::ui::dsl::Mod::AddRotZ(dd)); }
    }};

    // blends: normal, add, multiply, subtract
    (blend (normal) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Blend($crate::core::gfx::BlendMode::Alpha));
    }};
    (blend (add) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Blend($crate::core::gfx::BlendMode::Add));
    }};
    (blend (multiply) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Blend($crate::core::gfx::BlendMode::Multiply));
    }};
    (blend (subtract) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Blend($crate::core::gfx::BlendMode::Subtract));
    }};
    (MaskSource () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::MaskSource);
    }};
    (masksource () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::MaskSource);
    }};
    (MaskDest () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::MaskDest);
    }};
    (maskdest () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::MaskDest);
    }};

    // Text properties (SM-compatible)
    (font ($n:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{ $mods.push($crate::ui::dsl::Mod::Font($n)); }};
    (settext ($s:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::Content($crate::ui::actors::TextContent::from($s)));
    }};
    (horizalign ($dir:ident) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.push($crate::ui::dsl::Mod::TAlign($crate::__ui_textalign_from_ident!($dir)));
    }};

    // unknown
    ($other:ident ( $($args:expr),* ) $mods:ident $tw:ident $cur:ident $site:ident) => {
        compile_error!(concat!("act!: unknown or removed command: ", stringify!($other)));
    };
}
