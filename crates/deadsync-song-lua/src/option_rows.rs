use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::lua_util::{create_string_array, method_arg};
use crate::runtime::note_song_lua_side_effect;
use crate::values::{player_index_from_value, player_number_name, read_string, truthy};
use crate::{
    SONG_LUA_THEME_NAME, custom_multi_modifier_key, player_short_name, theme_pref_default,
};

#[derive(Clone, Copy)]
pub enum SongLuaOptionValues {
    Str(&'static [&'static str]),
    Bool(&'static [bool]),
    Int(&'static [i64]),
    Number(&'static [f64]),
}

impl SongLuaOptionValues {
    pub fn len(self) -> usize {
        match self {
            Self::Str(values) => values.len(),
            Self::Bool(values) => values.len(),
            Self::Int(values) => values.len(),
            Self::Number(values) => values.len(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct SongLuaOptionRowSpec {
    pub choices: SongLuaOptionValues,
    pub values: Option<SongLuaOptionValues>,
    pub layout_type: &'static str,
    pub select_type: &'static str,
    pub one_choice_for_all_players: bool,
    pub export_on_change: bool,
    pub hide_on_disable: bool,
    pub reload_row_messages: &'static [&'static str],
    pub broadcast_on_export: &'static [&'static str],
}

impl SongLuaOptionRowSpec {
    pub fn new(choices: SongLuaOptionValues) -> Self {
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

    pub fn values(mut self, values: SongLuaOptionValues) -> Self {
        self.values = Some(values);
        self
    }

    pub fn layout(mut self, layout_type: &'static str) -> Self {
        self.layout_type = layout_type;
        self
    }

    pub fn select(mut self, select_type: &'static str) -> Self {
        self.select_type = select_type;
        self
    }

    pub fn one_choice(mut self) -> Self {
        self.one_choice_for_all_players = true;
        self
    }

    pub fn export(mut self) -> Self {
        self.export_on_change = true;
        self
    }

    pub fn hide_on_disable(mut self) -> Self {
        self.hide_on_disable = true;
        self
    }

    pub fn reload(mut self, messages: &'static [&'static str]) -> Self {
        self.reload_row_messages = messages;
        self
    }
}

pub struct SongLuaNamedOptionRowSpec {
    pub row_name: String,
    pub spec: SongLuaOptionRowSpec,
}

pub struct SongLuaOperatorOptionRowSpec {
    pub row_name: String,
    pub spec: SongLuaOptionRowSpec,
    pub pref_name: Option<String>,
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
    "HideEarlyDecentWayOffColumnFlash",
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
pub const THEME_PREF_ROW_NAMES: &[&str] = &[
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

pub fn conf_option_row_spec(name: &str) -> SongLuaNamedOptionRowSpec {
    let (row_name, spec) = match name.to_ascii_lowercase().as_str() {
        "confaspectratio" => (
            "DisplayAspectRatio",
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_DISPLAY_ASPECT_RATIO))
                .values(SongLuaOptionValues::Number(
                    OPTION_DISPLAY_ASPECT_RATIO_VALUES,
                ))
                .one_choice(),
        ),
        "confdisplayresolution" => (
            "DisplayResolution",
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_DISPLAY_RESOLUTION))
                .one_choice(),
        ),
        "confdisplaymode" => (
            "DisplayMode",
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_DISPLAY_MODE)).one_choice(),
        ),
        "confrefreshrate" => (
            "RefreshRate",
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_REFRESH_RATE))
                .values(SongLuaOptionValues::Int(OPTION_REFRESH_RATE_VALUES))
                .one_choice(),
        ),
        "conffullscreentype" => (
            "FullscreenType",
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_FULLSCREEN_TYPE))
                .one_choice(),
        ),
        _ => (
            name,
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFF_ON)).one_choice(),
        ),
    };
    SongLuaNamedOptionRowSpec {
        row_name: row_name.to_string(),
        spec,
    }
}

