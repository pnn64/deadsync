use super::*;

pub(super) fn create_song_options_table(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("__songlua_music_rate", music_rate.max(0.0))?;
    table.set(
        "MusicRate",
        lua.create_function(move |_, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(1.0_f32);
            };
            if let Some(rate) = method_arg(&args, 0).cloned().and_then(read_f32) {
                owner.set("__songlua_music_rate", rate.max(0.0))?;
                return Ok(rate.max(0.0));
            }
            Ok(owner
                .get::<Option<f32>>("__songlua_music_rate")?
                .unwrap_or(1.0_f32))
        })?,
    )?;
    Ok(table)
}

pub(super) fn format_song_options_text(music_rate: f32) -> String {
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    format!("{rate}xMusic")
}

pub(super) struct PlayerLuaTables {
    pub(super) player_states: [Table; LUA_PLAYERS],
    pub(super) steps: [Table; LUA_PLAYERS],
}

pub(super) fn create_player_tables(
    lua: &Lua,
    context: &SongLuaCompileContext,
    song_runtime: &Table,
) -> mlua::Result<PlayerLuaTables> {
    let player_states = [
        create_player_state_table(lua, context.players[0].clone(), 0, song_runtime)?,
        create_player_state_table(lua, context.players[1].clone(), 1, song_runtime)?,
    ];
    let steps = [
        create_steps_table(
            lua,
            context.players[0].difficulty,
            context.players[0].display_bpms,
            context.song_dir.as_path(),
        )?,
        create_steps_table(
            lua,
            context.players[1].difficulty,
            context.players[1].display_bpms,
            context.song_dir.as_path(),
        )?,
    ];
    Ok(PlayerLuaTables {
        player_states,
        steps,
    })
}

pub(super) fn create_enabled_players_table(
    lua: &Lua,
    players: [SongLuaPlayerContext; LUA_PLAYERS],
) -> mlua::Result<Table> {
    let enabled = lua.create_table()?;
    let mut next_index = 1;
    for (player_index, player) in players.into_iter().enumerate() {
        if !player.enabled {
            continue;
        }
        enabled.set(next_index, player_number_name(player_index))?;
        next_index += 1;
    }
    Ok(enabled)
}

