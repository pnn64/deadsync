use std::path::Path;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::files::{file_path_string, theme_path};
use crate::lua_util::{create_owned_string_array, create_string_array, lua_text_value, method_arg};
use crate::runtime::note_song_lua_side_effect;
use crate::song_tables::{create_song_table, create_steps_table};
use crate::values::{
    lua_values_equal, player_index_from_value, player_number_name, read_boolish, read_f32,
    read_i32_value, read_string,
};
use crate::version::version_parts;
use crate::{
    GRAPH_DISPLAY_VALUE_RESOLUTION, LUA_PLAYERS, SONG_LUA_INITIAL_LIFE, SONG_LUA_THEME_NAME,
    SONG_LUA_THEME_PATH_PREFIX, SongLuaCompileContext, SongLuaDifficulty, SongLuaPlayerContext,
    song_lua_arch_name, song_lua_human_player_count, theme_has_string, theme_metric_bool,
    theme_metric_names, theme_metric_number_for_screen, theme_metric_value_for_human_players,
    theme_string, theme_string_names,
};

pub fn lua_table_to_string(args: &MultiValue) -> String {
    let name = args
        .get(1)
        .cloned()
        .and_then(read_string)
        .unwrap_or_else(|| "Value".to_string());
    match args.front() {
        Some(Value::Table(table)) if table.raw_len() == 0 => format!("{name} = {{}}"),
        Some(Value::Table(table)) => format!("{name} = {{...{} item(s)}}", table.raw_len()),
        Some(value) => format!(
            "{name} = {}",
            lua_text_value(value.clone()).unwrap_or_default()
        ),
        None => format!("{name} = nil"),
    }
}

pub fn create_version_parts_table(lua: &Lua, version: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, part) in version_parts(version).into_iter().enumerate() {
        table.raw_set(index + 1, part)?;
    }
    Ok(table)
}

pub fn create_display_bpms_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, bpms[0])?;
    table.raw_set(2, bpms[1])?;
    Ok(table)
}

pub fn create_radar_values_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}

pub fn create_song_group_table(lua: &Lua) -> mlua::Result<Table> {
    let group = lua.create_table()?;
    group.set(
        "GetSyncOffset",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(group)
}

pub fn create_single_value_array(lua: &Lua, value: Table) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(1, value)?;
    Ok(table)
}

pub fn set_string_method(lua: &Lua, table: &Table, name: &str, value: &str) -> mlua::Result<()> {
    let value = value.to_string();
    table.set(
        name,
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&value)?))
        })?,
    )
}

pub fn create_difficulty_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
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
        table.raw_set(idx + 1, difficulty.sm_name())?;
        reverse.raw_set(difficulty.sm_name(), idx)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

pub fn create_string_enum_table(lua: &Lua, names: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for (idx, name) in names.iter().enumerate() {
        table.raw_set(idx + 1, *name)?;
        reverse.raw_set(*name, idx)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

pub fn create_player_number_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let reverse = lua.create_table()?;
    for player in 0..LUA_PLAYERS {
        let name = player_number_name(player);
        table.raw_set(player + 1, name)?;
        table.raw_set(name, name)?;
        reverse.raw_set(name, player)?;
    }
    table.set(
        "Reverse",
        lua.create_function(move |_, _args: MultiValue| Ok(reverse.clone()))?,
    )?;
    Ok(table)
}

pub fn create_other_player_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set(player_number_name(0), player_number_name(1))?;
    table.raw_set(player_number_name(1), player_number_name(0))?;
    Ok(table)
}

pub fn create_screen_system_layer_helpers_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetCreditsMessage", "Free Play")?;
    Ok(table)
}

pub fn create_branch_table(lua: &Lua) -> mlua::Result<Table> {
    let branch = lua.create_table()?;
    for (name, screen) in [
        ("AfterScreenRankingDouble", "ScreenRainbow"),
        ("TitleMenu", "ScreenTitleMenu"),
        ("AfterInit", "ScreenTitleMenu"),
        ("AllowScreenSelectProfile", "ScreenSelectProfile"),
        ("AfterSelectProfile", "ScreenSelectColor"),
        ("AllowScreenSelectColor", "ScreenSelectColor"),
        ("AfterScreenSelectColor", "ScreenSelectStyle"),
        ("AllowScreenSelectPlayMode", "ScreenSelectPlayMode"),
        ("AllowScreenSelectPlayMode2", "ScreenProfileLoad"),
        ("AfterSelectPlayMode", "ScreenSelectMusic"),
        ("AfterEvaluationStage", "ScreenProfileSave"),
        ("AfterGameplay", "ScreenEvaluationStage"),
        ("AfterHeartEntry", "ScreenEvaluationStage"),
        ("AfterSelectMusic", "ScreenGameplay"),
        ("SSMCancel", "ScreenTitleMenu"),
        ("AllowScreenNameEntry", "ScreenNameEntryTraditional"),
        ("AllowScreenEvalSummary", "ScreenEvaluationSummary"),
        ("AfterProfileSave", "ScreenSelectMusic"),
        ("AfterProfileSaveSummary", "ScreenGameOver"),
        ("GameplayScreen", "ScreenGameplay"),
    ] {
        branch.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(screen)?))
            })?,
        )?;
    }
    Ok(branch)
}

pub fn scale_value(value: f32, from_low: f32, from_high: f32, to_low: f32, to_high: f32) -> f32 {
    let span = from_high - from_low;
    if span.abs() <= f32::EPSILON {
        to_low
    } else {
        (value - from_low) / span * (to_high - to_low) + to_low
    }
}

fn seconds_to_time_parts(seconds: f64) -> (i64, i64, i64) {
    let minutes = (seconds / 60.0).trunc() as i64;
    let secs = (seconds % 60.0).trunc() as i64;
    let centis = ((seconds - (minutes * 60 + secs) as f64) * 100.0).trunc() as i64;
    let centis = centis.clamp(0, 99);
    (minutes, secs, centis)
}

pub fn seconds_to_hhmmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{:02}:{:02}:{seconds:02}", minutes / 60, minutes % 60)
}

pub fn seconds_to_mss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{minutes:01}:{seconds:02}")
}

pub fn seconds_to_mmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format!("{minutes:02}:{seconds:02}")
}

pub fn seconds_to_mss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format!("{minutes:01}:{seconds:02}.{centis:02}")
}

pub fn seconds_to_mmss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

pub fn format_number_and_suffix(value: i64) -> String {
    let suffix = if (value % 100) / 10 == 1 {
        "th"
    } else {
        match value % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{value}{suffix}")
}

pub fn create_game_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    set_string_method(lua, &table, "GetName", "dance")?;
    Ok(table)
}

pub fn create_gameman_table(lua: &Lua) -> mlua::Result<Table> {
    let gameman = lua.create_table()?;
    gameman.set(
        "GetStylesForGame",
        lua.create_function(|lua, _args: MultiValue| {
            let styles = lua.create_table()?;
            styles.raw_set(1, create_style_table(lua, "single")?)?;
            Ok(styles)
        })?,
    )?;
    Ok(gameman)
}

