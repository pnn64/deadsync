use std::path::Path;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::*;

pub fn create_song_options_table(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("__songlua_music_rate", music_rate.max(0.0))?;
    table.set(
        "MusicRate",
        lua.create_function(move |_, mut args: crate::method_args::MethodArgs<2>| {
            if !matches!(args.front(), Some(Value::Table(_))) {
                return Ok(1.0_f32);
            }
            let rate = args.take_method_arg(0).and_then(read_f32);
            let Some(Value::Table(owner)) = args.front() else {
                return Ok(1.0_f32);
            };
            if let Some(rate) = rate {
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

pub struct PlayerLuaTables {
    pub player_states: [Table; LUA_PLAYERS],
    pub player_options: [Table; LUA_PLAYERS],
    pub steps: [Table; LUA_PLAYERS],
}

pub fn create_player_tables(
    lua: &Lua,
    context: &SongLuaCompileContext,
    song_runtime: &Table,
) -> mlua::Result<PlayerLuaTables> {
    let players = [
        create_player_state_table(lua, context.players[0].clone(), 0, song_runtime)?,
        create_player_state_table(lua, context.players[1].clone(), 1, song_runtime)?,
    ];
    let steps = [
        create_steps_table(
            lua,
            context.players[0].difficulty,
            context.players[0].display_bpms,
            context.song_dir.as_path(),
            song_lua_style_info(&context.style_name).steps_type,
        )?,
        create_steps_table(
            lua,
            context.players[1].difficulty,
            context.players[1].display_bpms,
            context.song_dir.as_path(),
            song_lua_style_info(&context.style_name).steps_type,
        )?,
    ];
    Ok(PlayerLuaTables {
        player_states: [players[0].0.clone(), players[1].0.clone()],
        player_options: [players[0].1.clone(), players[1].1.clone()],
        steps,
    })
}

pub fn create_enabled_players_table(
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
) -> mlua::Result<(Table, Table)> {
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
    set_string_method(lua, &table, "GetPlayerController", controller)?;
    set_string_method(lua, &table, "GetHealthState", health_state)?;
    set_string_method(lua, &table, "GetPlayerNumber", player_number)?;
    let options_for_get = options.clone();
    let options_for_current = options.clone();
    let options_for_set = options.clone();
    let options_for_string = options.clone();
    let options_for_array = options.clone();
    table.set(
        "GetPlayerOptions",
        lua.create_function(move |_, _self: Option<Value>| Ok(options_for_get.clone()))?,
    )?;
    table.set(
        "GetCurrentPlayerOptions",
        lua.create_function(move |_, _self: Option<Value>| Ok(options_for_current.clone()))?,
    )?;
    table.set(
        "GetSongPosition",
        lua.create_function(move |_, _args: MultiValue| Ok(song_position.clone()))?,
    )?;
    table.set(
        "GetPlayerOptionsString",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(player_options_parts(lua, &options_for_string)?.join(", "))
        })?,
    )?;
    table.set(
        "GetPlayerOptionsArray",
        lua.create_function(move |lua, _args: MultiValue| {
            lua.create_sequence_from(player_options_parts(lua, &options_for_array)?)
        })?,
    )?;
    table.set(
        "SetPlayerOptions",
        lua.create_function({
            move |lua, args: MultiValue| {
                let Some(Value::Table(_)) = args.front() else {
                    return Ok(());
                };
                let options_text = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                // LunaPlayerState::SetPlayerOptions parses a fresh PlayerOptions
                // and assigns it. Keep the table identity held by Lua readers,
                // but reset prior targets and approach speeds before parsing.
                #[cfg(feature = "test-support")]
                let previous = player_noteskin(&options_for_set)?;
                reset_player_options(lua, &options_for_set)?;
                apply_player_options_string(lua, &options_for_set, &options_text)?;
                #[cfg(feature = "test-support")]
                capture_skin_write(lua, &options_for_set, "setplayeroptions", previous)?;
                note_song_lua_side_effect(lua)?;
                Ok(())
            }
        })?,
    )?;
    Ok((table, options))
}

fn option_string_part(parts: &mut Vec<String>, name: &str, value: f32) {
    if value != 0.0 && value.is_finite() {
        parts.push(if value == 1.0 {
            name.to_string()
        } else {
            // Native AddPart uses lrint on a float percentage, including ties
            // to even. Round-tripping the string intentionally quantizes it.
            format!("{:.0}% {name}", (value * 100.0).round_ties_even())
        });
    }
}

fn player_options_parts(lua: &Lua, owner: &Table) -> mlua::Result<Vec<String>> {
    let state = player_option_state(lua, owner)?;
    let mut parts = vec!["NoHideLights".to_string()];
    if let Some(mode) = owner.raw_get::<Option<String>>("__songlua_speedmod_active")? {
        let value = owner.raw_get::<Option<f32>>(format!("__songlua_speedmod_{mode}"))?;
        let explicit = owner
            .raw_get::<Option<bool>>("__songlua_speedmod_explicit")?
            .unwrap_or(true);
        if let Some(value) = value.filter(|value| mode != "xmod" || *value != 1.0 || explicit) {
            parts.push(match mode.as_str() {
                "cmod" => format!("C{value:.0}"),
                "mmod" => format!("m{value:.0}"),
                "amod" => format!("A{value:.0}"),
                "camod" => format!("CA{value:.0}"),
                _ => format!(
                    "{}x",
                    format!("{value:.2}")
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                ),
            });
        }
    }
    if let Some(mode) = state.raw_get::<Option<usize>>("modtimersetting")?
        && let Some(name) = ["ModTimerGame", "ModTimerBeat", "ModTimerSong"].get(mode)
    {
        parts.push((*name).to_string());
    }
    for name in crate::player_options::OPTION_STRING_NAMES {
        let key = match *name {
            "RandomAttacks" => "randattack".to_string(),
            "NoAttacks" => "noattack".to_string(),
            _ => name.to_ascii_lowercase(),
        };
        let value = match state.raw_get::<Value>(key)? {
            Value::Boolean(value) => f32::from(value),
            value => read_f32(value).unwrap_or(0.0),
        };
        option_string_part(&mut parts, name, value);
        if *name == "Cosecant" {
            for col in 1..=16 {
                for prefix in crate::player_options::SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
                    let name = format!("{prefix}{col}");
                    let value = state
                        .raw_get::<Option<f32>>(name.to_ascii_lowercase())?
                        .unwrap_or(0.0);
                    option_string_part(&mut parts, &name, value);
                }
            }
        }
    }
    for name in SONG_LUA_PLAYER_OPTION_CAPABILITIES
        .iter()
        .filter(|name| player_option_uses_bool(&name.to_ascii_lowercase()))
    {
        if !crate::player_options::OPTION_STRING_NAMES.contains(name)
            && state
                .raw_get::<Option<bool>>(name.to_ascii_lowercase())?
                .unwrap_or(false)
        {
            parts.push((*name).to_string());
        }
    }
    let tilt = state.raw_get::<Option<f32>>("tilt")?.unwrap_or(0.0);
    let skew = state.raw_get::<Option<f32>>("skew")?.unwrap_or(0.0);
    match (tilt, skew) {
        (0.0, 0.0) => parts.push("Overhead".to_string()),
        (tilt, 0.0) => option_string_part(
            &mut parts,
            if tilt > 0.0 { "Distant" } else { "Hallway" },
            tilt.abs(),
        ),
        (tilt, skew) if (tilt - skew).abs() < 0.0001 => {
            option_string_part(&mut parts, "Space", skew)
        }
        (tilt, skew) if (tilt + skew).abs() < 0.0001 => {
            option_string_part(&mut parts, "Incoming", skew)
        }
        _ => {
            option_string_part(&mut parts, "Skew", skew);
            option_string_part(&mut parts, "Tilt", tilt);
        }
    }
    let skin = owner.raw_get::<Option<String>>("__songlua_noteskin_name")?.unwrap_or_default();
    if !skin.is_empty() && skin != SONG_LUA_DEFAULT_NOTESKIN_NAME {
        // RageUtil::Capitalize uppercases the first Unicode character only.
        let mut name = skin.clone();
        if let Some(first) = skin.chars().next() {
            // ITGmania's g_UpperCase only maps ASCII and these Latin-1 ranges.
            let code = first as u32;
            let upper = match code {
                0x61..=0x7a | 0xe0..=0xf6 | 0xf8..=0xfe => code - 0x20,
                _ => code,
            };
            if let Some(upper) = char::from_u32(upper).filter(|upper| *upper != first) {
                name.replace_range(..first.len_utf8(), &upper.to_string());
            }
        }
        parts.push(name);
    }
    Ok(parts)
}

pub fn create_steps_table(
    lua: &Lua,
    difficulty: SongLuaDifficulty,
    display_bpms: [f32; 2],
    song_dir: &Path,
    steps_type: &str,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetDifficulty", difficulty.sm_name())?;
    set_string_method(lua, &table, "GetStepsType", steps_type)?;
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
    let meter = difficulty.meter();
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

fn create_player_options_table(lua: &Lua, player: SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if player.perspective.tilt != 0.0 || player.perspective.skew != 0.0 {
        let state = player_option_state(lua, &table)?;
        state.set("tilt", player.perspective.tilt)?;
        state.set("skew", player.perspective.skew)?;
    }
    table.set(
        "__songlua_reference_bpm",
        player.display_bpms[1].max(player.display_bpms[0]).max(1.0),
    )?;
    install_speedmod_method(lua, &table, "CMod", player.speedmod, SongLuaSpeedMod::C)?;
    install_speedmod_state_method(lua, &table, "CAMod", Value::Nil)?;
    install_speedmod_method(lua, &table, "MMod", player.speedmod, SongLuaSpeedMod::M)?;
    install_speedmod_method(lua, &table, "AMod", player.speedmod, SongLuaSpeedMod::A)?;
    install_speedmod_method(lua, &table, "XMod", player.speedmod, SongLuaSpeedMod::X)?;
    // Native GetMods includes the initial profile speed. Default 1x is
    // omitted until explicitly set; later string round-trips must keep C/M/X.
    match player.speedmod {
        SongLuaSpeedMod::X(value) => {
            set_player_speedmod(&table, "xmod", Some(value))?;
            table.raw_set("__songlua_speedmod_explicit", value != 1.0)?;
        }
        SongLuaSpeedMod::C(value) => set_player_speedmod(&table, "cmod", Some(value))?,
        SongLuaSpeedMod::M(value) => set_player_speedmod(&table, "mmod", Some(value))?,
        SongLuaSpeedMod::A(value) => set_player_speedmod(&table, "amod", Some(value))?,
    }
    table.set(
        "FromString",
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                if let Some(text) = method_arg(&args, 0).cloned().and_then(read_string) {
                    #[cfg(feature = "test-support")]
                    let previous = player_noteskin(&table)?;
                    apply_player_options_string(lua, &table, &text)?;
                    #[cfg(feature = "test-support")]
                    capture_skin_write(lua, &table, "fromstring", previous)?;
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
            move |lua, args: MultiValue| {
                let col = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0) as i32;
                let count = song_lua_style_info(&current_song_lua_style_name(lua)).columns as i32;
                if col < 0 || col > count {
                    return Ok(None);
                }
                let mut value = player_option_number(lua, &table, "reverse")?
                    + player_option_number(lua, &table, &format!("reverse{}", col + 1))?;
                if col >= count / 2 {
                    value += player_option_number(lua, &table, "split")?;
                }
                if col % 2 == 1 {
                    value += player_option_number(lua, &table, "alternate")?;
                }
                if (count / 4..=count - 1 - count / 4).contains(&col) {
                    value += player_option_number(lua, &table, "cross")?;
                }
                if value > 2.0 {
                    value %= 2.0;
                }
                if value > 1.0 {
                    value = 2.0 - value;
                }
                Ok(Some(value))
            }
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
                return Ok(MultiValue::new());
            };
            let previous = player_noteskin(&owner)?;
            let mut accepted = Value::Nil;
            if let Some(noteskin_name) = method_arg(&args, 0).cloned()
                .map(|value| lua.coerce_string(value)).transpose()?.flatten()
            {
                let noteskin_name = noteskin_name.to_str()?.to_owned();
                let skins = lua.globals().get::<Table>("NOTESKIN")?;
                let exists = skins
                    .get::<Function>("DoesNoteSkinExist")?
                    .call::<bool>((skins, noteskin_name.clone()))?;
                if exists {
                    owner.raw_set("__songlua_noteskin_override", noteskin_name.clone())?;
                    owner.raw_set("__songlua_noteskin_name", noteskin_name)?;
                    accepted = Value::Boolean(true);
                    note_song_lua_side_effect(lua)?;
                }
                #[cfg(feature = "test-support")]
                capture_skin_write(lua, &owner, "noteskin", previous.clone())?;
            }
            if args.len() > 1 && matches!(args.back(), Some(Value::Boolean(true))) {
                return Ok(MultiValue::from_vec(vec![Value::Table(owner)]));
            }
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(&previous)?),
                accepted,
            ]))
        })?,
    )?;
    let mt = lua.create_table()?;
    let fallback_owner = table.clone();
    mt.set(
        "__index",
        lua.create_function(move |lua, (_owner, name): (Option<Value>, Option<Value>)| {
            let Some(name) = name.and_then(read_string) else {
                return Ok(Value::Nil);
            };
            if !is_player_option_method_name(&name) {
                return Ok(Value::Nil);
            }
            let method = create_player_option_method(lua, &fallback_owner, &name)?;
            fallback_owner.raw_set(name.as_str(), method.clone())?;
            Ok(Value::Function(method))
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

fn player_option_number(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<f32> {
    Ok(player_option_state(lua, owner)?
        .get::<Option<f32>>(name)?
        .unwrap_or(0.0))
}

fn create_player_option_method(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<Function> {
    let owner = owner.clone();
    let name = name.to_ascii_lowercase();
    if name == "modtimersetting"
        || (player_option_default_string(&name).is_none() && name != "batterylives")
    {
        return create_native_option(lua, &owner, name);
    }
    let key = lua.create_string(&name)?;
    let string = player_option_default_string(&name).is_some();
    // Only immutable method metadata is captured. State and approach tables
    // are resolved on every call, including after replacement by Lua code.
    let default = default_player_option_value(lua, &name)?;
    lua.create_function(
        move |lua, (_self, value, speed): (Option<Value>, Option<Value>, Option<f32>)| {
            let state = player_option_state(lua, &owner)?;
            if let Some(value) = value {
                let value = if string {
                    if matches!(value, Value::String(_)) {
                        value
                    } else {
                        default.clone()
                    }
                } else {
                    Value::Number(f64::from(read_f32(value).unwrap_or(0.0)))
                };
                state.set(&key, value)?;
                let speeds = player_option_speeds(lua, &owner)?;
                let speed = speed.or(speeds.get::<Option<f32>>(&key)?).unwrap_or(1.0);
                speeds.set(&key, speed.max(0.0))?;
                return Ok(Value::Table(owner.clone()));
            }
            Ok(match state.get::<Option<Value>>(&key)? {
                Some(value) => value,
                None => default.clone(),
            })
        },
    )
}

// PlayerOptions perspective aliases share only these two native fields.
// Keeping alias names out of the sampled state also makes write order explicit.
fn set_perspective_angle(state: &Table, key: &str, value: f32) -> mlua::Result<bool> {
    let (tilt, skew) = match key {
        "incoming" => (-value, value),
        "space" => (value, value),
        "hallway" => (-value, 0.0),
        "distant" => (value, 0.0),
        "overhead" => (0.0, 0.0),
        _ => return Ok(false),
    };
    state.set("tilt", tilt)?;
    state.set("skew", skew)?;
    Ok(true)
}

fn native_option_previous(state: &Table, speeds: &Table, key: &str) -> mlua::Result<[Value; 2]> {
    if !matches!(
        key,
        "overhead" | "incoming" | "space" | "hallway" | "distant" | "tilt" | "skew"
    ) {
        return Ok([
            Value::Number(f64::from(state.get::<Option<f32>>(key)?.unwrap_or(0.0))),
            Value::Number(f64::from(speeds.get::<Option<f32>>(key)?.unwrap_or(1.0))),
        ]);
    }
    let tilt = state.get::<Option<f32>>("tilt")?.unwrap_or(0.0);
    let skew = state.get::<Option<f32>>("skew")?.unwrap_or(0.0);
    let (value, speed_key) = match key {
        "overhead" => return Ok([Value::Boolean(tilt == 0.0 && skew == 0.0), Value::Nil]),
        "incoming" if (skew > 0.0 && tilt < 0.0) || (skew < 0.0 && tilt > 0.0) => (skew, "skew"),
        "space" if (skew > 0.0 && tilt > 0.0) || (skew < 0.0 && tilt < 0.0) => (skew, "skew"),
        "hallway" if skew == 0.0 && tilt < 0.0 => (-tilt, "tilt"),
        "distant" if skew == 0.0 && tilt > 0.0 => (tilt, "tilt"),
        "tilt" => (tilt, "tilt"),
        "skew" => (skew, "skew"),
        _ => return Ok([Value::Nil, Value::Nil]),
    };
    Ok([
        Value::Number(f64::from(value)),
        Value::Number(f64::from(
            speeds.get::<Option<f32>>(speed_key)?.unwrap_or(1.0),
        )),
    ])
}

fn create_timer_option(lua: &Lua, owner: &Table) -> mlua::Result<Function> {
    use crate::player_options::MOD_TIMER_NAMES;
    let owner = owner.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let state = player_option_state(lua, &owner)?;
        let previous = state.get::<Option<u8>>("modtimersetting")?.unwrap_or(3);
        if let Some(value) = method_arg(&args, 0).filter(|v| !matches!(v, Value::Nil)) {
            let mode = match value {
                Value::Integer(value) if (0..=3).contains(value) => Some(*value as u8),
                Value::Number(value) if (0.0..=3.0).contains(value) && value.fract() == 0.0 => {
                    Some(*value as u8)
                }
                Value::String(value) => {
                    let value = value.to_str()?;
                    MOD_TIMER_NAMES
                        .iter()
                        .position(|label| {
                            value == *label
                                || label
                                    .strip_prefix("ModTimerType_")
                                    .is_some_and(|legacy| value.eq_ignore_ascii_case(legacy))
                        })
                        .map(|mode| mode as u8)
                }
                _ => None,
            }
            .ok_or_else(|| mlua::Error::runtime("Invalid ModTimerType"))?;
            state.set("modtimersetting", mode)?;
        }
        let result = if matches!(args.back(), Some(Value::Boolean(true))) {
            Value::Table(owner.clone())
        } else if let Some(label) = MOD_TIMER_NAMES.get(usize::from(previous)) {
            Value::String(lua.create_string(*label)?)
        } else {
            Value::Nil
        };
        Ok(MultiValue::from_iter([result]))
    })
}

// Reference-only worker capture, owned by one compiler Lua VM. It lives for
// the chronological replay and is discarded with the compiled audit data.
// At most two million writes are retained; overflow fails the audit. Shipping
// builds contain neither this allocation nor the recording branch.
#[cfg(feature = "test-support")]
#[derive(Default)]
pub(crate) struct SongLuaBoolWrites(pub Vec<SongLuaBoolWrite>);

#[cfg(feature = "test-support")]
fn capture_bool_write(
    lua: &Lua,
    owner: &Table,
    key: &str,
    previous: bool,
    current: bool,
    chained: bool,
) -> mlua::Result<()> {
    if lua.app_data_ref::<SongLuaBoolWrites>().is_none() {
        return Ok(());
    }
    let globals = lua.globals();
    let mut player = None;
    for (index, name) in SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
        if globals.get::<Table>(*name)?.to_pointer() == owner.to_pointer() {
            player = Some(index);
            break;
        }
    }
    let Some(player) = player else { return Ok(()) };
    let runtime = globals.get::<Table>(SONG_LUA_RUNTIME_KEY)?;
    let write = SongLuaBoolWrite {
        player,
        key: key.to_owned(),
        beat: runtime.get(SONG_LUA_RUNTIME_BEAT_KEY)?,
        second: runtime.get(SONG_LUA_RUNTIME_SECONDS_KEY)?,
        previous,
        current,
        chained,
    };
    if let Some(mut capture) = lua.app_data_mut::<SongLuaBoolWrites>() {
        if capture.0.len() >= 2_000_000 {
            return Err(mlua::Error::runtime(
                "boolean option audit exceeded two million writes",
            ));
        }
        capture.0.push(write);
    }
    Ok(())
}

fn keep_option_speed(speeds: &Table, key: &str) -> mlua::Result<()> {
    if speeds.raw_get::<Option<f32>>(key)?.is_none() {
        // PlayerOptions::Init initializes scalar approach speeds to one.
        speeds.raw_set(key, 1.0)?;
    }
    Ok(())
}

fn create_native_option(lua: &Lua, owner: &Table, key: String) -> mlua::Result<Function> {
    if key == "modtimersetting" {
        return create_timer_option(lua, owner);
    }
    if player_option_uses_bool(&key) && key != "overhead" {
        let owner = owner.clone();
        return lua.create_function(move |lua, mut args: crate::method_args::MethodArgs<3>| {
            let state = player_option_state(lua, &owner)?;
            let previous = state.get::<Option<bool>>(key.as_str())?.unwrap_or(false);
            // BOOL_INTERFACE chains on a boolean second argument, even false.
            // Read it before consuming a possible owner-less first argument.
            let chained = matches!(args.take_method_arg(1), Some(Value::Boolean(_)));
            if let Some(Value::Boolean(value)) = args.take_method_arg(0) {
                state.set(key.as_str(), value)?;
                #[cfg(feature = "test-support")]
                capture_bool_write(lua, &owner, &key, previous, value, chained)?;
            }
            Ok(if chained {
                Value::Table(owner.clone())
            } else {
                Value::Boolean(previous)
            })
        });
    }
    let owner = owner.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let state = player_option_state(lua, &owner)?;
        let speeds = player_option_speeds(lua, &owner)?;
        // OptionsBinding returns the values from before the setter, unless the
        // final argument is true and requests chaining. Inactive aliases return
        // nil, nil; Overhead has a single boolean result.
        let previous = native_option_previous(&state, &speeds, &key)?;
        if key == "overhead" {
            if method_arg(&args, 0)
                .is_some_and(|v| !matches!(v, Value::Nil | Value::Boolean(false)))
            {
                set_perspective_angle(&state, &key, 0.0)?;
            }
        } else if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
            if set_perspective_angle(&state, &key, value)? {
                keep_option_speed(&speeds, "tilt")?;
                keep_option_speed(&speeds, "skew")?;
            } else {
                state.set(key.as_str(), value)?;
                keep_option_speed(&speeds, &key)?;
            }
        }
        if let Some(speed) = method_arg(&args, 1).cloned().and_then(read_f32) {
            // Native validates after setting the amount, preserving that write
            // if a negative speed raises a Lua error.
            if speed < 0.0 {
                return Err(mlua::Error::runtime(
                    "Arg must be greater than or equal to zero.",
                ));
            }
            if matches!(
                key.as_str(),
                "incoming" | "space" | "hallway" | "distant" | "overhead"
            ) {
                speeds.set("tilt", speed)?;
                speeds.set("skew", speed)?;
            } else {
                speeds.set(key.as_str(), speed)?;
            }
        }
        if matches!(args.back(), Some(Value::Boolean(true))) {
            return Ok(MultiValue::from_iter([Value::Table(owner.clone())]));
        }
        Ok(MultiValue::from_iter(
            previous
                .into_iter()
                .take(if key == "overhead" { 1 } else { 2 }),
        ))
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

fn player_option_speeds(lua: &Lua, owner: &Table) -> mlua::Result<Table> {
    if let Some(speeds) = owner.raw_get::<Option<Table>>("__songlua_player_option_speeds")? {
        return Ok(speeds);
    }
    let speeds = lua.create_table()?;
    owner.raw_set("__songlua_player_option_speeds", speeds.clone())?;
    Ok(speeds)
}

fn set_player_speed_approaches(lua: &Lua, owner: &Table, speed: Option<f32>) -> mlua::Result<()> {
    let speeds = player_option_speeds(lua, owner)?;
    let speed = speed
        .or(speeds.raw_get::<Option<f32>>("xmod")?)
        .unwrap_or(1.0)
        .max(0.0);
    // PlayerOptions::SetSpeedModApproaches updates all speed modes together.
    // Record these writes like Mini/Flip so sampled XMod targets do not
    // interpolate ahead of the matching size/position changes.
    for key in ["xmod", "cmod", "mmod"] {
        speeds.raw_set(key, speed)?;
    }
    Ok(())
}

fn reset_player_options(lua: &Lua, owner: &Table) -> mlua::Result<()> {
    let state = player_option_state(lua, owner)?;
    let speeds = player_option_speeds(lua, owner)?;
    for pair in state.pairs::<String, Value>() {
        let (key, _) = pair?;
        if key == "modtimersetting" {
            state.raw_set(key.as_str(), 3)?;
            speeds.raw_set(key, Value::Nil)?;
        } else {
            state.raw_set(key.as_str(), default_player_option_value(lua, &key)?)?;
            speeds.raw_set(key, 1.0_f32)?;
        }
    }
    set_player_speedmod(owner, "xmod", Some(1.0))?;
    owner.raw_set("__songlua_speedmod_explicit", false)?;
    owner.raw_set("__songlua_noteskin_name", "")?;
    owner.raw_set("__songlua_noteskin_override", Value::Nil)?;
    set_player_speed_approaches(lua, owner, Some(1.0))
}

fn apply_player_options_string(lua: &Lua, owner: &Table, text: &str) -> mlua::Result<()> {
    for option in text.split(',') {
        apply_player_option_token(lua, owner, option)?;
    }
    Ok(())
}

pub(crate) fn player_uses_modifiers(
    lua: &Lua,
    owner: &Table,
    song_options: &Table,
    text: &str,
) -> mlua::Result<bool> {
    // GameState::PlayerIsUsingModifier applies the string to copies and compares
    // the resulting values. Queries must not write to the live options.
    let requested = lua.create_table()?;
    let mut rate = None;
    let mut skin = None;
    for token in text.split(',') {
        let token = strip_player_option_prefix(token).trim();
        let lower = token.to_ascii_lowercase();
        if let Some(value) = lower
            .strip_suffix("xmusic")
            .and_then(|s| s.parse::<f32>().ok())
        {
            rate = Some(value);
        } else {
            let skins = lua.globals().get::<Table>("NOTESKIN")?;
            if skins
                .get::<Function>("DoesNoteSkinExist")?
                .call::<bool>((skins, token))?
            {
                skin = Some(lower);
            } else {
                apply_player_option_token(lua, &requested, token)?;
            }
        }
    }
    if let Some(rate) = rate {
        if song_options.raw_get::<f32>("__songlua_music_rate")? != rate {
            return Ok(false);
        }
    }
    if let Some(skin) = skin {
        if !owner
            .raw_get::<String>("__songlua_noteskin_name")?
            .eq_ignore_ascii_case(&skin)
        {
            return Ok(false);
        }
    }
    if let Some(mode) = requested.raw_get::<Option<String>>("__songlua_speedmod_active")? {
        let key = format!("__songlua_speedmod_{mode}");
        if owner
            .raw_get::<Option<String>>("__songlua_speedmod_active")?
            .as_deref()
            != Some(&mode)
            || owner.raw_get::<Value>(key.as_str())? != requested.raw_get::<Value>(key)?
        {
            return Ok(false);
        }
    }
    let current = player_option_state(lua, owner)?;
    for pair in player_option_state(lua, &requested)?.pairs::<String, Value>() {
        let (key, expected) = pair?;
        if !crate::player_options::SONG_LUA_PLAYER_OPTION_CAPABILITIES
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&key))
        {
            continue; // Native FromString ignores unrecognized modifier names.
        }
        let actual = match current.raw_get::<Value>(key.as_str())? {
            Value::Nil if key == "modtimersetting" => Value::Integer(3),
            Value::Nil => default_player_option_value(lua, &key)?,
            value => value,
        };
        if actual != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn apply_player_option_token(lua: &Lua, owner: &Table, raw: &str) -> mlua::Result<()> {
    let mut name = "";
    let mut amount = 1.0;
    let mut speed = 1.0;
    // PlayerOptions::FromOneModString visits every space-separated prefix;
    // later levels/speeds overwrite earlier ones and the final word is the mod.
    for part in raw.trim().split(' ').filter(|part| !part.is_empty()) {
        name = part;
        if part.eq_ignore_ascii_case("no") {
            amount = 0.0;
        } else if part.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-') {
            if part.ends_with('*') {
                return Ok(()); // Native rejects a misplaced approach-speed star.
            }
            amount = parse_player_option_amount(&part.to_ascii_lowercase()).unwrap_or(0.0);
        } else if let Some(prefix) = part.strip_prefix('*')
            && let Some(value) = crate::player_options::parse_option_float(prefix)
        {
            speed = if value.is_finite() { value } else { 1.0 };
        }
    }
    if name.is_empty() {
        return Ok(());
    }
    if apply_player_speed_option(owner, name)? {
        set_player_speed_approaches(lua, owner, Some(speed))?;
        return Ok(());
    }
    let lower = name.to_ascii_lowercase();
    if !crate::player_options::option_blocks_skin(&lower, amount, false)
        && apply_string_skin(lua, owner, &lower)?
    {
        return Ok(());
    }
    if !crate::player_options::option_blocks_skin(&lower, amount, true)
        && apply_string_skin(lua, owner, &raw.trim().to_ascii_lowercase())?
    {
        return Ok(());
    }
    crate::player_options::with_normalized_player_option_key(name, |key| {
        if key.is_empty() {
            return Ok(());
        }
        if key == "clearall" {
            reset_player_options(lua, owner)?;
            let skins = lua.globals().get::<Table>("NOTESKIN")?;
            let exists = skins.get::<Function>("DoesNoteSkinExist")?;
            let name = if exists.call::<bool>((skins.clone(), SONG_LUA_DEFAULT_NOTESKIN_NAME))? {
                SONG_LUA_DEFAULT_NOTESKIN_NAME.to_owned()
            } else if exists.call::<bool>((skins.clone(), "default"))? {
                "default".to_owned()
            } else {
                let names = skins.get::<Function>("GetNoteSkinNames")?.call::<Table>(skins)?;
                names.raw_get::<Option<String>>(2)?.unwrap_or_default()
            };
            owner.raw_set("__songlua_noteskin_name", name.clone())?;
            return owner.raw_set("__songlua_noteskin_override", name);
        }
        if key == "noteskin" && amount <= 0.5 {
            owner.raw_set("__songlua_noteskin_name", SONG_LUA_DEFAULT_NOTESKIN_NAME)?;
            return owner.raw_set("__songlua_noteskin_override", SONG_LUA_DEFAULT_NOTESKIN_NAME);
        }
        let state = player_option_state(lua, owner)?;
        let timer = match key {
            "modtimergame" => Some(0),
            "modtimerbeat" => Some(1),
            "modtimersong" => Some(2),
            "modtimerdefault" => Some(3),
            _ => None,
        };
        if let Some(timer) = timer {
            return state.set("modtimersetting", timer);
        }
        if set_perspective_angle(&state, key, amount)? {
            let speeds = player_option_speeds(lua, owner)?;
            speeds.set("tilt", speed)?;
            speeds.set("skew", speed)?;
            return Ok(());
        }
        let value = if player_option_uses_bool(key) {
            Value::Boolean(amount > 0.5)
        } else {
            Value::Number(f64::from(amount))
        };
        state.set(key, value)?;
        player_option_speeds(lua, owner)?.set(key, speed)
    })
}

#[cfg(feature = "test-support")]
pub(crate) struct SongLuaSkinWrites {
    pub writes: Vec<SongLuaSkinWrite>,
    pub active: bool,
}

#[cfg(feature = "test-support")]
impl Default for SongLuaSkinWrites {
    fn default() -> Self { Self { writes: Vec::new(), active: true } }
}

#[cfg(feature = "test-support")]
fn capture_skin_write(lua: &Lua, owner: &Table, key: &str, previous: String) -> mlua::Result<()> {
    if !lua.app_data_ref::<SongLuaSkinWrites>().is_some_and(|capture| capture.active) {
        return Ok(());
    }
    let globals = lua.globals();
    let mut player = None;
    for (index, name) in SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
        if globals.get::<Table>(*name)?.to_pointer() == owner.to_pointer() {
            player = Some(index);
            break;
        }
    }
    let Some(player) = player else { return Ok(()) };
    let runtime = globals.get::<Table>(SONG_LUA_RUNTIME_KEY)?;
    let write = SongLuaSkinWrite {
        player, key: key.to_owned(), previous, current: player_noteskin(owner)?,
        beat: runtime.get(SONG_LUA_RUNTIME_BEAT_KEY)?,
        second: runtime.get(SONG_LUA_RUNTIME_SECONDS_KEY)?,
    };
    if let Some(mut capture) = lua.app_data_mut::<SongLuaSkinWrites>() {
        if capture.writes.len() >= 2_000_000 {
            return Err(mlua::Error::runtime("noteskin audit exceeded two million writes"));
        }
        capture.writes.push(write);
    }
    Ok(())
}

