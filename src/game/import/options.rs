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
//! * `Mini` -> `mini_percent`
//! * `NoteSkin` -> `noteskin`
//! * `NoteFieldOffsetX` / `NoteFieldOffsetY` -> note-field offsets
//! * `TiltMultiplier`, `MeasureCounterLookahead`
//! * `PlayerOptionsString` -> turn + scroll (reverse) modifiers
//! * a set of boolean toggles whose name and meaning match 1:1
//!
//! Everything is pure (no disk / engine state) so it can be unit-tested with a
//! plain map of strings.

use std::collections::HashMap;

use deadsync_profile::{NoteSkin, PlayerOptionsData, ScrollOption, TurnOption};
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
    if let Some(skin) = sl_str(map, "NoteSkin") {
        out.noteskin = NoteSkin::new(skin);
    }
    if let Some(x) = sl_str(map, "NoteFieldOffsetX").and_then(leading_i32) {
        out.note_field_offset_x = x;
    }
    if let Some(y) = sl_str(map, "NoteFieldOffsetY").and_then(leading_i32) {
        out.note_field_offset_y = y;
    }
    if let Some(mult) = sl_f32(map, "TiltMultiplier") {
        out.tilt_multiplier = mult;
    }
    if let Some(look) = sl_str(map, "MeasureCounterLookahead").and_then(leading_i32) {
        out.measure_counter_lookahead = look.clamp(0, i32::from(u8::MAX)) as u8;
    }

    apply_bool_toggles(&mut out, map);

    if let Some(pos) = sl_str(map, "PlayerOptionsString") {
        apply_player_options_string(&mut out, pos);
    }

    out
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
    fn leaves_unspecified_options_at_base() {
        let base = PlayerOptionsData::default();
        let out = translate_player_options(&sl(&[("SpeedMod", "300")]), &base);
        // No SpeedModType -> scroll speed unchanged, everything else default.
        assert_eq!(out.scroll_speed, base.scroll_speed);
        assert_eq!(out.turn_option, base.turn_option);
        assert_eq!(out.mini_percent, base.mini_percent);
    }
}
