use mlua::Value;

use crate::{custom_option_default_text, read_i32_value};

pub const SONG_LUA_TOP_SCREEN_OPTION_ROWS: &[&str] = &[
    "SpeedModType",
    "SpeedMod",
    "Mini",
    "Perspective",
    "NoteSkin",
    "NoteSkinVariant",
    "JudgmentGraphic",
    "ComboFont",
    "HoldJudgment",
    "HeldGraphic",
    "BackgroundFilter",
    "NoteFieldOffsetX",
    "NoteFieldOffsetY",
    "VisualDelay",
    "MusicRate",
    "Stepchart",
    "ScreenAfterPlayerOptions",
    "Turn",
    "Scroll",
    "Hide",
    "LifeMeterType",
    "DataVisualizations",
    "TargetScore",
    "ActionOnMissedTarget",
    "GameplayExtras",
    "GameplayExtrasB",
    "GameplayExtrasC",
    "TiltMultiplier",
    "ErrorBar",
    "ErrorBarTrim",
    "ErrorBarOptions",
    "MeasureCounter",
    "MeasureCounterOptions",
    "MeasureLines",
    "TimingWindowOptions",
    "TimingWindows",
    "FaPlus",
    "ScoreBoxOptions",
    "StepStatsExtra",
    "FunOptions",
    "LifeBarOptions",
    "ComboColors",
    "ComboMode",
    "TimerMode",
    "JudgmentAnimation",
    "RailBalance",
    "ExtraAesthetics",
    "ScreenAfterPlayerOptions2",
    "Insert",
    "Remove",
    "Holds",
    "Attacks",
    "Characters",
    "HideLightType",
    "ScreenAfterPlayerOptions3",
    "Assist",
    "ShowBGChangesPlay",
    "ScreenAfterPlayerOptions4",
];

pub fn top_screen_option_row_name(value: Option<Value>) -> String {
    match value {
        Some(Value::String(name)) => name
            .to_str()
            .map(|name| name.to_string())
            .unwrap_or_default(),
        Some(value) => read_i32_value(value)
            .and_then(|index| top_screen_option_row_name_at(index).map(str::to_string))
            .unwrap_or_else(|| SONG_LUA_TOP_SCREEN_OPTION_ROWS[0].to_string()),
        None => SONG_LUA_TOP_SCREEN_OPTION_ROWS[0].to_string(),
    }
}

pub fn top_screen_option_row_name_at(index: i32) -> Option<&'static str> {
    let index = usize::try_from(index).ok()?;
    SONG_LUA_TOP_SCREEN_OPTION_ROWS
        .get(index)
        .or_else(|| {
            index
                .checked_sub(1)
                .and_then(|index| SONG_LUA_TOP_SCREEN_OPTION_ROWS.get(index))
        })
        .copied()
}

pub fn option_row_default_text(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "speedmodtype" => "X".to_string(),
        "speedmod" => "1".to_string(),
        "mini" => "0%".to_string(),
        "perspective" => "Overhead".to_string(),
        "noteskin" | "noteskinvariant" => "default".to_string(),
        "musicrate" => "1".to_string(),
        _ => custom_option_default_text(name).unwrap_or_default(),
    }
}

pub fn player_child_proxy_name(name: &str) -> Option<&'static str> {
    if name.eq_ignore_ascii_case("Judgment") {
        Some("Judgment")
    } else if name.eq_ignore_ascii_case("Combo") {
        Some("Combo")
    } else {
        None
    }
}

pub fn top_screen_player_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "PlayerP1",
        1 => "PlayerP2",
        _ => "",
    }
}

pub fn top_screen_player_index(name: &str) -> Option<usize> {
    match name {
        "PlayerP1" => Some(0),
        "PlayerP2" => Some(1),
        _ => None,
    }
}

pub fn top_screen_life_meter_index(name: &str) -> Option<usize> {
    match name {
        "LifeP1" => Some(0),
        "LifeP2" => Some(1),
        _ => None,
    }
}

pub fn top_screen_life_meter_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "LifeP1",
        1 => "LifeP2",
        _ => "",
    }
}

pub fn top_screen_score_index(name: &str) -> Option<usize> {
    match name {
        "ScoreP1" => Some(0),
        "ScoreP2" => Some(1),
        _ => None,
    }
}

pub fn top_screen_score_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "ScoreP1",
        1 => "ScoreP2",
        _ => "",
    }
}

pub fn top_screen_score_percent_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "PercentP1",
        1 => "PercentP2",
        _ => "",
    }
}

pub fn top_screen_steps_display_index(name: &str) -> Option<usize> {
    match name {
        "StepsDisplayP1" => Some(0),
        "StepsDisplayP2" => Some(1),
        _ => None,
    }
}

pub fn top_screen_song_meter_display_index(name: &str) -> Option<usize> {
    match name {
        "SongMeterDisplayP1" => Some(0),
        "SongMeterDisplayP2" => Some(1),
        _ => None,
    }
}

pub fn top_screen_life_meter_bar_index(name: &str) -> Option<usize> {
    match name {
        "LifeMeterBarP1" => Some(0),
        "LifeMeterBarP2" => Some(1),
        _ => None,
    }
}

pub fn underlay_score_index(name: &str) -> Option<usize> {
    match name {
        "P1Score" => Some(0),
        "P2Score" => Some(1),
        _ => None,
    }
}

pub fn underlay_score_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "P1Score",
        1 => "P2Score",
        _ => "",
    }
}

pub fn top_screen_step_stats_pane_index(name: &str) -> Option<usize> {
    match name {
        "StepStatsPaneP1" => Some(0),
        "StepStatsPaneP2" => Some(1),
        _ => None,
    }
}

pub fn top_screen_danger_index(name: &str) -> Option<usize> {
    match name {
        "DangerP1" => Some(0),
        "DangerP2" => Some(1),
        _ => None,
    }
}