fn player_noteskin(owner: &Table) -> mlua::Result<String> {
    Ok(owner.raw_get::<Option<String>>("__songlua_noteskin_name")?
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| SONG_LUA_DEFAULT_NOTESKIN_NAME.to_owned()))
}

fn apply_string_skin(lua: &Lua, owner: &Table, name: &str) -> mlua::Result<bool> {
    let skins = lua.globals().get::<Table>("NOTESKIN")?;
    let exists = skins.get::<Function>("DoesNoteSkinExist")?
        .call::<bool>((skins, name))?;
    if exists {
        owner.raw_set("__songlua_noteskin_override", name)?;
        owner.raw_set("__songlua_noteskin_name", name)?;
        note_song_lua_side_effect(lua)?;
    }
    Ok(exists)
}

fn apply_player_speed_option(owner: &Table, text: &str) -> mlua::Result<bool> {
    let Some((key, value)) = parse_player_speed_option(text) else {
        return Ok(false);
    };
    set_player_speedmod(owner, key, Some(value))?;
    Ok(true)
}

fn install_speedmod_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    speedmod: SongLuaSpeedMod,
    ctor: fn(f32) -> SongLuaSpeedMod,
) -> mlua::Result<()> {
    let initial = song_lua_speedmod_value(speedmod, ctor);
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
    // Retain the small set of values written by speedmod setters. Comparing Lua
    // handles avoids both owned text and BorrowedStr's shared-reference allocation.
    let active_keys = [
        key.as_str(),
        "xmod",
        "cmod",
        "mmod",
        "amod",
        "camod",
        "none",
        "",
    ]
    .into_iter()
    .map(|key| {
        let key = lua.create_string(key)?;
        let identity = key.to_pointer() as usize;
        Ok((key, identity))
    })
    .collect::<mlua::Result<Vec<_>>>()?
    .into_boxed_slice();
    table.set(
        name,
        lua.create_function(move |lua, mut args: crate::method_args::MethodArgs<3>| {
            if let Some(value) = args.take_method_arg(0) {
                if matches!(value, Value::Nil) {
                    set_player_speedmod_with_key(&owner, &key, &value_key, None)?;
                } else if let Some(value) = read_f32(value) {
                    set_player_speedmod_with_key(&owner, &key, &value_key, Some(value))?;
                    if matches!(key.as_str(), "xmod" | "cmod" | "mmod") {
                        let speed = args.take_method_arg(1).and_then(read_f32);
                        set_player_speed_approaches(lua, &owner, speed)?;
                    }
                }
                return Ok(Value::Table(owner.clone()));
            }

            let active = player_speedmod_is_active(&owner, &key, &active_keys)?;
            if active == Some(false) {
                return Ok(Value::Nil);
            }
            if active == Some(true) {
                return Ok(owner
                    .raw_get::<Option<f32>>(value_key.as_str())?
                    .map_or(Value::Nil, |value| Value::Number(f64::from(value))));
            }
            Ok(initial.clone())
        })?,
    )
}

