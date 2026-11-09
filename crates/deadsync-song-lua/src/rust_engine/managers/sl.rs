//! Simply Love state owned by this song runtime.
use super::fields::{array, object, set};
use super::*;
use crate::{
    DDR_DIFF_COLORS, ITG_DIFF_COLORS, LUA_PLAYERS, SL_COLORS, SL_DECORATIVE_COLORS,
    SL_FA_PLUS_COLORS, SL_JUDGMENT_COLORS, SONG_LUA_ACTIVE_COLOR_INDEX, SONG_LUA_NOTE_COLUMNS,
    SongLuaCompileContext, SongLuaPlayerContext, parse_color_text, player_short_name,
    song_lua_speedmod_parts, song_music_rate_value,
};

#[derive(Clone)]
struct StateContext {
    song_music_rate: f32,
    players: [SongLuaPlayerContext; LUA_PLAYERS],
}

pub(super) fn install(vm: &mut Vm, context: &SongLuaCompileContext) -> Result<(), Error> {
    let state = state(vm, context)?;
    vm.set_global(b"SL", state)?;
    let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?];
    let parse = vm.new_host_function(parse_chart_info, &captures)?;
    vm.set_global(b"ParseChartInfo", parse)?;
    let program = deadlib_lua::compile_named(
        include_bytes!("column_cues.lua"),
        b"@theme:/Scripts/SL-ChartParser.lua",
    )?;
    let parse = vm.execute(&program)?[0];
    vm.set_global(b"ParseColumnCues", parse)
}
pub(super) fn state(vm: &mut Vm, context: &SongLuaCompileContext) -> Result<Value, Error> {
    let captures = [vm.get_global(b"_G")?];
    let context = StateContext {
        song_music_rate: context.song_music_rate,
        players: context.players.clone(),
    };
    let initialize = vm.new_host_function_with_data(initialize, &captures, context)?;
    Ok(vm.call(initialize, &[])?[0])
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let context = call
        .function_data::<StateContext>()
        .expect("SL context")
        .clone();
    let state = create_sl_table(call, &context)?;
    call.push(Value::Table(state))
}
fn get(call: &mut HostCall<'_>, target: Value, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(target, key)
}
fn string_array(call: &mut HostCall<'_>, values: &[&str]) -> Result<TableId, Error> {
    array(call, values.iter().copied())
}
fn bool_array(call: &mut HostCall<'_>, values: &[bool]) -> Result<TableId, Error> {
    array(call, values.iter().copied())
}
fn create_color_array(call: &mut HostCall<'_>, values: &[&str]) -> Result<TableId, Error> {
    object(call, |call, table| {
        for (i, value) in values.iter().enumerate() {
            let color = array(call, parse_color_text(value).unwrap_or([1.; 4]))?;
            set(table, i + 1, color, call)?;
        }
        Ok(())
    })
}
fn noop(_: &mut HostCall<'_>) -> Result<(), Error> {
    Ok(())
}
fn logo(call: &mut HostCall<'_>) -> Result<(), Error> {
    let text = call.try_intern(b"Logo.png")?;
    call.push(text)
}
fn create_sl_srpg9(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(
            table,
            "Colors",
            string_array(call, SL_DECORATIVE_COLORS)?,
            call,
        )?;
        set(table, "TextColor", "#ffffff", call)?;
        set(table, "GetLogo", call.new_host_function(logo, &[])?, call)?;
        set(
            table,
            "MaybeRandomizeColor",
            call.new_host_function(noop, &[])?,
            call,
        )
    })
}
fn create_sl_table(call: &mut HostCall<'_>, context: &StateContext) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(
            table,
            "Global",
            create_sl_global_table(call, context)?,
            call,
        )?;
        set(table, "Colors", string_array(call, SL_COLORS)?, call)?;
        set(
            table,
            "DecorativeColors",
            string_array(call, SL_DECORATIVE_COLORS)?,
            call,
        )?;
        set(
            table,
            "ITGDiffColors",
            string_array(call, ITG_DIFF_COLORS)?,
            call,
        )?;
        set(
            table,
            "DDRDiffColors",
            string_array(call, DDR_DIFF_COLORS)?,
            call,
        )?;
        set(
            table,
            "JudgmentColors",
            create_sl_judgment_colors(call)?,
            call,
        )?;
        set(
            table,
            "Preferences",
            create_sl_mode_table(call, create_sl_preferences)?,
            call,
        )?;
        set(
            table,
            "Metrics",
            create_sl_mode_table(call, create_sl_metrics)?,
            call,
        )?;
        set(table, "GrooveStats", create_sl_groovestats(call)?, call)?;
        set(table, "ArrowCloud", create_sl_arrowcloud(call)?, call)?;
        set(table, "Downloads", call.try_new_table()?, call)?;
        set(table, "SRPG9", create_sl_srpg9(call)?, call)?;
        for player in 0..LUA_PLAYERS {
            set(
                table,
                player_short_name(player),
                create_sl_player_table(call, &context.players[player])?,
                call,
            )?;
        }
        Ok(())
    })
}