pub fn create_charman_table(lua: &Lua) -> mlua::Result<Table> {
    let charman = lua.create_table()?;
    charman.set(
        "GetAllCharacters",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    charman.set(
        "GetCharacterCount",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    for method in ["GetCharacter", "GetDefaultCharacter", "GetRandomCharacter"] {
        charman.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    Ok(charman)
}

pub fn create_theme_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let theme = lua.create_table()?;
    let human_player_count = song_lua_human_player_count(context);
    let screen_height = context.screen_height.max(1.0);
    set_string_method(lua, &theme, "GetCurThemeName", SONG_LUA_THEME_NAME)?;
    set_string_method(lua, &theme, "GetThemeDisplayName", SONG_LUA_THEME_NAME)?;
    set_string_method(lua, &theme, "GetCurLanguage", "en")?;
    set_string_method(
        lua,
        &theme,
        "GetCurrentThemeDirectory",
        SONG_LUA_THEME_PATH_PREFIX,
    )?;
    set_string_method(lua, &theme, "GetThemeAuthor", "")?;
    theme.set(
        "GetNumSelectableThemes",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    for method in ["DoesThemeExist", "IsThemeSelectable"] {
        theme.set(
            method,
            lua.create_function(|_, args: MultiValue| {
                let name = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                Ok(name.eq_ignore_ascii_case(SONG_LUA_THEME_NAME))
            })?,
        )?;
    }
    theme.set(
        "DoesLanguageExist",
        lua.create_function(|_, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(matches!(
                name.to_ascii_lowercase().as_str(),
                "en" | "english"
            ))
        })?,
    )?;
    theme.set(
        "get_theme_fallback_list",
        lua.create_function(|lua, _args: MultiValue| {
            create_string_array(lua, &[SONG_LUA_THEME_NAME])
        })?,
    )?;
    theme.set(
        "RunLuaScripts",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    theme.set(
        "GetMetric",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            theme_metric_value_for_human_players(
                lua,
                &group,
                &name,
                human_player_count,
                screen_height,
            )
        })?,
    )?;
    theme.set(
        "GetMetricF",
        lua.create_function(move |_, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(
                theme_metric_number_for_screen(&group, &name, human_player_count, screen_height)
                    .map_or(Value::Nil, |value| Value::Number(value as f64)),
            )
        })?,
    )?;
    theme.set(
        "GetMetricI",
        lua.create_function(move |_, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            Ok(
                theme_metric_number_for_screen(&group, &name, human_player_count, screen_height)
                    .map_or(Value::Nil, |value| Value::Integer(value.round() as i64)),
            )
        })?,
    )?;
    theme.set(
        "GetMetricB",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(theme_metric_bool(theme_metric_value_for_human_players(
                lua,
                &group,
                &name,
                human_player_count,
                screen_height,
            )?))
        })?,
    )?;
    theme.set(
        "HasMetric",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(!matches!(
                theme_metric_value_for_human_players(
                    lua,
                    &group,
                    &name,
                    human_player_count,
                    screen_height,
                )?,
                Value::Nil
            ))
        })?,
    )?;
    theme.set(
        "GetMetricNamesInGroup",
        lua.create_function(|lua, args: MultiValue| {
            let Some(group) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let names = theme_metric_names(&group);
            if names.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(Value::Table(create_owned_string_array(lua, &names)?))
            }
        })?,
    )?;
    theme.set(
        "GetStringNamesInGroup",
        lua.create_function(|lua, args: MultiValue| {
            let Some(section) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let names = theme_string_names(&section);
            if names.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(Value::Table(create_owned_string_array(lua, &names)?))
            }
        })?,
    )?;
    theme.set(
        "GetPathInfoB",
        lua.create_function(|lua, args: MultiValue| {
            let group = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let name = method_arg(&args, 1)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok((
                lua.create_string(theme_path("B", &group, &name))?,
                lua.create_string(&group)?,
                lua.create_string(&name)?,
            ))
        })?,
    )?;
    for (method_name, kind) in [
        ("GetPathB", "B"),
        ("GetPathF", "F"),
        ("GetPathG", "G"),
        ("GetPathO", "O"),
        ("GetPathS", "S"),
    ] {
        theme.set(
            method_name,
            lua.create_function({
                let kind = kind.to_string();
                move |lua, args: MultiValue| {
                    let group = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_string)
                        .unwrap_or_default();
                    let name = method_arg(&args, 1)
                        .cloned()
                        .and_then(read_string)
                        .unwrap_or_default();
                    Ok(Value::String(
                        lua.create_string(theme_path(&kind, &group, &name))?,
                    ))
                }
            })?,
        )?;
    }
    theme.set(
        "HasString",
        lua.create_function(|_, args: MultiValue| {
            let Some(section) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(theme_has_string(&section, &name))
        })?,
    )?;
    theme.set(
        "GetString",
        lua.create_function(|lua, args: MultiValue| {
            let Some(section) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::String(lua.create_string("")?));
            };
            let Some(name) = method_arg(&args, 1).cloned().and_then(read_string) else {
                return Ok(Value::String(lua.create_string("")?));
            };
            Ok(Value::String(
                lua.create_string(theme_string(&section, &name))?,
            ))
        })?,
    )?;
    theme.set(
        "GetSelectableThemeNames",
        lua.create_function(|lua, _args: MultiValue| {
            let names = lua.create_table()?;
            names.raw_set(1, SONG_LUA_THEME_NAME)?;
            Ok(names)
        })?,
    )?;
    for name in ["ReloadMetrics", "SetTheme"] {
        theme.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(theme)
}

pub fn create_profileman_table(lua: &Lua) -> mlua::Result<Table> {
    let profileman = lua.create_table()?;
    profileman.set("__songlua_stats_prefix", "")?;
    let machine_profile = create_profile_table(lua, "Machine")?;
    let player_profiles = [
        create_profile_table(lua, "Player 1")?,
        create_profile_table(lua, "Player 2")?,
    ];
    let fallback_profile = create_profile_table(lua, "Player")?;
    let local_profile = create_profile_table(lua, "Local Profile")?;
    profileman.set(
        "GetMachineProfile",
        lua.create_function({
            let machine_profile = machine_profile.clone();
            move |_, _args: MultiValue| Ok(machine_profile.clone())
        })?,
    )?;
    profileman.set(
        "GetProfile",
        lua.create_function({
            let machine_profile = machine_profile.clone();
            let player_profiles = player_profiles.clone();
            let fallback_profile = fallback_profile.clone();
            move |_, args: MultiValue| {
                let profile = match method_arg(&args, 0).and_then(profile_slot_from_value) {
                    Some(SongLuaProfileSlot::Player(index)) => player_profiles[index].clone(),
                    Some(SongLuaProfileSlot::Machine) => machine_profile.clone(),
                    None => fallback_profile.clone(),
                };
                Ok(profile)
            }
        })?,
    )?;
    profileman.set(
        "GetProfileDir",
        lua.create_function(|lua, args: MultiValue| {
            let dir = match method_arg(&args, 0).and_then(profile_slot_from_value) {
                Some(SongLuaProfileSlot::Machine) => "/Save/MachineProfile/",
                _ => "",
            };
            Ok(Value::String(lua.create_string(dir)?))
        })?,
    )?;
    profileman.set(
        "GetPlayerName",
        lua.create_function(|lua, args: MultiValue| {
            let name = method_arg(&args, 0)
                .and_then(player_index_from_value)
                .map(|index| format!("Player {}", index + 1))
                .unwrap_or_default();
            Ok(Value::String(lua.create_string(&name)?))
        })?,
    )?;
    profileman.set(
        "IsPersistentProfile",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    profileman.set(
        "GetNumLocalProfiles",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    profileman.set(
        "LocalProfileIDToDir",
        lua.create_function(|lua, args: MultiValue| {
            let id = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::String(
                lua.create_string(format!("/Save/LocalProfiles/{id}/"))?,
            ))
        })?,
    )?;
    profileman.set(
        "GetLocalProfileIDFromIndex",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    profileman.set(
        "GetLocalProfileFromIndex",
        lua.create_function({
            let local_profile = local_profile.clone();
            move |_, _args: MultiValue| Ok(local_profile.clone())
        })?,
    )?;
    profileman.set(
        "GetLocalProfile",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    profileman.set(
        "GetLocalProfileIndexFromID",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    for method in ["GetLocalProfileIDs", "GetLocalProfileDisplayNames"] {
        profileman.set(
            method,
            lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
        )?;
    }
    for method in [
        "IsSongNew",
        "ProfileWasLoadedFromMemoryCard",
        "LastLoadWasTamperedOrCorrupt",
        "ProfileFromMemoryCardIsNew",
    ] {
        profileman.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(false))?,
        )?;
    }
    profileman.set(
        "GetSongNumTimesPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    profileman.set(
        "SaveMachineProfile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    for method in ["SaveProfile", "SaveLocalProfile"] {
        profileman.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(false)
            })?,
        )?;
    }
    profileman.set(
        "GetStatsPrefix",
        lua.create_function({
            let profileman = profileman.clone();
            move |lua, _args: MultiValue| {
                let prefix = profileman
                    .get::<Option<String>>("__songlua_stats_prefix")?
                    .unwrap_or_default();
                Ok(Value::String(lua.create_string(&prefix)?))
            }
        })?,
    )?;
    profileman.set(
        "SetStatsPrefix",
        lua.create_function({
            let profileman = profileman.clone();
            move |lua, args: MultiValue| {
                let prefix = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                profileman.set("__songlua_stats_prefix", prefix)?;
                note_song_lua_side_effect(lua)?;
                Ok(profileman.clone())
            }
        })?,
    )?;
    Ok(profileman)
}

#[derive(Clone, Copy)]
enum SongLuaProfileSlot {
    Player(usize),
    Machine,
}

