use deadsync_rules::scroll::ScrollSpeedSetting;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayModsAttackMode {
    On,
    Off,
    Random,
    RandomVanish,
}

pub const DISPLAY_TURN_MIRROR: u16 = 1 << 0;
pub const DISPLAY_TURN_LEFT: u16 = 1 << 1;
pub const DISPLAY_TURN_RIGHT: u16 = 1 << 2;
pub const DISPLAY_TURN_LR_MIRROR: u16 = 1 << 3;
pub const DISPLAY_TURN_UD_MIRROR: u16 = 1 << 4;
pub const DISPLAY_TURN_SHUFFLE: u16 = 1 << 5;
pub const DISPLAY_TURN_BLENDER: u16 = 1 << 6;
pub const DISPLAY_TURN_RANDOM: u16 = 1 << 7;

#[derive(Clone, Copy, Debug)]
pub struct GameplayModsTextParams<'a> {
    pub speed: ScrollSpeedSetting,
    pub noteskin: &'a str,
    pub insert_mask: u8,
    pub remove_mask: u8,
    pub holds_mask: u8,
    pub turn_bits: u16,
    pub attack_mode: GameplayModsAttackMode,
    pub mini_percent: i16,
    pub spacing_percent: i16,
    pub visual_delay_ms: i16,
    pub average_error_bar_active: bool,
    pub avg_error_bar_intensity_centi: i16,
    pub avg_error_bar_interval_ms: u16,
    pub accel: [i16; 5],
    pub visual: [i16; 9],
    pub appearance: [i16; 5],
    pub scroll: [i16; 5],
    pub perspective_tilt: i16,
    pub perspective_skew: i16,
    pub dark: i16,
    pub blind: i16,
    pub cover: i16,
    pub disabled_timing_windows: u8,
}

fn begin_display_mod_option(out: &mut String) {
    if !out.is_empty() {
        out.push_str(", ");
    }
}

fn push_atomic_text(out: &mut String, text: &str) {
    let mut words = text.split(' ');
    if let Some(first) = words.next() {
        out.push_str(first);
    }
    for word in words {
        out.push('\u{00A0}');
        out.push_str(word);
    }
}

fn push_display_mod_option(out: &mut String, option: &str) {
    begin_display_mod_option(out);
    push_atomic_text(out, option);
}

fn push_trimmed_float2(out: &mut String, value: f32) {
    let start = out.len();
    write!(out, "{value:.2}").expect("writing to a String cannot fail");
    if out[start..].contains('.') {
        while out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
    }
}

fn append_speed(out: &mut String, speed: ScrollSpeedSetting) {
    begin_display_mod_option(out);
    match speed {
        ScrollSpeedSetting::XMod(value) => {
            push_trimmed_float2(out, value);
            out.push('x');
        }
        ScrollSpeedSetting::CMod(value) => {
            out.push('C');
            push_trimmed_float2(out, value);
        }
        ScrollSpeedSetting::MMod(value) => {
            out.push('m');
            push_trimmed_float2(out, value);
        }
    }
}

fn append_mod_part(out: &mut String, percent: i16, name: &str) {
    if percent == 0 {
        return;
    }
    begin_display_mod_option(out);
    if percent != 100 {
        write!(out, "{percent}%\u{00A0}").expect("writing to a String cannot fail");
    }
    push_atomic_text(out, name);
}

pub(crate) fn append_mini_part(out: &mut String, mini_percent: i16) {
    if mini_percent != 0 {
        begin_display_mod_option(out);
        write!(out, "{mini_percent}%\u{00A0}Mini").expect("writing to a String cannot fail");
    }
}

fn append_spacing_part(out: &mut String, spacing_percent: i16) {
    if spacing_percent != 0 {
        begin_display_mod_option(out);
        write!(out, "{spacing_percent}%\u{00A0}Spacing").expect("writing to a String cannot fail");
    }
}

pub(crate) fn append_average_error_bar_part(out: &mut String, params: GameplayModsTextParams<'_>) {
    if params.average_error_bar_active {
        begin_display_mod_option(out);
        out.push_str("ErrorBar");
        push_trimmed_float2(out, params.avg_error_bar_intensity_centi as f32 / 100.0);
        write!(out, "x(Avg:{}ms)", params.avg_error_bar_interval_ms)
            .expect("writing to a String cannot fail");
    }
}

pub(crate) fn append_perspective_parts(out: &mut String, tilt: i16, skew: i16) {
    if tilt == 0 && skew == 0 {
        push_display_mod_option(out, "Overhead");
        return;
    }
    if skew == 0 {
        if tilt > 0 {
            append_mod_part(out, tilt, "Distant");
        } else {
            append_mod_part(out, -tilt, "Hallway");
        }
        return;
    }
    if skew == tilt {
        append_mod_part(out, skew, "Space");
        return;
    }
    if skew == -tilt {
        append_mod_part(out, skew, "Incoming");
        return;
    }
    append_mod_part(out, skew, "Skew");
    append_mod_part(out, tilt, "Tilt");
}

