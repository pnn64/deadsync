pub mod i18n;
pub mod scorebox;
pub mod step_stats;
pub mod step_stats_gifs;

use deadsync_config::theme::{MachineFont, SrpgVariant, VisualStyle};

pub struct Assets {
    pub select_color: &'static str,
    pub shared_background: &'static str,
    pub effects: EffectAssets,
    pub shared_background_video: Option<&'static str>,
    pub menu_music: &'static str,
    pub select_color_size: [u32; 2],
    pub shared_background_size: [u32; 2],
}

pub struct EffectAssets {
    pub titlemenu_flycenter: &'static str,
    pub titlemenu_flytop: &'static str,
    pub titlemenu_flybottom: &'static str,
    pub gameplayin_splode: &'static str,
    pub gameplayin_minisplode: &'static str,
    pub combo_100milestone_splode: &'static str,
    pub combo_100milestone_minisplode: &'static str,
    pub combo_1000milestone_swoosh: &'static str,
}

pub struct FontAssetSpec {
    pub name: &'static str,
    pub ini_path: &'static str,
    pub fallback_font_name: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub struct TextureAssetSpec {
    pub key: &'static str,
    pub path: &'static str,
}

pub const FONT_ASSETS: [FontAssetSpec; 20] = [
    FontAssetSpec {
        name: "wendy",
        ini_path: "assets/fonts/wendy/_wendy small.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "miso",
        ini_path: "assets/fonts/miso/_miso light.ini",
        fallback_font_name: Some("game"),
    },
    FontAssetSpec {
        name: "cjk",
        ini_path: "assets/fonts/cjk/_jfonts 16px.ini",
        fallback_font_name: Some("emoji"),
    },
    FontAssetSpec {
        name: "emoji",
        ini_path: "assets/fonts/emoji/_emoji 16px.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "game",
        ini_path: "assets/fonts/game/_game chars 16px.ini",
        fallback_font_name: Some("cjk"),
    },
    FontAssetSpec {
        name: "wendy_monospace_numbers",
        ini_path: "assets/fonts/wendy/_wendy monospace numbers.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "wendy_screenevaluation",
        ini_path: "assets/fonts/wendy/_ScreenEvaluation numbers.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "wendy_combo",
        ini_path: "assets/fonts/_combo/wendy/Wendy.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_arial_rounded",
        ini_path: "assets/fonts/_combo/Arial Rounded/Arial Rounded.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_asap",
        ini_path: "assets/fonts/_combo/Asap/Asap.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_bebas_neue",
        ini_path: "assets/fonts/_combo/Bebas Neue/Bebas Neue.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_source_code",
        ini_path: "assets/fonts/_combo/Source Code/Source Code.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_work",
        ini_path: "assets/fonts/_combo/Work/Work.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_wendy_cursed",
        ini_path: "assets/fonts/_combo/Wendy (Cursed)/Wendy (Cursed).ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "combo_mega",
        ini_path: "assets/fonts/_combo/Mega/Mega.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "wendy_white",
        ini_path: "assets/fonts/wendy/_wendy white.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "mega_alpha",
        ini_path: "assets/fonts/Mega/_mega font.ini",
        fallback_font_name: Some("miso"),
    },
    FontAssetSpec {
        name: "mega_monospace_numbers",
        ini_path: "assets/fonts/Mega/_mega monospace numbers.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "mega_screenevaluation",
        ini_path: "assets/fonts/Mega/_ScreenEvaluation numbers.ini",
        fallback_font_name: None,
    },
    FontAssetSpec {
        name: "mega_game",
        ini_path: "assets/fonts/Mega/_game chars 36px 4x1.ini",
        fallback_font_name: None,
    },
];

pub const BASE_TEXTURE_ASSETS: &[TextureAssetSpec] = &[
    texture_asset("logo.png"),
    texture_asset("init_arrow.png"),
    texture_asset("dance.png"),
    texture_asset("select_mode/arrow-body.png"),
    texture_asset("select_mode/arrow-border.png"),
    texture_asset("select_mode/arrow-stripes.png"),
    texture_asset("select_mode/center-body.png"),
    texture_asset("select_mode/center-border.png"),
    texture_asset("select_mode/center-feet.png"),
    texture_asset("test_input/dance.png"),
    texture_asset("test_input/buttons.png"),
    texture_asset("test_input/highlight.png"),
    texture_asset("test_input/highlightgreen.png"),
    texture_asset("test_input/highlightred.png"),
    texture_asset("test_input/highlightarrow.png"),
    texture_asset("test_lights/bass light (blue).png"),
    texture_asset("test_lights/blue.png"),
    texture_asset("test_lights/cabinet ITG2.png"),
    texture_asset("test_lights/dance.png"),
    texture_asset("test_lights/highlight.png"),
    texture_asset("test_lights/pink.png"),
    texture_asset("test_lights/red.png"),
    texture_asset("test_lights/white.png"),
    texture_asset("meter_arrow.png"),
    texture_asset("name_entry_cursor.png"),
    texture_asset("has_lua.png"),
    texture_asset("has_edit.png"),
    texture_asset("rounded-square.png"),
    texture_asset("circle.png"),
    texture_asset("swoosh.png"),
    TextureAssetSpec {
        key: "graphics/menu_bg_technique/arrow_tex.png",
        path: "menu_bg_technique/arrow_tex.png",
    },
    TextureAssetSpec {
        key: "graphics/menu_bg_technique/square.png",
        path: "menu_bg_technique/square.png",
    },
    TextureAssetSpec {
        key: "graphics/menu_bg_technique/white_tex.png",
        path: "menu_bg_technique/white_tex.png",
    },
    texture_asset("fave-icon.png"),
    texture_asset("lock.png"),
    texture_asset("folder-solid.png"),
    texture_asset("GrooveStats.png"),
    texture_asset("nice.png"),
    texture_asset("BoogieStatsEX.png"),
    texture_asset("arrowcloud.png"),
    texture_asset("ITL.png"),
    texture_asset("crown.png"),
    texture_asset("srpg9_logo_alt.png"),
    texture_asset("srpg10_logo_alt.png"),
    texture_asset(SRPG10_TITLE_LOGO),
    texture_asset("combo_explosion.png"),
    TextureAssetSpec {
        key: "banner1.png",
        path: "_fallback/banner1.png",
    },
    TextureAssetSpec {
        key: "banner2.png",
        path: "_fallback/banner2.png",
    },
    TextureAssetSpec {
        key: "banner3.png",
        path: "_fallback/banner3.png",
    },
    TextureAssetSpec {
        key: "banner4.png",
        path: "_fallback/banner4.png",
    },
    TextureAssetSpec {
        key: "banner5.png",
        path: "_fallback/banner5.png",
    },
    TextureAssetSpec {
        key: "banner6.png",
        path: "_fallback/banner6.png",
    },
    TextureAssetSpec {
        key: "banner7.png",
        path: "_fallback/banner7.png",
    },
    TextureAssetSpec {
        key: "banner8.png",
        path: "_fallback/banner8.png",
    },
    TextureAssetSpec {
        key: "banner9.png",
        path: "_fallback/banner9.png",
    },
    TextureAssetSpec {
        key: "banner10.png",
        path: "_fallback/banner10.png",
    },
    TextureAssetSpec {
        key: "banner11.png",
        path: "_fallback/banner11.png",
    },
    TextureAssetSpec {
        key: "banner12.png",
        path: "_fallback/banner12.png",
    },
    texture_asset("grades/grades 1x19.png"),
    texture_asset("evaluation/failed.png"),
    texture_asset("evaluation/cleared.png"),
    texture_asset("feet-diagram.png"),
    texture_asset("practice/snap_display_icon_9x1 (doubleres).png"),
];

pub const GRADE_TEXTURE_ASSETS: &[TextureAssetSpec] = &[
    texture_asset("grades/star.png"),
    texture_asset("grades/s-plus.png"),
    texture_asset("grades/s.png"),
    texture_asset("grades/s-minus.png"),
    texture_asset("grades/a-plus.png"),
    texture_asset("grades/a.png"),
    texture_asset("grades/a-minus.png"),
    texture_asset("grades/b-plus.png"),
    texture_asset("grades/b.png"),
    texture_asset("grades/b-minus.png"),
    texture_asset("grades/c-plus.png"),
    texture_asset("grades/c.png"),
    texture_asset("grades/c-minus.png"),
    texture_asset("grades/d.png"),
    texture_asset("grades/f.png"),
    texture_asset("grades/q.png"),
    texture_asset("grades/affluent.png"),
    texture_asset("grades/goldstar (stretch).png"),
];

pub const SUBMIT_TEXTURE_ASSETS: &[TextureAssetSpec] = &[
    texture_asset("submit/LoadingSpinner_10x3.png"),
    texture_asset("submit/Hourglass_10x3.png"),
    texture_asset("submit/Check_1x1.png"),
    texture_asset("submit/Refresh_1x1.png"),
    texture_asset("submit/Rejected_1x1.png"),
];

pub const fn texture_asset(path: &'static str) -> TextureAssetSpec {
    TextureAssetSpec { key: path, path }
}

/// Logical font role in the theme, mirroring Simply Love's per-role .redir
/// table.
///
/// Do not use this for gameplay-side text such as notefield combo, judgment
/// label, or hold judgment text. Those follow each player's per-profile font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRole {
    /// Body text. Always Miso, regardless of the chosen MachineFont.
    Normal,
    /// Emphasized UI labels.
    Bold,
    /// Large screen titles.
    Header,
    /// Bottom-of-screen action prompts.
    Footer,
    /// Numeric stats text.
    Numbers,
    /// Evaluation panel numerics.
    ScreenEval,
    /// Big white headline numerals.
    Headline,
}

