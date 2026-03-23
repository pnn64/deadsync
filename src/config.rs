use crate::core::gfx::{BackendType, PresentModePolicy};
use crate::core::input::{
    GamepadCodeBinding, InputBinding, Keymap, PadDir, VirtualAction, WindowsPadBackend,
};
use crate::core::logging;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
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
pub enum SelectMusicItlWheelMode {
    Off,
    Score,
    PointsAndScore,
}

impl SelectMusicItlWheelMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Score => "Score",
            Self::PointsAndScore => "PointsAndScore",
        }
    }
}

impl FromStr for SelectMusicItlWheelMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "disable" | "disabled" => Ok(Self::Off),
            "score" | "scores" => Ok(Self::Score),
            "pointsandscore" | "pointsscore" | "points" => Ok(Self::PointsAndScore),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewPackMode {
    Disabled,
    OpenPack,
    HasScore,
}

impl NewPackMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::OpenPack => "OpenPack",
            Self::HasScore => "HasScore",
        }
    }
}

impl FromStr for NewPackMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "disabled" | "disable" | "off" => Ok(Self::Disabled),
            "openpack" | "open" => Ok(Self::OpenPack),
            "hasscore" | "score" | "scored" => Ok(Self::HasScore),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMusicScoreboxPlacement {
    Auto,
    StepPane,
}

impl SelectMusicScoreboxPlacement {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::StepPane => "StepPane",
        }
    }
}

impl FromStr for SelectMusicScoreboxPlacement {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "auto" => Ok(Self::Auto),
            "steppane" | "pane" => Ok(Self::StepPane),
            _ => Err(()),
        }
    }
}

pub const AUTO_SS_NUM_FLAGS: usize = 5;
pub const AUTO_SS_FLAG_NAMES: [&str; AUTO_SS_NUM_FLAGS] =
    ["PBs", "Fails", "Clears", "Quads", "Quints"];
pub const AUTO_SS_PBS: u8 = 1 << 0;
pub const AUTO_SS_FAILS: u8 = 1 << 1;
pub const AUTO_SS_CLEARS: u8 = 1 << 2;
pub const AUTO_SS_QUADS: u8 = 1 << 3;
pub const AUTO_SS_QUINTS: u8 = 1 << 4;

#[inline(always)]
pub const fn auto_screenshot_bit(idx: usize) -> u8 {
    match idx {
        0 => AUTO_SS_PBS,
        1 => AUTO_SS_FAILS,
        2 => AUTO_SS_CLEARS,
        3 => AUTO_SS_QUADS,
        4 => AUTO_SS_QUINTS,
        _ => 0,
    }
}

pub fn auto_screenshot_mask_to_str(mask: u8) -> String {
    if mask == 0 {
        return "Off".to_string();
    }
    let mut parts = Vec::with_capacity(AUTO_SS_NUM_FLAGS);
    for (idx, name) in AUTO_SS_FLAG_NAMES.iter().enumerate() {
        if (mask & auto_screenshot_bit(idx)) != 0 {
            parts.push(*name);
        }
    }
    parts.join("|")
}

pub fn auto_screenshot_mask_from_str(s: &str) -> u8 {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("off") {
        return 0;
    }
    let mut mask = 0u8;
    for part in trimmed.split('|') {
        match part.trim().to_ascii_lowercase().as_str() {
            "pbs" => mask |= AUTO_SS_PBS,
            "fails" => mask |= AUTO_SS_FAILS,
            "clears" => mask |= AUTO_SS_CLEARS,
            "quads" => mask |= AUTO_SS_QUADS,
            "quints" => mask |= AUTO_SS_QUINTS,
            _ => {}
        }
    }
    mask
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncGraphMode {
    Frequency,
    BeatIndex,
    PostKernelFingerprint,
}

impl SyncGraphMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Frequency => "Frequency",
            Self::BeatIndex => "BeatIndex",
            Self::PostKernelFingerprint => "PostKernelFingerprint",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Frequency => "Frequency",
            Self::BeatIndex => "Beat index",
            Self::PostKernelFingerprint => "Post-kernel fingerprint",
        }
    }
}

impl FromStr for SyncGraphMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "frequency" => Ok(Self::Frequency),
            "beatindex" | "beatdigest" | "digest" => Ok(Self::BeatIndex),
            "postkernelfingerprint" | "postkernel" | "fingerprint" => {
                Ok(Self::PostKernelFingerprint)
            }
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinePreferredPlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl MachinePreferredPlayStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Versus => "Versus",
            Self::Double => "Double",
        }
    }
}

impl FromStr for MachinePreferredPlayStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "single" | "1player" | "oneplayer" => Ok(Self::Single),
            "versus" | "2player" | "2players" | "twoplayer" | "twoplayers" => Ok(Self::Versus),
            "double" => Ok(Self::Double),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinePreferredPlayMode {
    #[default]
    Regular,
    Marathon,
}

impl MachinePreferredPlayMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "Regular",
            Self::Marathon => "Marathon",
        }
    }
}

impl FromStr for MachinePreferredPlayMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "regular" => Ok(Self::Regular),
            "marathon" => Ok(Self::Marathon),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutputMode {
    Auto,
    Shared,
    Exclusive,
}

impl AudioOutputMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Shared => "Shared",
            Self::Exclusive => "Exclusive",
        }
    }
}

impl FromStr for AudioOutputMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "shared" => Ok(Self::Shared),
            "exclusive" => Ok(Self::Exclusive),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxAudioBackend {
    Auto,
    PipeWire,
    PulseAudio,
    Jack,
    Alsa,
}

impl LinuxAudioBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::PipeWire => "PipeWire",
            Self::PulseAudio => "PulseAudio",
            Self::Jack => "JACK",
            Self::Alsa => "ALSA",
        }
    }
}

