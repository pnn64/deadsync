use crate::core::gfx::BackendType;
use crate::core::input::{
    GamepadCodeBinding, InputBinding, Keymap, PadDir, VirtualAction, WindowsPadBackend,
};
use log::{info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::Duration;
use winit::keyboard::KeyCode;

const CONFIG_PATH: &str = "deadsync.ini";
const DEFAULT_MACHINE_NOTESKIN: &str = "cel";

// --- Minimal INI reader ---
#[derive(Debug, Default)]
pub struct SimpleIni {
    sections: HashMap<String, HashMap<String, String>>,
}

impl SimpleIni {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        self.sections.clear();

        let mut current_section: Option<String> = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            // Section header: [SectionName]
            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let name = &line[1..line.len() - 1];
                let section = name.trim().to_string();
                current_section = Some(section.clone());
                self.sections.entry(section).or_default();
                continue;
            }

            // Key/value pair: key=value
            if let Some(eq_idx) = line.find('=') {
                let (key_raw, value_raw) = line.split_at(eq_idx);
                let key = key_raw.trim();
                if key.is_empty() {
                    continue;
                }
                // Skip '=' and trim whitespace from the value.
                let value = value_raw[1..].trim().to_string();
                let section = current_section.clone().unwrap_or_default();
                self.sections
                    .entry(section)
                    .or_default()
                    .insert(key.to_string(), value);
            }
        }

        Ok(())
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections.get(section).and_then(|s| s.get(key)).cloned()
    }

    pub fn get_section(&self, section: &str) -> Option<&HashMap<String, String>> {
        self.sections.get(section)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenType {
    Exclusive,
    Borderless,
}

impl FullscreenType {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Exclusive => "Exclusive",
            Self::Borderless => "Borderless",
        }
    }
}

impl FromStr for FullscreenType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "exclusive" => Ok(Self::Exclusive),
            "borderless" => Ok(Self::Borderless),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakdownStyle {
    Sl,
    Sn,
}

impl BreakdownStyle {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Sl => "SL",
            Self::Sn => "SN",
        }
    }
}

impl FromStr for BreakdownStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sl" => Ok(Self::Sl),
            "sn" => Ok(Self::Sn),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultFailType {
    Immediate,
    ImmediateContinue,
}

impl DefaultFailType {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Immediate => "Immediate",
            Self::ImmediateContinue => "ImmediateContinue",
        }
    }
}

impl FromStr for DefaultFailType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "immediate" => Ok(Self::Immediate),
            "immediatecontinue" | "immediate_continue" => Ok(Self::ImmediateContinue),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicPatternInfoMode {
    Tech,
    Stamina,
    Auto,
}

impl SelectMusicPatternInfoMode {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Tech => "Tech",
            Self::Stamina => "Stamina",
            Self::Auto => "Auto",
        }
    }
}

impl FromStr for SelectMusicPatternInfoMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tech" => Ok(Self::Tech),
            "stamina" => Ok(Self::Stamina),
            "auto" => Ok(Self::Auto),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameFlag {
    Dance,
}

impl GameFlag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dance => "dance",
        }
    }
}

impl FromStr for GameFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dance" => Ok(Self::Dance),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeFlag {
    SimplyLove,
}

impl ThemeFlag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SimplyLove => "Simply Love",
        }
    }
}

impl FromStr for ThemeFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "simplylove" => Ok(Self::SimplyLove),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageFlag {
    English,
}

impl LanguageFlag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::English => "English",
        }
    }
}

impl FromStr for LanguageFlag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "english" => Ok(Self::English),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warn => "Warn",
            Self::Info => "Info",
            Self::Debug => "Debug",
            Self::Trace => "Trace",
        }
    }

    pub const fn as_level_filter(self) -> log::LevelFilter {
        match self {
            Self::Error => log::LevelFilter::Error,
            Self::Warn => log::LevelFilter::Warn,
            Self::Info => log::LevelFilter::Info,
            Self::Debug => log::LevelFilter::Debug,
            Self::Trace => log::LevelFilter::Trace,
        }
    }
}

impl FromStr for LogLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" | "err" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Windowed,
    Fullscreen(FullscreenType),
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub vsync: bool,
    /// 0 = uncapped. N > 0 = cap redraw scheduling to N FPS.
    pub max_fps: u16,
    pub windowed: bool,
    pub fullscreen_type: FullscreenType,
    pub display_monitor: usize,
    pub game_flag: GameFlag,
    pub theme_flag: ThemeFlag,
    pub language_flag: LanguageFlag,
    pub log_level: LogLevel,
    /// 0=Off, 1=FPS, 2=FPS+Stutter.
    pub show_stats_mode: u8,
    pub translated_titles: bool,
    pub mine_hit_sound: bool,
    // Global background brightness during gameplay (ITGmania: Pref "BGBrightness").
    // 1.0 = full brightness, 0.0 = black.
    pub bg_brightness: f32,
    // ITGmania/Simply Love parity: center the active single-player notefield in gameplay.
    pub center_1player_notefield: bool,
    /// ITGmania-style wheel banner cache toggle.
    pub banner_cache: bool,
    /// Downscale divisor for cached banners (ITG default: 2).
    pub banner_cache_scale_divisor: u8,
    /// Minimum cached banner dimension in pixels (ITG default: 32).
    pub banner_cache_min_dimension: u16,
    /// Round cached banner dimensions to nearest power-of-two (ITG default: on).
    pub banner_cache_pow2: bool,
    /// Cached banner color depth in bits (supported: 8/16/32; deadsync default: 32).
    pub banner_cache_color_depth: u8,
    pub display_width: u32,
    pub display_height: u32,
    pub video_renderer: BackendType,
    pub gfx_debug: bool,
    /// Windows-only: choose which gamepad backend to use.
    pub windows_gamepad_backend: WindowsPadBackend,
    // When using the Software video renderer:
    // 0 = Auto (use all logical cores)
    // 1 = Single-threaded
    // N >= 2 = cap at N threads (clamped to available cores).
    pub software_renderer_threads: u8,
    // When parsing simfiles at startup:
    // 0 = Auto (use all logical cores) for cache misses
    // 1 = Single-threaded
    // N >= 2 = cap at N threads (clamped to available cores).
    pub song_parsing_threads: u8,
    pub simply_love_color: i32,
    pub show_select_music_gameplay_timer: bool,
    pub show_select_music_banners: bool,
    pub show_select_music_breakdown: bool,
    pub show_select_music_cdtitles: bool,
    pub show_music_wheel_grades: bool,
    pub show_music_wheel_lamps: bool,
    pub show_select_music_previews: bool,
    pub select_music_preview_loop: bool,
    /// zmod parity: enable keyboard-only shortcuts like Ctrl+R restart.
    pub keyboard_features: bool,
    /// Startup flow: show Select Profile before continuing.
    pub machine_show_select_profile: bool,
    /// Startup flow: show Select Color before continuing.
    pub machine_show_select_color: bool,
    /// Startup flow: show Select Style before continuing.
    pub machine_show_select_style: bool,
    /// Startup flow: show Select Play Mode before continuing.
    pub machine_show_select_play_mode: bool,
    /// Post-session flow from Select Music/Course: show Evaluation Summary.
    pub machine_show_eval_summary: bool,
    /// Post-session flow from Select Music/Course: show Name Entry.
    pub machine_show_name_entry: bool,
    /// Post-session flow from Select Music/Course: show GameOver.
    pub machine_show_gameover: bool,
    /// zmod parity: gameplay/eval difficulty meter also displays text labels.
    pub zmod_rating_box_text: bool,
    /// Show one decimal place for live gameplay BPM when BPM is non-integer.
    pub show_bpm_decimal: bool,
    /// Machine default fail behavior (ITGmania DefaultFailType).
    pub default_fail_type: DefaultFailType,
    pub select_music_breakdown_style: BreakdownStyle,
    pub select_music_pattern_info_mode: SelectMusicPatternInfoMode,
    pub show_select_music_scorebox: bool,
    pub show_random_courses: bool,
    pub show_most_played_courses: bool,
    pub show_course_individual_scores: bool,
    pub autosubmit_course_scores_individually: bool,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub master_volume: u8,
    pub menu_music: bool,
    pub music_volume: u8,
    // ITGmania PrefsManager "MusicWheelSwitchSpeed" (default 15).
    pub music_wheel_switch_speed: u8,
    pub sfx_volume: u8,
    // None = auto (use device default sample rate)
    pub audio_sample_rate_hz: Option<u32>,
    pub auto_populate_gs_scores: bool,
    pub rate_mod_preserves_pitch: bool,
    pub enable_arrowcloud: bool,
    pub enable_boogiestats: bool,
    pub enable_groovestats: bool,
    pub fastload: bool,
    pub cachesongs: bool,
    // Whether to apply Gaussian smoothing to the eval histogram (Simply Love style)
    pub smooth_histogram: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vsync: false,
            max_fps: 0,
            windowed: true,
            fullscreen_type: FullscreenType::Exclusive,
            display_monitor: 0,
            game_flag: GameFlag::Dance,
            theme_flag: ThemeFlag::SimplyLove,
            language_flag: LanguageFlag::English,
            log_level: LogLevel::Warn,
            show_stats_mode: 0,
            translated_titles: false,
            mine_hit_sound: true,
            bg_brightness: 0.7,
            center_1player_notefield: false,
            banner_cache: true,
            banner_cache_scale_divisor: 2,
            banner_cache_min_dimension: 32,
            banner_cache_pow2: true,
            banner_cache_color_depth: 16,
            display_width: 1600,
            display_height: 900,
            video_renderer: BackendType::OpenGL,
            gfx_debug: false,
            windows_gamepad_backend: WindowsPadBackend::Wgi,
            software_renderer_threads: 1,
            song_parsing_threads: 0,
            simply_love_color: 2, // Corresponds to DEFAULT_COLOR_INDEX
            show_select_music_gameplay_timer: true,
            show_select_music_banners: true,
            show_select_music_breakdown: true,
            show_select_music_cdtitles: true,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: true,
            show_select_music_previews: true,
            select_music_preview_loop: true,
            keyboard_features: true,
            machine_show_select_profile: true,
            machine_show_select_color: true,
            machine_show_select_style: true,
            machine_show_select_play_mode: true,
            machine_show_eval_summary: true,
            machine_show_name_entry: true,
            machine_show_gameover: true,
            zmod_rating_box_text: false,
            show_bpm_decimal: false,
            default_fail_type: DefaultFailType::ImmediateContinue,
            select_music_breakdown_style: BreakdownStyle::Sl,
            select_music_pattern_info_mode: SelectMusicPatternInfoMode::Tech,
            show_select_music_scorebox: true,
            show_random_courses: true,
            show_most_played_courses: true,
            show_course_individual_scores: true,
            autosubmit_course_scores_individually: true,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            master_volume: 90,
            menu_music: true,
            music_volume: 100,
            music_wheel_switch_speed: 15,
            sfx_volume: 100,
            audio_sample_rate_hz: None,
            auto_populate_gs_scores: false,
            rate_mod_preserves_pitch: true,
            enable_arrowcloud: false,
            enable_boogiestats: false,
            enable_groovestats: false,
            fastload: true,
            cachesongs: true,
            smooth_histogram: true,
        }
    }
}