/// Resolve a logical [`FontRole`] under the given [`MachineFont`] to a
/// registered font key.
///
/// Mirrors the Simply Love `<ThemeFont> <Role>.redir` table:
///
/// | Role         | Wendy (default)             | Mega                     |
/// | ------------ | --------------------------- | ------------------------ |
/// | `Normal`     | `miso`                      | `miso`                   |
/// | `Bold`       | `wendy`                     | `mega_alpha`             |
/// | `Header`     | `wendy`                     | `mega_alpha`             |
/// | `Footer`     | `wendy`                     | `mega_alpha`             |
/// | `Numbers`    | `wendy_monospace_numbers`   | `mega_monospace_numbers` |
/// | `ScreenEval` | `wendy_screenevaluation`    | `mega_screenevaluation`  |
/// | `Headline`   | `wendy_white`               | `mega_alpha`             |
pub fn machine_font_key(machine_font: MachineFont, role: FontRole) -> &'static str {
    use MachineFont::{Mega, Wendy};
    match (machine_font, role) {
        (_, FontRole::Normal) => "miso",
        (Wendy, FontRole::Bold | FontRole::Header | FontRole::Footer) => "wendy",
        (Mega, FontRole::Bold | FontRole::Header | FontRole::Footer) => "mega_alpha",
        (Wendy, FontRole::Numbers) => "wendy_monospace_numbers",
        (Mega, FontRole::Numbers) => "mega_monospace_numbers",
        (Wendy, FontRole::ScreenEval) => "wendy_screenevaluation",
        (Mega, FontRole::ScreenEval) => "mega_screenevaluation",
        (Wendy, FontRole::Headline) => "wendy_white",
        (Mega, FontRole::Headline) => "mega_alpha",
    }
}

