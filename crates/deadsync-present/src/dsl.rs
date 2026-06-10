use crate::actors::{Actor, TextAlign, TextContent};
use crate::{anim, font, runtime};
use deadsync_render::BlendMode;
use glam::Mat4 as Matrix4;
use smallvec::SmallVec;
// PARITY COMMENT STANDARD:
// PARITY[<Source>]: <mirrored behavior>. Ref: <file/symbol> when known.
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

/* ============================== TEXT =============================== */

#[doc(hidden)]
pub struct TextBuilder {
    x: f32,
    y: f32,
    hx: f32,
    vy: f32,
    color: [f32; 4],
    glow: [f32; 4],
    stroke_color: Option<[f32; 4]>,
    font: &'static str,
    content: TextContent,
    talign: TextAlign,
    z: i16,
    sx: f32,
    sy: f32,
    fit_w: Option<f32>,
    fit_h: Option<f32>,
    wrap_width_pixels: Option<i32>,
    line_spacing: Option<i32>,
    max_w: Option<f32>,
    max_h: Option<f32>,
    saw_max_w: bool,
    saw_max_h: bool,
    max_w_pre_zoom: bool,
    max_h_pre_zoom: bool,
    blend: BlendMode,
    effect: anim::EffectState,
    shx: f32,
    shy: f32,
    shc: [f32; 4],
    tween_salt: u64,
    tw: SmallVec<[anim::Step; 4]>,
}

