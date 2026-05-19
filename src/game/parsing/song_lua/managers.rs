use super::actor_host::{
    SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES,
    SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES,
    SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES,
    SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES,
    SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES,
    SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES,
    SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES, SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES,
    SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES,
    SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES, SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES,
    create_dummy_actor, resolve_script_path,
};
use super::song_tables::create_steps_table;
use super::*;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn create_theme_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
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

pub(super) fn create_gameman_table(lua: &Lua) -> mlua::Result<Table> {
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

pub(super) fn create_charman_table(lua: &Lua) -> mlua::Result<Table> {
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

fn song_lua_sound_paths_table(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(paths) = globals.get::<Option<Table>>(SONG_LUA_SOUND_PATHS_KEY)? {
        return Ok(paths);
    }
    let paths = lua.create_table()?;
    globals.set(SONG_LUA_SOUND_PATHS_KEY, paths.clone())?;
    Ok(paths)
}

fn record_song_lua_sound_path(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<()> {
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(());
    };
    let Ok(path) = resolve_script_path(lua, song_dir, &raw_path) else {
        return Ok(());
    };
    if !path.is_file() || !is_song_lua_audio_path(path.as_path()) {
        return Ok(());
    }

    let key = path.to_string_lossy().into_owned();
    let paths = song_lua_sound_paths_table(lua)?;
    for existing in paths.sequence_values::<String>() {
        if existing? == key {
            return Ok(());
        }
    }
    paths.raw_set(paths.raw_len() + 1, key)?;
    Ok(())
}

pub(super) fn create_sound_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let sound = lua.create_table()?;
    for name in ["DimMusic", "StopMusic"] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(sound_for_return.clone())
            })?,
        )?;
    }
    for name in ["PlayMusicPart", "PlayOnce"] {
        let sound_for_return = sound.clone();
        sound.set(
            name,
            lua.create_function({
                let song_dir = song_dir.to_path_buf();
                move |lua, args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    record_song_lua_sound_path(lua, song_dir.as_path(), &args)?;
                    Ok(sound_for_return.clone())
                }
            })?,
        )?;
    }
    let sound_for_return = sound.clone();
    sound.set(
        "PlayAnnouncer",
        lua.create_function(move |lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(sound_for_return.clone())
        })?,
    )?;
    sound.set(
        "GetPlayerBalance",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    sound.set(
        "IsTimingDelayed",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(sound)
}

fn theme_metric_value_for_human_players(
    lua: &Lua,
    group: &str,
    name: &str,
    human_player_count: usize,
    screen_height: f32,
) -> mlua::Result<Value> {
    if let Some(value) =
        theme_metric_number_for_screen(group, name, human_player_count, screen_height)
    {
        return Ok(Value::Number(value as f64));
    }
    if let Some(value) = theme_metric_string(group, name) {
        return Ok(Value::String(lua.create_string(&value)?));
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("DefaultNoteSkinName") {
        return Ok(Value::String(lua.create_string("default")?));
    }
    if name.eq_ignore_ascii_case("Class") {
        return Ok(Value::String(lua.create_string(group)?));
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("AutoSetStyle")
        || group.eq_ignore_ascii_case("ScreenHeartEntry")
            && name.eq_ignore_ascii_case("HeartEntryEnabled")
    {
        return Ok(Value::Boolean(false));
    }
    Ok(Value::Nil)
}

fn theme_metric_string(group: &str, name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("LineNames") {
        return theme_line_names(group).map(str::to_string);
    }
    if name.eq_ignore_ascii_case("Fallback") {
        return theme_screen_fallback(group).map(str::to_string);
    }
    if let Some(row) = name.strip_prefix("Line") {
        if let Some(metric) = theme_explicit_line_metric(group, row) {
            return Some(metric.to_string());
        }
        if group.eq_ignore_ascii_case("ScreenOptionsService") {
            return Some(format!("gamecommand;screen,Screen{row};name,{row}"));
        }
        if theme_screen_fallback(group).is_some() && !row.trim().is_empty() {
            return Some(format!("conf,{row}"));
        }
    }
    None
}

fn theme_explicit_line_metric(group: &str, row: &str) -> Option<&'static str> {
    if group.eq_ignore_ascii_case("ScreenGraphicsSoundOptions") {
        return match row {
            "VideoRenderer" => Some("lua,OperatorMenuOptionRows.VideoRenderer()"),
            "DisplayAspectRatio" => Some("lua,ConfAspectRatio()"),
            "DisplayResolution" => Some("lua,ConfDisplayResolution()"),
            "DisplayMode" => Some("lua,ConfDisplayMode()"),
            "FullscreenType" => Some("lua,ConfFullscreenType()"),
            "GlobalOffsetSeconds" => Some("lua,OperatorMenuOptionRows.GlobalOffsetSeconds()"),
            "VisualDelaySeconds" => Some("lua,OperatorMenuOptionRows.VisualDelaySeconds()"),
            _ => None,
        };
    }
    if group.eq_ignore_ascii_case("ScreenSystemOptions") {
        return match row {
            "Theme" => Some("lua,OperatorMenuOptionRows.Theme()"),
            "EditorNoteSkin" => Some("lua,OperatorMenuOptionRows.EditorNoteskin()"),
            _ => None,
        };
    }
    None
}

fn theme_line_names(group: &str) -> Option<&'static str> {
    if group.eq_ignore_ascii_case("ScreenPlayerOptions") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenPlayerOptions2") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenPlayerOptions3") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAttackMenu") {
        Some(SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenOptionsService") {
        Some(SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenSystemOptions") {
        Some(SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenInputOptions") {
        Some(SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenGraphicsSoundOptions") {
        Some(SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenVisualOptions") {
        Some(SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAppearanceOptions") {
        Some(SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenArcadeOptions") {
        Some(SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAdvancedOptions") {
        Some(SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenThemeOptions") {
        Some(SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenMenuTimerOptions") {
        Some(SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenUSBProfileOptions") {
        Some(SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenTournamentModeOptions") {
        Some(SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenGrooveStatsOptions") {
        Some(SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES)
    } else {
        None
    }
}

fn theme_screen_fallback(group: &str) -> Option<&'static str> {
    let lower = group.to_ascii_lowercase();
    match lower.as_str() {
        "screenoptionsservice" => Some("ScreenOptionsSimple"),
        "screenvisualoptions" => Some("ScreenOptionsServiceSub"),
        "screensystemoptions"
        | "screeninputoptions"
        | "screengraphicssoundoptions"
        | "screenappearanceoptions"
        | "screenarcadeoptions"
        | "screenadvancedoptions"
        | "screenthemeoptions"
        | "screenmenutimeroptions"
        | "screenusbprofileoptions"
        | "screentournamentmodeoptions"
        | "screengroovestatsoptions" => Some("ScreenOptionsServiceChild"),
        _ => None,
    }
}

pub(super) fn theme_metric_number(group: &str, name: &str) -> Option<f32> {
    theme_metric_number_for_human_players(group, name, LUA_PLAYERS)
}

fn theme_metric_number_for_human_players(
    group: &str,
    name: &str,
    human_player_count: usize,
) -> Option<f32> {
    theme_metric_number_for_screen(group, name, human_player_count, 480.0)
}

fn theme_metric_number_for_screen(
    group: &str,
    name: &str,
    human_player_count: usize,
    screen_height: f32,
) -> Option<f32> {
    if group.eq_ignore_ascii_case("Player") {
        if name.eq_ignore_ascii_case("ReceptorArrowsYStandard") {
            return Some(THEME_RECEPTOR_Y_STD);
        }
        if name.eq_ignore_ascii_case("ReceptorArrowsYReverse") {
            return Some(THEME_RECEPTOR_Y_REV);
        }
        if name.eq_ignore_ascii_case("DrawDistanceBeforeTargetsPixels") {
            return Some(screen_height.max(1.0) * 1.5);
        }
        if name.eq_ignore_ascii_case("DrawDistanceAfterTargetsPixels") {
            return Some(-130.0);
        }
    }
    if group.eq_ignore_ascii_case("Combo") && name.eq_ignore_ascii_case("ShowComboAt") {
        return Some(4.0);
    }
    if group.eq_ignore_ascii_case("GraphDisplay") {
        if name.eq_ignore_ascii_case("BodyWidth") {
            return Some(graph_display_body_size(human_player_count)[0]);
        }
        if name.eq_ignore_ascii_case("BodyHeight") {
            return Some(graph_display_body_size(human_player_count)[1]);
        }
    }
    if group.eq_ignore_ascii_case("LifeMeterBar") && name.eq_ignore_ascii_case("InitialValue") {
        return Some(SONG_LUA_INITIAL_LIFE);
    }
    if group.eq_ignore_ascii_case("MusicWheel") && name.eq_ignore_ascii_case("NumWheelItems") {
        return Some(15.0);
    }
    if group.eq_ignore_ascii_case("PlayerStageStats")
        && name.eq_ignore_ascii_case("NumGradeTiersUsed")
    {
        return Some(7.0);
    }
    None
}

pub(super) fn graph_display_body_size(human_player_count: usize) -> [f32; 2] {
    [
        if human_player_count == 1 {
            610.0
        } else {
            300.0
        },
        64.0,
    ]
}

pub(super) fn song_lua_human_player_count(context: &SongLuaCompileContext) -> usize {
    context
        .players
        .iter()
        .filter(|player| player.enabled)
        .count()
}

fn theme_metric_bool(value: Value) -> bool {
    match value {
        Value::Boolean(value) => value,
        Value::Integer(value) => value != 0,
        Value::Number(value) => value != 0.0,
        Value::String(value) => !value.to_str().is_ok_and(|text| text.is_empty()),
        _ => false,
    }
}

fn theme_metric_names(group: &str) -> Vec<String> {
    let mut names = Vec::new();
    if theme_line_names(group).is_some() {
        names.push("LineNames".to_string());
    }
    if theme_screen_fallback(group).is_some() {
        names.push("Fallback".to_string());
    }
    if let Some(lines) = theme_line_names(group) {
        names.extend(
            lines
                .split(',')
                .filter(|line| !line.trim().is_empty())
                .map(|line| format!("Line{}", line.trim())),
        );
    }
    if group.eq_ignore_ascii_case("Player") {
        names.extend(
            [
                "ReceptorArrowsYStandard",
                "ReceptorArrowsYReverse",
                "DrawDistanceBeforeTargetsPixels",
                "DrawDistanceAfterTargetsPixels",
            ]
            .into_iter()
            .map(str::to_string),
        );
    } else if group.eq_ignore_ascii_case("Common") {
        names.extend(
            ["DefaultNoteSkinName", "AutoSetStyle"]
                .into_iter()
                .map(str::to_string),
        );
    } else if group.eq_ignore_ascii_case("Combo") {
        names.push("ShowComboAt".to_string());
    } else if group.eq_ignore_ascii_case("GraphDisplay") {
        names.extend(["BodyWidth", "BodyHeight"].into_iter().map(str::to_string));
    } else if group.eq_ignore_ascii_case("LifeMeterBar") {
        names.push("InitialValue".to_string());
    } else if group.eq_ignore_ascii_case("MusicWheel") {
        names.push("NumWheelItems".to_string());
    } else if group.eq_ignore_ascii_case("PlayerStageStats") {
        names.push("NumGradeTiersUsed".to_string());
    } else if group.eq_ignore_ascii_case("ScreenHeartEntry") {
        names.push("HeartEntryEnabled".to_string());
    }
    names.sort_unstable();
    names.dedup();
    names
}

fn theme_string_names(section: &str) -> Vec<String> {
    if section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
    {
        return [
            SongLuaDifficulty::Beginner,
            SongLuaDifficulty::Easy,
            SongLuaDifficulty::Medium,
            SongLuaDifficulty::Hard,
            SongLuaDifficulty::Challenge,
            SongLuaDifficulty::Edit,
        ]
        .into_iter()
        .map(|difficulty| difficulty.sm_name().to_string())
        .collect();
    }
    if matches!(
        section,
        "OptionTitles"
            | "OptionNames"
            | "ThemePrefs"
            | "SLPlayerOptions"
            | "ScreenSelectPlayMode"
            | "ScreenSelectStyle"
            | "GameButton"
            | "TapNoteScore"
            | "TapNoteScoreFA+"
            | "HoldNoteScore"
            | "Stage"
            | "Months"
    ) {
        return [
            "Yes",
            "No",
            "Cancel",
            "DisplayMode",
            "MusicRate",
            "SpeedMod",
            "NoteSkin",
            "Difficulty_Hard",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
    }
    Vec::new()
}

pub(super) fn theme_path(kind: &str, group: &str, name: &str) -> String {
    let group = group.trim_matches('/');
    let name = name.trim_start_matches('/');
    if group.is_empty() {
        format!("{SONG_LUA_THEME_PATH_PREFIX}{kind}/{name}")
    } else {
        format!("{SONG_LUA_THEME_PATH_PREFIX}{kind}/{group}/{name}")
    }
}

#[derive(Clone, Copy)]
enum LuaOptionValues {
    Str(&'static [&'static str]),
    Bool(&'static [bool]),
    Int(&'static [i64]),
    Number(&'static [f64]),
}

impl LuaOptionValues {
    fn len(self) -> usize {
        match self {
            Self::Str(values) => values.len(),
            Self::Bool(values) => values.len(),
            Self::Int(values) => values.len(),
            Self::Number(values) => values.len(),
        }
    }
}

#[derive(Clone, Copy)]
struct SongLuaOptionRowSpec {
    choices: LuaOptionValues,
    values: Option<LuaOptionValues>,
    layout_type: &'static str,
    select_type: &'static str,
    one_choice_for_all_players: bool,
    export_on_change: bool,
    hide_on_disable: bool,
    reload_row_messages: &'static [&'static str],
    broadcast_on_export: &'static [&'static str],
}

impl SongLuaOptionRowSpec {
    fn new(choices: LuaOptionValues) -> Self {
        Self {
            choices,
            values: None,
            layout_type: "ShowAllInRow",
            select_type: "SelectOne",
            one_choice_for_all_players: false,
            export_on_change: false,
            hide_on_disable: false,
            reload_row_messages: &[],
            broadcast_on_export: &[],
        }
    }

    fn values(mut self, values: LuaOptionValues) -> Self {
        self.values = Some(values);
        self
    }

    fn layout(mut self, layout_type: &'static str) -> Self {
        self.layout_type = layout_type;
        self
    }

    fn select(mut self, select_type: &'static str) -> Self {
        self.select_type = select_type;
        self
    }

    fn one_choice(mut self) -> Self {
        self.one_choice_for_all_players = true;
        self
    }

    fn export(mut self) -> Self {
        self.export_on_change = true;
        self
    }

    fn hide_on_disable(mut self) -> Self {
        self.hide_on_disable = true;
        self
    }

    fn reload(mut self, messages: &'static [&'static str]) -> Self {
        self.reload_row_messages = messages;
        self
    }
}

const OPTION_YES_NO: &[&str] = &["Yes", "No"];
const OPTION_ON_OFF: &[&str] = &["On", "Off"];
const OPTION_OFF_ON: &[&str] = &["Off", "On"];
const OPTION_TRUE_FALSE: &[bool] = &[true, false];
const OPTION_FALSE_TRUE: &[bool] = &[false, true];
const OPTION_EMPTY: &[&str] = &[""];
const OPTION_NONE: &[&str] = &["None"];
const OPTION_ONE_TO_TWELVE: &[i64] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
const OPTION_ZERO_TO_NINE: &[i64] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
const OPTION_CASUAL_METERS: &[i64] = &[5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const OPTION_MENU_TIMER_CHOICES: &[&str] = &[
    "0:15", "0:30", "1:00", "1:30", "2:00", "3:00", "5:00", "7:30",
];
const OPTION_MENU_TIMER_VALUES: &[i64] = &[15, 30, 60, 90, 120, 180, 300, 450];
const OPTION_NICE_CHOICES: &[&str] = &["Off", "On", "OnWithSound"];
const OPTION_NICE_VALUES: &[i64] = &[0, 1, 2];
const OPTION_VISUAL_STYLE: &[&str] = &[
    "Hearts",
    "Arrows",
    "Bears",
    "Ducks",
    "Cats",
    "Spooky",
    "Gay",
    "Stars",
    "Thonk",
    "Technique",
    "SRPG9",
];
const OPTION_GAME_MODE: &[&str] = &["Casual", "ITG"];
const OPTION_AUTO_STYLE: &[&str] = &["none", "single", "versus", "double"];
const OPTION_MUSIC_WHEEL_STYLE: &[&str] = &["ITG", "IIDX"];
const OPTION_THEME_FONT: &[&str] = &["Common"];
const OPTION_BG_STYLE: &[&str] = &["Off", "Random"];
const OPTION_QR_LOGIN: &[&str] = &["Always", "Sometimes", "Never"];
const OPTION_SCORING_SYSTEM: &[&str] = &["EX", "ITG"];
const OPTION_STEP_STATS: &[&str] = &["Show", "Hide"];
const OPTION_SPEED_MOD_TYPE: &[&str] = &["X", "C", "M"];
const OPTION_SPEED_MOD: &[&str] = &["1", "1.5", "2", "C400", "M650"];
const OPTION_MINI: &[&str] = &["-100%", "0%", "25%", "50%", "100%", "150%"];
const OPTION_SPACING: &[&str] = &["-100%", "0%", "25%", "50%", "100%"];
const OPTION_NOTESKIN: &[&str] = &["default"];
const OPTION_BACKGROUND_FILTER: &[&str] = &["Off", "Dark", "Darker", "Darkest"];
const OPTION_NOTE_FIELD_OFFSET: &[&str] = &["0", "10", "25", "50"];
const OPTION_VISUAL_DELAY: &[&str] = &["-100ms", "0ms", "100ms"];
const OPTION_MUSIC_RATE: &[&str] = &["0.75", "1", "1.25", "1.5", "2"];
const OPTION_STEPCHART: &[&str] = &["Easy 1", "Medium 5", "Hard 9"];
const OPTION_SCREEN_AFTER_PLAYER_OPTIONS: &[&str] = &[
    "ScreenGameplay",
    "ScreenSelectMusic",
    "ScreenPlayerOptions",
    "ScreenPlayerOptions2",
];
const OPTION_HIDE: &[&str] = &[
    "Targets",
    "SongBG",
    "Combo",
    "Lifebar",
    "Score",
    "Danger",
    "ComboExplosions",
];
const OPTION_GAMEPLAY_EXTRAS: &[&str] = &[
    "ColumnFlashOnMiss",
    "SubtractiveScoring",
    "Pacemaker",
    "NPSGraphAtTop",
    "JudgmentTilt",
    "ColumnCues",
];
const OPTION_RESULTS_EXTRAS: &[&str] = &["TargetScore", "EvaluationPane", "Graphs"];
const OPTION_LIFE_METER_TYPE: &[&str] = &["Standard", "Battery"];
const OPTION_DATA_VISUALIZATIONS: &[&str] = &["None", "Target Score Graph", "Step Statistics"];
const OPTION_TARGET_SCORE: &[&str] = &[
    "GradeTier16",
    "GradeTier10",
    "Machine best",
    "Personal best",
];
const OPTION_TARGET_SCORE_NUMBER: &[&str] = &["1", "2", "3", "4", "5"];
const OPTION_ACTION_ON_MISSED_TARGET: &[&str] = &["Nothing", "Fail", "Restart"];
const OPTION_TILT_MULTIPLIER: &[&str] = &["1", "1.5", "2", "2.5", "3"];
const OPTION_ERROR_BAR: &[&str] = &["None", "Colorful", "Monochrome", "Text"];
const OPTION_ERROR_BAR_TRIM: &[&str] = &["Off", "Great", "Excellent"];
const OPTION_ERROR_BAR_OPTIONS: &[&str] = &["ErrorBarUp", "ErrorBarMultiTick"];
const OPTION_MEASURE_COUNTER: &[&str] = &["None", "8th", "12th", "16th", "24th", "32nd"];
const OPTION_MEASURE_COUNTER_OPTIONS: &[&str] =
    &["MeasureCounterLeft", "MeasureCounterUp", "HideLookahead"];
const OPTION_MEASURE_COUNTER_LOOKAHEAD: &[&str] = &["0", "1", "2", "4"];
const OPTION_MEASURE_LINES: &[&str] = &["Off", "Measure", "Quarter", "Eighth"];
const OPTION_TIMING_WINDOW_OPTIONS: &[&str] = &[
    "HideEarlyDecentWayOffJudgments",
    "HideEarlyDecentWayOffFlash",
];
const OPTION_TIMING_WINDOWS: &[&str] = &["All", "Hide Way Off", "Hide Decents and Way Offs"];
const OPTION_FA_PLUS: &[&str] = &["ShowFaPlusWindow", "ShowExScore", "ShowFaPlusPane"];
const OPTION_LIFE_BAR_OPTIONS: &[&str] = &["Normal", "Vertical", "Hidden"];
const OPTION_SCORE_BOX_OPTIONS: &[&str] = &["Machine", "Personal", "Rival"];
const OPTION_STEP_STATS_EXTRA: &[&str] = &["DensityGraph", "Measures", "Streams"];
const OPTION_FUN_OPTIONS: &[&str] = &["Confetti", "LaneCover", "ScreenFilter"];
const OPTION_COMBO_COLORS: &[&str] = &["Default", "Difficulty", "Judgment"];
const OPTION_COMBO_MODE: &[&str] = &["Standard", "Additive", "Proportional"];
const OPTION_TIMER_MODE: &[&str] = &["Song", "Remaining", "Off"];
const OPTION_JUDGMENT_ANIMATION: &[&str] = &["Default", "ProITG", "None"];
const OPTION_RAIL_BALANCE: &[&str] = &["Off", "Standard", "Strict"];
const OPTION_EXTRA_AESTHETICS: &[&str] = &["Backgrounds", "Particles", "ScreenFX"];
const OPTION_THEME_NAMES: &[&str] = &[SONG_LUA_THEME_NAME];
const OPTION_FAIL_TYPES: &[&str] = &["Immediate", "ImmediateContinue", "Off"];
const OPTION_LONG_TIME: &[&str] = &["2:30", "3:00", "4:00", "5:00", "Off"];
const OPTION_MARATHON_TIME: &[&str] = &["5:00", "7:30", "10:00", "15:00", "Off"];
const OPTION_LONG_TIME_VALUES: &[i64] = &[150, 180, 240, 300, 999_999];
const OPTION_MARATHON_TIME_VALUES: &[i64] = &[300, 450, 600, 900, 999_999];
const OPTION_MUSIC_WHEEL_SPEED: &[&str] = &[
    "Slow",
    "Normal",
    "Fast",
    "Faster",
    "Ridiculous",
    "Ludicrous",
    "Plaid",
];
const OPTION_MUSIC_WHEEL_SPEED_VALUES: &[i64] = &[5, 10, 15, 25, 30, 45, 100];
const OPTION_VIDEO_RENDERER: &[&str] = &["opengl"];
const OPTION_DISPLAY_ASPECT_RATIO: &[&str] = &["16:9", "4:3"];
const OPTION_DISPLAY_ASPECT_RATIO_VALUES: &[f64] = &[16.0 / 9.0, 4.0 / 3.0];
const OPTION_DISPLAY_RESOLUTION: &[&str] = &["1920x1080", "1280x720", "640x480"];
const OPTION_DISPLAY_MODE: &[&str] = &["Windowed", "Fullscreen"];
const OPTION_REFRESH_RATE: &[&str] = &["60", "120", "144"];
const OPTION_REFRESH_RATE_VALUES: &[i64] = &[60, 120, 144];
const OPTION_FULLSCREEN_TYPE: &[&str] = &["Borderless", "Exclusive"];
const OPTION_OFFSET_MS: &[&str] = &["-1000ms", "-500ms", "0ms", "500ms", "1000ms"];
const OPTION_OFFSET_SECONDS_VALUES: &[f64] = &[-1.0, -0.5, 0.0, 0.5, 1.0];
const OPTION_CUSTOM_SONG_SECONDS: &[&str] = &["1:45", "3:00", "5:00", "10:00", "15:00", "2:00:00"];
const OPTION_CUSTOM_SONG_SECONDS_VALUES: &[i64] = &[105, 180, 300, 600, 900, 7200];
const OPTION_CUSTOM_SONG_MEGABYTES: &[&str] = &["3 MB", "5 MB", "10 MB", "20 MB", "30 MB", "1 GB"];
const OPTION_CUSTOM_SONG_MEGABYTES_VALUES: &[i64] = &[3, 5, 10, 20, 30, 1000];
const OPTION_CUSTOM_SONG_TIMEOUT: &[&str] = &["3", "5", "10", "60"];
const OPTION_CUSTOM_SONG_TIMEOUT_VALUES: &[i64] = &[3, 5, 10, 60];
const OPTION_REFRESH_ACTOR_PROXY_MESSAGES: &[&str] = &["RefreshActorProxy"];
const THEME_PREF_ROW_NAMES: &[&str] = &[
    "AllowFailingOutOfSet",
    "NumberOfContinuesAllowed",
    "HideStockNoteSkins",
    "MusicWheelStyle",
    "AllowDanceSolo",
    "DefaultGameMode",
    "AutoStyle",
    "VisualStyle",
    "AllowThemeVideos",
    "RainbowMode",
    "WriteCustomScores",
    "KeyboardFeatures",
    "SampleMusicLoops",
    "RescoreEarlyHits",
    "AnimateBanners",
    "SimplyLoveColor",
    "EditModeLastSeenSong",
    "EditModeLastSeenStepsType",
    "EditModeLastSeenStyleType",
    "EditModeLastSeenDifficulty",
    "ScreenGrooveStatsLoginMenuTimer",
    "ScreenSelectMusicMenuTimer",
    "ScreenSelectMusicCasualMenuTimer",
    "ScreenPlayerOptionsMenuTimer",
    "ScreenEvaluationMenuTimer",
    "ScreenEvaluationNonstopMenuTimer",
    "ScreenEvaluationSummaryMenuTimer",
    "ScreenNameEntryMenuTimer",
    "AllowScreenSelectProfile",
    "AllowScreenSelectColor",
    "AllowScreenSelectPlayMode",
    "AllowScreenSelectPlayMode2",
    "AllowScreenEvalSummary",
    "AllowScreenGameOver",
    "AllowScreenNameEntry",
    "CasualMaxMeter",
    "UseImageCache",
    "nice",
    "LastActiveEvent",
    "EnableTournamentMode",
    "ScoringSystem",
    "StepStats",
    "EnforceNoCmod",
    "EnableGrooveStats",
    "AutoDownloadUnlocks",
    "SeparateUnlocksByPlayer",
    "QRLogin",
    "EnableOnlineLobbies",
];

pub(super) fn create_theme_prefs_table(lua: &Lua) -> mlua::Result<Table> {
    let prefs = lua.create_table()?;
    let store = lua.create_table()?;
    let get_store = store.clone();
    prefs.set(
        "Get",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let stored = get_store.get::<Value>(name.as_str())?;
            if !matches!(stored, Value::Nil) {
                return Ok(stored);
            }
            theme_pref_default(lua, &name)
        })?,
    )?;
    let set_store = store.clone();
    prefs.set(
        "Set",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(());
            };
            let value = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
            set_store.set(name, value)?;
            Ok(())
        })?,
    )?;
    prefs.set(
        "Save",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    let init_store = store;
    prefs.set(
        "InitAll",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let defs = if args.len() == 1 {
                args.front()
            } else {
                method_arg(&args, 0)
            };
            let Some(Value::Table(defs)) = defs else {
                return Ok(());
            };
            for pair in defs.pairs::<String, Table>() {
                let (name, def) = pair?;
                if matches!(init_store.get::<Value>(name.as_str())?, Value::Nil) {
                    let default = def.get::<Value>("Default").unwrap_or(Value::Nil);
                    if !matches!(default, Value::Nil) {
                        init_store.set(name, default)?;
                    }
                }
            }
            Ok(())
        })?,
    )?;
    Ok(prefs)
}

fn create_lua_option_array(lua: &Lua, values: LuaOptionValues) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match values {
        LuaOptionValues::Str(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        LuaOptionValues::Bool(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        LuaOptionValues::Int(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        LuaOptionValues::Number(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
    }
    Ok(table)
}

fn option_row_list_arg(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
    if let Some(Value::Table(table)) = method_arg(args, 0) {
        return Ok(table.clone());
    }
    if let Some(Value::Table(table)) = args.front() {
        return Ok(table.clone());
    }
    lua.create_table()
}

fn option_row_player_arg(args: &MultiValue) -> Option<&Value> {
    if matches!(args.front(), Some(Value::Table(_))) {
        if matches!(args.get(1), Some(Value::Table(_))) {
            args.get(2)
        } else {
            args.get(1)
        }
    } else {
        args.get(1)
    }
}

fn option_row_has_selection(table: &Table, count: usize) -> mlua::Result<bool> {
    for index in 1..=count.max(table.raw_len()) {
        match table.raw_get::<Value>(index)? {
            Value::Boolean(true) => return Ok(true),
            Value::Nil | Value::Boolean(false) => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}

fn option_row_values_table(row: &Table) -> mlua::Result<Table> {
    match row.get::<Value>("Values")? {
        Value::Table(values) => Ok(values),
        _ => row.get::<Table>("Choices"),
    }
}

fn option_row_selected_value(row: &Table, selections: &Table) -> mlua::Result<Value> {
    let values = option_row_values_table(row)?;
    let count = values.raw_len().max(selections.raw_len());
    for index in 1..=count {
        if truthy(&selections.raw_get::<Value>(index)?) {
            return values.raw_get(index);
        }
    }
    values.raw_get(1)
}

fn set_pref_option_save(lua: &Lua, row: &Table, pref_name: &str) -> mlua::Result<()> {
    let row_for_save = row.clone();
    let pref_name = pref_name.to_string();
    row.set(
        "SaveSelections",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let selections = option_row_list_arg(lua, &args)?;
            let value = option_row_selected_value(&row_for_save, &selections)?;
            let prefsmgr = lua.globals().get::<Table>("PREFSMAN")?;
            let set_preference = prefsmgr.get::<Function>("SetPreference")?;
            let _: Value = set_preference.call((prefsmgr, pref_name.as_str(), value))?;
            Ok(())
        })?,
    )
}

fn set_theme_pref_option_save(lua: &Lua, row: &Table, pref_name: &str) -> mlua::Result<()> {
    let row_for_save = row.clone();
    let pref_name = pref_name.to_string();
    row.set(
        "SaveSelections",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let selections = option_row_list_arg(lua, &args)?;
            let value = option_row_selected_value(&row_for_save, &selections)?;
            let theme_prefs = lua.globals().get::<Table>("ThemePrefs")?;
            let set = theme_prefs.get::<Function>("Set")?;
            set.call::<()>((theme_prefs, pref_name.as_str(), value))
        })?,
    )
}

fn set_custom_option_save(lua: &Lua, row: &Table, option_name: &str) -> mlua::Result<()> {
    let row_for_save = row.clone();
    let option_name = option_name.to_string();
    row.set(
        "SaveSelections",
        lua.create_function(move |lua, args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            let selections = option_row_list_arg(lua, &args)?;
            let selected_player = option_row_player_arg(&args)
                .and_then(player_index_from_value)
                .unwrap_or(0);
            let sl = lua.globals().get::<Table>("SL")?;
            if option_name.eq_ignore_ascii_case("MusicRate") {
                let global = sl.get::<Table>("Global")?;
                let mods = global.get::<Table>("ActiveModifiers")?;
                mods.set(
                    "MusicRate",
                    option_row_selected_value(&row_for_save, &selections)?,
                )?;
                return Ok(());
            }
            let player = sl.get::<Table>(player_short_name(selected_player))?;
            let mods = player.get::<Table>("ActiveModifiers")?;
            if row_for_save.get::<String>("SelectType")? == "SelectMultiple" {
                let choices = row_for_save.get::<Table>("Choices")?;
                for index in 1..=choices.raw_len().max(selections.raw_len()) {
                    let selected = truthy(&selections.raw_get::<Value>(index)?);
                    let choice = choices
                        .raw_get::<Value>(index)
                        .ok()
                        .and_then(read_string)
                        .unwrap_or_default();
                    let key = custom_multi_modifier_key(&option_name, &choice);
                    if !key.is_empty() {
                        mods.set(key, selected)?;
                    }
                }
            } else {
                mods.set(
                    option_name.as_str(),
                    option_row_selected_value(&row_for_save, &selections)?,
                )?;
            }
            Ok(())
        })?,
    )
}

fn custom_multi_modifier_key(option_name: &str, choice: &str) -> String {
    if option_name.eq_ignore_ascii_case("Hide") {
        format!("Hide{choice}")
    } else {
        choice.to_string()
    }
}

fn create_compat_option_row_table(
    lua: &Lua,
    name: &str,
    spec: SongLuaOptionRowSpec,
) -> mlua::Result<Table> {
    let row = lua.create_table()?;
    let choice_count = spec.choices.len();
    row.set("Name", name)?;
    row.set("Choices", create_lua_option_array(lua, spec.choices)?)?;
    if let Some(values) = spec.values {
        row.set("Values", create_lua_option_array(lua, values)?)?;
    }
    row.set("LayoutType", spec.layout_type)?;
    row.set("SelectType", spec.select_type)?;
    row.set("OneChoiceForAllPlayers", spec.one_choice_for_all_players)?;
    row.set("ExportOnChange", spec.export_on_change)?;
    row.set("HideOnDisable", spec.hide_on_disable)?;
    row.set(
        "ReloadRowMessages",
        create_string_array(lua, spec.reload_row_messages)?,
    )?;
    row.set(
        "BroadcastOnExport",
        create_string_array(lua, spec.broadcast_on_export)?,
    )?;
    row.set(
        "EnabledForPlayers",
        lua.create_function(|lua, _args: MultiValue| {
            create_string_array(lua, &[player_number_name(0), player_number_name(1)])
        })?,
    )?;
    row.set(
        "LoadSelections",
        lua.create_function(move |lua, args: MultiValue| {
            let list = option_row_list_arg(lua, &args)?;
            if choice_count > 0 && !option_row_has_selection(&list, choice_count)? {
                list.raw_set(1, true)?;
            }
            Ok(list)
        })?,
    )?;
    row.set(
        "SaveSelections",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(row)
}

pub(super) fn create_custom_option_row(lua: &Lua, name: &str) -> mlua::Result<Option<Table>> {
    let Some(spec) = custom_option_row_spec(name) else {
        return Ok(None);
    };
    let row = create_compat_option_row_table(lua, name, spec)?;
    set_custom_option_save(lua, &row, name)?;
    Ok(Some(row))
}

pub(super) fn create_conf_option_row(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let (row_name, spec) = match name.to_ascii_lowercase().as_str() {
        "confaspectratio" => (
            "DisplayAspectRatio",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DISPLAY_ASPECT_RATIO))
                .values(LuaOptionValues::Number(OPTION_DISPLAY_ASPECT_RATIO_VALUES))
                .one_choice(),
        ),
        "confdisplayresolution" => (
            "DisplayResolution",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DISPLAY_RESOLUTION)).one_choice(),
        ),
        "confdisplaymode" => (
            "DisplayMode",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DISPLAY_MODE)).one_choice(),
        ),
        "confrefreshrate" => (
            "RefreshRate",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_REFRESH_RATE))
                .values(LuaOptionValues::Int(OPTION_REFRESH_RATE_VALUES))
                .one_choice(),
        ),
        "conffullscreentype" => (
            "FullscreenType",
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FULLSCREEN_TYPE)).one_choice(),
        ),
        _ => (
            name,
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON)).one_choice(),
        ),
    };
    let row = create_compat_option_row_table(lua, row_name, spec)?;
    set_pref_option_save(lua, &row, row_name)?;
    Ok(row)
}

