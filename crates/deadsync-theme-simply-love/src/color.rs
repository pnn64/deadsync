//! Simply Love accents, judgment defaults, and difficulty display policy.

use deadlib_present::color::rgba_hex;
use deadsync_theme::color::{DifficultyColorScheme, JudgmentPalette, JudgmentPalettePreset};

/// Start at #C1006F in the decorative palette.
pub const DEFAULT_COLOR_INDEX: i32 = 2;

pub const FILE_DIFFICULTY_NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Challenge"];
pub const DISPLAY_DIFFICULTY_NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Challenge"];
pub const ZMOD_DISPLAY_DIFFICULTY_NAMES: [&str; 5] =
    ["Beginner", "Easy", "Medium", "Hard", "Expert"];

#[inline(always)]
const fn contains_ascii_ci(haystack: &str, needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let hay = haystack.as_bytes();
    if hay.len() < needle.len() {
        return false;
    }
    let limit = hay.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        let mut j = 0;
        while j < needle.len() {
            if !hay[i + j].eq_ignore_ascii_case(&needle[j]) {
                break;
            }
            j += 1;
        }
        if j == needle.len() {
            return true;
        }
        i += 1;
    }
    false
}

#[inline(always)]
#[must_use]
pub fn difficulty_display_name(difficulty_name: &str, zmod_rating_box_text: bool) -> &'static str {
    if difficulty_name.eq_ignore_ascii_case("edit") {
        return "Edit";
    }
    let difficulty_index = FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&name| name.eq_ignore_ascii_case(difficulty_name))
        .unwrap_or(2);
    if zmod_rating_box_text {
        ZMOD_DISPLAY_DIFFICULTY_NAMES[difficulty_index]
    } else {
        DISPLAY_DIFFICULTY_NAMES[difficulty_index]
    }
}

/// Canonical lowercase difficulty tag for SMX pad GIF filenames/roles
/// (`results_25@<tag>@<grade>.gif`): one of `beginner`, `easy`, `medium`,
/// `hard`, `challenge`, `edit`. Matches `difficulty_name` case-insensitively
/// against the raw sm-file difficulty string, independent of any
/// song-specific display remapping (e.g. a `(NOVICE)`-tagged Challenge chart
/// still tags its gif as `challenge`, not `beginner`) so a pack author can
/// target the file's actual difficulty rather than a per-song display quirk.
/// Unrecognized values fall back to `"medium"`, matching `difficulty_display_name`.
#[inline(always)]
#[must_use]
pub fn difficulty_gif_tag(difficulty_name: &str) -> &'static str {
    if difficulty_name.eq_ignore_ascii_case("edit") {
        return "edit";
    }
    const TAGS: [&str; 5] = ["beginner", "easy", "medium", "hard", "challenge"];
    let difficulty_index = FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&name| name.eq_ignore_ascii_case(difficulty_name))
        .unwrap_or(2);
    TAGS[difficulty_index]
}

#[inline(always)]
#[must_use]
pub fn difficulty_display_name_for_song(
    difficulty_name: &str,
    song_main_title: &str,
    zmod_rating_box_text: bool,
) -> &'static str {
    if !zmod_rating_box_text || !difficulty_name.eq_ignore_ascii_case("challenge") {
        return difficulty_display_name(difficulty_name, zmod_rating_box_text);
    }
    if contains_ascii_ci(song_main_title, b"(NOVICE)") {
        return difficulty_display_name("Beginner", true);
    }
    if contains_ascii_ci(song_main_title, b"(EASY)") {
        return difficulty_display_name("Easy", true);
    }
    if contains_ascii_ci(song_main_title, b"(MEDIUM)") {
        return difficulty_display_name("Medium", true);
    }
    if contains_ascii_ci(song_main_title, b"(HARD)") {
        return difficulty_display_name("Hard", true);
    }
    if contains_ascii_ci(song_main_title, b"(EDIT)") {
        return difficulty_display_name("Edit", true);
    }
    difficulty_display_name(difficulty_name, true)
}

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

