// Frozen core color code from 552763528, with public parent visibility for four private value helpers.
use mlua::Value;
use crate::read_string;
pub const SONG_LUA_ACTIVE_COLOR_INDEX: i64 = 1;
pub const SL_COLORS: &[&str] = &[
    "#FF5D47", "#FF577E", "#FF47B3", "#DD57FF", "#8885ff", "#3D94FF", "#00B8CC", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];
pub const SL_DECORATIVE_COLORS: &[&str] = &[
    "#FF3C23", "#FF003C", "#C1006F", "#8200A1", "#413AD0", "#0073FF", "#00ADC0", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];
pub const ITG_DIFF_COLORS: &[&str] = &[
    "#a355b8", "#1ec51d", "#d6db41", "#ba3049", "#2691c5", "#F7F7F7",
];
pub const DDR_DIFF_COLORS: &[&str] = &[
    "#2dccef", "#eaa910", "#ff344d", "#30d81e", "#e900ff", "#F7F7F7",
];
pub const SL_JUDGMENT_COLORS: &[&str] = &[
    "#21CCE8", "#e29c18", "#66c955", "#b45cff", "#c9855e", "#ff3030",
];
pub const SL_FA_PLUS_COLORS: &[&str] = &[
    "#21CCE8", "#ffffff", "#e29c18", "#66c955", "#b45cff", "#ff3030", "#ff00cc",
];

pub fn parse_color_text(text: &str) -> Option<[f32; 4]> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(hex) = text.strip_prefix('#')
        && matches!(hex.len(), 6 | 8)
    {
        let r = f32::from(u8::from_str_radix(&hex[0..2], 16).ok()?) / 255.0;
        let g = f32::from(u8::from_str_radix(&hex[2..4], 16).ok()?) / 255.0;
        let b = f32::from(u8::from_str_radix(&hex[4..6], 16).ok()?) / 255.0;
        let a = if hex.len() == 8 {
            f32::from(u8::from_str_radix(&hex[6..8], 16).ok()?) / 255.0
        } else {
            1.0
        };
        return Some([r, g, b, a]);
    }
    let mut parts = text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let red = parts.next()?.parse::<f32>().ok()?;
    let green = parts.next()?.parse::<f32>().ok()?;
    let blue = parts.next()?.parse::<f32>().ok()?;
    let alpha = match parts.next() {
        Some(alpha) => alpha.parse::<f32>().ok()?,
        None => 1.0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some([red, green, blue, alpha])
}

#[must_use]
pub fn palette_color(index: i64, palette: &[&str]) -> [f32; 4] {
    if palette.is_empty() {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let wrapped = (index - 1).rem_euclid(palette.len() as i64) as usize;
    parse_color_text(palette[wrapped]).unwrap_or([1.0, 1.0, 1.0, 1.0])
}

#[must_use]
pub const fn song_lua_palette(
    diff_palette: Option<&str>,
    decorative: bool,
) -> &'static [&'static str] {
    match diff_palette {
        Some(value) if value.eq_ignore_ascii_case("ITG") => ITG_DIFF_COLORS,
        Some(value) if value.eq_ignore_ascii_case("DDR") => DDR_DIFF_COLORS,
        _ if decorative => SL_DECORATIVE_COLORS,
        _ => SL_COLORS,
    }
}

#[must_use]
pub fn song_lua_player_color(player: usize, decorative: bool) -> [f32; 4] {
    let index = match player {
        0 => SONG_LUA_ACTIVE_COLOR_INDEX,
        1 => SONG_LUA_ACTIVE_COLOR_INDEX - 2,
        _ => return [1.0, 1.0, 1.0, 1.0],
    };
    palette_color(index, song_lua_palette(None, decorative))
}

#[must_use]
pub fn song_lua_player_score_color(player: usize) -> [f32; 4] {
    let index = match player {
        0 => SONG_LUA_ACTIVE_COLOR_INDEX,
        1 => SONG_LUA_ACTIVE_COLOR_INDEX - 2,
        _ => return [1.0, 1.0, 1.0, 1.0],
    };
    palette_color(index, SL_COLORS)
}

