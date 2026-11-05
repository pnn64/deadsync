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
    pub appearance: [i16; 6],
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
    let integer = value as i16;
    if value == f32::from(integer) {
        if integer == 0 && value.is_sign_negative() {
            out.push('-');
        }
        push_i16_decimal(out, integer);
        out.push_str(".00");
        out.truncate(out.len() - 3);
        return;
    }

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

fn push_i16_decimal(out: &mut String, value: i16) {
    const PAIRS: &[u8; 200] = b"00010203040506070809101112131415161718192021222324252627282930313233343536373839404142434445464748495051525354555657585960616263646566676869707172737475767778798081828384858687888990919293949596979899";
    let mut magnitude = value.unsigned_abs();
    let mut digits = [0u8; 5];
    let mut start = digits.len();
    while magnitude >= 100 {
        let pair = usize::from(magnitude % 100) * 2;
        magnitude /= 100;
        start -= 2;
        digits[start..start + 2].copy_from_slice(&PAIRS[pair..pair + 2]);
    }
    if magnitude >= 10 {
        start -= 2;
        let pair = usize::from(magnitude) * 2;
        digits[start..start + 2].copy_from_slice(&PAIRS[pair..pair + 2]);
    } else {
        start -= 1;
        digits[start] = b'0' + magnitude as u8;
    }
    if value < 0 {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&digits[start..]).expect("decimal digits are ASCII"));
}

fn append_mod_part(out: &mut String, percent: i16, name: &str) {
    if percent == 0 {
        return;
    }
    begin_display_mod_option(out);
    if percent != 100 {
        push_i16_decimal(out, percent);
        out.push_str("%\u{00A0}");
    }
    push_atomic_text(out, name);
}

pub(crate) fn append_mini_part(out: &mut String, mini_percent: i16) {
    if mini_percent != 0 {
        begin_display_mod_option(out);
        push_i16_decimal(out, mini_percent);
        out.push_str("%\u{00A0}Mini");
    }
}

fn append_spacing_part(out: &mut String, spacing_percent: i16) {
    if spacing_percent != 0 {
        begin_display_mod_option(out);
        push_i16_decimal(out, spacing_percent);
        out.push_str("%\u{00A0}Spacing");
    }
}

pub(crate) fn append_average_error_bar_part(out: &mut String, params: GameplayModsTextParams<'_>) {
    if params.average_error_bar_active {
        begin_display_mod_option(out);
        out.push_str("ErrorBar");
        push_trimmed_float2(out, f32::from(params.avg_error_bar_intensity_centi) / 100.0);
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
    const NAMES: [&str; 8] = [
        "Mirror",
        "Left",
        "Right",
        "LR-Mirror",
        "UD-Mirror",
        "Shuffle",
        "Blender",
        "Random",
    ];
    let mut remaining = bits & 0xff;
    while remaining != 0 {
        begin_display_mod_option(out);
        out.push_str(NAMES[remaining.trailing_zeros() as usize]);
        remaining &= remaining - 1;
    }
}

const fn attack_mode_name(mode: GameplayModsAttackMode) -> Option<&'static str> {
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
    if insert_mask | remove_mask | (holds_mask & 0x1f) == 0 {
        return;
    }
    const NAMES: [&str; 21] = [
        "NoHolds",
        "NoRolls",
        "NoMines",
        "Little",
        "Wide",
        "Big",
        "Quick",
        "BMRize",
        "Skippy",
        "Mines",
        "Echo",
        "Stomp",
        "Planted",
        "Floored",
        "Twister",
        "HoldsToRolls",
        "NoJumps",
        "NoHands",
        "NoLifts",
        "NoFakes",
        "NoQuads",
    ];
    const MASKS: [[u32; 256]; 3] = {
        let mut maps = [[0u32; 256]; 3];
        let mut value = 0usize;
        while value < 256 {
            maps[0][value] = ((value & 1) as u32) << 4
                | (((value >> 1) & 1) as u32) << 5
                | (((value >> 2) & 1) as u32) << 6
                | (((value >> 3) & 1) as u32) << 7
                | (((value >> 4) & 1) as u32) << 8
                | (((value >> 7) & 1) as u32) << 9
                | (((value >> 5) & 1) as u32) << 10
                | (((value >> 6) & 1) as u32) << 11;
            maps[1][value] = (((value >> 2) & 1) as u32)
                | (((value >> 1) & 1) as u32) << 2
                | ((value & 1) as u32) << 3
                | (((value >> 3) & 1) as u32) << 16
                | (((value >> 4) & 1) as u32) << 17
                | (((value >> 6) & 1) as u32) << 18
                | (((value >> 7) & 1) as u32) << 19
                | (((value >> 5) & 1) as u32) << 20;
            maps[2][value] = (((value >> 3) & 1) as u32) << 1
                | ((value & 1) as u32) << 12
                | (((value >> 1) & 1) as u32) << 13
                | (((value >> 2) & 1) as u32) << 14
                | (((value >> 4) & 1) as u32) << 15;
            value += 1;
        }
        maps
    };
    let mut remaining = MASKS[0][usize::from(insert_mask)]
        | MASKS[1][usize::from(remove_mask)]
        | MASKS[2][usize::from(holds_mask)];
    while remaining != 0 {
        begin_display_mod_option(out);
        out.push_str(NAMES[remaining.trailing_zeros() as usize]);
        remaining &= remaining - 1;
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

#[must_use]
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
    for (percent, name) in params.appearance.into_iter().zip([
        "Hidden",
        "Sudden",
        "Dynamic Sudden",
        "Stealth",
        "Blink",
        "RandomVanish",
    ]) {
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
        push_i16_decimal(&mut out, params.visual_delay_ms);
        out.push_str("ms\u{00A0}VisualDelay");
    }
    append_average_error_bar_part(&mut out, params);
    append_disabled_timing_windows(&mut out, params.disabled_timing_windows);
    out
}

#[cfg(test)]
#[path = "text_performance.rs"]
mod text_performance;

#[cfg(test)]
mod number_performance {
    include!("number_performance.rs");
}