impl Config {
    pub const fn display_mode(&self) -> DisplayMode {
        if self.windowed {
            DisplayMode::Windowed
        } else {
            DisplayMode::Fullscreen(self.fullscreen_type)
        }
    }
}

// Global, mutable configuration instance.
static CONFIG: std::sync::LazyLock<Mutex<Config>> =
    std::sync::LazyLock::new(|| Mutex::new(Config::default()));
static MACHINE_DEFAULT_NOTESKIN: std::sync::LazyLock<Mutex<String>> =
    std::sync::LazyLock::new(|| Mutex::new(DEFAULT_MACHINE_NOTESKIN.to_string()));
static SAVE_TX: std::sync::LazyLock<Option<mpsc::Sender<SaveReq>>> =
    std::sync::LazyLock::new(start_save_worker);

enum SaveReq {
    Write(String),
    Flush(mpsc::Sender<()>),
}

fn start_save_worker() -> Option<mpsc::Sender<SaveReq>> {
    let (tx, rx) = mpsc::channel::<SaveReq>();
    let spawn = thread::Builder::new()
        .name("deadsync-config-save".to_string())
        .spawn(move || save_worker_loop(rx));
    match spawn {
        Ok(_) => Some(tx),
        Err(e) => {
            warn!("Failed to start config save worker thread: {e}. Falling back to sync writes.");
            None
        }
    }
}

#[inline(always)]
fn queue_save_write(content: String) {
    if let Some(tx) = SAVE_TX.as_ref() {
        if let Err(err) = tx.send(SaveReq::Write(content))
            && let SaveReq::Write(content) = err.0
        {
            write_config_file(&content);
        }
        return;
    }
    write_config_file(&content);
}

fn save_worker_loop(rx: mpsc::Receiver<SaveReq>) {
    let mut pending_write: Option<String> = None;
    let mut flush_acks: Vec<mpsc::Sender<()>> = Vec::with_capacity(2);
    while let Ok(msg) = rx.recv() {
        match msg {
            SaveReq::Write(content) => pending_write = Some(content),
            SaveReq::Flush(ack) => flush_acks.push(ack),
        }
        while let Ok(msg) = rx.try_recv() {
            match msg {
                SaveReq::Write(content) => pending_write = Some(content),
                SaveReq::Flush(ack) => flush_acks.push(ack),
            }
        }
        if let Some(content) = pending_write.take() {
            write_config_file(&content);
        }
        for ack in flush_acks.drain(..) {
            let _ = ack.send(());
        }
    }
    if let Some(content) = pending_write.take() {
        write_config_file(&content);
    }
}

#[inline(always)]
fn write_config_file(content: &str) {
    if let Err(e) = std::fs::write(CONFIG_PATH, content) {
        warn!("Failed to save config file: {e}");
    }
}

pub fn flush_pending_saves() {
    if let Some(tx) = SAVE_TX.as_ref() {
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        if tx.send(SaveReq::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(5));
        }
    }
}

// --- File I/O ---

#[inline(always)]
const fn normalize_banner_cache_color_depth(bits: u8) -> u8 {
    if bits <= 8 {
        8
    } else if bits <= 16 {
        16
    } else {
        32
    }
}

#[inline(always)]
fn normalize_machine_default_noteskin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_MACHINE_NOTESKIN.to_string();
    }
    trimmed.to_ascii_lowercase()
}