fn custom_option_row_spec(name: &str) -> Option<SongLuaOptionRowSpec> {
    let lower = name.to_ascii_lowercase();
    let spec = match lower.as_str() {
        "speedmodtype" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SPEED_MOD_TYPE))
            .layout("ShowOneInRow")
            .export(),
        "speedmod" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SPEED_MOD))
            .layout("ShowOneInRow")
            .export(),
        "mini" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MINI)).layout("ShowOneInRow")
        }
        "spacing" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SPACING)).layout("ShowOneInRow")
        }
        "noteskin" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NOTESKIN))
            .layout("ShowOneInRow")
            .export(),
        "judgmentgraphic" | "holdjudgment" | "heldgraphic" | "heldmissgraphic" | "combofont" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NONE))
                .layout("ShowOneInRow")
                .export()
        }
        "noteskinvariant" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NONE))
            .layout("ShowOneInRow")
            .export()
            .hide_on_disable()
            .reload(OPTION_REFRESH_ACTOR_PROXY_MESSAGES),
        "backgroundfilter" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_BACKGROUND_FILTER))
                .values(LuaOptionValues::Str(OPTION_BACKGROUND_FILTER))
        }
        "notefieldoffsetx" | "notefieldoffsety" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NOTE_FIELD_OFFSET))
                .layout("ShowOneInRow")
                .export()
        }
        "visualdelay" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_VISUAL_DELAY))
            .layout("ShowOneInRow")
            .export(),
        "musicrate" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MUSIC_RATE))
            .layout("ShowOneInRow")
            .one_choice()
            .export(),
        "stepchart" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_STEPCHART)).export(),
        "screenafterplayeroptions"
        | "screenafterplayeroptions2"
        | "screenafterplayeroptions3"
        | "screenafterplayeroptions4" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SCREEN_AFTER_PLAYER_OPTIONS))
        }
        "hide" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_HIDE)).select("SelectMultiple")
        }
        "gameplayextras" | "gameplayextrasb" | "gameplayextrasc" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_GAMEPLAY_EXTRAS))
                .select("SelectMultiple")
        }
        "resultsextras" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_RESULTS_EXTRAS))
            .select("SelectMultiple"),
        "lifemetertype" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_LIFE_METER_TYPE)),
        "datavisualizations" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_DATA_VISUALIZATIONS))
        }
        "targetscore" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TARGET_SCORE)),
        "targetscorenumber" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TARGET_SCORE_NUMBER))
        }
        "actiononmissedtarget" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ACTION_ON_MISSED_TARGET))
        }
        "tiltmultiplier" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TILT_MULTIPLIER)),
        "errorbar" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ERROR_BAR)),
        "errorbartrim" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ERROR_BAR_TRIM)),
        "errorbaroptions" | "errorbarcap" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ERROR_BAR_OPTIONS))
                .select("SelectMultiple")
        }
        "measurecounter" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_COUNTER)),
        "measurecounteroptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_COUNTER_OPTIONS))
                .select("SelectMultiple")
        }
        "measurecounterlookahead" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_COUNTER_LOOKAHEAD))
        }
        "measurelines" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MEASURE_LINES)),
        "timingwindowoptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TIMING_WINDOW_OPTIONS))
                .select("SelectMultiple")
        }
        "timingwindows" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TIMING_WINDOWS)),
        "faplus" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FA_PLUS)).select("SelectMultiple")
        }
        "minindicator" | "miniindicator" | "miniindicatorcolor" | "stepstatsinfo"
        | "judgmentflash" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON)),
        "scoreboxoptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SCORE_BOX_OPTIONS))
                .select("SelectMultiple")
        }
        "stepstatsextra" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_STEP_STATS_EXTRA))
                .select("SelectMultiple")
        }
        "funoptions" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FUN_OPTIONS))
            .select("SelectMultiple"),
        "lifebaroptions" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_LIFE_BAR_OPTIONS))
        }
        "combocolors" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_COMBO_COLORS)),
        "combomode" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_COMBO_MODE)),
        "timermode" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_TIMER_MODE)),
        "judgmentanimation" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_JUDGMENT_ANIMATION))
        }
        "railbalance" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_RAIL_BALANCE)),
        "extraaesthetics" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_EXTRA_AESTHETICS))
                .select("SelectMultiple")
        }
        _ => return None,
    };
    Some(spec)
}