impl TextBuilder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            hx: 0.5,
            vy: 0.5,
            color: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            stroke_color: None,
            font: "miso",
            content: TextContent::default(),
            talign: TextAlign::Left,
            z: 0,
            sx: 1.0,
            sy: 1.0,
            fit_w: None,
            fit_h: None,
            wrap_width_pixels: None,
            line_spacing: None,
            max_w: None,
            max_h: None,
            saw_max_w: false,
            saw_max_h: false,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            blend: BlendMode::Alpha,
            effect: anim::EffectState::default(),
            shx: 0.0,
            shy: 0.0,
            shc: [0.0, 0.0, 0.0, 0.5],
            tween_salt: 0,
            tw: SmallVec::new(),
        }
    }

    #[inline(always)]
    pub fn set_tween(&mut self, steps: SmallVec<[anim::Step; 4]>) {
        self.tw = steps;
    }

    #[inline(always)]
    pub fn tweensalt(&mut self, salt: u64) {
        self.tween_salt = salt;
    }

    #[inline(always)]
    pub fn xy(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    #[inline(always)]
    pub fn x(&mut self, x: f32) {
        self.x = x;
    }

    #[inline(always)]
    pub fn y(&mut self, y: f32) {
        self.y = y;
    }

    #[inline(always)]
    pub fn addx(&mut self, dx: f32) {
        self.x += dx;
    }

    #[inline(always)]
    pub fn addy(&mut self, dy: f32) {
        self.y += dy;
    }

    #[inline(always)]
    pub fn align(&mut self, h: f32, v: f32) {
        self.hx = h;
        self.vy = v;
    }

    #[inline(always)]
    pub fn halign(&mut self, h: f32) {
        self.hx = h;
    }

    #[inline(always)]
    pub fn valign(&mut self, v: f32) {
        self.vy = v;
    }

    #[inline(always)]
    pub fn diffuse(&mut self, rgba: [f32; 4]) {
        self.color = rgba;
    }

    #[inline(always)]
    pub fn alpha(&mut self, a: f32) {
        self.color[3] = a;
    }

    #[inline(always)]
    pub fn glow(&mut self, rgba: [f32; 4]) {
        self.glow = rgba;
    }

    #[inline(always)]
    pub fn strokecolor(&mut self, rgba: [f32; 4]) {
        self.stroke_color = Some(rgba);
    }

    #[inline(always)]
    pub fn font(&mut self, font: &'static str) {
        self.font = font;
    }

    #[inline(always)]
    pub fn settext(&mut self, content: TextContent) {
        self.content = content;
    }

    #[inline(always)]
    pub fn horizalign(&mut self, align: TextAlign) {
        self.talign = align;
    }

    #[inline(always)]
    pub fn z(&mut self, z: i16) {
        self.z = z;
    }

    #[inline(always)]
    pub fn zoom(&mut self, f: f32) {
        self.sx = f;
        self.sy = f;
        if self.saw_max_w {
            self.max_w_pre_zoom = true;
        }
        if self.saw_max_h {
            self.max_h_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn zoomx(&mut self, x: f32) {
        self.sx = x;
        if self.saw_max_w {
            self.max_w_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn zoomy(&mut self, y: f32) {
        self.sy = y;
        if self.saw_max_h {
            self.max_h_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn addzoomx(&mut self, dx: f32) {
        self.sx += dx;
        if self.saw_max_w {
            self.max_w_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn addzoomy(&mut self, dy: f32) {
        self.sy += dy;
        if self.saw_max_h {
            self.max_h_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn zoomtowidth(&mut self, w: f32) {
        self.fit_w = Some(w);
    }

    #[inline(always)]
    pub fn zoomtoheight(&mut self, h: f32) {
        self.fit_h = Some(h);
    }

    #[inline(always)]
    pub fn wrapwidthpixels(&mut self, w: f32) {
        let wrap = w as i32;
        self.wrap_width_pixels = (wrap >= 0).then_some(wrap);
    }

    #[inline(always)]
    pub fn vertspacing(&mut self, spacing: f32) {
        // Mirrors SM5 `BitmapText:vertspacing(n)` — overrides the font's
        // default line spacing (i.e. the distance between successive lines).
        self.line_spacing = Some(spacing as i32);
    }

    #[inline(always)]
    pub fn maxwidth(&mut self, w: f32) {
        self.max_w = Some(w);
        self.saw_max_w = true;
        self.max_w_pre_zoom = false;
    }

    #[inline(always)]
    pub fn maxheight(&mut self, h: f32) {
        self.max_h = Some(h);
        self.saw_max_h = true;
        self.max_h_pre_zoom = false;
    }

    #[inline(always)]
    pub fn blend(&mut self, blend: BlendMode) {
        self.blend = blend;
    }

    #[inline(always)]
    pub fn shadowlength(&mut self, v: f32) {
        self.shx = v;
        self.shy = -v;
    }

    #[inline(always)]
    pub fn shadowlengthx(&mut self, v: f32) {
        self.shx = v;
    }

    #[inline(always)]
    pub fn shadowlengthy(&mut self, v: f32) {
        self.shy = -v;
    }

    #[inline(always)]
    pub fn shadowcolor(&mut self, c: [f32; 4]) {
        self.shc = c;
    }

    #[inline(always)]
    pub fn effectclock(&mut self, clock: anim::EffectClock) {
        self.effect.clock = clock;
    }

    #[inline(always)]
    pub fn effectmode(&mut self, mode: anim::EffectMode) {
        self.effect.mode = mode;
    }

    #[inline(always)]
    pub fn effectcolor1(&mut self, color: [f32; 4]) {
        self.effect.color1 = color;
    }

    #[inline(always)]
    pub fn effectcolor2(&mut self, color: [f32; 4]) {
        self.effect.color2 = color;
    }

    #[inline(always)]
    pub fn effectperiod(&mut self, v: f32) {
        if v > 0.0 {
            self.effect.period = v;
            self.effect.timing = [v * 0.5, 0.0, v * 0.5, 0.0, 0.0];
        }
    }

    #[inline(always)]
    pub fn effectoffset(&mut self, v: f32) {
        self.effect.offset = v;
    }

    #[inline(always)]
    pub fn effecttiming(&mut self, v: [f32; 5]) {
        let timing = [
            v[0].max(0.0),
            v[1].max(0.0),
            v[2].max(0.0),
            v[3].max(0.0),
            v[4].max(0.0),
        ];
        let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
        if total > 0.0 {
            self.effect.timing = timing;
            self.effect.period = total;
        }
    }

    #[inline(always)]
    pub fn effectmagnitude(&mut self, v: [f32; 3]) {
        self.effect.magnitude = v;
    }

    #[inline(always)]
    pub fn size(&mut self, _w: f32, _h: f32) {}

    #[inline(always)]
    pub fn zoomto(&mut self, _w: f32, _h: f32) {}

    #[inline(always)]
    pub fn mask_source(&mut self) {}

    #[inline(always)]
    pub fn mask_dest(&mut self) {}

    #[inline(always)]
    pub fn texcoordvelocity(&mut self, _vel: [f32; 2]) {}

    #[inline(always)]
    pub fn cropleft(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn cropright(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn croptop(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn cropbottom(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn fadeleft(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn faderight(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn fadetop(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn fadebottom(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn setstate(&mut self, _i: u32) {}

    #[inline(always)]
    pub fn animate(&mut self, _v: bool) {}

    #[inline(always)]
    pub fn setallstatedelays(&mut self, _s: f32) {}

    #[inline(always)]
    pub fn customtexturerect(&mut self, _uv: [f32; 4]) {}

    #[inline(always)]
    pub fn visible(&mut self, _v: bool) {}

    #[inline(always)]
    pub fn rotationx(&mut self, _d: f32) {}

    #[inline(always)]
    pub fn rotationy(&mut self, _d: f32) {}

    #[inline(always)]
    pub fn rotationz(&mut self, _d: f32) {}

    #[inline(always)]
    pub fn addrotationx(&mut self, _dd: f32) {}

    #[inline(always)]
    pub fn addrotationy(&mut self, _dd: f32) {}

    #[inline(always)]
    pub fn addrotationz(&mut self, _dd: f32) {}

    #[inline(always)]
    pub fn build(mut self, site_base: u64) -> Actor {
        if self.content.as_str().as_bytes().contains(&b'&')
            && let std::borrow::Cow::Owned(s) = font::replace_markers(self.content.as_str())
        {
            self.content = TextContent::Owned(s);
        }

        if !self.tw.is_empty() {
            let mut init = anim::TweenState::default();
            init.x = self.x;
            init.y = self.y;
            init.tint = self.color;
            init.glow = self.glow;
            init.scale = [self.sx, self.sy];

            let sid = runtime::site_id(site_base, self.tween_salt);
            let s = runtime::materialize(sid, init, &self.tw);

            self.x = s.x;
            self.y = s.y;
            self.color = s.tint;
            self.glow = s.glow;
            self.sx = s.scale[0];
            self.sy = s.scale[1];
        }

        Actor::Text {
            align: [self.hx, self.vy],
            offset: [self.x, self.y],
            local_transform: Matrix4::IDENTITY,
            color: self.color,
            stroke_color: self.stroke_color,
            glow: self.glow,
            font: self.font,
            content: self.content,
            attributes: Vec::new(),
            align_text: self.talign,
            z: self.z,
            scale: [self.sx, self.sy],
            fit_width: self.fit_w,
            fit_height: self.fit_h,
            line_spacing: self.line_spacing,
            wrap_width_pixels: self.wrap_width_pixels,
            max_width: self.max_w,
            max_height: self.max_h,
            max_w_pre_zoom: self.max_w_pre_zoom,
            max_h_pre_zoom: self.max_h_pre_zoom,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: self.blend,
            shadow_len: [self.shx, self.shy],
            shadow_color: self.shc,
            effect: self.effect,
        }
    }
}