/// Simply Love SRPG9 event colors mapped to the normal Select Color hue wheel.
/// The source theme uses `SL.SRPG9.Colors` directly when SRPG9 is active, but
/// `DeadSync`'s Select Color screen is keyed to `DECORATIVE_RGBA`.
pub const SRPG9_RGBA: [[f32; 4]; 12] = [
    rgba_hex("#c32020"), // Red
    rgba_hex("#bf0052"), // Pink
    rgba_hex("#9c0082"), // Purple
    rgba_hex("#5131a4"), // Violet
    rgba_hex("#006ecb"), // Blue
    rgba_hex("#009bcf"), // Light Blue
    rgba_hex("#51c0c8"), // Cyan
    rgba_hex("#36855b"), // Green-Blue
    rgba_hex("#3d6526"), // Green
    rgba_hex("#666000"), // Yellow
    rgba_hex("#954f00"), // Orange
    rgba_hex("#954f00"), // Orange
];

pub const SRPG10_RGBA: [[f32; 4]; 12] = [
    rgba_hex("#666000"), // Unaffiliated Yellow
    rgba_hex("#3d6526"), // Green
    rgba_hex("#36855b"), // Green-Blue
    rgba_hex("#36a392"), // DPRT Teal
    rgba_hex("#51c0c8"), // Cyan
    rgba_hex("#009bcf"), // Light Blue
    rgba_hex("#006ecb"), // FE Blue
    rgba_hex("#5131a4"), // Violet
    rgba_hex("#9c0082"), // Purple
    rgba_hex("#bf0052"), // SN Pink
    rgba_hex("#c32020"), // Red
    rgba_hex("#954f00"), // Orange
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

/// The immutable built-in palette, exactly matching the theme's historical
/// colors (including its hand-tuned dim variants).
pub const SIMPLY_LOVE_JUDGMENT_PALETTE: JudgmentPalette = JudgmentPalette::new(
    [
        JUDGMENT_RGBA[0],
        JUDGMENT_FA_PLUS_WHITE_RGBA,
        JUDGMENT_RGBA[1],
        JUDGMENT_RGBA[2],
        JUDGMENT_RGBA[3],
        JUDGMENT_RGBA[4],
        JUDGMENT_RGBA[5],
    ],
    [
        JUDGMENT_DIM_RGBA[0],
        JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA,
        JUDGMENT_DIM_RGBA[1],
        JUDGMENT_DIM_RGBA[2],
        JUDGMENT_DIM_RGBA[3],
        JUDGMENT_DIM_RGBA[4],
        JUDGMENT_DIM_RGBA[5],
    ],
    [
        JUDGMENT_DIM_EVAL_RGBA[0],
        JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA,
        JUDGMENT_DIM_EVAL_RGBA[1],
        JUDGMENT_DIM_EVAL_RGBA[2],
        JUDGMENT_DIM_EVAL_RGBA[3],
        JUDGMENT_DIM_EVAL_RGBA[4],
        JUDGMENT_DIM_EVAL_RGBA[5],
    ],
);

// Arrow Cloud "H.EX" score color.
pub const HARD_EX_SCORE_RGBA: [f32; 4] = rgba_hex("#FF00CC");

pub const EDIT_DIFFICULTY_RGBA: [f32; 4] = rgba_hex("#B4B7BA");

/// Fixed zmod/ITG difficulty palette, ordered Beginner through Challenge.
pub const ITG_DIFFICULTY_RGBA: [[f32; 4]; 5] = [
    rgba_hex("#a355b8"),
    rgba_hex("#1ec51d"),
    rgba_hex("#d6db41"),
    rgba_hex("#ba3049"),
    rgba_hex("#2691c5"),
];

/// Fixed zmod/DDR difficulty palette, ordered Beginner through Challenge.
pub const DDR_DIFFICULTY_RGBA: [[f32; 4]; 5] = [
    rgba_hex("#2dccef"),
    rgba_hex("#eaa910"),
    rgba_hex("#ff344d"),
    rgba_hex("#30d81e"),
    rgba_hex("#e900ff"),
];

/// Returns the Simply Love color for a given difficulty, based on an active theme color index.
#[inline(always)]
#[must_use]
pub fn difficulty_rgba(difficulty_name: &str, active_color_index: i32) -> [f32; 4] {
    difficulty_rgba_with_scheme(
        difficulty_name,
        active_color_index,
        DifficultyColorScheme::SimplyLove,
    )
}

/// Returns the selected zmod difficulty color for a file difficulty name.
#[inline(always)]
#[must_use]
pub fn difficulty_rgba_with_scheme(
    difficulty_name: &str,
    active_color_index: i32,
    scheme: DifficultyColorScheme,
) -> [f32; 4] {
    if difficulty_name.eq_ignore_ascii_case("edit") {
        return EDIT_DIFFICULTY_RGBA;
    }
    let difficulty_index = FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&name| name.eq_ignore_ascii_case(difficulty_name))
        .unwrap_or(2); // Default to Medium if not found

    match scheme {
        DifficultyColorScheme::SimplyLove => {
            let color_index = active_color_index - (4 - difficulty_index) as i32;
            simply_love_rgba(color_index)
        }
        // zmod lightens ITG's fixed palette by 25% for non-decorative uses.
        DifficultyColorScheme::Itg => lighten_rgba(ITG_DIFFICULTY_RGBA[difficulty_index]),
        DifficultyColorScheme::Ddr => DDR_DIFFICULTY_RGBA[difficulty_index],
    }
}

