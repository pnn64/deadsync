//! Translate Simply Love per-profile settings (`Simply Love UserPrefs.ini`,
//! section `[Simply Love]`) into a DeadSync [`PlayerOptionsData`].
//!
//! Simply Love serialises values with Lua's `tostring`, so booleans are written
//! as `true`/`false`, numbers as their decimal form, and strings verbatim. The
//! key names in `permitted_profile_settings` (see
//! `Simply-Love-SM5/Scripts/SL-PlayerProfiles.lua`) overlap heavily with the
//! names DeadSync already uses, but the *value* vocabularies for many enum-style
//! settings differ. To avoid silently corrupting a profile we only translate the
//! settings we can map with high confidence and leave everything else at the
//! DeadSync default:
//!
//! * `SpeedModType` + `SpeedMod` -> [`ScrollSpeedSetting`]
//! * `Mini` -> `mini_percent`, `Spacing` -> `spacing_percent`
//! * `NoteSkin` -> `noteskin`
//! * `JudgmentGraphic` / `HeldGraphic` / `HoldJudgment` -> the matching graphic
//!   (stock graphics only; custom theme graphics fall back to the default) and
//!   `ComboFont` -> `combo_font`
//! * `NoteFieldOffsetX` / `NoteFieldOffsetY` -> note-field offsets
//! * `VisualDelay` -> `visual_delay_ms`
//! * `TiltMultiplier`, `MeasureCounterLookahead`
//! * enum-valued settings whose value vocabulary matches a DeadSync `FromStr`:
//!   `BackgroundFilter`, `ComboColors`, `ComboMode`, `LifeMeterType`,
//!   `MeasureCounter`, `MeasureLines`, `ErrorBarTrim`, `MiniIndicator`,
//!   `StepStatsExtra`, `DataVisualizations` -> `step_statistics`,
//!   `TargetScore` -> `target_score` (Machine/Personal best only)
//! * `PlayerOptionsString` -> turn + scroll (reverse) modifiers
//! * SelectMultiple flag groups: `Colorful`/`Monochrome`/`Text`/`Highlight`/
//!   `Average` -> `error_bar_active_mask`; `Flash*` -> `column_flash_mask`
//! * a set of boolean toggles whose name and meaning match 1:1
//!
//! Everything is pure (no disk / engine state) so it can be unit-tested with a
//! plain map of strings.

use std::collections::HashMap;
use std::str::FromStr;

use deadsync_profile::{
    BackgroundFilter, ColumnFlashMask, ComboColors, ComboFont, ComboMode, ErrorBarMask,
    ErrorBarTrim, HeldMissGraphic, HoldJudgmentGraphic, JudgmentGraphic, LifeMeterType,
    MeasureCounter, MeasureLines, MiniIndicator, NoteSkin, PlayerOptionsData, ScrollOption,
    StepStatisticsMask, StepStatsExtra, TargetScoreSetting, TurnOption, error_bar_style_from_mask,
    error_bar_text_from_mask,
};
use deadsync_rules::scroll::ScrollSpeedSetting;

/// Settings read from a `[Simply Love]` INI section.
pub type SlSettings = HashMap<String, String>;

/// Parse a Simply Love boolean (`true`/`false`, also tolerating `1`/`0`).
fn sl_bool(map: &SlSettings, key: &str) -> Option<bool> {
    let raw = map.get(key)?.trim();
    match raw.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn sl_str<'a>(map: &'a SlSettings, key: &str) -> Option<&'a str> {
    let v = map.get(key)?.trim();
    if v.is_empty() { None } else { Some(v) }
}

fn sl_f32(map: &SlSettings, key: &str) -> Option<f32> {
    sl_str(map, key)?.parse::<f32>().ok()
}

/// Parse a Simply Love value into a DeadSync enum via its [`FromStr`]. Returns
/// `None` (leaving the caller's default in place) when the key is absent or the
/// value isn't one DeadSync recognises — the DeadSync `FromStr` impls normalise
/// case/punctuation and reject unknown vocabularies, so this never guesses.
fn sl_enum<T: FromStr>(map: &SlSettings, key: &str) -> Option<T> {
    sl_str(map, key)?.parse::<T>().ok()
}

/// Parse a signed integer out of a value that may carry trailing units, e.g.
/// Simply Love stores `Mini` as `"50%"`.
fn leading_i32(raw: &str) -> Option<i32> {
    let mut digits = String::new();
    for (i, ch) in raw.trim().chars().enumerate() {
        if ch == '-' && i == 0 {
            digits.push(ch);
        } else if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            break;
        }
    }
    digits.parse::<i32>().ok()
}

