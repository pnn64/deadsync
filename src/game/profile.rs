use crate::config::{self, SimpleIni};
use chrono::Local;
use deadsync_platform::dirs;
use deadsync_rules::scroll::{GUEST_SCROLL_SPEED, ScrollSpeedSetting};
use log::{debug, info, warn};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

mod update;

use deadsync_profile::{
    AccelEffectsMask, ActiveProfile, AppearanceEffectsMask, AttackMode, ColumnFlashBrightness,
    ColumnFlashMask, ColumnFlashSize, DEFAULT_PROFILE_ID, GameplayHudPlayerSnapshot,
    GameplayHudSnapshot, HideLightType, HoldsMask, InsertMask, LOCAL_PROFILE_MAX_ID, LastPlayed,
    LastPlayedCourse, LifeMeterType, LocalProfileSummary, MeasureCounter, MeasureLines,
    MiniIndicator, MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorScoreType,
    MiniIndicatorSize, MiniIndicatorSubtractiveDisplay, NoteSkin, PLAYER_SLOTS, PlayMode,
    PlayStyle, PlayerOptionsData, PlayerSide, Profile, ProfileStats, ProfileStatsDecodeError,
    RemoveMask, ScrollOption, StepStatisticsMask, StepStatsExtra, TargetScoreSetting,
    TimingTickMode, TimingWindowsOption, TurnOption, VisualEffectsMask, active_profile_is_guest,
    active_profile_local_id, add_known_pack_names, append_last_played_course_section,
    append_last_played_section, append_player_options_section, clamp_weight_pounds,
    cmp_profile_ids_case_insensitive, decode_profile_stats as decode_profile_stats_bytes,
    encode_profile_stats, find_profile_avatar_path, initials_from_name, is_local_profile_id,
    joined_player_mask, load_error_bar_options, load_last_played_course_section,
    load_last_played_section, load_timing_feedback_options, load_visual_player_options,
    next_local_profile_id, parse_favorites_content, parse_favorited_packs_content,
    parse_groovestats_is_pad_player, player_options_section,
    player_side_index as side_ix, player_side_is_joined,
    render_favorites_content, render_favorited_packs_content,
    rewrite_profile_display_name_content, sanitize_player_initials,
    unknown_pack_names,
};
pub use update::*;

#[inline(always)]
fn local_profile_dir(id: &str) -> PathBuf {
    dirs::app_dirs().profiles_root().join(id)
}

#[inline(always)]
pub fn local_profile_dir_for_id(id: &str) -> PathBuf {
    local_profile_dir(id)
}

#[inline(always)]
fn profile_ini_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("profile.ini")
}

#[inline(always)]
fn groovestats_ini_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("groovestats.ini")
}

#[inline(always)]
fn arrowcloud_ini_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("arrowcloud.ini")
}

#[inline(always)]
fn profile_stats_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("stats.bin")
}

#[inline(always)]
fn load_player_options(
    profile_conf: &SimpleIni,
    section: &str,
    default: &PlayerOptionsData,
) -> Option<PlayerOptionsData> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    if !has_any {
        return None;
    }

    let mut options = default.clone();
    load_visual_player_options(&mut options, |key| profile_conf.get(section, key));
    load_timing_feedback_options(&mut options, |key| profile_conf.get(section, key));
    load_error_bar_options(&mut options, |key| profile_conf.get(section, key));
    if let Some(step_statistics) = profile_conf
        .get(section, "StepStatistics")
        .and_then(|s| StepStatisticsMask::from_str(&s).ok())
    {
        options.step_statistics = step_statistics;
    } else if let Some(step_statistics) = profile_conf
        .get(section, "DataVisualizations")
        .and_then(|s| StepStatisticsMask::from_str(&s).ok())
    {
        options.step_statistics = step_statistics;
    }
    options.step_stats_extra = profile_conf
        .get(section, "StepStatsExtra")
        .and_then(|s| StepStatsExtra::from_str(&s).ok())
        .unwrap_or(options.step_stats_extra);
    options.target_score = profile_conf
        .get(section, "TargetScore")
        .and_then(|s| TargetScoreSetting::from_str(&s).ok())
        .unwrap_or(options.target_score);
    options.lifemeter_type = profile_conf
        .get(section, "LifeMeterType")
        .and_then(|s| LifeMeterType::from_str(&s).ok())
        .unwrap_or(options.lifemeter_type);
    options.measure_counter = profile_conf
        .get(section, "MeasureCounter")
        .and_then(|s| MeasureCounter::from_str(&s).ok())
        .unwrap_or(options.measure_counter);
    options.measure_counter_lookahead = profile_conf
        .get(section, "MeasureCounterLookahead")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v.min(4))
        .unwrap_or(options.measure_counter_lookahead);
    options.measure_counter_left = profile_conf
        .get(section, "MeasureCounterLeft")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.measure_counter_left, |v| v != 0);
    options.measure_counter_up = profile_conf
        .get(section, "MeasureCounterUp")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.measure_counter_up, |v| v != 0);
    options.measure_counter_vert = profile_conf
        .get(section, "MeasureCounterVert")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.measure_counter_vert, |v| v != 0);
    options.broken_run = profile_conf
        .get(section, "BrokenRun")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.broken_run, |v| v != 0);
    options.run_timer = profile_conf
        .get(section, "RunTimer")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.run_timer, |v| v != 0);
    options.measure_lines = profile_conf
        .get(section, "MeasureLines")
        .and_then(|s| MeasureLines::from_str(&s).ok())
        .unwrap_or(options.measure_lines);
    options.scroll_speed = profile_conf
        .get(section, "ScrollSpeed")
        .and_then(|s| ScrollSpeedSetting::from_str(&s).ok())
        .unwrap_or(options.scroll_speed);
    options.no_cmod_alternative = profile_conf
        .get(section, "NoCmodAlternative")
        .and_then(|s| deadsync_profile::NoCmodAlternative::from_str(&s).ok())
        .unwrap_or(options.no_cmod_alternative);
    options.turn_option = profile_conf
        .get(section, "Turn")
        .and_then(|s| TurnOption::from_str(&s).ok())
        .unwrap_or(options.turn_option);
    options.insert_active_mask = profile_conf
        .get(section, "InsertMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(InsertMask::from_bits_truncate)
        .unwrap_or(options.insert_active_mask);
    options.remove_active_mask = profile_conf
        .get(section, "RemoveMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(RemoveMask::from_bits_truncate)
        .unwrap_or(options.remove_active_mask);
    options.holds_active_mask = profile_conf
        .get(section, "HoldsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(HoldsMask::from_bits_truncate)
        .unwrap_or(options.holds_active_mask);
    options.accel_effects_active_mask = profile_conf
        .get(section, "AccelEffectsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(AccelEffectsMask::from_bits_truncate)
        .unwrap_or(options.accel_effects_active_mask);
    options.visual_effects_active_mask = profile_conf
        .get(section, "VisualEffectsMask")
        .and_then(|s| s.parse::<u16>().ok())
        .map(VisualEffectsMask::from_bits_truncate)
        .unwrap_or(options.visual_effects_active_mask);
    options.appearance_effects_active_mask = profile_conf
        .get(section, "AppearanceEffectsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(AppearanceEffectsMask::from_bits_truncate)
        .unwrap_or(options.appearance_effects_active_mask);
    options.attack_mode = profile_conf
        .get(section, "AttackMode")
        .or_else(|| profile_conf.get(section, "Attacks"))
        .and_then(|s| AttackMode::from_str(&s).ok())
        .unwrap_or(options.attack_mode);
    options.hide_light_type = profile_conf
        .get(section, "HideLightType")
        .and_then(|s| HideLightType::from_str(&s).ok())
        .unwrap_or(options.hide_light_type);
    options.rescore_early_hits = profile_conf
        .get(section, "RescoreEarlyHits")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.rescore_early_hits, |v| v != 0);
    options.hide_early_dw_judgments = profile_conf
        .get(section, "HideEarlyDecentWayOffJudgments")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_early_dw_judgments, |v| v != 0);
    options.hide_early_dw_flash = profile_conf
        .get(section, "HideEarlyDecentWayOffFlash")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_early_dw_flash, |v| v != 0);
    options.hide_early_dw_column_flash = profile_conf
        .get(section, "HideEarlyDecentWayOffColumnFlash")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_early_dw_column_flash, |v| v != 0);
    options.timing_windows = profile_conf
        .get(section, "TimingWindows")
        .and_then(|s| TimingWindowsOption::from_str(&s).ok())
        .unwrap_or(options.timing_windows);
    options.hide_targets = profile_conf
        .get(section, "HideTargets")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_targets, |v| v != 0);
    options.hide_song_bg = profile_conf
        .get(section, "HideSongBG")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_song_bg, |v| v != 0);
    options.hide_combo = profile_conf
        .get(section, "HideCombo")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_combo, |v| v != 0);
    options.hide_lifebar = profile_conf
        .get(section, "HideLifebar")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_lifebar, |v| v != 0);
    options.hide_score = profile_conf
        .get(section, "HideScore")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_score, |v| v != 0);
    options.hide_danger = profile_conf
        .get(section, "HideDanger")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_danger, |v| v != 0);
    options.hide_combo_explosions = profile_conf
        .get(section, "HideComboExplosions")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_combo_explosions, |v| v != 0);
    options.hide_username = profile_conf
        .get(section, "HideUsername")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.hide_username, |v| v != 0);
    options.column_flash_on_miss = profile_conf
        .get(section, "ColumnFlashOnMiss")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.column_flash_on_miss, |v| v != 0);
    options.column_flash_mask = profile_conf
        .get(section, "ColumnFlashMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(ColumnFlashMask::from_bits_truncate)
        .unwrap_or(options.column_flash_mask);
    options.column_flash_brightness = profile_conf
        .get(section, "ColumnFlashBrightness")
        .and_then(|s| ColumnFlashBrightness::from_str(&s).ok())
        .unwrap_or(options.column_flash_brightness);
    options.column_flash_size = profile_conf
        .get(section, "ColumnFlashSize")
        .and_then(|s| ColumnFlashSize::from_str(&s).ok())
        .unwrap_or(options.column_flash_size);
    options.subtractive_scoring = profile_conf
        .get(section, "SubtractiveScoring")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.subtractive_scoring, |v| v != 0);
    options.pacemaker = profile_conf
        .get(section, "Pacemaker")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.pacemaker, |v| v != 0);
    options.nps_graph_at_top = profile_conf
        .get(section, "NPSGraphAtTop")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.nps_graph_at_top, |v| v != 0);
    options.transparent_density_graph_bg = profile_conf
        .get(section, "TransparentDensityGraphBackground")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.transparent_density_graph_bg, |v| v != 0);
    options.smx_fsr_display = profile_conf
        .get(section, "SmxFsrDisplay")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.smx_fsr_display, |v| v != 0);
    options.smx_pad_input_display = profile_conf
        .get(section, "SmxPadInputDisplay")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.smx_pad_input_display, |v| v != 0);
    options.mini_indicator = profile_conf
        .get(section, "MiniIndicator")
        .and_then(|s| MiniIndicator::from_str(&s).ok())
        .unwrap_or({
            if options.subtractive_scoring {
                MiniIndicator::SubtractiveScoring
            } else if options.pacemaker {
                MiniIndicator::Pacemaker
            } else {
                options.mini_indicator
            }
        });
    if options.mini_indicator == MiniIndicator::SubtractiveScoring {
        options.subtractive_scoring = true;
    }
    if options.mini_indicator == MiniIndicator::Pacemaker {
        options.pacemaker = true;
    }
    options.mini_indicator_score_type = profile_conf
        .get(section, "MiniIndicatorScoreType")
        .and_then(|s| MiniIndicatorScoreType::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_score_type);
    options.mini_indicator_subtractive_display = profile_conf
        .get(section, "MiniIndicatorSubtractiveDisplay")
        .and_then(|s| MiniIndicatorSubtractiveDisplay::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_subtractive_display);
    options.mini_indicator_size = profile_conf
        .get(section, "MiniIndicatorSize")
        .and_then(|s| MiniIndicatorSize::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_size);
    options.mini_indicator_color = profile_conf
        .get(section, "MiniIndicatorColor")
        .and_then(|s| MiniIndicatorColor::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_color);
    options.mini_indicator_position = profile_conf
        .get(section, "MiniIndicatorPosition")
        .and_then(|s| MiniIndicatorPosition::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_position);
    options.scroll_option = profile_conf
        .get(section, "Scroll")
        .and_then(|s| ScrollOption::from_str(&s).ok())
        .unwrap_or_else(|| {
            let reverse_enabled = profile_conf
                .get(section, "ReverseScroll")
                .and_then(|v| v.parse::<u8>().ok())
                .map_or(options.reverse_scroll, |v| v != 0);
            if reverse_enabled {
                ScrollOption::Reverse
            } else {
                options.scroll_option
            }
        });
    options.reverse_scroll = options.scroll_option.contains(ScrollOption::Reverse);

    Some(options)
}