// Unknown/coercible values still use FromLua<String>, preserving numeric
// conversion and exact errors, including invalid UTF-8. No state is cached:
// every call observes the owner's current raw field.
fn player_speedmod_is_active(
    owner: &Table,
    key: &str,
    active_keys: &[(mlua::LuaString, usize)],
) -> mlua::Result<Option<bool>> {
    match owner.raw_get::<Value>("__songlua_speedmod_active")? {
        Value::Nil => return Ok(None),
        Value::String(active) => {
            // Retained handles keep these immutable strings alive. A matching
            // identity proves equality; a miss still takes the text fallback.
            let identity = active.to_pointer() as usize;
            if let Some(index) = active_keys.iter().position(|(_, key)| *key == identity) {
                return Ok(Some(index == 0));
            }
        }
        _ => {}
    }
    Ok(owner
        .raw_get::<Option<String>>("__songlua_speedmod_active")?
        .map(|active| active == key))
}

fn set_player_speedmod(owner: &Table, key: &str, value: Option<f32>) -> mlua::Result<()> {
    // FromString produces these five keys; installed methods already retain
    // their full field key. Preserve the fallback for other internal callers.
    let value_key = match key {
        "xmod" => "__songlua_speedmod_xmod",
        "cmod" => "__songlua_speedmod_cmod",
        "mmod" => "__songlua_speedmod_mmod",
        "amod" => "__songlua_speedmod_amod",
        "camod" => "__songlua_speedmod_camod",
        _ => {
            return set_player_speedmod_with_key(
                owner,
                key,
                &format!("__songlua_speedmod_{key}"),
                value,
            );
        }
    };
    set_player_speedmod_with_key(owner, key, value_key, value)
}