pub(super) fn custom_option_default_text(name: &str) -> Option<String> {
    custom_option_row_spec(name).map(|spec| option_value_text(spec.choices, 0))
}

fn option_value_text(values: LuaOptionValues, index: usize) -> String {
    match values {
        LuaOptionValues::Str(values) => values.get(index).copied().unwrap_or_default().to_string(),
        LuaOptionValues::Bool(values) => values.get(index).copied().unwrap_or(false).to_string(),
        LuaOptionValues::Int(values) => values.get(index).copied().unwrap_or_default().to_string(),
        LuaOptionValues::Number(values) => {
            values.get(index).copied().unwrap_or_default().to_string()
        }
    }
}

pub(super) fn create_theme_prefs_rows_table(lua: &Lua) -> mlua::Result<Table> {
    let rows = lua.create_table()?;
    rows.set(
        "GetRow",
        lua.create_function(|lua, args: MultiValue| {
            let name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let row = create_compat_option_row_table(lua, &name, theme_pref_row_spec(&name))?;
            set_theme_pref_option_save(lua, &row, &name)?;
            Ok(Value::Table(row))
        })?,
    )?;
    rows.set(
        "InitAll",
        lua.create_function(|lua, args: MultiValue| {
            let defs_arg = if args.len() == 1 {
                args.front()
            } else {
                method_arg(&args, 0)
            };
            let defs = defs_arg
                .cloned()
                .filter(|value| !matches!(value, Value::Nil))
                .unwrap_or(Value::Table(create_theme_pref_defs(lua)?));
            let theme_prefs = lua.globals().get::<Table>("ThemePrefs")?;
            let init = theme_prefs.get::<Function>("InitAll")?;
            init.call::<()>((defs,))
        })?,
    )?;
    Ok(rows)
}