pub fn custom_option_row_spec(name: &str) -> Option<SongLuaOptionRowSpec> {
    let lower = name.to_ascii_lowercase();
    let spec = match lower.as_str() {
        "speedmodtype" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_SPEED_MOD_TYPE))
                .layout("ShowOneInRow")
                .export()
        }
        "speedmod" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_SPEED_MOD))
            .layout("ShowOneInRow")
            .export(),
        "mini" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MINI)).layout("ShowOneInRow")
        }
        "spacing" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_SPACING))
            .layout("ShowOneInRow"),
        "noteskin" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_NOTESKIN))
            .layout("ShowOneInRow")
            .export(),
        "judgmentgraphic" | "holdjudgment" | "heldgraphic" | "heldmissgraphic" | "combofont" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_NONE))
                .layout("ShowOneInRow")
                .export()
        }
        "noteskinvariant" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_NONE))
            .layout("ShowOneInRow")
            .export()
            .hide_on_disable()
            .reload(OPTION_REFRESH_ACTOR_PROXY_MESSAGES),
        "backgroundfilter" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_BACKGROUND_FILTER))
                .values(SongLuaOptionValues::Str(OPTION_BACKGROUND_FILTER))
        }
        "notefieldoffsetx" | "notefieldoffsety" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_NOTE_FIELD_OFFSET))
                .layout("ShowOneInRow")
                .export()
        }
        "visualdelay" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_VISUAL_DELAY))
            .layout("ShowOneInRow")
            .export(),
        "musicrate" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MUSIC_RATE))
            .layout("ShowOneInRow")
            .one_choice()
            .export(),
        "stepchart" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_STEPCHART)).export()
        }
        "screenafterplayeroptions"
        | "screenafterplayeroptions2"
        | "screenafterplayeroptions3"
        | "screenafterplayeroptions4" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_SCREEN_AFTER_PLAYER_OPTIONS))
        }
        "hide" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_HIDE))
            .select("SelectMultiple"),
        "gameplayextras" | "gameplayextrasb" | "gameplayextrasc" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_GAMEPLAY_EXTRAS))
                .select("SelectMultiple")
        }
        "resultsextras" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_RESULTS_EXTRAS))
                .select("SelectMultiple")
        }
        "lifemetertype" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_LIFE_METER_TYPE))
        }
        "datavisualizations" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_DATA_VISUALIZATIONS))
        }
        "targetscore" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_TARGET_SCORE)),
        "targetscorenumber" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_TARGET_SCORE_NUMBER))
        }
        "actiononmissedtarget" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_ACTION_ON_MISSED_TARGET))
        }
        "tiltmultiplier" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_TILT_MULTIPLIER))
        }
        "errorbar" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_ERROR_BAR)),
        "errorbartrim" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_ERROR_BAR_TRIM))
        }
        "errorbaroptions" | "errorbarcap" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_ERROR_BAR_OPTIONS))
                .select("SelectMultiple")
        }
        "measurecounter" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MEASURE_COUNTER))
        }
        "measurecounteroptions" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MEASURE_COUNTER_OPTIONS))
                .select("SelectMultiple")
        }
        "measurecounterlookahead" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MEASURE_COUNTER_LOOKAHEAD))
        }
        "measurelines" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MEASURE_LINES)),
        "timingwindowoptions" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_TIMING_WINDOW_OPTIONS))
                .select("SelectMultiple")
        }
        "timingwindows" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_TIMING_WINDOWS))
        }
        "faplus" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_FA_PLUS))
            .select("SelectMultiple"),
        "minindicator" | "miniindicator" | "miniindicatorcolor" | "stepstatsinfo"
        | "judgmentflash" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFF_ON)),
        "scoreboxoptions" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_SCORE_BOX_OPTIONS))
                .select("SelectMultiple")
        }
        "stepstatsextra" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_STEP_STATS_EXTRA))
                .select("SelectMultiple")
        }
        "funoptions" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_FUN_OPTIONS))
            .select("SelectMultiple"),
        "lifebaroptions" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_LIFE_BAR_OPTIONS))
        }
        "combocolors" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_COMBO_COLORS)),
        "combomode" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_COMBO_MODE)),
        "timermode" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_TIMER_MODE)),
        "judgmentanimation" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_JUDGMENT_ANIMATION))
        }
        "railbalance" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_RAIL_BALANCE)),
        "extraaesthetics" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_EXTRA_AESTHETICS))
                .select("SelectMultiple")
        }
        _ => return None,
    };
    Some(spec)
}

pub fn custom_option_default_text(name: &str) -> Option<String> {
    custom_option_row_spec(name).map(|spec| option_value_text(spec.choices, 0))
}

pub fn option_value_text(values: SongLuaOptionValues, index: usize) -> String {
    match values {
        SongLuaOptionValues::Str(values) => {
            values.get(index).copied().unwrap_or_default().to_string()
        }
        SongLuaOptionValues::Bool(values) => {
            values.get(index).copied().unwrap_or(false).to_string()
        }
        SongLuaOptionValues::Int(values) => {
            values.get(index).copied().unwrap_or_default().to_string()
        }
        SongLuaOptionValues::Number(values) => {
            values.get(index).copied().unwrap_or_default().to_string()
        }
    }
}

