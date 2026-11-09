#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{Lua, MultiValue, Table, Value};

use crate::DDR_DIFF_COLORS;
use crate::ITG_DIFF_COLORS;
use crate::LUA_PLAYERS;
use crate::SL_COLORS;
use crate::SL_DECORATIVE_COLORS;
use crate::SL_FA_PLUS_COLORS;
use crate::SL_JUDGMENT_COLORS;
use crate::SONG_LUA_ACTIVE_COLOR_INDEX;
use crate::SONG_LUA_NOTE_COLUMNS;
use crate::SongLuaCompileContext;
use crate::SongLuaPlayerContext;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::create_bool_array;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::create_string_array;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::make_color_table;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::note_song_lua_side_effect;
use crate::parse_color_text;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::player_index_from_value;
use crate::player_short_name;
use crate::song_lua_speedmod_parts;
use crate::song_music_rate_value;

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn create_sl_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Global", create_sl_global_table(lua, context)?)?;
    table.set("Colors", create_string_array(lua, SL_COLORS)?)?;
    table.set(
        "DecorativeColors",
        create_string_array(lua, SL_DECORATIVE_COLORS)?,
    )?;
    table.set("ITGDiffColors", create_string_array(lua, ITG_DIFF_COLORS)?)?;
    table.set("DDRDiffColors", create_string_array(lua, DDR_DIFF_COLORS)?)?;
    table.set("JudgmentColors", create_sl_judgment_colors(lua)?)?;
    table.set(
        "Preferences",
        create_sl_mode_table(lua, create_sl_preferences)?,
    )?;
    table.set("Metrics", create_sl_mode_table(lua, create_sl_metrics)?)?;
    table.set("GrooveStats", create_sl_groovestats(lua)?)?;
    table.set("ArrowCloud", create_sl_arrowcloud(lua)?)?;
    table.set("Downloads", lua.create_table()?)?;
    table.set("SRPG9", create_sl_srpg9(lua)?)?;
    for player in 0..LUA_PLAYERS {
        table.set(
            player_short_name(player),
            create_sl_player_table(lua, &context.players[player])?,
        )?;
    }
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn create_sl_streams(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    init_sl_streams(lua, &table)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn init_sl_streams(lua: &Lua, table: &Table) -> mlua::Result<()> {
    for name in [
        "NotesPerMeasure",
        "EquallySpacedPerMeasure",
        "NPSperMeasure",
        "ColumnCues",
    ] {
        if !matches!(table.get::<Value>(name)?, Value::Table(_)) {
            table.set(name, lua.create_table()?)?;
        }
    }
    for name in [
        "PeakNPS",
        "Crossovers",
        "Footswitches",
        "Sideswitches",
        "Jacks",
        "Brackets",
    ] {
        if matches!(table.get::<Value>(name)?, Value::Nil) {
            table.set(name, 0.0)?;
        }
    }
    for name in ["Hash", "Filename", "StepsType", "Difficulty", "Description"] {
        if matches!(table.get::<Value>(name)?, Value::Nil) {
            table.set(name, "")?;
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn parse_chart_info(lua: &Lua, args: &MultiValue) -> mlua::Result<Table> {
    let player = args
        .get(1)
        .and_then(player_index_from_value)
        .map(player_short_name)
        .unwrap_or("P1");
    let globals = lua.globals();
    let sl = match globals.get::<Option<Table>>("SL")? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            globals.set("SL", table.clone())?;
            table
        }
    };
    let player_table = match sl.get::<Option<Table>>(player)? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            sl.set(player, table.clone())?;
            table
        }
    };
    let streams = match player_table.get::<Option<Table>>("Streams")? {
        Some(table) => table,
        None => {
            let table = create_sl_streams(lua)?;
            player_table.set("Streams", table.clone())?;
            table
        }
    };
    init_sl_streams(lua, &streams)?;
    if let Some(Value::Table(steps)) = args.front() {
        let player_number = if player == "P1" { "PlayerNumber_P1" } else { "PlayerNumber_P2" };
        for (field, method) in [("NotesPerMeasure", "GetNotesPerMeasure"),
            ("NPSperMeasure", "GetNpsPerMeasure"), ("PeakNPS", "GetPeakNps")] {
            let value = steps.get::<mlua::Function>(method)?
                .call::<Value>((steps.clone(), player_number))?;
            streams.raw_set(field, value)?;
        }
    }
    note_song_lua_side_effect(lua)?;
    Ok(streams)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_global_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("GameMode", "ITG")?;
    table.set("ActiveColorIndex", SONG_LUA_ACTIVE_COLOR_INDEX)?;
    table.set(
        "ActiveModifiers",
        create_sl_active_mods(lua, context.song_music_rate)?,
    )?;
    table.set("Stages", create_sl_stages(lua)?)?;
    table.set("MenuTimer", create_sl_menu_timer(lua)?)?;
    table.set("ScreenAfter", create_sl_screen_after(lua)?)?;
    table.set("PrevScreenOptionsServiceRow", lua.create_table()?)?;
    table.set("Online", lua.create_table()?)?;
    table.set("SampleMusicLoops", true)?;
    table.set("SampleMusicStartsImmediately", false)?;
    table.set("GameplayReloadCheck", false)?;
    table.set("WheelLocked", false)?;
    table.set("ContinuesRemaining", 0)?;
    table.set("ColumnCueMinTime", 1.5)?;
    table.set("TimeAtSessionStart", 0.0)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_active_mods(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("MusicRate", song_music_rate_value(music_rate))?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_player_table(lua: &Lua, player: &SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("ApiKey", "")?;
    table.set("ArrowCloudApiKey", "")?;
    table.set("ActiveModifiers", create_sl_player_mods(lua, player)?)?;
    table.set("Stages", create_sl_stages(lua)?)?;
    table.set("Streams", create_sl_streams(lua)?)?;
    table.set("HighScores", create_sl_high_scores(lua)?)?;
    table.set("Favorites", lua.create_table()?)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_player_mods(lua: &Lua, player: &SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let (speed_type, speed_value) = song_lua_speedmod_parts(player.speedmod);
    // SL_Init.lua initializes an unmodified profile with Step Statistics.
    table.set("DataVisualizations", if player.step_statistics_enabled { "Step Statistics" } else { "None" })?;
    table.set("ShowFaPlusWindow", false)?;
    table.set("ShowExScore", false)?;
    table.set("ShowHardEXScore", false)?;
    table.set("ShowFaPlusPane", false)?;
    table.set(
        "TimingWindows",
        create_bool_array(lua, &player.timing_windows)?,
    )?;
    table.set("SpeedModType", speed_type)?;
    table.set("SpeedMod", speed_value)?;
    table.set("Mini", "0%")?;
    table.set("Spacing", "0%")?;
    table.set("VisualDelay", "0ms")?;
    let filter = player.background_filter_percent.min(100);
    match filter {
        0 => table.set("BackgroundFilter", "Off")?,
        50 => table.set("BackgroundFilter", "Dark")?,
        75 => table.set("BackgroundFilter", "Darker")?,
        95 => table.set("BackgroundFilter", "Darkest")?,
        // The production profile also supports arbitrary percentages.
        value => table.set("BackgroundFilter", value)?,
    }
    table.set("HideTargets", false)?;
    table.set("HideSongBG", false)?;
    table.set("HideCombo", false)?;
    table.set("HideLifebar", player.hide_lifebar)?;
    table.set("HideScore", false)?;
    table.set("HideDanger", player.hide_danger)?;
    table.set("HideComboExplosions", false)?;
    table.set("ColumnFlashOnMiss", false)?;
    table.set("SubtractiveScoring", false)?;
    table.set("MeasureCounter", "None")?;
    table.set("MeasureCounterLeft", false)?;
    table.set("MeasureCounterUp", true)?;
    table.set("HideLookahead", false)?;
    table.set("MeasureLines", "Off")?;
    table.set("TargetScore", "Personal best")?;
    table.set("TargetScoreNumber", 100)?;
    table.set("ActionOnMissedTarget", "Nothing")?;
    table.set("Pacemaker", false)?;
    table.set("LifeMeterType", player.life_meter_type.as_str())?;
    table.set("NPSGraphAtTop", player.nps_graph_at_top)?;
    table.set("JudgmentTilt", false)?;
    table.set("TiltMultiplier", 1)?;
    table.set("ColumnCues", player.column_cues)?;
    table.set("ColumnCountdown", false)?;
    table.set("ShowHeldMiss", false)?;
    table.set("DisplayScorebox", true)?;
    table.set("ErrorBar", "None")?;
    table.set("ErrorBarUp", false)?;
    table.set("ErrorBarMultiTick", false)?;
    table.set("ErrorBarTrim", "Off")?;
    table.set("ErrorBarCap", 5)?;
    table.set("HideEarlyDecentWayOffJudgments", false)?;
    table.set("HideEarlyDecentWayOffFlash", false)?;
    table.set("HideEarlyDecentWayOffColumnFlash", false)?;
    table.set("FlashMiss", true)?;
    table.set("FlashWayOff", false)?;
    table.set("FlashDecent", false)?;
    table.set("FlashGreat", false)?;
    table.set("FlashExcellent", false)?;
    table.set("FlashFantastic", false)?;
    table.set("ComboColors", "Glow")?;
    table.set("ComboMode", "FullCombo")?;
    table.set("TimerMode", "Time")?;
    table.set("JudgmentAnimation", "Default")?;
    table.set("RailBalance", "No")?;
    table.set("NoteFieldOffsetX", 0)?;
    table.set("NoteFieldOffsetY", 0)?;
    table.set("HeldGraphic", "None")?;
    table.set("NoteSkin", player.noteskin_name.as_str())?;
    table.set("NoteSkinVariant", "default")?;
    table.set("JudgmentGraphic", "None")?;
    table.set("ComboFont", "None")?;
    table.set("PlayerOptionsString", "")?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_stages(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let stats = lua.create_table()?;
    stats.raw_set(1, create_sl_stage_stat(lua)?)?;
    table.set("PlayedThisGame", 0)?;
    table.set("Remaining", 1)?;
    table.set("Stats", stats)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_stage_stat(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let column_judgments = lua.create_table()?;
    for column in 1..=SONG_LUA_NOTE_COLUMNS {
        column_judgments.raw_set(column, create_sl_column_judgments(lua)?)?;
    }
    table.set("MusicRate", 1.0)?;
    table.set("DeathSecond", 0.0)?;
    table.set("worst_window", "W3")?;
    table.set("sequential_offsets", lua.create_table()?)?;
    table.set("column_judgments", column_judgments)?;
    table.set("ex_counts", create_sl_ex_counts(lua)?)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_column_judgments(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let early = lua.create_table()?;
    for name in ["W0", "W1", "W2", "W3", "W4", "W5", "Miss"] {
        table.set(name, 0)?;
        table.set(format!("{name}early"), 0)?;
        table.set(format!("{name}lf"), 0)?;
        table.set(format!("{name}rf"), 0)?;
        early.set(name, 0)?;
    }
    table.set("Early", early)?;
    table.set("MissBecauseHeld", 0)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_ex_counts(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in ["W0_total", "W1", "W2", "W3", "W4", "W5", "Miss"] {
        table.set(name, 0)?;
    }
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_high_scores(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("EnteringName", false)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_menu_timer(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
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
        table.set(name, 0)?;
    }
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_screen_after(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("PlayAgain", "ScreenSelectMusic")?;
    table.set("PlayerOptions", "ScreenGameplay")?;
    table.set("PlayerOptions2", "ScreenGameplay")?;
    table.set("PlayerOptions3", "ScreenGameplay")?;
    table.set("PlayerOptions4", "ScreenGameplay")?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_judgment_colors(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Casual", create_color_array(lua, SL_JUDGMENT_COLORS)?)?;
    table.set("ITG", create_color_array(lua, SL_JUDGMENT_COLORS)?)?;
    table.set("FA+", create_color_array(lua, SL_FA_PLUS_COLORS)?)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_color_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(
            index + 1,
            make_color_table(lua, parse_color_text(value).unwrap_or([1.0, 1.0, 1.0, 1.0]))?,
        )?;
    }
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_groovestats(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("IsConnected", false)?;
    table.set("GetScores", false)?;
    table.set("Leaderboard", false)?;
    table.set("AutoSubmit", false)?;
    table.set("ChartHashVersion", "2")?;
    table.set("RequestCache", lua.create_table()?)?;
    table.set("UnlocksCache", lua.create_table()?)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_arrowcloud(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Enabled", false)?;
    table.set("BaseURL", "https://api.arrowcloud.dance")?;
    table.set("RequestTimeout", 5)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_srpg9(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Colors", create_string_array(lua, SL_DECORATIVE_COLORS)?)?;
    table.set("TextColor", "#ffffff")?;
    table.set(
        "GetLogo",
        lua.create_function(|_, _args: MultiValue| Ok("Logo.png"))?,
    )?;
    table.set(
        "MaybeRandomizeColor",
        lua.create_function(|_, _args: MultiValue| Ok(()))?,
    )?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_mode_table(
    lua: &Lua,
    create: fn(&Lua, &str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for mode in ["Casual", "ITG", "FA+"] {
        table.set(mode, create(lua, mode)?)?;
    }
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_preferences(lua: &Lua, mode: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("TimingWindowAdd", 0.0015)?;
    table.set("RegenComboAfterMiss", if mode == "Casual" { 0 } else { 5 })?;
    table.set(
        "MaxRegenComboAfterMiss",
        if mode == "Casual" { 0 } else { 10 },
    )?;
    table.set(
        "MinTNSToHideNotes",
        if mode == "FA+" {
            "TapNoteScore_W4"
        } else {
            "TapNoteScore_W3"
        },
    )?;
    if mode != "Casual" {
        let prefs = lua.globals().get::<Option<Table>>("ThemePrefs")?;
        let rescore = match prefs {
            Some(prefs) => {
                let get = prefs.get::<mlua::Function>("Get")?;
                !matches!(
                    get.call::<Value>("RescoreEarlyHits")?,
                    Value::Nil | Value::Boolean(false)
                )
            }
            None => false,
        };
        table.set(
            "MinTNSToScoreNotes",
            if !rescore {
                "TapNoteScore_None"
            } else if mode == "FA+" {
                "TapNoteScore_W4"
            } else {
                "TapNoteScore_W3"
            },
        )?;
    }
    table.set("HarshHotLifePenalty", true)?;
    table.set("PercentageScoring", true)?;
    table.set("AllowW1", "AllowW1_Everywhere")?;
    table.set("SubSortByNumSteps", true)?;
    let w1 = if mode == "FA+" { 0.0135 } else { 0.0215 };
    let w2 = if mode == "FA+" { 0.0215 } else { 0.043 };
    let w3 = if mode == "FA+" { 0.043 } else { 0.102 };
    table.set("TimingWindowSecondsW1", w1)?;
    table.set("TimingWindowSecondsW2", w2)?;
    table.set("TimingWindowSecondsW3", w3)?;
    table.set(
        "TimingWindowSecondsW4",
        if mode == "ITG" { 0.135 } else { 0.102 },
    )?;
    table.set(
        "TimingWindowSecondsW5",
        match mode {
            "Casual" => 0.102,
            "FA+" => 0.135,
            _ => 0.18,
        },
    )?;
    table.set("TimingWindowSecondsHold", 0.32)?;
    table.set("TimingWindowSecondsMine", 0.07)?;
    table.set("TimingWindowSecondsRoll", 0.35)?;
    Ok(table)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn create_sl_metrics(lua: &Lua, mode: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    // Simply Love Scripts/SL_Init.lua defines separate scoring and life rules
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
            table.set(format!("{prefix}{name}"), value)?;
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
        table.set(format!("LifePercentChange{name}"), value)?;
    }
    table.set("InitialValue", 0.5)?;
    Ok(table)
}

pub(super) fn gameplay_lead_in_seconds(group: &str, name: &str) -> Option<f64> {
    if !group.eq_ignore_ascii_case("ScreenGameplay") { return None; }
    [("MinSecondsToStep", 6.0), ("MinSecondsToMusic", 2.0), ("MinSecondsToStepNextSong", 4.0)]
        .into_iter().find_map(|(metric, seconds)| name.eq_ignore_ascii_case(metric).then_some(seconds))
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(super) fn read_sl_metric(lua: &Lua, group: &str, name: &str) -> mlua::Result<Option<Value>> {
    if let Some(seconds) = gameplay_lead_in_seconds(group, name) {
        // ThemeManager::PushMetric runs the metrics.ini Lua expression at
        // every query. Use SL's current Lua number without rate clamping or
        // float narrowing; it can differ from GAMESTATE's song options.
        let rate = lua.globals().get::<Table>("SL")?.get::<Table>("Global")?
            .get::<Table>("ActiveModifiers")?.get::<f64>("MusicRate")?;
        return Ok(Some(Value::Number(seconds * rate)));
    }
    // Simply Love metrics.ini evaluates these expressions against the active
    // mode. Keep unrelated sections from seeing fields of SL.Metrics.
    let mapped = group.eq_ignore_ascii_case("ScoreKeeperNormal")
        && (name.starts_with("PercentScoreWeight") || name.starts_with("GradeWeight"))
        || group.eq_ignore_ascii_case("LifeMeterBar") && name.starts_with("LifePercentChange");
    if !mapped {
        return Ok(None);
    }
    let Some(sl) = lua.globals().get::<Option<Table>>("SL")? else {
        return Ok(None);
    };
    let mode = sl.get::<Table>("Global")?.get::<mlua::LuaString>("GameMode")?;
    let metrics = sl.get::<Table>("Metrics")?.get::<Table>(mode)?;
    let value = metrics.get::<Value>(name)?;
    Ok((!matches!(value, Value::Nil)).then_some(value))
}