/// Build a [`ScrollSpeedSetting`] from Simply Love's `SpeedModType` +
/// `SpeedMod`. SpeedModType is one of `x` / `C` / `M`.
fn scroll_speed_from_sl(map: &SlSettings) -> Option<ScrollSpeedSetting> {
    let kind = sl_str(map, "SpeedModType")?;
    let value = sl_f32(map, "SpeedMod")?;
    if !(value > 0.0) {
        return None;
    }
    match kind.chars().next()?.to_ascii_lowercase() {
        'c' => Some(ScrollSpeedSetting::CMod(value)),
        'm' => Some(ScrollSpeedSetting::MMod(value)),
        'x' => Some(ScrollSpeedSetting::XMod(value)),
        _ => None,
    }
}

/// Normalise one `PlayerOptionsString` token: drop whitespace, lowercase.
fn normalize_token(token: &str) -> String {
    token
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Apply the turn + scroll modifiers encoded in the engine
/// `PlayerOptionsString` (comma separated, e.g. `"1.5x, reverse, mirror"`).
fn apply_player_options_string(options: &mut PlayerOptionsData, raw: &str) {
    let mut scroll = ScrollOption::empty();
    let mut turn: Option<TurnOption> = None;

    for token in raw.split(',') {
        let tok = normalize_token(token);
        if tok.is_empty() {
            continue;
        }
        match tok.as_str() {
            "reverse" => scroll = scroll.union(ScrollOption::Reverse),
            "split" => scroll = scroll.union(ScrollOption::Split),
            "alternate" => scroll = scroll.union(ScrollOption::Alternate),
            "cross" => scroll = scroll.union(ScrollOption::Cross),
            "centered" => scroll = scroll.union(ScrollOption::Centered),
            other => {
                if let Ok(parsed) = other.parse::<TurnOption>() {
                    if parsed != TurnOption::None {
                        turn = Some(parsed);
                    }
                }
            }
        }
    }

    if let Some(t) = turn {
        options.turn_option = t;
    }
    if !scroll.is_normal() {
        options.scroll_option = scroll;
        options.reverse_scroll = scroll.contains(ScrollOption::Reverse);
    }
}

/// Translate the high-confidence Simply Love settings on top of `base`,
/// returning the resulting [`PlayerOptionsData`]. Unknown / ambiguous settings
/// are left exactly as they are in `base`.
pub fn translate_player_options(map: &SlSettings, base: &PlayerOptionsData) -> PlayerOptionsData {
    let mut out = base.clone();

    if let Some(speed) = scroll_speed_from_sl(map) {
        out.scroll_speed = speed;
    }
    if let Some(mini) = sl_str(map, "Mini").and_then(leading_i32) {
        out.mini_percent = mini;
    }
    if let Some(spacing) = sl_str(map, "Spacing").and_then(leading_i32) {
        out.spacing_percent = spacing;
    }
    if let Some(skin) = sl_str(map, "NoteSkin") {
        out.noteskin = NoteSkin::new(skin);
    }
    // Theme graphic / font names. These resolve only to graphics DeadSync ships
    // (via the stock-only parse); a Simply Love profile referencing a custom
    // theme graphic is left at the default rather than pointing at a missing
    // texture. Simply Love stores the full sprite filename for the three
    // graphics and the font directory name for ComboFont.
    if let Some(g) = sl_str(map, "JudgmentGraphic").and_then(JudgmentGraphic::from_stock_name) {
        out.judgment_graphic = g;
    }
    if let Some(g) = sl_str(map, "HeldGraphic").and_then(HeldMissGraphic::from_stock_name) {
        out.held_miss_graphic = g;
    }
    if let Some(g) = sl_str(map, "HoldJudgment").and_then(HoldJudgmentGraphic::from_stock_name) {
        out.hold_judgment_graphic = g;
    }
    if let Some(font) = sl_enum::<ComboFont>(map, "ComboFont") {
        out.combo_font = font;
    }
    if let Some(x) = sl_str(map, "NoteFieldOffsetX").and_then(leading_i32) {
        out.note_field_offset_x = x;
    }
    if let Some(y) = sl_str(map, "NoteFieldOffsetY").and_then(leading_i32) {
        out.note_field_offset_y = y;
    }
    if let Some(delay) = sl_str(map, "VisualDelay").and_then(leading_i32) {
        out.visual_delay_ms = delay;
    }
    if let Some(mult) = sl_f32(map, "TiltMultiplier") {
        out.tilt_multiplier = mult;
    }
    if let Some(look) = sl_str(map, "MeasureCounterLookahead").and_then(leading_i32) {
        out.measure_counter_lookahead = look.clamp(0, i32::from(u8::MAX)) as u8;
    }

    // Enum-valued settings whose Simply Love vocabulary matches DeadSync's
    // `FromStr`. Unknown values are ignored (default preserved).
    if let Some(v) = sl_enum::<BackgroundFilter>(map, "BackgroundFilter") {
        out.background_filter = v;
    }
    if let Some(v) = sl_enum::<ComboColors>(map, "ComboColors") {
        out.combo_colors = v;
    }
    if let Some(v) = sl_enum::<ComboMode>(map, "ComboMode") {
        out.combo_mode = v;
    }
    if let Some(v) = sl_enum::<LifeMeterType>(map, "LifeMeterType") {
        out.lifemeter_type = v;
    }
    if let Some(v) = sl_enum::<MeasureCounter>(map, "MeasureCounter") {
        out.measure_counter = v;
    }
    if let Some(v) = sl_enum::<MeasureLines>(map, "MeasureLines") {
        out.measure_lines = v;
    }
    if let Some(v) = sl_enum::<ErrorBarTrim>(map, "ErrorBarTrim") {
        out.error_bar_trim = v;
    }
    if let Some(v) = sl_enum::<MiniIndicator>(map, "MiniIndicator") {
        out.mini_indicator = v;
    }
    if let Some(v) = sl_enum::<StepStatisticsMask>(map, "DataVisualizations") {
        out.step_statistics = v;
    }
    if let Some(v) = sl_enum::<StepStatsExtra>(map, "StepStatsExtra") {
        out.step_stats_extra = v;
    }
    // Simply Love's `TargetScore` is one of SpecifiedValue / Machine best /
    // Personal best / Ghost Data. Only the latter two have a DeadSync equivalent;
    // the numeric and ghost-data variants are rejected by `FromStr` and ignored.
    if let Some(v) = sl_enum::<TargetScoreSetting>(map, "TargetScore") {
        out.target_score = v;
    }

    apply_bool_toggles(&mut out, map);
    apply_error_bar_flags(&mut out, map);
    apply_column_flash_flags(&mut out, map);

    if let Some(pos) = sl_str(map, "PlayerOptionsString") {
        apply_player_options_string(&mut out, pos);
    }

    out
}

/// Translate Simply Love's `JudgmentFlash` SelectMultiple booleans
/// (`FlashMiss`/`FlashWayOff`/…/`FlashFantastic`) into a [`ColumnFlashMask`].
/// Only applied when at least one flag is present so an absent group keeps the
/// DeadSync default.
fn apply_column_flash_flags(out: &mut PlayerOptionsData, map: &SlSettings) {
    let single_bits = [
        ("FlashMiss", ColumnFlashMask::MISS),
        ("FlashWayOff", ColumnFlashMask::WAY_OFF),
        ("FlashDecent", ColumnFlashMask::DECENT),
        ("FlashGreat", ColumnFlashMask::GREAT),
        ("FlashExcellent", ColumnFlashMask::EXCELLENT),
    ];

    let mut mask = ColumnFlashMask::empty();
    let mut seen = false;
    for (key, bit) in single_bits {
        if let Some(v) = sl_bool(map, key) {
            seen = true;
            if v {
                mask |= bit;
            }
        }
    }
    // Simply Love exposes a single "Fantastic" flash; DeadSync splits fantastic
    // into blue (W0/FA+) and white (W1) columns, so enable both.
    if let Some(v) = sl_bool(map, "FlashFantastic") {
        seen = true;
        if v {
            mask |= ColumnFlashMask::BLUE_FANTASTIC | ColumnFlashMask::WHITE_FANTASTIC;
        }
    }

    if seen {
        out.column_flash_mask = mask;
    }
}

/// Translate Simply Love's error-bar style SelectMultiple booleans
/// (`Colorful`/`Monochrome`/`Text`/`Highlight`/`Average`) into the
/// [`ErrorBarMask`], deriving the legacy `error_bar` / `error_bar_text` fields
/// from the resulting mask (mirroring the DeadSync profile loader). Only applied
/// when at least one flag is present.
fn apply_error_bar_flags(out: &mut PlayerOptionsData, map: &SlSettings) {
    let flags = [
        ("Colorful", ErrorBarMask::COLORFUL),
        ("Monochrome", ErrorBarMask::MONOCHROME),
        ("Text", ErrorBarMask::TEXT),
        ("Highlight", ErrorBarMask::HIGHLIGHT),
        ("Average", ErrorBarMask::AVERAGE),
    ];

    let mut mask = ErrorBarMask::empty();
    let mut seen = false;
    for (key, bit) in flags {
        if let Some(v) = sl_bool(map, key) {
            seen = true;
            if v {
                mask |= bit;
            }
        }
    }

    if seen {
        out.error_bar_active_mask = mask;
        out.error_bar = error_bar_style_from_mask(mask);
        out.error_bar_text = error_bar_text_from_mask(mask);
    }
}

/// Boolean toggles whose Simply Love key and meaning match a DeadSync field 1:1.
fn apply_bool_toggles(out: &mut PlayerOptionsData, map: &SlSettings) {
    macro_rules! set_bool {
        ($key:literal => $field:ident) => {
            if let Some(v) = sl_bool(map, $key) {
                out.$field = v;
            }
        };
    }

    set_bool!("HideTargets" => hide_targets);
    set_bool!("HideSongBG" => hide_song_bg);
    set_bool!("HideCombo" => hide_combo);
    set_bool!("HideLifebar" => hide_lifebar);
    set_bool!("HideScore" => hide_score);
    set_bool!("HideDanger" => hide_danger);
    set_bool!("HideComboExplosions" => hide_combo_explosions);
    set_bool!("MeasureCounterLeft" => measure_counter_left);
    set_bool!("MeasureCounterUp" => measure_counter_up);
    set_bool!("MeasureCounterVert" => measure_counter_vert);
    set_bool!("BrokenRun" => broken_run);
    set_bool!("RunTimer" => run_timer);
    set_bool!("RainbowMax" => rainbow_max);
    set_bool!("ResponsiveColors" => responsive_colors);
    set_bool!("ShowLifePercent" => show_life_percent);
    set_bool!("ColumnFlashOnMiss" => column_flash_on_miss);
    set_bool!("SubtractiveScoring" => subtractive_scoring);
    set_bool!("Pacemaker" => pacemaker);
    set_bool!("TrackEarlyJudgments" => track_early_judgments);
    set_bool!("ScaleGraph" => scale_scatterplot);
    set_bool!("NPSGraphAtTop" => nps_graph_at_top);
    set_bool!("JudgmentTilt" => judgment_tilt);
    set_bool!("ColumnCues" => column_cues);
    set_bool!("ColumnCountdown" => column_countdown);
    set_bool!("ErrorBarUp" => error_bar_up);
    set_bool!("ErrorBarMultiTick" => error_bar_multi_tick);
    set_bool!("ShowFaPlusWindow" => show_fa_plus_window);
    set_bool!("ShowExScore" => show_ex_score);
    set_bool!("ShowFaPlusPane" => show_fa_plus_pane);
    set_bool!("SmallerWhite" => fa_plus_10ms_blue_window);
    set_bool!("SplitWhites" => split_15_10ms);
    set_bool!("HideEarlyDecentWayOffJudgments" => hide_early_dw_judgments);
    set_bool!("HideEarlyDecentWayOffFlash" => hide_early_dw_flash);
    set_bool!("DisplayScorebox" => display_scorebox);
    set_bool!("JudgmentBack" => judgment_back);
    set_bool!("ErrorMSDisplay" => error_ms_display);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sl(pairs: &[(&str, &str)]) -> SlSettings {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn translates_speed_mod_variants() {
        let base = PlayerOptionsData::default();

        let c = translate_player_options(&sl(&[("SpeedModType", "C"), ("SpeedMod", "400")]), &base);
        assert_eq!(c.scroll_speed, ScrollSpeedSetting::CMod(400.0));

        let m = translate_player_options(&sl(&[("SpeedModType", "M"), ("SpeedMod", "550")]), &base);
        assert_eq!(m.scroll_speed, ScrollSpeedSetting::MMod(550.0));

        let x = translate_player_options(&sl(&[("SpeedModType", "x"), ("SpeedMod", "1.5")]), &base);
        assert_eq!(x.scroll_speed, ScrollSpeedSetting::XMod(1.5));
    }

    #[test]
    fn ignores_invalid_speed() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(&sl(&[("SpeedModType", "C"), ("SpeedMod", "0")]), &base);
        assert_eq!(out.scroll_speed, base.scroll_speed);

        let out2 =
            translate_player_options(&sl(&[("SpeedModType", "Q"), ("SpeedMod", "2")]), &base);
        assert_eq!(out2.scroll_speed, base.scroll_speed);
    }

    #[test]
    fn parses_mini_percentage() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(&sl(&[("Mini", "50%")]), &base);
        assert_eq!(out.mini_percent, 50);

        let neg = translate_player_options(&sl(&[("Mini", "-25%")]), &base);
        assert_eq!(neg.mini_percent, -25);
    }

    #[test]
    fn applies_noteskin_and_offsets() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[
                ("NoteSkin", "Metal"),
                ("NoteFieldOffsetX", "12"),
                ("NoteFieldOffsetY", "-8"),
            ]),
            &base,
        );
        assert_eq!(out.noteskin.as_str(), "metal");
        assert_eq!(out.note_field_offset_x, 12);
        assert_eq!(out.note_field_offset_y, -8);
    }

    #[test]
    fn parses_booleans_true_false() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[
                ("ShowExScore", "true"),
                ("ShowFaPlusWindow", "false"),
                ("HideCombo", "true"),
                ("Pacemaker", "1"),
                ("RainbowMax", "0"),
            ]),
            &base,
        );
        assert!(out.show_ex_score);
        assert!(!out.show_fa_plus_window);
        assert!(out.hide_combo);
        assert!(out.pacemaker);
        assert!(!out.rainbow_max);
    }

    #[test]
    fn parses_turn_and_reverse_from_options_string() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[("PlayerOptionsString", "1.5x, Reverse, Mirror, Overhead")]),
            &base,
        );
        assert_eq!(out.turn_option, TurnOption::Mirror);
        assert!(out.reverse_scroll);
        assert!(out.scroll_option.contains(ScrollOption::Reverse));
    }

    #[test]
    fn parses_spacing_and_visual_delay() {
        let base = PlayerOptionsData::default();
        let out =
            translate_player_options(&sl(&[("Spacing", "-25%"), ("VisualDelay", "12ms")]), &base);
        assert_eq!(out.spacing_percent, -25);
        assert_eq!(out.visual_delay_ms, 12);
    }

    #[test]
    fn translates_stock_graphics_and_font() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[
                ("JudgmentGraphic", "Wendy 2x7 (doubleres).png"),
                ("HoldJudgment", "ITG2 1x2 (doubleres).png"),
                ("HeldGraphic", "None"),
                ("ComboFont", "Bebas Neue"),
            ]),
            &base,
        );
        assert_eq!(
            out.judgment_graphic.as_str(),
            "judgements/Wendy 2x7 (doubleres).png"
        );
        assert_eq!(
            out.hold_judgment_graphic.as_str(),
            "hold_judgements/ITG2 1x2 (doubleres).png"
        );
        assert!(out.held_miss_graphic.is_none());
        assert_eq!(out.combo_font, ComboFont::BebasNeue);
    }

    #[test]
    fn ignores_unknown_custom_graphics() {
        let mut base = PlayerOptionsData::default();
        base.judgment_graphic = JudgmentGraphic::new("Wendy");
        let out = translate_player_options(
            &sl(&[
                ("JudgmentGraphic", "MyCustomTheme 2x7 (doubleres).png"),
                ("ComboFont", "SomeCustomFont"),
            ]),
            &base,
        );
        // Unknown graphic/font must not fabricate a path — keep the base value.
        assert_eq!(out.judgment_graphic, base.judgment_graphic);
        assert_eq!(out.combo_font, base.combo_font);
    }

    #[test]
    fn translates_enum_settings() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[
                ("BackgroundFilter", "50"),
                ("ComboColors", "RainbowScroll"),
                ("ComboMode", "CurrentCombo"),
                ("LifeMeterType", "Surround"),
                ("MeasureCounter", "16th"),
                ("MeasureLines", "Quarter"),
                ("ErrorBarTrim", "Great"),
                ("MiniIndicator", "Pacemaker"),
                ("ScaleGraph", "true"),
            ]),
            &base,
        );
        assert_eq!(
            out.background_filter,
            BackgroundFilter::from_str("50").unwrap()
        );
        assert_eq!(out.combo_colors, ComboColors::RainbowScroll);
        assert_eq!(out.combo_mode, ComboMode::CurrentCombo);
        assert_eq!(out.lifemeter_type, LifeMeterType::Surround);
        assert_eq!(out.measure_counter, MeasureCounter::Sixteenth);
        assert_eq!(out.measure_lines, MeasureLines::Quarter);
        assert_eq!(out.error_bar_trim, ErrorBarTrim::Great);
        assert_eq!(out.mini_indicator, MiniIndicator::Pacemaker);
        assert!(out.scale_scatterplot);
    }

    #[test]
    fn ignores_unknown_enum_values() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[("ComboColors", "NotARealValue"), ("MeasureCounter", "99th")]),
            &base,
        );
        assert_eq!(out.combo_colors, base.combo_colors);
        assert_eq!(out.measure_counter, base.measure_counter);
    }

    #[test]
    fn translates_step_stats_extra_and_target_score() {
        let base = PlayerOptionsData::default();

        let out = translate_player_options(
            &sl(&[
                ("StepStatsExtra", "ErrorStats"),
                ("TargetScore", "Machine best"),
            ]),
            &base,
        );
        assert_eq!(out.step_stats_extra, StepStatsExtra::ErrorStats);
        assert_eq!(out.target_score, TargetScoreSetting::MachineBest);

        let pb = translate_player_options(&sl(&[("TargetScore", "Personal best")]), &base);
        assert_eq!(pb.target_score, TargetScoreSetting::PersonalBest);

        // SL's numeric / ghost-data targets have no DeadSync equivalent → default.
        let specified = translate_player_options(
            &sl(&[
                ("TargetScore", "SpecifiedValue"),
                ("TargetScoreNumber", "95"),
            ]),
            &base,
        );
        assert_eq!(specified.target_score, base.target_score);
        let ghost = translate_player_options(&sl(&[("TargetScore", "Ghost Data")]), &base);
        assert_eq!(ghost.target_score, base.target_score);
    }

    #[test]
    fn translates_data_visualizations_legacy_values() {
        let base = PlayerOptionsData::default();

        let stats =
            translate_player_options(&sl(&[("DataVisualizations", "Step Statistics")]), &base);
        assert_eq!(stats.step_statistics, StepStatisticsMask::all_widgets());

        let none =
            translate_player_options(&sl(&[("DataVisualizations", "Target Score Graph")]), &base);
        assert_eq!(none.step_statistics, StepStatisticsMask::empty());
    }

    #[test]
    fn translates_error_bar_flags() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[
                ("Colorful", "true"),
                ("Monochrome", "false"),
                ("Text", "true"),
                ("Highlight", "false"),
                ("Average", "false"),
            ]),
            &base,
        );
        assert!(
            out.error_bar_active_mask
                .contains(ErrorBarMask::COLORFUL | ErrorBarMask::TEXT)
        );
        assert!(!out.error_bar_active_mask.contains(ErrorBarMask::MONOCHROME));
        assert_eq!(
            out.error_bar,
            error_bar_style_from_mask(out.error_bar_active_mask)
        );
        assert!(out.error_bar_text);
    }

    #[test]
    fn translates_column_flash_flags_splitting_fantastic() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(
            &sl(&[
                ("FlashMiss", "true"),
                ("FlashWayOff", "false"),
                ("FlashDecent", "false"),
                ("FlashGreat", "false"),
                ("FlashExcellent", "true"),
                ("FlashFantastic", "true"),
            ]),
            &base,
        );
        assert!(out.column_flash_mask.contains(ColumnFlashMask::MISS));
        assert!(out.column_flash_mask.contains(ColumnFlashMask::EXCELLENT));
        assert!(
            out.column_flash_mask
                .contains(ColumnFlashMask::BLUE_FANTASTIC | ColumnFlashMask::WHITE_FANTASTIC)
        );
        assert!(!out.column_flash_mask.contains(ColumnFlashMask::WAY_OFF));
    }

    #[test]
    fn flag_groups_absent_keep_default() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(&sl(&[("SpeedMod", "300")]), &base);
        assert_eq!(out.column_flash_mask, base.column_flash_mask);
        assert_eq!(out.error_bar_active_mask, base.error_bar_active_mask);
    }

    #[test]
    fn leaves_unspecified_options_at_base() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(&sl(&[("SpeedMod", "300")]), &base);
        // No SpeedModType -> scroll speed unchanged, everything else default.
        assert_eq!(out.scroll_speed, base.scroll_speed);
        assert_eq!(out.turn_option, base.turn_option);
        assert_eq!(out.mini_percent, base.mini_percent);
    }
}