impl FromStr for LinuxAudioBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "pipewire" | "pipe-wire" | "pw" => Ok(Self::PipeWire),
            "pulseaudio" | "pulse" => Ok(Self::PulseAudio),
            "jack" => Ok(Self::Jack),
            "alsa" => Ok(Self::Alsa),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub vsync: bool,
    /// Stored MaxFPS cap value. `0` means "off".
    pub max_fps: u16,
    pub present_mode_policy: PresentModePolicy,
    pub windowed: bool,
    pub fullscreen_type: FullscreenType,
    pub display_monitor: usize,
    pub game_flag: GameFlag,
    pub theme_flag: ThemeFlag,
    pub language_flag: LanguageFlag,
    pub log_level: LogLevel,
    pub log_to_file: bool,
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
    /// Cache Select Music CDTitles as raw RGBA blobs on disk.
    pub cdtitle_cache: bool,
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
    pub show_select_music_video_banners: bool,
    pub show_select_music_breakdown: bool,
    pub show_select_music_cdtitles: bool,
    pub show_music_wheel_grades: bool,
    pub show_music_wheel_lamps: bool,
    pub select_music_itl_wheel_mode: SelectMusicItlWheelMode,
    pub select_music_new_pack_mode: NewPackMode,
    pub show_select_music_previews: bool,
    pub show_select_music_preview_marker: bool,
    pub select_music_preview_loop: bool,
    /// zmod parity: enable keyboard-only shortcuts like Ctrl+R restart.
    pub keyboard_features: bool,
    /// Enable or disable animated gameplay background videos.
    pub show_video_backgrounds: bool,
    /// Startup flow: show Select Profile before continuing.
    pub machine_show_select_profile: bool,
    /// Startup flow: show Select Color before continuing.
    pub machine_show_select_color: bool,
    /// Startup flow: show Select Style before continuing.
    pub machine_show_select_style: bool,
    /// Startup flow: show Select Play Mode before continuing.
    pub machine_show_select_play_mode: bool,
    /// Startup flow fallback style used when Select Style is disabled.
    pub machine_preferred_style: MachinePreferredPlayStyle,
    /// Startup flow fallback mode used when Select Play Mode is disabled.
    pub machine_preferred_play_mode: MachinePreferredPlayMode,
    /// Machine-wide replay recording and replay menu visibility.
    pub machine_enable_replays: bool,
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
    /// Choose which null-or-die sync graph the Select Music overlay displays.
    pub null_or_die_sync_graph: SyncGraphMode,
    pub select_music_breakdown_style: BreakdownStyle,
    pub select_music_pattern_info_mode: SelectMusicPatternInfoMode,
    pub show_select_music_scorebox: bool,
    pub select_music_scorebox_placement: SelectMusicScoreboxPlacement,
    pub select_music_scorebox_cycle_itg: bool,
    pub select_music_scorebox_cycle_ex: bool,
    pub select_music_scorebox_cycle_hard_ex: bool,
    pub select_music_scorebox_cycle_tournaments: bool,
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
    pub assist_tick_volume: u8,
    pub sfx_volume: u8,
    // None = auto (use the backend default output route); Some(N) = startup output-device index.
    pub audio_output_device_index: Option<u16>,
    pub audio_output_mode: AudioOutputMode,
    pub linux_audio_backend: LinuxAudioBackend,
    // None = auto (use device default sample rate)
    pub audio_sample_rate_hz: Option<u32>,
    pub auto_download_unlocks: bool,
    pub auto_populate_gs_scores: bool,
    pub rate_mod_preserves_pitch: bool,
    pub enable_arrowcloud: bool,
    pub enable_boogiestats: bool,
    pub enable_groovestats: bool,
    pub separate_unlocks_by_player: bool,
    pub fastload: bool,
    pub cachesongs: bool,
    // Whether to apply Gaussian smoothing to the eval histogram (Simply Love style)
    pub smooth_histogram: bool,
    /// Conditions for auto-screenshotting the Evaluation screen.
    pub auto_screenshot_eval: u8,
    /// ITGmania InputFilter parity: per-input debounce window in seconds.
    pub input_debounce_seconds: f32,
    /// When true, gameplay arrow buttons (p*_up/down/left/right) are excluded from
    /// menu navigation. Only explicitly-bound menu buttons (p*_menu_*) work in menus.
    pub only_dedicated_menu_buttons: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vsync: false,
            max_fps: 0,
            present_mode_policy: PresentModePolicy::Immediate,
            windowed: true,
            fullscreen_type: FullscreenType::Exclusive,
            display_monitor: 0,
            game_flag: GameFlag::Dance,
            theme_flag: ThemeFlag::SimplyLove,
            language_flag: LanguageFlag::English,
            log_level: LogLevel::Warn,
            log_to_file: true,
            show_stats_mode: 0,
            translated_titles: false,
            mine_hit_sound: true,
            bg_brightness: 0.7,
            center_1player_notefield: false,
            banner_cache: true,
            cdtitle_cache: true,
            display_width: 1600,
            display_height: 900,
            video_renderer: BackendType::OpenGL,
            gfx_debug: false,
            windows_gamepad_backend: WindowsPadBackend::RawInput,
            software_renderer_threads: 1,
            song_parsing_threads: 0,
            simply_love_color: 2, // Corresponds to DEFAULT_COLOR_INDEX
            show_select_music_gameplay_timer: true,
            show_select_music_banners: true,
            show_select_music_video_banners: true,
            show_select_music_breakdown: true,
            show_select_music_cdtitles: true,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: true,
            select_music_itl_wheel_mode: SelectMusicItlWheelMode::Score,
            select_music_new_pack_mode: NewPackMode::Disabled,
            show_select_music_previews: true,
            show_select_music_preview_marker: false,
            select_music_preview_loop: true,
            keyboard_features: true,
            show_video_backgrounds: true,
            machine_show_select_profile: true,
            machine_show_select_color: true,
            machine_show_select_style: true,
            machine_show_select_play_mode: true,
            machine_preferred_style: MachinePreferredPlayStyle::Single,
            machine_preferred_play_mode: MachinePreferredPlayMode::Regular,
            machine_enable_replays: true,
            machine_show_eval_summary: true,
            machine_show_name_entry: true,
            machine_show_gameover: true,
            zmod_rating_box_text: false,
            show_bpm_decimal: false,
            default_fail_type: DefaultFailType::ImmediateContinue,
            null_or_die_sync_graph: SyncGraphMode::PostKernelFingerprint,
            select_music_breakdown_style: BreakdownStyle::Sl,
            select_music_pattern_info_mode: SelectMusicPatternInfoMode::Tech,
            show_select_music_scorebox: true,
            select_music_scorebox_placement: SelectMusicScoreboxPlacement::Auto,
            select_music_scorebox_cycle_itg: true,
            select_music_scorebox_cycle_ex: true,
            select_music_scorebox_cycle_hard_ex: true,
            select_music_scorebox_cycle_tournaments: true,
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
            assist_tick_volume: 100,
            sfx_volume: 100,
            audio_output_device_index: None,
            audio_output_mode: AudioOutputMode::Auto,
            linux_audio_backend: LinuxAudioBackend::Auto,
            audio_sample_rate_hz: None,
            auto_download_unlocks: false,
            auto_populate_gs_scores: false,
            rate_mod_preserves_pitch: true,
            enable_arrowcloud: false,
            enable_boogiestats: false,
            enable_groovestats: false,
            separate_unlocks_by_player: false,
            fastload: true,
            cachesongs: true,
            smooth_histogram: true,
            auto_screenshot_eval: 0,
            input_debounce_seconds: 0.02,
            only_dedicated_menu_buttons: false,
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
static LOCK_WAIT_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
static AUDIO_MIX_LEVELS_PACKED: std::sync::LazyLock<AtomicU32> = std::sync::LazyLock::new(|| {
    let cfg = Config::default();
    AtomicU32::new(pack_audio_mix_levels(
        cfg.master_volume,
        cfg.music_volume,
        cfg.sfx_volume,
        cfg.assist_tick_volume,
    ))
});
static MACHINE_DEFAULT_NOTESKIN: std::sync::LazyLock<Mutex<String>> =
    std::sync::LazyLock::new(|| Mutex::new(DEFAULT_MACHINE_NOTESKIN.to_string()));
static ADDITIONAL_SONG_FOLDERS: std::sync::LazyLock<Mutex<String>> =
    std::sync::LazyLock::new(|| Mutex::new(String::new()));
static SAVE_TX: std::sync::LazyLock<Option<mpsc::Sender<SaveReq>>> =
    std::sync::LazyLock::new(start_save_worker);

const LOCK_WAIT_REPORT_INTERVAL_NS: u64 = 5_000_000_000;
const LOCK_WAIT_SLOW_NS: u64 = 50_000;
const LOCK_WAIT_SPIKE_NS: u64 = 2_000_000;

struct LockWaitStats {
    lock_count: AtomicU64,
    wait_ns_total: AtomicU64,
    wait_ns_max: AtomicU64,
    slow_wait_count: AtomicU64,
    last_report_ns: AtomicU64,
}

impl LockWaitStats {
    const fn new() -> Self {
        Self {
            lock_count: AtomicU64::new(0),
            wait_ns_total: AtomicU64::new(0),
            wait_ns_max: AtomicU64::new(0),
            slow_wait_count: AtomicU64::new(0),
            last_report_ns: AtomicU64::new(0),
        }
    }
}

static CONFIG_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();

#[inline(always)]
fn lock_wait_stats_enabled() -> bool {
    log::max_level() >= log::LevelFilter::Debug
}

#[inline(always)]
fn lock_wait_now_ns() -> u64 {
    LOCK_WAIT_EPOCH.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

#[inline(always)]
fn record_lock_wait(lock_name: &str, stats: &LockWaitStats, waited_ns: u64) {
    stats.lock_count.fetch_add(1, Ordering::Relaxed);
    stats.wait_ns_total.fetch_add(waited_ns, Ordering::Relaxed);
    stats.wait_ns_max.fetch_max(waited_ns, Ordering::Relaxed);
    if waited_ns >= LOCK_WAIT_SLOW_NS {
        stats.slow_wait_count.fetch_add(1, Ordering::Relaxed);
    }
    if waited_ns >= LOCK_WAIT_SPIKE_NS {
        debug!(
            "lock-wait[{lock_name}] spike={:.3}ms",
            waited_ns as f64 / 1_000_000.0
        );
    }
    let now_ns = lock_wait_now_ns();
    let last_ns = stats.last_report_ns.load(Ordering::Relaxed);
    if now_ns.saturating_sub(last_ns) < LOCK_WAIT_REPORT_INTERVAL_NS {
        return;
    }
    if stats
        .last_report_ns
        .compare_exchange(last_ns, now_ns, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let lock_count = stats.lock_count.swap(0, Ordering::Relaxed);
    if lock_count == 0 {
        return;
    }
    let total_ns = stats.wait_ns_total.swap(0, Ordering::Relaxed);
    let max_ns = stats.wait_ns_max.swap(0, Ordering::Relaxed);
    let slow_count = stats.slow_wait_count.swap(0, Ordering::Relaxed);
    let avg_us = (total_ns as f64 / lock_count as f64) / 1_000.0;
    debug!(
        "lock-wait[{lock_name}] n={} avg={avg_us:.3}us max={:.3}us slow(>50us)={}",
        lock_count,
        max_ns as f64 / 1_000.0,
        slow_count
    );
}

#[inline(always)]
fn lock_config() -> std::sync::MutexGuard<'static, Config> {
    if !lock_wait_stats_enabled() {
        return CONFIG.lock().unwrap();
    }
    let start = Instant::now();
    let guard = CONFIG.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_lock_wait("CONFIG", &CONFIG_LOCK_WAIT_STATS, waited_ns);
    guard
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMixLevels {
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
}

#[inline(always)]
const fn pack_audio_mix_levels(
    master_volume: u8,
    music_volume: u8,
    sfx_volume: u8,
    assist_tick_volume: u8,
) -> u32 {
    u32::from_le_bytes([master_volume, music_volume, sfx_volume, assist_tick_volume])
}

#[inline(always)]
const fn unpack_audio_mix_levels(packed: u32) -> AudioMixLevels {
    let [master_volume, music_volume, sfx_volume, assist_tick_volume] = packed.to_le_bytes();
    AudioMixLevels {
        master_volume,
        music_volume,
        sfx_volume,
        assist_tick_volume,
    }
}

#[inline(always)]
fn sync_audio_mix_levels_from_config(cfg: &Config) {
    AUDIO_MIX_LEVELS_PACKED.store(
        pack_audio_mix_levels(
            cfg.master_volume,
            cfg.music_volume,
            cfg.sfx_volume,
            cfg.assist_tick_volume,
        ),
        Ordering::Release,
    );
}

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
fn normalize_machine_default_noteskin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_MACHINE_NOTESKIN.to_string();
    }
    trimmed.to_ascii_lowercase()
}

