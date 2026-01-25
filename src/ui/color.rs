/// Accepts "#rgb", "#rgba", "#rrggbb", "#rrggbbaa" (or without '#').
/// Panics on invalid input; use only with trusted literals.
/// Evaluated at COMPILE TIME if assigned to a const/static.
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
        $crate::ui::color::rgba_hex($hex)
    };
}

#[macro_export]
macro_rules! rgba_const {
    ($name:ident, $hex:literal $(,)?) => {
        const $name: [f32; 4] = $crate::ui::color::rgba_hex($hex);
    };
    ($vis:vis $name:ident, $hex:literal $(,)?) => {
        $vis const $name: [f32; 4] = $crate::ui::color::rgba_hex($hex);
    };
}

/* =========================== THEME PALETTES =========================== */

/// Start at #C1006F in the decorative palette.
pub const DEFAULT_COLOR_INDEX: i32 = 2;

pub const FILE_DIFFICULTY_NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Challenge"];
pub const DISPLAY_DIFFICULTY_NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Challenge"];

/// Decorative / sprite tint palette (hearts, backgrounds, sprites)
pub const DECORATIVE_RGBA: [[f32; 4]; 12] = [
    rgba_hex("#FF3C23"),
    rgba_hex("#FF003C"),
    rgba_hex("#C1006F"),
    rgba_hex("#8200A1"),
    rgba_hex("#413AD0"),
    rgba_hex("#0073FF"),
    rgba_hex("#00ADC0"),
    rgba_hex("#5CE087"),
    rgba_hex("#AEFA44"),
    rgba_hex("#FFFF00"),
    rgba_hex("#FFBE00"),
    rgba_hex("#FF7D00"),
];

/// Simply Love-ish UI accent palette
pub const SIMPLY_LOVE_RGBA: [[f32; 4]; 12] = [
    rgba_hex("#FF5D47"),
    rgba_hex("#FF577E"),
    rgba_hex("#FF47B3"),
    rgba_hex("#DD57FF"),
    rgba_hex("#8885ff"),
    rgba_hex("#3D94FF"),
    rgba_hex("#00B8CC"),
    rgba_hex("#5CE087"),
    rgba_hex("#AEFA44"),
    rgba_hex("#FFFF00"),
    rgba_hex("#FFBE00"),
    rgba_hex("#FF7D00"),
];

/// Judgment colors
pub const JUDGMENT_RGBA: [[f32; 4]; 6] = [
    rgba_hex("#21CCE8"), // Fantastic
    rgba_hex("#E29C18"), // Excellent
    rgba_hex("#66C955"), // Great
    rgba_hex("#B45CFF"), // Decent
    rgba_hex("#C9855E"), // Way Off
    rgba_hex("#FF3030"), // Miss
];

/// Dimmed judgment colors
pub const JUDGMENT_DIM_RGBA: [[f32; 4]; 6] = [
    rgba_hex("#0C4E59"),
    rgba_hex("#593D09"),
    rgba_hex("#2D5925"),
    rgba_hex("#3F2059"),
    rgba_hex("#593B29"),
    rgba_hex("#591010"),
];

/// Dimmed judgment colors for eval
pub const JUDGMENT_DIM_EVAL_RGBA: [[f32; 4]; 6] = [
    rgba_hex("#08363E"),
    rgba_hex("#3C2906"),
    rgba_hex("#1B3516"),
    rgba_hex("#301844"),
    rgba_hex("#352319"),
    rgba_hex("#440C0C"),
];

pub const JUDGMENT_FA_PLUS_WHITE_RGBA: [f32; 4] = rgba_hex("#FFFFFF");
pub const JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA: [f32; 4] = rgba_hex("#444444");
pub const JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA: [f32; 4] = rgba_hex("#595959");

pub const EDIT_DIFFICULTY_RGBA: [f32; 4] = rgba_hex("#B4B7BA");

/// Returns the Simply Love color for a given difficulty, based on an active theme color index.
#[inline(always)]
pub fn difficulty_rgba(difficulty_name: &str, active_color_index: i32) -> [f32; 4] {
    if difficulty_name.eq_ignore_ascii_case("edit") {
        return EDIT_DIFFICULTY_RGBA;
    }
    let difficulty_index = FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&name| name.eq_ignore_ascii_case(difficulty_name))
        .unwrap_or(2); // Default to Medium if not found

    let color_index = active_color_index - (4 - difficulty_index) as i32;
    simply_love_rgba(color_index)
}

#[inline(always)]
const fn wrap(n: usize, i: i32) -> usize {
    (i.rem_euclid(n as i32)) as usize
}

#[inline(always)]
pub fn decorative_rgba(idx: i32) -> [f32; 4] {
    DECORATIVE_RGBA[wrap(DECORATIVE_RGBA.len(), idx)]
}

#[inline(always)]
pub fn simply_love_rgba(idx: i32) -> [f32; 4] {
    SIMPLY_LOVE_RGBA[wrap(SIMPLY_LOVE_RGBA.len(), idx)]
}

/// Simply Love `LightenColor(c)` parity: multiplies RGB by 1.25, keeps alpha.
#[inline(always)]
pub fn lighten_rgba(c: [f32; 4]) -> [f32; 4] {
    [c[0] * 1.25, c[1] * 1.25, c[2] * 1.25, c[3]]
}

/// Menu selected color rule: “current `SIMPLY_LOVE` minus 2”
#[inline(always)]
pub fn menu_selected_rgba(active_idx: i32) -> [f32; 4] {
    simply_love_rgba(active_idx - 2)
}
