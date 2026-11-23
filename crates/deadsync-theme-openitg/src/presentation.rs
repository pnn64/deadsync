use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder, TweenSteps};
use deadlib_present::font::{self, Font, FontMap};
use deadlib_present::space::{screen_height, screen_width};
use deadlib_present::{anim, runtime};

pub(crate) const WIDTH: f32 = 640.0;
pub(crate) const HEIGHT: f32 = 480.0;
pub(crate) const INK: [f32; 3] = [0.92, 0.95, 0.99];
pub(crate) const MUTED: [f32; 3] = [0.57, 0.65, 0.74];
pub(crate) const ACCENT: [f32; 3] = [1.0, 0.69, 0.25];

#[derive(Clone, Copy)]
pub(crate) struct TextMetrics<'a> {
    fonts: &'a FontMap,
    font: Option<&'a Font>,
}

impl<'a> TextMetrics<'a> {
    pub(crate) fn new(fonts: &'a FontMap, font_name: &'static str) -> Self {
        Self {
            fonts,
            font: fonts.get(font_name),
        }
    }

    pub(crate) fn chain_key(self) -> Option<u64> {
        self.font.map(|font| font.chain_key)
    }

    pub(crate) fn width(self, text: &str) -> f32 {
        let text = font::replace_markers(text);
        let width = self.font.map_or_else(
            || text.chars().count() as i32 * 8,
            |font| {
                text.split('\n')
                    .map(|line| font::measure_line_width_logical(font, line, self.fonts))
                    .max()
                    .unwrap_or_default()
            },
        );
        (width + (width & 1)) as f32
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LinearMotion<const N: usize> {
    from: [f32; N],
    pub(crate) target: [f32; N],
    started: f64,
}

impl<const N: usize> Default for LinearMotion<N> {
    fn default() -> Self {
        Self::at([0.0; N])
    }
}

impl<const N: usize> LinearMotion<N> {
    pub(crate) const fn at(target: [f32; N]) -> Self {
        Self {
            from: target,
            target,
            started: 0.0,
        }
    }

    pub(crate) fn sample(self, now: f64, duration: f64) -> [f32; N] {
        let elapsed = now - self.started;
        if elapsed >= duration || self.from == self.target {
            return self.target;
        }
        let t = (elapsed.max(0.0) / duration) as f32;
        std::array::from_fn(|index| {
            (self.target[index] - self.from[index]).mul_add(t, self.from[index])
        })
    }

    pub(crate) fn retarget(&mut self, target: [f32; N], now: f64, duration: f64) {
        if self.target != target {
            self.from = self.sample(now, duration);
            self.target = target;
            self.started = now;
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Layout {
    left: f32,
    top: f32,
    scale: f32,
}

impl Layout {
    pub(crate) fn centered(width: f32, height: f32) -> Self {
        let scale = (width / WIDTH).min(height / HEIGHT);
        Self {
            left: (width - WIDTH * scale) * 0.5,
            top: (height - HEIGHT * scale) * 0.5,
            scale,
        }
    }

    pub(crate) fn translated_x(self, offset: f32) -> Self {
        Self {
            left: self.left + offset,
            ..self
        }
    }

    pub(crate) fn position(self, x: f32, y: f32) -> [f32; 2] {
        [self.left + x * self.scale, self.top + y * self.scale]
    }

    pub(crate) fn size(self, size: [f32; 2]) -> [f32; 2] {
        size.map(|value| value * self.scale)
    }

    pub(crate) fn panel(self, x: f32, y: f32, size: [f32; 2], rgba: [f32; 4], z: i16) -> Actor {
        quad(self.position(x, y), self.size(size), rgba, z)
    }

    pub(crate) fn label(
        self,
        position: [f32; 2],
        alignment: f32,
        content: TextContent,
        zoom: f32,
        rgba: [f32; 4],
    ) -> Actor {
        self.text(position, alignment, content, zoom, rgba).build(0)
    }

    pub(crate) fn bounded_label(
        self,
        position: [f32; 2],
        alignment: f32,
        width: f32,
        content: TextContent,
        zoom: f32,
        rgba: [f32; 4],
    ) -> Actor {
        let mut text = self.text(position, alignment, content, zoom, rgba);
        text.maxwidth(width * self.scale);
        text.build(0)
    }

    pub(crate) fn paragraph(
        self,
        position: [f32; 2],
        [width, height]: [f32; 2],
        content: TextContent,
        zoom: f32,
        rgba: [f32; 4],
    ) -> Actor {
        let mut text = self.text(position, 0.0, content, zoom, rgba);
        text.valign(0.0);
        text.wrapwidthpixels(width / zoom);
        text.maxheight(height * self.scale);
        text.build(0)
    }

    pub(crate) fn image(
        self,
        [x, y]: [f32; 2],
        [width, height]: [f32; 2],
        key: &std::sync::Arc<str>,
        z: i16,
    ) -> Actor {
        let [x, y] = self.position(x, y);
        deadlib_assets::present_dsl::cover_sprite(key.clone(), x, y, width, height, self.scale, z)
    }

    pub(crate) fn text(
        self,
        [x, y]: [f32; 2],
        alignment: f32,
        content: TextContent,
        zoom: f32,
        rgba: [f32; 4],
    ) -> TextBuilder {
        let [x, y] = self.position(x, y);
        let mut actor = TextBuilder::new();
        actor.xy(x, y);
        actor.halign(alignment);
        actor.zoom(zoom * self.scale);
        actor.font("miso");
        actor.settext(content);
        actor.diffuse(rgba);
        actor.z(16);
        actor
    }
}

pub(crate) fn quad(position: [f32; 2], size: [f32; 2], rgba: [f32; 4], z: i16) -> Actor {
    let mut actor = SpriteBuilder::solid();
    actor.align(0.0, 0.0);
    actor.xy(position[0], position[1]);
    actor.size(size[0], size[1]);
    actor.diffuse(rgba);
    actor.z(z);
    actor.build(0)
}

pub(crate) fn sprite(
    key: &'static str,
    [x, y]: [f32; 2],
    [width, height]: [f32; 2],
    rgba: [f32; 4],
    z: i16,
) -> SpriteBuilder {
    let mut sprite = SpriteBuilder::static_texture(key);
    sprite.align(0.5, 0.5);
    sprite.xy(x, y);
    sprite.size(width, height);
    sprite.diffuse(rgba);
    sprite.z(z);
    sprite
}

pub(crate) fn italic(mut actor: Actor, shear: f32) -> Actor {
    if let Actor::Text {
        local_transform, ..
    } = &mut actor
    {
        local_transform.y_axis.x = shear;
    }
    actor
}

pub(crate) fn push_red_backdrop(
    actors: &mut Vec<Actor>,
    [width, height]: [f32; 2],
    elapsed: f32,
    z: i16,
) {
    for (y, rotation) in [(0.25 * height, 0.0), (0.75 * height, 180.0)] {
        let mut half = sprite(
            "openitg/selection-backdrop.png",
            [width / 2.0, y],
            [width, height / 2.0],
            [1.0; 4],
            z,
        );
        half.rotationz(rotation);
        actors.push(half.build(0));
    }
    let scale = height / HEIGHT;
    for (index, y) in [91.0, 127.0, 172.0, 308.0, 350.0, 391.0]
        .into_iter()
        .enumerate()
    {
        let arrows = index % 2 == 0;
        let phase = elapsed * (0.44 + 0.07 * index as f32);
        let x = width / 2.0 + phase.sin() * 52.0 * scale;
        let key = if arrows {
            "openitg/selection-arrows.png"
        } else {
            "openitg/selection-dots.png"
        };
        actors.push(
            sprite(
                key,
                [x, y * scale],
                [
                    width + 112.0 * scale,
                    if arrows { 18.0 } else { 9.0 } * scale,
                ],
                [0.86, 0.58, 0.58, if arrows { 0.19 } else { 0.22 }],
                z + 1,
            )
            .build(0),
        );
    }
}

pub(crate) fn push_red_streaks(
    actors: &mut Vec<Actor>,
    [width, height]: [f32; 2],
    reveal: f32,
    z: i16,
) {
    let scale = height / HEIGHT;
    for y in [42.0, 446.0] {
        let mut band = sprite(
            crate::resources::TITLE_STREAK,
            [width / 2.0, y * scale],
            [width, 34.0 * scale],
            [1.0; 4],
            z,
        );
        band.cropright(1.0 - reveal);
        actors.push(band.build(0));
    }
}

pub(crate) fn black_fade(duration: f32, arriving: bool) -> Actor {
    const SITE: u64 = runtime::site_base(file!(), line!(), column!());
    let (start, end) = if arriving { (1.0, 0.0) } else { (0.0, 1.0) };
    let mut cover = SpriteBuilder::solid();
    cover.tweensalt(u64::from(arriving));
    cover.align(0.0, 0.0);
    cover.size(screen_width(), screen_height());
    cover.diffuse([0.0, 0.0, 0.0, start]);
    cover.z(1100);
    cover.build_tweened(SITE, || {
        let mut steps = TweenSteps::new();
        steps.push(anim::linear(duration).alpha(end).build());
        steps
    })
}