fn create_default_config_file() -> Result<(), std::io::Error> {
    info!("'{CONFIG_PATH}' not found, creating with default values.");
    let default = Config::default();

    let mut content = String::new();

    // [Options] section - keys in alphabetical order
    content.push_str("[Options]\n");
    content.push_str("AudioSampleRateHz=Auto\n");
    content.push_str(&format!(
        "AutoPopulateGrooveStatsScores={}\n",
        if default.auto_populate_gs_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("BGBrightness={}\n", default.bg_brightness));
    content.push_str(&format!(
        "BannerCache={}\n",
        if default.banner_cache { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "BannerCacheColorDepth={}\n",
        default.banner_cache_color_depth
    ));
    content.push_str(&format!(
        "BannerCacheMinDimension={}\n",
        default.banner_cache_min_dimension
    ));
    content.push_str(&format!(
        "BannerCachePow2={}\n",
        if default.banner_cache_pow2 { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "BannerCacheScaleDivisor={}\n",
        default.banner_cache_scale_divisor
    ));
    content.push_str(&format!(
        "CacheSongs={}\n",
        if default.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Center1Player={}\n",
        if default.center_1player_notefield {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseAutosubmitScoresIndividually={}\n",
        if default.autosubmit_course_scores_individually {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowIndividualScores={}\n",
        if default.show_course_individual_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowMostPlayed={}\n",
        if default.show_most_played_courses {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowRandom={}\n",
        if default.show_random_courses {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "DefaultFailType={}\n",
        default.default_fail_type.as_str()
    ));
    content.push_str(&format!("DefaultNoteSkin={DEFAULT_MACHINE_NOTESKIN}\n"));
    content.push_str(&format!("DisplayHeight={}\n", default.display_height));
    content.push_str(&format!("DisplayWidth={}\n", default.display_width));
    content.push_str(&format!("DisplayMonitor={}\n", default.display_monitor));
    content.push_str(&format!(
        "EnableArrowCloud={}\n",
        if default.enable_arrowcloud { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableBoogieStats={}\n",
        if default.enable_boogiestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableGrooveStats={}\n",
        if default.enable_groovestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FastLoad={}\n",
        if default.fastload { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FullscreenType={}\n",
        default.fullscreen_type.as_str()
    ));
    content.push_str(&format!("Game={}\n", default.game_flag.as_str()));
    content.push_str(&format!(
        "GamepadBackend={}\n",
        default.windows_gamepad_backend
    ));
    content.push_str(&format!(
        "GfxDebug={}\n",
        if default.gfx_debug { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "GlobalOffsetSeconds={}\n",
        default.global_offset_seconds
    ));
    content.push_str(&format!("Language={}\n", default.language_flag.as_str()));
    content.push_str(&format!("LogLevel={}\n", default.log_level.as_str()));
    content.push_str(&format!("MaxFps={}\n", default.max_fps));
    content.push_str(&format!(
        "VisualDelaySeconds={}\n",
        default.visual_delay_seconds
    ));
    content.push_str(&format!("MasterVolume={}\n", default.master_volume));
    content.push_str(&format!(
        "MenuMusic={}\n",
        if default.menu_music { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MineHitSound={}\n",
        if default.mine_hit_sound { "1" } else { "0" }
    ));
    content.push_str(&format!("MusicVolume={}\n", default.music_volume));
    content.push_str(&format!(
        "MusicWheelSwitchSpeed={}\n",
        default.music_wheel_switch_speed.max(1)
    ));
    content.push_str(&format!(
        "RateModPreservesPitch={}\n",
        if default.rate_mod_preserves_pitch {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicBreakdown={}\n",
        default.select_music_breakdown_style.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicShowBanners={}\n",
        if default.show_select_music_banners {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowBreakdown={}\n",
        if default.show_select_music_breakdown {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowCDTitles={}\n",
        if default.show_select_music_cdtitles {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelGrades={}\n",
        if default.show_music_wheel_grades {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelLamps={}\n",
        if default.show_music_wheel_lamps {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviews={}\n",
        if default.show_select_music_previews {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviewLoop={}\n",
        if default.select_music_preview_loop {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPatternInfo={}\n",
        default.select_music_pattern_info_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScorebox={}\n",
        if default.show_select_music_scorebox {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if default.show_stats_mode != 0 {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowStatsMode={}\n",
        default.show_stats_mode.min(2)
    ));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if default.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "SongParsingThreads={}\n",
        default.song_parsing_threads
    ));
    content.push_str(&format!(
        "SoftwareRendererThreads={}\n",
        default.software_renderer_threads
    ));
    content.push_str(&format!("Theme={}\n", default.theme_flag.as_str()));
    content.push_str(&format!("SFXVolume={}\n", default.sfx_volume));
    content.push_str(&format!(
        "TranslatedTitles={}\n",
        if default.translated_titles { "1" } else { "0" }
    ));
    content.push_str(&format!("VideoRenderer={}\n", default.video_renderer));
    content.push_str(&format!(
        "Vsync={}\n",
        if default.vsync { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Windowed={}\n",
        if default.windowed { "1" } else { "0" }
    ));
    content.push('\n');

    // [Keymaps] section with sane defaults (comma-separated)
    content.push_str("[Keymaps]\n");
    content.push_str("P1_Back=KeyCode::Escape\n");
    content.push_str("P1_Down=KeyCode::ArrowDown,KeyCode::KeyS\n");
    content.push_str("P1_Left=KeyCode::ArrowLeft,KeyCode::KeyA\n");
    content.push_str("P1_MenuDown=\n");
    content.push_str("P1_MenuLeft=\n");
    content.push_str("P1_MenuRight=\n");
    content.push_str("P1_MenuUp=\n");
    content.push_str("P1_Operator=\n");
    content.push_str("P1_Restart=\n");
    content.push_str("P1_Right=KeyCode::ArrowRight,KeyCode::KeyD\n");
    content.push_str("P1_Select=KeyCode::Slash\n");
    content.push_str("P1_Start=KeyCode::Enter\n");
    content.push_str("P1_Up=KeyCode::ArrowUp,KeyCode::KeyW\n");
    // Player 2 keyboard defaults: numpad directions + Start on NumpadEnter + Back on Numpad0.
    content.push_str("P2_Back=KeyCode::Numpad0\n");
    content.push_str("P2_Down=KeyCode::Numpad2\n");
    content.push_str("P2_Left=KeyCode::Numpad4\n");
    content.push_str("P2_MenuDown=\n");
    content.push_str("P2_MenuLeft=\n");
    content.push_str("P2_MenuRight=\n");
    content.push_str("P2_MenuUp=\n");
    content.push_str("P2_Operator=\n");
    content.push_str("P2_Restart=\n");
    content.push_str("P2_Right=KeyCode::Numpad6\n");
    content.push_str("P2_Select=KeyCode::NumpadDecimal\n");
    content.push_str("P2_Start=KeyCode::NumpadEnter\n");
    content.push_str("P2_Up=KeyCode::Numpad8\n");
    content.push('\n');

    // [Theme] section should be last
    content.push_str("[Theme]\n");
    content.push_str(&format!(
        "KeyboardFeatures={}\n",
        if default.keyboard_features { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachineShowEvalSummary={}\n",
        if default.machine_show_eval_summary {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowGameOver={}\n",
        if default.machine_show_gameover {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowNameEntry={}\n",
        if default.machine_show_name_entry {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectColor={}\n",
        if default.machine_show_select_color {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectPlayMode={}\n",
        if default.machine_show_select_play_mode {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectProfile={}\n",
        if default.machine_show_select_profile {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectStyle={}\n",
        if default.machine_show_select_style {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowSelectMusicGameplayTimer={}\n",
        if default.show_select_music_gameplay_timer {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("SimplyLoveColor={}\n", default.simply_love_color));
    content.push_str(&format!(
        "ZmodRatingBoxText={}\n",
        if default.zmod_rating_box_text {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowBpmDecimal={}\n",
        if default.show_bpm_decimal { "1" } else { "0" }
    ));
    content.push('\n');

    std::fs::write(CONFIG_PATH, content)
}

pub fn load() {
    // --- Load main deadsync.ini ---
    if !std::path::Path::new(CONFIG_PATH).exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {e}");
    }

    let mut conf = SimpleIni::new();
    match conf.load(CONFIG_PATH) {
        Ok(()) => {
            {
                let noteskin = conf
                    .get("Options", "DefaultNoteSkin")
                    .map(|v| normalize_machine_default_noteskin(&v))
                    .unwrap_or_else(|| DEFAULT_MACHINE_NOTESKIN.to_string());
                *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = noteskin;
            }

            // This block populates the global CONFIG struct from the file,
            // using default values for any missing keys.
            {
                let mut cfg = CONFIG.lock().unwrap();
                let default = Config::default();

                cfg.vsync = conf
                    .get("Options", "Vsync")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.vsync, |v| v != 0);
                cfg.max_fps = conf
                    .get("Options", "MaxFps")
                    .and_then(|v| v.parse::<u16>().ok())
                    .unwrap_or(default.max_fps);
                cfg.windowed = conf
                    .get("Options", "Windowed")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.windowed, |v| v != 0);
                cfg.fullscreen_type = conf
                    .get("Options", "FullscreenType")
                    .and_then(|v| FullscreenType::from_str(&v).ok())
                    .unwrap_or(default.fullscreen_type);
                cfg.game_flag = conf
                    .get("Options", "Game")
                    .and_then(|v| GameFlag::from_str(&v).ok())
                    .unwrap_or(default.game_flag);
                cfg.display_monitor = conf
                    .get("Options", "DisplayMonitor")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(default.display_monitor);
                cfg.auto_populate_gs_scores = conf
                    .get("Options", "AutoPopulateGrooveStatsScores")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.auto_populate_gs_scores, |v| v != 0);
                cfg.enable_groovestats = conf
                    .get("Options", "EnableGrooveStats")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.enable_groovestats, |v| v != 0);
                cfg.enable_arrowcloud = conf
                    .get("Options", "EnableArrowCloud")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.enable_arrowcloud, |v| v != 0);
                cfg.enable_boogiestats = conf
                    .get("Options", "EnableBoogieStats")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.enable_boogiestats, |v| v != 0);
                cfg.mine_hit_sound = conf
                    .get("Options", "MineHitSound")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.mine_hit_sound, |v| v != 0);
                cfg.show_stats_mode = conf
                    .get("Options", "ShowStatsMode")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map(|v| v.min(2))
                    .or_else(|| {
                        conf.get("Options", "ShowStats")
                            .and_then(|v| v.parse::<u8>().ok())
                            .map(|v| if v != 0 { 1 } else { 0 })
                    })
                    .unwrap_or(default.show_stats_mode);
                cfg.translated_titles = conf
                    .get("Options", "TranslatedTitles")
                    .or_else(|| conf.get("Options", "translatedtitles"))
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.translated_titles);
                cfg.bg_brightness = conf
                    .get("Options", "BGBrightness")
                    .and_then(|v| v.parse::<f32>().ok())
                    .map_or(default.bg_brightness, |v| v.clamp(0.0, 1.0));
                cfg.center_1player_notefield = conf
                    .get("Options", "Center1Player")
                    .or_else(|| conf.get("Options", "CenteredP1Notefield"))
                    .map(|v| v.trim().to_ascii_lowercase())
                    .and_then(|v| match v.as_str() {
                        "1" | "true" | "yes" | "on" => Some(true),
                        "0" | "false" | "no" | "off" => Some(false),
                        _ => None,
                    })
                    .unwrap_or(default.center_1player_notefield);
                cfg.autosubmit_course_scores_individually = conf
                    .get("Options", "CourseAutosubmitScoresIndividually")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.autosubmit_course_scores_individually, |v| v != 0);
                cfg.show_course_individual_scores = conf
                    .get("Options", "CourseShowIndividualScores")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_course_individual_scores, |v| v != 0);
                cfg.show_most_played_courses = conf
                    .get("Options", "CourseShowMostPlayed")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_most_played_courses, |v| v != 0);
                cfg.show_random_courses = conf
                    .get("Options", "CourseShowRandom")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_random_courses, |v| v != 0);
                cfg.default_fail_type = conf
                    .get("Options", "DefaultFailType")
                    .and_then(|v| DefaultFailType::from_str(&v).ok())
                    .unwrap_or(default.default_fail_type);
                cfg.banner_cache = conf
                    .get("Options", "BannerCache")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.banner_cache, |v| v != 0);
                cfg.banner_cache_color_depth = conf
                    .get("Options", "BannerCacheColorDepth")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(
                        default.banner_cache_color_depth,
                        normalize_banner_cache_color_depth,
                    );
                cfg.banner_cache_min_dimension = conf
                    .get("Options", "BannerCacheMinDimension")
                    .and_then(|v| v.parse::<u16>().ok())
                    .map_or(default.banner_cache_min_dimension, |v| v.clamp(1, 2048));
                cfg.banner_cache_pow2 = conf
                    .get("Options", "BannerCachePow2")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.banner_cache_pow2, |v| v != 0);
                cfg.banner_cache_scale_divisor = conf
                    .get("Options", "BannerCacheScaleDivisor")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.banner_cache_scale_divisor, |v| v.clamp(1, 8));
                cfg.display_width = conf
                    .get("Options", "DisplayWidth")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.display_width);
                cfg.display_height = conf
                    .get("Options", "DisplayHeight")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.display_height);
                cfg.video_renderer = conf
                    .get("Options", "VideoRenderer")
                    .and_then(|s| BackendType::from_str(&s).ok())
                    .unwrap_or(default.video_renderer);
                cfg.windows_gamepad_backend = conf
                    .get("Options", "GamepadBackend")
                    .and_then(|s| WindowsPadBackend::from_str(&s).ok())
                    .unwrap_or(default.windows_gamepad_backend);
                cfg.gfx_debug = conf
                    .get("Options", "GfxDebug")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.gfx_debug, |v| v != 0);
                cfg.global_offset_seconds = conf
                    .get("Options", "GlobalOffsetSeconds")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.global_offset_seconds);
                cfg.language_flag = conf
                    .get("Options", "Language")
                    .and_then(|v| LanguageFlag::from_str(&v).ok())
                    .unwrap_or(default.language_flag);
                cfg.log_level = conf
                    .get("Options", "LogLevel")
                    .and_then(|v| LogLevel::from_str(&v).ok())
                    .unwrap_or(default.log_level);
                cfg.visual_delay_seconds = conf
                    .get("Options", "VisualDelaySeconds")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.visual_delay_seconds);
                cfg.master_volume = conf
                    .get("Options", "MasterVolume")
                    .and_then(|v| v.parse().ok())
                    .map_or(default.master_volume, |v: u8| v.clamp(0, 100));
                cfg.menu_music = conf
                    .get("Options", "MenuMusic")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.menu_music, |v| v != 0);
                cfg.music_volume = conf
                    .get("Options", "MusicVolume")
                    .and_then(|v| v.parse().ok())
                    .map_or(default.music_volume, |v: u8| v.clamp(0, 100));
                cfg.music_wheel_switch_speed = conf
                    .get("Options", "MusicWheelSwitchSpeed")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.music_wheel_switch_speed, |v| v.max(1));
                cfg.sfx_volume = conf
                    .get("Options", "SFXVolume")
                    .and_then(|v| v.parse().ok())
                    .map_or(default.sfx_volume, |v: u8| v.clamp(0, 100));
                cfg.audio_sample_rate_hz = conf
                    .get("Options", "AudioSampleRateHz")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                            None
                        } else {
                            v.parse::<u32>().ok()
                        }
                    })
                    .or(default.audio_sample_rate_hz);
                cfg.rate_mod_preserves_pitch = conf
                    .get("Options", "RateModPreservesPitch")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.rate_mod_preserves_pitch, |v| v != 0);
                cfg.select_music_breakdown_style = conf
                    .get("Options", "SelectMusicBreakdown")
                    .and_then(|v| BreakdownStyle::from_str(&v).ok())
                    .unwrap_or(default.select_music_breakdown_style);
                cfg.show_select_music_banners = conf
                    .get("Options", "SelectMusicShowBanners")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_banners, |v| v != 0);
                cfg.show_select_music_breakdown = conf
                    .get("Options", "SelectMusicShowBreakdown")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_breakdown, |v| v != 0);
                cfg.show_select_music_cdtitles = conf
                    .get("Options", "SelectMusicShowCDTitles")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_cdtitles, |v| v != 0);
                cfg.show_music_wheel_grades = conf
                    .get("Options", "SelectMusicWheelGrades")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_music_wheel_grades, |v| v != 0);
                cfg.show_music_wheel_lamps = conf
                    .get("Options", "SelectMusicWheelLamps")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_music_wheel_lamps, |v| v != 0);
                cfg.show_select_music_previews = conf
                    .get("Options", "SelectMusicPreviews")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_previews, |v| v != 0);
                cfg.select_music_preview_loop = conf
                    .get("Options", "SelectMusicPreviewLoop")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.select_music_preview_loop, |v| v != 0);
                cfg.select_music_pattern_info_mode = conf
                    .get("Options", "SelectMusicPatternInfo")
                    .and_then(|v| SelectMusicPatternInfoMode::from_str(&v).ok())
                    .unwrap_or(default.select_music_pattern_info_mode);
                cfg.show_select_music_scorebox = conf
                    .get("Options", "SelectMusicScorebox")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_scorebox, |v| v != 0);
                cfg.fastload = conf
                    .get("Options", "FastLoad")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.fastload, |v| v != 0);
                cfg.cachesongs = conf
                    .get("Options", "CacheSongs")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.cachesongs, |v| v != 0);
                cfg.song_parsing_threads = conf
                    .get("Options", "SongParsingThreads")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                            Some(0u8)
                        } else {
                            v.parse::<u8>().ok()
                        }
                    })
                    .unwrap_or(default.song_parsing_threads);
                cfg.smooth_histogram = conf
                    .get("Options", "SmoothHistogram")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.smooth_histogram, |v| v != 0);
                cfg.theme_flag = conf
                    .get("Options", "Theme")
                    .and_then(|v| ThemeFlag::from_str(&v).ok())
                    .unwrap_or(default.theme_flag);
                cfg.software_renderer_threads = conf
                    .get("Options", "SoftwareRendererThreads")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                            Some(0u8)
                        } else {
                            v.parse::<u8>().ok()
                        }
                    })
                    .unwrap_or(default.software_renderer_threads);
                cfg.simply_love_color = conf
                    .get("Theme", "SimplyLoveColor")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.simply_love_color);
                cfg.show_select_music_gameplay_timer = conf
                    .get("Theme", "ShowSelectMusicGameplayTimer")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.show_select_music_gameplay_timer);
                cfg.keyboard_features = conf
                    .get("Theme", "KeyboardFeatures")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.keyboard_features);
                cfg.machine_show_eval_summary = conf
                    .get("Theme", "MachineShowEvalSummary")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_eval_summary);
                cfg.machine_show_name_entry = conf
                    .get("Theme", "MachineShowNameEntry")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_name_entry);
                cfg.machine_show_gameover = conf
                    .get("Theme", "MachineShowGameOver")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_gameover);
                cfg.machine_show_select_profile = conf
                    .get("Theme", "MachineShowSelectProfile")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_profile);
                cfg.machine_show_select_color = conf
                    .get("Theme", "MachineShowSelectColor")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_color);
                cfg.machine_show_select_style = conf
                    .get("Theme", "MachineShowSelectStyle")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_style);
                cfg.machine_show_select_play_mode = conf
                    .get("Theme", "MachineShowSelectPlayMode")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_play_mode);
                cfg.zmod_rating_box_text = conf
                    .get("Theme", "ZmodRatingBoxText")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.zmod_rating_box_text);
                cfg.show_bpm_decimal = conf
                    .get("Theme", "ShowBpmDecimal")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.show_bpm_decimal);

                info!("Configuration loaded from '{CONFIG_PATH}'.");
            } // Lock on CONFIG is released here.

            // Load keymaps from the same INI and publish globally.
            let km = load_keymap_from_ini_local(&conf);
            crate::core::input::set_keymap(km);

            // Only write [Options]/[Theme] if any of those keys are missing.
            let missing_opts = {
                let has = |sec: &str, key: &str| conf.get(sec, key).is_some();
                let mut miss = false;
                let options_keys = [
                    "AudioSampleRateHz",
                    "AutoPopulateGrooveStatsScores",
                    "BGBrightness",
                    "BannerCache",
                    "BannerCacheColorDepth",
                    "BannerCacheMinDimension",
                    "BannerCachePow2",
                    "BannerCacheScaleDivisor",
                    "CacheSongs",
                    "Center1Player",
                    "CourseAutosubmitScoresIndividually",
                    "CourseShowIndividualScores",
                    "CourseShowMostPlayed",
                    "CourseShowRandom",
                    "DefaultFailType",
                    "DefaultNoteSkin",
                    "DisplayHeight",
                    "DisplayWidth",
                    "FastLoad",
                    "EnableArrowCloud",
                    "EnableBoogieStats",
                    "EnableGrooveStats",
                    "FullscreenType",
                    "Game",
                    "GamepadBackend",
                    "GfxDebug",
                    "GlobalOffsetSeconds",
                    "Language",
                    "LogLevel",
                    "MaxFps",
                    "MasterVolume",
                    "MenuMusic",
                    "MineHitSound",
                    "MusicVolume",
                    "MusicWheelSwitchSpeed",
                    "SongParsingThreads",
                    "RateModPreservesPitch",
                    "SelectMusicBreakdown",
                    "SelectMusicShowBanners",
                    "SelectMusicShowBreakdown",
                    "SelectMusicShowCDTitles",
                    "SelectMusicWheelGrades",
                    "SelectMusicWheelLamps",
                    "SelectMusicPreviews",
                    "SelectMusicPreviewLoop",
                    "SelectMusicPatternInfo",
                    "SelectMusicScorebox",
                    "ShowStats",
                    "ShowStatsMode",
                    "SmoothHistogram",
                    "SFXVolume",
                    "SoftwareRendererThreads",
                    "Theme",
                    "TranslatedTitles",
                    "VideoRenderer",
                    "VisualDelaySeconds",
                    "Vsync",
                    "Windowed",
                ];
                for k in options_keys {
                    if !has("Options", k) {
                        miss = true;
                        break;
                    }
                }
                if !miss && !has("Theme", "SimplyLoveColor") {
                    miss = true;
                }
                if !miss && !has("Theme", "ShowSelectMusicGameplayTimer") {
                    miss = true;
                }
                if !miss && !has("Theme", "KeyboardFeatures") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowEvalSummary") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowGameOver") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowNameEntry") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectColor") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectPlayMode") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectProfile") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectStyle") {
                    miss = true;
                }
                if !miss && !has("Theme", "ZmodRatingBoxText") {
                    miss = true;
                }
                if !miss && !has("Theme", "ShowBpmDecimal") {
                    miss = true;
                }
                miss
            };
            if missing_opts {
                save_without_keymaps();
                info!("'{CONFIG_PATH}' updated with default values for any missing fields.");
            } else {
                info!("Configuration OK; no write needed.");
            }
        }
        Err(e) => {
            warn!("Failed to load '{CONFIG_PATH}': {e}. Using default values.");
            *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
        }
    }
}

