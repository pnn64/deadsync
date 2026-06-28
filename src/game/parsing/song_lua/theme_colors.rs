use mlua::{Lua, MultiValue, Table, Value};

use super::util::{
    make_color_table, parse_color_text, player_index_from_value, read_boolish, read_color_value,
    read_f32, read_string,
};

pub(super) const SONG_LUA_ACTIVE_COLOR_INDEX: i64 = 1;
pub(super) const SL_COLORS: &[&str] = &[
    "#FF5D47", "#FF577E", "#FF47B3", "#DD57FF", "#8885ff", "#3D94FF", "#00B8CC", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];
pub(super) const SL_DECORATIVE_COLORS: &[&str] = &[
    "#FF3C23", "#FF003C", "#C1006F", "#8200A1", "#413AD0", "#0073FF", "#00ADC0", "#5CE087",
    "#AEFA44", "#FFFF00", "#FFBE00", "#FF7D00",
];
pub(super) const ITG_DIFF_COLORS: &[&str] = &[
    "#a355b8", "#1ec51d", "#d6db41", "#ba3049", "#2691c5", "#F7F7F7",
];
pub(super) const DDR_DIFF_COLORS: &[&str] = &[
    "#2dccef", "#eaa910", "#ff344d", "#30d81e", "#e900ff", "#F7F7F7",
];
pub(super) const SL_JUDGMENT_COLORS: &[&str] = &[
    "#21CCE8", "#e29c18", "#66c955", "#b45cff", "#c9855e", "#ff3030",
];
pub(super) const SL_FA_PLUS_COLORS: &[&str] = &[
    "#21CCE8", "#ffffff", "#e29c18", "#66c955", "#b45cff", "#ff3030", "#ff00cc",
];

pub(super) fn create_color_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(
            index + 1,
            make_color_table(lua, parse_color_text(value).unwrap_or([1.0, 1.0, 1.0, 1.0]))?,
        )?;
    }
    Ok(table)
}