fn normalize_additional_song_folders(raw: &str) -> String {
    let mut out = String::new();
    for path in raw
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(path);
    }
    out
}

fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn load_additional_song_folders(conf: &SimpleIni) -> String {
    let read_only = conf
        .get("Options", "AdditionalSongFoldersReadOnly")
        .unwrap_or_default();
    let writable_raw = conf
        .get("Options", "AdditionalSongFoldersWritable")
        .unwrap_or_default();
    let deprecated = conf
        .get("Options", "AdditionalSongFolders")
        .unwrap_or_default();
    let writable = if writable_raw.trim().is_empty() {
        deprecated
    } else {
        writable_raw
    };

    if read_only.trim().is_empty() {
        return normalize_additional_song_folders(&writable);
    }
    if writable.trim().is_empty() {
        return normalize_additional_song_folders(&read_only);
    }

    let mut combined = String::with_capacity(read_only.len() + writable.len() + 1);
    combined.push_str(&read_only);
    combined.push(',');
    combined.push_str(&writable);
    normalize_additional_song_folders(&combined)
}

pub fn bootstrap_log_to_file() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().log_to_file;
    if conf.load(CONFIG_PATH).is_err() {
        return default;
    }
    conf.get("Options", "LogToFile")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default)
}

fn create_default_config_file() -> Result<(), std::io::Error> {
    info!("'{CONFIG_PATH}' not found, creating with default values.");
    let default = Config::default();

    let mut content = String::new();

    // [Options] section - keys in alphabetical order
    content.push_str("[Options]\n");
    content.push_str("AudioOutputDevice=Auto\n");
    content.push_str("AudioOutputMode=Auto\n");
    content.push_str("AudioSampleRateHz=Auto\n");
    content.push_str("AdditionalSongFolders=\n");
    content.push_str(&format!(
        "AutoDownloadUnlocks={}\n",
        if default.auto_download_unlocks {
            "1"
        } else {
            "0"
        }
    ));
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
        "CacheSongs={}\n",
        if default.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "CDTitleCache={}\n",
        if default.cdtitle_cache { "1" } else { "0" }
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
    content.push_str(&format!(
        "LogToFile={}\n",
        if default.log_to_file { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "LinuxAudioBackend={}\n",
        default.linux_audio_backend.as_str()
    ));
    content.push_str(&format!("MaxFps={}\n", default.max_fps));
    content.push_str(&format!(
        "PresentModePolicy={}\n",
        default.present_mode_policy
    ));
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
        "SelectMusicShowVideoBanners={}\n",
        if default.show_select_music_video_banners {
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
        "SelectMusicNewPackMode={}\n",
        default.select_music_new_pack_mode.as_str()
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
        "SelectMusicPreviewMarker={}\n",
        if default.show_select_music_preview_marker {
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
        "SelectMusicScoreboxPlacement={}\n",
        default.select_music_scorebox_placement.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleItg={}\n",
        if default.select_music_scorebox_cycle_itg {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleEx={}\n",
        if default.select_music_scorebox_cycle_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleHardEx={}\n",
        if default.select_music_scorebox_cycle_hard_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleTournaments={}\n",
        if default.select_music_scorebox_cycle_tournaments {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SeparateUnlocksByPlayer={}\n",
        if default.separate_unlocks_by_player {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "AutoScreenshotEval={}\n",
        auto_screenshot_mask_to_str(default.auto_screenshot_eval)
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
        default.show_stats_mode.min(3)
    ));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if default.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "InputDebounceTime={:.3}\n",
        default.input_debounce_seconds
    ));
    content.push_str(&format!(
        "OnlyDedicatedMenuButtons={}\n",
        if default.only_dedicated_menu_buttons {
            "1"
        } else {
            "0"
        }
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
    content.push_str(&format!(
        "AssistTickVolume={}\n",
        default.assist_tick_volume
    ));
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
        "VideoBackgrounds={}\n",
        if default.show_video_backgrounds {
            "1"
        } else {
            "0"
        }
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
        "MachineEnableReplays={}\n",
        if default.machine_enable_replays {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachinePreferredStyle={}\n",
        default.machine_preferred_style.as_str()
    ));
    content.push_str(&format!(
        "MachinePreferredPlayMode={}\n",
        default.machine_preferred_play_mode.as_str()
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
                *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = load_additional_song_folders(&conf);
            }

            // This block populates the global CONFIG struct from the file,
            // using default values for any missing keys.
            {
                let mut cfg = lock_config();
                let default = Config::default();

                cfg.vsync = conf
                    .get("Options", "Vsync")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.vsync, |v| v != 0);
                cfg.max_fps = conf
                    .get("Options", "MaxFps")
                    .and_then(|v| v.parse::<u16>().ok())
                    .unwrap_or(default.max_fps);
                cfg.present_mode_policy = conf
                    .get("Options", "PresentModePolicy")
                    .and_then(|s| PresentModePolicy::from_str(&s).ok())
                    .or_else(|| {
                        conf.get("Options", "UncappedMode").and_then(|s| {
                            match s.trim().to_ascii_lowercase().as_str() {
                                "balanced" => Some(PresentModePolicy::Mailbox),
                                "unhinged" | "maxfps" | "max_fps" | "max-fps" => {
                                    Some(PresentModePolicy::Immediate)
                                }
                                _ => None,
                            }
                        })
                    })
                    .unwrap_or(default.present_mode_policy);
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
                cfg.auto_download_unlocks = conf
                    .get("Options", "AutoDownloadUnlocks")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.auto_download_unlocks, |v| v != 0);
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
                cfg.separate_unlocks_by_player = conf
                    .get("Options", "SeparateUnlocksByPlayer")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.separate_unlocks_by_player, |v| v != 0);
                cfg.mine_hit_sound = conf
                    .get("Options", "MineHitSound")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.mine_hit_sound, |v| v != 0);
                cfg.show_stats_mode = conf
                    .get("Options", "ShowStatsMode")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map(|v| v.min(3))
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
                cfg.null_or_die_sync_graph = conf
                    .get("Options", "NullOrDieSyncGraph")
                    .and_then(|v| SyncGraphMode::from_str(&v).ok())
                    .unwrap_or(default.null_or_die_sync_graph);
                cfg.banner_cache = conf
                    .get("Options", "BannerCache")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.banner_cache, |v| v != 0);
                cfg.cdtitle_cache = conf
                    .get("Options", "CDTitleCache")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.cdtitle_cache, |v| v != 0);
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
                cfg.log_to_file = conf
                    .get("Options", "LogToFile")
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.log_to_file);
                cfg.linux_audio_backend = conf
                    .get("Options", "LinuxAudioBackend")
                    .and_then(|v| LinuxAudioBackend::from_str(&v).ok())
                    .unwrap_or(default.linux_audio_backend);
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
                cfg.assist_tick_volume = conf
                    .get("Options", "AssistTickVolume")
                    .and_then(|v| v.parse().ok())
                    .map_or(default.assist_tick_volume, |v: u8| v.clamp(0, 100));
                cfg.audio_output_device_index = conf
                    .get("Options", "AudioOutputDevice")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() || v.eq_ignore_ascii_case("auto") {
                            None
                        } else {
                            v.parse::<u16>().ok()
                        }
                    })
                    .or(default.audio_output_device_index);
                cfg.audio_output_mode = conf
                    .get("Options", "AudioOutputMode")
                    .and_then(|s| AudioOutputMode::from_str(&s).ok())
                    .unwrap_or(default.audio_output_mode);
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
                cfg.show_select_music_video_banners = conf
                    .get("Options", "SelectMusicShowVideoBanners")
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.show_select_music_video_banners);
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
                cfg.select_music_itl_wheel_mode = conf
                    .get("Options", "SelectMusicWheelITL")
                    .and_then(|v| SelectMusicItlWheelMode::from_str(&v).ok())
                    .unwrap_or(default.select_music_itl_wheel_mode);
                cfg.select_music_new_pack_mode = conf
                    .get("Options", "SelectMusicNewPackMode")
                    .and_then(|v| NewPackMode::from_str(&v).ok())
                    .unwrap_or(default.select_music_new_pack_mode);
                cfg.show_select_music_previews = conf
                    .get("Options", "SelectMusicPreviews")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_previews, |v| v != 0);
                cfg.show_select_music_preview_marker = conf
                    .get("Options", "SelectMusicPreviewMarker")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.show_select_music_preview_marker, |v| v != 0);
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
                cfg.select_music_scorebox_placement = conf
                    .get("Options", "SelectMusicScoreboxPlacement")
                    .and_then(|v| SelectMusicScoreboxPlacement::from_str(&v).ok())
                    .unwrap_or(default.select_music_scorebox_placement);
                cfg.select_music_scorebox_cycle_itg = conf
                    .get("Options", "SelectMusicScoreboxCycleItg")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.select_music_scorebox_cycle_itg, |v| v != 0);
                cfg.select_music_scorebox_cycle_ex = conf
                    .get("Options", "SelectMusicScoreboxCycleEx")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.select_music_scorebox_cycle_ex, |v| v != 0);
                cfg.select_music_scorebox_cycle_hard_ex = conf
                    .get("Options", "SelectMusicScoreboxCycleHardEx")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.select_music_scorebox_cycle_hard_ex, |v| v != 0);
                cfg.select_music_scorebox_cycle_tournaments = conf
                    .get("Options", "SelectMusicScoreboxCycleTournaments")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.select_music_scorebox_cycle_tournaments, |v| v != 0);
                cfg.auto_screenshot_eval = conf
                    .get("Options", "AutoScreenshotEval")
                    .map(|v| auto_screenshot_mask_from_str(&v))
                    .unwrap_or(default.auto_screenshot_eval);
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
                cfg.input_debounce_seconds = conf
                    .get("Options", "InputDebounceTime")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            return None;
                        }
                        let lower = v.to_ascii_lowercase();
                        if let Some(ms) = lower.strip_suffix("ms") {
                            return ms
                                .trim()
                                .parse::<f32>()
                                .ok()
                                .map(|n| (n / 1000.0).clamp(0.0, 0.2));
                        }
                        v.parse::<f32>().ok().map(|n| {
                            let secs = if n > 1.0 { n / 1000.0 } else { n };
                            secs.clamp(0.0, 0.2)
                        })
                    })
                    .unwrap_or(default.input_debounce_seconds);
                cfg.only_dedicated_menu_buttons = conf
                    .get("Options", "OnlyDedicatedMenuButtons")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map_or(default.only_dedicated_menu_buttons, |v| v != 0);
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
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.keyboard_features);
                cfg.show_video_backgrounds = conf
                    .get("Theme", "VideoBackgrounds")
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.show_video_backgrounds);
                cfg.machine_show_eval_summary = conf
                    .get("Theme", "MachineShowEvalSummary")
                    .and_then(|v| parse_bool_str(&v))
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
                cfg.machine_enable_replays = conf
                    .get("Theme", "MachineEnableReplays")
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
                    .unwrap_or(default.machine_enable_replays);
                cfg.machine_preferred_style = conf
                    .get("Theme", "MachinePreferredStyle")
                    .and_then(|v| MachinePreferredPlayStyle::from_str(&v).ok())
                    .unwrap_or(default.machine_preferred_style);
                cfg.machine_preferred_play_mode = conf
                    .get("Theme", "MachinePreferredPlayMode")
                    .and_then(|v| MachinePreferredPlayMode::from_str(&v).ok())
                    .unwrap_or(default.machine_preferred_play_mode);
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

                sync_audio_mix_levels_from_config(&cfg);
                logging::set_file_logging_enabled(cfg.log_to_file);
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
                    "AudioOutputDevice",
                    "AudioOutputMode",
                    "AudioSampleRateHz",
                    "AdditionalSongFolders",
                    "AutoDownloadUnlocks",
                    "AutoPopulateGrooveStatsScores",
                    "BGBrightness",
                    "BannerCache",
                    "CacheSongs",
                    "CDTitleCache",
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
                    "LogToFile",
                    "LinuxAudioBackend",
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
                    "SelectMusicShowVideoBanners",
                    "SelectMusicShowBreakdown",
                    "SelectMusicShowCDTitles",
                    "SelectMusicWheelGrades",
                    "SelectMusicWheelLamps",
                    "SelectMusicPreviews",
                    "SelectMusicPreviewLoop",
                    "SelectMusicPatternInfo",
                    "SelectMusicScorebox",
                    "SelectMusicScoreboxCycleItg",
                    "SelectMusicScoreboxCycleEx",
                    "SelectMusicScoreboxCycleHardEx",
                    "SelectMusicScoreboxCycleTournaments",
                    "SeparateUnlocksByPlayer",
                    "ShowStats",
                    "ShowStatsMode",
                    "SmoothHistogram",
                    "InputDebounceTime",
                    "OnlyDedicatedMenuButtons",
                    "AssistTickVolume",
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
                if !miss && !has("Theme", "VideoBackgrounds") {
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
                if !miss && !has("Theme", "MachineEnableReplays") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachinePreferredStyle") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachinePreferredPlayMode") {
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
            *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = String::new();
        }
    }
    let mut dedicated = get().only_dedicated_menu_buttons;
    if dedicated && !crate::core::input::any_player_has_dedicated_menu_buttons() {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has dedicated menu buttons mapped — disabling."
        );
        dedicated = false;
        lock_config().only_dedicated_menu_buttons = false;
    }
    crate::core::input::set_only_dedicated_menu_buttons(dedicated);
    crate::core::input::set_input_debounce_seconds(get().input_debounce_seconds);
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
fn parse_keycode(t: &str) -> Option<InputBinding> {
    let name = t.strip_prefix("KeyCode::")?;
    macro_rules! keycode_match {
        ($input:expr, $( $name:ident ),* $(,)?) => {
            match $input {
                $( stringify!($name) => Some(KeyCode::$name), )*
                _ => None,
            }
        };
    }
    keycode_match!(
        name,
        Backquote,
        Backslash,
        BracketLeft,
        BracketRight,
        Comma,
        Digit0,
        Digit1,
        Digit2,
        Digit3,
        Digit4,
        Digit5,
        Digit6,
        Digit7,
        Digit8,
        Digit9,
        Equal,
        IntlBackslash,
        IntlRo,
        IntlYen,
        KeyA,
        KeyB,
        KeyC,
        KeyD,
        KeyE,
        KeyF,
        KeyG,
        KeyH,
        KeyI,
        KeyJ,
        KeyK,
        KeyL,
        KeyM,
        KeyN,
        KeyO,
        KeyP,
        KeyQ,
        KeyR,
        KeyS,
        KeyT,
        KeyU,
        KeyV,
        KeyW,
        KeyX,
        KeyY,
        KeyZ,
        Minus,
        Period,
        Quote,
        Semicolon,
        Slash,
        AltLeft,
        AltRight,
        Backspace,
        CapsLock,
        ContextMenu,
        ControlLeft,
        ControlRight,
        Enter,
        SuperLeft,
        SuperRight,
        ShiftLeft,
        ShiftRight,
        Space,
        Tab,
        Convert,
        KanaMode,
        Lang1,
        Lang2,
        Lang3,
        Lang4,
        Lang5,
        NonConvert,
        Delete,
        End,
        Help,
        Home,
        Insert,
        PageDown,
        PageUp,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        NumLock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadBackspace,
        NumpadClear,
        NumpadClearEntry,
        NumpadComma,
        NumpadDecimal,
        NumpadDivide,
        NumpadEnter,
        NumpadEqual,
        NumpadHash,
        NumpadMemoryAdd,
        NumpadMemoryClear,
        NumpadMemoryRecall,
        NumpadMemoryStore,
        NumpadMemorySubtract,
        NumpadMultiply,
        NumpadParenLeft,
        NumpadParenRight,
        NumpadStar,
        NumpadSubtract,
        Escape,
        Fn,
        FnLock,
        PrintScreen,
        ScrollLock,
        Pause,
        BrowserBack,
        BrowserFavorites,
        BrowserForward,
        BrowserHome,
        BrowserRefresh,
        BrowserSearch,
        BrowserStop,
        Eject,
        LaunchApp1,
        LaunchApp2,
        LaunchMail,
        MediaPlayPause,
        MediaSelect,
        MediaStop,
        MediaTrackNext,
        MediaTrackPrevious,
        Power,
        Sleep,
        AudioVolumeDown,
        AudioVolumeMute,
        AudioVolumeUp,
        WakeUp,
        Meta,
        Hyper,
        Turbo,
        Abort,
        Resume,
        Suspend,
        Again,
        Copy,
        Cut,
        Find,
        Open,
        Paste,
        Props,
        Select,
        Undo,
        Hiragana,
        Katakana,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        F25,
        F26,
        F27,
        F28,
        F29,
        F30,
        F31,
        F32,
        F33,
        F34,
        F35,
    )
    .map(InputBinding::Key)
}