fn profile_slot_from_value(value: &Value) -> Option<SongLuaProfileSlot> {
    if let Some(index) = player_index_from_value(value) {
        return Some(SongLuaProfileSlot::Player(index));
    }
    match value {
        Value::Integer(2) => return Some(SongLuaProfileSlot::Machine),
        Value::Number(value) if *value == 2.0 => return Some(SongLuaProfileSlot::Machine),
        _ => {}
    }
    let text = match value {
        Value::String(text) => text.to_str().ok()?,
        _ => return None,
    };
    match text.as_ref() {
        "ProfileSlot_Player1" => Some(SongLuaProfileSlot::Player(0)),
        "ProfileSlot_Player2" => Some(SongLuaProfileSlot::Player(1)),
        "ProfileSlot_Machine" => Some(SongLuaProfileSlot::Machine),
        _ => None,
    }
}

fn create_profile_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let profile = lua.create_table()?;
    profile.set("__songlua_display_name", name)?;
    profile.set("__songlua_last_score_name", name)?;
    profile.set("__songlua_guid", "")?;
    profile.set("__songlua_profile_dir", "")?;
    profile.set("__songlua_weight_pounds", 0.0_f32)?;
    profile.set("__songlua_voomax", 0.0_f32)?;
    profile.set("__songlua_birth_year", 0.0_f32)?;
    profile.set("__songlua_ignore_step_count_calories", false)?;
    profile.set("__songlua_is_male", true)?;
    profile.set("__songlua_goal_type", 0_i64)?;
    profile.set("__songlua_goal_calories", 0.0_f32)?;
    profile.set("__songlua_goal_seconds", 0.0_f32)?;
    profile.set("__songlua_calories_today", 0.0_f32)?;
    profile.set("__songlua_total_calories", 0.0_f32)?;
    profile.set("__songlua_user_table", lua.create_table()?)?;

    set_state_string_getter(lua, &profile, "GetDisplayName", "__songlua_display_name")?;
    set_state_string_setter(lua, &profile, "SetDisplayName", "__songlua_display_name")?;
    set_state_string_getter(lua, &profile, "GetGUID", "__songlua_guid")?;
    set_state_string_getter(lua, &profile, "GetProfileDir", "__songlua_profile_dir")?;
    set_state_string_getter(
        lua,
        &profile,
        "GetLastUsedHighScoreName",
        "__songlua_last_score_name",
    )?;
    set_state_string_setter(
        lua,
        &profile,
        "SetLastUsedHighScoreName",
        "__songlua_last_score_name",
    )?;

    set_string_method(lua, &profile, "GetType", "ProfileType_Normal")?;
    profile.set(
        "GetPriority",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    profile.set(
        "AddScreenshot",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;
    profile.set(
        "GetAllUsedHighScoreNames",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                let names = lua.create_table()?;
                let name = profile
                    .get::<Option<String>>("__songlua_last_score_name")?
                    .unwrap_or_default();
                if !name.is_empty() {
                    names.raw_set(1, name)?;
                }
                Ok(names)
            }
        })?,
    )?;
    profile.set(
        "GetCharacter",
        lua.create_function({
            let profile = profile.clone();
            move |_, _args: MultiValue| profile.get::<Value>("__songlua_character")
        })?,
    )?;
    profile.set(
        "SetCharacter",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let character = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                profile.set("__songlua_character", character)?;
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;
    set_state_number_getter(lua, &profile, "GetWeightPounds", "__songlua_weight_pounds")?;
    set_state_number_setter(lua, &profile, "SetWeightPounds", "__songlua_weight_pounds")?;
    set_state_number_getter(lua, &profile, "GetVoomax", "__songlua_voomax")?;
    set_state_number_setter(lua, &profile, "SetVoomax", "__songlua_voomax")?;
    set_state_number_getter(lua, &profile, "GetBirthYear", "__songlua_birth_year")?;
    set_state_number_setter(lua, &profile, "SetBirthYear", "__songlua_birth_year")?;
    set_state_bool_getter(
        lua,
        &profile,
        "GetIgnoreStepCountCalories",
        "__songlua_ignore_step_count_calories",
    )?;
    set_state_bool_setter(
        lua,
        &profile,
        "SetIgnoreStepCountCalories",
        "__songlua_ignore_step_count_calories",
    )?;
    set_state_bool_getter(lua, &profile, "GetIsMale", "__songlua_is_male")?;
    set_state_bool_setter(lua, &profile, "SetIsMale", "__songlua_is_male")?;
    set_state_number_getter(lua, &profile, "GetGoalCalories", "__songlua_goal_calories")?;
    set_state_number_setter(lua, &profile, "SetGoalCalories", "__songlua_goal_calories")?;
    set_state_number_getter(lua, &profile, "GetGoalSeconds", "__songlua_goal_seconds")?;
    set_state_number_setter(lua, &profile, "SetGoalSeconds", "__songlua_goal_seconds")?;
    profile.set(
        "GetGoalType",
        lua.create_function({
            let profile = profile.clone();
            move |_, _args: MultiValue| profile.get::<Value>("__songlua_goal_type")
        })?,
    )?;
    profile.set(
        "SetGoalType",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let goal = method_arg(&args, 0).cloned().unwrap_or(Value::Integer(0));
                profile.set("__songlua_goal_type", goal)?;
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;

    for method in [
        "GetAge",
        "GetTotalNumSongsPlayed",
        "GetNumTotalSongsPlayed",
        "GetTotalSessions",
        "GetTotalSessionSeconds",
        "GetTotalGameplaySeconds",
        "GetSongNumTimesPlayed",
        "GetNumToasties",
        "GetTotalTapsAndHolds",
        "GetTotalJumps",
        "GetTotalHolds",
        "GetTotalRolls",
        "GetTotalMines",
        "GetTotalHands",
        "GetTotalLifts",
        "GetTotalDancePoints",
        "GetTotalStepsWithTopGrade",
        "GetTotalTrailsWithTopGrade",
    ] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
        )?;
    }
    for method in [
        "CalculateCaloriesFromHeartRate",
        "GetSongsActual",
        "GetCoursesActual",
        "GetSongsPossible",
        "GetCoursesPossible",
        "GetSongsPercentComplete",
        "GetCoursesPercentComplete",
        "GetSongsAndCoursesPercentCompleteAllDifficulties",
    ] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
        )?;
    }
    for method in ["IsCodeUnlocked", "HasPassedAnyStepsInSong"] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(false))?,
        )?;
    }
    for method in [
        "GetMostPopularSong",
        "GetMostPopularCourse",
        "GetLastPlayedSong",
        "GetLastPlayedCourse",
    ] {
        profile.set(
            method,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    set_state_number_getter(
        lua,
        &profile,
        "GetCaloriesBurnedToday",
        "__songlua_calories_today",
    )?;
    set_state_number_getter(
        lua,
        &profile,
        "GetTotalCaloriesBurned",
        "__songlua_total_calories",
    )?;
    profile.set(
        "GetDisplayTotalCaloriesBurned",
        lua.create_function({
            let profile = profile.clone();
            move |lua, _args: MultiValue| {
                let calories = profile
                    .get::<Option<f32>>("__songlua_total_calories")?
                    .unwrap_or(0.0)
                    .max(0.0) as i64;
                Ok(Value::String(lua.create_string(format!("{calories} Cal"))?))
            }
        })?,
    )?;
    profile.set(
        "AddCaloriesToDailyTotal",
        lua.create_function({
            let profile = profile.clone();
            move |lua, args: MultiValue| {
                let calories = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let today = profile
                    .get::<Option<f32>>("__songlua_calories_today")?
                    .unwrap_or(0.0)
                    + calories;
                let total = profile
                    .get::<Option<f32>>("__songlua_total_calories")?
                    .unwrap_or(0.0)
                    + calories;
                profile.set("__songlua_calories_today", today)?;
                profile.set("__songlua_total_calories", total)?;
                note_song_lua_side_effect(lua)?;
                Ok(profile.clone())
            }
        })?,
    )?;
    profile.set(
        "GetHighScoreList",
        lua.create_function(|lua, _args: MultiValue| create_high_score_list_table(lua))?,
    )?;
    profile.set(
        "GetHighScoreListIfExists",
        lua.create_function(|lua, _args: MultiValue| create_high_score_list_table(lua))?,
    )?;
    profile.set(
        "GetCategoryHighScoreList",
        lua.create_function(|lua, _args: MultiValue| create_high_score_list_table(lua))?,
    )?;
    profile.set(
        "GetUserTable",
        lua.create_function({
            let profile = profile.clone();
            move |_, _args: MultiValue| profile.get::<Table>("__songlua_user_table")
        })?,
    )?;
    profile.set(
        "get_songs",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    Ok(profile)
}

fn set_state_string_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, _args: MultiValue| {
                let value = table.get::<Option<String>>(key)?.unwrap_or_default();
                Ok(Value::String(lua.create_string(&value)?))
            }
        })?,
    )
}

