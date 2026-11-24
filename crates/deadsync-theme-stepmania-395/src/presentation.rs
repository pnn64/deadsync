use deadlib_present::actors::TextContent;
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadlib_present::space::{screen_height, screen_width};

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

    pub fn sprite(self, key: &'static str, at: [f32; 2], size: [f32; 2], z: i16) -> SpriteBuilder {
        let [x, y] = self.position(at);
        let mut sprite = SpriteBuilder::static_texture(key);
        sprite.xy(x, y);
        sprite.size(size[0] * self.scale, size[1] * self.scale);
        sprite.z(z);
        sprite
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