fn create_player_state_table(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
    song_runtime: &Table,
) -> mlua::Result<Table> {
    let controller = if player.enabled {
        "PlayerController_Human"
    } else {
        "PlayerController_Autoplay"
    };
    let health_state = if player.enabled {
        "HealthState_Alive"
    } else {
        "HealthState_Dead"
    };
    let player_number = player_number_name(player_index);
    let options = create_player_options_table(lua, player)?;
    let song_position = create_song_position_table(lua, song_runtime)?;
    let table = lua.create_table()?;
    table.set("__songlua_player_options_string", String::new())?;
    set_string_method(lua, &table, "GetPlayerController", controller)?;
    set_string_method(lua, &table, "GetHealthState", health_state)?;
    set_string_method(lua, &table, "GetPlayerNumber", player_number)?;
    let options_for_current = options.clone();
    let options_for_set = options.clone();
    table.set(
        "GetPlayerOptions",
        lua.create_function(move |_, _args: MultiValue| Ok(options.clone()))?,
    )?;
    table.set(
        "GetCurrentPlayerOptions",
        lua.create_function(move |_, _args: MultiValue| Ok(options_for_current.clone()))?,
    )?;
    table.set(
        "GetSongPosition",
        lua.create_function(move |_, _args: MultiValue| Ok(song_position.clone()))?,
    )?;
    table.set(
        "GetPlayerOptionsString",
        lua.create_function(|_, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(String::new());
            };
            Ok(owner
                .get::<Option<String>>("__songlua_player_options_string")?
                .unwrap_or_default())
        })?,
    )?;
    table.set(
        "GetPlayerOptionsArray",
        lua.create_function(|lua, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return lua.create_table();
            };
            create_player_options_array(lua, &owner)
        })?,
    )?;
    table.set(
        "SetPlayerOptions",
        lua.create_function({
            move |lua, args: MultiValue| {
                let Some(owner) = args.front().and_then(|value| match value {
                    Value::Table(table) => Some(table.clone()),
                    _ => None,
                }) else {
                    return Ok(());
                };
                let options_text = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                owner.set("__songlua_player_options_string", options_text.clone())?;
                apply_player_options_string(lua, &options_for_set, &options_text)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    Ok(table)
}

fn create_player_options_array(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    let text = owner
        .get::<Option<String>>("__songlua_player_options_string")?
        .unwrap_or_default();
    let table = lua.create_table()?;
    for (index, option) in text
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .enumerate()
    {
        table.raw_set(index + 1, option)?;
    }
    Ok(table)
}

pub(super) fn create_steps_table(
    lua: &Lua,
    difficulty: SongLuaDifficulty,
    display_bpms: [f32; 2],
    song_dir: &Path,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetDifficulty", difficulty.sm_name())?;
    set_string_method(lua, &table, "GetStepsType", "StepsType_Dance_Single")?;
    set_string_method(lua, &table, "GetDescription", "")?;
    set_string_method(lua, &table, "GetChartName", "")?;
    set_string_method(lua, &table, "GetAuthorCredit", "")?;
    set_string_method(lua, &table, "GetCredit", "")?;
    set_string_method(
        lua,
        &table,
        "GetFilename",
        &song_simfile_path(song_dir)
            .map(|path| file_path_string(path.as_path()))
            .unwrap_or_default(),
    )?;
    set_string_method(
        lua,
        &table,
        "GetMusicPath",
        &song_music_path(song_dir)
            .map(|path| file_path_string(path.as_path()))
            .unwrap_or_default(),
    )?;
    let meter = difficulty_meter(difficulty);
    table.set(
        "GetMeter",
        lua.create_function(move |_, _args: MultiValue| Ok(meter))?,
    )?;
    let timing = create_timing_table(lua, display_bpms)?;
    let display_bpms = create_display_bpms_table(lua, display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    table.set(
        "GetTimingData",
        lua.create_function(move |_, _args: MultiValue| Ok(timing.clone()))?,
    )?;
    let radar = create_radar_values_table(lua)?;
    table.set(
        "GetRadarValues",
        lua.create_function(move |_, _args: MultiValue| Ok(radar.clone()))?,
    )?;
    set_string_method(lua, &table, "GetDisplayBPMType", "DISPLAY_BPM_ACTUAL")?;
    Ok(table)
}

pub(super) fn create_display_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0])?;
    table.raw_set(2, bpms[1])?;
    Ok(table)
}

pub(super) fn display_bpms_for_args(
    args: &MultiValue,
    fallback: [f32; 2],
    default_rate: f32,
) -> mlua::Result<([f32; 2], f32)> {
    let rate = args
        .get(2)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(default_rate)
        .max(f32::EPSILON);
    let bpms = args
        .get(1)
        .and_then(display_bpms_from_value)
        .transpose()?
        .unwrap_or(fallback);
    Ok(([bpms[0] * rate, bpms[1] * rate], rate))
}

fn display_bpms_from_value(value: &Value) -> Option<mlua::Result<[f32; 2]>> {
    let Value::Table(table) = value else {
        return None;
    };
    Some(display_bpms_from_table(table))
}

fn display_bpms_from_table(table: &Table) -> mlua::Result<[f32; 2]> {
    if let Some(bpms) = table
        .get::<Option<Function>>("GetDisplayBpms")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(read_bpms_table)
    {
        if bpms[0] > 0.0 && bpms[1] > 0.0 {
            return Ok(bpms);
        }
    }
    let Some(timing) = table
        .get::<Option<Function>>("GetTimingData")?
        .map(|function| call_table_function(table, &function))
        .transpose()?
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
    else {
        return Ok([0.0, 0.0]);
    };
    Ok(timing
        .get::<Option<Function>>("GetActualBPM")?
        .map(|function| call_table_function(&timing, &function))
        .transpose()?
        .and_then(read_bpms_table)
        .unwrap_or([0.0, 0.0]))
}

fn call_table_function(table: &Table, function: &Function) -> mlua::Result<Value> {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    function.call(args)
}

fn read_bpms_table(value: Value) -> Option<[f32; 2]> {
    let Value::Table(table) = value else {
        return None;
    };
    Some([
        table.raw_get::<Value>(1).ok().and_then(read_f32)?,
        table.raw_get::<Value>(2).ok().and_then(read_f32)?,
    ])
}

pub(super) fn display_bpms_text(bpms: [f32; 2], rate: f32) -> String {
    let lower = format_display_bpm(bpms[0], rate);
    if (bpms[0] - bpms[1]).abs() <= f32::EPSILON {
        lower
    } else {
        format!("{lower} - {}", format_display_bpm(bpms[1], rate))
    }
}

fn format_display_bpm(value: f32, rate: f32) -> String {
    let text = if (rate - 1.0).abs() <= f32::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    text.strip_suffix(".0").unwrap_or(&text).to_string()
}

pub(super) fn create_radar_values_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}

pub(super) fn set_string_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    value: &str,
) -> mlua::Result<()> {
    let value = value.to_string();
    table.set(
        name,
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&value)?))
        })?,
    )
}

pub(super) fn set_path_methods(
    lua: &Lua,
    table: &Table,
    path_name: &str,
    has_name: &str,
    path: Option<&Path>,
) -> mlua::Result<()> {
    let path = path.map(file_path_string).unwrap_or_default();
    set_string_method(lua, table, path_name, &path)?;
    let has_file = !path.is_empty();
    table.set(
        has_name,
        lua.create_function(move |_, _args: MultiValue| Ok(has_file))?,
    )?;
    Ok(())
}

#[inline(always)]
fn difficulty_meter(difficulty: SongLuaDifficulty) -> i32 {
    match difficulty {
        SongLuaDifficulty::Beginner => 1,
        SongLuaDifficulty::Easy => 4,
        SongLuaDifficulty::Medium => 7,
        SongLuaDifficulty::Hard => 10,
        SongLuaDifficulty::Challenge => 12,
        SongLuaDifficulty::Edit => 0,
    }
}