#[inline(always)]
fn load_last_played(
    profile_conf: &SimpleIni,
    section: &str,
    default: &LastPlayed,
) -> Option<LastPlayed> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    load_last_played_section(has_any, |key| profile_conf.get(section, key), default)
}

#[inline(always)]
fn load_last_played_course(profile_conf: &SimpleIni, section: &str) -> Option<LastPlayedCourse> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    load_last_played_course_section(has_any, |key| profile_conf.get(section, key))
}

#[inline(always)]
fn profile_stats_tmp_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("stats.bin.tmp")
}

// Global statics for the loaded player profiles.
static PROFILES: std::sync::LazyLock<Mutex<[Profile; PLAYER_SLOTS]>> =
    std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Profile::default())));

#[derive(Debug)]
struct SessionState {
    active_profiles: [ActiveProfile; PLAYER_SLOTS],
    joined_mask: u8,
    music_rate: f32,
    timing_tick_mode: TimingTickMode,
    play_style: PlayStyle,
    play_mode: PlayMode,
    player_side: PlayerSide,
    fast_profile_switch_from_select_music: bool,
}

static SESSION: std::sync::LazyLock<Mutex<SessionState>> = std::sync::LazyLock::new(|| {
    Mutex::new(SessionState {
        active_profiles: [
            ActiveProfile::Local {
                id: DEFAULT_PROFILE_ID.to_string(),
            },
            ActiveProfile::Guest,
        ],
        joined_mask: joined_player_mask(true, false),
        music_rate: 1.0,
        timing_tick_mode: TimingTickMode::Off,
        play_style: PlayStyle::Single,
        play_mode: PlayMode::Regular,
        player_side: PlayerSide::P1,
        fast_profile_switch_from_select_music: false,
    })
});

static LOCK_WAIT_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
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

static SESSION_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();
static PROFILES_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();

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
fn lock_session() -> std::sync::MutexGuard<'static, SessionState> {
    if !lock_wait_stats_enabled() {
        return SESSION.lock().unwrap();
    }
    let start = Instant::now();
    let guard = SESSION.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_lock_wait("SESSION", &SESSION_LOCK_WAIT_STATS, waited_ns);
    guard
}

#[inline(always)]
fn lock_profiles() -> std::sync::MutexGuard<'static, [Profile; PLAYER_SLOTS]> {
    if !lock_wait_stats_enabled() {
        return PROFILES.lock().unwrap();
    }
    let start = Instant::now();
    let guard = PROFILES.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_lock_wait("PROFILES", &PROFILES_LOCK_WAIT_STATS, waited_ns);
    guard
}

#[inline(always)]
fn session_side_is_guest(side: PlayerSide) -> bool {
    active_profile_is_guest(&lock_session().active_profiles[side_ix(side)])
}

#[inline(always)]
fn machine_default_noteskin_value() -> NoteSkin {
    NoteSkin::new(&config::machine_default_noteskin())
}

/// Machine-default pad-light brightness used to seed a new profile, mirroring
/// `machine_default_noteskin_value`. Players adjust their own value afterwards.
#[inline(always)]
fn machine_default_light_brightness() -> u8 {
    config::get().smx_default_light_brightness
}

pub fn machine_default_noteskin() -> NoteSkin {
    machine_default_noteskin_value()
}

pub fn update_machine_default_noteskin(setting: NoteSkin) {
    if config::machine_default_noteskin().eq_ignore_ascii_case(setting.as_str()) {
        return;
    }
    config::update_machine_default_noteskin(setting.as_str());
    {
        let session = lock_session();
        let mut profiles = lock_profiles();
        for side in [PlayerSide::P1, PlayerSide::P2] {
            if active_profile_is_guest(&session.active_profiles[side_ix(side)]) {
                let profile = &mut profiles[side_ix(side)];
                profile.noteskin = setting.clone();
                profile.player_options_singles.noteskin = setting.clone();
                profile.player_options_doubles.noteskin = setting.clone();
            }
        }
    }
}

fn make_guest_profile() -> Profile {
    let mut guest = Profile::default();
    guest.display_name = "[ GUEST ]".to_string();
    guest.scroll_speed = GUEST_SCROLL_SPEED;
    guest.noteskin = machine_default_noteskin_value();
    guest.pad_light_brightness = machine_default_light_brightness();
    guest.avatar_path = None;
    guest.avatar_texture_key = None;
    guest.store_current_player_options_for_all_styles();
    guest
}

fn ensure_local_profile_files(id: &str) -> Result<(), std::io::Error> {
    let dir = local_profile_dir(id);
    let profile_ini = profile_ini_path(id);
    let groovestats_ini = groovestats_ini_path(id);
    let arrowcloud_ini = arrowcloud_ini_path(id);

    info!(
        "Profile files not found, creating defaults in '{}'.",
        dir.display()
    );
    fs::create_dir_all(&dir)?;

    // Create profile.ini
    if !profile_ini.exists() {
        let mut default_profile = Profile::default();
        default_profile.noteskin = machine_default_noteskin_value();
        default_profile.pad_light_brightness = machine_default_light_brightness();
        default_profile.store_current_player_options_for_all_styles();
        let mut content = String::new();
        append_player_options_section(
            &mut content,
            player_options_section(PlayStyle::Single),
            &default_profile.player_options_singles,
        );
        append_player_options_section(
            &mut content,
            player_options_section(PlayStyle::Double),
            &default_profile.player_options_doubles,
        );

        content.push_str("[userprofile]\n");
        content.push_str(&format!("DisplayName = {}\n", default_profile.display_name));
        content.push_str(&format!(
            "PlayerInitials = {}\n",
            default_profile.player_initials
        ));
        content.push('\n');

        content.push_str("[Editable]\n");
        content.push_str(&format!(
            "WeightPounds = {}\n",
            default_profile.weight_pounds
        ));
        content.push_str(&format!("BirthYear = {}\n", default_profile.birth_year));
        content.push_str(&format!(
            "IgnoreStepCountCalories = {}\n",
            i32::from(default_profile.ignore_step_count_calories)
        ));
        content.push('\n');

        // Stats (for ScreenGameOver parity)
        let today = Local::now().date_naive().to_string();
        content.push_str("[Stats]\n");
        content.push_str(&format!("CaloriesBurnedDate = {today}\n"));
        content.push_str(&format!(
            "CaloriesBurnedToday = {}\n",
            default_profile.calories_burned_today
        ));
        content.push('\n');

        fs::write(profile_ini, content)?;
    }

    // Create groovestats.ini
    if !groovestats_ini.exists() {
        let mut content = String::new();

        content.push_str("[GrooveStats]\n");
        content.push_str("ApiKey = \n");
        content.push_str("IsPadPlayer = 0\n");
        content.push_str("Username = \n");
        content.push('\n');

        fs::write(groovestats_ini, content)?;
    }

    // Create arrowcloud.ini
    if !arrowcloud_ini.exists() {
        let mut content = String::new();

        content.push_str("[ArrowCloud]\n");
        content.push_str("ApiKey = \n");
        content.push('\n');

        fs::write(arrowcloud_ini, content)?;
    }

    Ok(())
}

