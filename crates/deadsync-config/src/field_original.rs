// Frozen from c572dc9c05526572068d88ca2f872bad541229d9 for exact primitive and complete loader comparisons.
use super::*;
use std::str::FromStr;
pub fn parse_bool_str(raw: &str) -> Option<bool> {
    let raw = raw.trim();
    if matches!(raw, "1")
        || raw.eq_ignore_ascii_case("true")
        || raw.eq_ignore_ascii_case("yes")
        || raw.eq_ignore_ascii_case("on")
    {
        Some(true)
    } else if matches!(raw, "0")
        || raw.eq_ignore_ascii_case("false")
        || raw.eq_ignore_ascii_case("no")
        || raw.eq_ignore_ascii_case("off")
    {
        Some(false)
    } else {
        None
    }
}
pub fn parse_loose_bool_str(raw: &str) -> Option<bool> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    parse_bool_str(raw).or_else(|| raw.parse::<u8>().ok().map(|n| n != 0))
}
pub fn parse_auto_threads_u8(raw: &str) -> Option<u8> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("auto") || raw.is_empty() {
        Some(0)
    } else {
        raw.parse::<u8>().ok()
    }
}
pub fn load_theme_presentation_options(
    conf: &SimpleIni,
    default: ThemePresentationOptions,
) -> ThemePresentationOptions {
    let visual_style = conf.get("Theme", "VisualStyle");
    let legacy_visual_style = conf.get("Theme", "MenuBackgroundStyle");
    let srpg_variant = conf.get("Theme", "SrpgVariant");
    let legacy_srpg_variant = conf.get("Theme", "ThemeVariant");

    ThemePresentationOptions {
        simply_love_color: conf
            .get("Theme", "SimplyLoveColor")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(default.simply_love_color),
        show_select_music_gameplay_timer: conf
            .get("Theme", "ShowSelectMusicGameplayTimer")
            .and_then(parse_loose_bool_str)
            .unwrap_or(default.show_select_music_gameplay_timer),
        keyboard_features: conf
            .get("Theme", "KeyboardFeatures")
            .and_then(parse_bool_str)
            .unwrap_or(default.keyboard_features),
        visual_style: parse_visual_style(visual_style, legacy_visual_style, default.visual_style),
        srpg_variant: parse_srpg_variant(
            srpg_variant,
            legacy_srpg_variant,
            visual_style.or(legacy_visual_style),
            default.srpg_variant,
        ),
        show_video_backgrounds: conf
            .get("Theme", "VideoBackgrounds")
            .and_then(parse_bool_str)
            .unwrap_or(default.show_video_backgrounds),
        random_background_mode: conf
            .get("Theme", "RandomBackgroundMode")
            .and_then(|value| RandomBackgroundMode::from_str(value).ok())
            .unwrap_or(default.random_background_mode),
        zmod_rating_box_text: conf
            .get("Theme", "ZmodRatingBoxText")
            .and_then(parse_loose_bool_str)
            .unwrap_or(default.zmod_rating_box_text),
        show_bpm_decimal: conf
            .get("Theme", "ShowBpmDecimal")
            .and_then(parse_loose_bool_str)
            .unwrap_or(default.show_bpm_decimal),
        gameplay_bpm_position: conf
            .get("Theme", "GameplayBpmPosition")
            .or_else(|| conf.get("Theme", "BpmPosition"))
            .and_then(|value| GameplayBpmPosition::from_str(value).ok())
            .unwrap_or(default.gameplay_bpm_position),
        gameplay_banner_mode: conf
            .get("Theme", "GameplayBannerMode")
            .or_else(|| conf.get("Theme", "AnimateBanners"))
            .and_then(|value| GameplayBannerMode::from_str(value).ok())
            .unwrap_or(default.gameplay_banner_mode),
        tournament: TournamentModeOptions {
            enabled: conf
                .get("Theme", "EnableTournamentMode")
                .and_then(parse_loose_bool_str)
                .unwrap_or(default.tournament.enabled),
            scoring_system: conf
                .get("Theme", "ScoringSystem")
                .and_then(|value| TournamentScoringSystem::from_str(value).ok())
                .unwrap_or(default.tournament.scoring_system),
            show_step_stats: conf
                .get("Theme", "StepStats")
                .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                    "show" => Some(true),
                    "hide" => Some(false),
                    _ => parse_loose_bool_str(value),
                })
                .unwrap_or(default.tournament.show_step_stats),
            enforce_no_cmod: conf
                .get("Theme", "EnforceNoCmod")
                .and_then(parse_loose_bool_str)
                .unwrap_or(default.tournament.enforce_no_cmod),
        },
    }
}
pub fn step(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "show" => Some(true),
        "hide" => Some(false),
        _ => parse_loose_bool_str(value),
    }
}