fn create_sl_global_table(
    call: &mut HostCall<'_>,
    context: &StateContext,
) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "GameMode", "ITG", call)?;
        set(table, "ActiveColorIndex", SONG_LUA_ACTIVE_COLOR_INDEX, call)?;
        set(
            table,
            "ActiveModifiers",
            create_sl_active_mods(call, context.song_music_rate)?,
            call,
        )?;
        set(table, "Stages", create_sl_stages(call)?, call)?;
        set(table, "MenuTimer", create_sl_menu_timer(call)?, call)?;
        set(table, "ScreenAfter", create_sl_screen_after(call)?, call)?;
        set(
            table,
            "PrevScreenOptionsServiceRow",
            call.try_new_table()?,
            call,
        )?;
        set(table, "Online", call.try_new_table()?, call)?;
        set(table, "SampleMusicLoops", true, call)?;
        set(table, "SampleMusicStartsImmediately", false, call)?;
        set(table, "GameplayReloadCheck", false, call)?;
        set(table, "WheelLocked", false, call)?;
        set(table, "ContinuesRemaining", 0, call)?;
        set(table, "ColumnCueMinTime", 1.5, call)?;
        set(table, "TimeAtSessionStart", 0.0, call)?;
        Ok(())
    })
}

fn create_sl_active_mods(call: &mut HostCall<'_>, music_rate: f32) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "MusicRate", song_music_rate_value(music_rate), call)?;
        Ok(())
    })
}

fn create_sl_player_table(
    call: &mut HostCall<'_>,
    player: &SongLuaPlayerContext,
) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "ApiKey", "", call)?;
        set(table, "ArrowCloudApiKey", "", call)?;
        set(
            table,
            "ActiveModifiers",
            create_sl_player_mods(call, player)?,
            call,
        )?;
        set(table, "Stages", create_sl_stages(call)?, call)?;
        set(table, "Streams", create_sl_streams(call)?, call)?;
        set(table, "HighScores", create_sl_high_scores(call)?, call)?;
        set(table, "Favorites", call.try_new_table()?, call)?;
        Ok(())
    })
}