fn save_profile_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let play_style = get_session_play_style();
    let profile = {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        profile.store_current_player_options(play_style);
        profile.clone()
    };
    let mut content = String::new();

    append_player_options_section(
        &mut content,
        player_options_section(PlayStyle::Single),
        &profile.player_options_singles,
    );
    append_player_options_section(
        &mut content,
        player_options_section(PlayStyle::Double),
        &profile.player_options_doubles,
    );

    content.push_str("[userprofile]\n");
    content.push_str(&format!("DisplayName={}\n", profile.display_name));
    content.push_str(&format!("PlayerInitials={}\n", profile.player_initials));
    content.push('\n');

    content.push_str("[Editable]\n");
    content.push_str(&format!("WeightPounds={}\n", profile.weight_pounds));
    content.push_str(&format!("BirthYear={}\n", profile.birth_year));
    content.push_str(&format!(
        "IgnoreStepCountCalories={}\n",
        i32::from(profile.ignore_step_count_calories)
    ));
    content.push('\n');

    append_last_played_section(
        &mut content,
        "LastPlayedSingles",
        &profile.last_played_singles,
    );
    append_last_played_section(
        &mut content,
        "LastPlayedDoubles",
        &profile.last_played_doubles,
    );
    append_last_played_course_section(
        &mut content,
        "LastPlayedCourseSingles",
        &profile.last_played_course_singles,
    );
    append_last_played_course_section(
        &mut content,
        "LastPlayedCourseDoubles",
        &profile.last_played_course_doubles,
    );

    content.push_str("[Stats]\n");
    content.push_str(&format!(
        "CaloriesBurnedDate={}\n",
        profile.calories_burned_day
    ));
    content.push_str(&format!(
        "CaloriesBurnedToday={}\n",
        profile.calories_burned_today
    ));
    content.push('\n');

    let path = profile_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

#[inline(always)]
fn decode_profile_stats(bytes: &[u8], path: &Path) -> Option<ProfileStats> {
    match decode_profile_stats_bytes(bytes) {
        Ok(stats) => Some(stats),
        Err(ProfileStatsDecodeError::UnsupportedVersion(version)) => {
            warn!(
                "Unsupported profile stats version {} in '{}'.",
                version,
                path.display()
            );
            None
        }
        Err(ProfileStatsDecodeError::InvalidPayload) => {
            warn!("Failed to decode profile stats '{}'.", path.display());
            None
        }
    }
}

fn load_profile_stats(path: &Path) -> Option<ProfileStats> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to read {}: {}", path.display(), e);
            }
            return None;
        }
    };
    decode_profile_stats(&bytes, path)
}

fn save_profile_stats_for_side(side: PlayerSide) {
    let maybe_payload = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => {
                let profile = lock_profiles()[side_ix(side)].clone();
                Some((
                    id.clone(),
                    ProfileStats {
                        current_combo: profile.current_combo,
                        known_pack_names: profile.known_pack_names,
                    },
                ))
            }
            ActiveProfile::Guest => None,
        }
    };
    let Some((profile_id, payload)) = maybe_payload else {
        return;
    };
    let Some(buf) = encode_profile_stats(&payload) else {
        warn!("Failed to encode profile stats for '{}'.", profile_id);
        return;
    };

    let path = profile_stats_path(&profile_id);
    let tmp_path = profile_stats_tmp_path(&profile_id);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(
            "Failed to create profile stats directory '{}': {}",
            parent.display(),
            e
        );
        return;
    }
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write {}: {}", tmp_path.display(), e);
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        warn!("Failed to save {}: {}", path.display(), e);
        let _ = fs::remove_file(&tmp_path);
    }
}

