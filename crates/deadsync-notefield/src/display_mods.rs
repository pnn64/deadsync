use deadsync_rules::scroll::ScrollSpeedSetting;

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

fn trim_float2(value: f32) -> String {
    let mut s = format!("{value:.2}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

fn format_speed(speed: ScrollSpeedSetting) -> String {
    match speed {
        ScrollSpeedSetting::XMod(v) => format!("{}x", trim_float2(v)),
        ScrollSpeedSetting::CMod(v) => format!("C{}", trim_float2(v)),
        ScrollSpeedSetting::MMod(v) => format!("M{}", trim_float2(v)),
    }
}

fn append_mod_part(parts: &mut Vec<String>, percent: i16, name: &str) {
    if percent != 0 {
        parts.push(format!("{percent}% {name}"));
    }
}

pub(crate) fn append_mini_part(parts: &mut Vec<String>, mini_percent: i16) {
    append_mod_part(parts, mini_percent, "Mini");
}

fn append_spacing_part(parts: &mut Vec<String>, spacing_percent: i16) {
    append_mod_part(parts, spacing_percent, "Spacing");
}

pub(crate) fn append_average_error_bar_part(
    parts: &mut Vec<String>,
    params: GameplayModsTextParams<'_>,
) {
    if params.average_error_bar_active {
        let intensity = trim_float2(params.avg_error_bar_intensity_centi as f32 / 100.0);
        parts.push(format!(
            "ErrorBar{intensity}x(Avg:{}ms)",
            params.avg_error_bar_interval_ms
        ));
    }
}

fn push_display_mod_option(out: &mut String, option: &str) {
    if !out.is_empty() {
        out.push_str(", ");
    }
    out.push_str(&option.replace(' ', "\u{00A0}"));
}

pub(crate) fn join_display_mod_parts(parts: &[String]) -> String {
    let mut out = String::new();
    for part in parts {
        push_display_mod_option(&mut out, part);
    }
    out
}

pub(crate) fn append_perspective_parts(parts: &mut Vec<String>, tilt: i16, skew: i16) {
    if tilt == 0 && skew == 0 {
        parts.push("Overhead".to_string());
    } else if tilt < 0 || skew > 0 {
        parts.push("Incoming".to_string());
    } else if tilt > 0 || skew < 0 {
        parts.push("Space".to_string());
    }
}

pub(crate) fn append_turn_parts(parts: &mut Vec<String>, bits: u16) {
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
            parts.push(name.to_string());
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
    parts: &mut Vec<String>,
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
) {
    if remove_mask & (1 << 2) != 0 {
        parts.push("NoHolds".to_string());
    }
    if holds_mask & (1 << 3) != 0 {
        parts.push("NoRolls".to_string());
    }
    if remove_mask & (1 << 1) != 0 {
        parts.push("NoMines".to_string());
    }
    if remove_mask & (1 << 0) != 0 {
        parts.push("Little".to_string());
    }
    if insert_mask & (1 << 0) != 0 {
        parts.push("Wide".to_string());
    }
    if insert_mask & (1 << 1) != 0 {
        parts.push("Big".to_string());
    }
    if insert_mask & (1 << 2) != 0 {
        parts.push("Quick".to_string());
    }
    if insert_mask & (1 << 3) != 0 {
        parts.push("BMRize".to_string());
    }
    if insert_mask & (1 << 4) != 0 {
        parts.push("Skippy".to_string());
    }
    if insert_mask & (1 << 7) != 0 {
        parts.push("Mines".to_string());
    }
    if insert_mask & (1 << 5) != 0 {
        parts.push("Echo".to_string());
    }
    if insert_mask & (1 << 6) != 0 {
        parts.push("Stomp".to_string());
    }
    if holds_mask & (1 << 0) != 0 {
        parts.push("Planted".to_string());
    }
    if holds_mask & (1 << 1) != 0 {
        parts.push("Floored".to_string());
    }
    if holds_mask & (1 << 2) != 0 {
        parts.push("Twister".to_string());
    }
    if holds_mask & (1 << 4) != 0 {
        parts.push("HoldsToRolls".to_string());
    }
    if remove_mask & (1 << 3) != 0 {
        parts.push("NoJumps".to_string());
    }
    if remove_mask & (1 << 4) != 0 {
        parts.push("NoHands".to_string());
    }
    if remove_mask & (1 << 6) != 0 {
        parts.push("NoLifts".to_string());
    }
    if remove_mask & (1 << 7) != 0 {
        parts.push("NoFakes".to_string());
    }
    if remove_mask & (1 << 5) != 0 {
        parts.push("NoQuads".to_string());
    }
}

pub(crate) fn disabled_timing_windows_name(bits: u8) -> Option<String> {
    if bits == 0 {
        return None;
    }
    let mut windows = Vec::new();
    for ix in 0..5 {
        if bits & (1 << ix) != 0 {
            windows.push(format!("W{}", ix + 1));
        }
    }
    Some(format!("No {}", windows.join("/")))
}

pub fn gameplay_mods_text(params: GameplayModsTextParams<'_>) -> String {
    let mut parts = Vec::new();
    parts.push(format_speed(params.speed));
    append_mini_part(&mut parts, params.mini_percent);
    append_spacing_part(&mut parts, params.spacing_percent);
    if let Some(name) = attack_mode_name(params.attack_mode) {
        parts.push(name.to_string());
    }
    append_turn_parts(&mut parts, params.turn_bits);
    push_transform_parts(
        &mut parts,
        params.insert_mask,
        params.remove_mask,
        params.holds_mask,
    );
    append_perspective_parts(&mut parts, params.perspective_tilt, params.perspective_skew);
    if !params.noteskin.is_empty() {
        parts.push(params.noteskin.to_string());
    }
    if params.visual_delay_ms != 0 {
        parts.push(format!("{}ms VisualDelay", params.visual_delay_ms));
    }
    append_average_error_bar_part(&mut parts, params);
    if let Some(name) = disabled_timing_windows_name(params.disabled_timing_windows) {
        parts.push(name);
    }
    join_display_mod_parts(&parts)
}