pub(super) fn create_sl_custom_prefs_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "Get",
        lua.create_function(|lua, _args: MultiValue| create_theme_pref_defs(lua))?,
    )?;
    table.set(
        "Validate",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    table.set(
        "Init",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
}

fn create_theme_pref_defs(lua: &Lua) -> mlua::Result<Table> {
    let defs = lua.create_table()?;
    for name in THEME_PREF_ROW_NAMES {
        defs.set(*name, create_theme_pref_def(lua, name)?)?;
    }
    Ok(defs)
}

fn create_theme_pref_def(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let spec = theme_pref_row_spec(name);
    let def = lua.create_table()?;
    def.set("Default", theme_pref_default(lua, name)?)?;
    def.set("Choices", create_lua_option_array(lua, spec.choices)?)?;
    if let Some(values) = spec.values {
        def.set("Values", create_lua_option_array(lua, values)?)?;
    }
    Ok(def)
}

fn theme_pref_row_spec(name: &str) -> SongLuaOptionRowSpec {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "numberofcontinuesallowed" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Int(OPTION_ZERO_TO_NINE))
                .values(LuaOptionValues::Int(OPTION_ZERO_TO_NINE))
                .one_choice()
        }
        "casualmaxmeter" => SongLuaOptionRowSpec::new(LuaOptionValues::Int(OPTION_CASUAL_METERS))
            .values(LuaOptionValues::Int(OPTION_CASUAL_METERS))
            .one_choice(),
        "simplylovecolor" => SongLuaOptionRowSpec::new(LuaOptionValues::Int(OPTION_ONE_TO_TWELVE))
            .values(LuaOptionValues::Int(OPTION_ONE_TO_TWELVE))
            .one_choice(),
        "nice" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NICE_CHOICES))
            .values(LuaOptionValues::Int(OPTION_NICE_VALUES))
            .one_choice(),
        "screengroovestatsloginmenutimer"
        | "screenselectmusicmenutimer"
        | "screenselectmusiccasualmenutimer"
        | "screenplayeroptionsmenutimer"
        | "screenevaluationmenutimer"
        | "screenevaluationnonstopmenutimer"
        | "screenevaluationsummarymenutimer"
        | "screennameentrymenutimer" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MENU_TIMER_CHOICES))
                .values(LuaOptionValues::Int(OPTION_MENU_TIMER_VALUES))
                .one_choice()
        }
        "visualstyle" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_VISUAL_STYLE))
            .values(LuaOptionValues::Str(OPTION_VISUAL_STYLE))
            .one_choice(),
        "defaultgamemode" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_GAME_MODE))
            .values(LuaOptionValues::Str(OPTION_GAME_MODE))
            .one_choice(),
        "autostyle" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_AUTO_STYLE))
            .values(LuaOptionValues::Str(OPTION_AUTO_STYLE))
            .one_choice(),
        "musicwheelstyle" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MUSIC_WHEEL_STYLE))
                .values(LuaOptionValues::Str(OPTION_MUSIC_WHEEL_STYLE))
                .one_choice()
        }
        "themefont" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_THEME_FONT))
            .values(LuaOptionValues::Str(OPTION_THEME_FONT))
            .one_choice(),
        "songselectbg" | "resultsbg" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_BG_STYLE))
                .values(LuaOptionValues::Str(OPTION_BG_STYLE))
                .one_choice()
        }
        "qrlogin" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_QR_LOGIN))
            .values(LuaOptionValues::Str(OPTION_QR_LOGIN))
            .one_choice(),
        "scoringsystem" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_SCORING_SYSTEM))
            .values(LuaOptionValues::Str(OPTION_SCORING_SYSTEM))
            .one_choice(),
        "stepstats" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_STEP_STATS))
            .values(LuaOptionValues::Str(OPTION_STEP_STATS))
            .one_choice(),
        "editmodelastseensong"
        | "editmodelastseendifficulty"
        | "editmodelastseenstepstype"
        | "editmodelastseenstyletype"
        | "lastactiveevent" => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_EMPTY))
            .values(LuaOptionValues::Str(OPTION_EMPTY))
            .one_choice(),
        "rainbowmode" | "animatebanners" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_ON_OFF))
                .values(LuaOptionValues::Bool(OPTION_TRUE_FALSE))
                .one_choice()
        }
        "hidestocknoteskins" | "memorycards" => {
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON))
                .values(LuaOptionValues::Bool(OPTION_FALSE_TRUE))
                .one_choice()
        }
        _ => SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_YES_NO))
            .values(LuaOptionValues::Bool(OPTION_TRUE_FALSE))
            .one_choice(),
    }
}