fn mega_alpha_supports_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z' | '0'..='9' |
        ' ' | '?' | '!' | '.' | ',' | ';' | ':' | '\'' | '"' |
        '+' | '=' | '-' | '_' | '<' | '>' | '[' | ']' |
        '@' | '#' | '$' | '%' | '^' | '&' | '(' | ')' | '{' | '}' |
        '/' | '\\'
    )
}

#[inline]
fn mega_alpha_supports(text: &str) -> bool {
    text.chars().all(mega_alpha_supports_char)
}

/// Resolve a font role with the Mega whole-string fallback policy.
///
/// For alphabetic roles under [`MachineFont::Mega`], text containing any glyph
/// Mega cannot render falls back to Wendy for the whole actor. This avoids
/// mixed Mega/Miso strings from per-glyph fallback.
pub fn machine_font_key_for_text(
    machine_font: MachineFont,
    role: FontRole,
    text: &str,
) -> &'static str {
    match (machine_font, role) {
        (MachineFont::Mega, FontRole::Bold | FontRole::Header | FontRole::Footer)
            if !mega_alpha_supports(text) =>
        {
            "wendy"
        }
        _ => machine_font_key(machine_font, role),
    }
}

macro_rules! effect_assets {
    (
        $folder:literal,
        $title_suffix:literal,
        $gameplay_suffix:literal,
        $combo100_splode_suffix:literal,
        $combo100_minisplode_suffix:literal
    ) => {
        EffectAssets {
            titlemenu_flycenter: concat!(
                "visual_styles/",
                $folder,
                "/titlemenu_flycenter",
                $title_suffix,
                ".png"
            ),
            titlemenu_flytop: concat!(
                "visual_styles/",
                $folder,
                "/titlemenu_flytop",
                $title_suffix,
                ".png"
            ),
            titlemenu_flybottom: concat!(
                "visual_styles/",
                $folder,
                "/titlemenu_flybottom",
                $title_suffix,
                ".png"
            ),
            gameplayin_splode: concat!(
                "visual_styles/",
                $folder,
                "/gameplayin_splode",
                $gameplay_suffix,
                ".png"
            ),
            gameplayin_minisplode: concat!(
                "visual_styles/",
                $folder,
                "/gameplayin_minisplode",
                $gameplay_suffix,
                ".png"
            ),
            combo_100milestone_splode: concat!(
                "visual_styles/",
                $folder,
                "/combo_100milestone_splode",
                $combo100_splode_suffix,
                ".png"
            ),
            combo_100milestone_minisplode: concat!(
                "visual_styles/",
                $folder,
                "/combo_100milestone_minisplode",
                $combo100_minisplode_suffix,
                ".png"
            ),
            combo_1000milestone_swoosh: concat!(
                "visual_styles/",
                $folder,
                "/combo_1000milestone_swoosh.png"
            ),
        }
    };
}