fn set_state_string_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

fn set_state_number_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |_, _args: MultiValue| Ok(table.get::<Option<f32>>(key)?.unwrap_or(0.0))
        })?,
    )
}

fn set_state_number_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

fn set_state_bool_getter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |_, _args: MultiValue| Ok(table.get::<Option<bool>>(key)?.unwrap_or(false))
        })?,
    )
}

fn set_state_bool_setter(
    lua: &Lua,
    table: &Table,
    method: &'static str,
    key: &'static str,
) -> mlua::Result<()> {
    table.set(
        method,
        lua.create_function({
            let table = table.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(false);
                table.set(key, value)?;
                note_song_lua_side_effect(lua)?;
                Ok(table.clone())
            }
        })?,
    )
}

pub fn create_statsman_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let statsman = lua.create_table()?;
    let stage_stats = create_stage_stats_table(lua, context)?;
    for name in [
        "GetCurStageStats",
        "GetAccumPlayedStageStats",
        "GetFinalEvalStageStats",
    ] {
        statsman.set(
            name,
            lua.create_function({
                let stage_stats = stage_stats.clone();
                move |_, _args: MultiValue| Ok(stage_stats.clone())
            })?,
        )?;
    }
    statsman.set(
        "GetPlayedStageStats",
        lua.create_function({
            let stage_stats = stage_stats.clone();
            move |_, args: MultiValue| {
                let ago = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(1);
                Ok(if ago == 1 {
                    Value::Table(stage_stats.clone())
                } else {
                    Value::Nil
                })
            }
        })?,
    )?;
    statsman.set("Reset", lua.create_function(|_, _args: MultiValue| Ok(()))?)?;
    statsman.set(
        "GetStagesPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    for name in [
        "GetFinalGrade",
        "GetBestGrade",
        "GetWorstGrade",
        "GetBestFinalGrade",
    ] {
        set_string_method(lua, &statsman, name, "Grade_Tier07")?;
    }
    Ok(statsman)
}

fn create_stage_stats_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let stage_stats = lua.create_table()?;
    let player_stats = [
        create_player_stage_stats_table(
            lua,
            "Player",
            context.players[0].clone(),
            context.song_dir.as_path(),
        )?,
        create_player_stage_stats_table(
            lua,
            "Player",
            context.players[1].clone(),
            context.song_dir.as_path(),
        )?,
    ];
    let multi_player_stats = create_player_stage_stats_table(
        lua,
        "MultiPlayer",
        context.players[0].clone(),
        context.song_dir.as_path(),
    )?;
    let song = create_song_table(lua, context)?;
    stage_stats.set(
        "GetPlayerStageStats",
        lua.create_function(move |_, args: MultiValue| {
            let index = method_arg(&args, 0)
                .and_then(player_index_from_value)
                .unwrap_or(0);
            Ok(player_stats[index].clone())
        })?,
    )?;
    stage_stats.set(
        "GetMultiPlayerStageStats",
        lua.create_function(move |_, _args: MultiValue| Ok(multi_player_stats.clone()))?,
    )?;
    for name in ["GetPlayedSongs", "GetPossibleSongs"] {
        stage_stats.set(
            name,
            lua.create_function({
                let song = song.clone();
                move |lua, _args: MultiValue| {
                    let table = lua.create_table()?;
                    table.raw_set(1, song.clone())?;
                    Ok(table)
                }
            })?,
        )?;
    }
    stage_stats.set(
        "__songlua_total_steps_seconds",
        context.music_length_seconds.max(0.0),
    )?;
    stage_stats.set(
        "GetGameplaySeconds",
        lua.create_function({
            let total_seconds = context.music_length_seconds.max(0.0);
            move |_, _args: MultiValue| Ok(total_seconds)
        })?,
    )?;
    stage_stats.set(
        "GetTotalPossibleStepsSeconds",
        lua.create_function({
            let total_seconds = context.music_length_seconds.max(0.0);
            move |_, _args: MultiValue| Ok(total_seconds)
        })?,
    )?;
    stage_stats.set(
        "GetStepsSeconds",
        lua.create_function({
            let total_seconds = context.music_length_seconds.max(0.0);
            move |_, _args: MultiValue| Ok(total_seconds)
        })?,
    )?;
    stage_stats.set(
        "GetStageIndex",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    set_string_method(lua, &stage_stats, "GetStage", "Stage_1st")?;
    stage_stats.set(
        "AllFailed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stage_stats.set(
        "OnePassed",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    stage_stats.set(
        "PlayerHasHighScore",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stage_stats.set(
        "GetEarnedExtraStage",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stage_stats.set(
        "GaveUp",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(stage_stats)
}

fn create_player_stage_stats_table(
    lua: &Lua,
    high_score_name: &str,
    player: SongLuaPlayerContext,
    song_dir: &Path,
) -> mlua::Result<Table> {
    let stats = lua.create_table()?;
    stats.set("__songlua_failed", false)?;
    stats.set("__songlua_score", 0_i64)?;
    stats.set("__songlua_cur_max_score", 0_i64)?;
    stats.set("__songlua_actual_dance_points", 0_i64)?;
    stats.set("__songlua_possible_dance_points", 1_i64)?;
    let high_score = create_high_score_table(lua, high_score_name)?;
    let played_steps = lua.create_table()?;
    let possible_steps = lua.create_table()?;
    let steps = create_steps_table(lua, player.difficulty, player.display_bpms, song_dir)?;
    played_steps.raw_set(1, steps.clone())?;
    possible_steps.raw_set(1, steps)?;
    stats.set(
        "GetCaloriesBurned",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetNumControllerSteps",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetSurvivalSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetCurrentScoreMultiplier",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    stats.set(
        "GetCurMaxScore",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_cur_max_score")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    set_string_method(lua, &stats, "GetGrade", "Grade_Tier07")?;
    stats.set(
        "GetFailed",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<bool>>("__songlua_failed")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    stats.set(
        "FailPlayer",
        lua.create_function({
            let stats = stats.clone();
            move |lua, _args: MultiValue| {
                stats.set("__songlua_failed", true)?;
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "GetScore",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats.get::<Option<i64>>("__songlua_score")?.unwrap_or(0))
            }
        })?,
    )?;
    stats.set(
        "SetScore",
        lua.create_function({
            let stats = stats.clone();
            move |lua, args: MultiValue| {
                if let Some(score) = method_arg(&args, 0).cloned().and_then(read_i32_value)
                    && score >= 0
                {
                    stats.set("__songlua_score", score as i64)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "SetCurMaxScore",
        lua.create_function({
            let stats = stats.clone();
            move |lua, args: MultiValue| {
                if let Some(score) = method_arg(&args, 0).cloned().and_then(read_i32_value)
                    && score >= 0
                {
                    stats.set("__songlua_cur_max_score", score as i64)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "SetDancePointLimits",
        lua.create_function({
            let stats = stats.clone();
            move |lua, args: MultiValue| {
                let actual = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0)
                    .max(0) as i64;
                let possible = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(1)
                    .max(1) as i64;
                stats.set("__songlua_possible_dance_points", possible)?;
                stats.set("__songlua_actual_dance_points", actual.min(possible))?;
                note_song_lua_side_effect(lua)?;
                Ok(stats.clone())
            }
        })?,
    )?;
    stats.set(
        "GetPercentDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                let actual = stats
                    .get::<Option<i64>>("__songlua_actual_dance_points")?
                    .unwrap_or(0);
                let possible = stats
                    .get::<Option<i64>>("__songlua_possible_dance_points")?
                    .unwrap_or(1)
                    .max(1);
                Ok(actual as f64 / possible as f64)
            }
        })?,
    )?;
    stats.set(
        "GetActualDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_actual_dance_points")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    stats.set(
        "GetPossibleDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_possible_dance_points")?
                    .unwrap_or(1))
            }
        })?,
    )?;
    stats.set(
        "GetCurrentPossibleDancePoints",
        lua.create_function({
            let stats = stats.clone();
            move |_, _args: MultiValue| {
                Ok(stats
                    .get::<Option<i64>>("__songlua_possible_dance_points")?
                    .unwrap_or(1))
            }
        })?,
    )?;
    stats.set(
        "GetCurrentLife",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_INITIAL_LIFE))?,
    )?;
    stats.set(
        "GetCurrentCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetCurrentMissCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetLifeRemainingSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetAliveSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    stats.set(
        "GetLifeRecord",
        lua.create_function(|lua, args: MultiValue| {
            let samples = method_arg(&args, 1)
                .cloned()
                .and_then(read_i32_value)
                .unwrap_or(GRAPH_DISPLAY_VALUE_RESOLUTION as i32)
                .max(1) as usize;
            create_life_record_table(lua, samples, SONG_LUA_INITIAL_LIFE)
        })?,
    )?;
    stats.set(
        "GetTapNoteScores",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetHoldNoteScores",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "FullCombo",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "MaxCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetLessonScoreActual",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetLessonScoreNeeded",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetPlayedSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(played_steps.clone()))?,
    )?;
    stats.set(
        "GetPossibleSteps",
        lua.create_function(move |_, _args: MultiValue| Ok(possible_steps.clone()))?,
    )?;
    stats.set(
        "GetComboList",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    stats.set(
        "GetRadarActual",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 0.0))?,
    )?;
    stats.set(
        "GetRadarPossible",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 1.0))?,
    )?;
    stats.set(
        "GetRadarValues",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 0.0))?,
    )?;
    stats.set(
        "GetHighScore",
        lua.create_function(move |_, _args: MultiValue| Ok(high_score.clone()))?,
    )?;
    stats.set(
        "GetMachineHighScoreIndex",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    stats.set(
        "GetPersonalHighScoreIndex",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    set_string_method(lua, &stats, "GetStageAward", "StageAward_None")?;
    set_string_method(lua, &stats, "GetPeakComboAward", "PeakComboAward_None")?;
    stats.set(
        "IsDisqualified",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "GetPercentageOfTaps",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    set_string_method(
        lua,
        &stats,
        "GetBestFullComboTapNoteScore",
        "TapNoteScore_None",
    )?;
    stats.set(
        "FullComboOfScore",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    stats.set(
        "GetSongsPassed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    stats.set(
        "GetSongsPlayed",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    Ok(stats)
}

pub fn create_life_record_table(lua: &Lua, samples: usize, life: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let value = life.clamp(0.0, 1.0);
    for index in 1..=samples {
        table.raw_set(index, value)?;
    }
    Ok(table)
}

fn create_stage_radar_values_table(lua: &Lua, fallback: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetValue",
        lua.create_function(move |_, _args: MultiValue| Ok(fallback))?,
    )?;
    Ok(table)
}

fn create_high_score_list_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let high_scores = lua.create_table()?;
    high_scores.raw_set(1, create_high_score_table(lua, "Machine")?)?;
    let high_scores_for_lookup = high_scores.clone();
    table.set(
        "GetHighScores",
        lua.create_function({
            let high_scores = high_scores.clone();
            move |_, _args: MultiValue| Ok(high_scores.clone())
        })?,
    )?;
    table.set(
        "GetHighestScoreOfName",
        lua.create_function(move |_, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            for score in high_scores_for_lookup.sequence_values::<Table>() {
                let score = score?;
                let score_name = score
                    .get::<Function>("GetName")?
                    .call::<String>(score.clone())?;
                if score_name == name {
                    return Ok(Value::Table(score));
                }
            }
            Ok(Value::Nil)
        })?,
    )?;
    table.set(
        "GetRankOfName",
        lua.create_function(move |_, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(0_i64);
            };
            for (index, score) in high_scores.sequence_values::<Table>().enumerate() {
                let score = score?;
                let score_name = score
                    .get::<Function>("GetName")?
                    .call::<String>(score.clone())?;
                if score_name == name {
                    return Ok(index as i64 + 1);
                }
            }
            Ok(0_i64)
        })?,
    )?;
    Ok(table)
}

