//! ARGB color values and their hexadecimal representation.

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
        let digit = |byte: u8| {
            let number = byte.wrapping_sub(b'0');
            if number < 10 {
                return Some(number);
            }
            let letter = (byte | 0x20).wrapping_sub(b'a');
            if letter < 6 { Some(letter + 10) } else { None }
        };
        let bytes = hex.as_bytes();
        let byte = |idx: usize| Some((digit(bytes[idx])? << 4) | digit(bytes[idx + 1])?);
        let (a, r, g, b) = match bytes.len() {
            6 => (255, byte(0)?, byte(2)?, byte(4)?),
            8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
            _ => return None,
        };
        Some(Self {
            a: f32::from(a) / 255.0,
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
        })
    }

    /// Format as `#RRGGBB` when opaque, otherwise `#AARRGGBB`.
    #[must_use]
    pub fn to_hex(self) -> String {
        // Widen before the half bias; the u8 cast also clamps and maps NaN to zero.
        let channel = |v: f32| (f64::from(v * 255.0) + 0.5) as u8;
        let (r, g, b) = (channel(self.r), channel(self.g), channel(self.b));
        let a = channel(self.a);
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let channels = [a, r, g, b];
        let channels = if a == 255 {
            &channels[1..]
        } else {
            &channels[..]
        };
        let mut out = String::with_capacity(1 + channels.len() * 2);
        out.push('#');
        for &channel in channels {
            out.push(char::from(HEX[usize::from(channel >> 4)]));
            out.push(char::from(HEX[usize::from(channel & 15)]));
        }
        out
    }
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