fn save_groovestats_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let profile = lock_profiles()[side_ix(side)].clone();
    let mut content = String::new();

    content.push_str("[GrooveStats]\n");
    content.push_str(&format!("ApiKey={}\n", profile.groovestats_api_key));
    content.push_str(&format!(
        "IsPadPlayer={}\n",
        if profile.groovestats_is_pad_player {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("Username={}\n", profile.groovestats_username));
    content.push('\n');

    let path = groovestats_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

fn save_arrowcloud_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let profile = lock_profiles()[side_ix(side)].clone();
    let mut content = String::new();

    content.push_str("[ArrowCloud]\n");
    content.push_str(&format!("ApiKey={}\n", profile.arrowcloud_api_key));
    content.push('\n');

    let path = arrowcloud_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Update the active profile's ArrowCloud API key (in memory + on disk).
/// No-op when the side has no local profile loaded (Guest).
pub fn set_arrowcloud_api_key_for_side(side: PlayerSide, api_key: &str) {
    {
        let mut profiles = lock_profiles();
        profiles[side_ix(side)].arrowcloud_api_key = api_key.to_string();
    }
    save_arrowcloud_ini_for_side(side);
}

/// Write a new ArrowCloud API key for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link ArrowCloud" flow where the user picks a profile that isn't
/// necessarily joined on P1 or P2.  Also refreshes the in-memory copy
/// on any session side currently loading that profile, so other screens
/// see the new key immediately.
pub fn set_arrowcloud_api_key_for_id(profile_id: &str, api_key: &str) {
    // Update any session side currently bound to this profile id.
    let matching_sides: Vec<PlayerSide> = {
        let session = lock_session();
        [PlayerSide::P1, PlayerSide::P2]
            .iter()
            .copied()
            .filter(|side| {
                matches!(
                    &session.active_profiles[side_ix(*side)],
                    ActiveProfile::Local { id } if id == profile_id
                )
            })
            .collect()
    };
    if !matching_sides.is_empty() {
        let mut profiles = lock_profiles();
        for side in &matching_sides {
            profiles[side_ix(*side)].arrowcloud_api_key = api_key.to_string();
        }
    }

    // Persist directly to that profile's ArrowCloud.ini, even if the
    // profile isn't loaded on any side right now.
    let mut content = String::new();
    content.push_str("[ArrowCloud]\n");
    content.push_str(&format!("ApiKey={api_key}\n"));
    content.push('\n');
    let path = arrowcloud_ini_path(profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Returns the saved ArrowCloud API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  Empty string if the profile has no key yet or the
/// file is missing / malformed.
pub fn get_arrowcloud_api_key_for_id(profile_id: &str) -> String {
    let path = arrowcloud_ini_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return String::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ApiKey=") {
            return rest.trim().to_string();
        }
        if let Some(rest) = line.strip_prefix("ApiKey =") {
            return rest.trim().to_string();
        }
    }
    String::new()
}

/// Update the active profile's GrooveStats credentials (API key,
/// username, and `IsPadPlayer=true` — Simply Love parity, see
/// `BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:46`) for
/// the given session side, persisting to its `GrooveStats.ini` on disk.
/// No-op when the side has no local profile loaded (Guest).
pub fn set_groovestats_credentials_for_side(side: PlayerSide, api_key: &str, username: &str) {
    {
        let mut profiles = lock_profiles();
        let p = &mut profiles[side_ix(side)];
        p.groovestats_api_key = api_key.to_string();
        p.groovestats_username = username.to_string();
        p.groovestats_is_pad_player = true;
    }
    save_groovestats_ini_for_side(side);
}

/// Write new GrooveStats credentials for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link GrooveStats" flow.  Also refreshes the in-memory copy on any
/// session side currently bound to that profile id.
pub fn set_groovestats_credentials_for_id(profile_id: &str, api_key: &str, username: &str) {
    let matching_sides: Vec<PlayerSide> = {
        let session = lock_session();
        [PlayerSide::P1, PlayerSide::P2]
            .iter()
            .copied()
            .filter(|side| {
                matches!(
                    &session.active_profiles[side_ix(*side)],
                    ActiveProfile::Local { id } if id == profile_id
                )
            })
            .collect()
    };
    if !matching_sides.is_empty() {
        let mut profiles = lock_profiles();
        for side in &matching_sides {
            let p = &mut profiles[side_ix(*side)];
            p.groovestats_api_key = api_key.to_string();
            p.groovestats_username = username.to_string();
            p.groovestats_is_pad_player = true;
        }
    }

    // Persist directly to that profile's GrooveStats.ini, even if the
    // profile isn't loaded on any side right now.
    let mut content = String::new();
    content.push_str("[GrooveStats]\n");
    content.push_str(&format!("ApiKey={api_key}\n"));
    content.push_str("IsPadPlayer=1\n");
    content.push_str(&format!("Username={username}\n"));
    content.push('\n');
    let path = groovestats_ini_path(profile_id);
    if let Err(e) = fs::write(&path, content) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Returns the saved GrooveStats API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  `None` if the profile has no key yet or the file is
/// missing / malformed; `Some` always wraps a non-empty trimmed key.
pub fn get_groovestats_api_key_for_id(profile_id: &str) -> Option<String> {
    let path = groovestats_ini_path(profile_id);
    let text = fs::read_to_string(&path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("ApiKey=")
            .or_else(|| line.strip_prefix("ApiKey ="));
        if let Some(rest) = rest {
            let key = rest.trim();
            if key.is_empty() {
                return None;
            }
            return Some(key.to_string());
        }
    }
    None
}

fn load_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };

    // If the requested profile folder no longer exists (e.g. the user renamed
    // the default folder on disk), fall back to the first available local
    // profile or Guest.
    let profile_id = match profile_id {
        Some(id) if !local_profile_dir(&id).is_dir() => {
            let fallback = scan_local_profiles().into_iter().next().map(|p| p.id);
            if let Some(ref fb_id) = fallback {
                info!("Profile folder '{id}' not found; falling back to '{fb_id}'.");
                let mut session = lock_session();
                session.active_profiles[side_ix(side)] = ActiveProfile::Local { id: fb_id.clone() };
            } else {
                info!("Profile folder '{id}' not found and no other profiles exist; using Guest.");
                let mut session = lock_session();
                session.active_profiles[side_ix(side)] = ActiveProfile::Guest;
            }
            fallback
        }
        other => other,
    };

    let Some(profile_id) = profile_id else {
        let mut profiles = lock_profiles();
        profiles[side_ix(side)] = make_guest_profile();
        return;
    };

    let profile_ini = profile_ini_path(&profile_id);
    let groovestats_ini = groovestats_ini_path(&profile_id);
    let arrowcloud_ini = arrowcloud_ini_path(&profile_id);
    if (!profile_ini.exists() || !groovestats_ini.exists() || !arrowcloud_ini.exists())
        && let Err(e) = ensure_local_profile_files(&profile_id)
    {
        warn!("Failed to create default profile files: {e}");
        // Proceed with default struct values and attempt to save them.
    }

    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let mut default_profile = Profile::default();
        default_profile.noteskin = machine_default_noteskin_value();
        default_profile.pad_light_brightness = machine_default_light_brightness();
        default_profile.store_current_player_options_for_all_styles();

        // Load profile.ini
        let mut profile_conf = SimpleIni::new();
        if profile_conf.load(&profile_ini).is_ok() {
            profile.display_name = profile_conf
                .get("userprofile", "DisplayName")
                .unwrap_or(default_profile.display_name.clone());
            profile.player_initials = profile_conf
                .get("userprofile", "PlayerInitials")
                .map(|initials| sanitize_player_initials(&initials))
                .filter(|initials| !initials.is_empty())
                .unwrap_or(default_profile.player_initials.clone());
            profile.player_options_singles = load_player_options(
                &profile_conf,
                player_options_section(PlayStyle::Single),
                &default_profile.player_options_singles,
            )
            .unwrap_or_else(|| default_profile.player_options_singles.clone());
            profile.player_options_doubles = load_player_options(
                &profile_conf,
                player_options_section(PlayStyle::Double),
                &default_profile.player_options_doubles,
            )
            .unwrap_or_else(|| default_profile.player_options_doubles.clone());
            profile.apply_player_options_for_style(get_session_play_style());

            // Optional last-played sections: keep the legacy [LastPlayed]
            // fallback so older profile.ini files still load cleanly.
            profile.last_played_singles = load_last_played(
                &profile_conf,
                "LastPlayedSingles",
                &default_profile.last_played_singles,
            )
            .or_else(|| {
                load_last_played(
                    &profile_conf,
                    "LastPlayed",
                    &default_profile.last_played_singles,
                )
            })
            .unwrap_or_else(|| default_profile.last_played_singles.clone());
            profile.last_played_doubles = load_last_played(
                &profile_conf,
                "LastPlayedDoubles",
                &default_profile.last_played_doubles,
            )
            .or_else(|| {
                load_last_played(
                    &profile_conf,
                    "LastPlayed",
                    &default_profile.last_played_doubles,
                )
            })
            .unwrap_or_else(|| default_profile.last_played_doubles.clone());
            profile.last_played_course_singles =
                load_last_played_course(&profile_conf, "LastPlayedCourseSingles")
                    .or_else(|| load_last_played_course(&profile_conf, "LastPlayedCourse"))
                    .unwrap_or_else(|| default_profile.last_played_course_singles.clone());
            profile.last_played_course_doubles =
                load_last_played_course(&profile_conf, "LastPlayedCourseDoubles")
                    .or_else(|| load_last_played_course(&profile_conf, "LastPlayedCourse"))
                    .unwrap_or_else(|| default_profile.last_played_course_doubles.clone());

            profile.weight_pounds = profile_conf
                .get("Editable", "WeightPounds")
                .and_then(|s| s.parse::<i32>().ok())
                .map(clamp_weight_pounds)
                .unwrap_or(default_profile.weight_pounds);

            profile.birth_year = profile_conf
                .get("Editable", "BirthYear")
                .and_then(|s| s.parse::<i32>().ok())
                .map(|year| year.max(0))
                .unwrap_or(default_profile.birth_year);

            // Profile stats (ScreenGameOver parity). Keep the legacy [Stats]
            // fallback so older profile.ini files still load cleanly.
            profile.ignore_step_count_calories = profile_conf
                .get("Editable", "IgnoreStepCountCalories")
                .or_else(|| profile_conf.get("Stats", "IgnoreStepCountCalories"))
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.ignore_step_count_calories, |v| v != 0);

            let today = Local::now().date_naive().to_string();
            let saved_day = profile_conf
                .get("Stats", "CaloriesBurnedDate")
                .unwrap_or_default();
            let saved_cals = profile_conf
                .get("Stats", "CaloriesBurnedToday")
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(default_profile.calories_burned_today);

            if saved_day.trim() == today {
                profile.calories_burned_day = today;
                profile.calories_burned_today = saved_cals;
            } else {
                profile.calories_burned_day = today;
                profile.calories_burned_today = 0.0;
            }
        } else {
            warn!(
                "Failed to load '{}', using default profile settings.",
                profile_ini.display()
            );
        }

        let stats =
            load_profile_stats(&profile_stats_path(&profile_id)).unwrap_or_else(|| ProfileStats {
                current_combo: default_profile.current_combo,
                known_pack_names: HashSet::new(),
            });
        profile.current_combo = stats.current_combo;
        profile.known_pack_names = stats.known_pack_names;
        profile.favorites = load_favorites(&profile_id);
        profile.favorited_packs = load_favorited_packs(&profile_id);

        // Load groovestats.ini
        let mut gs_conf = SimpleIni::new();
        if gs_conf.load(&groovestats_ini).is_ok() {
            profile.groovestats_api_key = gs_conf
                .get("GrooveStats", "ApiKey")
                .unwrap_or(default_profile.groovestats_api_key.clone());
            let is_pad_player = gs_conf.get("GrooveStats", "IsPadPlayer");
            profile.groovestats_is_pad_player = parse_groovestats_is_pad_player(
                is_pad_player.as_deref(),
                default_profile.groovestats_is_pad_player,
            );
            profile.groovestats_username = gs_conf
                .get("GrooveStats", "Username")
                .unwrap_or(default_profile.groovestats_username);
        } else {
            warn!(
                "Failed to load '{}', using default GrooveStats info.",
                groovestats_ini.display()
            );
        }

        // Load arrowcloud.ini
        let mut ac_conf = SimpleIni::new();
        if ac_conf.load(&arrowcloud_ini).is_ok() {
            profile.arrowcloud_api_key = ac_conf
                .get("ArrowCloud", "ApiKey")
                .unwrap_or(default_profile.arrowcloud_api_key.clone());
        } else {
            warn!(
                "Failed to load '{}', using default ArrowCloud info.",
                arrowcloud_ini.display()
            );
        }

        profile.avatar_path = find_profile_avatar_path(&local_profile_dir(&profile_id));
        profile.avatar_texture_key = None;
    } // Lock is released here.

    save_profile_ini_for_side(side);
    save_profile_stats_for_side(side);
    save_groovestats_ini_for_side(side);
    save_arrowcloud_ini_for_side(side);
    info!("Profile configuration files updated with default values for any missing fields.");
}

pub fn load() {
    load_for_side(PlayerSide::P1);
    load_for_side(PlayerSide::P2);
}

/// Returns a copy of the currently loaded profile data.
pub fn get() -> Profile {
    get_for_side(get_session_player_side())
}

pub fn get_for_side(side: PlayerSide) -> Profile {
    lock_profiles()[side_ix(side)].clone()
}

pub fn footer_fields_for_side(side: PlayerSide) -> (Option<String>, String) {
    let profiles = lock_profiles();
    let p = &profiles[side_ix(side)];
    (p.avatar_texture_key.clone(), p.display_name.clone())
}

