use crate::SONG_LUA_THEME_NAME;

#[derive(Clone, Copy)]
pub enum SongLuaOptionValues {
    Str(&'static [&'static str]),
    Bool(&'static [bool]),
    Int(&'static [i64]),
    Number(&'static [f64]),
}

impl SongLuaOptionValues {
    #[must_use]
    pub const fn len(self) -> usize {
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
    #[must_use]
    pub const fn new(choices: SongLuaOptionValues) -> Self {
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

    #[must_use]
    pub const fn values(mut self, values: SongLuaOptionValues) -> Self {
        self.values = Some(values);
        self
    }

    #[must_use]
    pub const fn layout(mut self, layout_type: &'static str) -> Self {
        self.layout_type = layout_type;
        self
    }

    #[must_use]
    pub const fn select(mut self, select_type: &'static str) -> Self {
        self.select_type = select_type;
        self
    }

    #[must_use]
    pub const fn one_choice(mut self) -> Self {
        self.one_choice_for_all_players = true;
        self
    }

    #[must_use]
    pub const fn export(mut self) -> Self {
        self.export_on_change = true;
        self
    }

    #[must_use]
    pub const fn hide_on_disable(mut self) -> Self {
        self.hide_on_disable = true;
        self
    }

    #[must_use]
    pub const fn reload(mut self, messages: &'static [&'static str]) -> Self {
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

const OPTION_ROW_KEY_STACK_BYTES: usize = 64;

#[inline]
fn with_ascii_lowercase_option_key<T>(name: &str, use_key: impl FnOnce(&str) -> T) -> T {
    if name.len() <= OPTION_ROW_KEY_STACK_BYTES {
        let mut key = [0u8; OPTION_ROW_KEY_STACK_BYTES];
        key[..name.len()].copy_from_slice(name.as_bytes());
        key[..name.len()].make_ascii_lowercase();
        let key = std::str::from_utf8(&key[..name.len()])
            .expect("ASCII case folding preserves valid UTF-8");
        use_key(key)
    } else {
        let key = name.to_ascii_lowercase();
        use_key(&key)
    }
}

#[must_use]
pub fn conf_option_row_spec(name: &str) -> SongLuaNamedOptionRowSpec {
    with_ascii_lowercase_option_key(name, |key| conf_option_row_spec_normalized(name, key))
}

fn conf_option_row_spec_normalized(name: &str, key: &str) -> SongLuaNamedOptionRowSpec {
    let (row_name, spec) = match key {
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

#[must_use]
pub fn custom_option_row_spec(name: &str) -> Option<SongLuaOptionRowSpec> {
    with_ascii_lowercase_option_key(name, custom_option_row_spec_normalized)
}

fn custom_option_row_spec_normalized(key: &str) -> Option<SongLuaOptionRowSpec> {
    let spec = match key {
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

#[must_use]
pub fn custom_option_default_text(name: &str) -> Option<String> {
    custom_option_row_spec(name).map(|spec| option_value_text(spec.choices, 0))
}

#[must_use]
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

#[must_use]
pub fn theme_pref_row_spec(name: &str) -> SongLuaOptionRowSpec {
    with_ascii_lowercase_option_key(name, theme_pref_row_spec_normalized)
}

fn theme_pref_row_spec_normalized(key: &str) -> SongLuaOptionRowSpec {
    match key {
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

#[must_use]
pub fn operator_menu_option_row_spec(
    method_name: &str,
    kind_arg: Option<&str>,
) -> SongLuaOperatorOptionRowSpec {
    with_ascii_lowercase_option_key(method_name, |key| {
        operator_menu_option_row_spec_normalized(method_name, kind_arg, key)
    })
}

fn operator_menu_option_row_spec_normalized(
    method_name: &str,
    kind_arg: Option<&str>,
    key: &str,
) -> SongLuaOperatorOptionRowSpec {
    let (row_name, spec, pref_name) = match key {
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