fn create_sl_player_mods(
    call: &mut HostCall<'_>,
    player: &SongLuaPlayerContext,
) -> Result<TableId, Error> {
    object(call, |call, table| {
        let (speed_type, speed_value) = song_lua_speedmod_parts(player.speedmod);
        // SL_Init.call initializes an unmodified profile with Step Statistics.
        set(
            table,
            "DataVisualizations",
            if player.step_statistics_enabled {
                "Step Statistics"
            } else {
                "None"
            },
            call,
        )?;
        set(table, "ShowFaPlusWindow", false, call)?;
        set(table, "ShowExScore", false, call)?;
        set(table, "ShowHardEXScore", false, call)?;
        set(table, "ShowFaPlusPane", false, call)?;
        set(
            table,
            "TimingWindows",
            bool_array(call, &player.timing_windows)?,
            call,
        )?;
        set(table, "SpeedModType", speed_type, call)?;
        set(table, "SpeedMod", speed_value, call)?;
        set(table, "Mini", "0%", call)?;
        set(table, "Spacing", "0%", call)?;
        set(table, "VisualDelay", "0ms", call)?;
        let filter = player.background_filter_percent.min(100);
        match filter {
            0 => set(table, "BackgroundFilter", "Off", call)?,
            50 => set(table, "BackgroundFilter", "Dark", call)?,
            75 => set(table, "BackgroundFilter", "Darker", call)?,
            95 => set(table, "BackgroundFilter", "Darkest", call)?,
            // The production profile also supports arbitrary percentages.
            value => set(table, "BackgroundFilter", value, call)?,
        }
        set(table, "HideTargets", false, call)?;
        set(table, "HideSongBG", false, call)?;
        set(table, "HideCombo", false, call)?;
        set(table, "HideLifebar", player.hide_lifebar, call)?;
        set(table, "HideScore", false, call)?;
        set(table, "HideDanger", player.hide_danger, call)?;
        set(table, "HideComboExplosions", false, call)?;
        set(table, "ColumnFlashOnMiss", false, call)?;
        set(table, "SubtractiveScoring", false, call)?;
        set(table, "MeasureCounter", "None", call)?;
        set(table, "MeasureCounterLeft", false, call)?;
        set(table, "MeasureCounterUp", true, call)?;
        set(table, "HideLookahead", false, call)?;
        set(table, "MeasureLines", "Off", call)?;
        set(table, "TargetScore", "Personal best", call)?;
        set(table, "TargetScoreNumber", 100, call)?;
        set(table, "ActionOnMissedTarget", "Nothing", call)?;
        set(table, "Pacemaker", false, call)?;
        set(
            table,
            "LifeMeterType",
            player.life_meter_type.as_str(),
            call,
        )?;
        set(table, "NPSGraphAtTop", player.nps_graph_at_top, call)?;
        set(table, "JudgmentTilt", false, call)?;
        set(table, "TiltMultiplier", 1, call)?;
        set(table, "ColumnCues", player.column_cues, call)?;
        set(table, "ColumnCountdown", false, call)?;
        set(table, "ShowHeldMiss", false, call)?;
        set(table, "DisplayScorebox", true, call)?;
        set(table, "ErrorBar", "None", call)?;
        set(table, "ErrorBarUp", false, call)?;
        set(table, "ErrorBarMultiTick", false, call)?;
        set(table, "ErrorBarTrim", "Off", call)?;
        set(table, "ErrorBarCap", 5, call)?;
        set(table, "HideEarlyDecentWayOffJudgments", false, call)?;
        set(table, "HideEarlyDecentWayOffFlash", false, call)?;
        set(table, "HideEarlyDecentWayOffColumnFlash", false, call)?;
        set(table, "FlashMiss", true, call)?;
        set(table, "FlashWayOff", false, call)?;
        set(table, "FlashDecent", false, call)?;
        set(table, "FlashGreat", false, call)?;
        set(table, "FlashExcellent", false, call)?;
        set(table, "FlashFantastic", false, call)?;
        set(table, "ComboColors", "Glow", call)?;
        set(table, "ComboMode", "FullCombo", call)?;
        set(table, "TimerMode", "Time", call)?;
        set(table, "JudgmentAnimation", "Default", call)?;
        set(table, "RailBalance", "No", call)?;
        set(table, "NoteFieldOffsetX", 0, call)?;
        set(table, "NoteFieldOffsetY", 0, call)?;
        set(table, "HeldGraphic", "None", call)?;
        set(table, "NoteSkin", player.noteskin_name.as_str(), call)?;
        set(table, "NoteSkinVariant", "default", call)?;
        set(table, "JudgmentGraphic", "None", call)?;
        set(table, "ComboFont", "None", call)?;
        set(table, "PlayerOptionsString", "", call)?;
        Ok(())
    })
}

fn create_sl_stages(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        let stats = call.try_new_table()?;
        set(stats, 1, create_sl_stage_stat(call)?, call)?;
        set(table, "PlayedThisGame", 0, call)?;
        set(table, "Remaining", 1, call)?;
        set(table, "Stats", stats, call)?;
        Ok(())
    })
}