// --- Keymap defaults and parsing (kept in config to avoid coupling input.rs to config) ---

// Stable iteration order for all virtual actions when serializing [Keymaps].
const ALL_VIRTUAL_ACTIONS: [VirtualAction; 26] = [
    VirtualAction::p1_back,
    VirtualAction::p1_down,
    VirtualAction::p1_left,
    VirtualAction::p1_menu_down,
    VirtualAction::p1_menu_left,
    VirtualAction::p1_menu_right,
    VirtualAction::p1_menu_up,
    VirtualAction::p1_operator,
    VirtualAction::p1_restart,
    VirtualAction::p1_right,
    VirtualAction::p1_select,
    VirtualAction::p1_start,
    VirtualAction::p1_up,
    VirtualAction::p2_back,
    VirtualAction::p2_down,
    VirtualAction::p2_left,
    VirtualAction::p2_menu_down,
    VirtualAction::p2_menu_left,
    VirtualAction::p2_menu_right,
    VirtualAction::p2_menu_up,
    VirtualAction::p2_operator,
    VirtualAction::p2_restart,
    VirtualAction::p2_right,
    VirtualAction::p2_select,
    VirtualAction::p2_start,
    VirtualAction::p2_up,
];

fn default_keymap_local() -> Keymap {
    use VirtualAction as A;
    let mut km = Keymap::default();
    // Player 1 defaults (WASD + arrows, Enter/Escape).
    km.bind(
        A::p1_up,
        &[
            InputBinding::Key(KeyCode::ArrowUp),
            InputBinding::Key(KeyCode::KeyW),
        ],
    );
    km.bind(
        A::p1_down,
        &[
            InputBinding::Key(KeyCode::ArrowDown),
            InputBinding::Key(KeyCode::KeyS),
        ],
    );
    km.bind(
        A::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::Key(KeyCode::KeyA),
        ],
    );
    km.bind(
        A::p1_right,
        &[
            InputBinding::Key(KeyCode::ArrowRight),
            InputBinding::Key(KeyCode::KeyD),
        ],
    );
    km.bind(A::p1_select, &[InputBinding::Key(KeyCode::Slash)]);
    km.bind(A::p1_start, &[InputBinding::Key(KeyCode::Enter)]);
    km.bind(A::p1_back, &[InputBinding::Key(KeyCode::Escape)]);
    // Player 2 defaults (numpad directions + Start on NumpadEnter).
    km.bind(A::p2_up, &[InputBinding::Key(KeyCode::Numpad8)]);
    km.bind(A::p2_down, &[InputBinding::Key(KeyCode::Numpad2)]);
    km.bind(A::p2_left, &[InputBinding::Key(KeyCode::Numpad4)]);
    km.bind(A::p2_right, &[InputBinding::Key(KeyCode::Numpad6)]);
    km.bind(A::p2_select, &[InputBinding::Key(KeyCode::NumpadDecimal)]);
    km.bind(A::p2_start, &[InputBinding::Key(KeyCode::NumpadEnter)]);
    km.bind(A::p2_back, &[InputBinding::Key(KeyCode::Numpad0)]);
    // Leave P2_Menu/Operator/Restart unbound by default for now.
    km
}