pub(super) fn create_operator_menu_option_rows_table(lua: &Lua) -> mlua::Result<Table> {
    let rows = lua.create_table()?;
    for method_name in [
        "Theme",
        "EditorNoteskin",
        "DefaultFailType",
        "LongAndMarathonTime",
        "MusicWheelSpeed",
        "VideoRenderer",
        "GlobalOffsetSeconds",
        "VisualDelaySeconds",
        "MemoryCards",
        "CustomSongsMaxSeconds",
        "CustomSongsMaxMegabytes",
        "CustomSongsLoadTimeout",
    ] {
        rows.set(
            method_name,
            lua.create_function({
                let method_name = method_name.to_string();
                move |lua, args: MultiValue| {
                    create_operator_menu_option_row(lua, &method_name, &args).map(Value::Table)
                }
            })?,
        )?;
    }

    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(|lua, args: MultiValue| {
            let method_name = method_arg(&args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(Value::Function(lua.create_function(
                move |lua, args: MultiValue| {
                    create_operator_menu_option_row(lua, &method_name, &args).map(Value::Table)
                },
            )?))
        })?,
    )?;
    let _ = rows.set_metatable(Some(mt));
    Ok(rows)
}

fn create_operator_menu_option_row(
    lua: &Lua,
    method_name: &str,
    args: &MultiValue,
) -> mlua::Result<Table> {
    let lower = method_name.to_ascii_lowercase();
    let (row_name, spec, pref_name) = match lower.as_str() {
        "theme" => (
            "Theme".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_THEME_NAMES)).one_choice(),
            Some("Theme".to_string()),
        ),
        "editornoteskin" => (
            "EditorNoteSkin".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_NOTESKIN))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("EditorNoteSkinP1".to_string()),
        ),
        "defaultfailtype" => (
            "DefaultFailType".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_FAIL_TYPES)).one_choice(),
            Some("DefaultModifiers".to_string()),
        ),
        "longandmarathontime" => {
            let kind = method_arg(args, 0)
                .cloned()
                .and_then(read_string)
                .unwrap_or_else(|| "Long".to_string());
            let (choices, values, pref_name) = if kind.eq_ignore_ascii_case("Marathon") {
                (
                    OPTION_MARATHON_TIME,
                    OPTION_MARATHON_TIME_VALUES,
                    "MarathonVerSongSeconds",
                )
            } else {
                (
                    OPTION_LONG_TIME,
                    OPTION_LONG_TIME_VALUES,
                    "LongVerSongSeconds",
                )
            };
            (
                format!("{kind} Time"),
                SongLuaOptionRowSpec::new(LuaOptionValues::Str(choices))
                    .values(LuaOptionValues::Int(values))
                    .layout("ShowOneInRow")
                    .one_choice(),
                Some(pref_name.to_string()),
            )
        }
        "musicwheelspeed" => (
            "MusicWheelSpeed".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_MUSIC_WHEEL_SPEED))
                .values(LuaOptionValues::Int(OPTION_MUSIC_WHEEL_SPEED_VALUES))
                .one_choice(),
            Some("MusicWheelSwitchSpeed".to_string()),
        ),
        "videorenderer" => (
            "VideoRenderer".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_VIDEO_RENDERER)).one_choice(),
            Some("VideoRenderers".to_string()),
        ),
        "globaloffsetseconds" => (
            "GlobalOffsetSeconds".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFFSET_MS))
                .values(LuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("GlobalOffsetSeconds".to_string()),
        ),
        "visualdelayseconds" => (
            "VisualDelaySeconds".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFFSET_MS))
                .values(LuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("VisualDelaySeconds".to_string()),
        ),
        "memorycards" => (
            "MemoryCards".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON))
                .values(LuaOptionValues::Bool(OPTION_FALSE_TRUE))
                .one_choice(),
            Some("MemoryCards".to_string()),
        ),
        "customsongsmaxseconds" => (
            "CustomSongsMaxSeconds".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_CUSTOM_SONG_SECONDS))
                .values(LuaOptionValues::Int(OPTION_CUSTOM_SONG_SECONDS_VALUES))
                .one_choice(),
            Some("CustomSongsMaxSeconds".to_string()),
        ),
        "customsongsmaxmegabytes" => (
            "CustomSongsMaxMegabytes".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_CUSTOM_SONG_MEGABYTES))
                .values(LuaOptionValues::Int(OPTION_CUSTOM_SONG_MEGABYTES_VALUES))
                .one_choice(),
            Some("CustomSongsMaxMegabytes".to_string()),
        ),
        "customsongsloadtimeout" => (
            "CustomSongsLoadTimeout".to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_CUSTOM_SONG_TIMEOUT))
                .values(LuaOptionValues::Int(OPTION_CUSTOM_SONG_TIMEOUT_VALUES))
                .one_choice(),
            Some("CustomSongsLoadTimeout".to_string()),
        ),
        _ => (
            method_name.to_string(),
            SongLuaOptionRowSpec::new(LuaOptionValues::Str(OPTION_OFF_ON)).one_choice(),
            None,
        ),
    };
    let row = create_compat_option_row_table(lua, &row_name, spec)?;
    if let Some(pref_name) = pref_name {
        set_pref_option_save(lua, &row, &pref_name)?;
    }
    Ok(row)
}