fn create_sl_stage_stat(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        let column_judgments = call.try_new_table()?;
        for column in 1..=SONG_LUA_NOTE_COLUMNS {
            set(
                column_judgments,
                column,
                create_sl_column_judgments(call)?,
                call,
            )?;
        }
        set(table, "MusicRate", 1.0, call)?;
        set(table, "DeathSecond", 0.0, call)?;
        set(table, "worst_window", "W3", call)?;
        set(table, "sequential_offsets", call.try_new_table()?, call)?;
        set(table, "column_judgments", column_judgments, call)?;
        set(table, "ex_counts", create_sl_ex_counts(call)?, call)?;
        Ok(())
    })
}

fn create_sl_column_judgments(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        let early = call.try_new_table()?;
        for name in ["W0", "W1", "W2", "W3", "W4", "W5", "Miss"] {
            set(table, name, 0, call)?;
            set(table, format!("{name}early"), 0, call)?;
            set(table, format!("{name}lf"), 0, call)?;
            set(table, format!("{name}rf"), 0, call)?;
            set(early, name, 0, call)?;
        }
        set(table, "Early", early, call)?;
        set(table, "MissBecauseHeld", 0, call)?;
        Ok(())
    })
}

fn create_sl_ex_counts(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        for name in ["W0_total", "W1", "W2", "W3", "W4", "W5", "Miss"] {
            set(table, name, 0, call)?;
        }
        Ok(())
    })
}

fn create_sl_high_scores(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "EnteringName", false, call)?;
        Ok(())
    })
}

fn create_sl_menu_timer(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        for name in [
            "ScreenGrooveStatsLogin",
            "ScreenNameEntry",
            "ScreenPlayerOptions",
            "ScreenSelectMusic",
            "ScreenSelectMusicCasual",
            "ScreenEvaluation",
            "ScreenEvaluationNonstop",
            "ScreenEvaluationSummary",
        ] {
            set(table, name, 0, call)?;
        }
        Ok(())
    })
}

fn create_sl_screen_after(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "PlayAgain", "ScreenSelectMusic", call)?;
        set(table, "PlayerOptions", "ScreenGameplay", call)?;
        set(table, "PlayerOptions2", "ScreenGameplay", call)?;
        set(table, "PlayerOptions3", "ScreenGameplay", call)?;
        set(table, "PlayerOptions4", "ScreenGameplay", call)?;
        Ok(())
    })
}

fn create_sl_judgment_colors(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(
            table,
            "Casual",
            create_color_array(call, SL_JUDGMENT_COLORS)?,
            call,
        )?;
        set(
            table,
            "ITG",
            create_color_array(call, SL_JUDGMENT_COLORS)?,
            call,
        )?;
        set(
            table,
            "FA+",
            create_color_array(call, SL_FA_PLUS_COLORS)?,
            call,
        )?;
        Ok(())
    })
}

fn create_sl_groovestats(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "IsConnected", false, call)?;
        set(table, "GetScores", false, call)?;
        set(table, "Leaderboard", false, call)?;
        set(table, "AutoSubmit", false, call)?;
        set(table, "ChartHashVersion", "2", call)?;
        set(table, "RequestCache", call.try_new_table()?, call)?;
        set(table, "UnlocksCache", call.try_new_table()?, call)?;
        Ok(())
    })
}

fn create_sl_arrowcloud(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "Enabled", false, call)?;
        set(table, "BaseURL", "https://api.arrowcloud.dance", call)?;
        set(table, "RequestTimeout", 5, call)?;
        Ok(())
    })
}