#[inline(always)]
fn parse_action_key_lower(k: &str) -> Option<VirtualAction> {
    use VirtualAction::{
        p1_back, p1_down, p1_left, p1_menu_down, p1_menu_left, p1_menu_right, p1_menu_up,
        p1_operator, p1_restart, p1_right, p1_select, p1_start, p1_up, p2_back, p2_down, p2_left,
        p2_menu_down, p2_menu_left, p2_menu_right, p2_menu_up, p2_operator, p2_restart, p2_right,
        p2_select, p2_start, p2_up,
    };
    match k {
        "p1_up" => Some(p1_up),
        "p1_down" => Some(p1_down),
        "p1_left" => Some(p1_left),
        "p1_right" => Some(p1_right),
        "p1_start" => Some(p1_start),
        "p1_back" => Some(p1_back),
        "p1_menuup" => Some(p1_menu_up),
        "p1_menudown" => Some(p1_menu_down),
        "p1_menuleft" => Some(p1_menu_left),
        "p1_menuright" => Some(p1_menu_right),
        "p1_select" => Some(p1_select),
        "p1_operator" => Some(p1_operator),
        "p1_restart" => Some(p1_restart),
        "p2_up" => Some(p2_up),
        "p2_down" => Some(p2_down),
        "p2_left" => Some(p2_left),
        "p2_right" => Some(p2_right),
        "p2_start" => Some(p2_start),
        "p2_back" => Some(p2_back),
        "p2_menuup" => Some(p2_menu_up),
        "p2_menudown" => Some(p2_menu_down),
        "p2_menuleft" => Some(p2_menu_left),
        "p2_menuright" => Some(p2_menu_right),
        "p2_select" => Some(p2_select),
        "p2_operator" => Some(p2_operator),
        "p2_restart" => Some(p2_restart),
        _ => None,
    }
}

#[inline(always)]
const fn action_to_ini_key(action: VirtualAction) -> &'static str {
    use VirtualAction::{
        p1_back, p1_down, p1_left, p1_menu_down, p1_menu_left, p1_menu_right, p1_menu_up,
        p1_operator, p1_restart, p1_right, p1_select, p1_start, p1_up, p2_back, p2_down, p2_left,
        p2_menu_down, p2_menu_left, p2_menu_right, p2_menu_up, p2_operator, p2_restart, p2_right,
        p2_select, p2_start, p2_up,
    };
    match action {
        p1_up => "P1_Up",
        p1_down => "P1_Down",
        p1_left => "P1_Left",
        p1_right => "P1_Right",
        p1_start => "P1_Start",
        p1_back => "P1_Back",
        p1_menu_up => "P1_MenuUp",
        p1_menu_down => "P1_MenuDown",
        p1_menu_left => "P1_MenuLeft",
        p1_menu_right => "P1_MenuRight",
        p1_select => "P1_Select",
        p1_operator => "P1_Operator",
        p1_restart => "P1_Restart",
        p2_up => "P2_Up",
        p2_down => "P2_Down",
        p2_left => "P2_Left",
        p2_right => "P2_Right",
        p2_start => "P2_Start",
        p2_back => "P2_Back",
        p2_menu_up => "P2_MenuUp",
        p2_menu_down => "P2_MenuDown",
        p2_menu_left => "P2_MenuLeft",
        p2_menu_right => "P2_MenuRight",
        p2_select => "P2_Select",
        p2_operator => "P2_Operator",
        p2_restart => "P2_Restart",
    }
}

#[inline(always)]
fn binding_to_token(binding: InputBinding) -> String {
    match binding {
        InputBinding::Key(code) => format!("KeyCode::{code:?}"),
        InputBinding::PadDir(dir) => format!("PadDir::{dir:?}"),
        InputBinding::PadDirOn { device, dir } => {
            format!("Pad{device}::Dir::{dir:?}")
        }
        InputBinding::GamepadCode(binding) => {
            let mut s = String::new();
            use std::fmt::Write;
            let _ = write!(&mut s, "PadCode[0x{:08X}]", binding.code_u32);
            if let Some(device) = binding.device {
                let _ = write!(&mut s, "@{device}");
            }
            if let Some(uuid) = binding.uuid {
                s.push('#');
                for b in &uuid {
                    let _ = write!(&mut s, "{b:02X}");
                }
            }
            s
        }
    }
}

