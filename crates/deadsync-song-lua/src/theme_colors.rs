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

const fn fixed_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

const fn fixed_hex_pair(first: u8, second: u8) -> Option<u8> {
    let Some(low) = fixed_hex_digit(second) else {
        return None;
    };
    if first == b'+' {
        return Some(low);
    }
    let Some(high) = fixed_hex_digit(first) else {
        return None;
    };
    Some((high << 4) | low)
}

// These literals are decoded during compilation; runtime parsing stays unchanged.
const fn fixed_color(text: &str) -> [f32; 4] {
    let bytes = text.as_bytes();
    assert!(bytes.len() == 7 && bytes[0] == b'#');
    let Some(r) = fixed_hex_pair(bytes[1], bytes[2]) else {
        panic!("invalid fixed red");
    };
    let Some(g) = fixed_hex_pair(bytes[3], bytes[4]) else {
        panic!("invalid fixed green");
    };
    let Some(b) = fixed_hex_pair(bytes[5], bytes[6]) else {
        panic!("invalid fixed blue");
    };
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

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
        0 => const { fixed_color("#da4453") },
        1 => const { fixed_color("#4a89dc") },
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
        return const { fixed_color("#B4B7BA") };
    }
    palette_color(
        SONG_LUA_ACTIVE_COLOR_INDEX + difficulty - 4,
        song_lua_palette(None, decorative),
    )
}

#[must_use]
pub fn custom_difficulty_color(name: &str) -> Option<[f32; 4]> {
    let color = match name {
        "Beginner" | "Difficulty_Beginner" => const { fixed_color("#ff32f8") },
        "Easy" | "Difficulty_Easy" | "Freestyle" => const { fixed_color("#2cff00") },
        "Medium" | "Difficulty_Medium" | "HalfDouble" => const { fixed_color("#fee600") },
        "Hard" | "Difficulty_Hard" | "Crazy" => const { fixed_color("#ff2f39") },
        "Challenge" | "Difficulty_Challenge" | "Nightmare" => const { fixed_color("#1cd8ff") },
        "Edit" | "Difficulty_Edit" => const { fixed_color("#cccccc") },
        "Couple" | "Difficulty_Couple" => const { fixed_color("#ed0972") },
        "Routine" | "Difficulty_Routine" => const { fixed_color("#ff9a00") },
        _ => return None,
    };
    Some(color)
}

#[must_use]
pub fn stage_color(name: &str) -> Option<[f32; 4]> {
    let color = match name {
        "Stage_1st" => const { fixed_color("#00ffc7") },
        "Stage_2nd" => const { fixed_color("#58ff00") },
        "Stage_3rd" => const { fixed_color("#f400ff") },
        "Stage_4th" => const { fixed_color("#00ffda") },
        "Stage_5th" => const { fixed_color("#ed00ff") },
        "Stage_6th" => const { fixed_color("#73ff00") },
        "Stage_Next" => const { fixed_color("#73ff00") },
        "Stage_Final" | "Stage_Extra2" => const { fixed_color("#ff0707") },
        "Stage_Extra1" => const { fixed_color("#fafa00") },
        "Stage_Nonstop" | "Stage_Oni" | "Stage_Endless" | "Stage_Event" | "Stage_Demo" => {
            const { fixed_color("#ffffff") }
        }
        _ => return None,
    };
    Some(color)
}

#[must_use]
pub fn judgment_line_color(name: &str) -> Option<[f32; 4]> {
    let color = match name {
        "JudgmentLine_W1" => const { fixed_color("#bfeaff") },
        "JudgmentLine_W2" => const { fixed_color("#fff568") },
        "JudgmentLine_W3" => const { fixed_color("#a4ff00") },
        "JudgmentLine_W4" => const { fixed_color("#34bfff") },
        "JudgmentLine_W5" => const { fixed_color("#e44dff") },
        "JudgmentLine_Held" => const { fixed_color("#ffffff") },
        "JudgmentLine_Miss" => const { fixed_color("#ff3c3c") },
        "JudgmentLine_MaxCombo" => const { fixed_color("#ffc600") },
        _ => return None,
    };
    Some(color)
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

#[cfg(test)]
mod tests {
    use super::parse_color_text;

    #[test]
    fn numeric_colors_require_three_or_four_valid_components() {
        assert_eq!(parse_color_text("1,,0.5,0.25"), Some([1.0, 0.5, 0.25, 1.0]));
        assert_eq!(
            parse_color_text("1,0.5,0.25,0.75"),
            Some([1.0, 0.5, 0.25, 0.75])
        );
        assert_eq!(parse_color_text("1,0.5,0.25,1,0"), None);
        assert_eq!(parse_color_text("1,bad,0.25,1"), None);
    }
}