pub const ASSETS: [Assets; VisualStyle::ALL.len()] = [
    Assets {
        select_color: "visual_styles/hearts/select_color.png",
        shared_background: "visual_styles/hearts/shared_background.png",
        effects: effect_assets!("hearts", "", "", "", ""),
        shared_background_video: None,
        menu_music: "assets/music/in_two (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/arrows/select_color.png",
        shared_background: "visual_styles/arrows/shared_background.png",
        effects: effect_assets!(
            "arrows",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/halcyon farms (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/bears/select_color.png",
        shared_background: "visual_styles/bears/shared_background.png",
        effects: effect_assets!("bears", "", "", " (doubleres)", " (doubleres)"),
        shared_background_video: None,
        menu_music: "assets/music/vrtuous faults (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/ducks/select_color.png",
        shared_background: "visual_styles/ducks/shared_background.png",
        effects: effect_assets!("ducks", "", " (doubleres)", " (doubleres)", " (doubleres)"),
        shared_background_video: None,
        menu_music: "assets/music/Xuxa fami VRC6 (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/cats/select_color.png",
        shared_background: "visual_styles/cats/shared_background.png",
        effects: effect_assets!("cats", "", "", "", " (doubleres)"),
        shared_background_video: None,
        menu_music: "assets/music/Beanmania IIDX (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/spooky/select_color.png",
        shared_background: "visual_styles/spooky/shared_background.png",
        effects: effect_assets!(
            "spooky",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Spooky Scary Chiptunes (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/gay/select_color.png",
        shared_background: "visual_styles/gay/shared_background.png",
        effects: effect_assets!("gay", "", "", "", ""),
        shared_background_video: None,
        menu_music: "assets/music/Mystical Wheelbarrow Journey (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/stars/select_color.png",
        shared_background: "visual_styles/stars/shared_background.png",
        effects: effect_assets!(
            "stars",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Shooting Star - faux VRC6 remix (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/thonk/select_color.png",
        shared_background: "visual_styles/thonk/shared_background.png",
        effects: effect_assets!(
            "thonk",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Da Box of Kardboard Too (feat Naoki vs ZigZag) - TaroNuke Remix (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/technique/select_color.png",
        shared_background: "visual_styles/technique/shared_background.png",
        effects: effect_assets!(
            "technique",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)",
            " (doubleres)"
        ),
        shared_background_video: None,
        menu_music: "assets/music/Quaq (loop).ogg",
        select_color_size: [668, 566],
        shared_background_size: [2048, 2048],
    },
    Assets {
        select_color: "visual_styles/srpg9/select_color.png",
        shared_background: "visual_styles/srpg9/shared_background.png",
        effects: effect_assets!("srpg9", "", "", "", ""),
        shared_background_video: Some("assets/graphics/visual_styles/srpg9/background_video.mp4"),
        menu_music: "assets/music/SRPG9 (loop).ogg",
        select_color_size: [244, 244],
        shared_background_size: [1920, 1080],
    },
];

pub const SRPG10_ASSETS: Assets = Assets {
    select_color: "visual_styles/srpg10/select_color.png",
    shared_background: "visual_styles/srpg10/shared_background.png",
    effects: effect_assets!("srpg10", "", "", "", ""),
    shared_background_video: Some("assets/graphics/visual_styles/srpg10/background_video.mp4"),
    menu_music: "assets/music/SRPG10 (loop).ogg",
    select_color_size: [244, 219],
    shared_background_size: [2581, 1452],
};

pub const SRPG10_TITLE_LOGO: &str = "srpg10_logo_main.png";
pub const SRPG10_EVAL_FAILED_SFX: &str = "assets/sounds/srpg10_eval_failed.ogg";
pub const SRPG10_EVAL_PASSED_SFX: &str = "assets/sounds/srpg10_eval_passed.ogg";
pub const SRPG10_GAMEOVER_MUSIC: &str = "assets/music/SRPG10-GameOver.ogg";
pub const SRPG10_EVAL_PAINT: &str = "visual_styles/srpg10/eval/paint.png";
pub const SRPG10_EVAL_RED_LINES: &str = "visual_styles/srpg10/eval/red_lines.png";
pub const SRPG10_EVAL_EXPEDITION_FAILED: &str = "visual_styles/srpg10/eval/expedition_failed.png";
pub const SRPG10_EVAL_PASS_BG: &str = "visual_styles/srpg10/eval/pass_bg.png";
pub const SRPG10_EVAL_GOLD_LEAF_BG: &str = "visual_styles/srpg10/eval/gold_leaf_background.png";
pub const SRPG10_EVAL_VICTORY: &str = "visual_styles/srpg10/eval/victory.png";

pub const SRPG10_EVAL_TEXTURES: [&str; 6] = [
    SRPG10_EVAL_PAINT,
    SRPG10_EVAL_RED_LINES,
    SRPG10_EVAL_EXPEDITION_FAILED,
    SRPG10_EVAL_PASS_BG,
    SRPG10_EVAL_GOLD_LEAF_BG,
    SRPG10_EVAL_VICTORY,
];

#[inline(always)]
pub fn for_style(style: VisualStyle) -> &'static Assets {
    &ASSETS[style_index(style)]
}

#[inline(always)]
pub fn for_style_and_variant(style: VisualStyle, variant: SrpgVariant) -> &'static Assets {
    if style.is_srpg() && variant == SrpgVariant::Srpg10 {
        &SRPG10_ASSETS
    } else {
        for_style(style)
    }
}

#[inline(always)]
pub const fn srpg10_active(style: VisualStyle, variant: SrpgVariant) -> bool {
    style.is_srpg() && matches!(variant, SrpgVariant::Srpg10)
}

#[inline(always)]
pub fn title_logo_texture_key(style: VisualStyle, variant: SrpgVariant) -> Option<&'static str> {
    srpg10_active(style, variant).then_some(SRPG10_TITLE_LOGO)
}

#[inline(always)]
pub fn select_color_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant).select_color
}

#[inline(always)]
pub fn shared_background_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant).shared_background
}