#[inline(always)]
fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    let t = tok.trim();
    // Keyboard
    if let Some(rest) = t.strip_prefix("KeyCode::") {
        let code = match rest {
            // Special keys
            "Enter" => KeyCode::Enter,
            "Escape" => KeyCode::Escape,
            "ArrowUp" => KeyCode::ArrowUp,
            "ArrowDown" => KeyCode::ArrowDown,
            "ArrowLeft" => KeyCode::ArrowLeft,
            "ArrowRight" => KeyCode::ArrowRight,
            "Slash" => KeyCode::Slash,
            // Numpad keys
            "Numpad0" => KeyCode::Numpad0,
            "Numpad1" => KeyCode::Numpad1,
            "Numpad2" => KeyCode::Numpad2,
            "Numpad3" => KeyCode::Numpad3,
            "Numpad4" => KeyCode::Numpad4,
            "Numpad5" => KeyCode::Numpad5,
            "Numpad6" => KeyCode::Numpad6,
            "Numpad7" => KeyCode::Numpad7,
            "Numpad8" => KeyCode::Numpad8,
            "Numpad9" => KeyCode::Numpad9,
            "NumpadAdd" => KeyCode::NumpadAdd,
            "NumpadDivide" => KeyCode::NumpadDivide,
            "NumpadDecimal" => KeyCode::NumpadDecimal,
            "NumpadComma" => KeyCode::NumpadComma,
            "NumpadEnter" => KeyCode::NumpadEnter,
            "NumpadEqual" => KeyCode::NumpadEqual,
            "NumpadMultiply" => KeyCode::NumpadMultiply,
            "NumpadSubtract" => KeyCode::NumpadSubtract,
            // Letter keys A-Z
            "KeyA" => KeyCode::KeyA,
            "KeyB" => KeyCode::KeyB,
            "KeyC" => KeyCode::KeyC,
            "KeyD" => KeyCode::KeyD,
            "KeyE" => KeyCode::KeyE,
            "KeyF" => KeyCode::KeyF,
            "KeyG" => KeyCode::KeyG,
            "KeyH" => KeyCode::KeyH,
            "KeyI" => KeyCode::KeyI,
            "KeyJ" => KeyCode::KeyJ,
            "KeyK" => KeyCode::KeyK,
            "KeyL" => KeyCode::KeyL,
            "KeyM" => KeyCode::KeyM,
            "KeyN" => KeyCode::KeyN,
            "KeyO" => KeyCode::KeyO,
            "KeyP" => KeyCode::KeyP,
            "KeyQ" => KeyCode::KeyQ,
            "KeyR" => KeyCode::KeyR,
            "KeyS" => KeyCode::KeyS,
            "KeyT" => KeyCode::KeyT,
            "KeyU" => KeyCode::KeyU,
            "KeyV" => KeyCode::KeyV,
            "KeyW" => KeyCode::KeyW,
            "KeyX" => KeyCode::KeyX,
            "KeyY" => KeyCode::KeyY,
            "KeyZ" => KeyCode::KeyZ,
            _ => return None,
        };
        return Some(InputBinding::Key(code));
    }

    // Gamepad low-level code binding:
    //   PadCode[0xDEADBEEF]
    //   PadCode[0xDEADBEEF]@0
    //   PadCode[0xDEADBEEF]#00112233AABBCCDDEEFF001122334455
    //   PadCode[0xDEADBEEF]@0#00112233AABBCCDDEEFF001122334455
    //
    // where 0x... or decimal is the `PadCode(u32)` shown in the Sandbox/Input screens,
    // @N restricts to device index N,
    // and #... restricts to a 16-byte UUID (32 hex chars, no dashes).
    if let Some(rest) = t.strip_prefix("PadCode[")
        && let Some(end) = rest.find(']')
    {
        let code_str = &rest[..end];
        let mut tail = &rest[end + 1..];

        let code_u32 = if let Some(hex) = code_str
            .strip_prefix("0x")
            .or_else(|| code_str.strip_prefix("0X"))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            u32::from_str(code_str).ok()?
        };

        let mut device: Option<usize> = None;
        let mut uuid: Option<[u8; 16]> = None;

        // Parse optional @device and #uuid, in any order.
        loop {
            if let Some(rest2) = tail.strip_prefix('@') {
                let mut digits = String::new();
                for ch in rest2.chars() {
                    if ch.is_ascii_digit() {
                        digits.push(ch);
                    } else {
                        break;
                    }
                }
                if digits.is_empty() {
                    break;
                }
                if let Ok(dev_idx) = usize::from_str(&digits) {
                    device = Some(dev_idx);
                }
                tail = &rest2[digits.len()..];
                continue;
            }

            if let Some(rest2) = tail.strip_prefix('#') {
                let mut hex_digits = String::new();
                for ch in rest2.chars() {
                    if ch.is_ascii_hexdigit() {
                        hex_digits.push(ch);
                    } else {
                        break;
                    }
                }
                if hex_digits.len() == 32 {
                    let mut bytes = [0u8; 16];
                    let mut ok = true;
                    for i in 0..16 {
                        let start = i * 2;
                        let end = start + 2;
                        if let Ok(b) = u8::from_str_radix(&hex_digits[start..end], 16) {
                            bytes[i] = b
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        uuid = Some(bytes);
                    }
                }
                tail = &rest2[hex_digits.len()..];
                continue;
            }

            break;
        }

        return Some(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32,
            device,
            uuid,
        }));
    }

    // Gamepad (any pad): PadDir::Up
    if let Some(rest) = t.strip_prefix("PadDir::") {
        let dir = match rest {
            "Up" => PadDir::Up,
            "Down" => PadDir::Down,
            "Left" => PadDir::Left,
            "Right" => PadDir::Right,
            _ => return None,
        };
        return Some(InputBinding::PadDir(dir));
    }

    // Gamepad (device-specific): Pad0::Dir::Up
    // Split by "::"
    let parts: Vec<&str> = t.split("::").collect();
    if parts.len() == 3 {
        let (pad_part, kind, name) = (parts[0], parts[1], parts[2]);
        // Parse device index from PadN
        if let Some(dev_str) = pad_part.strip_prefix("Pad") {
            if dev_str.is_empty() {
                // Treat as any-pad; handled at top via PadDir prefix.
                return match kind {
                    "Dir" => match name {
                        "Up" => Some(InputBinding::PadDir(PadDir::Up)),
                        "Down" => Some(InputBinding::PadDir(PadDir::Down)),
                        "Left" => Some(InputBinding::PadDir(PadDir::Left)),
                        "Right" => Some(InputBinding::PadDir(PadDir::Right)),
                        _ => None,
                    },
                    _ => None,
                };
            }
            if let Ok(device) = dev_str.parse::<usize>() {
                return match kind {
                    "Dir" => match name {
                        "Up" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Up,
                        }),
                        "Down" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Down,
                        }),
                        "Left" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Left,
                        }),
                        "Right" => Some(InputBinding::PadDirOn {
                            device,
                            dir: PadDir::Right,
                        }),
                        _ => None,
                    },
                    _ => None,
                };
            }
        }
    }

    None
}

fn load_keymap_from_ini_local(conf: &SimpleIni) -> Keymap {
    // When [Keymaps] is present, start from explicit user entries and then fill
    // in any completely missing actions from built-in defaults. When the whole
    // section is absent, fall back to defaults entirely.
    if let Some(section) = conf
        .get_section("Keymaps")
        .or_else(|| conf.get_section("keymaps"))
    {
        let mut km = Keymap::default();
        let mut seen: Vec<VirtualAction> = Vec::new();

        for (k, v) in section {
            let key = k.to_ascii_lowercase();
            if let Some(action) = parse_action_key_lower(&key) {
                let mut bindings = Vec::new();
                for tok in v.split(',') {
                    if let Some(b) = parse_binding_token(tok) {
                        bindings.push(b);
                    }
                }
                km.bind(action, &bindings);
                seen.push(action);
            }
        }

        let defaults = default_keymap_local();
        for act in ALL_VIRTUAL_ACTIONS {
            if !seen.contains(&act) {
                let mut bindings = Vec::new();
                let mut i = 0;
                while let Some(b) = defaults.binding_at(act, i) {
                    bindings.push(b);
                    i += 1;
                }
                if !bindings.is_empty() {
                    km.bind(act, &bindings);
                }
            }
        }
        if km.binding_at(VirtualAction::p1_select, 0).is_none() {
            km.bind(
                VirtualAction::p1_select,
                &[InputBinding::Key(KeyCode::Slash)],
            );
        }
        if km.binding_at(VirtualAction::p2_select, 0).is_none() {
            km.bind(
                VirtualAction::p2_select,
                &[InputBinding::Key(KeyCode::NumpadDecimal)],
            );
        }

        km
    } else {
        default_keymap_local()
    }
}

/// Update a keyboard binding in Primary/Secondary slots, ensuring that the
/// given key code is not used in any other Primary/Secondary slot for P1/P2.
/// Default slots (index 0) are never modified.
pub fn update_keymap_binding_unique_keyboard(
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) {
    // Update keyboard bindings while ensuring that `keycode` is unique across
    // all Primary/Secondary slots (index >= 1) for P1/P2.
    let current = crate::core::input::get_keymap();
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings: Vec<InputBinding> = Vec::new();
        let mut i = 0;
        while let Some(b) = current.binding_at(act, i) {
            bindings.push(b);
            i += 1;
        }

        // Remove this key from all Primary/Secondary slots (index >= 1).
        if !bindings.is_empty() {
            let mut filtered: Vec<InputBinding> = Vec::with_capacity(bindings.len());
            for (slot_idx, b) in bindings.iter().enumerate() {
                if slot_idx >= 1
                    && let InputBinding::Key(code) = b
                    && *code == keycode
                {
                    continue;
                }
                filtered.push(*b);
            }
            bindings = filtered;
        }

        if act == action {
            // If Secondary requested but there is no Primary yet, collapse this
            // to the first non-default slot so we don't implicitly duplicate
            // defaults into Primary.
            let slot_count_before = bindings.len();
            let mut effective_index = index;
            if index >= 2 && slot_count_before <= 1 {
                effective_index = 1;
            }

            let new_binding = InputBinding::Key(keycode);
            if effective_index == 0 {
                if bindings.is_empty() {
                    bindings.push(new_binding);
                } else {
                    bindings[0] = new_binding;
                }
            } else if bindings.len() <= effective_index {
                if bindings.is_empty() {
                    bindings.push(new_binding);
                } else {
                    bindings.push(new_binding);
                }
            } else {
                bindings[effective_index] = new_binding;
            }
        }

        new_map.bind(act, &bindings);
    }

    crate::core::input::set_keymap(new_map);
    save_without_keymaps();
}

/// Update a gamepad binding in Primary/Secondary slots, ensuring that the
/// given physical binding is not used in any other Primary/Secondary slot
/// for P1/P2. Default slots (index 0) are never modified.
pub fn update_keymap_binding_unique_gamepad(
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) {
    let current = crate::core::input::get_keymap();
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings: Vec<InputBinding> = Vec::new();
        let mut i = 0;
        while let Some(b) = current.binding_at(act, i) {
            bindings.push(b);
            i += 1;
        }

        // Remove this binding from all Primary/Secondary slots (index >= 1).
        if !bindings.is_empty() {
            let mut filtered: Vec<InputBinding> = Vec::with_capacity(bindings.len());
            for (slot_idx, b) in bindings.iter().enumerate() {
                if slot_idx >= 1 && *b == binding {
                    continue;
                }
                filtered.push(*b);
            }
            bindings = filtered;
        }

        if act == action {
            // If Secondary requested but there is no Primary yet, collapse this
            // to the first non-default slot so we don't implicitly duplicate
            // defaults into Primary.
            let slot_count_before = bindings.len();
            let mut effective_index = index;
            if index >= 2 && slot_count_before <= 1 {
                effective_index = 1;
            }

            if effective_index == 0 {
                if bindings.is_empty() {
                    bindings.push(binding);
                } else {
                    bindings[0] = binding;
                }
            } else if bindings.len() <= effective_index {
                if bindings.is_empty() {
                    bindings.push(binding);
                } else {
                    bindings.push(binding);
                }
            } else {
                bindings[effective_index] = binding;
            }
        }

        new_map.bind(act, &bindings);
    }

    crate::core::input::set_keymap(new_map);
    save_without_keymaps();
}