pub fn theme_pref_row_spec(name: &str) -> SongLuaOptionRowSpec {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "numberofcontinuesallowed" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Int(OPTION_ZERO_TO_NINE))
                .values(SongLuaOptionValues::Int(OPTION_ZERO_TO_NINE))
                .one_choice()
        }
        "casualmaxmeter" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Int(OPTION_CASUAL_METERS))
                .values(SongLuaOptionValues::Int(OPTION_CASUAL_METERS))
                .one_choice()
        }
        "simplylovecolor" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Int(OPTION_ONE_TO_TWELVE))
                .values(SongLuaOptionValues::Int(OPTION_ONE_TO_TWELVE))
                .one_choice()
        }
        "nice" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_NICE_CHOICES))
            .values(SongLuaOptionValues::Int(OPTION_NICE_VALUES))
            .one_choice(),
        "screengroovestatsloginmenutimer"
        | "screenselectmusicmenutimer"
        | "screenselectmusiccasualmenutimer"
        | "screenplayeroptionsmenutimer"
        | "screenevaluationmenutimer"
        | "screenevaluationnonstopmenutimer"
        | "screenevaluationsummarymenutimer"
        | "screennameentrymenutimer" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MENU_TIMER_CHOICES))
                .values(SongLuaOptionValues::Int(OPTION_MENU_TIMER_VALUES))
                .one_choice()
        }
        "visualstyle" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_VISUAL_STYLE))
            .values(SongLuaOptionValues::Str(OPTION_VISUAL_STYLE))
            .one_choice(),
        "defaultgamemode" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_GAME_MODE))
            .values(SongLuaOptionValues::Str(OPTION_GAME_MODE))
            .one_choice(),
        "autostyle" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_AUTO_STYLE))
            .values(SongLuaOptionValues::Str(OPTION_AUTO_STYLE))
            .one_choice(),
        "musicwheelstyle" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MUSIC_WHEEL_STYLE))
                .values(SongLuaOptionValues::Str(OPTION_MUSIC_WHEEL_STYLE))
                .one_choice()
        }
        "themefont" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_THEME_FONT))
            .values(SongLuaOptionValues::Str(OPTION_THEME_FONT))
            .one_choice(),
        "songselectbg" | "resultsbg" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_BG_STYLE))
                .values(SongLuaOptionValues::Str(OPTION_BG_STYLE))
                .one_choice()
        }
        "qrlogin" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_QR_LOGIN))
            .values(SongLuaOptionValues::Str(OPTION_QR_LOGIN))
            .one_choice(),
        "scoringsystem" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_SCORING_SYSTEM))
                .values(SongLuaOptionValues::Str(OPTION_SCORING_SYSTEM))
                .one_choice()
        }
        "stepstats" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_STEP_STATS))
            .values(SongLuaOptionValues::Str(OPTION_STEP_STATS))
            .one_choice(),
        "editmodelastseensong"
        | "editmodelastseendifficulty"
        | "editmodelastseenstepstype"
        | "editmodelastseenstyletype"
        | "lastactiveevent" => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_EMPTY))
            .values(SongLuaOptionValues::Str(OPTION_EMPTY))
            .one_choice(),
        "rainbowmode" | "animatebanners" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_ON_OFF))
                .values(SongLuaOptionValues::Bool(OPTION_TRUE_FALSE))
                .one_choice()
        }
        "hidestocknoteskins" | "memorycards" => {
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFF_ON))
                .values(SongLuaOptionValues::Bool(OPTION_FALSE_TRUE))
                .one_choice()
        }
        _ => SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_YES_NO))
            .values(SongLuaOptionValues::Bool(OPTION_TRUE_FALSE))
            .one_choice(),
    }
}

