//! A small ARGB color type used by config options that carry colors (such as
//! the gameplay background color).

/// An ARGB color. Each channel is a linear value in the range `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub a: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self {
        a: 1.0,
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    /// Build an opaque color (alpha = 1.0) from RGB channels.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { a: 1.0, r, g, b }
    }

    /// Channels as an `[r, g, b, a]` array for the renderer's tint/diffuse.
    pub fn to_rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Parse a hex color string (case-insensitive, trimmed, optional leading
    /// `#`). Accepts both 6-digit `RRGGBB` (opaque) and 8-digit `AARRGGBB`
    /// forms. Returns `None` for malformed input so the caller can fall back to
    /// a default.
    pub fn from_hex(raw: &str) -> Option<Self> {
        let hex = raw.trim().trim_start_matches('#');
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();
        let chan = |idx: usize| Some(byte(idx)? as f32 / 255.0);
        match hex.len() {
            6 => Some(Self {
                a: 1.0,
                r: chan(0)?,
                g: chan(2)?,
                b: chan(4)?,
            }),
            8 => Some(Self {
                a: chan(0)?,
                r: chan(2)?,
                g: chan(4)?,
                b: chan(6)?,
            }),
            _ => None,
        }
    }

    /// Format as an uppercase hex string: `#RRGGBB` when fully opaque, otherwise
    /// `#AARRGGBB`. Round-trips with [`Color::from_hex`].
    pub fn to_hex(self) -> String {
        let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (r, g, b) = (channel(self.r), channel(self.g), channel(self.b));
        let a = channel(self.a);
        if a == 255 {
            format!("#{r:02X}{g:02X}{b:02X}")
        } else {
            format!("#{a:02X}{r:02X}{g:02X}{b:02X}")
        }
    }
}
