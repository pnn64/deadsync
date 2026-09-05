//! Color parsing, channel conversion, interpolation, and desaturation.

/// Accepts "#rgb", "#rgba", "#rrggbb", "#rrggbbaa" (or without '#').
/// Panics on invalid input; use only with trusted literals.
/// Evaluated at COMPILE TIME if assigned to a const/static.
#[must_use]
/// # Panics
///
/// Panics if an internal state invariant is violated.
pub const fn rgba_hex(s: &str) -> [f32; 4] {
    let bytes = s.as_bytes();

    // Handle optional '#' by offsetting start index
    let (bytes, len) = if !bytes.is_empty() && bytes[0] == b'#' {
        let (_, rem) = bytes.split_at(1);
        (rem, s.len() - 1)
    } else {
        (bytes, s.len())
    };

    // Const-safe hex char to u8
    const fn val(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => 10 + (b - b'a'),
            b'A'..=b'F' => 10 + (b - b'A'),
            _ => panic!("invalid hex digit in color string"),
        }
    }

    // Combine two hex digits into a byte
    const fn byte2(h: u8, l: u8) -> u8 {
        (val(h) << 4) | val(l)
    }

    // Expand 4-bit color to 8-bit (e.g. F -> FF)
    const fn rep(n: u8) -> u8 {
        (val(n) << 4) | val(n)
    }

    let (r, g, b, a) = match len {
        3 => (rep(bytes[0]), rep(bytes[1]), rep(bytes[2]), 0xFF),
        4 => (rep(bytes[0]), rep(bytes[1]), rep(bytes[2]), rep(bytes[3])),
        6 => (
            byte2(bytes[0], bytes[1]),
            byte2(bytes[2], bytes[3]),
            byte2(bytes[4], bytes[5]),
            0xFF,
        ),
        8 => (
            byte2(bytes[0], bytes[1]),
            byte2(bytes[2], bytes[3]),
            byte2(bytes[4], bytes[5]),
            byte2(bytes[6], bytes[7]),
        ),
        _ => panic!("color hex string must be 3, 4, 6, or 8 digits"),
    };

    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

/// ARGB color used by runtime/configurable presentation values.
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
    #[must_use]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { a: 1.0, r, g, b }
    }

    /// Build a color from render-order `[r, g, b, a]` channels.
    #[must_use]
    pub const fn from_rgba(rgba: [f32; 4]) -> Self {
        Self {
            a: rgba[3],
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
        }
    }

    /// Channels as an `[r, g, b, a]` array for render tint/diffuse values.
    #[must_use]
    pub const fn to_rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Parse a config hex color string: trimmed, optional leading `#`,
    /// 6-digit `RRGGBB` or 8-digit `AARRGGBB`.
    #[must_use]
    pub fn from_hex(raw: &str) -> Option<Self> {
        let hex = raw.trim().trim_start_matches('#');
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();
        let chan = |idx: usize| Some(f32::from(byte(idx)?) / 255.0);
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

    /// Format as `#RRGGBB` when opaque, otherwise `#AARRGGBB`.
    #[must_use]
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

#[macro_export]
macro_rules! rgba {
    ($hex:literal $(,)?) => {
        $crate::color::rgba_hex($hex)
    };
}

#[macro_export]
macro_rules! rgba_const {
    ($name:ident, $hex:literal $(,)?) => {
        const $name: [f32; 4] = $crate::color::rgba_hex($hex);
    };
    ($vis:vis $name:ident, $hex:literal $(,)?) => {
        $vis const $name: [f32; 4] = $crate::color::rgba_hex($hex);
    };
}

/// Interpolate two scalar values with an unclamped factor.
#[must_use]
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

/// Interpolate RGBA channels with an unclamped factor.
#[must_use]
#[inline]
pub fn lerp_color(t: f32, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
        lerp(a[3], b[3], t),
    ]
}

/// Desaturate RGB toward luminance, preserving alpha and clamping the amount.
#[must_use]
#[inline]
pub fn desaturate_rgb(mut c: [f32; 4], desat: f32) -> [f32; 4] {
    let d = desat.clamp(0.0, 1.0);
    if d <= 0.0 {
        return c;
    }
    let luma = (0.3 * c[0]).mul_add(1.0, (0.59 * c[1]).mul_add(1.0, 0.11 * c[2]));
    c[0] = c[0] + d * (luma - c[0]);
    c[1] = c[1] + d * (luma - c[1]);
    c[2] = c[2] + d * (luma - c[2]);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_color_hex_parses_rgb_and_argb() {
        assert_eq!(Color::from_hex("#000000"), Some(Color::BLACK));
        assert_eq!(Color::from_hex("FFFFFF"), Some(Color::rgb(1.0, 1.0, 1.0)));
        let gray = Color::from_hex("#0C0C0C").unwrap();
        assert_eq!(
            gray.to_rgba(),
            [12.0 / 255.0, 12.0 / 255.0, 12.0 / 255.0, 1.0]
        );

        let c = Color::from_hex("#8001FE7F").unwrap();
        assert_eq!(c.a, 128.0 / 255.0);
        assert_eq!(c.r, 1.0 / 255.0);
        assert_eq!(c.g, 254.0 / 255.0);
        assert_eq!(c.b, 127.0 / 255.0);
    }

    #[test]
    fn config_color_hex_rejects_malformed_input() {
        assert_eq!(Color::from_hex(""), None);
        assert_eq!(Color::from_hex("#FFF"), None);
        assert_eq!(Color::from_hex("#GGGGGG"), None);
        assert_eq!(Color::from_hex("#1234567"), None);
        assert_eq!(Color::from_hex("#123456789"), None);
    }

    #[test]
    fn config_color_hex_is_case_insensitive_and_trims() {
        assert_eq!(Color::from_hex("  #0c0c0c  "), Color::from_hex("#0C0C0C"));
        assert_eq!(
            Color::from_hex("  80ffffff  "),
            Color::from_hex("#80FFFFFF")
        );
    }

    #[test]
    fn config_color_hex_formats_uppercase() {
        assert_eq!(Color::from_hex("#0C0C0C").unwrap().to_hex(), "#0C0C0C");
        assert_eq!(Color::from_hex("#8001FE7F").unwrap().to_hex(), "#8001FE7F");
    }
}