fn create_player_options_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    install_speedmod_method(lua, &table, "CMod", player.speedmod, SongLuaSpeedMod::C)?;
    install_speedmod_state_method(lua, &table, "CAMod", Value::Nil)?;
    install_speedmod_method(lua, &table, "MMod", player.speedmod, SongLuaSpeedMod::M)?;
    install_speedmod_method(lua, &table, "AMod", player.speedmod, SongLuaSpeedMod::A)?;
    install_speedmod_method(lua, &table, "XMod", player.speedmod, SongLuaSpeedMod::X)?;
    table.set(
        "FromString",
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                if let Some(text) = method_arg(&args, 0).cloned().and_then(read_string) {
                    apply_player_options_string(lua, &table, &text)?;
                }
                Ok(table.clone())
            }
        })?,
    )?;
    for name in ["Mirror", "Left", "Right", "Reverse", "Mini", "Skew", "Tilt"] {
        table.set(name, create_player_option_method(lua, &table, name)?)?;
    }
    for name in ["IsEasierForSongAndSteps", "IsEasierForCourseAndTrail"] {
        table.set(name, lua.create_function(|_, _args: MultiValue| Ok(false))?)?;
    }
    table.set(
        "GetReversePercentForColumn",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| player_option_number(lua, &table, "reverse")
        })?,
    )?;
    table.set(
        "UsingReverse",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| Ok(player_option_number(lua, &table, "reverse")? == 1.0)
        })?,
    )?;
    table.set(
        "GetStepAttacks",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let enabled = player_option_number(lua, &table, "noattack")? <= 0.0
                    && player_option_number(lua, &table, "randattack")? <= 0.0;
                Ok(i64::from(enabled))
            }
        })?,
    )?;
    table.set(
        "DisableTimingWindow",
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                if let Some(window) = method_arg(&args, 0).cloned().and_then(timing_window_name) {
                    disabled_timing_windows(lua, &table)?.set(window, true)?;
                    note_song_lua_side_effect(lua)?;
                }
                Ok(table.clone())
            }
        })?,
    )?;
    table.set(
        "ResetDisabledTimingWindows",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                table.raw_set("__songlua_disabled_timing_windows", lua.create_table()?)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )?;
    table.set(
        "GetDisabledTimingWindows",
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let disabled = disabled_timing_windows(lua, &table)?;
                let out = lua.create_table()?;
                let mut index = 1_i64;
                for window in SONG_LUA_TIMING_WINDOW_NAMES {
                    if disabled.get::<Option<bool>>(window)?.unwrap_or(false) {
                        out.raw_set(index, window)?;
                        index += 1;
                    }
                }
                Ok(out)
            }
        })?,
    )?;
    table.set("__songlua_noteskin_name", player.noteskin_name.clone())?;
    table.set(
        "NoteSkin",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(owner) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            if let Some(noteskin_name) = method_arg(&args, 0).cloned().and_then(read_string) {
                owner.raw_set("__songlua_noteskin_name", noteskin_name)?;
                return Ok(Value::Table(owner));
            }
            let noteskin_name = owner
                .raw_get::<Option<String>>("__songlua_noteskin_name")?
                .unwrap_or_else(|| player.noteskin_name.clone());
            Ok(Value::String(lua.create_string(&noteskin_name)?))
        })?,
    )?;
    let mt = lua.create_table()?;
    let fallback_owner = table.clone();
    mt.set(
        "__index",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(Value::Function(create_player_option_method(
                lua,
                &fallback_owner,
                &name,
            )?))
        })?,
    )?;
    let _ = table.set_metatable(Some(mt));
    Ok(table)
}

fn disabled_timing_windows(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    if let Some(table) = owner.raw_get::<Option<Table>>("__songlua_disabled_timing_windows")? {
        return Ok(table);
    }
    let table = lua.create_table()?;
    owner.raw_set("__songlua_disabled_timing_windows", table.clone())?;
    Ok(table)
}

fn timing_window_name(value: Value) -> Option<&'static str> {
    let index = match value {
        Value::Integer(value) => i32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() => Some(value.round() as i32),
        Value::String(text) => text
            .to_str()
            .ok()?
            .chars()
            .rev()
            .find(|ch| ch.is_ascii_digit())
            .and_then(|ch| ch.to_digit(10))
            .map(|value| value as i32),
        _ => None,
    }?;
    (1..=5)
        .contains(&index)
        .then_some(SONG_LUA_TIMING_WINDOW_NAMES[index as usize - 1])
}

fn player_option_number(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<f32> {
    Ok(player_option_state(lua, owner)?
        .get::<Option<f32>>(name)?
        .unwrap_or(0.0))
}

fn create_player_option_method(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<Function> {
    let owner = owner.clone();
    let name = name.to_ascii_lowercase();
    lua.create_function(move |lua, args: MultiValue| {
        let state = player_option_state(lua, &owner)?;
        if let Some(value) = method_arg(&args, 0).cloned() {
            state.set(
                name.as_str(),
                normalize_player_option_value(lua, &name, value)?,
            )?;
            return Ok(Value::Table(owner.clone()));
        }
        Ok(match state.get::<Option<Value>>(name.as_str())? {
            Some(value) => value,
            None => default_player_option_value(lua, &name)?,
        })
    })
}

fn player_option_state(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    if let Some(state) = owner.raw_get::<Option<Table>>("__songlua_player_option_state")? {
        return Ok(state);
    }
    let state = lua.create_table()?;
    owner.raw_set("__songlua_player_option_state", state.clone())?;
    Ok(state)
}

fn apply_player_options_string(lua: &Lua, owner: &Table, text: &str) -> mlua::Result<()> {
    for option in text.split(',') {
        apply_player_option_token(lua, owner, option)?;
    }
    Ok(())
}

fn apply_player_option_token(lua: &Lua, owner: &Table, raw: &str) -> mlua::Result<()> {
    let text = strip_player_option_prefix(raw);
    if text.is_empty() || apply_player_speed_option(owner, text)? {
        return Ok(());
    }

    let (head, tail) = split_first_word(text);
    let (amount, name) = if !tail.is_empty() {
        parse_player_option_amount(head).map_or((None, text), |amount| (Some(amount), tail))
    } else {
        (None, text)
    };
    let key = normalize_player_option_key(name);
    if key.is_empty() {
        return Ok(());
    }

    let state = player_option_state(lua, owner)?;
    let value = if player_option_uses_bool(key.as_str()) {
        Value::Boolean(amount.map_or(true, |amount| amount != 0.0))
    } else {
        Value::Number(amount.unwrap_or(1.0) as f64)
    };
    state.set(key.as_str(), value)
}

fn strip_player_option_prefix(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        let Some(rest) = trimmed.strip_prefix('*') else {
            return trimmed;
        };
        let prefix_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if prefix_len == 0 {
            return trimmed;
        }
        text = &rest[prefix_len..];
    }
}

fn split_first_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.find(char::is_whitespace) {
        Some(index) => (&text[..index], text[index..].trim_start()),
        None => (text, ""),
    }
}