fn set_player_speedmod_with_key(
    owner: &Table,
    key: &str,
    value_key: &str,
    value: Option<f32>,
) -> mlua::Result<()> {
    if let Some(value) = value {
        owner.raw_set("__songlua_speedmod_active", key)?;
        owner.raw_set(value_key, value)?;
        owner.raw_set("__songlua_speedmod_explicit", Value::Nil)?;
    } else {
        owner.raw_set("__songlua_speedmod_active", "none")?;
        owner.raw_set(value_key, Value::Nil)?;
    }
    Ok(())
}

pub fn create_song_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
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
    let steps_by_type_for_get = steps_by_type;
    table.set(
        "GetStepsByStepsType",
        lua.create_function(move |lua, args: MultiValue| {
            if !method_arg(&args, 0)
                .cloned()
                .is_some_and(song_lua_steps_type_is_dance_single)
            {
                return lua.create_table();
            }
            Ok(steps_by_type_for_get.clone())
        })?,
    )?;
    Ok(table)
}

pub fn create_course_table(
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

pub fn create_trail_table(
    lua: &Lua,
    song: Table,
    steps: Table,
    display_bpms: [f32; 2],
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("__songlua_steps", steps.clone())?;
    let steps_type = steps
        .get::<Function>("GetStepsType")?
        .call::<String>(steps.clone())?;
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
    set_string_method(lua, &table, "GetStepsType", &steps_type)?;
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

pub fn create_song_util_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetPlayableSteps",
        lua.create_function(|lua, args: MultiValue| {
            let Some(song) = song_util_song_arg(&args)? else {
                return Ok(Value::Table(lua.create_table()?));
            };
            call_song_steps_method(lua, &song, "GetAllSteps", Value::Nil)
        })?,
    )?;
    table.set(
        "GetPlayableStepsByStepsType",
        lua.create_function(|lua, args: MultiValue| {
            let Some(song) = song_util_song_arg(&args)? else {
                return Ok(Value::Table(lua.create_table()?));
            };
            let steps_type = match args
                .iter()
                .skip_while(|value| {
                    !matches!(value, Value::Table(table) if table.to_pointer() == song.to_pointer())
                })
                .nth(1)
                .cloned()
            {
                Some(value) => value,
                None => Value::String(lua.create_string("StepsType_Dance_Single")?),
            };
            call_song_steps_method(lua, &song, "GetStepsByStepsType", steps_type)
        })?,
    )?;
    Ok(table)
}

