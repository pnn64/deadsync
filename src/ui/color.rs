/// Accepts "#rgb", "#rgba", "#rrggbb", "#rrggbbaa" (or without '#').
/// Panics on invalid input; use only with trusted literals.
pub fn rgba_hex(s: &str) -> [f32; 4] {
    #[inline(always)]
    fn nib(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => 10 + (b - b'a'),
            b'A'..=b'F' => 10 + (b - b'A'),
            _ => panic!("invalid hex digit"),
        }
    }
    #[inline(always)]
    fn byte2(h: u8, l: u8) -> u8 {
        (nib(h) << 4) | nib(l)
    }
    #[inline(always)]
    fn rep(n: u8) -> u8 {
        (n << 4) | n
    }

    let bytes = s.as_bytes();
    let off = (bytes.first() == Some(&b'#')) as usize;
    let n = bytes.len() - off;
    let b = &bytes[off..];

    let (r, g, bl, a) = match n {
        3 => (rep(b[0]), rep(b[1]), rep(b[2]), 0xFF),
        4 => (rep(b[0]), rep(b[1]), rep(b[2]), rep(b[3])),
        6 => (
            byte2(b[0], b[1]),
            byte2(b[2], b[3]),
            byte2(b[4], b[5]),
            0xFF,
        ),
        8 => (
            byte2(b[0], b[1]),
            byte2(b[2], b[3]),
            byte2(b[4], b[5]),
            byte2(b[6], b[7]),
        ),
        _ => panic!("hex must be 3/4/6/8 digits"),
    };

    [
        (r as f32) / 255.0,
        (g as f32) / 255.0,
        (bl as f32) / 255.0,
        (a as f32) / 255.0,
    ]
}

/* =========================== THEME PALETTES =========================== */

/// Start at #C1006F in the decorative palette.
pub const DEFAULT_COLOR_INDEX: i32 = 2;

/// Decorative / sprite tint palette (hearts, backgrounds, sprites)
pub const DECORATIVE_HEX: [&str; 12] = [
    "#FF3C23", "#FF003C", "#C1006F", "#8200A1", "#413AD0", "#0073FF", "#00ADC0", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];

/// Simply Love-ish UI accent palette (text highlights, etc.)
pub const SIMPLY_LOVE_HEX: [&str; 12] = [
    "#FF5D47", "#FF577E", "#FF47B3", "#DD57FF", "#8885ff", "#3D94FF", "#00B8CC", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];

/// Judgment colors for the statistics display, ordered from best to worst.
pub const JUDGMENT_HEX: [&str; 6] = [
    "#21CCE8", // Fantastic
    "#E29C18", // Excellent
    "#66C955", // Great
    "#B45CFF", // Decent
    "#C9855E", // Way Off
    "#FF3030", // Miss
];

/// Dimmed judgment colors for leading zeros in stats displays.
pub const JUDGMENT_DIM_HEX: [&str; 6] = [
    "#0C4E59", "#593D09", "#2D5925", "#3F2059", "#593B29", "#591010",
];

/// Dimmed judgment colors for the evaluation screen stats.
pub const JUDGMENT_DIM_EVAL_HEX: [&str; 6] = [
    "#08363E", "#3C2906", "#1B3516", "#301844", "#352319", "#440C0C",
];

/// Undimmed white Fantastic color for FA+ (outer window).
pub const JUDGMENT_FA_PLUS_WHITE_HEX: &str = "#FFFFFF";

/// Dimmed white Fantastic color for FA+ eval pane.
pub const JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_HEX: &str = "#444444";

/// Dimmed white Fantastic color for FA+ gameplay side pane.
pub const JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_HEX: &str = "#595959";

/// Difficulty names as they appear in simfiles. Used for parsing and lookups.
pub const FILE_DIFFICULTY_NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Challenge"];
/// Difficulty names as they should be displayed in the UI.
pub const DISPLAY_DIFFICULTY_NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Expert"];

/// Returns the Simply Love color for a given difficulty, based on an active theme color index.
#[inline(always)]
pub fn difficulty_rgba(difficulty_name: &str, active_color_index: i32) -> [f32; 4] {
    let difficulty_index = FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&name| name.eq_ignore_ascii_case(difficulty_name))
        .unwrap_or(2); // Default to Medium if not found

    let color_index = active_color_index - (4 - difficulty_index) as i32;
    simply_love_rgba(color_index)
}

#[inline(always)]
fn wrap(n: usize, i: i32) -> usize {
    (i.rem_euclid(n as i32)) as usize
}

#[inline(always)]
pub fn decorative_rgba(idx: i32) -> [f32; 4] {
    rgba_hex(DECORATIVE_HEX[wrap(DECORATIVE_HEX.len(), idx)])
}

#[inline(always)]
pub fn simply_love_rgba(idx: i32) -> [f32; 4] {
    rgba_hex(SIMPLY_LOVE_HEX[wrap(SIMPLY_LOVE_HEX.len(), idx)])
}

/// Simply Love `LightenColor(c)` parity: multiplies RGB by 1.25, keeps alpha.
#[inline(always)]
pub fn lighten_rgba(c: [f32; 4]) -> [f32; 4] {
    [c[0] * 1.25, c[1] * 1.25, c[2] * 1.25, c[3]]
}

/// Menu selected color rule: “current SIMPLY_LOVE minus 2”
#[inline(always)]
pub fn menu_selected_rgba(active_idx: i32) -> [f32; 4] {
    simply_love_rgba(active_idx - 2)
}