fn parse_player_option_amount(text: &str) -> Option<f32> {
    let text = text.trim();
    let percent = text.ends_with('%');
    let raw = text.trim_end_matches('%');
    let value = raw.parse::<f32>().ok()?;
    Some(if percent { value / 100.0 } else { value })
}

fn normalize_player_option_key(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn apply_player_speed_option(owner: &Table, text: &str) -> mlua::Result<bool> {
    let compact: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    if let Some(value) = compact
        .strip_suffix('x')
        .and_then(|raw| raw.parse::<f32>().ok())
    {
        set_player_speedmod(owner, "xmod", Some(value))?;
        return Ok(true);
    }
    for (prefix, key) in [("ca", "camod"), ("c", "cmod"), ("m", "mmod"), ("a", "amod")] {
        if let Some(value) = compact
            .strip_prefix(prefix)
            .and_then(|raw| raw.parse::<f32>().ok())
            .or_else(|| {
                compact
                    .strip_suffix(prefix)
                    .and_then(|raw| raw.parse::<f32>().ok())
            })
        {
            set_player_speedmod(owner, key, Some(value))?;
            return Ok(true);
        }
    }
    Ok(false)
}

#[inline(always)]
fn normalize_player_option_value(lua: &Lua, name: &str, value: Value) -> mlua::Result<Value> {
    if player_option_uses_bool(name) {
        return Ok(Value::Boolean(read_boolish(value).unwrap_or(false)));
    }
    if player_option_default_string(name).is_some() {
        return Ok(match value {
            Value::String(_) => value,
            _ => default_player_option_value(lua, name)?,
        });
    }
    Ok(Value::Number(read_f32(value).unwrap_or(0.0) as f64))
}

#[inline(always)]
fn default_player_option_value(lua: &Lua, name: &str) -> mlua::Result<Value> {
    if player_option_uses_bool(name) {
        return Ok(Value::Boolean(false));
    }
    if let Some(value) = player_option_default_string(name) {
        return Ok(Value::String(lua.create_string(value)?));
    }
    Ok(Value::Number(0.0))
}

#[inline(always)]
fn player_option_default_string(name: &str) -> Option<&'static str> {
    Some(match name {
        "drainsetting" => "DrainType_Normal",
        "failsetting" => "FailType_Immediate",
        "hidelightsetting" => "HideLightType_NoHideLights",
        "lifesetting" => "LifeType_Bar",
        "mintnstohidenotes" => "TapNoteScore_None",
        "modtimersetting" => "ModTimerType_Default",
        _ => return None,
    })
}

#[inline(always)]
fn player_option_uses_bool(name: &str) -> bool {
    matches!(
        name,
        "attackmines"
            | "backwards"
            | "big"
            | "bmrize"
            | "cosecant"
            | "dizzyholds"
            | "echo"
            | "floored"
            | "holdrolls"
            | "hypershuffle"
            | "left"
            | "little"
            | "lrmirror"
            | "mirror"
            | "mines"
            | "muteonerror"
            | "nohands"
            | "noholds"
            | "nojumps"
            | "nolifts"
            | "nomines"
            | "noquads"
            | "norolls"
            | "nostretch"
            | "nofakes"
            | "overhead"
            | "planted"
            | "quick"
            | "right"
            | "shuffle"
            | "skippy"
            | "softshuffle"
            | "stealthpastreceptors"
            | "stealthtype"
            | "stomp"
            | "supershuffle"
            | "turnnone"
            | "twister"
            | "udmirror"
            | "wide"
            | "zbuffer"
    )
}

fn install_speedmod_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    speedmod: SongLuaSpeedMod,
    ctor: fn(f32) -> SongLuaSpeedMod,
) -> mlua::Result<()> {
    let initial = speedmod_value(speedmod, ctor);
    install_speedmod_state_method(lua, table, name, initial)
}

fn install_speedmod_state_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    initial: Value,
) -> mlua::Result<()> {
    let owner = table.clone();
    let key = name.to_ascii_lowercase();
    let value_key = format!("__songlua_speedmod_{key}");
    table.set(
        name,
        lua.create_function(move |_, args: MultiValue| {
            if let Some(value) = method_arg(&args, 0).cloned() {
                if matches!(value, Value::Nil) {
                    set_player_speedmod(&owner, key.as_str(), None)?;
                } else if let Some(value) = read_f32(value) {
                    set_player_speedmod(&owner, key.as_str(), Some(value))?;
                }
                return Ok(Value::Table(owner.clone()));
            }

            let active = owner.raw_get::<Option<String>>("__songlua_speedmod_active")?;
            if active
                .as_deref()
                .is_some_and(|active| active != key.as_str())
            {
                return Ok(Value::Nil);
            }
            if active.as_deref() == Some(key.as_str()) {
                return Ok(owner
                    .raw_get::<Option<f32>>(value_key.as_str())?
                    .map_or(Value::Nil, |value| Value::Number(value as f64)));
            }
            Ok(initial.clone())
        })?,
    )
}