fn song_util_song_arg(args: &MultiValue) -> mlua::Result<Option<Table>> {
    for value in args {
        let Value::Table(table) = value else {
            continue;
        };
        if matches!(table.get::<Value>("GetAllSteps")?, Value::Function(_)) {
            return Ok(Some(table.clone()));
        }
    }
    Ok(None)
}

fn call_song_steps_method(
    lua: &Lua,
    song: &Table,
    method_name: &str,
    argument: Value,
) -> mlua::Result<Value> {
    let Value::Function(method) = song.get::<Value>(method_name)? else {
        return Ok(Value::Table(lua.create_table()?));
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(song.clone()));
    if !matches!(argument, Value::Nil) {
        args.push_back(argument);
    }
    method.call::<Value>(args)
}

pub fn create_songman_table(
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
            let current_course_dir = current_course_dir;
            let current_dir = current_dir;
            let current_group = current_group.clone();
            let current_title = current_title;
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
            let current_course = current_course;
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
            let current_song = current_song;
            let current_steps = current_steps;
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
            let current_group = current_group;
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
    let popular_courses = group_courses;
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
            create_steps_table(
                lua,
                difficulty,
                display_bpms,
                song_dir,
                "StepsType_Dance_Single",
            )?,
        )?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options_lua() -> Lua {
        let lua = Lua::new();
        // Native FromString/clearall requires a manager even with no skin assets.
        let skins = lua.create_table().expect("empty noteskin manager");
        skins.set("DoesNoteSkinExist", lua.create_function(|_, _: MultiValue| Ok(false))
            .expect("empty skin lookup")).expect("install lookup");
        skins.set("GetNoteSkinNames", lua.create_function(|lua, _: MultiValue| lua.create_table())
            .expect("empty skin list")).expect("install names");
        lua.globals().set("NOTESKIN", skins).expect("expose noteskin manager");
        lua
    }

    #[test]
    fn reverse_columns_match_native_composition() {
        for (name, expected) in [
            ("single", &[0.5, 0.625, 1.0, 0.875][..]),
            (
                "double",
                &[0.5, 0.375, 0.5, 0.625, 1.0, 0.875, 0.75, 0.875][..],
            ),
        ] {
            let lua = options_lua();
            let style = crate::tables::create_style_table(&lua, name).expect("style");
            let gamestate = lua.create_table().expect("gamestate");
            gamestate
                .set(
                    "GetCurrentStyle",
                    lua.create_function(move |_, _: MultiValue| Ok(style.clone()))
                        .expect("style getter"),
                )
                .expect("install getter");
            lua.globals()
                .set("GAMESTATE", gamestate)
                .expect("expose gamestate");
            let options = create_player_options_table(&lua, SongLuaPlayerContext::default())
                .expect("options");
            lua.globals()
                .set("o", options.clone())
                .expect("expose options");
            lua.load(
                "o:FromString('25% Reverse, 50% Split, 12.5% Alternate, 25% Cross, 25% Reverse1')",
            )
            .exec()
            .expect("write scroll options");
            let query = options
                .get::<Function>("GetReversePercentForColumn")
                .expect("query");
            for (column, &expected) in expected.iter().enumerate() {
                assert_eq!(
                    query
                        .call::<f32>((options.clone(), column))
                        .expect("column reverse"),
                    expected,
                    "{name} column {column}"
                );
            }
            assert!(
                query
                    .call::<Option<f32>>((options.clone(), -1))
                    .expect("negative column")
                    .is_none()
            );
            assert!(
                query
                    .call::<Option<f32>>((options.clone(), expected.len() + 1))
                    .expect("overflow column")
                    .is_none()
            );
            lua.load("o:FromString('clearall, 175% Reverse'); assert(o:GetReversePercentForColumn(0) == 0.25); o:FromString('clearall, -50% Reverse'); assert(o:GetReversePercentForColumn(0) == -0.5)").exec().expect("native folding");
        }
    }

    #[test]
    fn perspective_aliases_read_shared_native_fields_and_speeds() {
        let lua = Lua::new();
        let options =
            create_player_options_table(&lua, SongLuaPlayerContext::default()).expect("options");
        lua.globals()
            .set("o", options.clone())
            .expect("expose options");
        lua.load(
            r#"
assert(o:Overhead() == true and select('#', o:Overhead()) == 1)
for _, name in ipairs{'Incoming', 'Space', 'Hallway', 'Distant'} do
    local value, speed = o[name](o)
    assert(value == nil and speed == nil and select('#', o[name](o)) == 2)
end
o:FromString('*3 50% incoming')
assert(o:Tilt() == -0.5 and o:Skew() == 0.5 and o:Overhead() == false)
local value, speed = o:Incoming()
assert(value == 0.5 and speed == 3)
local old, oldspeed = o:Space(-0.25, 7)
assert(old == nil and oldspeed == nil and o:Space() == -0.25)
assert(o:Incoming() == nil and o:Tilt() == -0.25 and o:Skew() == -0.25)
old, oldspeed = o:Space(0.75)
assert(old == -0.25 and oldspeed == 7)
o:Hallway(-0.75)
assert(o:Hallway() == nil and o:Distant() == 0.75 and o:Skew() == 0)
old, oldspeed = o:Tilt(-0.4, 2)
assert(old == 0.75 and oldspeed == 7 and math.abs(o:Hallway() - 0.4) < 1e-6)
old, oldspeed = o:Skew(0.6, 9)
assert(old == 0 and oldspeed == 7)
value, speed = o:Incoming()
assert(math.abs(value - 0.6) < 1e-6 and speed == 9)
assert(o:Space() == nil and o:Hallway() == nil and o:Distant() == nil)
o:Incoming('-0.5', '4')
assert(o:Incoming() == -0.5 and o:Tilt() == 0.5 and o:Skew() == -0.5)
o:Distant(-0.25)
assert(o:Hallway() == 0.25 and o:Distant() == nil)
"#,
        )
        .exec()
        .expect("native perspective aliases");
        let state = player_option_state(&lua, &options).expect("state");
        for alias in ["incoming", "space", "hallway", "distant", "overhead"] {
            assert!(matches!(
                state.raw_get::<Value>(alias).expect("alias state"),
                Value::Nil
            ));
        }
    }

    #[test]
    fn timer_binding_matches_native_enum_and_float_protocol() {
        let lua = options_lua();
        let options =
            create_player_options_table(&lua, SongLuaPlayerContext::default()).expect("options");
        lua.globals().set("o", options).expect("expose options");
        lua.load(
            r#"
assert(o:ModTimerSetting() == 'ModTimerType_Default' and select('#', o:ModTimerSetting()) == 1)
assert(o:ModTimerSetting('ModTimerType_Song') == 'ModTimerType_Default')
assert(o:ModTimerSetting('GaMe', true) == o and o:ModTimerSetting() == 'ModTimerType_Game')
assert(o:ModTimerSetting(1) == 'ModTimerType_Game' and o:ModTimerSetting() == 'ModTimerType_Beat')
assert(o:ModTimerSetting(nil) == 'ModTimerType_Beat' and o:ModTimerSetting(nil, true) == o)
for _, invalid in ipairs({-1, 4, 1.5, '1', 'modtimertype_song', false}) do
    assert(not pcall(function() o:ModTimerSetting(invalid) end))
    assert(o:ModTimerSetting() == 'ModTimerType_Beat')
end
for _, mode in ipairs({'Game', 'Beat', 'Song', 'Default'}) do
    o:FromString('*0 no modtimer'..mode)
    assert(o:ModTimerSetting() == 'ModTimerType_'..mode)
end
for _, name in ipairs({'ModTimerMult', 'ModTimerOffset'}) do
    local value, speed = o[name](o)
    assert(value == 0 and speed == 1 and select('#', o[name](o)) == 2)
    assert(o[name](o, -2, 3, true) == o)
    value, speed = o[name](o, 1)
    assert(value == -2 and speed == 3)
    assert(not pcall(function() o[name](o, -.5, -1) end))
    assert(o[name](o) == -.5 and select(2, o[name](o)) == 3)
end
o:FromString('modtimersong, clearall')
assert(o:ModTimerSetting() == 'ModTimerType_Default')
assert(o:ModTimerMult() == 0 and select(2, o:ModTimerMult()) == 1)
assert(o:ModTimerOffset() == 0 and select(2, o:ModTimerOffset()) == 1)
"#,
        )
        .exec()
        .expect("native timer protocol");
    }

    #[test]
    fn perspective_methods_keep_native_truthiness_chaining_and_error_order() {
        let lua = Lua::new();
        let options =
            create_player_options_table(&lua, SongLuaPlayerContext::default()).expect("options");
        lua.globals().set("o", options).expect("expose options");
        lua.load(
            r#"
assert(o:Space(0.5, 3, true) == o)
assert(o:Overhead(false, 4, true) == o)
assert(o:Tilt() == 0.5 and o:Skew() == 0.5)
local value, speed = o:Space()
assert(value == 0.5 and speed == 4)
assert(o:Overhead(false, 6) == false and o:Overhead() == false)
assert(select(2, o:Tilt()) == 6 and select(2, o:Skew()) == 6)
assert(o:Overhead(0) == false and o:Overhead() == true)
assert(o:Overhead(true) == o)
assert(o:Space(0.75, 4, true):Incoming(-0.5, true) == o)
value, speed = o:Incoming(nil, 11)
assert(value == -0.5 and speed == 4)
assert(o:Incoming() == -0.5 and select(2, o:Tilt()) == 11 and select(2, o:Skew()) == 11)
assert(o:Hallway(true) == o and o:Incoming() == -0.5)
assert(not pcall(o.Hallway, o, 0.75, -1))
assert(o:Hallway() == 0.75 and select(2, o:Hallway()) == 11)
assert(o:Overhead(nil, 8, true) == o and o:Hallway() == 0.75)
assert(select(2, o:Tilt()) == 8 and select(2, o:Skew()) == 8)
o:FromString('no overhead')
assert(o:Overhead() == true)
o:FromString('*3 50% incoming,*7 -25% space,*4 no distant')
assert(o:Overhead() == true and select(2, o:Tilt()) == 4 and select(2, o:Skew()) == 4)
"#,
        )
        .exec()
        .expect("native perspective calls");
    }

    #[test]
    fn perspective_getters_select_one_speed_correction_in_any_order() {
        let lua = Lua::new();
        let options =
            create_player_options_table(&lua, SongLuaPlayerContext::default()).expect("options");
        lua.globals().set("o", options).expect("expose options");
        lua.load(r#"
-- Sharkmode queries all four aliases in a pairs loop. Only the active alias
-- may be truthy; otherwise hash iteration changes the chosen correction.
for _, order in ipairs{{'Hallway','Distant','Incoming','Space'}, {'Space','Incoming','Distant','Hallway'}} do
    for _, active in ipairs(order) do
        o[active](o, 0.5, 3)
        local selected, count = nil, 0
        for _, name in ipairs(order) do
            if o[name](o) then selected = name; count = count + 1 end
        end
        assert(selected == active and count == 1)
    end
end
"#).exec().expect("stable perspective selection");
    }

    #[test]
    fn state_options_replace_previous_targets() {
        let lua = Lua::new();
        let context = SongLuaCompileContext::new(Path::new("."), "Option assignment");
        let runtime = create_song_runtime_table(&lua, &context).expect("create runtime");
        let (state, options) =
            create_player_state_table(&lua, context.players[0].clone(), 0, &runtime)
                .expect("create player state");
        lua.globals()
            .set("player", state)
            .expect("expose player state");
        lua.load(
            r#"
local options = player:GetPlayerOptions("ModsLevel_Song")
options:FromString("*7 50% Drunk, Shuffle, C500, *2 75% Space")
options:FromString("25% Mini")
assert(options:Drunk() == 0.5 and options:Mini() == 0.25)
assert(options:Space() == 0.75 and options:Overhead() == false)
player:SetPlayerOptions("ModsLevel_Song", "*3 75% Reverse")
assert(options == player:GetPlayerOptions("ModsLevel_Song"))
assert(options:Drunk() == 0 and options:Mini() == 0 and not options:Shuffle())
assert(options:Overhead() == true and options:Space() == nil)
assert(options:XMod() == 1 and options:CMod() == nil and options:Reverse() == 0.75)
player:SetPlayerOptions("ModsLevel_Song", "")
assert(options:Reverse() == 0 and options:XMod() == 1)
"#,
        )
        .exec()
        .expect("run native-style option assignment");
        let speeds = player_option_speeds(&lua, &options).expect("read approach speeds");
        for key in ["drunk", "reverse", "xmod", "cmod", "mmod", "tilt", "skew"] {
            assert_eq!(speeds.raw_get::<f32>(key).expect("recorded approach"), 1.0);
        }
    }

    #[test]
    fn modifier_prefix_order_matches_native() {
        let lua = Lua::new();
        let options =
            create_player_options_table(&lua, SongLuaPlayerContext::default()).expect("options");
        lua.globals().set("o", options).expect("expose options");
        // These assertions also run against the linked ITGmania PlayerOptions
        // binding in the native text/prefix probe, including negative speeds.
        lua.load(
            r#"
o:Tipsy(7)
o:FromString('0.8 0% Tipsy')
assert(o:Tipsy() == 0)
o:FromString('*2 10% *5 25% Drunk')
assert(o:Drunk() == 0.25 and select(2, o:Drunk()) == 5)
o:FromString('50% no Flip')
assert(o:Flip() == 0)
o:FromString('no 75% Flip')
assert(o:Flip() == 0.75)
o:FromString('*inf 25% Mini')
assert(o:Mini() == 0.25 and select(2, o:Mini()) == 1)
o:FromString('*-2 25% Mini')
assert(select(2, o:Mini()) == -2)
o:FromString('100ms Passmark')
assert(math.abs(o:Passmark() - 0.1) < 1e-6)
o:FromString('25* Drunk')
assert(o:Drunk() == 0.25)
o:FromString('.5 Drunk')
assert(o:Drunk() == 1)
"#,
        )
        .exec()
        .expect("native modifier prefixes");
    }

    #[test]
    fn speed_option_writes_preserve_shared_approach_speed() {
        let lua = Lua::new();
        let options = create_player_options_table(&lua, SongLuaPlayerContext::default()).unwrap();
        lua.globals().set("options", options.clone()).unwrap();
        for (script, expected) in [
            ("options:XMod(2.5)", 1.0),
            ("options:XMod(2.5, 10000)", 10000.0),
            ("options:CMod(300)", 10000.0),
            ("options:MMod(400, 0)", 0.0),
            ("options:XMod(1.5)", 0.0),
            ("options:FromString('*7 2x')", 7.0),
            ("options:FromString('C500')", 1.0),
        ] {
            lua.load(script).exec().unwrap();
            let speeds = player_option_speeds(&lua, &options).unwrap();
            for key in ["xmod", "cmod", "mmod"] {
                assert_eq!(
                    speeds.raw_get::<f32>(key).unwrap(),
                    expected,
                    "{script}: {key}"
                );
            }
        }
    }

    #[test]
    fn boolean_options_match_native_binding() {
        let methods = SONG_LUA_PLAYER_OPTION_CAPABILITIES
            .iter()
            .copied()
            .filter(|name| {
                *name != "Overhead" && player_option_uses_bool(&name.to_ascii_lowercase())
            })
            .collect::<Vec<_>>();
        assert_eq!(methods.len(), 40, "cover every native BOOL_INTERFACE");
        for method in methods {
            let key = method.to_ascii_lowercase();
            let lua = Lua::new();
            let options = create_player_options_table(&lua, SongLuaPlayerContext::default())
                .expect("create options");
            lua.globals()
                .set("o", options.clone())
                .expect("expose options");
            lua.load(
                r#"
assert(o:StealthType() == false and select('#', o:StealthType()) == 1)
assert(o:StealthType(true) == false and o:StealthType() == true)
for _, value in ipairs({0, 1, 'true', 'false', {}}) do
    assert(o:StealthType(value) == true and o:StealthType() == true)
end
assert(o:StealthType(nil, false) == o)
assert(o:StealthType(false, 0, true) == true and o:StealthType() == false)
assert(o:StealthType(true, true) == o)
"#
                .replace("StealthType", method),
            )
            .exec()
            .expect("native boolean protocol");
            // Direct boolean writes have no approach speed, even with a numeric arg.
            let speeds = player_option_speeds(&lua, &options).expect("read speeds");
            lua.load(format!("o:{method}(true, -1)"))
                .exec()
                .expect("numeric arg is ignored");
            assert_eq!(
                speeds.get::<Option<f32>>(key).expect("no approach speed"),
                None
            );
        }
    }

    #[test]
    fn boolean_option_strings_use_native_half_threshold() {
        let lua = Lua::new();
        let options = create_player_options_table(&lua, SongLuaPlayerContext::default())
            .expect("create option table");
        lua.globals()
            .set("options", options)
            .expect("expose options");
        lua.load(
            r#"
options:FromString('50% stealthpastreceptors')
assert(options:StealthPastReceptors() == false)
options:FromString('51% stealthpastreceptors')
assert(options:StealthPastReceptors() == true)
options:FromString('-50% stealthpastreceptors')
assert(options:StealthPastReceptors() == false)
options:FromString('25% noholds')
assert(options:NoHolds() == false)
options:FromString('51% noholds')
assert(options:NoHolds() == true)
"#,
        )
        .exec()
        .expect("native boolean threshold");
    }

    #[test]
    fn dynamic_player_option_methods_are_cached_after_first_lookup() {
        let lua = Lua::new();
        let options = create_player_options_table(&lua, SongLuaPlayerContext::default()).unwrap();
        assert!(
            options
                .raw_get::<Option<Function>>("Drunk")
                .unwrap()
                .is_none()
        );

        let first = options.get::<Function>("Drunk").unwrap();
        let cached = options.raw_get::<Function>("Drunk").unwrap();
        assert_eq!(first.to_pointer(), cached.to_pointer());

        first.call::<Value>((options.clone(), 0.5_f32)).unwrap();
        assert_eq!(player_option_number(&lua, &options, "drunk").unwrap(), 0.5);
    }
}