fn create_high_score_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let score = lua.create_table()?;
    set_string_method(lua, &score, "GetName", name)?;
    set_string_method(lua, &score, "GetGrade", "Grade_Tier07")?;
    set_string_method(lua, &score, "GetDate", "")?;
    set_string_method(lua, &score, "GetModifiers", "")?;
    score.set(
        "GetScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetPercentDP",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    score.set(
        "GetPercentDancePoints",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    score.set(
        "GetTapNoteScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetHoldNoteScore",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetMaxCombo",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    score.set(
        "GetSurvivalSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    score.set(
        "IsFillInMarker",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    score.set(
        "GetRadarValues",
        lua.create_function(|lua, _args: MultiValue| create_stage_radar_values_table(lua, 0.0))?,
    )?;
    set_string_method(lua, &score, "GetStageAward", "StageAward_None")?;
    set_string_method(lua, &score, "GetPeakComboAward", "PeakComboAward_None")?;
    Ok(score)
}

pub fn create_screen_table(
    lua: &Lua,
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("w", width)?;
    table.set("h", height)?;
    table.set("cx", center_x)?;
    table.set("cy", center_y)?;
    table.set("l", 0.0_f32)?;
    table.set("t", 0.0_f32)?;
    table.set("r", width)?;
    table.set("b", height)?;
    Ok(table)
}

pub fn create_prefsmgr_table(
    lua: &Lua,
    global_offset_seconds: f32,
    display_aspect_ratio: f32,
    display_width: i32,
    display_height: i32,
) -> mlua::Result<Table> {
    let prefsmgr = lua.create_table()?;
    let pref_store = lua.create_table()?;
    let get_pref_store = pref_store.clone();
    prefsmgr.set(
        "GetPreference",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let stored = get_pref_store.get::<Value>(key.as_str())?;
            if !matches!(stored, Value::Nil) {
                return Ok(stored);
            }
            prefsmgr_default_value(
                lua,
                &key,
                global_offset_seconds,
                display_aspect_ratio,
                display_width,
                display_height,
            )
        })?,
    )?;
    let set_pref_store = pref_store.clone();
    let prefsmgr_for_set = prefsmgr.clone();
    prefsmgr.set(
        "SetPreference",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(prefsmgr_for_set.clone());
            };
            if !matches!(
                prefsmgr_default_value(
                    lua,
                    &key,
                    global_offset_seconds,
                    display_aspect_ratio,
                    display_width,
                    display_height,
                )?,
                Value::Nil
            ) {
                let value = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
                set_pref_store.set(key, value)?;
            }
            Ok(prefsmgr_for_set.clone())
        })?,
    )?;
    prefsmgr.set(
        "PreferenceExists",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(!matches!(
                prefsmgr_default_value(
                    lua,
                    &key,
                    global_offset_seconds,
                    display_aspect_ratio,
                    display_width,
                    display_height,
                )?,
                Value::Nil
            ))
        })?,
    )?;
    let prefsmgr_for_save = prefsmgr.clone();
    prefsmgr.set(
        "SavePreferences",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(prefsmgr_for_save.clone())
        })?,
    )?;
    let reset_pref_store = pref_store;
    let prefsmgr_for_reset = prefsmgr.clone();
    prefsmgr.set(
        "SetPreferenceToDefault",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            if let Some(key) = method_arg(&args, 0).cloned().and_then(read_string) {
                reset_pref_store.set(key, Value::Nil)?;
            }
            Ok(prefsmgr_for_reset.clone())
        })?,
    )?;
    Ok(prefsmgr)
}