pub fn groovestats_api_key_for_side(side: PlayerSide) -> String {
    lock_profiles()[side_ix(side)]
        .groovestats_api_key
        .trim()
        .to_string()
}

pub fn scorebox_fields_for_side(side: PlayerSide) -> (bool, bool, String, String, String) {
    let profiles = lock_profiles();
    let p = &profiles[side_ix(side)];
    (
        p.display_scorebox,
        p.show_ex_score,
        p.groovestats_api_key.clone(),
        p.arrowcloud_api_key.clone(),
        p.groovestats_username.clone(),
    )
}

pub fn gameplay_hud_snapshot() -> GameplayHudSnapshot {
    let (play_style, player_side, joined_mask, p1_guest, p2_guest) = {
        let session = lock_session();
        (
            session.play_style,
            session.player_side,
            session.joined_mask,
            active_profile_is_guest(&session.active_profiles[side_ix(PlayerSide::P1)]),
            active_profile_is_guest(&session.active_profiles[side_ix(PlayerSide::P2)]),
        )
    };
    let profiles = lock_profiles();
    let p1_profile = &profiles[side_ix(PlayerSide::P1)];
    let p2_profile = &profiles[side_ix(PlayerSide::P2)];
    GameplayHudSnapshot {
        play_style,
        player_side,
        p1: GameplayHudPlayerSnapshot {
            joined: player_side_is_joined(joined_mask, PlayerSide::P1),
            guest: p1_guest,
            display_name: p1_profile.display_name.clone(),
            avatar_texture_key: p1_profile.avatar_texture_key.clone(),
            hide_username: p1_profile.hide_username,
        },
        p2: GameplayHudPlayerSnapshot {
            joined: player_side_is_joined(joined_mask, PlayerSide::P2),
            guest: p2_guest,
            display_name: p2_profile.display_name.clone(),
            avatar_texture_key: p2_profile.avatar_texture_key.clone(),
            hide_username: p2_profile.hide_username,
        },
    }
}

pub fn set_avatar_texture_key_for_side(side: PlayerSide, key: Option<String>) {
    let mut profiles = lock_profiles();
    profiles[side_ix(side)].avatar_texture_key = key;
}

// --- Session helpers ---
pub fn get_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    lock_session().active_profiles[side_ix(side)].clone()
}

pub fn active_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    let session = lock_session();
    active_profile_local_id(&session.active_profiles[side_ix(side)]).map(str::to_owned)
}

/// The local profile that owns a given physical pad. `is_p2_side` is the pad's
/// player side (P2 vs P1), taken from its SDK slot (slot 1 = P2), NOT the raw
/// hardware jumper bit. In Doubles one player drives both pads, so both map to the
/// joined player's side; otherwise the pad maps to its own side.
pub fn active_local_profile_id_for_pad(is_p2_side: bool) -> Option<String> {
    let side = if get_session_play_style() == PlayStyle::Double {
        get_session_player_side()
    } else if is_p2_side {
        PlayerSide::P2
    } else {
        PlayerSide::P1
    };
    active_local_profile_id_for_side(side)
}

/// Pad-light brightness (0..=100) for the player on a given physical pad slot,
/// using the same side mapping as `active_local_profile_id_for_pad` (Doubles →
/// the one joined player for both pads; otherwise the pad's own side). Reads the
/// active profile's value (guest profiles are seeded from the machine default).
pub fn pad_light_brightness_for_pad(is_p2_side: bool) -> u8 {
    let side = if get_session_play_style() == PlayStyle::Double {
        get_session_player_side()
    } else if is_p2_side {
        PlayerSide::P2
    } else {
        PlayerSide::P1
    };
    lock_profiles()[side_ix(side)].pad_light_brightness
}

pub fn known_pack_names_for_local_profile(profile_id: &str) -> Option<HashSet<String>> {
    let session = lock_session();
    let profiles = lock_profiles();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let Some(id) = active_profile_local_id(&session.active_profiles[side_ix(side)]) else {
            continue;
        };
        if id == profile_id {
            return Some(profiles[side_ix(side)].known_pack_names.clone());
        }
    }
    None
}

pub fn mark_known_pack_names_for_local_profile<'a>(
    profile_id: &str,
    pack_names: impl IntoIterator<Item = &'a str>,
) {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_id.is_empty() || pack_names.is_empty() {
        return;
    }
    let save_side = {
        let session = lock_session();
        let mut profiles = lock_profiles();
        let mut save_side = None;
        for side in [PlayerSide::P1, PlayerSide::P2] {
            let Some(id) = active_profile_local_id(&session.active_profiles[side_ix(side)]) else {
                continue;
            };
            if id != profile_id {
                continue;
            }
            let profile = &mut profiles[side_ix(side)];
            let changed =
                add_known_pack_names(&mut profile.known_pack_names, pack_names.iter().copied());
            if changed && save_side.is_none() {
                save_side = Some(side);
            }
        }
        save_side
    };
    if let Some(side) = save_side {
        save_profile_stats_for_side(side);
    }
}

pub fn sync_known_packs(profile_ids: &[String], scanned_pack_names: &[String]) -> HashSet<String> {
    if profile_ids.is_empty() {
        return HashSet::new();
    }
    let mut out = HashSet::new();
    for profile_id in profile_ids {
        let known_pack_names = known_pack_names_for_local_profile(profile_id).unwrap_or_default();
        if known_pack_names.is_empty() && !scanned_pack_names.is_empty() {
            mark_known_pack_names_for_local_profile(
                profile_id,
                scanned_pack_names.iter().map(String::as_str),
            );
            continue;
        }
        out.extend(unknown_pack_names(&known_pack_names, scanned_pack_names));
    }
    out
}

pub fn mark_pack_known(profile_ids: &[String], name: &str) {
    mark_packs_known(profile_ids, std::iter::once(name));
}

pub fn mark_packs_known<'a>(profile_ids: &[String], pack_names: impl IntoIterator<Item = &'a str>) {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_ids.is_empty() || pack_names.is_empty() {
        return;
    }
    for profile_id in profile_ids {
        mark_known_pack_names_for_local_profile(profile_id, pack_names.iter().copied());
    }
}

// --- Favorites ---

fn favorites_path(profile_id: &str) -> PathBuf {
    dirs::app_dirs()
        .profiles_root()
        .join(profile_id)
        .join("favorites.txt")
}

fn load_favorites(profile_id: &str) -> HashSet<String> {
    let path = favorites_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return HashSet::new();
    };
    parse_favorites_content(&text)
}

fn save_favorites(profile_id: &str, favorites: &HashSet<String>) {
    let path = favorites_path(profile_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = render_favorites_content(favorites);
    let tmp_path = path.with_extension("tmp");
    if fs::write(&tmp_path, text.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp_path, &path);
    }
}

/// Toggle a song's favorite status for the given player side.
/// Returns `true` if the song is now a favorite, `false` if removed.
pub fn toggle_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    let Some(profile_id) = active_local_profile_id_for_side(side) else {
        return false;
    };
    let is_now_favorite = {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.favorites.contains(chart_hash) {
            profile.favorites.remove(chart_hash);
            false
        } else {
            profile.favorites.insert(chart_hash.to_string());
            true
        }
    };
    let favorites = lock_profiles()[side_ix(side)].favorites.clone();
    save_favorites(&profile_id, &favorites);
    is_now_favorite
}

/// Check if a chart hash is favorited for the given player side.
pub fn is_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    let profiles = lock_profiles();
    profiles[side_ix(side)].favorites.contains(chart_hash)
}

/// Test/bench helper: mark a chart hash as favorited for the given side in the
/// in-memory profile only, without persisting to disk. Lets benchmarks exercise
/// the favorites render path deterministically.
pub fn seed_session_favorite(side: PlayerSide, chart_hash: &str) {
    let mut profiles = lock_profiles();
    profiles[side_ix(side)]
        .favorites
        .insert(chart_hash.to_string());
}

fn favorited_packs_path(profile_id: &str) -> PathBuf {
    dirs::app_dirs()
        .profiles_root()
        .join(profile_id)
        .join("favorited_packs.txt")
}

fn load_favorited_packs(profile_id: &str) -> HashSet<String> {
    let path = favorited_packs_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return HashSet::new();
    };
    parse_favorited_packs_content(&text)
}

fn save_favorited_packs(profile_id: &str, packs: &HashSet<String>) {
    let path = favorited_packs_path(profile_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = render_favorited_packs_content(packs);
    let tmp_path = path.with_extension("tmp");
    if fs::write(&tmp_path, text.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp_path, &path);
    }
}

/// Toggle a pack's favorite status for the given player side, identifying the
/// pack by its display name. Returns `true` if the pack is
/// now a favorite, `false` if it was removed.
pub fn toggle_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    let Some(profile_id) = active_local_profile_id_for_side(side) else {
        return false;
    };
    let is_now_favorite = {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let existing = profile
            .favorited_packs
            .iter()
            .find(|p| *p == pack_name)
            .cloned();
        if let Some(existing) = existing {
            profile.favorited_packs.remove(&existing);
            false
        } else {
            profile.favorited_packs.insert(pack_name.to_string());
            true
        }
    };
    let packs = lock_profiles()[side_ix(side)].favorited_packs.clone();
    save_favorited_packs(&profile_id, &packs);
    is_now_favorite
}

/// Check if a pack name is favorited for the given player side.
pub fn is_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    let profiles = lock_profiles();
    profiles[side_ix(side)]
        .favorited_packs
        .iter()
        .any(|p| *p == pack_name)
}

