//! Color parsing, channel conversion, interpolation, and desaturation.

pub use deadlib_render_core::color::Color;

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