pub(super) fn install_theme_color_helpers(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.set(
        "GetHexColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(index) = args
                .get(0)
                .cloned()
                .and_then(read_f32)
                .map(|value| value.trunc() as i64)
            else {
                return Ok(make_color_table(lua, [1.0, 1.0, 1.0, 1.0])?);
            };
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            let diff_palette = args.get(2).cloned().and_then(read_string);
            let itg_diff = diff_palette
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("ITG"));
            let palette = match diff_palette.as_deref() {
                Some(value) if value.eq_ignore_ascii_case("ITG") => ITG_DIFF_COLORS,
                Some(value) if value.eq_ignore_ascii_case("DDR") => DDR_DIFF_COLORS,
                _ if decorative => SL_DECORATIVE_COLORS,
                _ => SL_COLORS,
            };
            let color = palette_color(index, palette);
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
            let decorative = args.get(0).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(
                lua,
                palette_color(
                    SONG_LUA_ACTIVE_COLOR_INDEX,
                    if decorative {
                        SL_DECORATIVE_COLORS
                    } else {
                        SL_COLORS
                    },
                ),
            )
        })?,
    )?;
    globals.set(
        "PlayerColor",
        lua.create_function(|lua, args: MultiValue| {
            let player = args.get(0).and_then(player_index_from_value);
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            let index = match player {
                Some(0) => SONG_LUA_ACTIVE_COLOR_INDEX,
                Some(1) => SONG_LUA_ACTIVE_COLOR_INDEX - 2,
                _ => return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]),
            };
            make_color_table(
                lua,
                palette_color(
                    index,
                    if decorative {
                        SL_DECORATIVE_COLORS
                    } else {
                        SL_COLORS
                    },
                ),
            )
        })?,
    )?;
    globals.set(
        "PlayerScoreColor",
        lua.create_function(|lua, args: MultiValue| {
            let player = args.get(0).and_then(player_index_from_value);
            let index = match player {
                Some(0) => SONG_LUA_ACTIVE_COLOR_INDEX,
                Some(1) => SONG_LUA_ACTIVE_COLOR_INDEX - 2,
                _ => return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]),
            };
            make_color_table(lua, palette_color(index, SL_COLORS))
        })?,
    )?;
    globals.set(
        "PlayerDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = match args.get(0).and_then(player_index_from_value) {
                Some(0) => parse_color_text("#da4453").unwrap_or([1.0, 1.0, 1.0, 1.0]),
                Some(1) => parse_color_text("#4a89dc").unwrap_or([1.0, 1.0, 1.0, 1.0]),
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "DifficultyColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(difficulty) = args.get(0).cloned().and_then(difficulty_index_from_value)
            else {
                return make_color_table(lua, parse_color_text("#B4B7BA").unwrap_or([1.0; 4]));
            };
            if difficulty == 5 {
                return make_color_table(lua, parse_color_text("#B4B7BA").unwrap_or([1.0; 4]));
            }
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(
                lua,
                palette_color(
                    SONG_LUA_ACTIVE_COLOR_INDEX + difficulty - 4,
                    if decorative {
                        SL_DECORATIVE_COLORS
                    } else {
                        SL_COLORS
                    },
                ),
            )
        })?,
    )?;
    globals.set(
        "CustomDifficultyToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(custom_difficulty_color)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "CustomDifficultyToDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(custom_difficulty_color)
                .map(|color| tone_color(color, 0.5))
                .unwrap_or([0.5, 0.5, 0.5, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "CustomDifficultyToLightColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(custom_difficulty_color)
                .map(|color| {
                    [
                        light_color_component(color[0]),
                        light_color_component(color[1]),
                        light_color_component(color[2]),
                        color[3],
                    ]
                })
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "StepsOrTrailToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(steps_or_trail_color)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "StageToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(stage_color)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "StageToStrokeColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(stage_color)
                .map(|color| tone_color(color, 0.5))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "JudgmentLineToStrokeColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(judgment_line_color)
                .map(|color| tone_color(color, 0.5))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
        })?,
    )?;
    globals.set(
        "JudgmentLineToColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(judgment_line_color)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            make_color_table(lua, color)
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
                    .get(0)
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
                .get(0)
                .cloned()
                .and_then(read_color_value)
                .map(|color| color[3])
                .unwrap_or(1.0))
        })?,
    )?;
    globals.set(
        "ColorToHex",
        lua.create_function(|_, args: MultiValue| {
            let color = args
                .get(0)
                .cloned()
                .and_then(read_color_value)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            Ok(color_to_hex(color))
        })?,
    )?;
    globals.set(
        "BoostColor",
        lua.create_function(|lua, args: MultiValue| {
            let color = args
                .get(0)
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
                .get(0)
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
    Ok(())
}

fn palette_color(index: i64, palette: &[&str]) -> [f32; 4] {
    if palette.is_empty() {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let wrapped = (index - 1).rem_euclid(palette.len() as i64) as usize;
    parse_color_text(palette[wrapped]).unwrap_or([1.0, 1.0, 1.0, 1.0])
}

fn difficulty_index_from_value(value: Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() => Some(value.trunc() as i64),
        Value::String(text) => match text.to_str().ok()?.as_ref() {
            "Beginner" | "Difficulty_Beginner" => Some(0),
            "Easy" | "Difficulty_Easy" => Some(1),
            "Medium" | "Difficulty_Medium" => Some(2),
            "Hard" | "Difficulty_Hard" => Some(3),
            "Challenge" | "Difficulty_Challenge" => Some(4),
            "Edit" | "Difficulty_Edit" => Some(5),
            _ => None,
        },
        _ => None,
    }
}

fn custom_difficulty_color(value: Value) -> Option<[f32; 4]> {
    let name = read_string(value)?;
    let hex = match name.as_str() {
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

fn steps_or_trail_color(value: Value) -> Option<[f32; 4]> {
    if let Some(color) = custom_difficulty_color(value.clone()) {
        return Some(color);
    }
    let table = match value {
        Value::Table(table) => table,
        _ => return None,
    };
    let Value::Function(get_difficulty) = table.get::<Value>("GetDifficulty").ok()? else {
        return None;
    };
    custom_difficulty_color(get_difficulty.call::<Value>(table).ok()?)
}

fn stage_color(value: Value) -> Option<[f32; 4]> {
    let name = read_string(value)?;
    let hex = match name.as_str() {
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

fn judgment_line_color(value: Value) -> Option<[f32; 4]> {
    let name = read_string(value)?;
    let hex = match name.as_str() {
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

fn light_color_component(value: f32) -> f32 {
    value * 0.5 + 0.5
}

fn tone_color(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

fn blend_color(first: [f32; 4], second: [f32; 4]) -> [f32; 4] {
    [
        0.5 * (first[0] + second[0]),
        0.5 * (first[1] + second[1]),
        0.5 * (first[2] + second[2]),
        0.5 * (first[3] + second[3]),
    ]
}

fn color_to_hex(color: [f32; 4]) -> String {
    let component = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u8;
    format!(
        "{:02X}{:02X}{:02X}{:02X}",
        component(color[0]),
        component(color[1]),
        component(color[2]),
        component(color[3])
    )
}