#[inline(always)]
const fn wrap(n: usize, i: i32) -> usize {
    (i.rem_euclid(n as i32)) as usize
}

#[inline(always)]
#[must_use]
pub const fn decorative_rgba(idx: i32) -> [f32; 4] {
    DECORATIVE_RGBA[wrap(DECORATIVE_RGBA.len(), idx)]
}

#[inline(always)]
#[must_use]
pub const fn srpg9_rgba(idx: i32) -> [f32; 4] {
    SRPG9_RGBA[wrap(SRPG9_RGBA.len(), idx)]
}

#[inline(always)]
#[must_use]
pub const fn srpg10_rgba(idx: i32) -> [f32; 4] {
    SRPG10_RGBA[wrap(SRPG10_RGBA.len(), idx)]
}

#[inline(always)]
#[must_use]
pub const fn simply_love_rgba(idx: i32) -> [f32; 4] {
    SIMPLY_LOVE_RGBA[wrap(SIMPLY_LOVE_RGBA.len(), idx)]
}

/// Simply Love `LightenColor(c)` parity: multiplies RGB by 1.25, keeps alpha.
#[inline(always)]
#[must_use]
pub fn lighten_rgba(c: [f32; 4]) -> [f32; 4] {
    [c[0] * 1.25, c[1] * 1.25, c[2] * 1.25, c[3]]
}

/// Menu selected color rule: “current `SIMPLY_LOVE` minus 2”
#[inline(always)]
#[must_use]
pub const fn menu_selected_rgba(active_idx: i32) -> [f32; 4] {
    simply_love_rgba(active_idx - 2)
}

/// Historical palette identity and custom-color dimming used by Simply Love.
pub const JUDGMENT_PRESET: JudgmentPalettePreset = JudgmentPalettePreset {
    id: "simply-love",
    name: "Simply Love",
    palette: SIMPLY_LOVE_JUDGMENT_PALETTE,
    dim_peaks: [0x59, 0x44],
};