pub fn prefsmgr_default_value(
    lua: &Lua,
    key: &str,
    global_offset_seconds: f32,
    display_aspect_ratio: f32,
    display_width: i32,
    display_height: i32,
) -> mlua::Result<Value> {
    let lower = key.to_ascii_lowercase();
    if lower == "globaloffsetseconds" {
        Ok(Value::Number(global_offset_seconds as f64))
    } else if lower == "displayaspectratio" {
        Ok(Value::Number(display_aspect_ratio as f64))
    } else if lower == "displaywidth" {
        Ok(Value::Integer(display_width as i64))
    } else if lower == "displayheight" {
        Ok(Value::Integer(display_height as i64))
    } else if lower == "videorenderers" {
        Ok(Value::String(lua.create_string("opengl")?))
    } else if lower == "visualdelayseconds" {
        Ok(Value::Number(0.0))
    } else if lower == "bgbrightness" {
        Ok(Value::Number(1.0))
    } else if lower == "timingwindowscale" {
        Ok(Value::Number(1.0))
    } else if lower == "timingwindowadd" {
        Ok(Value::Number(0.0015))
    } else if lower.starts_with("timingwindowseconds") {
        Ok(Value::Number(0.0))
    } else if matches!(
        lower.as_str(),
        "autogengroupcourses"
            | "center1player"
            | "eastereggs"
            | "eventmode"
            | "harshhotlifepenalty"
            | "memorycards"
            | "menutimer"
            | "onlydedicatedmenubuttons"
            | "showbanners"
            | "shownativelanguage"
            | "threekeynavigation"
    ) {
        Ok(Value::Boolean(false))
    } else if matches!(
        lower.as_str(),
        "coinspercredit"
            | "customsongsloadtimeout"
            | "customsongsmaxmegabytes"
            | "customsongsmaxseconds"
            | "lifedifficultyscale"
            | "longversongseconds"
            | "marathonversongseconds"
            | "maxhighscoresperlistfor"
            | "maxhighscoresperlistformachine"
            | "maxregencomboaftermiss"
            | "mintnstoscorenotes"
            | "musicwheelswitchspeed"
            | "regencomboaftermiss"
            | "songsperplay"
            | "soundvolume"
            | "refreshrate"
    ) {
        let value = match lower.as_str() {
            "longversongseconds" => 150,
            "marathonversongseconds" => 300,
            "refreshrate" => 60,
            "maxhighscoresperlistfor" | "maxhighscoresperlistformachine" => 10,
            "songsperplay" | "soundvolume" => 1,
            _ => 0,
        };
        Ok(Value::Integer(value))
    } else if matches!(
        lower.as_str(),
        "coinmode"
            | "defaultmodifiers"
            | "editornoteskinp1"
            | "editornoteskinp2"
            | "displaymode"
            | "displayresolution"
            | "fullscreentype"
            | "httpallowhosts"
            | "premium"
    ) {
        let value = match lower.as_str() {
            "coinmode" => "CoinMode_Free",
            "displaymode" => "Windowed",
            "displayresolution" => "1920x1080",
            "editornoteskinp1" | "editornoteskinp2" => "default",
            "fullscreentype" => "Borderless",
            "premium" => "Premium_Off",
            _ => "",
        };
        Ok(Value::String(lua.create_string(value)?))
    } else if lower == "theme" {
        Ok(Value::String(lua.create_string(SONG_LUA_THEME_NAME)?))
    } else if lower.starts_with("defaultlocalprofileid") {
        Ok(Value::String(lua.create_string("")?))
    } else {
        Ok(Value::Nil)
    }
}

pub fn set_path_methods(
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

pub fn create_timing_table(lua: &Lua, bpms: [f32; 2]) -> mlua::Result<Table> {
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

pub fn create_style_table(lua: &Lua, style_name: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let style = crate::song_lua_style_info(style_name);
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
            info.set("Name", crate::song_lua_style_column_name(index))?;
            info.set("Track", index as i64)?;
            info.set("XOffset", crate::song_lua_style_column_x(style.name, index))?;
            Ok(info)
        })?,
    )?;
    Ok(table)
}

pub fn display_bpms_for_args(
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

pub fn create_split_table(lua: &Lua, text: &str, separator: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if separator.is_empty() {
        if text.is_empty() {
            table.raw_set(1, "")?;
        } else {
            for (idx, value) in text.chars().enumerate() {
                table.raw_set(idx + 1, value.to_string())?;
            }
        }
        return Ok(table);
    }
    for (idx, value) in text.split(separator).enumerate() {
        table.raw_set(idx + 1, value)?;
    }
    Ok(table)
}

pub fn create_range_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(mut start) = args.front().cloned().and_then(read_f32) else {
        return Ok(Value::Nil);
    };
    let stop = if let Some(stop) = args.get(1).cloned().and_then(read_f32) {
        stop
    } else {
        let stop = start;
        start = 1.0;
        stop
    };
    let mut step = args.get(2).cloned().and_then(read_f32).unwrap_or(1.0);
    if step.abs() <= f32::EPSILON {
        return Ok(Value::Table(lua.create_table()?));
    }
    if step > 0.0 && start > stop {
        step = -step;
    }
    if step < 0.0 && start < stop {
        return Ok(Value::Table(lua.create_table()?));
    }

    let table = lua.create_table()?;
    let mut index = 1;
    let mut value = start;
    while (step > 0.0 && value <= stop + f32::EPSILON)
        || (step < 0.0 && value >= stop - f32::EPSILON)
    {
        table.raw_set(index, lua_number_value(value))?;
        index += 1;
        value += step;
        if index > 10_000 {
            break;
        }
    }
    Ok(Value::Table(table))
}

pub fn stringify_lua_table(lua: &Lua, args: &MultiValue) -> mlua::Result<Value> {
    let Some(Value::Table(table)) = args.front().cloned() else {
        return Ok(Value::Nil);
    };
    let form = args.get(1).cloned().and_then(read_string);
    let format = lua
        .globals()
        .get::<Table>("string")?
        .get::<Function>("format")?;
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        let value = value?;
        let text = if let Some(form) = form.as_deref()
            && matches!(value, Value::Integer(_) | Value::Number(_))
        {
            let mut call_args = MultiValue::new();
            call_args.push_back(Value::String(lua.create_string(form)?));
            call_args.push_back(value);
            lua_text_value(format.call::<Value>(call_args)?)?
        } else {
            lua_text_value(value)?
        };
        out.raw_set(idx + 1, text)?;
    }
    Ok(Value::Table(out))
}

pub fn map_lua_table(lua: &Lua, function: &Function, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (idx, value) in table.sequence_values::<Value>().enumerate() {
        out.raw_set(idx + 1, function.call::<Value>(value?)?)?;
    }
    Ok(out)
}

pub fn deduplicate_lua_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let mut seen = Vec::new();
    let mut out_index = 1;
    for value in table.sequence_values::<Value>() {
        let value = value?;
        if seen.iter().any(|seen| lua_values_equal(seen, &value)) {
            continue;
        }
        seen.push(value.clone());
        out.raw_set(out_index, value)?;
        out_index += 1;
    }
    Ok(out)
}

pub fn rotate_lua_table(lua: &Lua, args: &MultiValue, left: bool) -> mlua::Result<Table> {
    let Some(Value::Table(input)) = args.front() else {
        return lua.create_table();
    };
    let len = input.raw_len();
    if len == 0 {
        return lua.create_table();
    }
    let shift = args.get(1).cloned().and_then(read_i32_value).unwrap_or(1) as i64;
    let len_i64 = len as i64;
    let out = lua.create_table()?;
    for index in 1..=len_i64 {
        let source = if left {
            (index + shift - 1).rem_euclid(len_i64) + 1
        } else {
            (index - shift - 1).rem_euclid(len_i64) + 1
        };
        out.raw_set(index, input.raw_get::<Value>(source)?)?;
    }
    Ok(out)
}

pub fn create_background_filter_values(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Off", 0)?;
    table.set("Dark", 50)?;
    table.set("Darker", 75)?;
    table.set("Darkest", 95)?;
    table.raw_set(0, 0)?;
    table.raw_set(50, 50)?;
    table.raw_set(75, 75)?;
    table.raw_set(95, 95)?;
    Ok(table)
}

pub fn create_gameplay_layout(
    lua: &Lua,
    screen_center_y: f32,
    reverse: bool,
) -> mlua::Result<Table> {
    let combo_y = screen_center_y + if reverse { -30.0 } else { 30.0 };
    let judgment_y = screen_center_y + if reverse { 30.0 } else { -30.0 };
    let sign = if reverse { -1.0 } else { 1.0 };
    let table = lua.create_table()?;
    table.set("Combo", create_layout_slot(lua, combo_y, None)?)?;
    table.set("ErrorBar", create_layout_slot(lua, judgment_y, Some(30.0))?)?;
    table.set(
        "MeasureCounter",
        create_layout_slot(lua, judgment_y + sign * 28.0, None)?,
    )?;
    table.set(
        "SubtractiveScoring",
        create_layout_slot(lua, judgment_y - sign * 28.0, None)?,
    )?;
    Ok(table)
}

pub fn create_credits_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Credits", 0)?;
    table.set("Remainder", 0)?;
    table.set("CoinsPerCredit", 1)?;
    Ok(table)
}

pub fn create_index_array(lua: &Lua, len: usize) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for index in 1..=len {
        table.raw_set(index, index)?;
    }
    Ok(table)
}