pub(super) fn create_profileman_table(lua: &Lua) -> mlua::Result<Table> {
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

pub(super) fn create_statsman_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
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

pub(super) fn create_life_record_table(
    lua: &Lua,
    samples: usize,
    life: f32,
) -> mlua::Result<Table> {
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

pub(super) fn create_display_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
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

pub(super) fn create_memcardman_table(lua: &Lua) -> mlua::Result<Table> {
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

pub(super) fn create_unlockman_table(lua: &Lua) -> mlua::Result<Table> {
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

pub(super) fn create_hooks_table(lua: &Lua) -> mlua::Result<Table> {
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

fn song_lua_arch_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "Mac OS X"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "freebsd") {
        "FreeBSD"
    } else {
        "Unknown"
    }
}

pub(super) fn create_noteskin_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<Table> {
    let noteskin = lua.create_table()?;
    let default_noteskin = song_lua_default_noteskin_name(context);

    let default_metric_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetric",
        lua.create_function(
            move |lua, (_self, element, value): (Table, String, String)| {
                let Some(metric) = crate::game::parsing::noteskin::song_lua_noteskin_metric(
                    &default_metric_skin,
                    &element,
                    &value,
                ) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricForNoteSkin",
        lua.create_function(
            move |lua, (_self, element, value, skin): (Table, String, String, String)| {
                let Some(metric) = crate::game::parsing::noteskin::song_lua_noteskin_metric(
                    &skin, &element, &value,
                ) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;

    let default_metric_f_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricF",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_f(
                &default_metric_f_skin,
                &element,
                &value,
            )
            .unwrap_or(0.0_f32))
        })?,
    )?;
    noteskin.set(
        "GetMetricFForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_f(
                    &skin, &element, &value,
                )
                .unwrap_or(0.0_f32))
            },
        )?,
    )?;

    let default_metric_i_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricI",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(song_lua_noteskin_metric_i(
                &default_metric_i_skin,
                &element,
                &value,
            ))
        })?,
    )?;
    noteskin.set(
        "GetMetricIForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(song_lua_noteskin_metric_i(&skin, &element, &value))
            },
        )?,
    )?;

    let default_metric_b_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricB",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_b(
                &default_metric_b_skin,
                &element,
                &value,
            )
            .unwrap_or(false))
        })?,
    )?;
    noteskin.set(
        "GetMetricBForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(crate::game::parsing::noteskin::song_lua_noteskin_metric_b(
                    &skin, &element, &value,
                )
                .unwrap_or(false))
            },
        )?,
    )?;

    noteskin.set(
        "GetMetricA",
        lua.create_function(
            move |lua, (_self, _element, _value): (Table, String, String)| {
                song_lua_noteskin_metric_a(lua)
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricAForNoteSkin",
        lua.create_function(
            move |lua, (_self, _element, _value, _skin): (Table, String, String, String)| {
                song_lua_noteskin_metric_a(lua)
            },
        )?,
    )?;

    let default_path_skin = default_noteskin.clone();
    noteskin.set(
        "GetPath",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                let path = song_lua_noteskin_path(&default_path_skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetPathForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                let path = song_lua_noteskin_path(&skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;

    let default_load_skin = default_noteskin.clone();
    noteskin.set(
        "LoadActor",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                song_lua_noteskin_actor(lua, &default_load_skin, &button, &element)
            },
        )?,
    )?;
    noteskin.set(
        "LoadActorForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                song_lua_noteskin_actor(lua, &skin, &button, &element)
            },
        )?,
    )?;

    noteskin.set(
        "DoesNoteSkinExist",
        lua.create_function(|_, (_self, skin): (Table, String)| {
            Ok(crate::game::parsing::noteskin::song_lua_noteskin_exists(
                &skin,
            ))
        })?,
    )?;
    noteskin.set(
        "GetNoteSkinNames",
        lua.create_function(|lua, _args: MultiValue| {
            let names = crate::game::parsing::noteskin::discover_itg_skins("dance");
            let table = lua.create_table()?;
            for (idx, name) in names.into_iter().enumerate() {
                table.raw_set(idx + 1, name)?;
            }
            Ok(table)
        })?,
    )?;
    noteskin.set(
        "HasVariants",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "IsNoteSkinVariant",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "GetVariantNamesForNoteSkin",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    Ok(noteskin)
}

fn song_lua_default_noteskin_name(context: &SongLuaCompileContext) -> String {
    context
        .players
        .iter()
        .find(|player| player.enabled)
        .map(|player| player.noteskin_name.clone())
        .or_else(|| {
            context
                .players
                .first()
                .map(|player| player.noteskin_name.clone())
        })
        .unwrap_or_else(|| crate::game::profile::NoteSkin::default().to_string())
}

fn song_lua_noteskin_path(skin: &str, button: &str, element: &str) -> String {
    crate::game::parsing::noteskin::song_lua_noteskin_resolve_path(skin, button, element)
        .map(|path| file_path_string(path.as_path()))
        .unwrap_or_default()
}

fn song_lua_noteskin_metric_i(skin: &str, element: &str, value: &str) -> i64 {
    let Some(metric) =
        crate::game::parsing::noteskin::song_lua_noteskin_metric(skin, element, value)
    else {
        return 0;
    };
    let metric = metric.trim();
    metric
        .parse::<i64>()
        .ok()
        .or_else(|| {
            metric
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| value.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64)
        })
        .unwrap_or(0)
}

