use mlua::{Lua, MultiValue, Table, Value};

use deadsync_song_lua::{
    SONG_LUA_ACTIVE_COLOR_INDEX, blend_color, color_to_hex, custom_difficulty_color,
    judgment_line_color, light_color, make_color_table, palette_color, parse_color_text,
    player_index_from_value, read_boolish, read_color_value, read_f32, read_string,
    song_lua_difficulty_color, song_lua_difficulty_index, song_lua_palette, song_lua_player_color,
    song_lua_player_dark_color, song_lua_player_score_color, stage_color, tone_color,
};

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
            let decorative = args.get(0).cloned().and_then(read_boolish).unwrap_or(false);
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
            let Some(player) = args.get(0).and_then(player_index_from_value) else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            let decorative = args.get(1).cloned().and_then(read_boolish).unwrap_or(false);
            make_color_table(lua, song_lua_player_color(player, decorative))
        })?,
    )?;
    globals.set(
        "PlayerScoreColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(player) = args.get(0).and_then(player_index_from_value) else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            make_color_table(lua, song_lua_player_score_color(player))
        })?,
    )?;
    globals.set(
        "PlayerDarkColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(player) = args.get(0).and_then(player_index_from_value) else {
                return make_color_table(lua, [1.0, 1.0, 1.0, 1.0]);
            };
            make_color_table(lua, song_lua_player_dark_color(player))
        })?,
    )?;
    globals.set(
        "DifficultyColor",
        lua.create_function(|lua, args: MultiValue| {
            let Some(difficulty) = args.get(0).cloned().and_then(difficulty_index_from_value)
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
                args.get(0)
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
                args.get(0)
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
                args.get(0)
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
                args.get(0)
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
                args.get(0)
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
                args.get(0)
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
                args.get(0)
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
                args.get(0)
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
            Ok(color_to_hex(
                args.get(0)
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

fn difficulty_index_from_value(value: Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() => Some(value.trunc() as i64),
        Value::String(text) => song_lua_difficulty_index(text.to_str().ok()?.as_ref()),
        _ => None,
    }
}

fn custom_difficulty_color_value(value: Value) -> Option<[f32; 4]> {
    custom_difficulty_color(&read_string(value)?)
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
    stage_color(&read_string(value)?)
}

fn judgment_line_color_value(value: Value) -> Option<[f32; 4]> {
    judgment_line_color(&read_string(value)?)
}