/// Test/bench helper: mark a pack as favorited for the given side in the
/// in-memory profile only, without persisting to disk.
pub fn seed_session_favorited_pack(side: PlayerSide, pack_name: &str) {
    let mut profiles = lock_profiles();
    profiles[side_ix(side)]
        .favorited_packs
        .insert(pack_name.to_string());
}

pub fn set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> Profile {
    {
        let mut session = lock_session();
        let slot = &mut session.active_profiles[side_ix(side)];
        if *slot == profile {
            return get_for_side(side);
        }
        *slot = profile;
    }
    load_for_side(side);
    get_for_side(side)
}

pub fn set_active_profiles(p1: ActiveProfile, p2: ActiveProfile) -> [Profile; PLAYER_SLOTS] {
    let _ = set_active_profile_for_side(PlayerSide::P1, p1);
    let _ = set_active_profile_for_side(PlayerSide::P2, p2);
    [get_for_side(PlayerSide::P1), get_for_side(PlayerSide::P2)]
}

pub fn scan_local_profiles() -> Vec<LocalProfileSummary> {
    let root = dirs::app_dirs().profiles_root();
    let Ok(read_dir) = fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let Some(id) = entry
            .file_name()
            .to_str()
            .map(std::string::ToString::to_string)
        else {
            continue;
        };
        if !is_local_profile_id(&id) {
            continue;
        }

        let ini_path = entry.path().join("profile.ini");
        if !ini_path.is_file() {
            continue;
        }

        let mut display_name = id.clone();
        let mut ini = SimpleIni::new();
        if ini.load(&ini_path).is_ok()
            && let Some(name) = ini.get("userprofile", "DisplayName")
            && !name.trim().is_empty()
        {
            display_name = name;
        }

        let avatar_path = find_profile_avatar_path(&entry.path());

        out.push(LocalProfileSummary {
            id,
            display_name,
            avatar_path,
        });
    }

    out.sort_by(|a, b| cmp_profile_ids_case_insensitive(&a.id, &b.id));
    out
}

fn scan_local_profile_numbers() -> Vec<u32> {
    let root = dirs::app_dirs().profiles_root();
    let Ok(read_dir) = fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.len() != 8 {
            continue;
        }
        let Ok(n) = name.parse::<u32>() else {
            continue;
        };
        if n <= LOCAL_PROFILE_MAX_ID {
            out.push(n);
        }
    }
    out
}

fn allocate_local_profile_id() -> Result<String, std::io::Error> {
    next_local_profile_id(scan_local_profile_numbers())
        .ok_or_else(|| std::io::Error::other("Too many profiles"))
}

pub fn create_local_profile(display_name: &str) -> Result<String, std::io::Error> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let id = allocate_local_profile_id()?;
    let dir = local_profile_dir(&id);
    fs::create_dir_all(&dir)?;

    let mut default_profile = Profile::default();
    default_profile.noteskin = machine_default_noteskin_value();
    default_profile.pad_light_brightness = machine_default_light_brightness();
    default_profile.store_current_player_options_for_all_styles();
    let initials = initials_from_name(name);
    let mut content = String::new();
    append_player_options_section(
        &mut content,
        player_options_section(PlayStyle::Single),
        &default_profile.player_options_singles,
    );
    append_player_options_section(
        &mut content,
        player_options_section(PlayStyle::Double),
        &default_profile.player_options_doubles,
    );
    content.push_str("[userprofile]\n");
    content.push_str(&format!("DisplayName={name}\n"));
    content.push_str(&format!("PlayerInitials={initials}\n"));
    content.push('\n');

    content.push_str("[Editable]\n");
    content.push_str("WeightPounds=0\n");
    content.push_str("BirthYear=0\n");
    content.push_str("IgnoreStepCountCalories=0\n");
    content.push('\n');

    let today = Local::now().date_naive().to_string();
    content.push_str("[Stats]\n");
    content.push_str(&format!("CaloriesBurnedDate={today}\n"));
    content.push_str("CaloriesBurnedToday=0\n");
    content.push('\n');
    fs::write(profile_ini_path(&id), content)?;

    let mut gs = String::new();
    gs.push_str("[GrooveStats]\n");
    gs.push_str("ApiKey=\n");
    gs.push_str("IsPadPlayer=0\n");
    gs.push_str("Username=\n");
    gs.push('\n');
    fs::write(groovestats_ini_path(&id), gs)?;

    let mut ac = String::new();
    ac.push_str("[ArrowCloud]\n");
    ac.push_str("ApiKey=\n");
    ac.push('\n');
    fs::write(arrowcloud_ini_path(&id), ac)?;

    Ok(id)
}

fn rewrite_profile_display_name(path: &Path, display_name: &str) -> Result<(), std::io::Error> {
    let src = fs::read_to_string(path)?;
    fs::write(
        path,
        rewrite_profile_display_name_content(&src, display_name),
    )
}

pub fn rename_local_profile(id: &str, display_name: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }

    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let ini_path = profile_ini_path(id);
    if !ini_path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }
    rewrite_profile_display_name(&ini_path, name)?;

    let p1_active = active_local_profile_id_for_side(PlayerSide::P1)
        .as_deref()
        .is_some_and(|active_id| active_id == id);
    let p2_active = active_local_profile_id_for_side(PlayerSide::P2)
        .as_deref()
        .is_some_and(|active_id| active_id == id);
    if p1_active || p2_active {
        let mut profiles = lock_profiles();
        if p1_active {
            profiles[side_ix(PlayerSide::P1)].display_name = name.to_string();
        }
        if p2_active {
            profiles[side_ix(PlayerSide::P2)].display_name = name.to_string();
        }
    }

    Ok(())
}

pub fn delete_local_profile(id: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }

    let dir = local_profile_dir(id);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }

    fs::remove_dir_all(&dir)?;

    for side in [PlayerSide::P1, PlayerSide::P2] {
        let is_active = active_local_profile_id_for_side(side)
            .as_deref()
            .is_some_and(|active_id| active_id == id);
        if is_active {
            let _ = set_active_profile_for_side(side, ActiveProfile::Guest);
        }
    }

    Ok(())
}

pub fn get_session_music_rate() -> f32 {
    let s = lock_session();
    let r = s.music_rate;
    if r.is_finite() && r > 0.0 { r } else { 1.0 }
}

pub fn set_session_music_rate(rate: f32) {
    let mut s = lock_session();
    s.music_rate = if rate.is_finite() && rate > 0.0 {
        rate.clamp(0.5, 3.0)
    } else {
        1.0
    };
}

pub fn get_session_timing_tick_mode() -> TimingTickMode {
    lock_session().timing_tick_mode
}

pub fn set_session_timing_tick_mode(mode: TimingTickMode) {
    lock_session().timing_tick_mode = mode;
}

pub fn get_session_play_style() -> PlayStyle {
    lock_session().play_style
}

pub fn set_session_play_style(style: PlayStyle) {
    let prev_style = {
        let mut session = lock_session();
        let prev_style = session.play_style;
        if prev_style == style {
            return;
        }
        session.play_style = style;
        prev_style
    };

    let mut profiles = lock_profiles();
    for profile in profiles.iter_mut() {
        profile.store_current_player_options(prev_style);
        profile.apply_player_options_for_style(style);
    }
}

pub fn get_session_play_mode() -> PlayMode {
    lock_session().play_mode
}

pub fn set_session_play_mode(mode: PlayMode) {
    lock_session().play_mode = mode;
}

pub fn get_session_player_side() -> PlayerSide {
    lock_session().player_side
}

pub fn set_session_player_side(side: PlayerSide) {
    lock_session().player_side = side;
}

pub fn is_session_side_joined(side: PlayerSide) -> bool {
    let mask = lock_session().joined_mask;
    player_side_is_joined(mask, side)
}

pub fn is_session_side_guest(side: PlayerSide) -> bool {
    session_side_is_guest(side)
}

pub fn set_session_joined(p1: bool, p2: bool) {
    lock_session().joined_mask = joined_player_mask(p1, p2);
}

pub fn set_fast_profile_switch_from_select_music(enabled: bool) {
    lock_session().fast_profile_switch_from_select_music = enabled;
}

pub fn fast_profile_switch_from_select_music() -> bool {
    lock_session().fast_profile_switch_from_select_music
}

pub fn take_fast_profile_switch_from_select_music() -> bool {
    let mut session = lock_session();
    let was_set = session.fast_profile_switch_from_select_music;
    session.fast_profile_switch_from_select_music = false;
    was_set
}

#[cfg(test)]
mod tests {
    use super::{
        LastPlayed, LastPlayedCourse, MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorSize,
        MiniIndicatorSubtractiveDisplay, NoteSkin, PlayStyle, PlayerOptionsData, Profile,
        SimpleIni, TimingWindowsOption, append_player_options_section, load_player_options,
        parse_groovestats_is_pad_player, player_options_section,
    };
    use deadsync_profile::{
        DEFAULT_BIRTH_YEAR, DEFAULT_WEIGHT_POUNDS, ErrorBarMask, ErrorBarStyle,
        LiveTimingStatsMask, NoCmodAlternative, TapExplosionMask, error_bar_mask_from_style,
        error_bar_style_from_mask, error_bar_text_from_mask, normalize_tap_explosion_mask,
    };
    use std::str::FromStr;