pub fn operator_menu_option_row_spec(
    method_name: &str,
    kind_arg: Option<&str>,
) -> SongLuaOperatorOptionRowSpec {
    let lower = method_name.to_ascii_lowercase();
    let (row_name, spec, pref_name) = match lower.as_str() {
        "theme" => (
            "Theme".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_THEME_NAMES)).one_choice(),
            Some("Theme".to_string()),
        ),
        "editornoteskin" => (
            "EditorNoteSkin".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_NOTESKIN))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("EditorNoteSkinP1".to_string()),
        ),
        "defaultfailtype" => (
            "DefaultFailType".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_FAIL_TYPES)).one_choice(),
            Some("DefaultModifiers".to_string()),
        ),
        "longandmarathontime" => {
            let kind = kind_arg.unwrap_or("Long");
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
                SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(choices))
                    .values(SongLuaOptionValues::Int(values))
                    .layout("ShowOneInRow")
                    .one_choice(),
                Some(pref_name.to_string()),
            )
        }
        "musicwheelspeed" => (
            "MusicWheelSpeed".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_MUSIC_WHEEL_SPEED))
                .values(SongLuaOptionValues::Int(OPTION_MUSIC_WHEEL_SPEED_VALUES))
                .one_choice(),
            Some("MusicWheelSwitchSpeed".to_string()),
        ),
        "videorenderer" => (
            "VideoRenderer".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_VIDEO_RENDERER)).one_choice(),
            Some("VideoRenderers".to_string()),
        ),
        "globaloffsetseconds" => (
            "GlobalOffsetSeconds".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFFSET_MS))
                .values(SongLuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("GlobalOffsetSeconds".to_string()),
        ),
        "visualdelayseconds" => (
            "VisualDelaySeconds".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFFSET_MS))
                .values(SongLuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES))
                .layout("ShowOneInRow")
                .one_choice(),
            Some("VisualDelaySeconds".to_string()),
        ),
        "memorycards" => (
            "MemoryCards".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFF_ON))
                .values(SongLuaOptionValues::Bool(OPTION_FALSE_TRUE))
                .one_choice(),
            Some("MemoryCards".to_string()),
        ),
        "customsongsmaxseconds" => (
            "CustomSongsMaxSeconds".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_CUSTOM_SONG_SECONDS))
                .values(SongLuaOptionValues::Int(OPTION_CUSTOM_SONG_SECONDS_VALUES))
                .one_choice(),
            Some("CustomSongsMaxSeconds".to_string()),
        ),
        "customsongsmaxmegabytes" => (
            "CustomSongsMaxMegabytes".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_CUSTOM_SONG_MEGABYTES))
                .values(SongLuaOptionValues::Int(
                    OPTION_CUSTOM_SONG_MEGABYTES_VALUES,
                ))
                .one_choice(),
            Some("CustomSongsMaxMegabytes".to_string()),
        ),
        "customsongsloadtimeout" => (
            "CustomSongsLoadTimeout".to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_CUSTOM_SONG_TIMEOUT))
                .values(SongLuaOptionValues::Int(OPTION_CUSTOM_SONG_TIMEOUT_VALUES))
                .one_choice(),
            Some("CustomSongsLoadTimeout".to_string()),
        ),
        _ => (
            method_name.to_string(),
            SongLuaOptionRowSpec::new(SongLuaOptionValues::Str(OPTION_OFF_ON)).one_choice(),
            None,
        ),
    };
    SongLuaOperatorOptionRowSpec {
        row_name,
        spec,
        pref_name,
    }
}

pub fn create_theme_prefs_table(lua: &Lua) -> mlua::Result<Table> {
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

fn create_lua_option_array(lua: &Lua, values: SongLuaOptionValues) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match values {
        SongLuaOptionValues::Str(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        SongLuaOptionValues::Bool(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        SongLuaOptionValues::Int(values) => {
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, *value)?;
            }
        }
        SongLuaOptionValues::Number(values) => {
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

pub fn create_custom_option_row(lua: &Lua, name: &str) -> mlua::Result<Option<Table>> {
    let Some(spec) = custom_option_row_spec(name) else {
        return Ok(None);
    };
    let row = create_compat_option_row_table(lua, name, spec)?;
    set_custom_option_save(lua, &row, name)?;
    Ok(Some(row))
}

pub fn create_conf_option_row(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let row_spec = conf_option_row_spec(name);
    let row = create_compat_option_row_table(lua, &row_spec.row_name, row_spec.spec)?;
    set_pref_option_save(lua, &row, &row_spec.row_name)?;
    Ok(row)
}

pub fn create_theme_prefs_rows_table(lua: &Lua) -> mlua::Result<Table> {
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

pub fn create_sl_custom_prefs_table(lua: &Lua) -> mlua::Result<Table> {
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

pub fn create_operator_menu_option_rows_table(lua: &Lua) -> mlua::Result<Table> {
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
    let kind = method_arg(args, 0).cloned().and_then(read_string);
    let row_spec = operator_menu_option_row_spec(method_name, kind.as_deref());
    let row = create_compat_option_row_table(lua, &row_spec.row_name, row_spec.spec)?;
    if let Some(pref_name) = row_spec.pref_name {
        set_pref_option_save(lua, &row, &pref_name)?;
    }
    Ok(row)
}