fn set_player_speedmod(owner: &Table, key: &str, value: Option<f32>) -> mlua::Result<()> {
    let value_key = format!("__songlua_speedmod_{key}");
    if let Some(value) = value {
        owner.raw_set("__songlua_speedmod_active", key)?;
        owner.raw_set(value_key.as_str(), value)?;
    } else {
        owner.raw_set("__songlua_speedmod_active", "none")?;
        owner.raw_set(value_key.as_str(), Value::Nil)?;
    }
    Ok(())
}

fn speedmod_value(speedmod: SongLuaSpeedMod, ctor: fn(f32) -> SongLuaSpeedMod) -> Value {
    match (speedmod, ctor(0.0)) {
        (SongLuaSpeedMod::X(value), SongLuaSpeedMod::X(_))
        | (SongLuaSpeedMod::C(value), SongLuaSpeedMod::C(_))
        | (SongLuaSpeedMod::M(value), SongLuaSpeedMod::M(_))
        | (SongLuaSpeedMod::A(value), SongLuaSpeedMod::A(_)) => Value::Number(value as f64),
        _ => Value::Nil,
    }
}

pub(super) fn create_song_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let song_dir = song_dir_string(context.song_dir.as_path());
    let steps_by_type = create_steps_by_steps_type_table(
        lua,
        context.song_display_bpms,
        context.song_dir.as_path(),
    )?;
    set_string_method(lua, &table, "GetSongDir", &song_dir)?;
    set_string_method(lua, &table, "GetMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplayMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplayFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplaySubTitle", "")?;
    set_string_method(lua, &table, "GetTranslitSubTitle", "")?;
    set_string_method(lua, &table, "GetDisplayArtist", "")?;
    set_string_method(lua, &table, "GetTranslitArtist", "")?;
    set_string_method(
        lua,
        &table,
        "GetGroupName",
        &song_group_name(&context.song_dir),
    )?;
    let music_path = song_music_path(&context.song_dir);
    let banner_path = song_named_image_path(&context.song_dir, &["banner", "bn"]);
    let background_path = song_named_image_path(&context.song_dir, &["background", "bg"]);
    let jacket_path = song_named_image_path(&context.song_dir, &["jacket", "cover"]);
    let cd_image_path = song_named_image_path(&context.song_dir, &["cdtitle", "cdimage", "disc"]);
    set_path_methods(
        lua,
        &table,
        "GetMusicPath",
        "HasMusic",
        music_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetBannerPath",
        "HasBanner",
        banner_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetBackgroundPath",
        "HasBackground",
        background_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetJacketPath",
        "HasJacket",
        jacket_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetCDImagePath",
        "HasCDImage",
        cd_image_path.as_deref(),
    )?;
    let music_length_seconds = context.music_length_seconds.max(0.0);
    table.set(
        "MusicLengthSeconds",
        lua.create_function(move |_, _args: MultiValue| Ok(music_length_seconds))?,
    )?;
    table.set(
        "GetFirstSecond",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetLastSecond",
        lua.create_function(move |_, _args: MultiValue| Ok(music_length_seconds))?,
    )?;
    table.set(
        "GetFirstBeat",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    let last_beat = music_length_seconds * context.song_display_bpms[1].max(0.0) / 60.0;
    table.set(
        "GetLastBeat",
        lua.create_function(move |_, _args: MultiValue| Ok(last_beat))?,
    )?;
    set_string_method(lua, &table, "GetOrTryAtLeastToGetSimfileAuthor", "")?;
    table.set(
        "GetStageCost",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    let display_bpms = create_display_bpms_table(lua, context.song_display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    let timing = create_timing_table(lua, context.song_display_bpms)?;
    table.set(
        "GetTimingData",
        lua.create_function(move |_, _args: MultiValue| Ok(timing.clone()))?,
    )?;
    let all_steps = steps_by_type.clone();
    table.set(
        "GetAllSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(all_steps.clone()))?,
    )?;
    table.set(
        "HasStepsType",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single))
        })?,
    )?;
    table.set(
        "HasStepsTypeAndDifficulty",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
                && method_arg(&args, 1)
                    .cloned()
                    .and_then(song_lua_difficulty_from_value)
                    .is_some())
        })?,
    )?;
    table.set(
        "HasEdits",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single))
        })?,
    )?;
    let one_steps = steps_by_type.clone();
    table.set(
        "GetOneSteps",
        lua.create_function(move |_, args: MultiValue| {
            if !method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
            {
                return Ok(Value::Nil);
            }
            let Some(difficulty) = method_arg(&args, 1)
                .cloned()
                .and_then(song_lua_difficulty_from_value)
            else {
                return Ok(Value::Nil);
            };
            Ok(one_steps
                .raw_get::<Option<Table>>(usize::from(difficulty.sort_key()) + 1)?
                .map(Value::Table)
                .unwrap_or(Value::Nil))
        })?,
    )?;
    let steps_by_type_for_get = steps_by_type.clone();
    table.set(
        "GetStepsByStepsType",
        lua.create_function(move |lua, args: MultiValue| {
            if !method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
            {
                return Ok(lua.create_table()?);
            }
            Ok(steps_by_type_for_get.clone())
        })?,
    )?;
    Ok(table)
}