    #[test]
    fn mini_indicator_style_settings_round_trip() {
        assert_eq!(
            MiniIndicatorSize::from_str(&MiniIndicatorSize::Default.to_string()).unwrap(),
            MiniIndicatorSize::Default
        );
        assert_eq!(
            MiniIndicatorSize::from_str(&MiniIndicatorSize::Large.to_string()).unwrap(),
            MiniIndicatorSize::Large
        );
        assert_eq!(
            MiniIndicatorColor::from_str(&MiniIndicatorColor::Default.to_string()).unwrap(),
            MiniIndicatorColor::Default
        );
        assert_eq!(
            MiniIndicatorColor::from_str(&MiniIndicatorColor::Detailed.to_string()).unwrap(),
            MiniIndicatorColor::Detailed
        );
        assert_eq!(
            MiniIndicatorColor::from_str(&MiniIndicatorColor::Combo.to_string()).unwrap(),
            MiniIndicatorColor::Combo
        );
        assert_eq!(
            MiniIndicatorSubtractiveDisplay::from_str(
                &MiniIndicatorSubtractiveDisplay::Points.to_string(),
            )
            .unwrap(),
            MiniIndicatorSubtractiveDisplay::Points
        );
        assert_eq!(
            MiniIndicatorPosition::from_str(&MiniIndicatorPosition::UnderUpArrow.to_string())
                .unwrap(),
            MiniIndicatorPosition::UnderUpArrow
        );
    }

    #[test]
    fn no_cmod_alternative_round_trips_through_player_options_ini() {
        let section = player_options_section(PlayStyle::Single);
        for alt in [
            NoCmodAlternative::None,
            NoCmodAlternative::XMod,
            NoCmodAlternative::MMod,
        ] {
            let options = PlayerOptionsData {
                no_cmod_alternative: alt,
                ..PlayerOptionsData::default()
            };
            let mut content = String::new();
            append_player_options_section(&mut content, section, &options);

            let mut ini = SimpleIni::new();
            ini.load_str(&content);
            let loaded = load_player_options(&ini, section, &PlayerOptionsData::default())
                .expect("section has keys, so it should load");

            assert_eq!(loaded.no_cmod_alternative, alt);
        }
    }

    #[test]
    fn mini_indicator_style_defaults_preserve_legacy_look() {
        let profile = Profile::default();
        assert_eq!(profile.mini_indicator_size, MiniIndicatorSize::Default);
        assert_eq!(profile.mini_indicator_color, MiniIndicatorColor::Default);
        assert_eq!(
            profile.mini_indicator_subtractive_display,
            MiniIndicatorSubtractiveDisplay::Percent
        );
        assert_eq!(
            profile.mini_indicator_position,
            MiniIndicatorPosition::Default
        );
    }

    #[test]
    fn groovestats_is_pad_player_requires_explicit_one() {
        assert!(parse_groovestats_is_pad_player(Some("1"), false));
        assert!(!parse_groovestats_is_pad_player(Some("0"), false));
        assert!(!parse_groovestats_is_pad_player(Some("2"), false));
        assert!(!parse_groovestats_is_pad_player(Some("255"), false));
    }

    #[test]
    fn groovestats_is_pad_player_uses_default_on_invalid_value() {
        assert!(parse_groovestats_is_pad_player(None, true));
        assert!(!parse_groovestats_is_pad_player(None, false));
        assert!(parse_groovestats_is_pad_player(Some("abc"), true));
        assert!(!parse_groovestats_is_pad_player(Some("abc"), false));
    }

    #[test]
    fn calculated_weight_pounds_uses_itg_default_when_unset() {
        assert_eq!(
            Profile::default().calculated_weight_pounds(),
            DEFAULT_WEIGHT_POUNDS
        );
        assert_eq!(
            Profile {
                weight_pounds: 165,
                ..Profile::default()
            }
            .calculated_weight_pounds(),
            165
        );
    }

    #[test]
    fn age_years_for_uses_birth_year_or_default() {
        assert_eq!(
            Profile::default().age_years_for(2026),
            2026 - DEFAULT_BIRTH_YEAR
        );
        assert_eq!(
            Profile {
                birth_year: 2000,
                ..Profile::default()
            }
            .age_years_for(2026),
            26
        );
    }

    #[test]
    fn last_played_uses_singles_for_single_and_versus() {
        let singles = LastPlayed {
            song_music_path: Some("single.ogg".to_string()),
            chart_hash: Some("singlehash".to_string()),
            difficulty_index: 3,
        };
        let doubles = LastPlayed {
            song_music_path: Some("double.ogg".to_string()),
            chart_hash: Some("doublehash".to_string()),
            difficulty_index: 7,
        };
        let profile = Profile {
            last_played_singles: singles.clone(),
            last_played_doubles: doubles.clone(),
            ..Profile::default()
        };

        assert_eq!(profile.last_played(PlayStyle::Single), &singles);
        assert_eq!(profile.last_played(PlayStyle::Versus), &singles);
        assert_eq!(profile.last_played(PlayStyle::Double), &doubles);
    }

    #[test]
    fn last_played_course_uses_singles_for_single_and_versus() {
        let singles = LastPlayedCourse {
            course_path: Some("Courses/Single.crs".to_string()),
            difficulty_name: Some("Hard".to_string()),
        };
        let doubles = LastPlayedCourse {
            course_path: Some("Courses/Double.crs".to_string()),
            difficulty_name: Some("Challenge".to_string()),
        };
        let profile = Profile {
            last_played_course_singles: singles.clone(),
            last_played_course_doubles: doubles.clone(),
            ..Profile::default()
        };

        assert_eq!(profile.last_played_course(PlayStyle::Single), &singles);
        assert_eq!(profile.last_played_course(PlayStyle::Versus), &singles);
        assert_eq!(profile.last_played_course(PlayStyle::Double), &doubles);
    }

    #[test]
    fn player_options_use_singles_for_single_and_versus() {
        let mut profile = Profile::default();
        profile.mini_percent = 12;
        profile.global_offset_shift_ms = 9;
        profile.store_current_player_options(PlayStyle::Single);
        profile.mini_percent = 48;
        profile.global_offset_shift_ms = -11;
        profile.store_current_player_options(PlayStyle::Double);

        assert_eq!(profile.player_options(PlayStyle::Single).mini_percent, 12);
        assert_eq!(profile.player_options(PlayStyle::Versus).mini_percent, 12);
        assert_eq!(profile.player_options(PlayStyle::Double).mini_percent, 48);
        assert_eq!(
            profile
                .player_options(PlayStyle::Single)
                .global_offset_shift_ms,
            9
        );
        assert_eq!(
            profile
                .player_options(PlayStyle::Versus)
                .global_offset_shift_ms,
            9
        );
        assert_eq!(
            profile
                .player_options(PlayStyle::Double)
                .global_offset_shift_ms,
            -11
        );
    }

    #[test]
    fn apply_player_options_for_style_restores_separate_snapshots() {
        let mut profile = Profile::default();
        profile.mini_percent = 18;
        profile.show_ex_score = true;
        profile.score_position = deadsync_profile::ScorePosition::StepStatistics;
        profile.score_display_mode = deadsync_profile::ScoreDisplayMode::Predictive;
        profile.global_offset_shift_ms = 7;
        profile.timing_windows = TimingWindowsOption::WayOffs;
        profile.receptor_noteskin = Some(NoteSkin::new("default"));
        profile.tap_explosion_noteskin = Some(NoteSkin::new("metal"));
        profile.tap_explosion_active_mask =
            TapExplosionMask::all().difference(TapExplosionMask::HELD);
        profile.store_current_player_options(PlayStyle::Single);

        profile.mini_percent = 62;
        profile.show_ex_score = false;
        profile.score_position = deadsync_profile::ScorePosition::Normal;
        profile.score_display_mode = deadsync_profile::ScoreDisplayMode::Normal;
        profile.global_offset_shift_ms = -13;
        profile.timing_windows = TimingWindowsOption::FantasticsAndExcellents;
        profile.receptor_noteskin = Some(NoteSkin::new("cyber"));
        profile.tap_explosion_noteskin = None;
        profile.tap_explosion_active_mask = TapExplosionMask::HELD;
        profile.store_current_player_options(PlayStyle::Double);

        profile.apply_player_options_for_style(PlayStyle::Single);
        assert_eq!(profile.mini_percent, 18);
        assert!(profile.show_ex_score);
        assert_eq!(
            profile.score_position,
            deadsync_profile::ScorePosition::StepStatistics
        );
        assert_eq!(
            profile.score_display_mode,
            deadsync_profile::ScoreDisplayMode::Predictive
        );
        assert_eq!(profile.global_offset_shift_ms, 7);
        assert_eq!(profile.timing_windows, TimingWindowsOption::WayOffs);
        assert_eq!(profile.receptor_noteskin, Some(NoteSkin::new("default")));
        assert_eq!(profile.tap_explosion_noteskin, Some(NoteSkin::new("metal")));
        assert_eq!(
            profile.tap_explosion_active_mask,
            TapExplosionMask::all().difference(TapExplosionMask::HELD)
        );

        profile.apply_player_options_for_style(PlayStyle::Double);
        assert_eq!(profile.mini_percent, 62);
        assert!(!profile.show_ex_score);
        assert_eq!(
            profile.score_position,
            deadsync_profile::ScorePosition::Normal
        );
        assert_eq!(
            profile.score_display_mode,
            deadsync_profile::ScoreDisplayMode::Normal
        );
        assert_eq!(profile.global_offset_shift_ms, -13);
        assert_eq!(
            profile.timing_windows,
            TimingWindowsOption::FantasticsAndExcellents
        );
        assert_eq!(profile.receptor_noteskin, Some(NoteSkin::new("cyber")));
        assert_eq!(profile.tap_explosion_noteskin, None);
        assert_eq!(profile.tap_explosion_active_mask, TapExplosionMask::HELD);
    }

