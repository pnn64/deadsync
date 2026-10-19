use mlua::{Lua, MultiValue, Table, Value};

use crate::lua_util::{make_color_table, read_color_value};
use crate::values::{player_index_from_value, read_boolish, read_f32, read_string};

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

pub fn install_theme_color_helpers(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.set(
        "GetHexColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(index) = args
                .front()
                .cloned()
                .and_then(read_f32)
                .map(|value| value.trunc() as i64)
            else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            let diff_palette = args.get(2).cloned().and_then(read_string);
            let itg_diff = diff_palette
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("ITG"));
            let color = palette_color(index, song_lua_palette(diff_palette.as_deref(), decorative));
            make_color_table(
                lua,
                if itg_diff && !decorative {
                    tone_color(color, 1.25)
                } else {
                    color
                },
            )
        })?,
    )?;
    globals.set(
        "GetCurrentColor",
        lua.create_function(|lua, args: MultiValue| {
            let decorative = args
                .front()
                .cloned()
                .and_then(read_boolish)
                .unwrap_or(false);
            make_color_table(
                lua,
                palette_color(
                    SONG_LUA_ACTIVE_COLOR_INDEX,
                    song_lua_palette(None, decorative),
                ),
            )
        })?,
    )?;
    globals.set(
        "PlayerColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(lua, song_lua_player_color(player, decorative))
        })?,
    )?;
    globals.set(
        "PlayerScoreColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            make_color_table(lua, song_lua_player_score_color(player))
        })?,
    )?;
    globals.set(
        "PlayerDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(player) = args.front().and_then(player_index_from_value) else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            make_color_table(lua, song_lua_player_dark_color(player))
        })?,
    )?;
    globals.set(
        "DifficultyColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(difficulty) = args.front().cloned().and_then(difficulty_index_from_value)
            else {
                return make_color_table(lua, parse_color_text("#B4B7BA").unwrap_or([1.0; 4]));
            };
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(lua, song_lua_difficulty_color(difficulty, decorative))
        })?,
    )?;
    globals.set(
        "CustomDifficultyToColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(custom_difficulty_color_value)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "CustomDifficultyToDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(custom_difficulty_color_value)
                    .map(|color| tone_color(color, 0.5))
                    .unwrap_or([0.5, 0.5, 0.5, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "CustomDifficultyToLightColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(custom_difficulty_color_value)
                    .map(light_color)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "StepsOrTrailToColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(steps_or_trail_color)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "StageToColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(stage_color_value)
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "StageToStrokeColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(stage_color_value)
                    .map(|color| tone_color(color, 0.5))
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "JudgmentLineToStrokeColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(judgment_line_color_value)
                    .map(|color| tone_color(color, 0.5))
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            )
        })?,
    )?;
    globals.set(
        "JudgmentLineToColor",
        lua.create_function(|lua, args: MultiValue| {
            make_color_table(
                lua,
                args.front()
                    .cloned()
                    .and_then(judgment_line_color_value)
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            )
        })?,
    )?;
    for (name, factor) in [
        ("LightenColor", 1.25_f32),
        ("ColorLightTone", 1.5_f32),
        ("ColorMidTone", 1.0_f32 / 1.5_f32),
        ("ColorDarkTone", 0.5_f32),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, args: MultiValue| {
                let color = args
                    .front()
                    .cloned()
                    .and_then(read_color_value)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                make_color_table(lua, tone_color(color, factor))
            })?,
        )?;
    }
    globals.set(
        "HasAlpha",
        lua.create_function(|_, args: MultiValue| {
            Ok(args
                .front()
                .cloned()
                .and_then(read_color_value)
                .map(|color| color[3])
                .unwrap_or(1.0))
        })?,
    )?;
    globals.set(
        "ColorToHex",
        lua.create_function(|_, args: MultiValue| {
            Ok(color_to_hex(
                args.front()
                    .cloned()
                    .and_then(read_color_value)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            ))
        })?,
    )?;
    globals.set(
        "BoostColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .front()
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let boost = args.get(1).cloned().and_then(read_f32).unwrap_or(1.0);
            make_color_table(lua, tone_color(color, boost))
        })?,
    )?;
    globals.set(
        "BlendColors",
        lua.create_function(|lua, args: MultiValue| {
            let first = args
                .front()
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let second = args
                .get(1)
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, blend_color(first, second))
        })?,
    )?;
    // ITGmania fallback 02 Colors.lua keeps Lua number precision and uses
    // its final sector branch for every hue outside sectors zero to four.
    lua.load(r#"
function HSVToColor(hsv)
    local s, v = hsv.Sat, hsv.Value
    local a = hsv.Alpha or 0
    if s == 0 then
        return {v, v, v, a}
    end
    local h = hsv.Hue / 60
    local i = math.floor(h)
    local f = h - i
    local p = v * (1 - s)
    local q = v * (1 - s * f)
    local t = v * (1 - s * (1 - f))
    if i == 0 then return {v, t, p, a}
    elseif i == 1 then return {q, v, p, a}
    elseif i == 2 then return {p, v, t, a}
    elseif i == 3 then return {p, q, v, a}
    elseif i == 4 then return {t, p, v, a}
    else return {v, p, q, a}
    end
end
function HSVA(h, s, v, a)
    return HSVToColor{Hue=h, Sat=s, Value=v, Alpha=a}
end
"#)
        .set_name("native HSV color helpers")
        .set_environment(globals.clone())
        .exec()?;
    // Retain the original fallback functions and their source line numbers.
    lua.load(r#"




































































































































































































































































































function ColorToHSV(c)
	local r = c[1]
	local g = c[2]
	local b = c[3]
	-- alpha requires error checking sometimes.
	local a = HasAlpha(c)

	local h = 0
	local s = 0
	local v = 0

	local min = math.min( r, g, b )
	local max = math.max( r, g, b )
	v = max

	local delta = max - min

	-- xxx: how do we deal with complete black?
	if min == 0 and max == 0 then
		-- we have complete darkness; make it cheap.
		return {
			Hue = 0,
			Sat = 0,
			Value = 0,
			Alpha = a
		}
	end

	if max ~= 0 then
		s = delta / max -- rofl deltamax :|
	else
		-- r = g = b = 0; s = 0, v is undefined
		s = 0
		h = -1
		return {
			Hue = h,
			Sat = s,
			Value = v,
			Alpha = 1
		}
	end

	if r == max then
		h = ( g - b ) / delta     -- yellow/magenta
	elseif g == max then
		h = 2 + ( b - r ) / delta -- cyan/yellow
	else
		h = 4 + ( r - g ) / delta -- magenta/cyan
	end

	h = h * 60 -- degrees

	if h < 0 then
		h = h + 360
	end

	return {
		Hue = h,
		Sat = s,
		Value = v,
		Alpha = a
	}
end

























function Brightness(color,percent)
	local c = ColorToHSV(color)
	-- error checking
	if percent < 0 then
		percent = 0.0
	elseif percent > 1 then
		percent = 1.0
	end
	c.Value = percent
	return HSVToColor(c)
end

"#).set_name("@theme:/Scripts/02 Colors.lua").exec()?;
    Ok(())
}

fn difficulty_index_from_value(value: Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() => Some(value.trunc() as i64),
        Value::String(text) => song_lua_difficulty_index(text.to_str().ok()?.as_ref()),
        _ => None,
    }
}

fn custom_difficulty_color_value(value: Value) -> Option<[f32; 4]> {
    let Value::String(text) = value else {
        return None;
    };
    custom_difficulty_color(text.to_str().ok()?.as_ref())
}

fn steps_or_trail_color(value: Value) -> Option<[f32; 4]> {
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

fn stage_color_value(value: Value) -> Option<[f32; 4]> {
    let Value::String(text) = value else {
        return None;
    };
    stage_color(text.to_str().ok()?.as_ref())
}

fn judgment_line_color_value(value: Value) -> Option<[f32; 4]> {
    let Value::String(text) = value else {
        return None;
    };
    judgment_line_color(text.to_str().ok()?.as_ref())
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

#[cfg(test)]
#[path = "../tests/perf/color_inputs.rs"]
mod input_perf;