#[inline(always)]
fn parse_pad_dir(name: &str) -> Option<PadDir> {
    match name {
        "Up" => Some(PadDir::Up),
        "Down" => Some(PadDir::Down),
        "Left" => Some(PadDir::Left),
        "Right" => Some(PadDir::Right),
        _ => None,
    }
}

#[inline(always)]
fn parse_pad_code(t: &str) -> Option<InputBinding> {
    let rest = t.strip_prefix("PadCode[")?;
    let end = rest.find(']')?;
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

    let mut device = None;
    let mut uuid = None;
    loop {
        if let Some(rest) = tail.strip_prefix('@') {
            let mut digits = String::new();
            for ch in rest.chars() {
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
            tail = &rest[digits.len()..];
            continue;
        }
        if let Some(rest) = tail.strip_prefix('#') {
            let mut hex_digits = String::new();
            for ch in rest.chars() {
                if ch.is_ascii_hexdigit() {
                    hex_digits.push(ch);
                } else {
                    break;
                }
            }
            if hex_digits.len() == 32 {
                let mut bytes = [0u8; 16];
                let mut ok = true;
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let start = i * 2;
                    let end = start + 2;
                    if let Ok(parsed) = u8::from_str_radix(&hex_digits[start..end], 16) {
                        *byte = parsed;
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    uuid = Some(bytes);
                }
            }
            tail = &rest[hex_digits.len()..];
            continue;
        }
        break;
    }

    Some(InputBinding::GamepadCode(GamepadCodeBinding {
        code_u32,
        device,
        uuid,
    }))
}

#[inline(always)]
fn parse_pad_device_binding(t: &str) -> Option<InputBinding> {
    let mut parts = t.split("::");
    let pad = parts.next()?;
    let kind = parts.next()?;
    let name = parts.next()?;
    if parts.next().is_some() || kind != "Dir" {
        return None;
    }

    let dev = pad.strip_prefix("Pad")?;
    let dir = parse_pad_dir(name)?;
    if dev.is_empty() {
        return Some(InputBinding::PadDir(dir));
    }
    Some(InputBinding::PadDirOn {
        device: dev.parse::<usize>().ok()?,
        dir,
    })
}

#[inline(always)]
fn parse_pad_dir_binding(t: &str) -> Option<InputBinding> {
    t.strip_prefix("PadDir::")
        .and_then(parse_pad_dir)
        .map(InputBinding::PadDir)
        .or_else(|| parse_pad_device_binding(t))
}

#[inline(always)]
fn parse_binding_token(tok: &str) -> Option<InputBinding> {
    let t = tok.trim();
    parse_keycode(t)
        .or_else(|| parse_pad_code(t))
        .or_else(|| parse_pad_dir_binding(t))
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

#[inline(always)]
fn first_editable_binding_slot(bindings: &[InputBinding]) -> usize {
    if matches!(bindings.first(), Some(InputBinding::Key(_))) {
        1
    } else {
        0
    }
}

#[inline(always)]
fn requested_to_actual_binding_slot(requested_index: usize, first_editable: usize) -> usize {
    if first_editable == 0 {
        requested_index.saturating_sub(1)
    } else {
        requested_index
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
        let first_editable = first_editable_binding_slot(&bindings);

        // Remove this key from all editable slots for this action.
        if !bindings.is_empty() {
            let mut filtered: Vec<InputBinding> = Vec::with_capacity(bindings.len());
            for (slot_idx, b) in bindings.iter().enumerate() {
                if slot_idx >= first_editable
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
            let mut effective_index = requested_to_actual_binding_slot(index, first_editable);
            // If Secondary requested but there is no Primary yet, collapse to
            // the first editable slot.
            if effective_index > first_editable && bindings.len() <= first_editable {
                effective_index = first_editable;
            }

            let new_binding = InputBinding::Key(keycode);
            if bindings.len() <= effective_index {
                if bindings.is_empty() {
                    bindings.push(new_binding);
                } else {
                    bindings.push(new_binding);
                }
            } else if effective_index == 0 {
                if bindings.is_empty() {
                    bindings.push(new_binding);
                } else {
                    bindings[0] = new_binding;
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
        let first_editable = first_editable_binding_slot(&bindings);

        // Remove this binding from all editable slots for this action.
        if !bindings.is_empty() {
            let mut filtered: Vec<InputBinding> = Vec::with_capacity(bindings.len());
            for (slot_idx, b) in bindings.iter().enumerate() {
                if slot_idx >= first_editable && *b == binding {
                    continue;
                }
                filtered.push(*b);
            }
            bindings = filtered;
        }

        if act == action {
            let mut effective_index = requested_to_actual_binding_slot(index, first_editable);
            // If Secondary requested but there is no Primary yet, collapse to
            // the first editable slot.
            if effective_index > first_editable && bindings.len() <= first_editable {
                effective_index = first_editable;
            }

            if bindings.len() <= effective_index {
                if bindings.is_empty() {
                    bindings.push(binding);
                } else {
                    bindings.push(binding);
                }
            } else if effective_index == 0 {
                if bindings.is_empty() {
                    bindings.push(binding);
                } else {
                    bindings[0] = binding;
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
    let cfg = *lock_config();
    let keymap = crate::core::input::get_keymap();
    let machine_default_noteskin = MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone();
    let additional_song_folders = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();

    let mut content = String::new();

    // [Options] (alphabetical order)
    content.push_str("[Options]\n");
    let audio_output_device = cfg
        .audio_output_device_index
        .map_or_else(|| "Auto".to_string(), |idx| idx.to_string());
    content.push_str(&format!("AudioOutputDevice={audio_output_device}\n"));
    content.push_str(&format!(
        "AudioOutputMode={}\n",
        cfg.audio_output_mode.as_str()
    ));
    let audio_rate_str = match cfg.audio_sample_rate_hz {
        None => "Auto".to_string(),
        Some(hz) => hz.to_string(),
    };
    content.push_str(&format!("AudioSampleRateHz={audio_rate_str}\n"));
    content.push_str(&format!(
        "AdditionalSongFolders={additional_song_folders}\n"
    ));
    content.push_str(&format!(
        "AutoDownloadUnlocks={}\n",
        if cfg.auto_download_unlocks { "1" } else { "0" }
    ));
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
        "CacheSongs={}\n",
        if cfg.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "CDTitleCache={}\n",
        if cfg.cdtitle_cache { "1" } else { "0" }
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
    content.push_str(&format!(
        "NullOrDieSyncGraph={}\n",
        cfg.null_or_die_sync_graph.as_str()
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
    content.push_str(&format!(
        "LogToFile={}\n",
        if cfg.log_to_file { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "LinuxAudioBackend={}\n",
        cfg.linux_audio_backend.as_str()
    ));
    content.push_str(&format!("MaxFps={}\n", cfg.max_fps));
    content.push_str(&format!("PresentModePolicy={}\n", cfg.present_mode_policy));
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
        "SelectMusicShowVideoBanners={}\n",
        if cfg.show_select_music_video_banners {
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
        "SelectMusicWheelITL={}\n",
        cfg.select_music_itl_wheel_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicNewPackMode={}\n",
        cfg.select_music_new_pack_mode.as_str()
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
        "SelectMusicPreviewMarker={}\n",
        if cfg.show_select_music_preview_marker {
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
        "SelectMusicScoreboxPlacement={}\n",
        cfg.select_music_scorebox_placement.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleItg={}\n",
        if cfg.select_music_scorebox_cycle_itg {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleEx={}\n",
        if cfg.select_music_scorebox_cycle_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleHardEx={}\n",
        if cfg.select_music_scorebox_cycle_hard_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleTournaments={}\n",
        if cfg.select_music_scorebox_cycle_tournaments {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SeparateUnlocksByPlayer={}\n",
        if cfg.separate_unlocks_by_player {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "AutoScreenshotEval={}\n",
        auto_screenshot_mask_to_str(cfg.auto_screenshot_eval)
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if cfg.show_stats_mode != 0 { "1" } else { "0" }
    ));
    content.push_str(&format!("ShowStatsMode={}\n", cfg.show_stats_mode.min(3)));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if cfg.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "InputDebounceTime={:.3}\n",
        cfg.input_debounce_seconds
    ));
    content.push_str(&format!(
        "OnlyDedicatedMenuButtons={}\n",
        if cfg.only_dedicated_menu_buttons {
            "1"
        } else {
            "0"
        }
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
    content.push_str(&format!("AssistTickVolume={}\n", cfg.assist_tick_volume));
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
        "VideoBackgrounds={}\n",
        if cfg.show_video_backgrounds { "1" } else { "0" }
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
        "MachineEnableReplays={}\n",
        if cfg.machine_enable_replays { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachinePreferredStyle={}\n",
        cfg.machine_preferred_style.as_str()
    ));
    content.push_str(&format!(
        "MachinePreferredPlayMode={}\n",
        cfg.machine_preferred_play_mode.as_str()
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
    *lock_config()
}

pub fn audio_mix_levels() -> AudioMixLevels {
    unpack_audio_mix_levels(AUDIO_MIX_LEVELS_PACKED.load(Ordering::Acquire))
}

pub fn machine_default_noteskin() -> String {
    MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone()
}

pub fn additional_song_folders() -> String {
    ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone()
}

pub fn update_display_mode(mode: DisplayMode) {
    let mut dirty = false;
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if cfg.display_monitor == monitor {
            return;
        }
        cfg.display_monitor = monitor;
    }
    save_without_keymaps();
}

pub fn update_video_renderer(renderer: BackendType) {
    {
        let mut cfg = lock_config();
        if cfg.video_renderer == renderer {
            return;
        }
        cfg.video_renderer = renderer;
    }
    save_without_keymaps();
}

pub fn update_gfx_debug(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.gfx_debug == enabled {
            return;
        }
        cfg.gfx_debug = enabled;
    }
    save_without_keymaps();
}

pub fn update_simply_love_color(index: i32) {
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if (cfg.visual_delay_seconds - clamped).abs() < f32::EPSILON {
            return;
        }
        cfg.visual_delay_seconds = clamped;
    }
    save_without_keymaps();
}

pub fn update_vsync(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.vsync == enabled {
            return;
        }
        cfg.vsync = enabled;
    }
    save_without_keymaps();
}

pub fn update_max_fps(max_fps: u16) {
    {
        let mut cfg = lock_config();
        if cfg.max_fps == max_fps {
            return;
        }
        cfg.max_fps = max_fps;
    }
    save_without_keymaps();
}

pub fn update_present_mode_policy(mode: PresentModePolicy) {
    {
        let mut cfg = lock_config();
        if cfg.present_mode_policy == mode {
            return;
        }
        cfg.present_mode_policy = mode;
    }
    save_without_keymaps();
}

pub fn update_show_stats_mode(mode: u8) {
    let mode = mode.min(3);
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if cfg.log_level == level {
            return;
        }
        cfg.log_level = level;
    }
    save_without_keymaps();
}

pub fn update_log_to_file(enabled: bool) {
    logging::set_file_logging_enabled(enabled);
    {
        let mut cfg = lock_config();
        if cfg.log_to_file == enabled {
            return;
        }
        cfg.log_to_file = enabled;
    }
    save_without_keymaps();
}

#[cfg(target_os = "windows")]
pub fn update_windows_gamepad_backend(backend: WindowsPadBackend) {
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if (cfg.bg_brightness - clamped).abs() < f32::EPSILON {
            return;
        }
        cfg.bg_brightness = clamped;
    }
    save_without_keymaps();
}

pub fn update_center_1player_notefield(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.center_1player_notefield == enabled {
            return;
        }
        cfg.center_1player_notefield = enabled;
    }
    save_without_keymaps();
}

pub fn update_banner_cache(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.banner_cache == enabled {
            return;
        }
        cfg.banner_cache = enabled;
    }
    save_without_keymaps();
}

pub fn update_cdtitle_cache(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.cdtitle_cache == enabled {
            return;
        }
        cfg.cdtitle_cache = enabled;
    }
    save_without_keymaps();
}

pub fn update_song_parsing_threads(threads: u8) {
    {
        let mut cfg = lock_config();
        if cfg.song_parsing_threads == threads {
            return;
        }
        cfg.song_parsing_threads = threads;
    }
    save_without_keymaps();
}

pub fn update_cache_songs(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.cachesongs == enabled {
            return;
        }
        cfg.cachesongs = enabled;
    }
    save_without_keymaps();
}

pub fn update_fastload(enabled: bool) {
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if cfg.master_volume == vol {
            return;
        }
        cfg.master_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_music_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.music_volume == vol {
            return;
        }
        cfg.music_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_menu_music(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.menu_music == enabled {
            return;
        }
        cfg.menu_music = enabled;
    }
    save_without_keymaps();
}

pub fn update_software_renderer_threads(threads: u8) {
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if cfg.sfx_volume == vol {
            return;
        }
        cfg.sfx_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_assist_tick_volume(volume: u8) {
    let vol = volume.clamp(0, 100);
    {
        let mut cfg = lock_config();
        if cfg.assist_tick_volume == vol {
            return;
        }
        cfg.assist_tick_volume = vol;
        sync_audio_mix_levels_from_config(&cfg);
    }
    save_without_keymaps();
}

pub fn update_audio_sample_rate(rate: Option<u32>) {
    {
        let mut cfg = lock_config();
        if cfg.audio_sample_rate_hz == rate {
            return;
        }
        cfg.audio_sample_rate_hz = rate;
    }
    save_without_keymaps();
}

pub fn update_audio_output_device(index: Option<u16>) {
    {
        let mut cfg = lock_config();
        if cfg.audio_output_device_index == index {
            return;
        }
        cfg.audio_output_device_index = index;
    }
    save_without_keymaps();
}

pub fn update_audio_output_mode(mode: AudioOutputMode) {
    {
        let mut cfg = lock_config();
        if cfg.audio_output_mode == mode {
            return;
        }
        cfg.audio_output_mode = mode;
    }
    save_without_keymaps();
}

#[cfg(target_os = "linux")]
pub fn update_linux_audio_backend(backend: LinuxAudioBackend) {
    {
        let mut cfg = lock_config();
        if cfg.linux_audio_backend == backend {
            return;
        }
        cfg.linux_audio_backend = backend;
    }
    save_without_keymaps();
}

pub fn update_mine_hit_sound(enabled: bool) {
    {
        let mut cfg = lock_config();
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
        let mut cfg = lock_config();
        if cfg.music_wheel_switch_speed == speed {
            return;
        }
        cfg.music_wheel_switch_speed = speed;
    }
    save_without_keymaps();
}

pub fn update_translated_titles(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.translated_titles == enabled {
            return;
        }
        cfg.translated_titles = enabled;
    }
    save_without_keymaps();
}

pub fn update_rate_mod_preserves_pitch(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.rate_mod_preserves_pitch == enabled {
            return;
        }
        cfg.rate_mod_preserves_pitch = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_breakdown_style(style: BreakdownStyle) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_breakdown_style == style {
            return;
        }
        cfg.select_music_breakdown_style = style;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_breakdown(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_breakdown == enabled {
            return;
        }
        cfg.show_select_music_breakdown = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_banners(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_banners == enabled {
            return;
        }
        cfg.show_select_music_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_video_banners(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_video_banners == enabled {
            return;
        }
        cfg.show_select_music_video_banners = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_cdtitles(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_cdtitles == enabled {
            return;
        }
        cfg.show_select_music_cdtitles = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_grades(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_music_wheel_grades == enabled {
            return;
        }
        cfg.show_music_wheel_grades = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_music_wheel_lamps(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_music_wheel_lamps == enabled {
            return;
        }
        cfg.show_music_wheel_lamps = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_itl_wheel_mode(mode: SelectMusicItlWheelMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_itl_wheel_mode == mode {
            return;
        }
        cfg.select_music_itl_wheel_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_select_music_new_pack_mode(mode: NewPackMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_new_pack_mode == mode {
            return;
        }
        cfg.select_music_new_pack_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_previews(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_previews == enabled {
            return;
        }
        cfg.show_select_music_previews = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_preview_marker(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_preview_marker == enabled {
            return;
        }
        cfg.show_select_music_preview_marker = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_preview_loop(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_preview_loop == enabled {
            return;
        }
        cfg.select_music_preview_loop = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_pattern_info_mode(mode: SelectMusicPatternInfoMode) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_pattern_info_mode == mode {
            return;
        }
        cfg.select_music_pattern_info_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_gameplay_timer(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_gameplay_timer == enabled {
            return;
        }
        cfg.show_select_music_gameplay_timer = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_select_music_scorebox(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_select_music_scorebox == enabled {
            return;
        }
        cfg.show_select_music_scorebox = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_placement(mode: SelectMusicScoreboxPlacement) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_placement == mode {
            return;
        }
        cfg.select_music_scorebox_placement = mode;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_itg(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_itg == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_itg = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_ex(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_ex == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_ex = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_hard_ex(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_hard_ex == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_hard_ex = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_scorebox_cycle_tournaments(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_scorebox_cycle_tournaments == enabled {
            return;
        }
        cfg.select_music_scorebox_cycle_tournaments = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_screenshot_eval(mask: u8) {
    {
        let mut cfg = lock_config();
        if cfg.auto_screenshot_eval == mask {
            return;
        }
        cfg.auto_screenshot_eval = mask;
    }
    save_without_keymaps();
}

pub fn update_show_random_courses(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_random_courses == enabled {
            return;
        }
        cfg.show_random_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_most_played_courses(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_most_played_courses == enabled {
            return;
        }
        cfg.show_most_played_courses = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_course_individual_scores(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_course_individual_scores == enabled {
            return;
        }
        cfg.show_course_individual_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_autosubmit_course_scores_individually(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.autosubmit_course_scores_individually == enabled {
            return;
        }
        cfg.autosubmit_course_scores_individually = enabled;
    }
    save_without_keymaps();
}

pub fn update_zmod_rating_box_text(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.zmod_rating_box_text == enabled {
            return;
        }
        cfg.zmod_rating_box_text = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_bpm_decimal(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_bpm_decimal == enabled {
            return;
        }
        cfg.show_bpm_decimal = enabled;
    }
    save_without_keymaps();
}

pub fn update_default_fail_type(fail_type: DefaultFailType) {
    {
        let mut cfg = lock_config();
        if cfg.default_fail_type == fail_type {
            return;
        }
        cfg.default_fail_type = fail_type;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_sync_graph(mode: SyncGraphMode) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_sync_graph == mode {
            return;
        }
        cfg.null_or_die_sync_graph = mode;
    }
    save_without_keymaps();
}

pub fn update_input_debounce_seconds(seconds: f32) {
    let seconds = seconds.clamp(0.0, 0.2);
    {
        let mut cfg = lock_config();
        if (cfg.input_debounce_seconds - seconds).abs() <= f32::EPSILON {
            return;
        }
        cfg.input_debounce_seconds = seconds;
    }
    crate::core::input::set_input_debounce_seconds(seconds);
    save_without_keymaps();
}

pub fn update_only_dedicated_menu_buttons(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.only_dedicated_menu_buttons == enabled {
            return;
        }
        cfg.only_dedicated_menu_buttons = enabled;
    }
    crate::core::input::set_only_dedicated_menu_buttons(enabled);
    save_without_keymaps();
}

pub fn update_keyboard_features(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.keyboard_features == enabled {
            return;
        }
        cfg.keyboard_features = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_profile(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_profile == enabled {
            return;
        }
        cfg.machine_show_select_profile = enabled;
    }
    save_without_keymaps();
}

pub fn update_show_video_backgrounds(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.show_video_backgrounds == enabled {
            return;
        }
        cfg.show_video_backgrounds = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_color(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_color == enabled {
            return;
        }
        cfg.machine_show_select_color = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_style(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_style == enabled {
            return;
        }
        cfg.machine_show_select_style = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_select_play_mode(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_select_play_mode == enabled {
            return;
        }
        cfg.machine_show_select_play_mode = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_preferred_style(style: MachinePreferredPlayStyle) {
    {
        let mut cfg = lock_config();
        if cfg.machine_preferred_style == style {
            return;
        }
        cfg.machine_preferred_style = style;
    }
    save_without_keymaps();
}

pub fn update_machine_preferred_play_mode(mode: MachinePreferredPlayMode) {
    {
        let mut cfg = lock_config();
        if cfg.machine_preferred_play_mode == mode {
            return;
        }
        cfg.machine_preferred_play_mode = mode;
    }
    save_without_keymaps();
}

pub fn update_machine_show_eval_summary(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_eval_summary == enabled {
            return;
        }
        cfg.machine_show_eval_summary = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_name_entry(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_name_entry == enabled {
            return;
        }
        cfg.machine_show_name_entry = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_show_gameover(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_show_gameover == enabled {
            return;
        }
        cfg.machine_show_gameover = enabled;
    }
    save_without_keymaps();
}

pub fn update_machine_enable_replays(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.machine_enable_replays == enabled {
            return;
        }
        cfg.machine_enable_replays = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_groovestats(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_groovestats == enabled {
            return;
        }
        cfg.enable_groovestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_boogiestats(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_boogiestats == enabled {
            return;
        }
        cfg.enable_boogiestats = enabled;
    }
    save_without_keymaps();
}

pub fn update_enable_arrowcloud(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.enable_arrowcloud == enabled {
            return;
        }
        cfg.enable_arrowcloud = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_download_unlocks(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.auto_download_unlocks == enabled {
            return;
        }
        cfg.auto_download_unlocks = enabled;
    }
    save_without_keymaps();
}

pub fn update_auto_populate_gs_scores(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.auto_populate_gs_scores == enabled {
            return;
        }
        cfg.auto_populate_gs_scores = enabled;
    }
    save_without_keymaps();
}

pub fn update_separate_unlocks_by_player(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.separate_unlocks_by_player == enabled {
            return;
        }
        cfg.separate_unlocks_by_player = enabled;
    }
    save_without_keymaps();
}

pub fn update_game_flag(flag: GameFlag) {
    {
        let mut cfg = lock_config();
        if cfg.game_flag == flag {
            return;
        }
        cfg.game_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_theme_flag(flag: ThemeFlag) {
    {
        let mut cfg = lock_config();
        if cfg.theme_flag == flag {
            return;
        }
        cfg.theme_flag = flag;
    }
    save_without_keymaps();
}

pub fn update_language_flag(flag: LanguageFlag) {
    {
        let mut cfg = lock_config();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keycode_common_keys() {
        let cases = [
            ("KeyCode::Enter", KeyCode::Enter),
            ("KeyCode::Escape", KeyCode::Escape),
            ("KeyCode::ArrowUp", KeyCode::ArrowUp),
            ("KeyCode::ArrowDown", KeyCode::ArrowDown),
            ("KeyCode::ArrowLeft", KeyCode::ArrowLeft),
            ("KeyCode::ArrowRight", KeyCode::ArrowRight),
            ("KeyCode::Slash", KeyCode::Slash),
            ("KeyCode::KeyA", KeyCode::KeyA),
            ("KeyCode::KeyZ", KeyCode::KeyZ),
            ("KeyCode::Numpad0", KeyCode::Numpad0),
            ("KeyCode::Numpad9", KeyCode::Numpad9),
            ("KeyCode::NumpadEnter", KeyCode::NumpadEnter),
            ("KeyCode::NumpadDecimal", KeyCode::NumpadDecimal),
        ];
        for (token, expected) in cases {
            assert_eq!(
                parse_keycode(token),
                Some(InputBinding::Key(expected)),
                "failed for {token}"
            );
        }
    }

    #[test]
    fn auto_screenshot_mask_roundtrips() {
        let mask = AUTO_SS_PBS | AUTO_SS_CLEARS | AUTO_SS_QUINTS;
        let encoded = auto_screenshot_mask_to_str(mask);
        assert_eq!(encoded, "PBs|Clears|Quints");
        assert_eq!(auto_screenshot_mask_from_str(&encoded), mask);
    }

    #[test]
    fn auto_screenshot_mask_handles_off_and_unknown_tokens() {
        assert_eq!(auto_screenshot_mask_from_str(""), 0);
        assert_eq!(auto_screenshot_mask_from_str("Off"), 0);
        assert_eq!(
            auto_screenshot_mask_from_str("PBs|unknown|Fails"),
            AUTO_SS_PBS | AUTO_SS_FAILS
        );
    }

    #[test]
    fn parse_keycode_previously_missing_keys() {
        let cases = [
            ("KeyCode::Period", KeyCode::Period),
            ("KeyCode::AltLeft", KeyCode::AltLeft),
            ("KeyCode::AltRight", KeyCode::AltRight),
            ("KeyCode::ControlLeft", KeyCode::ControlLeft),
            ("KeyCode::ControlRight", KeyCode::ControlRight),
            ("KeyCode::ShiftLeft", KeyCode::ShiftLeft),
            ("KeyCode::ShiftRight", KeyCode::ShiftRight),
            ("KeyCode::Space", KeyCode::Space),
            ("KeyCode::Tab", KeyCode::Tab),
            ("KeyCode::Backspace", KeyCode::Backspace),
            ("KeyCode::CapsLock", KeyCode::CapsLock),
            ("KeyCode::Delete", KeyCode::Delete),
            ("KeyCode::Home", KeyCode::Home),
            ("KeyCode::End", KeyCode::End),
            ("KeyCode::PageUp", KeyCode::PageUp),
            ("KeyCode::PageDown", KeyCode::PageDown),
            ("KeyCode::Insert", KeyCode::Insert),
            ("KeyCode::F1", KeyCode::F1),
            ("KeyCode::F12", KeyCode::F12),
            ("KeyCode::PrintScreen", KeyCode::PrintScreen),
            ("KeyCode::Comma", KeyCode::Comma),
            ("KeyCode::Minus", KeyCode::Minus),
            ("KeyCode::Equal", KeyCode::Equal),
            ("KeyCode::BracketLeft", KeyCode::BracketLeft),
            ("KeyCode::Backquote", KeyCode::Backquote),
            ("KeyCode::Digit0", KeyCode::Digit0),
            ("KeyCode::Digit9", KeyCode::Digit9),
            ("KeyCode::NumLock", KeyCode::NumLock),
            ("KeyCode::ScrollLock", KeyCode::ScrollLock),
            ("KeyCode::Pause", KeyCode::Pause),
            ("KeyCode::ContextMenu", KeyCode::ContextMenu),
            ("KeyCode::SuperLeft", KeyCode::SuperLeft),
            ("KeyCode::AudioVolumeMute", KeyCode::AudioVolumeMute),
            ("KeyCode::F35", KeyCode::F35),
        ];
        for (token, expected) in cases {
            assert_eq!(
                parse_keycode(token),
                Some(InputBinding::Key(expected)),
                "failed for {token}"
            );
        }
    }

    #[test]
    fn parse_keycode_rejects_invalid() {
        assert_eq!(parse_keycode("KeyCode::NotAKey"), None);
        assert_eq!(parse_keycode("KeyCode::"), None);
        assert_eq!(parse_keycode("NotKeyCode::Enter"), None);
        assert_eq!(parse_keycode("Enter"), None);
        assert_eq!(parse_keycode(""), None);
    }

    #[test]
    fn parse_pad_dir_valid() {
        assert_eq!(parse_pad_dir("Up"), Some(PadDir::Up));
        assert_eq!(parse_pad_dir("Down"), Some(PadDir::Down));
        assert_eq!(parse_pad_dir("Left"), Some(PadDir::Left));
        assert_eq!(parse_pad_dir("Right"), Some(PadDir::Right));
    }

    #[test]
    fn parse_pad_dir_invalid() {
        assert_eq!(parse_pad_dir("up"), None);
        assert_eq!(parse_pad_dir(""), None);
        assert_eq!(parse_pad_dir("UpDown"), None);
    }

    #[test]
    fn parse_pad_dir_binding_short_form() {
        assert_eq!(
            parse_pad_dir_binding("PadDir::Up"),
            Some(InputBinding::PadDir(PadDir::Up))
        );
        assert_eq!(
            parse_pad_dir_binding("PadDir::Right"),
            Some(InputBinding::PadDir(PadDir::Right))
        );
    }

    #[test]
    fn parse_pad_device_binding_any_pad_long_form() {
        assert_eq!(
            parse_pad_device_binding("Pad::Dir::Down"),
            Some(InputBinding::PadDir(PadDir::Down))
        );
    }

    #[test]
    fn parse_pad_device_binding_device_specific() {
        assert_eq!(
            parse_pad_device_binding("Pad0::Dir::Up"),
            Some(InputBinding::PadDirOn {
                device: 0,
                dir: PadDir::Up,
            })
        );
        assert_eq!(
            parse_pad_device_binding("Pad3::Dir::Left"),
            Some(InputBinding::PadDirOn {
                device: 3,
                dir: PadDir::Left,
            })
        );
    }

    #[test]
    fn parse_pad_dir_binding_rejects_invalid() {
        assert_eq!(parse_pad_dir_binding("PadDir::Diagonal"), None);
        assert_eq!(parse_pad_dir_binding("Pad0::Btn::A"), None);
        assert_eq!(parse_pad_dir_binding("Pad0::Dir"), None);
        assert_eq!(parse_pad_dir_binding("NotPad::Dir::Up"), None);
    }

    #[test]
    fn parse_pad_code_hex_only() {
        assert_eq!(
            parse_pad_code("PadCode[0xDEADBEEF]"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0xDEADBEEF,
                device: None,
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_pad_code_decimal() {
        assert_eq!(
            parse_pad_code("PadCode[42]"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 42,
                device: None,
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_pad_code_with_device() {
        assert_eq!(
            parse_pad_code("PadCode[0x00000001]@2"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 1,
                device: Some(2),
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_pad_code_with_uuid() {
        let uuid_hex = "00112233AABBCCDDEEFF001122334455";
        let token = format!("PadCode[0xFF]#{uuid_hex}");
        let expected_uuid = [
            0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33,
            0x44, 0x55,
        ];
        assert_eq!(
            parse_pad_code(&token),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0xFF,
                device: None,
                uuid: Some(expected_uuid),
            }))
        );
    }

    #[test]
    fn parse_pad_code_with_device_and_uuid() {
        let token = "PadCode[0xDEADBEEF]@0#00112233AABBCCDDEEFF001122334455";
        let Some(InputBinding::GamepadCode(binding)) = parse_pad_code(token) else {
            panic!("expected gamepad code binding");
        };
        assert_eq!(binding.code_u32, 0xDEADBEEF);
        assert_eq!(binding.device, Some(0));
        assert!(binding.uuid.is_some());
    }

    #[test]
    fn parse_pad_code_rejects_invalid() {
        assert_eq!(parse_pad_code("PadCode[]"), None);
        assert_eq!(parse_pad_code("PadCode[xyz]"), None);
        assert_eq!(parse_pad_code("NotPadCode[0x01]"), None);
        assert_eq!(parse_pad_code(""), None);
    }

    #[test]
    fn parse_binding_token_dispatches_keycode() {
        assert_eq!(
            parse_binding_token("KeyCode::Period"),
            Some(InputBinding::Key(KeyCode::Period))
        );
    }

    #[test]
    fn parse_binding_token_dispatches_pad_dir() {
        assert_eq!(
            parse_binding_token("PadDir::Up"),
            Some(InputBinding::PadDir(PadDir::Up))
        );
    }

    #[test]
    fn parse_binding_token_dispatches_pad_device() {
        assert_eq!(
            parse_binding_token("Pad0::Dir::Left"),
            Some(InputBinding::PadDirOn {
                device: 0,
                dir: PadDir::Left,
            })
        );
    }

    #[test]
    fn parse_binding_token_dispatches_pad_code() {
        assert_eq!(
            parse_binding_token("PadCode[0x42]"),
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0x42,
                device: None,
                uuid: None,
            }))
        );
    }

    #[test]
    fn parse_binding_token_trims_whitespace() {
        assert_eq!(
            parse_binding_token("  KeyCode::Enter  "),
            Some(InputBinding::Key(KeyCode::Enter))
        );
    }

    #[test]
    fn parse_binding_token_rejects_garbage() {
        assert_eq!(parse_binding_token("garbage"), None);
        assert_eq!(parse_binding_token(""), None);
    }

    #[test]
    fn round_trip_keyboard_bindings() {
        let keys = [
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::Period,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::Space,
            KeyCode::Tab,
            KeyCode::Backspace,
            KeyCode::ArrowUp,
            KeyCode::KeyA,
            KeyCode::KeyZ,
            KeyCode::Digit0,
            KeyCode::Digit9,
            KeyCode::Numpad0,
            KeyCode::Numpad2,
            KeyCode::NumpadEnter,
            KeyCode::NumpadDecimal,
            KeyCode::F1,
            KeyCode::F12,
            KeyCode::F35,
            KeyCode::ControlLeft,
            KeyCode::ShiftRight,
            KeyCode::SuperLeft,
            KeyCode::PrintScreen,
            KeyCode::Comma,
            KeyCode::Minus,
            KeyCode::Slash,
            KeyCode::Backquote,
            KeyCode::BracketLeft,
            KeyCode::AudioVolumeMute,
        ];
        for key in keys {
            let binding = InputBinding::Key(key);
            let token = binding_to_token(binding);
            assert_eq!(
                parse_binding_token(&token),
                Some(binding),
                "round-trip failed for {key:?}: token was {token:?}"
            );
        }
    }

    #[test]
    fn round_trip_pad_dir() {
        for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
            let binding = InputBinding::PadDir(dir);
            let token = binding_to_token(binding);
            assert_eq!(
                parse_binding_token(&token),
                Some(binding),
                "round-trip failed for {dir:?}"
            );
        }
    }

    #[test]
    fn round_trip_pad_dir_on() {
        for device in [0, 1, 5] {
            for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
                let binding = InputBinding::PadDirOn { device, dir };
                let token = binding_to_token(binding);
                assert_eq!(
                    parse_binding_token(&token),
                    Some(binding),
                    "round-trip failed for device={device}, dir={dir:?}"
                );
            }
        }
    }

    #[test]
    fn round_trip_gamepad_code() {
        let cases = [
            GamepadCodeBinding {
                code_u32: 0xDEADBEEF,
                device: None,
                uuid: None,
            },
            GamepadCodeBinding {
                code_u32: 42,
                device: Some(0),
                uuid: None,
            },
            GamepadCodeBinding {
                code_u32: 0xFF,
                device: None,
                uuid: Some([
                    0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22,
                    0x33, 0x44, 0x55,
                ]),
            },
            GamepadCodeBinding {
                code_u32: 0x01,
                device: Some(3),
                uuid: Some([0xAB; 16]),
            },
        ];
        for binding in cases {
            let input = InputBinding::GamepadCode(binding);
            let token = binding_to_token(input);
            assert_eq!(
                parse_binding_token(&token),
                Some(input),
                "round-trip failed for {binding:?}: token was {token:?}"
            );
        }
    }
}