pub(super) fn create_course_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    song: Table,
    trail: Table,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let course_dir = context.song_dir.join("compat-course.crs");
    let course_dir = file_path_string(course_dir.as_path());
    set_string_method(lua, &table, "GetDisplayMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDisplayFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitMainTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetTranslitFullTitle", &context.main_title)?;
    set_string_method(lua, &table, "GetDescription", "")?;
    set_string_method(lua, &table, "GetScripter", "")?;
    set_string_method(lua, &table, "GetCourseDir", &course_dir)?;
    set_string_method(lua, &table, "GetCourseType", "CourseType_Nonstop")?;
    set_string_method(lua, &table, "GetDifficulty", "Difficulty_Medium")?;
    let background_path = song_named_image_path(&context.song_dir, &["background", "bg"]);
    let banner_path = song_named_image_path(&context.song_dir, &["banner", "bn"]);
    set_path_methods(
        lua,
        &table,
        "GetBannerPath",
        "HasBanner",
        banner_path.as_deref(),
    )?;
    set_path_methods(
        lua,
        &table,
        "GetBackgroundPath",
        "HasBackground",
        background_path.as_deref(),
    )?;
    let entries = create_single_value_array(
        lua,
        create_trail_entry_table(lua, song, trail.raw_get::<Table>("__songlua_steps")?)?,
    )?;
    let all_trails = create_single_value_array(lua, trail.clone())?;
    table.set(
        "GetCourseEntries",
        lua.create_function(move |_, _args: MultiValue| Ok(entries.clone()))?,
    )?;
    table.set(
        "GetAllTrails",
        lua.create_function(move |_, _args: MultiValue| Ok(all_trails.clone()))?,
    )?;
    table.set(
        "GetTrail",
        lua.create_function(move |_, _args: MultiValue| Ok(trail.clone()))?,
    )?;
    table.set(
        "GetEstimatedNumStages",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    for (method, value) in [
        ("AllSongsAreFixed", true),
        ("IsAutogen", false),
        ("IsEndless", false),
        ("IsPlayable", true),
    ] {
        table.set(
            method,
            lua.create_function(move |_, _args: MultiValue| Ok(value))?,
        )?;
    }
    Ok(table)
}