pub fn create_author_table(lua: &Lua, steps: Option<&Value>) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let Some(Value::Table(steps)) = steps else {
        return Ok(out);
    };
    let mut values = Vec::new();
    for method in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
        if let Some(text) = call_string_method(steps, method)? {
            if !text.is_empty() && !values.iter().any(|value| value == &text) {
                values.push(text);
            }
        }
    }
    for (index, value) in values.into_iter().enumerate() {
        out.raw_set(index + 1, value)?;
    }
    Ok(out)
}

pub fn create_ex_judgment_counts(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in [
        "W0",
        "W1",
        "W2",
        "W3",
        "W4",
        "W5",
        "Miss",
        "Hands",
        "Holds",
        "Mines",
        "Rolls",
        "totalHands",
        "totalHolds",
        "totalMines",
        "totalRolls",
    ] {
        table.set(name, 0_i64)?;
    }
    Ok(table)
}

pub fn create_network_response_table(lua: &Lua) -> mlua::Result<Table> {
    let response = lua.create_table()?;
    response.set("status", 0)?;
    response.set("code", 0)?;
    response.set("body", "")?;
    response.set("error", "offline")?;
    response.set("headers", lua.create_table()?)?;
    response.set(
        "IsFinished",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    let response_for_get = response.clone();
    response.set(
        "GetResponse",
        lua.create_function(move |_, _args: MultiValue| Ok(response_for_get.clone()))?,
    )?;
    response.set(
        "Cancel",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(response)
}

pub fn create_display_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let display = lua.create_table()?;
    let width = context.screen_width.max(1.0).round() as i32;
    let height = context.screen_height.max(1.0).round() as i32;
    let specs = create_display_specs_table(lua, width, height)?;

    display.set(
        "GetDisplayWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(width))?,
    )?;
    display.set(
        "GetDisplayHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(height))?,
    )?;
    display.set(
        "GetFPS",
        lua.create_function(|_, _args: MultiValue| Ok(60))?,
    )?;
    display.set("GetVPF", lua.create_function(|_, _args: MultiValue| Ok(1))?)?;
    display.set(
        "GetCumFPS",
        lua.create_function(|_, _args: MultiValue| Ok(60))?,
    )?;
    display.set(
        "GetDisplaySpecs",
        lua.create_function(move |_, _args: MultiValue| Ok(specs.clone()))?,
    )?;
    display.set(
        "SupportsRenderToTexture",
        lua.create_function(|_, _args: MultiValue| Ok(true))?,
    )?;
    display.set(
        "SupportsFullscreenBorderlessWindow",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(display)
}

pub fn create_memcardman_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetCardState",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("MemoryCardState_none")?))
        })?,
    )?;
    table.set(
        "GetName",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    for name in ["MountCard", "UnmountCard"] {
        table.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(false)
            })?,
        )?;
    }
    Ok(table)
}

pub fn create_unlockman_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in [
        "GetNumUnlocks",
        "GetNumUnlocked",
        "GetPoints",
        "GetPointsForProfile",
        "GetPointsUntilNextUnlock",
    ] {
        table.set(name, lua.create_function(|_, _args: MultiValue| Ok(0_i64))?)?;
    }
    table.set(
        "AnyUnlocksToCelebrate",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    table.set(
        "GetUnlockEntryIndexToCelebrate",
        lua.create_function(|_, _args: MultiValue| Ok(-1_i64))?,
    )?;
    for name in ["FindEntryID", "GetUnlockEntry"] {
        table.set(
            name,
            lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    table.set(
        "GetSongsUnlockedByEntryID",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    table.set(
        "GetStepsUnlockedByEntryID",
        lua.create_function(|lua, _args: MultiValue| {
            Ok((lua.create_table()?, lua.create_table()?))
        })?,
    )?;
    for name in [
        "PreferUnlockEntryID",
        "UnlockEntryID",
        "UnlockEntryIndex",
        "LockEntryID",
        "LockEntryIndex",
    ] {
        table.set(
            name,
            lua.create_function({
                let table = table.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(table.clone())
                }
            })?,
        )?;
    }
    table.set(
        "IsSongLocked",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    table.set(
        "IsCourseLocked",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    table.set(
        "IsStepsLocked",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    Ok(table)
}

pub fn create_hooks_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetArchName",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(song_lua_arch_name())?))
        })?,
    )?;
    table.set(
        "GetClipboard",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    table.set(
        "SetClipboard",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    for method in ["OpenFile", "OpenURL", "RestartProgram"] {
        table.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(false)
            })?,
        )?;
    }
    Ok(table)
}