fn save_without_keymaps() {
    // Manual writer that keeps [Options]/[Theme] sorted and emits a stable,
    // CamelCase [Keymaps] section derived from the current in-memory keymap.
    let cfg = *CONFIG.lock().unwrap();
    let keymap = crate::core::input::get_keymap();
    let machine_default_noteskin = MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone();

    let mut content = String::new();

    // [Options] (alphabetical order)
    content.push_str("[Options]\n");
    let audio_rate_str = match cfg.audio_sample_rate_hz {
        None => "Auto".to_string(),
        Some(hz) => hz.to_string(),
    };
    content.push_str(&format!("AudioSampleRateHz={audio_rate_str}\n"));
    content.push_str(&format!(
        "AutoPopulateGrooveStatsScores={}\n",
        if cfg.auto_populate_gs_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "BGBrightness={}\n",
        cfg.bg_brightness.clamp(0.0, 1.0)
    ));
    content.push_str(&format!(
        "BannerCache={}\n",
        if cfg.banner_cache { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "BannerCacheColorDepth={}\n",
        cfg.banner_cache_color_depth
    ));
    content.push_str(&format!(
        "BannerCacheMinDimension={}\n",
        cfg.banner_cache_min_dimension
    ));
    content.push_str(&format!(
        "BannerCachePow2={}\n",
        if cfg.banner_cache_pow2 { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "BannerCacheScaleDivisor={}\n",
        cfg.banner_cache_scale_divisor
    ));
    content.push_str(&format!(
        "CacheSongs={}\n",
        if cfg.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Center1Player={}\n",
        if cfg.center_1player_notefield {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseAutosubmitScoresIndividually={}\n",
        if cfg.autosubmit_course_scores_individually {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowIndividualScores={}\n",
        if cfg.show_course_individual_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowMostPlayed={}\n",
        if cfg.show_most_played_courses {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowRandom={}\n",
        if cfg.show_random_courses { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "DefaultFailType={}\n",
        cfg.default_fail_type.as_str()
    ));
    content.push_str(&format!("DefaultNoteSkin={machine_default_noteskin}\n"));
    content.push_str(&format!("DisplayHeight={}\n", cfg.display_height));
    content.push_str(&format!("DisplayWidth={}\n", cfg.display_width));
    content.push_str(&format!(
        "EnableArrowCloud={}\n",
        if cfg.enable_arrowcloud { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableBoogieStats={}\n",
        if cfg.enable_boogiestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableGrooveStats={}\n",
        if cfg.enable_groovestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FastLoad={}\n",
        if cfg.fastload { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FullscreenType={}\n",
        cfg.fullscreen_type.as_str()
    ));
    content.push_str(&format!("Game={}\n", cfg.game_flag.as_str()));
    content.push_str(&format!("GamepadBackend={}\n", cfg.windows_gamepad_backend));
    content.push_str(&format!(
        "GfxDebug={}\n",
        if cfg.gfx_debug { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "GlobalOffsetSeconds={}\n",
        cfg.global_offset_seconds
    ));
    content.push_str(&format!("Language={}\n", cfg.language_flag.as_str()));
    content.push_str(&format!("LogLevel={}\n", cfg.log_level.as_str()));
    content.push_str(&format!("MaxFps={}\n", cfg.max_fps));
    content.push_str(&format!(
        "VisualDelaySeconds={}\n",
        cfg.visual_delay_seconds
    ));
    content.push_str(&format!("MasterVolume={}\n", cfg.master_volume));
    content.push_str(&format!(
        "MenuMusic={}\n",
        if cfg.menu_music { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MineHitSound={}\n",
        if cfg.mine_hit_sound { "1" } else { "0" }
    ));
    content.push_str(&format!("MusicVolume={}\n", cfg.music_volume));
    content.push_str(&format!(
        "MusicWheelSwitchSpeed={}\n",
        cfg.music_wheel_switch_speed.max(1)
    ));
    content.push_str(&format!(
        "RateModPreservesPitch={}\n",
        if cfg.rate_mod_preserves_pitch {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicBreakdown={}\n",
        cfg.select_music_breakdown_style.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicShowBanners={}\n",
        if cfg.show_select_music_banners {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowBreakdown={}\n",
        if cfg.show_select_music_breakdown {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowCDTitles={}\n",
        if cfg.show_select_music_cdtitles {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelGrades={}\n",
        if cfg.show_music_wheel_grades {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelLamps={}\n",
        if cfg.show_music_wheel_lamps { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "SelectMusicPreviews={}\n",
        if cfg.show_select_music_previews {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviewLoop={}\n",
        if cfg.select_music_preview_loop {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPatternInfo={}\n",
        cfg.select_music_pattern_info_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScorebox={}\n",
        if cfg.show_select_music_scorebox {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if cfg.show_stats_mode != 0 { "1" } else { "0" }
    ));
    content.push_str(&format!("ShowStatsMode={}\n", cfg.show_stats_mode.min(2)));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if cfg.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!("DisplayMonitor={}\n", cfg.display_monitor));
    content.push_str(&format!(
        "SongParsingThreads={}\n",
        cfg.song_parsing_threads
    ));
    content.push_str(&format!(
        "SoftwareRendererThreads={}\n",
        cfg.software_renderer_threads
    ));
    content.push_str(&format!("Theme={}\n", cfg.theme_flag.as_str()));
    content.push_str(&format!("SFXVolume={}\n", cfg.sfx_volume));
    content.push_str(&format!(
        "TranslatedTitles={}\n",
        if cfg.translated_titles { "1" } else { "0" }
    ));
    content.push_str(&format!("VideoRenderer={}\n", cfg.video_renderer));
    content.push_str(&format!("Vsync={}\n", if cfg.vsync { "1" } else { "0" }));
    content.push_str(&format!(
        "Windowed={}\n",
        if cfg.windowed { "1" } else { "0" }
    ));
    content.push('\n');

    // [Keymaps] – stable order with CamelCase keys.
    content.push_str("[Keymaps]\n");
    for act in ALL_VIRTUAL_ACTIONS {
        let key_name = action_to_ini_key(act);
        let mut tokens: Vec<String> = Vec::new();
        let mut i = 0;
        while let Some(binding) = keymap.binding_at(act, i) {
            tokens.push(binding_to_token(binding));
            i += 1;
        }
        let value = tokens.join(",");
        content.push_str(key_name);
        content.push('=');
        content.push_str(&value);
        content.push('\n');
    }

    // [Theme] – last section
    content.push('\n');
    content.push_str("[Theme]\n");
    content.push_str(&format!(
        "KeyboardFeatures={}\n",
        if cfg.keyboard_features { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachineShowEvalSummary={}\n",
        if cfg.machine_show_eval_summary {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowGameOver={}\n",
        if cfg.machine_show_gameover { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachineShowNameEntry={}\n",
        if cfg.machine_show_name_entry {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectColor={}\n",
        if cfg.machine_show_select_color {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectPlayMode={}\n",
        if cfg.machine_show_select_play_mode {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectProfile={}\n",
        if cfg.machine_show_select_profile {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectStyle={}\n",
        if cfg.machine_show_select_style {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowSelectMusicGameplayTimer={}\n",
        if cfg.show_select_music_gameplay_timer {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("SimplyLoveColor={}\n", cfg.simply_love_color));
    content.push_str(&format!(
        "ZmodRatingBoxText={}\n",
        if cfg.zmod_rating_box_text { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "ShowBpmDecimal={}\n",
        if cfg.show_bpm_decimal { "1" } else { "0" }
    ));
    content.push('\n');

    queue_save_write(content);
}

pub fn get() -> Config {
    *CONFIG.lock().unwrap()
}

pub fn machine_default_noteskin() -> String {
    MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone()
}

pub fn update_display_mode(mode: DisplayMode) {
    let mut dirty = false;
    {
        let mut cfg = CONFIG.lock().unwrap();
        match mode {
            DisplayMode::Windowed => {
                if !cfg.windowed {
                    cfg.windowed = true;
                    dirty = true;
                }
            }
            DisplayMode::Fullscreen(fullscreen_type) => {
                if cfg.windowed {
                    cfg.windowed = false;
                    dirty = true;
                }
                if cfg.fullscreen_type != fullscreen_type {
                    cfg.fullscreen_type = fullscreen_type;
                    dirty = true;
                }
            }
        }
    }
    if dirty {
        save_without_keymaps();
    }
}

pub fn update_display_resolution(width: u32, height: u32) {
    let mut dirty = false;
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.display_width != width {
            cfg.display_width = width;
            dirty = true;
        }
        if cfg.display_height != height {
            cfg.display_height = height;
            dirty = true;
        }
    }
    if dirty {
        save_without_keymaps();
    }
}

pub fn update_display_monitor(monitor: usize) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.display_monitor == monitor {
            return;
        }
        cfg.display_monitor = monitor;
    }
    save_without_keymaps();
}

pub fn update_video_renderer(renderer: BackendType) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.video_renderer == renderer {
            return;
        }
        cfg.video_renderer = renderer;
    }
    save_without_keymaps();
}

pub fn update_gfx_debug(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.gfx_debug == enabled {
            return;
        }
        cfg.gfx_debug = enabled;
    }
    save_without_keymaps();
}

pub fn update_simply_love_color(index: i32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        // No change, no need to write to disk.
        if cfg.simply_love_color == index {
            return;
        }
        cfg.simply_love_color = index;
    }
    save_without_keymaps();
}

#[allow(dead_code)]
pub fn update_global_offset(offset: f32) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if (cfg.global_offset_seconds - offset).abs() < f32::EPSILON {
            return;
        }
        cfg.global_offset_seconds = offset;
    }
    save_without_keymaps();
}

#[allow(dead_code)]
pub fn update_visual_delay_seconds(delay: f32) {
    let clamped = delay.clamp(-1.0, 1.0);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if (cfg.visual_delay_seconds - clamped).abs() < f32::EPSILON {
            return;
        }
        cfg.visual_delay_seconds = clamped;
    }
    save_without_keymaps();
}

pub fn update_vsync(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.vsync == enabled {
            return;
        }
        cfg.vsync = enabled;
    }
    save_without_keymaps();
}

pub fn update_max_fps(max_fps: u16) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.max_fps == max_fps {
            return;
        }
        cfg.max_fps = max_fps;
    }
    save_without_keymaps();
}

pub fn update_show_stats_mode(mode: u8) {
    let mode = mode.min(2);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_stats_mode == mode {
            return;
        }
        cfg.show_stats_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_log_level(level: LogLevel) {
    log::set_max_level(level.as_level_filter());
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.log_level == level {
            return;
        }
        cfg.log_level = level;
    }
    save_without_keymaps();
}

#[cfg(target_os = "windows")]
pub fn update_windows_gamepad_backend(backend: WindowsPadBackend) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.windows_gamepad_backend == backend {
            return;
        }
        cfg.windows_gamepad_backend = backend;
    }
    save_without_keymaps();
}

pub fn update_bg_brightness(brightness: f32) {
    let clamped = brightness.clamp(0.0, 1.0);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if (cfg.bg_brightness - clamped).abs() < f32::EPSILON {
            return;
        }
        cfg.bg_brightness = clamped;
    }
    save_without_keymaps();
}

pub fn update_center_1player_notefield(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.center_1player_notefield == enabled {
            return;
        }
        cfg.center_1player_notefield = enabled;
    }
    save_without_keymaps();
}

pub fn update_banner_cache(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.banner_cache == enabled {
            return;
        }
        cfg.banner_cache = enabled;
    }
    save_without_keymaps();
}

pub fn update_banner_cache_color_depth(bits: u8) {
    let bits = normalize_banner_cache_color_depth(bits);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.banner_cache_color_depth == bits {
            return;
        }
        cfg.banner_cache_color_depth = bits;
    }
    save_without_keymaps();
}

pub fn update_banner_cache_min_dimension(px: u16) {
    let px = px.clamp(1, 2048);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.banner_cache_min_dimension == px {
            return;
        }
        cfg.banner_cache_min_dimension = px;
    }
    save_without_keymaps();
}

pub fn update_banner_cache_pow2(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.banner_cache_pow2 == enabled {
            return;
        }
        cfg.banner_cache_pow2 = enabled;
    }
    save_without_keymaps();
}

pub fn update_banner_cache_scale_divisor(divisor: u8) {
    let divisor = divisor.clamp(1, 8);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.banner_cache_scale_divisor == divisor {
            return;
        }
        cfg.banner_cache_scale_divisor = divisor;
    }
    save_without_keymaps();
}

pub fn update_song_parsing_threads(threads: u8) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.song_parsing_threads == threads {
            return;
        }
        cfg.song_parsing_threads = threads;
    }
    save_without_keymaps();
}

pub fn update_cache_songs(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.cachesongs == enabled {
            return;
        }
        cfg.cachesongs = enabled;
    }
    save_without_keymaps();
}

pub fn update_fastload(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.fastload == enabled {
            return;
        }
        cfg.fastload = enabled;
    }
    save_without_keymaps();
}

pub fn update_master_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.master_volume == vol {
            return;
        }
        cfg.master_volume = vol;
    }
    save_without_keymaps();
}

pub fn update_music_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.music_volume == vol {
            return;
        }
        cfg.music_volume = vol;
    }
    save_without_keymaps();
}