fn song_lua_noteskin_metric_a(lua: &Lua) -> mlua::Result<Function> {
    lua.create_function(|_, actor: Table| Ok(Value::Table(actor)))
}

fn song_lua_noteskin_actor(
    lua: &Lua,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<Table> {
    let resolved =
        crate::game::parsing::noteskin::song_lua_noteskin_resolve_path(skin, button, element);
    let model_path = resolved
        .as_ref()
        .and_then(|path| song_lua_noteskin_model_template_path(skin, path));
    let sprite_path = resolved
        .as_ref()
        .filter(|_| model_path.is_none())
        .filter(|path| is_song_lua_image_path(path));
    let actor = create_dummy_actor(
        lua,
        if model_path.is_some() {
            "Model"
        } else if sprite_path.is_some() {
            "Sprite"
        } else {
            "Actor"
        },
    )?;
    tag_song_lua_noteskin_actor(&actor, skin, button, element)?;
    if let Some(path) = sprite_path {
        actor.set("Texture", file_path_string(path.as_path()))?;
    }
    if let Some(path) = model_path {
        let path = file_path_string(path.as_path());
        actor.set("Meshes", path.clone())?;
        actor.set("Materials", path.clone())?;
        actor.set("Bones", path)?;
    }
    Ok(actor)
}

fn song_lua_noteskin_model_template_path(skin: &str, template_path: &Path) -> Option<PathBuf> {
    if !template_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        return None;
    }
    let script = fs::read_to_string(template_path).ok()?;
    if !script.to_ascii_lowercase().contains("def.model") {
        return None;
    }
    if let Some((button, element)) = first_noteskin_get_path_args(&script) {
        return crate::game::parsing::noteskin::song_lua_noteskin_resolve_path(
            skin, &button, &element,
        );
    }
    let raw = first_lua_field_string(&script, "Meshes")
        .or_else(|| first_lua_field_string(&script, "Materials"))
        .or_else(|| first_lua_field_string(&script, "Bones"))?;
    resolve_noteskin_template_relative_path(template_path, &raw)
}