#[inline(always)]
pub fn titlemenu_flycenter_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .titlemenu_flycenter
}

#[inline(always)]
pub fn titlemenu_flytop_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .titlemenu_flytop
}

#[inline(always)]
pub fn titlemenu_flybottom_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .titlemenu_flybottom
}

#[inline(always)]
pub fn gameplayin_splode_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .gameplayin_splode
}

#[inline(always)]
pub fn gameplayin_minisplode_texture_key(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .gameplayin_minisplode
}

#[inline(always)]
pub fn combo_100milestone_splode_texture_key(
    style: VisualStyle,
    variant: SrpgVariant,
) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .combo_100milestone_splode
}

#[inline(always)]
pub fn combo_100milestone_minisplode_texture_key(
    style: VisualStyle,
    variant: SrpgVariant,
) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .combo_100milestone_minisplode
}

#[inline(always)]
pub fn combo_1000milestone_swoosh_texture_key(
    style: VisualStyle,
    variant: SrpgVariant,
) -> &'static str {
    for_style_and_variant(style, variant)
        .effects
        .combo_1000milestone_swoosh
}

#[inline(always)]
pub fn shared_background_video_asset_path(
    style: VisualStyle,
    variant: SrpgVariant,
) -> Option<&'static str> {
    for_style_and_variant(style, variant).shared_background_video
}