pub(super) fn create_trail_table(
    lua: &Lua,
    song: Table,
    steps: Table,
    display_bpms: [f32; 2],
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("__songlua_steps", steps.clone())?;
    let entry = create_trail_entry_table(lua, song, steps)?;
    let entries = create_single_value_array(lua, entry.clone())?;
    table.set(
        "GetTrailEntries",
        lua.create_function({
            let entries = entries.clone();
            move |_, _args: MultiValue| Ok(entries.clone())
        })?,
    )?;
    table.set(
        "GetTrailEntry",
        lua.create_function(move |_, args: MultiValue| {
            let index = method_arg(&args, 0)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(0);
            let index = usize::try_from(index.max(0)).unwrap_or(0) + 1;
            Ok(entries
                .raw_get::<Option<Table>>(index)?
                .unwrap_or_else(|| entry.clone()))
        })?,
    )?;
    set_string_method(lua, &table, "GetStepsType", "StepsType_Dance_Single")?;
    set_string_method(lua, &table, "GetDifficulty", "Difficulty_Medium")?;
    table.set(
        "GetMeter",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    let display_bpms = create_display_bpms_table(lua, display_bpms)?;
    table.set(
        "GetDisplayBpms",
        lua.create_function(move |_, _args: MultiValue| Ok(display_bpms.clone()))?,
    )?;
    table.set(
        "GetRadarValues",
        lua.create_function(|lua, _args: MultiValue| create_radar_values_table(lua))?,
    )?;
    Ok(table)
}

fn create_trail_entry_table(lua: &Lua, song: Table, steps: Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetSong",
        lua.create_function(move |_, _args: MultiValue| Ok(song.clone()))?,
    )?;
    table.set(
        "GetSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(steps.clone()))?,
    )?;
    set_string_method(lua, &table, "GetCourseEntryType", "CourseEntryType_Fixed")?;
    set_string_method(lua, &table, "GetNormalModifiers", "")?;
    set_string_method(lua, &table, "GetAttackModifiers", "")?;
    table.set(
        "IsSecret",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(table)
}

pub(super) fn create_songman_table(
    lua: &Lua,
    current_song: Table,
    current_steps: Table,
    current_course: Table,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
    let songman = lua.create_table()?;
    let current_group = song_group_name(&context.song_dir);
    let current_title = context.main_title.clone();
    let current_dir = song_dir_string(context.song_dir.as_path());
    let current_course_dir = file_path_string(context.song_dir.join("compat-course.crs").as_path());
    let all_songs = create_single_value_array(lua, current_song.clone())?;
    let all_courses = create_single_value_array(lua, current_course.clone())?;
    let groups = create_string_array(lua, &[current_group.as_str()])?;
    let course_groups = create_string_array(lua, &[current_group.as_str()])?;
    let group_songs = create_single_value_array(lua, current_song.clone())?;
    let group_courses = create_single_value_array(lua, current_course.clone())?;
    let group = create_song_group_table(lua)?;
    let group_banner_path = song_named_image_path(&context.song_dir, &["banner", "bn"])
        .as_deref()
        .map(file_path_string)
        .unwrap_or_default();

    songman.set(
        "GetSongFromSteps",
        lua.create_function({
            let current_song = current_song.clone();
            move |_, _args: MultiValue| Ok(current_song.clone())
        })?,
    )?;
    songman.set(
        "FindSong",
        lua.create_function({
            let current_song = current_song.clone();
            let current_dir = current_dir.clone();
            let current_group = current_group.clone();
            let current_title = current_title.clone();
            move |_, args: MultiValue| {
                let Some(query) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                if song_lookup_matches(&query, &current_dir, &current_group, &current_title) {
                    Ok(Value::Table(current_song.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }
        })?,
    )?;
    songman.set(
        "FindCourse",
        lua.create_function({
            let current_course = current_course.clone();
            let current_course_dir = current_course_dir.clone();
            let current_dir = current_dir.clone();
            let current_group = current_group.clone();
            let current_title = current_title.clone();
            move |_, args: MultiValue| {
                let Some(query) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                if song_lookup_matches(&query, &current_course_dir, &current_group, &current_title)
                    || song_lookup_matches(&query, &current_dir, &current_group, &current_title)
                {
                    Ok(Value::Table(current_course.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }
        })?,
    )?;
    songman.set(
        "GetRandomSong",
        lua.create_function({
            let current_song = current_song.clone();
            move |_, _args: MultiValue| Ok(current_song.clone())
        })?,
    )?;
    songman.set(
        "GetRandomCourse",
        lua.create_function({
            let current_course = current_course.clone();
            move |_, _args: MultiValue| Ok(current_course.clone())
        })?,
    )?;
    songman.set(
        "GetAllSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(all_songs.clone()))?,
    )?;
    songman.set(
        "GetAllCourses",
        lua.create_function(move |_, _args: MultiValue| Ok(all_courses.clone()))?,
    )?;
    songman.set(
        "GetSongGroupNames",
        lua.create_function(move |_, _args: MultiValue| Ok(groups.clone()))?,
    )?;
    songman.set(
        "GetCourseGroupNames",
        lua.create_function(move |_, _args: MultiValue| Ok(course_groups.clone()))?,
    )?;
    songman.set(
        "GetSongsInGroup",
        lua.create_function({
            let current_group = current_group.clone();
            let group_songs = group_songs.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                if group == current_group {
                    Ok(group_songs.clone())
                } else {
                    lua.create_table()
                }
            }
        })?,
    )?;
    songman.set(
        "GetCoursesInGroup",
        lua.create_function({
            let current_group = current_group.clone();
            let group_courses = group_courses.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                if group == current_group {
                    Ok(group_courses.clone())
                } else {
                    lua.create_table()
                }
            }
        })?,
    )?;
    for method in ["DoesSongGroupExist", "DoesCourseGroupExist"] {
        songman.set(
            method,
            lua.create_function({
                let current_group = current_group.clone();
                move |_, args: MultiValue| {
                    Ok(method_arg(&args, 0)
                        .cloned()
                        .and_then(read_string)
                        .is_some_and(|group| group == current_group))
                }
            })?,
        )?;
    }
    songman.set(
        "GetExtraStageInfo",
        lua.create_function({
            let current_song = current_song.clone();
            let current_steps = current_steps.clone();
            move |_, _args: MultiValue| Ok((current_song.clone(), current_steps.clone()))
        })?,
    )?;
    for method in ["GetSongColor", "GetSongGroupColor", "GetCourseColor"] {
        songman.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                make_color_table(lua, [1.0, 1.0, 1.0, 1.0])
            })?,
        )?;
    }
    songman.set(
        "GetSongRank",
        lua.create_function(|_, args: MultiValue| {
            if matches!(method_arg(&args, 0), Some(Value::Table(_))) {
                Ok(Value::Integer(1))
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    songman.set(
        "ShortenGroupName",
        lua.create_function(|lua, args: MultiValue| {
            let group = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(lua.create_string(&group)?))
        })?,
    )?;
    for method in ["GetSongGroupBannerPath", "GetCourseGroupBannerPath"] {
        songman.set(
            method,
            lua.create_function({
                let current_group = current_group.clone();
                let group_banner_path = group_banner_path.clone();
                move |lua, args: MultiValue| {
                    let group = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_string)
                        .unwrap_or_default();
                    let path = if group == current_group {
                        group_banner_path.as_str()
                    } else {
                        ""
                    };
                    Ok(Value::String(lua.create_string(path)?))
                }
            })?,
        )?;
    }
    songman.set(
        "SongToPreferredSortSectionName",
        lua.create_function({
            let current_group = current_group.clone();
            move |_, args: MultiValue| {
                if matches!(method_arg(&args, 0), Some(Value::Table(_))) {
                    Ok(current_group.clone())
                } else {
                    Ok(String::new())
                }
            }
        })?,
    )?;
    songman.set(
        "GetPreferredSortSongsBySectionName",
        lua.create_function({
            let current_group = current_group.clone();
            let group_songs = group_songs.clone();
            move |lua, args: MultiValue| {
                let section = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                if section == current_group {
                    Ok(group_songs.clone())
                } else {
                    lua.create_table()
                }
            }
        })?,
    )?;
    songman.set(
        "GetGroup",
        lua.create_function(move |_, _args: MultiValue| Ok(group.clone()))?,
    )?;
    for (method, value) in [
        ("GetNumSongs", 1_i64),
        ("GetNumLockedSongs", 0),
        ("GetNumUnlockedSongs", 1),
        ("GetNumSelectableAndUnlockedSongs", 1),
        ("GetNumAdditionalSongs", 0),
        ("GetNumSongGroups", 1),
        ("GetNumCourses", 1),
        ("GetNumAdditionalCourses", 0),
        ("GetNumCourseGroups", 1),
    ] {
        songman.set(
            method,
            lua.create_function(move |_, _args: MultiValue| Ok(value))?,
        )?;
    }
    for method in ["SetPreferredSongs", "SetPreferredCourses"] {
        songman.set(
            method,
            lua.create_function({
                let songman = songman.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(songman.clone())
                }
            })?,
        )?;
    }
    let preferred_sort_songs = group_songs.clone();
    songman.set(
        "GetPreferredSortSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(preferred_sort_songs.clone()))?,
    )?;
    let preferred_sort_courses = group_courses.clone();
    songman.set(
        "GetPreferredSortCourses",
        lua.create_function(move |_, _args: MultiValue| Ok(preferred_sort_courses.clone()))?,
    )?;
    songman.set(
        "GetPopularSongs",
        lua.create_function(move |_, _args: MultiValue| Ok(group_songs.clone()))?,
    )?;
    let popular_courses = group_courses.clone();
    songman.set(
        "GetPopularCourses",
        lua.create_function(move |_, _args: MultiValue| Ok(popular_courses.clone()))?,
    )?;
    for method in [
        "WasLoadedFromAdditionalSongs",
        "WasLoadedFromAdditionalCourses",
    ] {
        songman.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(false))?,
        )?;
    }
    Ok(songman)
}

fn create_song_group_table(lua: &Lua) -> mlua::Result<Table> {
    let group = lua.create_table()?;
    group.set(
        "GetSyncOffset",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(group)
}

fn create_single_value_array(lua: &Lua, value: Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, value)?;
    Ok(table)
}

fn song_lookup_matches(query: &str, song_dir: &str, group: &str, title: &str) -> bool {
    let query = query.trim().replace('\\', "/");
    !query.is_empty()
        && (query == song_dir
            || song_dir.contains(query.as_str())
            || query.eq_ignore_ascii_case(group)
            || query.eq_ignore_ascii_case(title))
}

fn create_steps_by_steps_type_table(
    lua: &Lua,
    display_bpms: [f32; 2],
    song_dir: &Path,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (idx, difficulty) in [
        SongLuaDifficulty::Beginner,
        SongLuaDifficulty::Easy,
        SongLuaDifficulty::Medium,
        SongLuaDifficulty::Hard,
        SongLuaDifficulty::Challenge,
        SongLuaDifficulty::Edit,
    ]
    .into_iter()
    .enumerate()
    {
        table.raw_set(
            idx + 1,
            create_steps_table(lua, difficulty, display_bpms, song_dir)?,
        )?;
    }
    Ok(table)
}

fn create_timing_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let actual_bpms = create_display_bpms_table(lua, bpms)?;
    table.set(
        "GetActualBPM",
        lua.create_function(move |_, _args: MultiValue| Ok(actual_bpms.clone()))?,
    )?;
    let timing_bpms = create_timing_bpms_table(lua, bpms)?;
    table.set(
        "GetBPMs",
        lua.create_function(move |_, _args: MultiValue| Ok(timing_bpms.clone()))?,
    )?;
    let has_bpm_changes = (bpms[0] - bpms[1]).abs() > f32::EPSILON;
    table.set(
        "HasBPMChanges",
        lua.create_function(move |_, _args: MultiValue| Ok(has_bpm_changes))?,
    )?;
    let bpm_at_beat = bpms[0].max(0.0);
    table.set(
        "GetBPMAtBeat",
        lua.create_function(move |_, _args: MultiValue| Ok(bpm_at_beat))?,
    )?;
    table.set(
        "GetElapsedTimeFromBeat",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0))
        })?,
    )?;
    table.set(
        "GetBeatFromElapsedTime",
        lua.create_function(|_, args: MultiValue| {
            Ok(method_arg(&args, 0)
                .cloned()
                .and_then(read_f32)
                .unwrap_or(0.0))
        })?,
    )?;
    Ok(table)
}

