use crate::actors::{Actor, IntoTextureKey, SizeSpec, SpriteSource, TextAlign, TextContent};
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

/* ======================== SPRITE/QUAD CORE ======================== */

#[doc(hidden)]
pub struct SpriteBuilder {
    source: SpriteSource,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hx: f32,
    vy: f32,
    tint: [f32; 4],
    glow: [f32; 4],
    z: i16,
    vis: bool,
    fx: bool,
    fy: bool,
    cl: f32,
    cr: f32,
    ct: f32,
    cb: f32,
    fl: f32,
    fr: f32,
    ft: f32,
    fb: f32,
    blend: BlendMode,
    mask_source: bool,
    mask_dest: bool,
    rot_x: f32,
    rot_y: f32,
    rot_z: f32,
    uv: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    texv: Option<[f32; 2]>,
    anim_enable: bool,
    state_delay: f32,
    effect: anim::EffectState,
    sx: f32,
    sy: f32,
    shx: f32,
    shy: f32,
    shc: [f32; 4],
    tween_salt: u64,
    tw: SmallVec<[anim::Step; 4]>,
}

impl SpriteBuilder {
    #[inline(always)]
    pub fn with_source(source: SpriteSource) -> Self {
        Self {
            source,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hx: 0.5,
            vy: 0.5,
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            z: 0,
            vis: true,
            fx: false,
            fy: false,
            cl: 0.0,
            cr: 0.0,
            ct: 0.0,
            cb: 0.0,
            fl: 0.0,
            fr: 0.0,
            ft: 0.0,
            fb: 0.0,
            blend: BlendMode::Alpha,
            mask_source: false,
            mask_dest: false,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            uv: None,
            cell: None,
            grid: None,
            texv: None,
            anim_enable: false,
            state_delay: 0.1,
            effect: anim::EffectState::default(),
            sx: 1.0,
            sy: 1.0,
            shx: 0.0,
            shy: 0.0,
            shc: [0.0, 0.0, 0.0, 0.5],
            tween_salt: 0,
            tw: SmallVec::new(),
        }
    }

    #[inline(always)]
    pub fn texture<T: IntoTextureKey>(tex: T) -> Self {
        Self::with_source(tex.into_sprite_source())
    }

    #[inline(always)]
    pub fn static_texture(tex: &'static str) -> Self {
        Self::with_source(SpriteSource::TextureStatic(tex))
    }

    #[inline(always)]
    pub fn solid() -> Self {
        Self::with_source(SpriteSource::Solid)
    }

    #[inline(always)]
    pub fn source(&self) -> &SpriteSource {
        &self.source
    }

    #[inline(always)]
    pub const fn uv_rect(&self) -> Option<[f32; 4]> {
        self.uv
    }

    #[inline(always)]
    pub const fn cell(&self) -> Option<(u32, u32)> {
        self.cell
    }