pub fn create_websocket_table(lua: &Lua) -> mlua::Result<Table> {
    let websocket = lua.create_table()?;
    websocket.set("is_open", false)?;
    websocket.set(
        "IsOpen",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    for name in ["Send", "Close"] {
        websocket.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(websocket)
}

pub fn create_ini_file_table(
    lua: &Lua,
    theme_name: &'static str,
    product_version: &'static str,
) -> mlua::Result<Table> {
    let ini = lua.create_table()?;
    ini.set(
        "ReadFile",
        lua.create_function(move |lua, args: MultiValue| {
            let path = crate::method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let table = lua.create_table()?;
            if path.ends_with("ThemeInfo.ini") {
                let info = lua.create_table()?;
                info.set("DisplayName", theme_name)?;
                info.set("Version", product_version)?;
                info.set("Author", "DeadSync song-Lua compat")?;
                table.set("ThemeInfo", info)?;
            }
            Ok(table)
        })?,
    )?;
    ini.set(
        "WriteFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    Ok(ini)
}

pub fn create_rage_file_util_table(lua: &Lua) -> mlua::Result<Table> {
    let util = lua.create_table()?;
    util.set(
        "CreateRageFile",
        lua.create_function(|lua, _args: MultiValue| create_rage_file_table(lua))?,
    )?;
    Ok(util)
}

fn create_rage_file_table(lua: &Lua) -> mlua::Result<Table> {
    let file = lua.create_table()?;
    file.set(
        "Open",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    file.set(
        "Write",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    file.set(
        "Read",
        lua.create_function(|_, _args: MultiValue| Ok(String::new()))?,
    )?;
    for name in ["Close", "destroy"] {
        file.set(
            name,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    Ok(file)
}

fn create_display_specs_table(lua: &Lua, width: i32, height: i32) -> mlua::Result<Table> {
    let specs = lua.create_table()?;
    specs.raw_set(1, create_display_spec_table(lua, width, height)?)?;
    let mt = lua.create_table()?;
    mt.set(
        "__tostring",
        lua.create_function(|_, _self: Value| Ok("DisplaySpecs: songlua"))?,
    )?;
    let _ = specs.set_metatable(Some(mt));
    Ok(specs)
}

fn create_display_spec_table(lua: &Lua, width: i32, height: i32) -> mlua::Result<Table> {
    let spec = lua.create_table()?;
    let mode = create_display_mode_table(lua, width, height)?;
    let modes = lua.create_table()?;
    modes.raw_set(1, mode.clone())?;
    set_string_method(lua, &spec, "GetId", "Default")?;
    set_string_method(lua, &spec, "GetName", "Default Display")?;
    spec.set(
        "GetSupportedModes",
        lua.create_function({
            let modes = modes.clone();
            move |_, _args: MultiValue| Ok(modes.clone())
        })?,
    )?;
    spec.set(
        "IsVirtual",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    spec.set(
        "GetCurrentMode",
        lua.create_function(move |_, _args: MultiValue| Ok(mode.clone()))?,
    )?;
    Ok(spec)
}

fn create_display_mode_table(lua: &Lua, width: i32, height: i32) -> mlua::Result<Table> {
    let mode = lua.create_table()?;
    mode.set(
        "GetWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(width))?,
    )?;
    mode.set(
        "GetHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(height))?,
    )?;
    mode.set(
        "GetRefreshRate",
        lua.create_function(|_, _args: MultiValue| Ok(60))?,
    )?;
    Ok(mode)
}

fn create_layout_slot(lua: &Lua, y: f32, max_height: Option<f32>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("y", y)?;
    if let Some(max_height) = max_height {
        table.set("maxHeight", max_height)?;
    }
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

fn call_string_method(table: &Table, name: &str) -> mlua::Result<Option<String>> {
    let Some(function) = table.get::<Option<Function>>(name)? else {
        return Ok(None);
    };
    let mut args = MultiValue::new();
    args.push_back(Value::Table(table.clone()));
    Ok(Some(lua_text_value(function.call::<Value>(args)?)?))
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

fn lua_number_value(value: f32) -> Value {
    if value.is_finite() && value.fract().abs() <= f32::EPSILON {
        Value::Integer(value as i64)
    } else {
        Value::Number(value.into())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mlua::{Lua, MultiValue, Table, Value};

    use super::{
        create_author_table, create_display_bpms_table, create_ex_judgment_counts,
        create_gameplay_layout, create_radar_values_table, create_range_table,
        create_single_value_array, create_song_group_table, create_split_table, create_style_table,
        create_timing_table, display_bpms_for_args, lua_table_to_string, rotate_lua_table,
        set_path_methods, set_string_method,
    };

    #[test]
    fn split_table_handles_empty_separator() {
        let lua = Lua::new();
        let table = create_split_table(&lua, "abc", "").unwrap();

        assert_eq!(table.raw_get::<String>(1).unwrap(), "a");
        assert_eq!(table.raw_get::<String>(2).unwrap(), "b");
        assert_eq!(table.raw_get::<String>(3).unwrap(), "c");
    }

    #[test]
    fn range_table_counts_down_when_needed() {
        let lua = Lua::new();
        let mut args = MultiValue::new();
        args.push_back(Value::Integer(3));
        args.push_back(Value::Integer(1));

        let Value::Table(table) = create_range_table(&lua, &args).unwrap() else {
            panic!("range should return a table");
        };
        assert_eq!(table.raw_get::<i64>(1).unwrap(), 3);
        assert_eq!(table.raw_get::<i64>(2).unwrap(), 2);
        assert_eq!(table.raw_get::<i64>(3).unwrap(), 1);
    }

    #[test]
    fn rotate_lua_table_wraps_indices() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.raw_set(1, "a").unwrap();
        table.raw_set(2, "b").unwrap();
        table.raw_set(3, "c").unwrap();

        let mut args = MultiValue::new();
        args.push_back(Value::Table(table));
        args.push_back(Value::Integer(1));

        let rotated = rotate_lua_table(&lua, &args, false).unwrap();
        assert_eq!(rotated.raw_get::<String>(1).unwrap(), "c");
        assert_eq!(rotated.raw_get::<String>(2).unwrap(), "a");
        assert_eq!(rotated.raw_get::<String>(3).unwrap(), "b");
    }

    #[test]
    fn lua_table_to_string_reports_sequence_length() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.raw_set(1, "a").unwrap();
        table.raw_set(2, "b").unwrap();
        let mut args = MultiValue::new();
        args.push_back(Value::Table(table));
        args.push_back(Value::String(lua.create_string("Rows").unwrap()));

        assert_eq!(lua_table_to_string(&args), "Rows = {...2 item(s)}");
    }

    #[test]
    fn gameplay_layout_places_reverse_slots() {
        let lua = Lua::new();
        let layout = create_gameplay_layout(&lua, 240.0, true).unwrap();
        let combo = layout.get::<Table>("Combo").unwrap();
        let error_bar = layout.get::<Table>("ErrorBar").unwrap();

        assert_eq!(combo.get::<f32>("y").unwrap(), 210.0);
        assert_eq!(error_bar.get::<f32>("y").unwrap(), 270.0);
        assert_eq!(error_bar.get::<f32>("maxHeight").unwrap(), 30.0);
    }

    #[test]
    fn author_table_deduplicates_step_methods() {
        let lua = Lua::new();
        let steps = lua.create_table().unwrap();
        for method in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
            steps
                .set(
                    method,
                    lua.create_function(|_, _: Value| Ok("author")).unwrap(),
                )
                .unwrap();
        }

        let authors = create_author_table(&lua, Some(&Value::Table(steps))).unwrap();
        assert_eq!(authors.raw_len(), 1);
        assert_eq!(authors.raw_get::<String>(1).unwrap(), "author");
    }

    #[test]
    fn ex_judgment_counts_defaults_to_zero() {
        let lua = Lua::new();
        let counts = create_ex_judgment_counts(&lua).unwrap();

        assert_eq!(counts.get::<i64>("W1").unwrap(), 0);
        assert_eq!(counts.get::<i64>("Miss").unwrap(), 0);
        assert_eq!(counts.get::<i64>("totalRolls").unwrap(), 0);
    }

    #[test]
    fn radar_values_default_to_zero() {
        let lua = Lua::new();
        let radar = create_radar_values_table(&lua).unwrap();
        let get_value = radar.get::<mlua::Function>("GetValue").unwrap();

        assert_eq!(get_value.call::<f32>(MultiValue::new()).unwrap(), 0.0);
    }

    #[test]
    fn song_group_table_has_zero_sync_offset() {
        let lua = Lua::new();
        let group = create_song_group_table(&lua).unwrap();
        let get_sync_offset = group.get::<mlua::Function>("GetSyncOffset").unwrap();

        assert_eq!(get_sync_offset.call::<f32>(MultiValue::new()).unwrap(), 0.0);
    }

    #[test]
    fn single_value_array_wraps_table() {
        let lua = Lua::new();
        let value = lua.create_table().unwrap();
        value.set("name", "song").unwrap();

        let array = create_single_value_array(&lua, value).unwrap();
        assert_eq!(array.raw_len(), 1);
        assert_eq!(
            array
                .raw_get::<Table>(1)
                .unwrap()
                .get::<String>("name")
                .unwrap(),
            "song"
        );
    }

    #[test]
    fn string_method_returns_fixed_value() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        set_string_method(&lua, &table, "GetName", "single").unwrap();
        let get_name = table.get::<mlua::Function>("GetName").unwrap();

        assert_eq!(
            get_name.call::<String>(MultiValue::new()).unwrap(),
            "single"
        );
    }

    #[test]
    fn path_methods_report_path_presence() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        set_path_methods(
            &lua,
            &table,
            "GetPath",
            "HasPath",
            Some(Path::new("songs\\pack\\song.ogg")),
        )
        .unwrap();
        let get_path = table.get::<mlua::Function>("GetPath").unwrap();
        let has_path = table.get::<mlua::Function>("HasPath").unwrap();

        assert_eq!(
            get_path.call::<String>(MultiValue::new()).unwrap(),
            "songs/pack/song.ogg"
        );
        assert!(has_path.call::<bool>(MultiValue::new()).unwrap());
    }

    #[test]
    fn timing_table_exposes_bpm_policy() {
        let lua = Lua::new();
        let timing = create_timing_table(&lua, [120.0, 180.0]).unwrap();
        let get_bpms = timing.get::<mlua::Function>("GetBPMs").unwrap();
        let has_bpm_changes = timing.get::<mlua::Function>("HasBPMChanges").unwrap();
        let bpms = get_bpms.call::<Table>(MultiValue::new()).unwrap();

        assert_eq!(bpms.raw_get::<f32>(1).unwrap(), 120.0);
        assert_eq!(bpms.raw_get::<f32>(2).unwrap(), 180.0);
        assert!(has_bpm_changes.call::<bool>(MultiValue::new()).unwrap());
    }

    #[test]
    fn style_table_exposes_column_info() {
        let lua = Lua::new();
        let style = create_style_table(&lua, "double").unwrap();
        let columns = style.get::<mlua::Function>("ColumnsPerPlayer").unwrap();
        let get_column_info = style.get::<mlua::Function>("GetColumnInfo").unwrap();
        let mut args = MultiValue::new();
        args.push_back(Value::Table(style));
        args.push_back(Value::Integer(1));

        let info = get_column_info.call::<Table>(args).unwrap();
        assert_eq!(columns.call::<i64>(MultiValue::new()).unwrap(), 8);
        assert_eq!(info.get::<String>("Name").unwrap(), "Left");
        assert_eq!(info.get::<i64>("Track").unwrap(), 0);
    }

    #[test]
    fn display_bpms_for_args_uses_table_and_rate() {
        let lua = Lua::new();
        let bpms = create_display_bpms_table(&lua, [100.0, 200.0]).unwrap();
        let steps = lua.create_table().unwrap();
        steps
            .set(
                "GetDisplayBpms",
                lua.create_function(move |_, _: Value| Ok(bpms.clone()))
                    .unwrap(),
            )
            .unwrap();
        let mut args = MultiValue::new();
        args.push_back(Value::Nil);
        args.push_back(Value::Table(steps));
        args.push_back(Value::Number(1.5));

        let (bpms, rate) = display_bpms_for_args(&args, [60.0, 60.0], 1.0).unwrap();
        assert_eq!(bpms, [150.0, 300.0]);
        assert_eq!(rate, 1.5);
    }
}