    #[test]
    fn tap_explosion_none_choice_disables_resolution() {
        let profile = Profile {
            tap_explosion_noteskin: Some(NoteSkin::none_choice()),
            ..Profile::default()
        };

        assert!(profile.tap_explosion_noteskin_hidden());
        assert_eq!(profile.resolved_tap_explosion_noteskin(), None);
    }

    #[test]
    fn tap_explosion_mask_migrates_new_bits_from_old_profiles() {
        let old_all = TapExplosionMask::FANTASTIC
            | TapExplosionMask::EXCELLENT
            | TapExplosionMask::GREAT
            | TapExplosionMask::DECENT
            | TapExplosionMask::WAY_OFF
            | TapExplosionMask::HELD;

        assert_eq!(
            normalize_tap_explosion_mask(old_all.bits(), 1),
            TapExplosionMask::all()
        );
        assert_eq!(normalize_tap_explosion_mask(old_all.bits(), 2), old_all);
    }

    #[test]
    fn tap_explosion_miss_window_uses_miss_mask() {
        let mut profile = Profile::default();
        assert!(profile.tap_explosion_window_enabled("Miss"));

        profile
            .tap_explosion_active_mask
            .remove(TapExplosionMask::MISS);
        assert!(!profile.tap_explosion_window_enabled("Miss"));
        assert!(profile.tap_explosion_window_enabled("Held"));
    }

    #[test]
    fn persisted_row_mask_bit_layouts_are_stable() {
        use super::{
            AccelEffectsMask, AppearanceEffectsMask, HoldsMask, InsertMask, RemoveMask,
            VisualEffectsMask,
        };

        // InsertMask: persisted bits 0..=6 (Mines is runtime-only and
        // intentionally not represented here).
        assert_eq!(InsertMask::WIDE.bits(), 1 << 0);
        assert_eq!(InsertMask::BIG.bits(), 1 << 1);
        assert_eq!(InsertMask::QUICK.bits(), 1 << 2);
        assert_eq!(InsertMask::BMRIZE.bits(), 1 << 3);
        assert_eq!(InsertMask::SKIPPY.bits(), 1 << 4);
        assert_eq!(InsertMask::ECHO.bits(), 1 << 5);
        assert_eq!(InsertMask::STOMP.bits(), 1 << 6);
        assert_eq!(InsertMask::all().bits(), 0b0111_1111);

        // RemoveMask: bits 0..=7
        assert_eq!(RemoveMask::LITTLE.bits(), 1 << 0);
        assert_eq!(RemoveMask::NO_MINES.bits(), 1 << 1);
        assert_eq!(RemoveMask::NO_HOLDS.bits(), 1 << 2);
        assert_eq!(RemoveMask::NO_JUMPS.bits(), 1 << 3);
        assert_eq!(RemoveMask::NO_HANDS.bits(), 1 << 4);
        assert_eq!(RemoveMask::NO_QUADS.bits(), 1 << 5);
        assert_eq!(RemoveMask::NO_LIFTS.bits(), 1 << 6);
        assert_eq!(RemoveMask::NO_FAKES.bits(), 1 << 7);
        assert_eq!(RemoveMask::all().bits(), 0xFF);

        assert_eq!(HoldsMask::PLANTED.bits(), 1 << 0);
        assert_eq!(HoldsMask::FLOORED.bits(), 1 << 1);
        assert_eq!(HoldsMask::TWISTER.bits(), 1 << 2);
        assert_eq!(HoldsMask::NO_ROLLS.bits(), 1 << 3);
        assert_eq!(HoldsMask::HOLDS_TO_ROLLS.bits(), 1 << 4);
        assert_eq!(HoldsMask::all().bits(), 0b0001_1111);

        assert_eq!(AccelEffectsMask::BOOST.bits(), 1 << 0);
        assert_eq!(AccelEffectsMask::BRAKE.bits(), 1 << 1);
        assert_eq!(AccelEffectsMask::WAVE.bits(), 1 << 2);
        assert_eq!(AccelEffectsMask::EXPAND.bits(), 1 << 3);
        assert_eq!(AccelEffectsMask::BOOMERANG.bits(), 1 << 4);
        assert_eq!(AccelEffectsMask::all().bits(), 0b0001_1111);

        assert_eq!(VisualEffectsMask::DRUNK.bits(), 1 << 0);
        assert_eq!(VisualEffectsMask::DIZZY.bits(), 1 << 1);
        assert_eq!(VisualEffectsMask::CONFUSION.bits(), 1 << 2);
        assert_eq!(VisualEffectsMask::BIG.bits(), 1 << 3);
        assert_eq!(VisualEffectsMask::FLIP.bits(), 1 << 4);
        assert_eq!(VisualEffectsMask::INVERT.bits(), 1 << 5);
        assert_eq!(VisualEffectsMask::TORNADO.bits(), 1 << 6);
        assert_eq!(VisualEffectsMask::TIPSY.bits(), 1 << 7);
        assert_eq!(VisualEffectsMask::BUMPY.bits(), 1 << 8);
        assert_eq!(VisualEffectsMask::BEAT.bits(), 1 << 9);
        assert_eq!(VisualEffectsMask::all().bits(), 0b11_1111_1111);

        assert_eq!(AppearanceEffectsMask::HIDDEN.bits(), 1 << 0);
        assert_eq!(AppearanceEffectsMask::SUDDEN.bits(), 1 << 1);
        assert_eq!(AppearanceEffectsMask::STEALTH.bits(), 1 << 2);
        assert_eq!(AppearanceEffectsMask::BLINK.bits(), 1 << 3);
        assert_eq!(AppearanceEffectsMask::RANDOM_VANISH.bits(), 1 << 4);
        assert_eq!(AppearanceEffectsMask::all().bits(), 0b0001_1111);

        assert_eq!(ErrorBarMask::COLORFUL.bits(), 1 << 0);
        assert_eq!(ErrorBarMask::MONOCHROME.bits(), 1 << 1);
        assert_eq!(ErrorBarMask::TEXT.bits(), 1 << 2);
        assert_eq!(ErrorBarMask::HIGHLIGHT.bits(), 1 << 3);
        assert_eq!(ErrorBarMask::AVERAGE.bits(), 1 << 4);
        assert_eq!(ErrorBarMask::all().bits(), 0b0001_1111);

        assert_eq!(LiveTimingStatsMask::MEAN.bits(), 1 << 0);
        assert_eq!(LiveTimingStatsMask::MEAN_ABS.bits(), 1 << 1);
        assert_eq!(LiveTimingStatsMask::MAX.bits(), 1 << 2);
        assert_eq!(LiveTimingStatsMask::all().bits(), 0b0000_0111);

        assert_eq!(TapExplosionMask::FANTASTIC.bits(), 1 << 0);
        assert_eq!(TapExplosionMask::EXCELLENT.bits(), 1 << 1);
        assert_eq!(TapExplosionMask::GREAT.bits(), 1 << 2);
        assert_eq!(TapExplosionMask::DECENT.bits(), 1 << 3);
        assert_eq!(TapExplosionMask::WAY_OFF.bits(), 1 << 4);
        assert_eq!(TapExplosionMask::HELD.bits(), 1 << 5);
        assert_eq!(TapExplosionMask::MISS.bits(), 1 << 6);
        assert_eq!(TapExplosionMask::HOLDING.bits(), 1 << 7);
        assert_eq!(TapExplosionMask::all().bits(), 0xFF);
    }

    #[test]
    fn from_bits_truncate_drops_unrepresented_bits() {
        use super::{InsertMask, VisualEffectsMask};

        // InsertMask only persists 7 bits; bit 7 (Mines) belongs to runtime.
        assert_eq!(InsertMask::from_bits_truncate(0xFF), InsertMask::all());
        assert_eq!(InsertMask::from_bits_truncate(0xFF).bits(), 0b0111_1111);

        // VisualEffectsMask is 10 bits in a u16.
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX),
            VisualEffectsMask::all()
        );
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX).bits(),
            0b11_1111_1111
        );
    }

    #[test]
    fn error_bar_helpers_roundtrip_through_mask() {
        // Style + text combine into mask bits.
        let mask = error_bar_mask_from_style(ErrorBarStyle::Colorful, true);
        assert!(mask.contains(ErrorBarMask::COLORFUL));
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);
        assert!(error_bar_text_from_mask(mask));

        // Style precedence: Colorful > Monochrome > Highlight > Average > None.
        let mask = ErrorBarMask::COLORFUL | ErrorBarMask::MONOCHROME;
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);

        // Text-only mask round-trips to (Style::None, text=true) — the legacy
        // canonicalization quirk preserved by the typed helpers.
        let mask = error_bar_mask_from_style(ErrorBarStyle::Text, false);
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert!(!mask.contains(ErrorBarMask::COLORFUL));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(error_bar_text_from_mask(mask));

        // Empty mask means no error bar at all.
        let mask = error_bar_mask_from_style(ErrorBarStyle::None, false);
        assert!(mask.is_empty());
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(!error_bar_text_from_mask(mask));
    }
}
