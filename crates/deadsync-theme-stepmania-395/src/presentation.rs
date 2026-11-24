use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadlib_present::space::{screen_height, screen_width};

use crate::resources::NORMAL_FONT;

pub(crate) fn push_footer(actors: &mut Vec<Actor>, value: &str, alpha: f32) {
    let layout = Layout::current();
    actors.push(
        layout
            .sprite("stepmania-395/footer", [320.0, 464.0], [640.0, 32.0], 95)
            .build(0),
    );
    let content =
        TextContent::inline_str(value).unwrap_or_else(|| TextContent::Owned(value.to_owned()));
    let mut help = layout.text(NORMAL_FONT, content, [320.0, 456.0], 0.5);
    help.zoomy(0.5 * layout.scale * alpha);
    help.maxwidth(600.0 * layout.scale);
    help.z(97);
    actors.push(help.build(0));
}

/// The original canvas fitted to the current viewport.
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub scale: f32,
    left: f32,
    top: f32,
}

impl Layout {
    pub fn current() -> Self {
        let scale = (screen_width() / 640.0).min(screen_height() / 480.0);
        Self {
            scale,
            left: (screen_width() - 640.0 * scale) * 0.5,
            top: (screen_height() - 480.0 * scale) * 0.5,
        }
    }

    pub fn position(self, [x, y]: [f32; 2]) -> [f32; 2] {
        [self.left + x * self.scale, self.top + y * self.scale]
    }

    pub fn translated(self, x: f32) -> Self {
        Self {
            left: self.left + x * self.scale,
            ..self
        }
    }

    pub fn sprite(self, key: &'static str, at: [f32; 2], size: [f32; 2], z: i16) -> SpriteBuilder {
        let [x, y] = self.position(at);
        let mut sprite = SpriteBuilder::static_texture(key);
        sprite.xy(x, y);
        sprite.size(size[0] * self.scale, size[1] * self.scale);
        sprite.z(z);
        sprite
    }

    pub fn solid(self, at: [f32; 2], size: [f32; 2], rgba: [f32; 4], z: i16) -> Actor {
        let [x, y] = self.position(at);
        let mut sprite = SpriteBuilder::solid();
        sprite.xy(x, y);
        sprite.size(size[0] * self.scale, size[1] * self.scale);
        sprite.diffuse(rgba);
        sprite.z(z);
        sprite.build(0)
    }

    pub fn text(
        self,
        font: &'static str,
        value: TextContent,
        at: [f32; 2],
        zoom: f32,
    ) -> TextBuilder {
        let [x, y] = self.position(at);
        let mut text = TextBuilder::new();
        text.font(font);
        text.settext(value);
        text.xy(x, y);
        text.zoom(zoom * self.scale);
        text.z(1);
        text
    }
}
