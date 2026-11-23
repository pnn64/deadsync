use crate::resources::FONT;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadlib_present::space::{screen_height, screen_width};

pub const PAPER: [f32; 3] = [0.91, 0.96, 0.94];
pub const MUTED: [f32; 3] = [0.54, 0.68, 0.69];
pub const MINT: [f32; 3] = [0.20, 0.96, 0.72];
pub const AMBER: [f32; 3] = [1.0, 0.67, 0.30];
pub const PITCH: [f32; 3] = [0.035, 0.08, 0.12];

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 480.0;

#[derive(Clone, Copy)]
pub(crate) struct Scene {
    origin: [f32; 2],
    scale: f32,
    alpha: f32,
}

impl Scene {
    pub(crate) fn new(alpha: f32) -> Self {
        let [w, h] = [screen_width(), screen_height()];
        let scale = (w / WIDTH).min(h / HEIGHT);
        Self {
            origin: [(w - WIDTH * scale) * 0.5, (h - HEIGHT * scale) * 0.5],
            scale,
            alpha: alpha.clamp(0.0, 1.0),
        }
    }

    pub(crate) fn cover(self, actors: &mut Vec<Actor>) {
        self.fill_screen(actors, PITCH, -100);
        for y in [23.0, 456.0] {
            self.rect(actors, [28.0, y], [584.0, 1.0], MUTED, 0.35, -80);
        }
    }

    pub(crate) fn fill_screen(self, actors: &mut Vec<Actor>, ink: [f32; 3], z: i16) {
        actors.push(quad(
            [0.0, 0.0],
            [screen_width(), screen_height()],
            color(ink, self.alpha),
            z,
        ));
    }

    pub(crate) fn rect(
        self,
        actors: &mut Vec<Actor>,
        at: [f32; 2],
        size: [f32; 2],
        ink: [f32; 3],
        opacity: f32,
        z: i16,
    ) {
        actors.push(quad(
            self.xy(at),
            size.map(|value| value * self.scale),
            color(ink, opacity * self.alpha),
            z,
        ));
    }

    pub(crate) fn text(
        self,
        actors: &mut Vec<Actor>,
        at: [f32; 2],
        [zoom, width]: [f32; 2],
        text: impl Into<TextContent>,
        ink: [f32; 3],
    ) {
        let mut actor = self.text_builder(at, zoom, text.into(), ink);
        actor.align(0.0, 0.5);
        actor.maxwidth(width * self.scale);
        actors.push(actor.build(0));
    }

    pub(crate) fn paragraph(
        self,
        actors: &mut Vec<Actor>,
        at: [f32; 2],
        [width, height]: [f32; 2],
        zoom: f32,
        text: impl Into<TextContent>,
        ink: [f32; 3],
    ) {
        let mut actor = self.text_builder(at, zoom, text.into(), ink);
        actor.align(0.0, 0.0);
        actor.wrapwidthpixels(width / zoom);
        actor.maxheight(height * self.scale);
        actors.push(actor.build(0));
    }

    pub(crate) fn with_alpha(self, alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            ..self
        }
    }

    pub(crate) fn text_builder(
        self,
        at: [f32; 2],
        zoom: f32,
        text: TextContent,
        ink: [f32; 3],
    ) -> TextBuilder {
        let [x, y] = self.xy(at);
        let mut actor = TextBuilder::new();
        actor.xy(x, y);
        actor.font(FONT);
        actor.settext(text);
        actor.zoom(zoom * self.scale);
        actor.diffuse(color(ink, self.alpha));
        actor.z(10);
        actor
    }

    pub(crate) fn banner(
        self,
        actors: &mut Vec<Actor>,
        center: [f32; 2],
        size: [f32; 2],
        texture: &std::sync::Arc<str>,
    ) {
        let [x, y] = self.xy(center);
        let [w, h] = size.map(|length| length * self.scale);
        let mut banner =
            deadlib_assets::present_dsl::cover_sprite(texture.clone(), x, y, w, h, 1.0, 0);
        if let Actor::Sprite { tint, .. } = &mut banner {
            tint[3] = self.alpha;
        }
        actors.push(banner);
    }

    pub(crate) fn motif(self, actors: &mut Vec<Actor>, at: [f32; 2], time: f32) {
        for (index, peak) in [0.45, 0.84, 0.64, 1.0, 0.56, 0.76, 0.32]
            .into_iter()
            .enumerate()
        {
            let pulse = 0.88 + 0.12 * (time * 4.2 - index as f32 * 0.8).sin();
            let height = 67.0 * peak * pulse;
            self.rect(
                actors,
                [at[0] + index as f32 * 26.0, at[1] - height],
                [18.0, height],
                if index == 3 { AMBER } else { MINT },
                0.9,
                0,
            );
        }
    }

    pub(crate) fn xy(self, point: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|i| point[i].mul_add(self.scale, self.origin[i]))
    }
}

fn color([r, g, b]: [f32; 3], alpha: f32) -> [f32; 4] {
    [r, g, b, alpha]
}

fn quad([x, y]: [f32; 2], [w, h]: [f32; 2], rgba: [f32; 4], z: i16) -> Actor {
    let mut actor = SpriteBuilder::solid();
    actor.align(0.0, 0.0);
    actor.xy(x, y);
    actor.size(w, h);
    actor.diffuse(rgba);
    actor.z(z);
    actor.build(0)
}