fn create_timing_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0].max(0.0))?;
    if (bpms[0] - bpms[1]).abs() > f32::EPSILON {
        table.raw_set(2, bpms[1].max(0.0))?;
    }
    Ok(table)
}

pub(super) fn create_style_table(lua: &Lua, style_name: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let style = song_lua_style_info(style_name);
    let style_name_for_get = style.name.to_string();
    table.set(
        "GetName",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&style_name_for_get)?))
        })?,
    )?;
    set_string_method(lua, &table, "GetStepsType", style.steps_type)?;
    set_string_method(lua, &table, "GetStyleType", style.style_type)?;
    table.set(
        "ColumnsPerPlayer",
        lua.create_function(move |_, _args: MultiValue| Ok(style.columns as i64))?,
    )?;
    table.set(
        "GetWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(style.width))?,
    )?;
    table.set(
        "GetColumnInfo",
        lua.create_function(move |lua, args: MultiValue| {
            let index = method_arg(&args, 1)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(1)
                .clamp(1, style.columns as i32) as usize
                - 1;
            let info = lua.create_table()?;
            info.set("Name", song_lua_style_column_name(index))?;
            info.set("Track", index as i64)?;
            info.set("XOffset", song_lua_style_column_x(style.name, index))?;
            Ok(info)
        })?,
    )?;
    Ok(table)
}