/// Decorative colors for paired pads, or the same player color on a lone pad.
#[must_use]
pub const fn underglow_rgba(active_idx: i32, lone_pad: bool) -> [[f32; 4]; 2] {
    let p1 = decorative_rgba(active_idx);
    [
        p1,
        if lone_pad {
            p1
        } else {
            decorative_rgba(active_idx - 2)
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_selection_wraps_menu_and_pad_colors() {
        for index in -12..24 {
            assert_eq!(menu_selected_rgba(index), simply_love_rgba(index + 10));
            assert_eq!(
                underglow_rgba(index, false),
                [decorative_rgba(index), decorative_rgba(index + 10)]
            );
            assert_eq!(underglow_rgba(index, true), [decorative_rgba(index); 2]);
        }
    }

    #[test]
    fn catalog_preserves_historical_palette_and_dim_variants() {
        let catalog = deadsync_config::judgment_palettes::JudgmentPaletteCatalog::from_ini(
            "[General]\nDefaultPalette=simply-love\n",
            JUDGMENT_PRESET,
        );
        let palette = catalog.resolve(None);
        assert_eq!(palette, SIMPLY_LOVE_JUDGMENT_PALETTE);
        // Built-in dim colors are hand-tuned, not reconstructed from their base colors.
        assert_ne!(
            palette,
            JudgmentPalette::from_base_colors(palette.colors, JUDGMENT_PRESET.dim_peaks)
        );
    }

    #[test]
    fn srpg9_order_tracks_decorative_wheel() {
        assert_eq!(srpg9_rgba(0), rgba_hex("#c32020"));
        assert_eq!(srpg9_rgba(7), rgba_hex("#36855b"));
        assert_eq!(srpg9_rgba(8), rgba_hex("#3d6526"));
        assert_eq!(srpg9_rgba(9), rgba_hex("#666000"));
        assert_eq!(srpg9_rgba(11), rgba_hex("#954f00"));
    }

    #[test]
    fn srpg10_order_matches_theme_reference() {
        assert_eq!(srpg10_rgba(0), rgba_hex("#666000"));
        assert_eq!(srpg10_rgba(3), rgba_hex("#36a392"));
        assert_eq!(srpg10_rgba(6), rgba_hex("#006ecb"));
        assert_eq!(srpg10_rgba(9), rgba_hex("#bf0052"));
        assert_eq!(srpg10_rgba(11), rgba_hex("#954f00"));
    }

    #[test]
    fn difficulty_gif_tag_maps_each_file_difficulty_and_edit() {
        assert_eq!(difficulty_gif_tag("Beginner"), "beginner");
        assert_eq!(difficulty_gif_tag("Easy"), "easy");
        assert_eq!(difficulty_gif_tag("Medium"), "medium");
        assert_eq!(difficulty_gif_tag("Hard"), "hard");
        assert_eq!(difficulty_gif_tag("Challenge"), "challenge");
        assert_eq!(difficulty_gif_tag("Edit"), "edit");
        // Case-insensitive.
        assert_eq!(difficulty_gif_tag("hard"), "hard");
        assert_eq!(difficulty_gif_tag("EDIT"), "edit");
        // A Challenge chart tagged for display as "Novice"/"Expert" elsewhere
        // still tags its gif by its real file difficulty, not the display name.
        assert_eq!(difficulty_gif_tag("Challenge"), "challenge");
        // Unrecognized values fall back to "medium".
        assert_eq!(difficulty_gif_tag("Bogus"), "medium");
    }

    #[test]
    fn zmod_difficulty_color_schemes_match_reference_palettes() {
        assert_eq!(
            difficulty_rgba_with_scheme("Beginner", 4, DifficultyColorScheme::Itg),
            lighten_rgba(rgba_hex("#a355b8"))
        );
        assert_eq!(
            difficulty_rgba_with_scheme("Challenge", 0, DifficultyColorScheme::Ddr),
            rgba_hex("#e900ff")
        );
        assert_eq!(
            difficulty_rgba_with_scheme("Edit", 0, DifficultyColorScheme::Ddr),
            EDIT_DIFFICULTY_RGBA
        );
        assert_eq!(
            difficulty_rgba_with_scheme("Unknown", 4, DifficultyColorScheme::Itg),
            lighten_rgba(rgba_hex("#d6db41"))
        );
    }
}