fn first_noteskin_get_path_args(script: &str) -> Option<(String, String)> {
    let lower = script.to_ascii_lowercase();
    if let Some(start) = lower.find("noteskin:getpath") {
        let open = script[start..].find('(').map(|idx| start + idx + 1)?;
        let (button, next) = parse_lua_string(script, open)?;
        let comma = script[next..].find(',').map(|idx| next + idx + 1)?;
        let (element, _) = parse_lua_string(script, comma)?;
        return Some((button, element));
    }
    None
}

fn first_lua_field_string(script: &str, field: &str) -> Option<String> {
    let lower = script.to_ascii_lowercase();
    let needle = field.to_ascii_lowercase();
    let start = lower.find(&needle)?;
    let eq = script[start + field.len()..]
        .find('=')
        .map(|idx| start + field.len() + idx + 1)?;
    parse_lua_string(script, eq).map(|(value, _)| value)
}

fn parse_lua_string(script: &str, mut cursor: usize) -> Option<(String, usize)> {
    let bytes = script.as_bytes();
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let quote = *bytes.get(cursor)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    cursor += 1;
    let mut out = String::new();
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        cursor += 1;
        if byte == quote {
            return Some((out, cursor));
        }
        if byte == b'\\' {
            if let Some(next) = bytes.get(cursor).copied() {
                cursor += 1;
                out.push(next as char);
            }
        } else {
            out.push(byte as char);
        }
    }
    None
}

fn resolve_noteskin_template_relative_path(template_path: &Path, raw: &str) -> Option<PathBuf> {
    let normalized = raw.replace('\\', "/");
    let raw_path = Path::new(normalized.trim());
    if raw_path.is_absolute() && raw_path.is_file() {
        return Some(raw_path.to_path_buf());
    }
    let candidate = template_path.parent()?.join(raw_path);
    candidate.is_file().then_some(candidate)
}

fn tag_song_lua_noteskin_actor(
    actor: &Table,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<()> {
    actor.set("__songlua_noteskin_name", skin.trim().to_ascii_lowercase())?;
    actor.set("__songlua_noteskin_button", button)?;
    actor.set("__songlua_noteskin_element", element)
}

pub(super) fn theme_string(section: &str, name: &str) -> String {
    if section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
    {
        return name.trim_start_matches("Difficulty_").to_string();
    }
    if matches!(
        section,
        "OptionTitles"
            | "OptionNames"
            | "ThemePrefs"
            | "SLPlayerOptions"
            | "ScreenSelectPlayMode"
            | "ScreenSelectStyle"
            | "GameButton"
            | "TapNoteScore"
            | "TapNoteScoreFA+"
            | "HoldNoteScore"
            | "Stage"
            | "Months"
    ) {
        return name.replace('_', " ");
    }
    match name {
        "Yes" => "Yes".to_string(),
        "No" => "No".to_string(),
        "Cancel" => "Cancel".to_string(),
        _ => name.to_string(),
    }
}

fn theme_has_string(section: &str, name: &str) -> bool {
    section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
        || matches!(
            section,
            "OptionTitles"
                | "OptionNames"
                | "ThemePrefs"
                | "SLPlayerOptions"
                | "ScreenSelectPlayMode"
                | "ScreenSelectStyle"
                | "GameButton"
                | "TapNoteScore"
                | "TapNoteScoreFA+"
                | "HoldNoteScore"
                | "Stage"
                | "Months"
        )
        || matches!(name, "Yes" | "No" | "Cancel")
}

fn theme_pref_default(lua: &Lua, name: &str) -> mlua::Result<Value> {
    let lower = name.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "casualmaxmeter"
            | "numberofcontinuesallowed"
            | "screenselectmusicmenutimer"
            | "screenselectmusiccasualmenutimer"
            | "screenplayeroptionsmenutimer"
            | "screenevaluationmenutimer"
            | "screenevaluationnonstopmenutimer"
            | "screenevaluationsummarymenutimer"
            | "screennameentrymenutimer"
            | "screengroovestatsloginmenutimer"
            | "simplylovecolor"
            | "nice"
    ) {
        return Ok(Value::Integer(match lower.as_str() {
            "casualmaxmeter" => 12,
            "simplylovecolor" => 1,
            _ => 0,
        }));
    }
    if matches!(
        lower.as_str(),
        "visualstyle"
            | "lastactiveevent"
            | "musicwheelstyle"
            | "themefont"
            | "defaultgamemode"
            | "autostyle"
            | "songselectbg"
            | "resultsbg"
            | "scoringsystem"
            | "stepstats"
            | "editmodelastseensong"
            | "editmodelastseendifficulty"
            | "editmodelastseenstepstype"
            | "editmodelastseenstyletype"
    ) {
        let value = match lower.as_str() {
            "themefont" => "Common",
            "defaultgamemode" => "Dance",
            "songselectbg" | "resultsbg" => "Off",
            "musicwheelstyle" => "Default",
            "autostyle" => "Default",
            _ => "",
        };
        return Ok(Value::String(lua.create_string(value)?));
    }
    Ok(Value::Boolean(matches!(lower.as_str(), "useimagecache")))
}