fn create_sl_mode_table(
    call: &mut HostCall<'_>,
    create: fn(&mut HostCall<'_>, &str) -> Result<TableId, Error>,
) -> Result<TableId, Error> {
    object(call, |call, table| {
        for mode in ["Casual", "ITG", "FA+"] {
            set(table, mode, create(call, mode)?, call)?;
        }
        Ok(())
    })
}

fn create_sl_preferences(call: &mut HostCall<'_>, mode: &str) -> Result<TableId, Error> {
    object(call, |call, table| {
        set(table, "TimingWindowAdd", 0.0015, call)?;
        set(
            table,
            "RegenComboAfterMiss",
            if mode == "Casual" { 0 } else { 5 },
            call,
        )?;
        set(
            table,
            "MaxRegenComboAfterMiss",
            if mode == "Casual" { 0 } else { 10 },
            call,
        )?;
        set(
            table,
            "MinTNSToHideNotes",
            if mode == "FA+" {
                "TapNoteScore_W4"
            } else {
                "TapNoteScore_W3"
            },
            call,
        )?;
        if mode != "Casual" {
            let globals = call.captures()[0];
            let prefs = get(call, globals, "ThemePrefs")?;
            let rescore = if prefs == Value::Nil {
                false
            } else {
                super::table(call, prefs)?;
                call.with_roots(&[prefs], |call| {
                    let get = get(call, prefs, "Get")?;
                    if !matches!(get, Value::Function(_) | Value::Native(_)) {
                        return Err(call.error(b"theme preference getter must be a function"));
                    }
                    let key = call.try_intern(b"RescoreEarlyHits")?;
                    let result = call
                        .call(get, &[key])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    Ok(!matches!(result, Value::Nil | Value::Boolean(false)))
                })?
            };
            set(
                table,
                "MinTNSToScoreNotes",
                if !rescore {
                    "TapNoteScore_None"
                } else if mode == "FA+" {
                    "TapNoteScore_W4"
                } else {
                    "TapNoteScore_W3"
                },
                call,
            )?;
        }
        set(table, "HarshHotLifePenalty", true, call)?;
        set(table, "PercentageScoring", true, call)?;
        set(table, "AllowW1", "AllowW1_Everywhere", call)?;
        set(table, "SubSortByNumSteps", true, call)?;
        let w1 = if mode == "FA+" { 0.0135 } else { 0.0215 };
        let w2 = if mode == "FA+" { 0.0215 } else { 0.043 };
        let w3 = if mode == "FA+" { 0.043 } else { 0.102 };
        set(table, "TimingWindowSecondsW1", w1, call)?;
        set(table, "TimingWindowSecondsW2", w2, call)?;
        set(table, "TimingWindowSecondsW3", w3, call)?;
        set(
            table,
            "TimingWindowSecondsW4",
            if mode == "ITG" { 0.135 } else { 0.102 },
            call,
        )?;
        set(
            table,
            "TimingWindowSecondsW5",
            match mode {
                "Casual" => 0.102,
                "FA+" => 0.135,
                _ => 0.18,
            },
            call,
        )?;
        set(table, "TimingWindowSecondsHold", 0.32, call)?;
        set(table, "TimingWindowSecondsMine", 0.07, call)?;
        set(table, "TimingWindowSecondsRoll", 0.35, call)?;
        Ok(())
    })
}

fn create_sl_metrics(call: &mut HostCall<'_>, mode: &str) -> Result<TableId, Error> {
    object(call, |call, table| {
        // Simply Love Scripts/SL_Init.call defines separate scoring and life rules
        // for Casual, ITG and FA+. Score and grade weights share the same values.
        let scores = match mode {
            "Casual" => [3, 2, 1, 0, 0, 0, 0, 3, -1, 0],
            "FA+" => [5, 5, 4, 2, 0, -12, 0, 5, -6, 0],
            _ => [5, 4, 2, 0, -6, -12, 0, 5, -6, 0],
        };
        for (name, value) in [
            "W1",
            "W2",
            "W3",
            "W4",
            "W5",
            "Miss",
            "LetGo",
            "Held",
            "HitMine",
            "CheckpointHit",
        ]
        .into_iter()
        .zip(scores)
        {
            for prefix in ["PercentScoreWeight", "GradeWeight"] {
                set(table, format!("{prefix}{name}"), value, call)?;
            }
        }
        let life = match mode {
            "Casual" => [0.0; 9],
            "FA+" => [0.008, 0.008, 0.008, 0.004, 0.0, -0.1, -0.08, 0.008, -0.05],
            _ => [0.008, 0.008, 0.004, 0.0, -0.05, -0.1, -0.08, 0.008, -0.05],
        };
        for (name, value) in [
            "W1", "W2", "W3", "W4", "W5", "Miss", "LetGo", "Held", "HitMine",
        ]
        .into_iter()
        .zip(life)
        {
            set(table, format!("LifePercentChange{name}"), value, call)?;
        }
        set(table, "InitialValue", 0.5, call)?;
        Ok(())
    })
}

const STREAM_TABLES: &[&str] = &[
    "NotesPerMeasure",
    "EquallySpacedPerMeasure",
    "NPSperMeasure",
    "ColumnCues",
];
const STREAM_NUMBERS: &[&str] = &[
    "PeakNPS",
    "Crossovers",
    "Footswitches",
    "Sideswitches",
    "Jacks",
    "Brackets",
];
const STREAM_STRINGS: &[&str] = &["Hash", "Filename", "StepsType", "Difficulty", "Description"];

fn init_streams(call: &mut HostCall<'_>, streams: Value) -> Result<(), Error> {
    for &name in STREAM_TABLES {
        if !matches!(get(call, streams, name)?, Value::Table(_)) {
            let value = Value::Table(call.try_new_table()?);
            assign(call, streams, name, value)?;
        }
    }
    for &name in STREAM_NUMBERS {
        if get(call, streams, name)? == Value::Nil {
            assign(call, streams, name, Value::Number(0.))?;
        }
    }
    for &name in STREAM_STRINGS {
        if get(call, streams, name)? == Value::Nil {
            let value = call.try_intern(b"")?;
            assign(call, streams, name, value)?;
        }
    }
    Ok(())
}
fn create_sl_streams(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| init_streams(call, Value::Table(table)))
}
fn assign(call: &mut HostCall<'_>, target: Value, name: &str, value: Value) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let key = call.try_intern(name.as_bytes())?;
        call.table_set(target, key, value)
    })
}
fn ensure(
    call: &mut HostCall<'_>,
    parent: Value,
    name: &str,
    streams: bool,
) -> Result<Value, Error> {
    let value = get(call, parent, name)?;
    if value != Value::Nil {
        super::table(call, value)?;
        return Ok(value);
    }
    let value = Value::Table(if streams {
        create_sl_streams(call)?
    } else {
        call.try_new_table()?
    });
    assign(call, parent, name, value)?;
    Ok(value)
}
fn player(call: &HostCall<'_>) -> usize {
    match call.arguments().get(1).copied().unwrap_or(Value::Nil) {
        Value::Number(1.) => 1,
        Value::String(id)
            if call.string_bytes(id).is_some_and(|s| {
                s.eq_ignore_ascii_case(b"P2") || s.eq_ignore_ascii_case(b"PlayerNumber_P2")
            }) =>
        {
            1
        }
        _ => 0,
    }
}
fn parse_chart_info(call: &mut HostCall<'_>) -> Result<(), Error> {
    let player = player(call);
    let steps = call.arguments().first().copied().unwrap_or(Value::Nil);
    let sl = ensure(call, call.captures()[0], "SL", false)?;
    call.with_roots(&[sl], |call| {
        let player_state = ensure(call, sl, crate::player_short_name(player), false)?;
        call.with_roots(&[player_state], |call| {
            let streams = ensure(call, player_state, "Streams", true)?;
            call.with_roots(&[streams], |call| {
                init_streams(call, streams)?;
                if let Value::Table(_) = steps {
                    let pn = call.try_intern(crate::player_number_name(player).as_bytes())?;
                    call.with_roots(&[pn], |call| {
                        for (field, method) in [
                            ("NotesPerMeasure", "GetNotesPerMeasure"),
                            ("NPSperMeasure", "GetNpsPerMeasure"),
                            ("PeakNPS", "GetPeakNps"),
                        ] {
                            let function = get(call, steps, method)?;
                            if !matches!(function, Value::Function(_) | Value::Native(_)) {
                                return Err(
                                    call.error(b"chart statistic method must be a function")
                                );
                            }
                            let value = call
                                .call(function, &[steps, pn])?
                                .first()
                                .copied()
                                .unwrap_or(Value::Nil);
                            set(super::table(call, streams)?, field, value, call)?;
                        }
                        Ok(())
                    })?;
                }
                side_effect(call, call.captures()[0], call.captures()[1])?;
                call.push(streams)
            })
        })
    })
}