pub fn update_menu_music(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.menu_music == enabled {
            return;
        }
        cfg.menu_music = enabled;
    }
    save_without_keymaps();
}

pub fn update_software_renderer_threads(threads: u8) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.software_renderer_threads == threads {
            return;
        }
        cfg.software_renderer_threads = threads;
    }
    save_without_keymaps();
}

pub fn update_sfx_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.sfx_volume == vol {
            return;
        }
        cfg.sfx_volume = vol;
    }
    save_without_keymaps();
}

pub fn update_audio_sample_rate(rate: Option<u32>) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.audio_sample_rate_hz == rate {
            return;
        }
        cfg.audio_sample_rate_hz = rate;
    }
    save_without_keymaps();
}

pub fn update_mine_hit_sound(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.mine_hit_sound == enabled {
            return;
        }
        cfg.mine_hit_sound = enabled;
    }
    save_without_keymaps();
}

pub fn update_music_wheel_switch_speed(speed: u8) {
    let speed = speed.max(1);
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.music_wheel_switch_speed == speed {
            return;
        }
        cfg.music_wheel_switch_speed = speed;
    }
    save_without_keymaps();
}

pub fn update_translated_titles(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.translated_titles == enabled {
            return;
        }
        cfg.translated_titles = enabled;
    }
    save_without_keymaps();
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.rate_mod_preserves_pitch == enabled {
            return;
        }
        cfg.rate_mod_preserves_pitch = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_breakdown_style(style: BreakdownStyle) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.select_music_breakdown_style == style {
            return;
        }
        cfg.select_music_breakdown_style = style;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_breakdown(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_select_music_breakdown == enabled {
            return;
        }
        cfg.show_select_music_breakdown = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_banners(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_select_music_banners == enabled {
            return;
        }
        cfg.show_select_music_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_cdtitles(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_select_music_cdtitles == enabled {
            return;
        }
        cfg.show_select_music_cdtitles = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_grades(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_music_wheel_grades == enabled {
            return;
        }
        cfg.show_music_wheel_grades = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_lamps(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_music_wheel_lamps == enabled {
            return;
        }
        cfg.show_music_wheel_lamps = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_previews(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_select_music_previews == enabled {
            return;
        }
        cfg.show_select_music_previews = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_preview_loop(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.select_music_preview_loop == enabled {
            return;
        }
        cfg.select_music_preview_loop = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_pattern_info_mode(mode: SelectMusicPatternInfoMode) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.select_music_pattern_info_mode == mode {
            return;
        }
        cfg.select_music_pattern_info_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_gameplay_timer(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_select_music_gameplay_timer == enabled {
            return;
        }
        cfg.show_select_music_gameplay_timer = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_scorebox(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_select_music_scorebox == enabled {
            return;
        }
        cfg.show_select_music_scorebox = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_random_courses(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_random_courses == enabled {
            return;
        }
        cfg.show_random_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_most_played_courses(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_most_played_courses == enabled {
            return;
        }
        cfg.show_most_played_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_course_individual_scores(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_course_individual_scores == enabled {
            return;
        }
        cfg.show_course_individual_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_autosubmit_course_scores_individually(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.autosubmit_course_scores_individually == enabled {
            return;
        }
        cfg.autosubmit_course_scores_individually = enabled;
    }
    save_without_keymaps();
}

pub fn update_zmod_rating_box_text(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.zmod_rating_box_text == enabled {
            return;
        }
        cfg.zmod_rating_box_text = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_bpm_decimal(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.show_bpm_decimal == enabled {
            return;
        }
        cfg.show_bpm_decimal = enabled;
    }
    save_without_keymaps();
}

pub fn update_default_fail_type(fail_type: DefaultFailType) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.default_fail_type == fail_type {
            return;
        }
        cfg.default_fail_type = fail_type;
    }
    save_without_keymaps();
}

pub fn update_keyboard_features(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.keyboard_features == enabled {
            return;
        }
        cfg.keyboard_features = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_profile(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_select_profile == enabled {
            return;
        }
        cfg.machine_show_select_profile = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_color(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_select_color == enabled {
            return;
        }
        cfg.machine_show_select_color = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_style(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_select_style == enabled {
            return;
        }
        cfg.machine_show_select_style = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_play_mode(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_select_play_mode == enabled {
            return;
        }
        cfg.machine_show_select_play_mode = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_eval_summary(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_eval_summary == enabled {
            return;
        }
        cfg.machine_show_eval_summary = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_name_entry(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_name_entry == enabled {
            return;
        }
        cfg.machine_show_name_entry = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_gameover(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.machine_show_gameover == enabled {
            return;
        }
        cfg.machine_show_gameover = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_groovestats(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.enable_groovestats == enabled {
            return;
        }
        cfg.enable_groovestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_boogiestats(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.enable_boogiestats == enabled {
            return;
        }
        cfg.enable_boogiestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_arrowcloud(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.enable_arrowcloud == enabled {
            return;
        }
        cfg.enable_arrowcloud = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_populate_gs_scores(enabled: bool) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.auto_populate_gs_scores == enabled {
            return;
        }
        cfg.auto_populate_gs_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_game_flag(flag: GameFlag) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.game_flag == flag {
            return;
        }
        cfg.game_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_theme_flag(flag: ThemeFlag) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.theme_flag == flag {
            return;
        }
        cfg.theme_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_language_flag(flag: LanguageFlag) {
    {
        let mut cfg = CONFIG.lock().unwrap();
        if cfg.language_flag == flag {
            return;
        }
        cfg.language_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_machine_default_noteskin(noteskin: &str) {
    let normalized = normalize_machine_default_noteskin(noteskin);
    {
        let mut current = MACHINE_DEFAULT_NOTESKIN.lock().unwrap();
        if *current == normalized {
            return;
        }
        *current = normalized;
    }
    save_without_keymaps();
}