pub(crate) fn append_turn_parts(out: &mut String, bits: u16) {
    for (bit, name) in [
        (DISPLAY_TURN_MIRROR, "Mirror"),
        (DISPLAY_TURN_LEFT, "Left"),
        (DISPLAY_TURN_RIGHT, "Right"),
        (DISPLAY_TURN_LR_MIRROR, "LR-Mirror"),
        (DISPLAY_TURN_UD_MIRROR, "UD-Mirror"),
        (DISPLAY_TURN_SHUFFLE, "Shuffle"),
        (DISPLAY_TURN_BLENDER, "Blender"),
        (DISPLAY_TURN_RANDOM, "Random"),
    ] {
        if bits & bit != 0 {
            push_display_mod_option(out, name);
        }
    }
}

fn attack_mode_name(mode: GameplayModsAttackMode) -> Option<&'static str> {
    match mode {
        GameplayModsAttackMode::On => None,
        GameplayModsAttackMode::Off => Some("NoAttacks"),
        GameplayModsAttackMode::Random => Some("RandomAttacks"),
        GameplayModsAttackMode::RandomVanish => Some("RandomVanish"),
    }
}

pub(crate) fn push_transform_parts(
    out: &mut String,
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
) {
    for (enabled, name) in [
        (remove_mask & (1 << 2) != 0, "NoHolds"),
        (holds_mask & (1 << 3) != 0, "NoRolls"),
        (remove_mask & (1 << 1) != 0, "NoMines"),
        (remove_mask & (1 << 0) != 0, "Little"),
        (insert_mask & (1 << 0) != 0, "Wide"),
        (insert_mask & (1 << 1) != 0, "Big"),
        (insert_mask & (1 << 2) != 0, "Quick"),
        (insert_mask & (1 << 3) != 0, "BMRize"),
        (insert_mask & (1 << 4) != 0, "Skippy"),
        (insert_mask & (1 << 7) != 0, "Mines"),
        (insert_mask & (1 << 5) != 0, "Echo"),
        (insert_mask & (1 << 6) != 0, "Stomp"),
        (holds_mask & (1 << 0) != 0, "Planted"),
        (holds_mask & (1 << 1) != 0, "Floored"),
        (holds_mask & (1 << 2) != 0, "Twister"),
        (holds_mask & (1 << 4) != 0, "HoldsToRolls"),
        (remove_mask & (1 << 3) != 0, "NoJumps"),
        (remove_mask & (1 << 4) != 0, "NoHands"),
        (remove_mask & (1 << 6) != 0, "NoLifts"),
        (remove_mask & (1 << 7) != 0, "NoFakes"),
        (remove_mask & (1 << 5) != 0, "NoQuads"),
    ] {
        if enabled {
            push_display_mod_option(out, name);
        }
    }
}

pub(crate) fn append_disabled_timing_windows(out: &mut String, bits: u8) {
    if bits == 0 {
        return;
    }
    begin_display_mod_option(out);
    out.push_str("No\u{00A0}");
    let mut first = true;
    for ix in 0_u8..5 {
        if bits & (1_u8 << ix) == 0 {
            continue;
        }
        if !first {
            out.push('/');
        }
        out.push('W');
        out.push(char::from(b'1' + ix));
        first = false;
    }
}

pub fn gameplay_mods_text(params: GameplayModsTextParams<'_>) -> String {
    let mut out = String::with_capacity(64);
    append_speed(&mut out, params.speed);
    for (percent, name) in
        params
            .accel
            .into_iter()
            .zip(["Boost", "Brake", "Wave", "Expand", "Boomerang"])
    {
        append_mod_part(&mut out, percent, name);
    }
    for (percent, name) in params.visual.into_iter().zip([
        "Drunk",
        "Dizzy",
        "Confusion",
        "Flip",
        "Invert",
        "Tornado",
        "Tipsy",
        "Bumpy",
        "Beat",
    ]) {
        append_mod_part(&mut out, percent, name);
    }
    append_mini_part(&mut out, params.mini_percent);
    append_spacing_part(&mut out, params.spacing_percent);
    for (percent, name) in
        params
            .appearance
            .into_iter()
            .zip(["Hidden", "Sudden", "Stealth", "Blink", "RandomVanish"])
    {
        append_mod_part(&mut out, percent, name);
    }
    for (percent, name) in
        params
            .scroll
            .into_iter()
            .zip(["Reverse", "Split", "Alternate", "Cross", "Centered"])
    {
        append_mod_part(&mut out, percent, name);
    }
    append_mod_part(&mut out, params.dark, "Dark");
    append_mod_part(&mut out, params.blind, "Blind");
    append_mod_part(&mut out, params.cover, "Hide BG");
    if let Some(name) = attack_mode_name(params.attack_mode) {
        push_display_mod_option(&mut out, name);
    }
    append_turn_parts(&mut out, params.turn_bits);
    push_transform_parts(
        &mut out,
        params.insert_mask,
        params.remove_mask,
        params.holds_mask,
    );
    append_perspective_parts(&mut out, params.perspective_tilt, params.perspective_skew);
    if !params.noteskin.is_empty() {
        push_display_mod_option(&mut out, params.noteskin);
    }
    if params.visual_delay_ms != 0 {
        begin_display_mod_option(&mut out);
        write!(out, "{}ms\u{00A0}VisualDelay", params.visual_delay_ms)
            .expect("writing to a String cannot fail");
    }
    append_average_error_bar_part(&mut out, params);
    append_disabled_timing_windows(&mut out, params.disabled_timing_windows);
    out
}