    #[inline(always)]
    pub const fn grid(&self) -> Option<(u32, u32)> {
        self.grid
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
    pub fn z(&mut self, z: i16) {
        self.z = z;
    }

    #[inline(always)]
    pub fn diffuse(&mut self, rgba: [f32; 4]) {
        self.tint = rgba;
    }

    #[inline(always)]
    pub fn alpha(&mut self, a: f32) {
        self.tint[3] = a;
    }

    #[inline(always)]
    pub fn glow(&mut self, rgba: [f32; 4]) {
        self.glow = rgba;
    }

    #[inline(always)]
    pub fn blend(&mut self, blend: BlendMode) {
        self.blend = blend;
    }

    #[inline(always)]
    pub fn size(&mut self, w: f32, h: f32) {
        self.w = w;
        self.h = h;
    }

    #[inline(always)]
    pub fn zoom(&mut self, f: f32) {
        self.sx = f;
        self.sy = f;
    }

    #[inline(always)]
    pub fn zoomx(&mut self, x: f32) {
        self.sx = x;
    }

    #[inline(always)]
    pub fn zoomy(&mut self, y: f32) {
        self.sy = y;
    }

    #[inline(always)]
    pub fn addzoomx(&mut self, dx: f32) {
        self.sx += dx;
    }

    #[inline(always)]
    pub fn addzoomy(&mut self, dy: f32) {
        self.sy += dy;
    }

    #[inline(always)]
    pub fn zoomto(&mut self, tw: f32, th: f32) {
        self.zoomto_with_native_dims(tw, th, [0.0, 0.0]);
    }

    #[inline(always)]
    pub fn zoomto_with_native_dims(&mut self, tw: f32, th: f32, native_dims: [f32; 2]) {
        let [nw, nh] = native_dims;
        let base_w = if self.w == 0.0 { nw } else { self.w };
        let base_h = if self.h == 0.0 { nh } else { self.h };
        self.sx = if base_w == 0.0 { 0.0 } else { tw / base_w };
        self.sy = if base_h == 0.0 { 0.0 } else { th / base_h };
    }

    #[inline(always)]
    pub fn zoomtowidth(&mut self, new_w: f32) {
        if self.w > 0.0 && self.h > 0.0 {
            let aspect = self.h / self.w;
            self.w = new_w;
            self.h = self.w * aspect;
        } else {
            self.w = new_w;
        }
    }

    #[inline(always)]
    pub fn zoomtoheight(&mut self, new_h: f32) {
        if self.w > 0.0 && self.h > 0.0 {
            let aspect = self.w / self.h;
            self.h = new_h;
            self.w = self.h * aspect;
        } else {
            self.h = new_h;
        }
    }

    #[inline(always)]
    pub fn mask_source(&mut self) {
        self.mask_source = true;
    }

    #[inline(always)]
    pub fn mask_dest(&mut self) {
        self.mask_dest = true;
    }

    #[inline(always)]
    pub fn texcoordvelocity(&mut self, vel: [f32; 2]) {
        self.texv = Some(vel);
    }

    #[inline(always)]
    pub fn cropleft(&mut self, v: f32) {
        self.cl = v;
    }

    #[inline(always)]
    pub fn cropright(&mut self, v: f32) {
        self.cr = v;
    }

    #[inline(always)]
    pub fn croptop(&mut self, v: f32) {
        self.ct = v;
    }

    #[inline(always)]
    pub fn cropbottom(&mut self, v: f32) {
        self.cb = v;
    }

    #[inline(always)]
    pub fn fadeleft(&mut self, v: f32) {
        self.fl = v;
    }

    #[inline(always)]
    pub fn faderight(&mut self, v: f32) {
        self.fr = v;
    }

    #[inline(always)]
    pub fn fadetop(&mut self, v: f32) {
        self.ft = v;
    }

    #[inline(always)]
    pub fn fadebottom(&mut self, v: f32) {
        self.fb = v;
    }

    #[inline(always)]
    pub fn setstate(&mut self, i: u32) {
        self.cell = Some((i, u32::MAX));
        self.grid = None;
        self.uv = None;
    }

    #[inline(always)]
    pub fn animate(&mut self, v: bool) {
        self.anim_enable = v;
    }

    #[inline(always)]
    pub fn setallstatedelays(&mut self, s: f32) {
        self.state_delay = s.max(0.0);
    }

    #[inline(always)]
    pub fn customtexturerect(&mut self, uv: [f32; 4]) {
        self.uv = Some(uv);
        self.cell = None;
        self.grid = None;
    }

    #[inline(always)]
    pub fn visible(&mut self, v: bool) {
        self.vis = v;
    }

    #[inline(always)]
    pub fn rotationx(&mut self, d: f32) {
        self.rot_x = d;
    }

    #[inline(always)]
    pub fn rotationy(&mut self, d: f32) {
        self.rot_y = d;
    }

    #[inline(always)]
    pub fn rotationz(&mut self, d: f32) {
        self.rot_z = d;
    }

    #[inline(always)]
    pub fn addrotationx(&mut self, dd: f32) {
        self.rot_x += dd;
    }

    #[inline(always)]
    pub fn addrotationy(&mut self, dd: f32) {
        self.rot_y += dd;
    }

    #[inline(always)]
    pub fn addrotationz(&mut self, dd: f32) {
        self.rot_z += dd;
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
    pub fn strokecolor(&mut self, _rgba: [f32; 4]) {}

    #[inline(always)]
    pub fn font(&mut self, _font: &'static str) {}

    #[inline(always)]
    pub fn settext(&mut self, _content: TextContent) {}

    #[inline(always)]
    pub fn horizalign(&mut self, _align: TextAlign) {}

    #[inline(always)]
    pub fn wrapwidthpixels(&mut self, _w: f32) {}

    #[inline(always)]
    pub fn vertspacing(&mut self, _s: f32) {}

    #[inline(always)]
    pub fn maxwidth(&mut self, _w: f32) {}

    #[inline(always)]
    pub fn maxheight(&mut self, _h: f32) {}

    #[inline(always)]
    pub fn build(self, site_base: u64) -> Actor {
        self.build_with_native_dims(site_base, None)
    }

    #[inline(always)]
    pub fn build_with_native_dims(
        mut self,
        site_base: u64,
        native_dims: Option<[f32; 2]>,
    ) -> Actor {
        if !self.tw.is_empty() && self.w == 0.0 && self.h == 0.0 {
            if let Some([nw, nh]) = native_dims {
                self.w = nw;
                self.h = nh;
            }
        }

        if !self.tw.is_empty() {
            let mut init = anim::TweenState::default();
            init.x = self.x;
            init.y = self.y;
            init.w = self.w;
            init.h = self.h;
            init.hx = self.hx;
            init.vy = self.vy;
            init.tint = self.tint;
            init.glow = self.glow;
            init.visible = self.vis;
            init.flip_x = self.fx;
            init.flip_y = self.fy;
            init.rot_x = self.rot_x;
            init.rot_y = self.rot_y;
            init.rot_z = self.rot_z;
            init.fade_l = self.fl;
            init.fade_r = self.fr;
            init.fade_t = self.ft;
            init.fade_b = self.fb;
            init.crop_l = self.cl;
            init.crop_r = self.cr;
            init.crop_t = self.ct;
            init.crop_b = self.cb;
            init.scale = [self.sx, self.sy];

            let sid = runtime::site_id(site_base, self.tween_salt);
            let s = runtime::materialize(sid, init, &self.tw);

            self.x = s.x;
            self.y = s.y;
            self.w = s.w;
            self.h = s.h;
            self.hx = s.hx;
            self.vy = s.vy;
            self.tint = s.tint;
            self.glow = s.glow;
            self.vis = s.visible;
            self.fx = s.flip_x;
            self.fy = s.flip_y;
            self.rot_x = s.rot_x;
            self.rot_y = s.rot_y;
            self.rot_z = s.rot_z;
            self.fl = s.fade_l;
            self.fr = s.fade_r;
            self.ft = s.fade_t;
            self.fb = s.fade_b;
            self.cl = s.crop_l;
            self.cr = s.crop_r;
            self.ct = s.crop_t;
            self.cb = s.crop_b;
            self.sx = s.scale[0];
            self.sy = s.scale[1];
        }

        if self.sx < 0.0 {
            self.fx = !self.fx;
            self.sx = -self.sx;
        }
        if self.sy < 0.0 {
            self.fy = !self.fy;
            self.sy = -self.sy;
        }

        let scale_carry = if self.w != 0.0 || self.h != 0.0 {
            self.w *= self.sx;
            self.h *= self.sy;
            if self.w == 0.0 && self.h == 0.0 {
                [0.0, 0.0]
            } else {
                [1.0, 1.0]
            }
        } else {
            [self.sx, self.sy]
        };

        Actor::Sprite {
            align: [self.hx, self.vy],
            offset: [self.x, self.y],
            world_z: 0.0,
            size: [SizeSpec::Px(self.w), SizeSpec::Px(self.h)],
            source: self.source,
            tint: self.tint,
            glow: self.glow,
            z: self.z,
            cell: self.cell,
            grid: self.grid,
            uv_rect: self.uv,
            visible: self.vis,
            flip_x: self.fx,
            flip_y: self.fy,
            cropleft: self.cl,
            cropright: self.cr,
            croptop: self.ct,
            cropbottom: self.cb,
            fadeleft: self.fl,
            faderight: self.fr,
            fadetop: self.ft,
            fadebottom: self.fb,
            blend: self.blend,
            mask_source: self.mask_source,
            mask_dest: self.mask_dest,
            rot_x_deg: self.rot_x,
            rot_y_deg: self.rot_y,
            rot_z_deg: self.rot_z,
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            texcoordvelocity: self.texv,
            animate: self.anim_enable,
            state_delay: self.state_delay,
            scale: scale_carry,
            shadow_len: [self.shx, self.shy],
            shadow_color: self.shc,
            effect: self.effect,
        }
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
