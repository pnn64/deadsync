mod audio;
mod ini;
mod keybinds;
#[path = "config/null_or_die.rs"]
mod null_or_die_cfg;
mod store;
mod theme;
mod video;

pub use self::audio::{AudioMixLevels, AudioOutputMode, LinuxAudioBackend};
pub use self::ini::SimpleIni;
pub use self::keybinds::{
    update_keymap_binding_unique_gamepad, update_keymap_binding_unique_keyboard,
};
pub use self::null_or_die_cfg::null_or_die_bias_cfg;
pub use self::store::{bootstrap_log_to_file, load};
pub use self::theme::{
    AUTO_SS_CLEARS, AUTO_SS_FAILS, AUTO_SS_FLAG_NAMES, AUTO_SS_NUM_FLAGS, AUTO_SS_PBS,
    AUTO_SS_QUADS, AUTO_SS_QUINTS, BreakdownStyle, DefaultFailType, GameFlag, LanguageFlag,
    LogLevel, MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode,
    SelectMusicItlWheelMode, SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement,
    SyncGraphMode, ThemeFlag, auto_screenshot_bit, auto_screenshot_mask_from_str,
    auto_screenshot_mask_to_str,
};
pub use self::video::{DisplayMode, FullscreenType};

use self::audio::{pack_audio_mix_levels, unpack_audio_mix_levels};
use self::keybinds::{
    ALL_VIRTUAL_ACTIONS, action_to_ini_key, binding_to_token, load_keymap_from_ini_local,
};
#[cfg(test)]
use self::keybinds::{
    parse_binding_token, parse_keycode, parse_pad_code, parse_pad_device_binding, parse_pad_dir,
    parse_pad_dir_binding,
};
use self::null_or_die_cfg::{
    clamp_null_or_die_confidence_percent, clamp_null_or_die_magic_offset_ms,
    clamp_null_or_die_positive_ms, null_or_die_kernel_target_str, null_or_die_kernel_type_str,
    parse_null_or_die_kernel_target, parse_null_or_die_kernel_type,
};
use self::store::{normalize_machine_default_noteskin, save_without_keymaps};
use crate::core::gfx::{BackendType, PresentModePolicy};
use crate::core::input::WindowsPadBackend;
#[cfg(test)]
use crate::core::input::{GamepadCodeBinding, InputBinding, PadDir};
use crate::core::logging;
use log::{debug, info, warn};
use null_or_die::{BiasCfg, BiasKernel, KernelTarget};
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(test)]
use winit::keyboard::KeyCode;
const CONFIG_PATH: &str = "deadsync.ini";
const DEFAULT_MACHINE_NOTESKIN: &str = "cel";

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
    /// Minimum confidence percent required for pack sync saves.
    pub null_or_die_confidence_percent: u8,
    pub null_or_die_fingerprint_ms: f64,
    pub null_or_die_window_ms: f64,
    pub null_or_die_step_ms: f64,
    pub null_or_die_magic_offset_ms: f64,
    pub null_or_die_kernel_target: KernelTarget,
    pub null_or_die_kernel_type: BiasKernel,
    pub null_or_die_full_spectrogram: bool,
    pub select_music_breakdown_style: BreakdownStyle,
    pub select_music_pattern_info_mode: SelectMusicPatternInfoMode,
    pub show_select_music_scorebox: bool,
    pub select_music_scorebox_placement: SelectMusicScoreboxPlacement,
    pub select_music_scorebox_cycle_itg: bool,
    pub select_music_scorebox_cycle_ex: bool,
    pub select_music_scorebox_cycle_hard_ex: bool,
    pub select_music_scorebox_cycle_tournaments: bool,
    pub select_music_chart_info_peak_nps: bool,
    pub select_music_chart_info_matrix_rating: bool,
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
            null_or_die_confidence_percent: 80,
            null_or_die_fingerprint_ms: 50.0,
            null_or_die_window_ms: 10.0,
            null_or_die_step_ms: 0.2,
            null_or_die_magic_offset_ms: 0.0,
            null_or_die_kernel_target: KernelTarget::Digest,
            null_or_die_kernel_type: BiasKernel::Rising,
            null_or_die_full_spectrogram: false,
            select_music_breakdown_style: BreakdownStyle::Sl,
            select_music_pattern_info_mode: SelectMusicPatternInfoMode::Tech,
            show_select_music_scorebox: true,
            select_music_scorebox_placement: SelectMusicScoreboxPlacement::Auto,
            select_music_scorebox_cycle_itg: true,
            select_music_scorebox_cycle_ex: true,
            select_music_scorebox_cycle_hard_ex: true,
            select_music_scorebox_cycle_tournaments: true,
            select_music_chart_info_peak_nps: true,
            select_music_chart_info_matrix_rating: false,
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

pub fn update_select_music_chart_info_peak_nps(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_chart_info_peak_nps == enabled {
            return;
        }
        cfg.select_music_chart_info_peak_nps = enabled;
    }
    save_without_keymaps();
}

pub fn update_select_music_chart_info_matrix_rating(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.select_music_chart_info_matrix_rating == enabled {
            return;
        }
        cfg.select_music_chart_info_matrix_rating = enabled;
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

pub fn update_null_or_die_confidence_percent(value: u8) {
    let value = clamp_null_or_die_confidence_percent(value);
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_confidence_percent == value {
            return;
        }
        cfg.null_or_die_confidence_percent = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_fingerprint_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_fingerprint_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_fingerprint_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_window_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_window_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_window_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_step_ms(value: f64) {
    let value = clamp_null_or_die_positive_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_step_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_step_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_magic_offset_ms(value: f64) {
    let value = clamp_null_or_die_magic_offset_ms(value);
    {
        let mut cfg = lock_config();
        if (cfg.null_or_die_magic_offset_ms - value).abs() <= f64::EPSILON {
            return;
        }
        cfg.null_or_die_magic_offset_ms = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_kernel_target(value: KernelTarget) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_kernel_target == value {
            return;
        }
        cfg.null_or_die_kernel_target = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_kernel_type(value: BiasKernel) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_kernel_type == value {
            return;
        }
        cfg.null_or_die_kernel_type = value;
    }
    save_without_keymaps();
}

pub fn update_null_or_die_full_spectrogram(enabled: bool) {
    {
        let mut cfg = lock_config();
        if cfg.null_or_die_full_spectrogram == enabled {
            return;
        }
        cfg.null_or_die_full_spectrogram = enabled;
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

    fn assert_tenths_eq(actual: f64, expected_tenths: i32) {
        assert_eq!((actual * 10.0).round() as i32, expected_tenths);
    }

    #[test]
    fn clamp_null_or_die_confidence_caps_at_100() {
        assert_eq!(clamp_null_or_die_confidence_percent(0), 0);
        assert_eq!(clamp_null_or_die_confidence_percent(80), 80);
        assert_eq!(clamp_null_or_die_confidence_percent(120), 100);
    }

    #[test]
    fn clamp_null_or_die_positive_ms_uses_tenths() {
        assert_tenths_eq(clamp_null_or_die_positive_ms(0.0), 1);
        assert_tenths_eq(clamp_null_or_die_positive_ms(10.04), 100);
        assert_tenths_eq(clamp_null_or_die_positive_ms(10.05), 101);
        assert_tenths_eq(clamp_null_or_die_positive_ms(1000.0), 1000);
    }

    #[test]
    fn clamp_null_or_die_magic_offset_uses_tenths() {
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(-200.0), -1000);
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(0.04), 0);
        assert_tenths_eq(clamp_null_or_die_magic_offset_ms(0.05), 1);
    }

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
