use mlua::{Lua, MultiValue, Table, Value};

use super::theme_colors::{
    DDR_DIFF_COLORS, ITG_DIFF_COLORS, SL_COLORS, SL_DECORATIVE_COLORS, SL_FA_PLUS_COLORS,
    SL_JUDGMENT_COLORS, SONG_LUA_ACTIVE_COLOR_INDEX, create_color_array,
};
use super::types::{SongLuaCompileContext, SongLuaPlayerContext, SongLuaSpeedMod};
use super::util::{create_bool_array, create_string_array};
use super::{LUA_PLAYERS, SONG_LUA_NOTE_COLUMNS};

pub(super) fn create_sl_table(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<Table> {
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

pub(super) fn create_sl_streams(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    init_sl_streams(lua, &table)?;
    Ok(table)
}

pub(super) fn init_sl_streams(lua: &Lua, table: &Table) -> mlua::Result<()> {
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

pub(super) fn player_short_name(player: usize) -> &'static str {
    match player {
        0 => "P1",
        1 => "P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

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
    table.set("ColumnCueMinTime", 0.0)?;
    table.set("TimeAtSessionStart", 0.0)?;
    Ok(table)
}

fn create_sl_active_mods(lua: &Lua, music_rate: f32) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("MusicRate", song_music_rate_value(music_rate))?;
    Ok(table)
}

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

fn create_sl_player_mods(lua: &Lua, player: &SongLuaPlayerContext) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let (speed_type, speed_value) = speedmod_parts(player.speedmod);
    table.set("DataVisualizations", "None")?;
    table.set("ShowFaPlusWindow", false)?;
    table.set("ShowExScore", false)?;
    table.set("ShowHardEXScore", false)?;
    table.set("ShowFaPlusPane", false)?;
    table.set(
        "TimingWindows",
        create_bool_array(lua, &[true, true, true, false, false])?,
    )?;
    table.set("SpeedModType", speed_type)?;
    table.set("SpeedMod", speed_value)?;
    table.set("Mini", "0%")?;
    table.set("Spacing", "0%")?;
    table.set("VisualDelay", "0ms")?;
    table.set("BackgroundFilter", 0)?;
    table.set("HideTargets", false)?;
    table.set("HideSongBG", false)?;
    table.set("HideCombo", false)?;
    table.set("HideLifebar", false)?;
    table.set("HideScore", false)?;
    table.set("HideDanger", false)?;
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
    table.set("LifeMeterType", "Standard")?;
    table.set("NPSGraphAtTop", false)?;
    table.set("JudgmentTilt", false)?;
    table.set("TiltMultiplier", 1)?;
    table.set("ColumnCues", false)?;
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

fn create_sl_stages(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let stats = lua.create_table()?;
    stats.raw_set(1, create_sl_stage_stat(lua)?)?;
    table.set("PlayedThisGame", 0)?;
    table.set("Remaining", 1)?;
    table.set("Stats", stats)?;
    Ok(table)
}

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

fn create_sl_ex_counts(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for name in ["W0_total", "W1", "W2", "W3", "W4", "W5", "Miss"] {
        table.set(name, 0)?;
    }
    Ok(table)
}

fn create_sl_high_scores(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("EnteringName", false)?;
    Ok(table)
}

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

fn create_sl_screen_after(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("PlayAgain", "ScreenSelectMusic")?;
    table.set("PlayerOptions", "ScreenGameplay")?;
    table.set("PlayerOptions2", "ScreenGameplay")?;
    table.set("PlayerOptions3", "ScreenGameplay")?;
    table.set("PlayerOptions4", "ScreenGameplay")?;
    Ok(table)
}

fn create_sl_judgment_colors(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Casual", create_color_array(lua, SL_JUDGMENT_COLORS)?)?;
    table.set("ITG", create_color_array(lua, SL_JUDGMENT_COLORS)?)?;
    table.set("FA+", create_color_array(lua, SL_FA_PLUS_COLORS)?)?;
    Ok(table)
}

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

fn create_sl_arrowcloud(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("Enabled", false)?;
    table.set("BaseURL", "https://api.arrowcloud.dance")?;
    table.set("RequestTimeout", 5)?;
    Ok(table)
}

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
    table.set("MinTNSToScoreNotes", "TapNoteScore_None")?;
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
        if mode == "Casual" { 0.102 } else { 0.135 },
    )?;
    table.set(
        "TimingWindowSecondsW5",
        if mode == "Casual" { 0.102 } else { 0.18 },
    )?;
    table.set("TimingWindowSecondsHold", 0.32)?;
    table.set("TimingWindowSecondsMine", 0.07)?;
    table.set("TimingWindowSecondsRoll", 0.35)?;
    Ok(table)
}

fn create_sl_metrics(lua: &Lua, mode: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let fa_plus = mode == "FA+";
    for (name, value) in [
        ("PercentScoreWeightW1", 3),
        ("PercentScoreWeightW2", if fa_plus { 3 } else { 2 }),
        ("PercentScoreWeightW3", 1),
        ("PercentScoreWeightW4", 0),
        ("PercentScoreWeightW5", 0),
        ("PercentScoreWeightMiss", 0),
        ("PercentScoreWeightLetGo", 0),
        ("PercentScoreWeightHeld", 3),
        ("PercentScoreWeightHitMine", -1),
        ("PercentScoreWeightCheckpointHit", 0),
        ("GradeWeightW1", 3),
        ("GradeWeightW2", if fa_plus { 3 } else { 2 }),
        ("GradeWeightW3", 1),
        ("GradeWeightW4", 0),
        ("GradeWeightW5", 0),
        ("GradeWeightMiss", 0),
        ("GradeWeightLetGo", 0),
        ("GradeWeightHeld", 3),
        ("GradeWeightHitMine", -1),
        ("GradeWeightCheckpointHit", 0),
    ] {
        table.set(name, value)?;
    }
    for (name, value) in [
        ("LifePercentChangeW1", 0.008),
        ("LifePercentChangeW2", 0.008),
        ("LifePercentChangeW3", 0.004),
        ("LifePercentChangeW4", 0.0),
        ("LifePercentChangeW5", -0.04),
        ("LifePercentChangeMiss", -0.08),
        ("LifePercentChangeHitMine", -0.05),
        ("LifePercentChangeHeld", 0.008),
        ("LifePercentChangeLetGo", -0.08),
    ] {
        table.set(name, value)?;
    }
    Ok(table)
}

fn speedmod_parts(speedmod: SongLuaSpeedMod) -> (&'static str, f32) {
    match speedmod {
        SongLuaSpeedMod::X(value) => ("X", value),
        SongLuaSpeedMod::C(value) => ("C", value),
        SongLuaSpeedMod::M(value) => ("M", value),
        SongLuaSpeedMod::A(value) => ("A", value),
    }
}

fn song_music_rate_value(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}