#[inline(always)]
pub fn menu_music_asset_path(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    for_style_and_variant(style, variant).menu_music
}

#[inline(always)]
pub const fn menu_music_folder_name(style: VisualStyle, variant: SrpgVariant) -> &'static str {
    if style.is_srpg() {
        variant.as_str()
    } else {
        style.as_str()
    }
}

pub fn all_assets() -> impl Iterator<Item = &'static Assets> {
    ASSETS.iter().chain(std::iter::once(&SRPG10_ASSETS))
}

pub fn initial_font_assets() -> impl Iterator<Item = &'static FontAssetSpec> {
    FONT_ASSETS.iter()
}

pub fn initial_texture_assets() -> impl Iterator<Item = TextureAssetSpec> {
    BASE_TEXTURE_ASSETS
        .iter()
        .copied()
        .chain(all_assets().flat_map(|asset| {
            [
                texture_asset(asset.select_color),
                texture_asset(asset.shared_background),
                texture_asset(asset.effects.titlemenu_flycenter),
                texture_asset(asset.effects.titlemenu_flytop),
                texture_asset(asset.effects.titlemenu_flybottom),
                texture_asset(asset.effects.gameplayin_splode),
                texture_asset(asset.effects.gameplayin_minisplode),
                texture_asset(asset.effects.combo_100milestone_splode),
                texture_asset(asset.effects.combo_100milestone_minisplode),
                texture_asset(asset.effects.combo_1000milestone_swoosh),
            ]
            .into_iter()
        }))
        .chain(SRPG10_EVAL_TEXTURES.into_iter().map(texture_asset))
        .chain(GRADE_TEXTURE_ASSETS.iter().copied())
        .chain(SUBMIT_TEXTURE_ASSETS.iter().copied())
        .chain(
            step_stats_gifs::STEP_STATS_GIF_TEXTURES
                .into_iter()
                .map(texture_asset),
        )
}

#[inline(always)]
pub fn effect_zoom_scale(texture_key: &str) -> f32 {
    if texture_key.contains("doubleres") {
        0.5
    } else {
        1.0
    }
}

#[inline(always)]
pub fn srpg10_faction_name(color_index: i32) -> &'static str {
    match color_index.rem_euclid(12) {
        0..=2 => "Unaffiliated",
        3..=5 => "Democratic People's Republic of Timing",
        6..=8 => "Footspeed Empire",
        _ => "Stamina Nation",
    }
}

#[inline(always)]
pub fn select_color_aspect(style: VisualStyle, variant: SrpgVariant) -> f32 {
    let size = for_style_and_variant(style, variant).select_color_size;
    size[0] as f32 / size[1] as f32
}

#[inline(always)]
pub fn select_color_zoom_scale(style: VisualStyle, variant: SrpgVariant) -> f32 {
    566.0 / for_style_and_variant(style, variant).select_color_size[1] as f32
}

#[inline(always)]
pub fn is_shared_background_texture(key: &str) -> bool {
    all_assets().any(|asset| asset.shared_background == key)
}

#[inline(always)]
pub fn texture_needs_repeat_sampler(key: &str) -> bool {
    matches!(
        key,
        "swoosh.png" | "graphics/menu_bg_technique/square.png" | "grades/goldstar (stretch).png"
    ) || is_shared_background_texture(key)
}