#[must_use]
pub fn song_lua_player_dark_color(player: usize) -> [f32; 4] {
    match player {
        0 => parse_color_text("#da4453").unwrap_or([1.0, 1.0, 1.0, 1.0]),
        1 => parse_color_text("#4a89dc").unwrap_or([1.0, 1.0, 1.0, 1.0]),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

#[must_use]
pub fn song_lua_difficulty_index(name: &str) -> Option<i64> {
    match name {
        "Beginner" | "Difficulty_Beginner" => Some(0),
        "Easy" | "Difficulty_Easy" => Some(1),
        "Medium" | "Difficulty_Medium" => Some(2),
        "Hard" | "Difficulty_Hard" => Some(3),
        "Challenge" | "Difficulty_Challenge" => Some(4),
        "Edit" | "Difficulty_Edit" => Some(5),
        _ => None,
    }
}

#[must_use]
pub fn song_lua_difficulty_color(difficulty: i64, decorative: bool) -> [f32; 4] {
    if difficulty == 5 {
        return parse_color_text("#B4B7BA").unwrap_or([1.0; 4]);
    }
    palette_color(
        SONG_LUA_ACTIVE_COLOR_INDEX + difficulty - 4,
        song_lua_palette(None, decorative),
    )
}

#[must_use]
pub fn custom_difficulty_color(name: &str) -> Option<[f32; 4]> {
    let hex = match name {
        "Beginner" | "Difficulty_Beginner" => "#ff32f8",
        "Easy" | "Difficulty_Easy" | "Freestyle" => "#2cff00",
        "Medium" | "Difficulty_Medium" | "HalfDouble" => "#fee600",
        "Hard" | "Difficulty_Hard" | "Crazy" => "#ff2f39",
        "Challenge" | "Difficulty_Challenge" | "Nightmare" => "#1cd8ff",
        "Edit" | "Difficulty_Edit" => "#cccccc",
        "Couple" | "Difficulty_Couple" => "#ed0972",
        "Routine" | "Difficulty_Routine" => "#ff9a00",
        _ => return None,
    };
    parse_color_text(hex)
}

#[must_use]
pub fn stage_color(name: &str) -> Option<[f32; 4]> {
    let hex = match name {
        "Stage_1st" => "#00ffc7",
        "Stage_2nd" => "#58ff00",
        "Stage_3rd" => "#f400ff",
        "Stage_4th" => "#00ffda",
        "Stage_5th" => "#ed00ff",
        "Stage_6th" => "#73ff00",
        "Stage_Next" => "#73ff00",
        "Stage_Final" | "Stage_Extra2" => "#ff0707",
        "Stage_Extra1" => "#fafa00",
        "Stage_Nonstop" | "Stage_Oni" | "Stage_Endless" | "Stage_Event" | "Stage_Demo" => "#ffffff",
        _ => return None,
    };
    parse_color_text(hex)
}

#[must_use]
pub fn judgment_line_color(name: &str) -> Option<[f32; 4]> {
    let hex = match name {
        "JudgmentLine_W1" => "#bfeaff",
        "JudgmentLine_W2" => "#fff568",
        "JudgmentLine_W3" => "#a4ff00",
        "JudgmentLine_W4" => "#34bfff",
        "JudgmentLine_W5" => "#e44dff",
        "JudgmentLine_Held" => "#ffffff",
        "JudgmentLine_Miss" => "#ff3c3c",
        "JudgmentLine_MaxCombo" => "#ffc600",
        _ => return None,
    };
    parse_color_text(hex)
}

#[must_use]
pub fn light_color(color: [f32; 4]) -> [f32; 4] {
    [
        color[0].mul_add(0.5, 0.5),
        color[1].mul_add(0.5, 0.5),
        color[2].mul_add(0.5, 0.5),
        color[3],
    ]
}

#[must_use]
pub fn tone_color(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

#[must_use]
pub const fn blend_color(first: [f32; 4], second: [f32; 4]) -> [f32; 4] {
    [
        f32::midpoint(first[0], second[0]),
        f32::midpoint(first[1], second[1]),
        f32::midpoint(first[2], second[2]),
        f32::midpoint(first[3], second[3]),
    ]
}

#[must_use]
pub fn color_to_hex(color: [f32; 4]) -> String {
    let component = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u8;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut bytes = [0; 8];
    for (output, value) in bytes.chunks_exact_mut(2).zip(color) {
        let value = component(value);
        output[0] = HEX[(value >> 4) as usize];
        output[1] = HEX[(value & 0x0f) as usize];
    }
    String::from_utf8(bytes.to_vec()).expect("hexadecimal lookup table contains only ASCII")
}

pub(super) fn custom_difficulty_color_value(value: Value) -> Option<[f32; 4]> {
    custom_difficulty_color(&read_string(value)?)
}
pub(super) fn stage_color_value(value: Value) -> Option<[f32; 4]> {
    stage_color(&read_string(value)?)
}
pub(super) fn judgment_line_color_value(value: Value) -> Option<[f32; 4]> {
    judgment_line_color(&read_string(value)?)
}
pub(super) fn steps_or_trail_color(value: Value) -> Option<[f32; 4]> {
    if let Some(color) = custom_difficulty_color_value(value.clone()) {
        return Some(color);
    }
    let Value::Table(table) = value else {
        return None;
    };
    let Value::Function(get_difficulty) = table.get::<Value>("GetDifficulty").ok()? else {
        return None;
    };
    custom_difficulty_color_value(get_difficulty.call::<Value>(table).ok()?)
}