pub fn bundled_music_asset_paths() -> impl Iterator<Item = &'static str> {
    [
        "assets/music/select_course (loop).ogg",
        "assets/music/credits.ogg",
        SRPG10_GAMEOVER_MUSIC,
    ]
    .into_iter()
    .chain(all_assets().map(|assets| assets.menu_music))
}

const fn style_index(style: VisualStyle) -> usize {
    match style {
        VisualStyle::Hearts => 0,
        VisualStyle::Arrows => 1,
        VisualStyle::Bears => 2,
        VisualStyle::Ducks => 3,
        VisualStyle::Cats => 4,
        VisualStyle::Spooky => 5,
        VisualStyle::Gay => 6,
        VisualStyle::Stars => 7,
        VisualStyle::Thonk => 8,
        VisualStyle::Technique => 9,
        VisualStyle::Srpg9 => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_font_key_normal_is_always_miso() {
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::Normal),
            "miso"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Normal),
            "miso"
        );
    }

    #[test]
    fn machine_font_key_wendy_routes_to_wendy_family() {
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::Bold),
            "wendy"
        );
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::Header),
            "wendy"
        );
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::Footer),
            "wendy"
        );
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::Numbers),
            "wendy_monospace_numbers"
        );
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::ScreenEval),
            "wendy_screenevaluation"
        );
        assert_eq!(
            machine_font_key(MachineFont::Wendy, FontRole::Headline),
            "wendy_white"
        );
    }

    #[test]
    fn machine_font_key_mega_routes_to_mega_family() {
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Bold),
            "mega_alpha"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Header),
            "mega_alpha"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Footer),
            "mega_alpha"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Numbers),
            "mega_monospace_numbers"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::ScreenEval),
            "mega_screenevaluation"
        );
        assert_eq!(
            machine_font_key(MachineFont::Mega, FontRole::Headline),
            "mega_alpha"
        );
    }

    #[test]
    fn machine_font_key_for_text_passes_through_when_wendy() {
        for role in [
            FontRole::Normal,
            FontRole::Bold,
            FontRole::Header,
            FontRole::Footer,
            FontRole::Numbers,
            FontRole::ScreenEval,
        ] {
            assert_eq!(
                machine_font_key_for_text(MachineFont::Wendy, role, "anything"),
                machine_font_key(MachineFont::Wendy, role),
                "role={role:?}"
            );
        }
    }

    #[test]
    fn machine_font_key_for_text_uses_mega_alpha_for_ascii() {
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Header, "Select Music"),
            "mega_alpha"
        );
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Footer, "Press Start"),
            "mega_alpha"
        );
    }

    #[test]
    fn machine_font_key_for_text_falls_back_wholesale_for_unsupported_chars() {
        assert_eq!(
            machine_font_key_for_text(
                MachineFont::Mega,
                FontRole::Header,
                "\u{30ea}\u{30ba}\u{30e0}"
            ),
            "wendy"
        );
        assert_eq!(
            machine_font_key_for_text(
                MachineFont::Mega,
                FontRole::Footer,
                "\u{25d0} \u{2714} \u{2298}"
            ),
            "wendy"
        );
        assert_eq!(
            machine_font_key_for_text(MachineFont::Mega, FontRole::Bold, "Hello\u{2014}World"),
            "wendy"
        );
    }

    #[test]
    fn machine_font_key_for_text_keeps_numeric_roles_on_mega_unconditionally() {
        assert_eq!(
            machine_font_key_for_text(
                MachineFont::Mega,
                FontRole::Numbers,
                "\u{30ea}\u{30ba}\u{30e0}"
            ),
            "mega_monospace_numbers"
        );
        assert_eq!(
            machine_font_key_for_text(
                MachineFont::Mega,
                FontRole::ScreenEval,
                "\u{30ea}\u{30ba}\u{30e0}"
            ),
            "mega_screenevaluation"
        );
    }

    #[test]
    fn visual_styles_have_matching_asset_rows() {
        for style in VisualStyle::ALL {
            let assets = for_style(style);
            assert!(!assets.select_color.is_empty());
            assert!(!assets.shared_background.is_empty());
            assert!(!assets.menu_music.is_empty());
        }
    }

    #[test]
    fn srpg10_variant_uses_srpg10_assets() {
        assert_eq!(
            for_style_and_variant(VisualStyle::Srpg9, SrpgVariant::Srpg10).menu_music,
            "assets/music/SRPG10 (loop).ogg"
        );
        assert_eq!(
            for_style_and_variant(VisualStyle::Hearts, SrpgVariant::Srpg10).menu_music,
            for_style(VisualStyle::Hearts).menu_music
        );
    }

    #[test]
    fn srpg10_selectors_only_enable_for_srpg_style_and_variant() {
        assert!(srpg10_active(VisualStyle::Srpg9, SrpgVariant::Srpg10));
        assert!(!srpg10_active(VisualStyle::Srpg9, SrpgVariant::Srpg9));
        assert!(!srpg10_active(VisualStyle::Hearts, SrpgVariant::Srpg10));

        assert_eq!(
            title_logo_texture_key(VisualStyle::Srpg9, SrpgVariant::Srpg10),
            Some(SRPG10_TITLE_LOGO)
        );
        assert_eq!(
            title_logo_texture_key(VisualStyle::Hearts, SrpgVariant::Srpg10),
            None
        );
    }

    #[test]
    fn asset_key_selectors_follow_variant_assets() {
        assert_eq!(
            select_color_texture_key(VisualStyle::Srpg9, SrpgVariant::Srpg10),
            SRPG10_ASSETS.select_color
        );
        assert_eq!(
            shared_background_texture_key(VisualStyle::Srpg9, SrpgVariant::Srpg10),
            SRPG10_ASSETS.shared_background
        );
        assert_eq!(
            shared_background_video_asset_path(VisualStyle::Srpg9, SrpgVariant::Srpg10),
            SRPG10_ASSETS.shared_background_video
        );
        assert_eq!(
            menu_music_asset_path(VisualStyle::Hearts, SrpgVariant::Srpg10),
            for_style(VisualStyle::Hearts).menu_music
        );
    }

    #[test]
    fn menu_music_folder_name_uses_srpg_variant_folder() {
        assert_eq!(
            menu_music_folder_name(VisualStyle::Srpg9, SrpgVariant::Srpg10),
            "SRPG10"
        );
        assert_eq!(
            menu_music_folder_name(VisualStyle::Srpg9, SrpgVariant::Srpg9),
            "SRPG9"
        );
        assert_eq!(
            menu_music_folder_name(VisualStyle::Hearts, SrpgVariant::Srpg10),
            "Hearts"
        );
    }

    #[test]
    fn bundled_music_assets_are_deduped_by_callers_not_table() {
        let paths: Vec<_> = bundled_music_asset_paths().collect();
        assert!(paths.contains(&SRPG10_GAMEOVER_MUSIC));
        assert!(paths.contains(&"assets/music/in_two (loop).ogg"));
    }

    #[test]
    fn initial_font_assets_include_runtime_fallback_chain() {
        let fallback = |name: &str| {
            initial_font_assets()
                .find(|spec| spec.name == name)
                .and_then(|spec| spec.fallback_font_name)
        };
        assert_eq!(fallback("miso"), Some("game"));
        assert_eq!(fallback("game"), Some("cjk"));
        assert_eq!(fallback("cjk"), Some("emoji"));
        assert_eq!(fallback("mega_alpha"), Some("miso"));
    }

    #[test]
    fn initial_texture_assets_include_base_and_theme_catalogs() {
        let assets: Vec<_> = initial_texture_assets().collect();
        assert!(assets.iter().any(|asset| asset.key == "logo.png"));
        assert!(assets.iter().any(|asset| asset.key == SRPG10_TITLE_LOGO));
        assert!(
            assets
                .iter()
                .any(|asset| asset.key == SRPG10_ASSETS.shared_background)
        );
        assert!(
            assets
                .iter()
                .any(|asset| asset.key == step_stats_gifs::STEP_STATS_GIF_TEXTURES[0])
        );
    }

    #[test]
    fn repeat_sampler_policy_includes_stretched_and_shared_textures() {
        assert!(texture_needs_repeat_sampler(
            "grades/goldstar (stretch).png"
        ));
        assert!(texture_needs_repeat_sampler(ASSETS[0].shared_background));
        assert!(!texture_needs_repeat_sampler("logo.png"));
    }
}
