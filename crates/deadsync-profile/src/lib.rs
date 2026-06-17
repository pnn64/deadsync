use bincode::{Decode, Encode};
use bitflags::bitflags;
use chrono::{Datelike, Local};
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_score::ScoreImportEndpoint;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub mod pad_config;

pub const PLAYER_SLOTS: usize = 2;
pub const DEFAULT_PROFILE_ID: &str = "00000000";
pub const LOCAL_PROFILE_MAX_ID: u32 = 99_999_999;
pub const SESSION_JOINED_MASK_P1: u8 = 1 << 0;
pub const SESSION_JOINED_MASK_P2: u8 = 1 << 1;
pub const DEFAULT_WEIGHT_POUNDS: i32 = 120;
pub const DEFAULT_BIRTH_YEAR: i32 = 1995;
pub const PLAYER_INITIALS_MAX_LEN: usize = 4;
pub const HUD_OFFSET_MIN: i32 = -250;
pub const HUD_OFFSET_MAX: i32 = 250;
pub const SPACING_PERCENT_MIN: i32 = -100;
pub const SPACING_PERCENT_MAX: i32 = 100;
pub const MINI_PERCENT_MIN: i32 = -100;
pub const MINI_PERCENT_MAX: i32 = 150;
pub const NOTE_FIELD_OFFSET_X_MIN: i32 = 0;
pub const NOTE_FIELD_OFFSET_X_MAX: i32 = 50;
pub const NOTE_FIELD_OFFSET_Y_MIN: i32 = -50;
pub const NOTE_FIELD_OFFSET_Y_MAX: i32 = 50;
pub const VISUAL_DELAY_MS_MIN: i32 = -100;
pub const VISUAL_DELAY_MS_MAX: i32 = 100;
pub const TILT_THRESHOLD_MIN_MS: u32 = 0;
pub const TILT_THRESHOLD_MAX_MS: u32 = 100;
pub const TILT_MIN_THRESHOLD_DEFAULT_MS: u32 = 0;
pub const TILT_MAX_THRESHOLD_DEFAULT_MS: u32 = 50;
pub const LONG_ERROR_BAR_INTENSITY_MIN: f32 = 1.0;
pub const LONG_ERROR_BAR_INTENSITY_MAX: f32 = 4.0;
pub const LONG_ERROR_BAR_INTENSITY_STEP: f32 = 0.25;
pub const LONG_ERROR_BAR_INTENSITY_DEFAULT: f32 = 2.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_MIN: f32 = 1.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_MAX: f32 = 2.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_STEP: f32 = 0.25;
pub const AVERAGE_ERROR_BAR_INTENSITY_DEFAULT: f32 = 1.0;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MIN: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MAX: u32 = 2000;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_STEP: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT: u32 = 400;
pub const LONG_ERROR_BAR_THRESHOLD_MS_MIN: u32 = 1;
pub const LONG_ERROR_BAR_THRESHOLD_MS_MAX: u32 = 15;
pub const LONG_ERROR_BAR_THRESHOLD_MS_DEFAULT: u32 = 4;
pub const LONG_ERROR_BAR_MIN_SAMPLES_MIN: u32 = 4;
pub const LONG_ERROR_BAR_MIN_SAMPLES_MAX: u32 = 64;
pub const LONG_ERROR_BAR_MIN_SAMPLES_DEFAULT: u32 = 16;
pub const CUSTOM_FANTASTIC_WINDOW_MIN_MS: u8 = 1;
pub const CUSTOM_FANTASTIC_WINDOW_MAX_MS: u8 = 22;
pub const CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS: u8 = 10;

/// Fallback pad-light brightness (0..=100) when a profile has no saved value.
/// New profiles are seeded from the StepManiaX machine default instead (see
/// `game::profile`); this is only the in-crate default for a fresh struct.
pub const PAD_LIGHT_BRIGHTNESS_DEFAULT: u8 = 100;

/// Clamp a pad-light brightness to the valid 0..=100 percent range.
#[inline(always)]
pub const fn clamp_pad_light_brightness(percent: u8) -> u8 {
    if percent > 100 { 100 } else { percent }
}
pub const TEXT_ERROR_BAR_THRESHOLD_MS_MIN: u32 = 1;
pub const TEXT_ERROR_BAR_THRESHOLD_MS_MAX: u32 = 50;
pub const TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT: u32 = 10;
pub const TAP_EXPLOSION_MASK_VERSION: u8 = 2;
pub const DEFAULT_COLUMN_FLASH_MASK: ColumnFlashMask = ColumnFlashMask::MISS;

#[inline(always)]
pub const fn clamp_weight_pounds(weight_pounds: i32) -> i32 {
    if weight_pounds == 0 {
        0
    } else if weight_pounds < 20 {
        20
    } else if weight_pounds > 1000 {
        1000
    } else {
        weight_pounds
    }
}

#[inline(always)]
pub const fn resolved_weight_pounds(weight_pounds: i32) -> i32 {
    if weight_pounds == 0 {
        DEFAULT_WEIGHT_POUNDS
    } else {
        weight_pounds
    }
}

#[inline(always)]
pub const fn age_years_for_birth_year(birth_year: i32, current_year: i32) -> i32 {
    if birth_year == 0 {
        current_year - DEFAULT_BIRTH_YEAR
    } else {
        current_year - birth_year
    }
}

#[inline]
fn set_i32_if_changed(value: &mut i32, new_value: i32) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_f32_if_changed(value: &mut f32, new_value: f32) -> bool {
    if (*value - new_value).abs() < 1e-6 {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_u32_if_changed(value: &mut u32, new_value: u32) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_u8_if_changed(value: &mut u8, new_value: u8) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline(always)]
pub fn tap_explosion_mask_for_window(window: &str) -> Option<TapExplosionMask> {
    match window {
        "W0" | "W1" => Some(TapExplosionMask::FANTASTIC),
        "W2" => Some(TapExplosionMask::EXCELLENT),
        "W3" => Some(TapExplosionMask::GREAT),
        "W4" => Some(TapExplosionMask::DECENT),
        "W5" => Some(TapExplosionMask::WAY_OFF),
        "Miss" => Some(TapExplosionMask::MISS),
        "Held" => Some(TapExplosionMask::HELD),
        _ => None,
    }
}

#[inline(always)]
pub fn tap_explosion_mask_enabled(mask: TapExplosionMask, window: &str) -> bool {
    let Some(flag) = tap_explosion_mask_for_window(window) else {
        return false;
    };
    mask.contains(flag)
}

#[inline(always)]
pub fn normalize_tap_explosion_mask(bits: u8, version: u8) -> TapExplosionMask {
    let mut mask = TapExplosionMask::from_bits_truncate(bits);
    if version < TAP_EXPLOSION_MASK_VERSION {
        mask.insert(TapExplosionMask::MISS | TapExplosionMask::HOLDING);
    }
    mask
}

#[inline(always)]
pub const fn column_flash_mask_for_grade(
    grade: JudgeGrade,
    blue_fantastic: bool,
) -> ColumnFlashMask {
    match grade {
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                ColumnFlashMask::BLUE_FANTASTIC
            } else {
                ColumnFlashMask::WHITE_FANTASTIC
            }
        }
        JudgeGrade::Excellent => ColumnFlashMask::EXCELLENT,
        JudgeGrade::Great => ColumnFlashMask::GREAT,
        JudgeGrade::Decent => ColumnFlashMask::DECENT,
        JudgeGrade::WayOff => ColumnFlashMask::WAY_OFF,
        JudgeGrade::Miss => ColumnFlashMask::MISS,
    }
}

#[inline(always)]
pub const fn column_flash_mask_enabled(
    mask: ColumnFlashMask,
    grade: JudgeGrade,
    blue_fantastic: bool,
) -> bool {
    mask.contains(column_flash_mask_for_grade(grade, blue_fantastic))
}

#[inline(always)]
pub const fn clamp_tilt_threshold_ms(ms: u32) -> u32 {
    if ms > TILT_THRESHOLD_MAX_MS {
        TILT_THRESHOLD_MAX_MS
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_long_error_bar_threshold_ms(ms: u32) -> u32 {
    if ms < LONG_ERROR_BAR_THRESHOLD_MS_MIN {
        LONG_ERROR_BAR_THRESHOLD_MS_MIN
    } else if ms > LONG_ERROR_BAR_THRESHOLD_MS_MAX {
        LONG_ERROR_BAR_THRESHOLD_MS_MAX
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_text_error_bar_threshold_ms(ms: u32) -> u32 {
    if ms < TEXT_ERROR_BAR_THRESHOLD_MS_MIN {
        TEXT_ERROR_BAR_THRESHOLD_MS_MIN
    } else if ms > TEXT_ERROR_BAR_THRESHOLD_MS_MAX {
        TEXT_ERROR_BAR_THRESHOLD_MS_MAX
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_long_error_bar_min_samples(n: u32) -> u32 {
    if n < LONG_ERROR_BAR_MIN_SAMPLES_MIN {
        LONG_ERROR_BAR_MIN_SAMPLES_MIN
    } else if n > LONG_ERROR_BAR_MIN_SAMPLES_MAX {
        LONG_ERROR_BAR_MIN_SAMPLES_MAX
    } else {
        n
    }
}

#[inline]
pub fn clamp_long_error_bar_intensity(value: f32) -> f32 {
    if !value.is_finite() {
        return LONG_ERROR_BAR_INTENSITY_DEFAULT;
    }
    let clamped = value.clamp(LONG_ERROR_BAR_INTENSITY_MIN, LONG_ERROR_BAR_INTENSITY_MAX);
    let steps = ((clamped - LONG_ERROR_BAR_INTENSITY_MIN) / LONG_ERROR_BAR_INTENSITY_STEP).round();
    (LONG_ERROR_BAR_INTENSITY_MIN + steps * LONG_ERROR_BAR_INTENSITY_STEP)
        .clamp(LONG_ERROR_BAR_INTENSITY_MIN, LONG_ERROR_BAR_INTENSITY_MAX)
}

#[inline]
pub fn clamp_average_error_bar_intensity(value: f32) -> f32 {
    if !value.is_finite() {
        return AVERAGE_ERROR_BAR_INTENSITY_DEFAULT;
    }
    let clamped = value.clamp(
        AVERAGE_ERROR_BAR_INTENSITY_MIN,
        AVERAGE_ERROR_BAR_INTENSITY_MAX,
    );
    let steps =
        ((clamped - AVERAGE_ERROR_BAR_INTENSITY_MIN) / AVERAGE_ERROR_BAR_INTENSITY_STEP).round();
    (AVERAGE_ERROR_BAR_INTENSITY_MIN + steps * AVERAGE_ERROR_BAR_INTENSITY_STEP).clamp(
        AVERAGE_ERROR_BAR_INTENSITY_MIN,
        AVERAGE_ERROR_BAR_INTENSITY_MAX,
    )
}

#[inline]
pub const fn clamp_average_error_bar_interval_ms(ms: u32) -> u32 {
    let clamped = if ms < AVERAGE_ERROR_BAR_INTERVAL_MS_MIN {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
    } else if ms > AVERAGE_ERROR_BAR_INTERVAL_MS_MAX {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
    } else {
        ms
    };
    let steps = (clamped - AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        + AVERAGE_ERROR_BAR_INTERVAL_MS_STEP / 2)
        / AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;
    AVERAGE_ERROR_BAR_INTERVAL_MS_MIN + steps * AVERAGE_ERROR_BAR_INTERVAL_MS_STEP
}

#[inline(always)]
pub const fn clamp_custom_fantastic_window_ms(ms: u8) -> u8 {
    if ms < CUSTOM_FANTASTIC_WINDOW_MIN_MS {
        CUSTOM_FANTASTIC_WINDOW_MIN_MS
    } else if ms > CUSTOM_FANTASTIC_WINDOW_MAX_MS {
        CUSTOM_FANTASTIC_WINDOW_MAX_MS
    } else {
        ms
    }
}

pub fn sanitize_player_initials(raw: &str) -> String {
    let mut out = String::with_capacity(PLAYER_INITIALS_MAX_LEN);
    for ch in raw.chars() {
        if out.len() >= PLAYER_INITIALS_MAX_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '?' || ch == '!' {
            out.push(ch.to_ascii_uppercase());
        }
    }
    out
}

pub fn initials_from_name(name: &str) -> String {
    let mut out = sanitize_player_initials(name);
    match out.len() {
        0 => "??".to_string(),
        1 => {
            out.push('?');
            out
        }
        _ => out,
    }
}

pub fn parse_profile_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn parse_groovestats_is_pad_player(value: Option<&str>, default: bool) -> bool {
    value
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default, |v| v == 1)
}

pub fn parse_last_played_value(value: Option<&str>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[inline(always)]
pub fn is_local_profile_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s != "." && s != ".." && !s.contains(['/', '\\', '\0'])
}

#[inline(always)]
pub fn cmp_profile_ids_case_insensitive(a: &str, b: &str) -> core::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
        .then_with(|| a.cmp(b))
}

pub fn next_local_profile_id(existing: Vec<u32>) -> Option<String> {
    next_local_profile_number(existing, LOCAL_PROFILE_MAX_ID).map(|n| format!("{n:08}"))
}

pub fn rewrite_profile_display_name_content(src: &str, display_name: &str) -> String {
    let mut out = String::with_capacity(src.len() + display_name.len() + 32);
    let mut in_userprofile = false;
    let mut saw_userprofile = false;
    let mut wrote_display = false;

    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_userprofile && !wrote_display {
                out.push_str("DisplayName=");
                out.push_str(display_name);
                out.push('\n');
                wrote_display = true;
            }
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_userprofile = section.eq_ignore_ascii_case("userprofile");
            if in_userprofile {
                saw_userprofile = true;
            }
            out.push_str(raw_line);
            out.push('\n');
            continue;
        }

        if in_userprofile && let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim();
            if key.eq_ignore_ascii_case("DisplayName") {
                out.push_str("DisplayName=");
                out.push_str(display_name);
                out.push('\n');
                wrote_display = true;
                continue;
            }
        }

        out.push_str(raw_line);
        out.push('\n');
    }

    if !saw_userprofile {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("[userprofile]\n");
        out.push_str("DisplayName=");
        out.push_str(display_name);
        out.push('\n');
    } else if in_userprofile && !wrote_display {
        out.push_str("DisplayName=");
        out.push_str(display_name);
        out.push('\n');
    }

    out
}

pub fn find_profile_avatar_path(dir: &Path) -> Option<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return None;
    };
    let mut avatar = None;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.eq_ignore_ascii_case("profile.png") {
            return Some(path);
        }
        if avatar.is_none() && name.eq_ignore_ascii_case("avatar.png") {
            avatar = Some(path);
        }
    }
    avatar
}

fn next_local_profile_number(mut nums: Vec<u32>, max: u32) -> Option<u32> {
    nums.retain(|&n| n <= max);
    nums.sort_unstable();
    nums.dedup();

    let mut first_free = 0_u32;
    for &n in &nums {
        if n == first_free {
            first_free += 1;
        } else if n > first_free {
            break;
        }
    }

    let mut next = nums.last().copied().unwrap_or(0);
    if !nums.is_empty() {
        next = next.saturating_add(1);
    }
    if next > max {
        if first_free > max {
            None
        } else {
            Some(first_free)
        }
    } else {
        Some(next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveProfile {
    Guest,
    Local { id: String },
}

#[inline(always)]
pub fn active_profile_is_guest(profile: &ActiveProfile) -> bool {
    matches!(profile, ActiveProfile::Guest)
}

#[inline(always)]
pub fn active_profile_local_id(profile: &ActiveProfile) -> Option<&str> {
    match profile {
        ActiveProfile::Local { id } => Some(id),
        ActiveProfile::Guest => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl PlayStyle {
    #[inline(always)]
    pub const fn chart_type(self) -> &'static str {
        match self {
            Self::Single | Self::Versus => "dance-single",
            Self::Double => "dance-double",
        }
    }

    #[inline(always)]
    pub const fn cols_per_player(self) -> usize {
        match self {
            Self::Single | Self::Versus => 4,
            Self::Double => 8,
        }
    }

    #[inline(always)]
    pub const fn player_count(self) -> usize {
        match self {
            Self::Single | Self::Double => 1,
            Self::Versus => 2,
        }
    }

    #[inline(always)]
    pub const fn total_cols(self) -> usize {
        self.cols_per_player() * self.player_count()
    }
}

#[inline(always)]
pub const fn player_options_section(style: PlayStyle) -> &'static str {
    match style {
        PlayStyle::Single | PlayStyle::Versus => "PlayerOptionsSingles",
        PlayStyle::Double => "PlayerOptionsDoubles",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Regular,
    Marathon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerSide {
    #[default]
    P1,
    P2,
}

#[inline(always)]
pub const fn player_side_index(side: PlayerSide) -> usize {
    match side {
        PlayerSide::P1 => 0,
        PlayerSide::P2 => 1,
    }
}

#[inline(always)]
pub const fn player_side_number(side: PlayerSide) -> u8 {
    match side {
        PlayerSide::P1 => 1,
        PlayerSide::P2 => 2,
    }
}

#[inline(always)]
pub const fn player_side_for_index(player_idx: usize) -> PlayerSide {
    match player_idx {
        1 => PlayerSide::P2,
        _ => PlayerSide::P1,
    }
}

#[inline(always)]
pub const fn player_side_joined_mask(side: PlayerSide) -> u8 {
    match side {
        PlayerSide::P1 => SESSION_JOINED_MASK_P1,
        PlayerSide::P2 => SESSION_JOINED_MASK_P2,
    }
}

#[inline(always)]
pub const fn joined_player_mask(p1: bool, p2: bool) -> u8 {
    let p1_mask = if p1 { SESSION_JOINED_MASK_P1 } else { 0 };
    let p2_mask = if p2 { SESSION_JOINED_MASK_P2 } else { 0 };
    p1_mask | p2_mask
}

#[inline(always)]
pub const fn play_style_for_joined(
    style: PlayStyle,
    p1_joined: bool,
    p2_joined: bool,
) -> PlayStyle {
    if p1_joined && p2_joined {
        PlayStyle::Versus
    } else {
        match style {
            PlayStyle::Versus => PlayStyle::Single,
            PlayStyle::Single | PlayStyle::Double => style,
        }
    }
}

#[inline(always)]
pub const fn player_side_is_joined(joined_mask: u8, side: PlayerSide) -> bool {
    joined_mask & player_side_joined_mask(side) != 0
}

#[inline(always)]
pub const fn runtime_player_is_p2(play_style: PlayStyle, side: PlayerSide) -> bool {
    matches!(
        (play_style, side),
        (PlayStyle::Single | PlayStyle::Double, PlayerSide::P2)
    )
}

#[inline(always)]
pub const fn is_single_p2_side(play_style: PlayStyle, side: PlayerSide) -> bool {
    matches!((play_style, side), (PlayStyle::Single, PlayerSide::P2))
}

#[inline(always)]
pub const fn runtime_player_index(play_style: PlayStyle, side: PlayerSide) -> usize {
    if matches!(play_style, PlayStyle::Versus) {
        player_side_index(side)
    } else {
        0
    }
}

#[inline(always)]
pub const fn runtime_player_side(
    play_style: PlayStyle,
    session_side: PlayerSide,
    player_idx: usize,
) -> PlayerSide {
    if matches!(play_style, PlayStyle::Versus) {
        player_side_for_index(player_idx)
    } else {
        session_side
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perspective {
    #[default]
    Overhead,
    Hallway,
    Distant,
    Incoming,
    Space,
}

impl Perspective {
    #[inline(always)]
    pub const fn tilt_skew(self) -> (f32, f32) {
        match self {
            Self::Overhead => (0.0, 0.0),
            Self::Hallway => (-1.0, 0.0),
            Self::Distant => (1.0, 0.0),
            Self::Incoming => (-1.0, 1.0),
            Self::Space => (1.0, 1.0),
        }
    }
}

impl FromStr for Perspective {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "overhead" => Ok(Self::Overhead),
            "hallway" => Ok(Self::Hallway),
            "distant" => Ok(Self::Distant),
            "incoming" => Ok(Self::Incoming),
            "space" => Ok(Self::Space),
            other => Err(format!("'{other}' is not a valid Perspective setting")),
        }
    }
}

impl core::fmt::Display for Perspective {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overhead => write!(f, "Overhead"),
            Self::Hallway => write!(f, "Hallway"),
            Self::Distant => write!(f, "Distant"),
            Self::Incoming => write!(f, "Incoming"),
            Self::Space => write!(f, "Space"),
        }
    }
}

/// Alternative speed-mod type to auto-apply when a chart is tagged "no CMod".
///
/// When a player is on CMod and selects a chart whose title/subtitle contains
/// "no cmod", the game transparently switches them to this mod type for that
/// play only. The persisted CMod setting is never written, so returning to
/// song select restores it. `None` leaves the player on CMod (they must switch
/// manually). See `player_options::apply_no_cmod_alternative`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoCmodAlternative {
    #[default]
    None,
    XMod,
    MMod,
}

impl FromStr for NoCmodAlternative {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" | "off" => Ok(Self::None),
            "xmod" | "x" => Ok(Self::XMod),
            "mmod" | "m" => Ok(Self::MMod),
            other => Err(format!(
                "'{other}' is not a valid NoCmodAlternative setting"
            )),
        }
    }
}

impl core::fmt::Display for NoCmodAlternative {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::XMod => write!(f, "XMod"),
            Self::MMod => write!(f, "MMod"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnOption {
    #[default]
    None,
    Mirror,
    Left,
    Right,
    LRMirror,
    UDMirror,
    Shuffle,
    Blender,
    Random,
}

impl FromStr for TurnOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" | "noturn" | "noturning" | "noturns" => Ok(Self::None),
            "mirror" => Ok(Self::Mirror),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "lrmirror" => Ok(Self::LRMirror),
            "udmirror" => Ok(Self::UDMirror),
            "shuffle" => Ok(Self::Shuffle),
            "blender" | "supershuffle" => Ok(Self::Blender),
            "random" | "hypershuffle" => Ok(Self::Random),
            other => Err(format!("'{other}' is not a valid Turn setting")),
        }
    }
}

impl core::fmt::Display for TurnOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Mirror => write!(f, "Mirror"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::LRMirror => write!(f, "LRMirror"),
            Self::UDMirror => write!(f, "UDMirror"),
            Self::Shuffle => write!(f, "Shuffle"),
            Self::Blender => write!(f, "Blender"),
            Self::Random => write!(f, "Random"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollOption(u8);

#[allow(non_upper_case_globals)]
impl ScrollOption {
    pub const Normal: Self = Self(0);
    pub const Reverse: Self = Self(1 << 0);
    pub const Split: Self = Self(1 << 1);
    pub const Alternate: Self = Self(1 << 2);
    pub const Cross: Self = Self(1 << 3);
    pub const Centered: Self = Self(1 << 4);

    #[inline(always)]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    #[inline(always)]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline(always)]
    pub const fn is_normal(self) -> bool {
        self.0 == 0
    }
}

impl Default for ScrollOption {
    fn default() -> Self {
        Self::Normal
    }
}

impl FromStr for ScrollOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        if raw.is_empty() {
            return Err("Scroll setting is empty".to_string());
        }
        let lower = raw.to_lowercase();
        let mut result = Self::empty();
        for token in lower.split(|c: char| c == '+' || c == ',' || c.is_whitespace()) {
            if token.is_empty() {
                continue;
            }
            let flag = match token {
                "normal" => Self::Normal,
                "reverse" => Self::Reverse,
                "split" => Self::Split,
                "alternate" => Self::Alternate,
                "cross" => Self::Cross,
                "centered" => Self::Centered,
                other => {
                    return Err(format!("'{other}' is not a valid Scroll setting"));
                }
            };
            if flag.0 != 0 {
                result = result.union(flag);
            }
        }
        Ok(result)
    }
}

impl core::fmt::Display for ScrollOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_normal() {
            return write!(f, "Normal");
        }

        let mut first = true;
        let mut write_flag = |name: &str, present: bool, f: &mut core::fmt::Formatter<'_>| {
            if !present {
                return Ok(());
            }
            if !first {
                write!(f, "+")?;
            }
            first = false;
            write!(f, "{name}")
        };

        write_flag("Reverse", self.contains(Self::Reverse), f)?;
        write_flag("Split", self.contains(Self::Split), f)?;
        write_flag("Alternate", self.contains(Self::Alternate), f)?;
        write_flag("Cross", self.contains(Self::Cross), f)?;
        write_flag("Centered", self.contains(Self::Centered), f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboMode {
    #[default]
    FullCombo,
    CurrentCombo,
}

impl FromStr for ComboMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "fullcombo" => Ok(Self::FullCombo),
            "currentcombo" => Ok(Self::CurrentCombo),
            other => Err(format!("'{other}' is not a valid ComboMode setting")),
        }
    }
}

impl core::fmt::Display for ComboMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FullCombo => write!(f, "FullCombo"),
            Self::CurrentCombo => write!(f, "CurrentCombo"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboColors {
    #[default]
    Glow,
    Solid,
    Rainbow,
    RainbowScroll,
    None,
}

impl FromStr for ComboColors {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "glow" => Ok(Self::Glow),
            "solid" => Ok(Self::Solid),
            "rainbow" => Ok(Self::Rainbow),
            "rainbowscroll" => Ok(Self::RainbowScroll),
            "none" => Ok(Self::None),
            other => Err(format!("'{other}' is not a valid ComboColors setting")),
        }
    }
}

impl core::fmt::Display for ComboColors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Glow => write!(f, "Glow"),
            Self::Solid => write!(f, "Solid"),
            Self::Rainbow => write!(f, "Rainbow"),
            Self::RainbowScroll => write!(f, "RainbowScroll"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboFont {
    #[default]
    Wendy,
    ArialRounded,
    Asap,
    BebasNeue,
    SourceCode,
    Work,
    WendyCursed,
    Mega,
    None,
}

impl FromStr for ComboFont {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "wendy" => Ok(Self::Wendy),
            "arial rounded" | "arialrounded" => Ok(Self::ArialRounded),
            "asap" => Ok(Self::Asap),
            "bebas neue" | "bebasneue" => Ok(Self::BebasNeue),
            "source code" | "sourcecode" => Ok(Self::SourceCode),
            "work" => Ok(Self::Work),
            "wendy (cursed)" | "wendy cursed" | "wendycursed" => Ok(Self::WendyCursed),
            "mega" => Ok(Self::Mega),
            "none" => Ok(Self::None),
            other => Err(format!("'{other}' is not a valid ComboFont setting")),
        }
    }
}

impl core::fmt::Display for ComboFont {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Wendy => write!(f, "Wendy"),
            Self::ArialRounded => write!(f, "Arial Rounded"),
            Self::Asap => write!(f, "Asap"),
            Self::BebasNeue => write!(f, "Bebas Neue"),
            Self::SourceCode => write!(f, "Source Code"),
            Self::Work => write!(f, "Work"),
            Self::WendyCursed => write!(f, "Wendy (Cursed)"),
            Self::Mega => write!(f, "Mega"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetScoreSetting {
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    AMinus,
    A,
    APlus,
    SMinus,
    #[default]
    S,
    SPlus,
    MachineBest,
    PersonalBest,
}

impl FromStr for TargetScoreSetting {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "cminus" | "c-" => Ok(Self::CMinus),
            "c" => Ok(Self::C),
            "cplus" | "c+" => Ok(Self::CPlus),
            "bminus" | "b-" => Ok(Self::BMinus),
            "b" => Ok(Self::B),
            "bplus" | "b+" => Ok(Self::BPlus),
            "aminus" | "a-" => Ok(Self::AMinus),
            "a" => Ok(Self::A),
            "aplus" | "a+" => Ok(Self::APlus),
            "sminus" | "s-" => Ok(Self::SMinus),
            "" | "s" => Ok(Self::S),
            "splus" | "s+" => Ok(Self::SPlus),
            "machinebest" | "machine" => Ok(Self::MachineBest),
            "personalbest" | "personal" => Ok(Self::PersonalBest),
            other => Err(format!("'{other}' is not a valid TargetScore setting")),
        }
    }
}

impl core::fmt::Display for TargetScoreSetting {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CMinus => write!(f, "C-"),
            Self::C => write!(f, "C"),
            Self::CPlus => write!(f, "C+"),
            Self::BMinus => write!(f, "B-"),
            Self::B => write!(f, "B"),
            Self::BPlus => write!(f, "B+"),
            Self::AMinus => write!(f, "A-"),
            Self::A => write!(f, "A"),
            Self::APlus => write!(f, "A+"),
            Self::SMinus => write!(f, "S-"),
            Self::S => write!(f, "S"),
            Self::SPlus => write!(f, "S+"),
            Self::MachineBest => write!(f, "Machine Best"),
            Self::PersonalBest => write!(f, "Personal Best"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorBarStyle {
    #[default]
    None,
    Colorful,
    Monochrome,
    Text,
    Highlight,
    Average,
}

impl FromStr for ErrorBarStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "colorful" => Ok(Self::Colorful),
            "monochrome" => Ok(Self::Monochrome),
            "text" => Ok(Self::Text),
            "highlight" => Ok(Self::Highlight),
            "average" => Ok(Self::Average),
            other => Err(format!("'{other}' is not a valid ErrorBar setting")),
        }
    }
}

impl core::fmt::Display for ErrorBarStyle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Colorful => write!(f, "Colorful"),
            Self::Monochrome => write!(f, "Monochrome"),
            Self::Text => write!(f, "Text"),
            Self::Highlight => write!(f, "Highlight"),
            Self::Average => write!(f, "Average"),
        }
    }
}

bitflags! {
    /// Persisted bitmask of live timing statistics shown during gameplay.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct LiveTimingStatsMask: u8 {
        const MEAN     = 1 << 0;
        const MEAN_ABS = 1 << 1;
        const MAX      = 1 << 2;
    }
}

bitflags! {
    /// Persisted bitmask for the Error Bar SelectMultiple row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ErrorBarMask: u8 {
        const COLORFUL   = 1 << 0;
        const MONOCHROME = 1 << 1;
        const TEXT       = 1 << 2;
        const HIGHLIGHT  = 1 << 3;
        const AVERAGE    = 1 << 4;
    }
}

#[inline(always)]
pub const fn error_bar_mask_from_style(style: ErrorBarStyle, text: bool) -> ErrorBarMask {
    let text_bits = if text { ErrorBarMask::TEXT.bits() } else { 0 };
    let style_bits = match style {
        ErrorBarStyle::None => 0,
        ErrorBarStyle::Colorful => ErrorBarMask::COLORFUL.bits(),
        ErrorBarStyle::Monochrome => ErrorBarMask::MONOCHROME.bits(),
        ErrorBarStyle::Text => ErrorBarMask::TEXT.bits(),
        ErrorBarStyle::Highlight => ErrorBarMask::HIGHLIGHT.bits(),
        ErrorBarStyle::Average => ErrorBarMask::AVERAGE.bits(),
    };
    ErrorBarMask::from_bits_truncate(text_bits | style_bits)
}

#[inline(always)]
pub const fn error_bar_style_from_mask(mask: ErrorBarMask) -> ErrorBarStyle {
    if mask.contains(ErrorBarMask::COLORFUL) {
        ErrorBarStyle::Colorful
    } else if mask.contains(ErrorBarMask::MONOCHROME) {
        ErrorBarStyle::Monochrome
    } else if mask.contains(ErrorBarMask::HIGHLIGHT) {
        ErrorBarStyle::Highlight
    } else if mask.contains(ErrorBarMask::AVERAGE) {
        ErrorBarStyle::Average
    } else {
        ErrorBarStyle::None
    }
}

#[inline(always)]
pub const fn error_bar_text_from_mask(mask: ErrorBarMask) -> bool {
    mask.contains(ErrorBarMask::TEXT)
}

bitflags! {
    /// Persisted bitmask of enabled appearance transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct AppearanceEffectsMask: u8 {
        const HIDDEN         = 1 << 0;
        const SUDDEN         = 1 << 1;
        const STEALTH        = 1 << 2;
        const BLINK          = 1 << 3;
        const RANDOM_VANISH  = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled acceleration transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct AccelEffectsMask: u8 {
        const BOOST     = 1 << 0;
        const BRAKE     = 1 << 1;
        const WAVE      = 1 << 2;
        const EXPAND    = 1 << 3;
        const BOOMERANG = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled hold transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct HoldsMask: u8 {
        const PLANTED        = 1 << 0;
        const FLOORED        = 1 << 1;
        const TWISTER        = 1 << 2;
        const NO_ROLLS       = 1 << 3;
        const HOLDS_TO_ROLLS = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled visual transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct VisualEffectsMask: u16 {
        const DRUNK     = 1 << 0;
        const DIZZY     = 1 << 1;
        const CONFUSION = 1 << 2;
        const BIG       = 1 << 3;
        const FLIP      = 1 << 4;
        const INVERT    = 1 << 5;
        const TORNADO   = 1 << 6;
        const TIPSY     = 1 << 7;
        const BUMPY     = 1 << 8;
        const BEAT      = 1 << 9;
    }
}

bitflags! {
    /// Persisted bitmask of enabled chart insert transforms.
    ///
    /// Bit layout matches the runtime insert-mask constants, except bit 7
    /// (Mines) is runtime/attack-only and is deliberately not represented
    /// here.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct InsertMask: u8 {
        const WIDE   = 1 << 0;
        const BIG    = 1 << 1;
        const QUICK  = 1 << 2;
        const BMRIZE = 1 << 3;
        const SKIPPY = 1 << 4;
        const ECHO   = 1 << 5;
        const STOMP  = 1 << 6;
    }
}

bitflags! {
    /// Persisted bitmask of enabled chart removal transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct RemoveMask: u8 {
        const LITTLE   = 1 << 0;
        const NO_MINES = 1 << 1;
        const NO_HOLDS = 1 << 2;
        const NO_JUMPS = 1 << 3;
        const NO_HANDS = 1 << 4;
        const NO_QUADS = 1 << 5;
        const NO_LIFTS = 1 << 6;
        const NO_FAKES = 1 << 7;
    }
}

bitflags! {
    /// Persisted bitmask of tap explosion windows enabled for gameplay.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TapExplosionMask: u8 {
        const FANTASTIC = 1 << 0;
        const EXCELLENT = 1 << 1;
        const GREAT     = 1 << 2;
        const DECENT    = 1 << 3;
        const WAY_OFF   = 1 << 4;
        const HELD      = 1 << 5;
        const MISS      = 1 << 6;
        const HOLDING   = 1 << 7;
    }
}

bitflags! {
    /// Persisted bitmask of judgments that trigger gameplay column flashes.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ColumnFlashMask: u8 {
        const BLUE_FANTASTIC  = 1 << 0;
        const WHITE_FANTASTIC = 1 << 1;
        const EXCELLENT       = 1 << 2;
        const GREAT           = 1 << 3;
        const DECENT          = 1 << 4;
        const WAY_OFF         = 1 << 5;
        const MISS            = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnFlashBrightness {
    #[default]
    Normal,
    Dimmed,
}

impl FromStr for ColumnFlashBrightness {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "normal" | "default" | "standard" => Ok(Self::Normal),
            "dimmed" | "dim" | "chris" | "compact" => Ok(Self::Dimmed),
            other => Err(format!(
                "'{other}' is not a valid ColumnFlashBrightness setting"
            )),
        }
    }
}

impl core::fmt::Display for ColumnFlashBrightness {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Dimmed => write!(f, "Dimmed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnFlashSize {
    #[default]
    Default,
    Compact,
}

impl FromStr for ColumnFlashSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" | "normal" | "full" | "standard" => Ok(Self::Default),
            "compact" | "short" | "shorter" | "chris" => Ok(Self::Compact),
            other => Err(format!("'{other}' is not a valid ColumnFlashSize setting")),
        }
    }
}

impl core::fmt::Display for ColumnFlashSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Compact => write!(f, "Compact"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttackMode {
    Off,
    #[default]
    On,
    Random,
}

impl FromStr for AttackMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "noattacks" | "noattack" => Ok(Self::Off),
            "on" | "normal" => Ok(Self::On),
            "random" | "randomattacks" => Ok(Self::Random),
            other => Err(format!("'{other}' is not a valid AttackMode setting")),
        }
    }
}

impl core::fmt::Display for AttackMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::On => write!(f, "On"),
            Self::Random => write!(f, "Random"),
        }
    }
}

/// Hard cap for the evaluation scatter plot's vertical scale, selectable
/// per profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScatterplotMaxWindow {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

impl FromStr for ScatterplotMaxWindow {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "off" | "none" | "autoscale" | "0" => Ok(Self::Off),
            "fantastic" | "fantasticmax" | "fa" => Ok(Self::Fantastic),
            "excellent" | "excellentmax" | "ex" => Ok(Self::Excellent),
            "great" | "greatmax" | "gr" => Ok(Self::Great),
            other => Err(format!(
                "'{other}' is not a valid ScatterplotMaxWindow setting"
            )),
        }
    }
}

impl core::fmt::Display for ScatterplotMaxWindow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Fantastic => write!(f, "Fantastic"),
            Self::Excellent => write!(f, "Excellent"),
            Self::Great => write!(f, "Great"),
        }
    }
}

/// Gameplay percent score placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScorePosition {
    #[default]
    Normal,
    StepStatistics,
}

impl FromStr for ScorePosition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "normal" | "default" | "top" => Ok(Self::Normal),
            "stepstatistics" | "stepstats" | "stats" => Ok(Self::StepStatistics),
            other => Err(format!("'{other}' is not a valid ScorePosition setting")),
        }
    }
}

impl core::fmt::Display for ScorePosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::StepStatistics => write!(f, "Step Statistics"),
        }
    }
}

/// Gameplay percent score value semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScoreDisplayMode {
    #[default]
    Normal,
    Predictive,
}

impl FromStr for ScoreDisplayMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "normal" | "default" | "actual" | "current" => Ok(Self::Normal),
            "predictive" | "predicted" | "prediction" => Ok(Self::Predictive),
            other => Err(format!("'{other}' is not a valid ScoreDisplay setting")),
        }
    }
}

impl core::fmt::Display for ScoreDisplayMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Predictive => write!(f, "Predictive"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifeMeterType {
    #[default]
    Standard,
    Surround,
    Vertical,
}

impl FromStr for LifeMeterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "" | "standard" => Ok(Self::Standard),
            "surround" => Ok(Self::Surround),
            "vertical" => Ok(Self::Vertical),
            other => Err(format!("'{other}' is not a valid LifeMeterType setting")),
        }
    }
}

impl core::fmt::Display for LifeMeterType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Standard => write!(f, "Standard"),
            Self::Surround => write!(f, "Surround"),
            Self::Vertical => write!(f, "Vertical"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorBarTrim {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

impl FromStr for ErrorBarTrim {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "fantastic" => Ok(Self::Fantastic),
            "excellent" => Ok(Self::Excellent),
            "great" => Ok(Self::Great),
            other => Err(format!("'{other}' is not a valid ErrorBarTrim setting")),
        }
    }
}

impl core::fmt::Display for ErrorBarTrim {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Fantastic => write!(f, "Fantastic"),
            Self::Excellent => write!(f, "Excellent"),
            Self::Great => write!(f, "Great"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingWindowsOption {
    #[default]
    None,
    WayOffs,
    DecentsAndWayOffs,
    FantasticsAndExcellents,
}

impl TimingWindowsOption {
    #[inline(always)]
    pub const fn disabled_windows(self) -> [bool; 5] {
        match self {
            Self::None => [false; 5],
            Self::WayOffs => [false, false, false, false, true],
            Self::DecentsAndWayOffs => [false, false, false, true, true],
            Self::FantasticsAndExcellents => [true, true, false, false, false],
        }
    }
}

impl FromStr for TimingWindowsOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "way offs" | "wayoffs" => Ok(Self::WayOffs),
            "decents + way offs" | "decents+wayoffs" | "decents and way offs" => {
                Ok(Self::DecentsAndWayOffs)
            }
            "fantastics + excellents" | "fantastics+excellents" | "fantastics and excellents" => {
                Ok(Self::FantasticsAndExcellents)
            }
            other => Err(format!("'{other}' is not a valid TimingWindows setting")),
        }
    }
}

impl core::fmt::Display for TimingWindowsOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::WayOffs => write!(f, "Way Offs"),
            Self::DecentsAndWayOffs => write!(f, "Decents + Way Offs"),
            Self::FantasticsAndExcellents => write!(f, "Fantastics + Excellents"),
        }
    }
}

bitflags! {
    /// Persisted bitmask of enabled Step Statistics gameplay widgets.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct StepStatisticsMask: u16 {
        const DENSITY_GRAPH    = 1 << 0;
        const SONG_BANNER      = 1 << 1;
        const JUDGMENT_COUNTER = 1 << 2;
        const SONG_DURATION    = 1 << 3;
        const PACK_BANNER      = 1 << 4;
        const SONG_INFO        = 1 << 5;
        const STEP_COUNTS      = 1 << 6;
        const PEAK_NPS         = 1 << 7;
    }
}

impl StepStatisticsMask {
    pub const ALL_WIDGET_BITS: u16 = Self::DENSITY_GRAPH.bits()
        | Self::SONG_BANNER.bits()
        | Self::JUDGMENT_COUNTER.bits()
        | Self::SONG_DURATION.bits()
        | Self::PACK_BANNER.bits()
        | Self::SONG_INFO.bits()
        | Self::STEP_COUNTS.bits()
        | Self::PEAK_NPS.bits();

    #[inline(always)]
    pub const fn all_widgets() -> Self {
        Self::from_bits_retain(Self::ALL_WIDGET_BITS)
    }

    #[inline(always)]
    pub fn pack_info_enabled(self) -> bool {
        self.intersects(Self::PACK_BANNER | Self::SONG_INFO)
    }
}

fn normalize_option_key(s: &str) -> String {
    let mut key = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    key
}

fn step_statistics_bit_from_key(key: &str) -> Option<StepStatisticsMask> {
    match key {
        "densitygraph" | "density" => Some(StepStatisticsMask::DENSITY_GRAPH),
        "songbanner" | "banner" => Some(StepStatisticsMask::SONG_BANNER),
        "judgmentcounter" | "judgementcounter" | "judgmentcounts" | "judgementcounts"
        | "judgmentscounter" | "judgementscounter" | "judgment" | "judgement" | "judgments"
        | "judgements" => Some(StepStatisticsMask::JUDGMENT_COUNTER),
        "songduration" | "songtime" | "duration" | "time" => {
            Some(StepStatisticsMask::SONG_DURATION)
        }
        "packbanner" | "packinfo" | "songinfo" => Some(StepStatisticsMask::PACK_BANNER),
        "stepcounts" | "steps" | "holdsminesrolls" | "jumpsminesholds" => {
            Some(StepStatisticsMask::STEP_COUNTS)
        }
        "peaknps" => Some(StepStatisticsMask::PEAK_NPS),
        _ => None,
    }
}

impl FromStr for StepStatisticsMask {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let key = normalize_option_key(trimmed);
        match key.as_str() {
            "" | "none" => return Ok(Self::empty()),
            // Legacy DataVisualizations values.
            "targetscoregraph" | "targetscore" | "target" => return Ok(Self::empty()),
            "stepstatistics" | "stepstats" => return Ok(Self::all_widgets()),
            _ => {}
        }

        if let Ok(bits) = trimmed.parse::<u16>() {
            return Ok(Self::from_bits_retain(bits & Self::ALL_WIDGET_BITS));
        }

        let mut mask = Self::empty();
        for part in trimmed.split([',', '|', ';']) {
            let key = normalize_option_key(part);
            if key.is_empty() {
                continue;
            }
            if matches!(key.as_str(), "gsbox" | "groovestatsbox" | "scorebox") {
                continue;
            }
            let Some(bit) = step_statistics_bit_from_key(key.as_str()) else {
                return Err(format!("'{part}' is not a valid StepStatistics setting"));
            };
            mask.insert(bit);
        }
        Ok(mask)
    }
}

impl core::fmt::Display for StepStatisticsMask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        const BEFORE_PACK: [(StepStatisticsMask, &str); 4] = [
            (StepStatisticsMask::DENSITY_GRAPH, "Density Graph"),
            (StepStatisticsMask::SONG_BANNER, "Song Banner"),
            (StepStatisticsMask::JUDGMENT_COUNTER, "Judgements"),
            (StepStatisticsMask::SONG_DURATION, "Song Duration"),
        ];
        const AFTER_PACK: [(StepStatisticsMask, &str); 2] = [
            (StepStatisticsMask::STEP_COUNTS, "Step Counts"),
            (StepStatisticsMask::PEAK_NPS, "Peak NPS"),
        ];
        if self.is_empty() {
            return write!(f, "None");
        }
        let mut first = true;
        for (bit, label) in BEFORE_PACK {
            if !self.contains(bit) {
                continue;
            }
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{label}")?;
            first = false;
        }
        if self.pack_info_enabled() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "Pack Info")?;
            first = false;
        }
        for (bit, label) in AFTER_PACK {
            if !self.contains(bit) {
                continue;
            }
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{label}")?;
            first = false;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStatsExtra {
    #[default]
    None,
    ErrorStats,
    AmongUs,
    BrodyQuest,
    CatJAM,
    CrabPls,
    DancingDuck,
    DonChan,
    NyanCat,
    Randomizer,
    RinCat,
    Snoop,
    Sonic,
}

impl StepStatsExtra {
    pub const RANDOMIZER_CHOICES: [Self; 10] = [
        Self::AmongUs,
        Self::BrodyQuest,
        Self::CatJAM,
        Self::CrabPls,
        Self::DancingDuck,
        Self::DonChan,
        Self::NyanCat,
        Self::RinCat,
        Self::Snoop,
        Self::Sonic,
    ];

    #[inline(always)]
    pub const fn renderable(self) -> bool {
        !matches!(self, Self::None | Self::ErrorStats | Self::Randomizer)
    }
}

impl FromStr for StepStatsExtra {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match normalize_option_key(s).as_str() {
            "" | "none" => Ok(Self::None),
            "errorstats" | "error" => Ok(Self::ErrorStats),
            "amongus" => Ok(Self::AmongUs),
            "brodyquest" => Ok(Self::BrodyQuest),
            "catjam" => Ok(Self::CatJAM),
            "crabpls" => Ok(Self::CrabPls),
            "dancingduck" => Ok(Self::DancingDuck),
            "donchan" => Ok(Self::DonChan),
            "nyancat" => Ok(Self::NyanCat),
            "randomizer" | "random" => Ok(Self::Randomizer),
            "rincat" => Ok(Self::RinCat),
            "snoop" => Ok(Self::Snoop),
            "sonic" => Ok(Self::Sonic),
            other => Err(format!("'{other}' is not a valid StepStatsExtra setting")),
        }
    }
}

impl core::fmt::Display for StepStatsExtra {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::ErrorStats => write!(f, "ErrorStats"),
            Self::AmongUs => write!(f, "AmongUs"),
            Self::BrodyQuest => write!(f, "BrodyQuest"),
            Self::CatJAM => write!(f, "CatJAM"),
            Self::CrabPls => write!(f, "CrabPls"),
            Self::DancingDuck => write!(f, "Dancing Duck"),
            Self::DonChan => write!(f, "DonChan"),
            Self::NyanCat => write!(f, "Nyan Cat"),
            Self::Randomizer => write!(f, "Randomizer"),
            Self::RinCat => write!(f, "Rin Cat"),
            Self::Snoop => write!(f, "Snoop"),
            Self::Sonic => write!(f, "Sonic"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeasureCounter {
    #[default]
    None,
    Eighth,
    Twelfth,
    Sixteenth,
    TwentyFourth,
    ThirtySecond,
}

impl MeasureCounter {
    #[inline(always)]
    pub const fn notes_threshold(self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::Eighth => Some(8),
            Self::Twelfth => Some(12),
            Self::Sixteenth => Some(16),
            Self::TwentyFourth => Some(24),
            Self::ThirtySecond => Some(32),
        }
    }

    #[inline(always)]
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::TwentyFourth => 1.5,
            Self::ThirtySecond => 2.0,
            _ => 1.0,
        }
    }
}

impl FromStr for MeasureCounter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "8th" => Ok(Self::Eighth),
            "12th" => Ok(Self::Twelfth),
            "16th" => Ok(Self::Sixteenth),
            "24th" => Ok(Self::TwentyFourth),
            "32nd" => Ok(Self::ThirtySecond),
            other => Err(format!("'{other}' is not a valid MeasureCounter setting")),
        }
    }
}

impl core::fmt::Display for MeasureCounter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Eighth => write!(f, "8th"),
            Self::Twelfth => write!(f, "12th"),
            Self::Sixteenth => write!(f, "16th"),
            Self::TwentyFourth => write!(f, "24th"),
            Self::ThirtySecond => write!(f, "32nd"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeasureLines {
    #[default]
    Off,
    Measure,
    Quarter,
    Eighth,
}

impl FromStr for MeasureLines {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "measure" => Ok(Self::Measure),
            "quarter" => Ok(Self::Quarter),
            "eighth" => Ok(Self::Eighth),
            other => Err(format!("'{other}' is not a valid MeasureLines setting")),
        }
    }
}

impl core::fmt::Display for MeasureLines {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Measure => write!(f, "Measure"),
            Self::Quarter => write!(f, "Quarter"),
            Self::Eighth => write!(f, "Eighth"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicator {
    #[default]
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

impl FromStr for MiniIndicator {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" => Ok(Self::None),
            "subtractivescoring" | "subtractive" => Ok(Self::SubtractiveScoring),
            "predictivescoring" | "predictive" => Ok(Self::PredictiveScoring),
            "pacescoring" | "pace" => Ok(Self::PaceScoring),
            "rivalscoring" | "rival" => Ok(Self::RivalScoring),
            "pacemaker" => Ok(Self::Pacemaker),
            "streamprog" | "streamprogress" | "stream" => Ok(Self::StreamProg),
            other => Err(format!("'{other}' is not a valid MiniIndicator setting")),
        }
    }
}

impl core::fmt::Display for MiniIndicator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::SubtractiveScoring => write!(f, "SubtractiveScoring"),
            Self::PredictiveScoring => write!(f, "PredictiveScoring"),
            Self::PaceScoring => write!(f, "PaceScoring"),
            Self::RivalScoring => write!(f, "RivalScoring"),
            Self::Pacemaker => write!(f, "Pacemaker"),
            Self::StreamProg => write!(f, "StreamProg"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorScoreType {
    #[default]
    Itg,
    Ex,
    HardEx,
}

impl FromStr for MiniIndicatorScoreType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "itg" => Ok(Self::Itg),
            "ex" => Ok(Self::Ex),
            "hardex" | "hex" => Ok(Self::HardEx),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorScoreType setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorScoreType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Itg => write!(f, "ITG"),
            Self::Ex => write!(f, "Ex"),
            Self::HardEx => write!(f, "HardEx"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorSubtractiveDisplay {
    #[default]
    Percent,
    Points,
}

impl FromStr for MiniIndicatorSubtractiveDisplay {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "percent" | "percentage" => Ok(Self::Percent),
            "points" | "point" | "dancepoints" | "dp" => Ok(Self::Points),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorSubtractiveDisplay setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorSubtractiveDisplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Percent => write!(f, "Percent"),
            Self::Points => write!(f, "Points"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorSize {
    #[default]
    Default,
    Large,
}

impl FromStr for MiniIndicatorSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" => Ok(Self::Default),
            "large" | "big" => Ok(Self::Large),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorSize setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Large => write!(f, "Large"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorColor {
    #[default]
    Default,
    Detailed,
    Combo,
}

impl FromStr for MiniIndicatorColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" => Ok(Self::Default),
            "detailed" => Ok(Self::Detailed),
            "combo" | "combocolor" | "combocolour" => Ok(Self::Combo),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorColor setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorColor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Detailed => write!(f, "Detailed"),
            Self::Combo => write!(f, "Combo"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorPosition {
    #[default]
    Default,
    UnderUpArrow,
}

impl FromStr for MiniIndicatorPosition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" | "normal" => Ok(Self::Default),
            "underuparrow" | "uparrow" | "arrow" | "left" => Ok(Self::UnderUpArrow),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorPosition setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::UnderUpArrow => write!(f, "UnderUpArrow"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HideLightType {
    #[default]
    NoHideLights,
    HideAllLights,
    HideMarqueeLights,
    HideBassLights,
}

impl FromStr for HideLightType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "nohidelights" => Ok(Self::NoHideLights),
            "hidealllights" => Ok(Self::HideAllLights),
            "hidemarqueelights" => Ok(Self::HideMarqueeLights),
            "hidebasslights" => Ok(Self::HideBassLights),
            other => Err(format!("'{other}' is not a valid HideLightType setting")),
        }
    }
}

impl core::fmt::Display for HideLightType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoHideLights => write!(f, "NoHideLights"),
            Self::HideAllLights => write!(f, "HideAllLights"),
            Self::HideMarqueeLights => write!(f, "HideMarqueeLights"),
            Self::HideBassLights => write!(f, "HideBassLights"),
        }
    }
}

/// Background-darkening alpha for the per-notefield underlay quad, expressed
/// as an integer percentage in `0..=100` (0 = no filter, 100 = fully opaque
/// black). Reads accept the legacy enum labels (`Off|Dark|Darker|Darkest`) so
/// existing profiles migrate automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundFilter(u8);

impl BackgroundFilter {
    /// Default for new profiles. Matches the old `Darkest` enum variant.
    pub const DEFAULT: Self = Self(95);
    pub const OFF: Self = Self(0);
    pub const MAX_PERCENT: u8 = 100;

    /// Construct from a raw percentage, clamping to `0..=100`.
    #[inline]
    pub const fn from_percent(value: u8) -> Self {
        let clamped = if value > Self::MAX_PERCENT {
            Self::MAX_PERCENT
        } else {
            value
        };
        Self(clamped)
    }

    /// Construct from any signed integer, clamping to `0..=100`.
    #[inline]
    pub fn from_i32(value: i32) -> Self {
        Self::from_percent(value.clamp(0, Self::MAX_PERCENT as i32) as u8)
    }

    /// Underlying percentage value `0..=100`.
    #[inline]
    pub const fn percent(self) -> u8 {
        self.0
    }

    /// Alpha value in `0.0..=1.0` to be passed to `diffuse`.
    #[inline]
    pub fn alpha(self) -> f32 {
        self.0 as f32 / Self::MAX_PERCENT as f32
    }

    /// Convenience for branches that toggle on the "no filter" case.
    #[inline]
    pub const fn is_off(self) -> bool {
        self.0 == 0
    }
}

impl Default for BackgroundFilter {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl FromStr for BackgroundFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "off" => return Ok(Self(0)),
            "dark" => return Ok(Self(50)),
            "darker" => return Ok(Self(75)),
            "darkest" => return Ok(Self(95)),
            _ => {}
        }

        let numeric = trimmed.trim_end_matches('%').trim();
        let value: i32 = numeric
            .parse()
            .map_err(|_| format!("'{s}' is not a valid BackgroundFilter setting"))?;
        if !(0..=Self::MAX_PERCENT as i32).contains(&value) {
            return Err(format!(
                "BackgroundFilter percent {value} out of range 0..=100"
            ));
        }
        Ok(Self(value as u8))
    }
}

impl core::fmt::Display for BackgroundFilter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteSkin {
    raw: String,
}

impl NoteSkin {
    pub const DEFAULT_NAME: &'static str = "default";
    pub const CEL_NAME: &'static str = "cel";
    pub const NONE_NAME: &'static str = "__none__";

    #[inline(always)]
    fn normalize(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_ascii_lowercase())
    }

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self::from_str(raw).unwrap_or_default()
    }

    #[inline(always)]
    pub fn none_choice() -> Self {
        Self {
            raw: Self::NONE_NAME.to_string(),
        }
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    #[inline(always)]
    pub fn is_none_choice(&self) -> bool {
        self.raw == Self::NONE_NAME
    }
}

impl Default for NoteSkin {
    fn default() -> Self {
        Self {
            raw: Self::CEL_NAME.to_string(),
        }
    }
}

impl FromStr for NoteSkin {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = Self::normalize(s)
            .ok_or_else(|| format!("'{}' is not a valid NoteSkin setting", s.trim()))?;
        Ok(Self { raw: normalized })
    }
}

impl core::fmt::Display for NoteSkin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[inline(always)]
pub fn resolve_noteskin_choice<'a>(
    noteskin: Option<&'a NoteSkin>,
    fallback: &'a NoteSkin,
) -> &'a NoteSkin {
    noteskin.unwrap_or(fallback)
}

#[inline(always)]
pub fn tap_explosion_skin_hidden(noteskin: Option<&NoteSkin>) -> bool {
    noteskin.is_some_and(NoteSkin::is_none_choice)
}

#[inline(always)]
pub fn resolve_tap_explosion_skin<'a>(
    noteskin: Option<&'a NoteSkin>,
    fallback: &'a NoteSkin,
) -> Option<&'a NoteSkin> {
    if tap_explosion_skin_hidden(noteskin) {
        None
    } else {
        Some(resolve_noteskin_choice(noteskin, fallback))
    }
}

fn normalize_graphic_key(
    raw: &str,
    folder: &str,
    stock_aliases: &[(&str, &str)],
) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("graphic setting was empty".to_string());
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok("None".to_string());
    }

    let basename = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .trim();
    if basename.eq_ignore_ascii_case("none") {
        return Ok("None".to_string());
    }

    let normalized = basename.to_ascii_lowercase();
    if let Some((_, key)) = stock_aliases
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(&normalized))
    {
        return Ok((*key).to_string());
    }

    Ok(format!("{folder}/{basename}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldJudgmentGraphic(String);

impl HoldJudgmentGraphic {
    pub const DEFAULT_KEY: &'static str = "hold_judgements/Love 1x2 (doubleres).png";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("love", Self::DEFAULT_KEY),
        ("love 1x2 (doubleres).png", Self::DEFAULT_KEY),
        (
            "hold_judgements/love 1x2 (doubleres).png",
            Self::DEFAULT_KEY,
        ),
        ("mute", "hold_judgements/mute 1x2 (doubleres).png"),
        (
            "mute 1x2 (doubleres).png",
            "hold_judgements/mute 1x2 (doubleres).png",
        ),
        (
            "hold_judgements/mute 1x2 (doubleres).png",
            "hold_judgements/mute 1x2 (doubleres).png",
        ),
        ("itg2", "hold_judgements/ITG2 1x2 (doubleres).png"),
        (
            "itg2 1x2 (doubleres).png",
            "hold_judgements/ITG2 1x2 (doubleres).png",
        ),
        (
            "hold_judgements/itg2 1x2 (doubleres).png",
            "hold_judgements/ITG2 1x2 (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "hold_judgements", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for HoldJudgmentGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for HoldJudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "hold_judgements", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for HoldJudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldMissGraphic(String);

impl HeldMissGraphic {
    pub const DEFAULT_KEY: &'static str = "None";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("love", "held_miss/Love (doubleres).png"),
        ("love (doubleres).png", "held_miss/Love (doubleres).png"),
        (
            "held_miss/love (doubleres).png",
            "held_miss/Love (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "held_miss", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for HeldMissGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for HeldMissGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "held_miss", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for HeldMissGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentGraphic(String);

impl JudgmentGraphic {
    pub const DEFAULT_KEY: &'static str = "judgements/Love 2x7 (doubleres).png";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("bebas", "judgements/Bebas 2x7 (doubleres).png"),
        (
            "bebas 2x7 (doubleres).png",
            "judgements/Bebas 2x7 (doubleres).png",
        ),
        (
            "judgements/bebas 2x7 (doubleres).png",
            "judgements/Bebas 2x7 (doubleres).png",
        ),
        ("censored", "judgements/Censored 1x7 (doubleres).png"),
        (
            "censored 1x7 (doubleres).png",
            "judgements/Censored 1x7 (doubleres).png",
        ),
        (
            "judgements/censored 1x7 (doubleres).png",
            "judgements/Censored 1x7 (doubleres).png",
        ),
        ("chromatic", "judgements/Chromatic 2x7 (doubleres).png"),
        (
            "chromatic 2x7 (doubleres).png",
            "judgements/Chromatic 2x7 (doubleres).png",
        ),
        (
            "judgements/chromatic 2x7 (doubleres).png",
            "judgements/Chromatic 2x7 (doubleres).png",
        ),
        ("code", "judgements/Code 2x7 (doubleres).png"),
        (
            "code 2x7 (doubleres).png",
            "judgements/Code 2x7 (doubleres).png",
        ),
        (
            "judgements/code 2x7 (doubleres).png",
            "judgements/Code 2x7 (doubleres).png",
        ),
        ("comic sans", "judgements/Comic Sans 2x7 (doubleres).png"),
        ("comicsans", "judgements/Comic Sans 2x7 (doubleres).png"),
        (
            "comic sans 2x7 (doubleres).png",
            "judgements/Comic Sans 2x7 (doubleres).png",
        ),
        (
            "judgements/comic sans 2x7 (doubleres).png",
            "judgements/Comic Sans 2x7 (doubleres).png",
        ),
        ("emoticon", "judgements/Emoticon 2x7 (doubleres).png"),
        (
            "emoticon 2x7 (doubleres).png",
            "judgements/Emoticon 2x7 (doubleres).png",
        ),
        (
            "judgements/emoticon 2x7 (doubleres).png",
            "judgements/Emoticon 2x7 (doubleres).png",
        ),
        ("focus", "judgements/Focus 2x7 (doubleres).png"),
        (
            "focus 2x7 (doubleres).png",
            "judgements/Focus 2x7 (doubleres).png",
        ),
        (
            "judgements/focus 2x7 (doubleres).png",
            "judgements/Focus 2x7 (doubleres).png",
        ),
        ("grammar", "judgements/Grammar 2x7 (doubleres).png"),
        (
            "grammar 2x7 (doubleres).png",
            "judgements/Grammar 2x7 (doubleres).png",
        ),
        (
            "judgements/grammar 2x7 (doubleres).png",
            "judgements/Grammar 2x7 (doubleres).png",
        ),
        (
            "groovenights",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "groove nights",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "groovenights 2x7 (doubleres).png",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "judgements/groovenights 2x7 (doubleres).png",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        ("itg2", "judgements/ITG2 2x7 (doubleres).png"),
        (
            "itg2 2x7 (doubleres).png",
            "judgements/ITG2 2x7 (doubleres).png",
        ),
        (
            "judgements/itg2 2x7 (doubleres).png",
            "judgements/ITG2 2x7 (doubleres).png",
        ),
        ("love", Self::DEFAULT_KEY),
        ("love 2x7 (doubleres).png", Self::DEFAULT_KEY),
        ("judgements/love 2x7 (doubleres).png", Self::DEFAULT_KEY),
        ("love chroma", "judgements/Love Chroma 2x7 (doubleres).png"),
        ("lovechroma", "judgements/Love Chroma 2x7 (doubleres).png"),
        (
            "love chroma 2x7 (doubleres).png",
            "judgements/Love Chroma 2x7 (doubleres).png",
        ),
        (
            "judgements/love chroma 2x7 (doubleres).png",
            "judgements/Love Chroma 2x7 (doubleres).png",
        ),
        ("miso", "judgements/Miso 2x7 (doubleres).png"),
        (
            "miso 2x7 (doubleres).png",
            "judgements/Miso 2x7 (doubleres).png",
        ),
        (
            "judgements/miso 2x7 (doubleres).png",
            "judgements/Miso 2x7 (doubleres).png",
        ),
        ("papyrus", "judgements/Papyrus 2x7 (doubleres).png"),
        (
            "papyrus 2x7 (doubleres).png",
            "judgements/Papyrus 2x7 (doubleres).png",
        ),
        (
            "judgements/papyrus 2x7 (doubleres).png",
            "judgements/Papyrus 2x7 (doubleres).png",
        ),
        (
            "rainbowmatic",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        (
            "rainbowmatic 2x7 (doubleres).png",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        (
            "judgements/rainbowmatic 2x7 (doubleres).png",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        ("roboto", "judgements/Roboto 2x7 (doubleres).png"),
        (
            "roboto 2x7 (doubleres).png",
            "judgements/Roboto 2x7 (doubleres).png",
        ),
        (
            "judgements/roboto 2x7 (doubleres).png",
            "judgements/Roboto 2x7 (doubleres).png",
        ),
        ("shift", "judgements/Shift 2x7 (doubleres).png"),
        (
            "shift 2x7 (doubleres).png",
            "judgements/Shift 2x7 (doubleres).png",
        ),
        (
            "judgements/shift 2x7 (doubleres).png",
            "judgements/Shift 2x7 (doubleres).png",
        ),
        ("tactics", "judgements/Tactics 2x7 (doubleres).png"),
        (
            "tactics 2x7 (doubleres).png",
            "judgements/Tactics 2x7 (doubleres).png",
        ),
        (
            "judgements/tactics 2x7 (doubleres).png",
            "judgements/Tactics 2x7 (doubleres).png",
        ),
        ("wendy", "judgements/Wendy 2x7 (doubleres).png"),
        (
            "wendy 2x7 (doubleres).png",
            "judgements/Wendy 2x7 (doubleres).png",
        ),
        (
            "judgements/wendy 2x7 (doubleres).png",
            "judgements/Wendy 2x7 (doubleres).png",
        ),
        (
            "wendy chroma",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
        ("wendychroma", "judgements/Wendy Chroma 2x7 (doubleres).png"),
        (
            "wendy chroma 2x7 (doubleres).png",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
        (
            "judgements/wendy chroma 2x7 (doubleres).png",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "judgements", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for JudgmentGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for JudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "judgements", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for JudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameplayHudPlayerSnapshot {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub avatar_texture_key: Option<String>,
    pub hide_username: bool,
}

#[derive(Debug, Clone)]
pub struct GameplayHudSnapshot {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub p1: GameplayHudPlayerSnapshot,
    pub p2: GameplayHudPlayerSnapshot,
}

pub struct LocalProfileSummary {
    pub id: String,
    pub display_name: String,
    pub avatar_path: Option<PathBuf>,
}

const PROFILE_STATS_VERSION_V1: u16 = 1;

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct LegacyProfileStatsV1 {
    version: u16,
    current_combo: u32,
}

#[derive(Debug, Clone, Encode, Decode)]
struct ProfileStatsV1 {
    version: u16,
    current_combo: u32,
    known_pack_names: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileStats {
    pub current_combo: u32,
    pub known_pack_names: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileStatsDecodeError {
    UnsupportedVersion(u16),
    InvalidPayload,
}

pub fn decode_profile_stats(bytes: &[u8]) -> Result<ProfileStats, ProfileStatsDecodeError> {
    if let Ok((stats, _)) =
        bincode::decode_from_slice::<ProfileStatsV1, _>(bytes, bincode::config::standard())
    {
        if stats.version != PROFILE_STATS_VERSION_V1 {
            return Err(ProfileStatsDecodeError::UnsupportedVersion(stats.version));
        }
        return Ok(ProfileStats {
            current_combo: stats.current_combo,
            known_pack_names: stats.known_pack_names.into_iter().collect(),
        });
    }
    if let Ok((stats, _)) =
        bincode::decode_from_slice::<LegacyProfileStatsV1, _>(bytes, bincode::config::standard())
    {
        if stats.version != PROFILE_STATS_VERSION_V1 {
            return Err(ProfileStatsDecodeError::UnsupportedVersion(stats.version));
        }
        return Ok(ProfileStats {
            current_combo: stats.current_combo,
            known_pack_names: HashSet::new(),
        });
    }
    Err(ProfileStatsDecodeError::InvalidPayload)
}

pub fn encode_profile_stats(stats: &ProfileStats) -> Option<Vec<u8>> {
    let mut known_pack_names: Vec<String> = stats.known_pack_names.iter().cloned().collect();
    known_pack_names.sort_unstable();
    bincode::encode_to_vec(
        ProfileStatsV1 {
            version: PROFILE_STATS_VERSION_V1,
            current_combo: stats.current_combo,
            known_pack_names,
        },
        bincode::config::standard(),
    )
    .ok()
}

pub fn parse_favorites_content(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub fn render_favorites_content(favorites: &HashSet<String>) -> String {
    let mut sorted: Vec<&str> = favorites.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.join("\n")
}

pub fn parse_favorited_packs_content(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub fn render_favorited_packs_content(packs: &HashSet<String>) -> String {
    let mut sorted: Vec<&str> = packs.iter().map(String::as_str).collect();
    sorted.sort_unstable_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    sorted.join("\n")
}

pub fn add_known_pack_names<'a>(
    known_pack_names: &mut HashSet<String>,
    pack_names: impl IntoIterator<Item = &'a str>,
) -> bool {
    let mut changed = false;
    for name in pack_names {
        changed |= known_pack_names.insert(name.to_owned());
    }
    changed
}

pub fn unknown_pack_names(
    known_pack_names: &HashSet<String>,
    scanned_pack_names: &[String],
) -> HashSet<String> {
    scanned_pack_names
        .iter()
        .filter(|name| !known_pack_names.contains(name.as_str()))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPlayed {
    pub song_music_path: Option<String>,
    pub chart_hash: Option<String>,
    pub difficulty_index: usize,
}

pub fn append_last_played_section(content: &mut String, section: &str, last_played: &LastPlayed) {
    content.push_str(&format!("[{section}]\n"));
    if let Some(path) = &last_played.song_music_path {
        content.push_str(&format!("MusicPath={path}\n"));
    } else {
        content.push_str("MusicPath=\n");
    }
    if let Some(hash) = &last_played.chart_hash {
        content.push_str(&format!("ChartHash={hash}\n"));
    } else {
        content.push_str("ChartHash=\n");
    }
    content.push_str(&format!(
        "DifficultyIndex={}\n",
        last_played.difficulty_index
    ));
    content.push('\n');
}

pub fn load_last_played_section<F>(
    has_any: bool,
    mut get: F,
    default: &LastPlayed,
) -> Option<LastPlayed>
where
    F: FnMut(&str) -> Option<String>,
{
    if !has_any {
        return None;
    }

    Some(LastPlayed {
        song_music_path: parse_last_played_value(get("MusicPath").as_deref()),
        chart_hash: parse_last_played_value(get("ChartHash").as_deref()),
        difficulty_index: get("DifficultyIndex")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default.difficulty_index),
    })
}

impl Default for LastPlayed {
    fn default() -> Self {
        Self {
            song_music_path: None,
            chart_hash: None,
            // Mirror FILE_DIFFICULTY_NAMES[2] ("Medium") as the default.
            difficulty_index: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LastPlayedCourse {
    pub course_path: Option<String>,
    pub difficulty_name: Option<String>,
}

pub fn append_last_played_course_section(
    content: &mut String,
    section: &str,
    last_played: &LastPlayedCourse,
) {
    content.push_str(&format!("[{section}]\n"));
    if let Some(path) = &last_played.course_path {
        content.push_str(&format!("CoursePath={path}\n"));
    } else {
        content.push_str("CoursePath=\n");
    }
    if let Some(name) = &last_played.difficulty_name {
        content.push_str(&format!("DifficultyName={name}\n"));
    } else {
        content.push_str("DifficultyName=\n");
    }
    content.push('\n');
}

pub fn load_last_played_course_section<F>(has_any: bool, mut get: F) -> Option<LastPlayedCourse>
where
    F: FnMut(&str) -> Option<String>,
{
    if !has_any {
        return None;
    }

    Some(LastPlayedCourse {
        course_path: parse_last_played_value(get("CoursePath").as_deref()),
        difficulty_name: parse_last_played_value(get("DifficultyName").as_deref()),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerOptionsData {
    pub background_filter: BackgroundFilter,
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub held_miss_graphic: HeldMissGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub noteskin: NoteSkin,
    pub mine_noteskin: Option<NoteSkin>,
    pub receptor_noteskin: Option<NoteSkin>,
    pub tap_explosion_noteskin: Option<NoteSkin>,
    pub tap_explosion_active_mask: TapExplosionMask,
    pub scroll_speed: ScrollSpeedSetting,
    pub no_cmod_alternative: NoCmodAlternative,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    pub insert_active_mask: InsertMask,
    pub remove_active_mask: RemoveMask,
    pub holds_active_mask: HoldsMask,
    pub accel_effects_active_mask: AccelEffectsMask,
    pub visual_effects_active_mask: VisualEffectsMask,
    pub appearance_effects_active_mask: AppearanceEffectsMask,
    pub attack_mode: AttackMode,
    pub hide_light_type: HideLightType,
    pub rescore_early_hits: bool,
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
    pub hide_early_dw_column_flash: bool,
    pub timing_windows: TimingWindowsOption,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub track_early_judgments: bool,
    pub scale_scatterplot: bool,
    pub scatterplot_max_window: ScatterplotMaxWindow,
    pub score_position: ScorePosition,
    pub score_display_mode: ScoreDisplayMode,
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    /// Pad-light brightness 0..=100 (gamma-mapped on send; 0 = off, 100 = full).
    pub pad_light_brightness: u8,
    pub judgment_tilt: bool,
    pub column_cues: bool,
    pub measure_cues: bool,
    pub judgment_back: bool,
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    pub live_timing_stats: bool,
    pub live_timing_stats_mask: LiveTimingStatsMask,
    pub rainbow_max: bool,
    pub responsive_colors: bool,
    pub show_life_percent: bool,
    pub tilt_multiplier: f32,
    pub tilt_min_threshold_ms: u32,
    pub tilt_max_threshold_ms: u32,
    pub error_bar_active_mask: ErrorBarMask,
    pub error_bar: ErrorBarStyle,
    pub error_bar_text: bool,
    pub text_error_bar_scalable: bool,
    pub text_error_bar_threshold_ms: u32,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub center_tick: bool,
    pub short_average_error_bar_enabled: bool,
    pub average_error_bar_intensity: f32,
    pub average_error_bar_interval_ms: u32,
    pub long_error_bar_enabled: bool,
    pub long_error_bar_intensity: f32,
    pub long_error_bar_threshold_ms: u32,
    pub long_error_bar_min_samples: u32,
    pub step_statistics: StepStatisticsMask,
    pub step_stats_extra: StepStatsExtra,
    pub target_score: TargetScoreSetting,
    pub lifemeter_type: LifeMeterType,
    pub measure_counter: MeasureCounter,
    pub measure_counter_lookahead: u8,
    pub measure_counter_left: bool,
    pub measure_counter_up: bool,
    pub measure_counter_vert: bool,
    pub broken_run: bool,
    pub run_timer: bool,
    pub measure_lines: MeasureLines,
    pub hide_targets: bool,
    pub hide_song_bg: bool,
    pub hide_combo: bool,
    pub hide_lifebar: bool,
    pub hide_score: bool,
    pub hide_danger: bool,
    pub hide_combo_explosions: bool,
    pub hide_username: bool,
    pub column_flash_on_miss: bool,
    pub column_flash_mask: ColumnFlashMask,
    pub column_flash_brightness: ColumnFlashBrightness,
    pub column_flash_size: ColumnFlashSize,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub transparent_density_graph_bg: bool,
    pub smx_fsr_display: bool,
    pub smx_pad_input_display: bool,
    pub mini_indicator: MiniIndicator,
    pub mini_indicator_score_type: MiniIndicatorScoreType,
    pub mini_indicator_subtractive_display: MiniIndicatorSubtractiveDisplay,
    pub mini_indicator_size: MiniIndicatorSize,
    pub mini_indicator_color: MiniIndicatorColor,
    pub mini_indicator_position: MiniIndicatorPosition,
    pub mini_percent: i32,
    pub spacing_percent: i32,
    pub perspective: Perspective,
    pub note_field_offset_x: i32,
    pub note_field_offset_y: i32,
    pub judgment_offset_x: i32,
    pub judgment_offset_y: i32,
    pub combo_offset_x: i32,
    pub combo_offset_y: i32,
    pub error_bar_offset_x: i32,
    pub error_bar_offset_y: i32,
    pub visual_delay_ms: i32,
    pub global_offset_shift_ms: i32,
}

fn default_player_options() -> PlayerOptionsData {
    PlayerOptionsData {
        background_filter: BackgroundFilter::default(),
        hold_judgment_graphic: HoldJudgmentGraphic::default(),
        held_miss_graphic: HeldMissGraphic::default(),
        judgment_graphic: JudgmentGraphic::default(),
        combo_font: ComboFont::default(),
        combo_colors: ComboColors::default(),
        combo_mode: ComboMode::default(),
        carry_combo_between_songs: true,
        noteskin: NoteSkin::default(),
        mine_noteskin: None,
        receptor_noteskin: None,
        tap_explosion_noteskin: None,
        tap_explosion_active_mask: TapExplosionMask::all(),
        scroll_speed: ScrollSpeedSetting::default(),
        no_cmod_alternative: NoCmodAlternative::default(),
        scroll_option: ScrollOption::default(),
        reverse_scroll: false,
        turn_option: TurnOption::default(),
        insert_active_mask: InsertMask::empty(),
        remove_active_mask: RemoveMask::empty(),
        holds_active_mask: HoldsMask::empty(),
        accel_effects_active_mask: AccelEffectsMask::empty(),
        visual_effects_active_mask: VisualEffectsMask::empty(),
        appearance_effects_active_mask: AppearanceEffectsMask::empty(),
        attack_mode: AttackMode::default(),
        hide_light_type: HideLightType::default(),
        rescore_early_hits: true,
        hide_early_dw_judgments: false,
        hide_early_dw_flash: false,
        hide_early_dw_column_flash: false,
        timing_windows: TimingWindowsOption::default(),
        show_fa_plus_window: false,
        show_ex_score: false,
        show_hard_ex_score: false,
        show_fa_plus_pane: false,
        fa_plus_10ms_blue_window: false,
        split_15_10ms: false,
        track_early_judgments: false,
        scale_scatterplot: false,
        scatterplot_max_window: ScatterplotMaxWindow::Off,
        score_position: ScorePosition::Normal,
        score_display_mode: ScoreDisplayMode::Normal,
        custom_fantastic_window: false,
        custom_fantastic_window_ms: CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS,
        pad_light_brightness: PAD_LIGHT_BRIGHTNESS_DEFAULT,
        judgment_tilt: false,
        column_cues: false,
        measure_cues: false,
        judgment_back: false,
        error_ms_display: false,
        display_scorebox: true,
        live_timing_stats: false,
        live_timing_stats_mask: LiveTimingStatsMask::empty(),
        rainbow_max: false,
        responsive_colors: false,
        show_life_percent: false,
        tilt_multiplier: 1.0,
        tilt_min_threshold_ms: TILT_MIN_THRESHOLD_DEFAULT_MS,
        tilt_max_threshold_ms: TILT_MAX_THRESHOLD_DEFAULT_MS,
        error_bar_active_mask: error_bar_mask_from_style(ErrorBarStyle::default(), false),
        error_bar: ErrorBarStyle::default(),
        error_bar_text: false,
        text_error_bar_scalable: false,
        text_error_bar_threshold_ms: TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT,
        error_bar_up: false,
        error_bar_multi_tick: false,
        error_bar_trim: ErrorBarTrim::default(),
        center_tick: false,
        short_average_error_bar_enabled: true,
        average_error_bar_intensity: AVERAGE_ERROR_BAR_INTENSITY_DEFAULT,
        average_error_bar_interval_ms: AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT,
        long_error_bar_enabled: true,
        long_error_bar_intensity: LONG_ERROR_BAR_INTENSITY_DEFAULT,
        long_error_bar_threshold_ms: LONG_ERROR_BAR_THRESHOLD_MS_DEFAULT,
        long_error_bar_min_samples: LONG_ERROR_BAR_MIN_SAMPLES_DEFAULT,
        step_statistics: StepStatisticsMask::default(),
        step_stats_extra: StepStatsExtra::default(),
        target_score: TargetScoreSetting::default(),
        lifemeter_type: LifeMeterType::default(),
        measure_counter: MeasureCounter::default(),
        measure_counter_lookahead: 2,
        measure_counter_left: true,
        measure_counter_up: false,
        measure_counter_vert: false,
        broken_run: false,
        run_timer: false,
        measure_lines: MeasureLines::default(),
        hide_targets: false,
        hide_song_bg: false,
        hide_combo: false,
        hide_lifebar: false,
        hide_score: false,
        hide_danger: false,
        hide_combo_explosions: false,
        hide_username: false,
        column_flash_on_miss: false,
        column_flash_mask: DEFAULT_COLUMN_FLASH_MASK,
        column_flash_brightness: ColumnFlashBrightness::Normal,
        column_flash_size: ColumnFlashSize::Default,
        subtractive_scoring: false,
        pacemaker: false,
        nps_graph_at_top: false,
        transparent_density_graph_bg: false,
        smx_fsr_display: false,
        smx_pad_input_display: false,
        mini_indicator: MiniIndicator::None,
        mini_indicator_score_type: MiniIndicatorScoreType::Itg,
        mini_indicator_subtractive_display: MiniIndicatorSubtractiveDisplay::Percent,
        mini_indicator_size: MiniIndicatorSize::Default,
        mini_indicator_color: MiniIndicatorColor::Default,
        mini_indicator_position: MiniIndicatorPosition::Default,
        mini_percent: 0,
        spacing_percent: 0,
        perspective: Perspective::default(),
        note_field_offset_x: 0,
        note_field_offset_y: 0,
        judgment_offset_x: 0,
        judgment_offset_y: 0,
        combo_offset_x: 0,
        combo_offset_y: 0,
        error_bar_offset_x: 0,
        error_bar_offset_y: 0,
        visual_delay_ms: 0,
        global_offset_shift_ms: 0,
    }
}

impl Default for PlayerOptionsData {
    fn default() -> Self {
        default_player_options()
    }
}

#[inline(always)]
fn load_u8_bool<F>(get: &mut F, key: &str, default: bool) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    get(key)
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(default, |v| v != 0)
}

pub fn load_visual_player_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.background_filter = get("BackgroundFilter")
        .and_then(|s| BackgroundFilter::from_str(&s).ok())
        .unwrap_or(options.background_filter);
    options.hold_judgment_graphic = get("HoldJudgmentGraphic")
        .and_then(|s| HoldJudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.hold_judgment_graphic.clone());
    options.held_miss_graphic = get("HeldGraphic")
        .or_else(|| get("HeldMissGraphic"))
        .and_then(|s| HeldMissGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.held_miss_graphic.clone());
    options.judgment_graphic = get("JudgmentGraphic")
        .and_then(|s| JudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.judgment_graphic.clone());
    options.combo_font = get("ComboFont")
        .and_then(|s| ComboFont::from_str(&s).ok())
        .unwrap_or(options.combo_font);
    options.combo_colors = get("ComboColors")
        .and_then(|s| ComboColors::from_str(&s).ok())
        .unwrap_or(options.combo_colors);
    options.combo_mode = get("ComboMode")
        .and_then(|s| ComboMode::from_str(&s).ok())
        .unwrap_or(options.combo_mode);
    options.carry_combo_between_songs = get("CarryComboBetweenSongs")
        .or_else(|| get("ComboContinuesBetweenSongs"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.carry_combo_between_songs, |v| v != 0);
    options.noteskin = get("NoteSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok())
        .unwrap_or_else(|| options.noteskin.clone());
    options.mine_noteskin = get("MineSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.receptor_noteskin = get("ReceptorSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.tap_explosion_noteskin =
        get("TapExplosionSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    let tap_explosion_mask_version = get("TapExplosionMaskVersion")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    options.tap_explosion_active_mask = get("TapExplosionMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|bits| normalize_tap_explosion_mask(bits, tap_explosion_mask_version))
        .unwrap_or(options.tap_explosion_active_mask);
    options.mini_percent = get("MiniPercent")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.mini_percent);
    options.spacing_percent = get("Spacing")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.spacing_percent);
    options.perspective = get("Perspective")
        .and_then(|s| Perspective::from_str(&s).ok())
        .unwrap_or(options.perspective);
    options.note_field_offset_x = get("NoteFieldOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_x);
    options.note_field_offset_y = get("NoteFieldOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_y);
    options.judgment_offset_x = get("JudgmentOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_x);
    options.judgment_offset_y = get("JudgmentOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_y);
    options.combo_offset_x = get("ComboOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_x);
    options.combo_offset_y = get("ComboOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_y);
    options.error_bar_offset_x = get("ErrorBarOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_x);
    options.error_bar_offset_y = get("ErrorBarOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_y);
    options.visual_delay_ms = get("VisualDelayMs")
        .or_else(|| get("VisualDelay"))
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.visual_delay_ms);
    options.global_offset_shift_ms = get("GlobalOffsetShiftMs")
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.global_offset_shift_ms);
}

pub fn load_timing_feedback_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.show_fa_plus_window =
        load_u8_bool(&mut get, "ShowFaPlusWindow", options.show_fa_plus_window);
    options.show_ex_score = load_u8_bool(&mut get, "ShowExScore", options.show_ex_score);
    options.show_hard_ex_score =
        load_u8_bool(&mut get, "ShowHardEXScore", options.show_hard_ex_score);
    options.show_fa_plus_pane = load_u8_bool(&mut get, "ShowFaPlusPane", options.show_fa_plus_pane);
    options.fa_plus_10ms_blue_window =
        load_u8_bool(&mut get, "SmallerWhite", options.fa_plus_10ms_blue_window);
    options.split_15_10ms = get("SplitWhites")
        .or_else(|| get("Split1510ms"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.split_15_10ms, |v| v != 0);
    options.track_early_judgments = load_u8_bool(
        &mut get,
        "TrackEarlyJudgments",
        options.track_early_judgments,
    );
    options.scale_scatterplot = get("ScaleScatterplot")
        .or_else(|| get("ScatterplotGreatMax"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.scale_scatterplot, |v| v != 0);
    options.scatterplot_max_window = get("ScatterplotMaxWindow")
        .and_then(|s| ScatterplotMaxWindow::from_str(&s).ok())
        .unwrap_or(options.scatterplot_max_window);
    options.score_position = get("ScorePosition")
        .and_then(|s| ScorePosition::from_str(&s).ok())
        .unwrap_or(options.score_position);
    options.score_display_mode = get("ScoreDisplay")
        .or_else(|| get("ScoreDisplayMode"))
        .and_then(|s| ScoreDisplayMode::from_str(&s).ok())
        .unwrap_or(options.score_display_mode);
    options.custom_fantastic_window = load_u8_bool(
        &mut get,
        "CustomFantasticWindow",
        options.custom_fantastic_window,
    );
    options.custom_fantastic_window_ms = get("CustomFantasticWindowMs")
        .and_then(|s| s.parse::<u8>().ok())
        .map(clamp_custom_fantastic_window_ms)
        .unwrap_or(options.custom_fantastic_window_ms);
    options.pad_light_brightness = get("PadLightBrightness")
        .and_then(|s| s.parse::<u8>().ok())
        .map(clamp_pad_light_brightness)
        .unwrap_or(options.pad_light_brightness);
    options.judgment_tilt = load_u8_bool(&mut get, "JudgmentTilt", options.judgment_tilt);
    options.column_cues = load_u8_bool(&mut get, "ColumnCues", options.column_cues);
    options.measure_cues = load_u8_bool(&mut get, "MeasureCues", options.measure_cues);
    options.judgment_back = load_u8_bool(&mut get, "JudgmentBack", options.judgment_back);
    options.error_ms_display = load_u8_bool(&mut get, "ErrorMSDisplay", options.error_ms_display);
    options.display_scorebox = load_u8_bool(&mut get, "DisplayScorebox", options.display_scorebox);
    let legacy_live_timing_stats =
        load_u8_bool(&mut get, "LiveTimingStats", options.live_timing_stats);
    if let Some(mask) = get("LiveTimingStatsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(LiveTimingStatsMask::from_bits_truncate)
    {
        options.live_timing_stats_mask = mask;
        options.live_timing_stats = legacy_live_timing_stats;
    } else {
        options.live_timing_stats = legacy_live_timing_stats;
        if legacy_live_timing_stats {
            options.live_timing_stats_mask = LiveTimingStatsMask::all();
        }
    }
    options.rainbow_max = load_u8_bool(&mut get, "RainbowMax", options.rainbow_max);
    options.responsive_colors =
        load_u8_bool(&mut get, "ResponsiveColors", options.responsive_colors);
    options.show_life_percent =
        load_u8_bool(&mut get, "ShowLifePercent", options.show_life_percent);
    options.tilt_multiplier = get("TiltMultiplier")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(options.tilt_multiplier);
    options.tilt_min_threshold_ms = get("TiltMinThresholdMs")
        .or_else(|| get("TiltCutoffMs"))
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_tilt_threshold_ms)
        .unwrap_or(options.tilt_min_threshold_ms);
    options.tilt_max_threshold_ms = get("TiltMaxThresholdMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_tilt_threshold_ms)
        .unwrap_or(options.tilt_max_threshold_ms);
    if options.tilt_max_threshold_ms < options.tilt_min_threshold_ms {
        options.tilt_max_threshold_ms = options.tilt_min_threshold_ms;
    }
}

pub fn load_error_bar_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.error_bar = get("ErrorBar")
        .and_then(|s| ErrorBarStyle::from_str(&s).ok())
        .unwrap_or(options.error_bar);
    options.error_bar_text = get("ErrorBarText")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_text, |v| v != 0);
    options.text_error_bar_scalable = get("TextErrorBarScalable")
        .and_then(|s| parse_profile_bool(&s))
        .or_else(|| get("TextErrorBar10ms").and_then(|s| parse_profile_bool(&s)))
        .unwrap_or(options.text_error_bar_scalable);
    options.text_error_bar_threshold_ms = get("TextErrorBarThresholdMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_text_error_bar_threshold_ms)
        .unwrap_or(options.text_error_bar_threshold_ms);
    let mask_from_key = get("ErrorBarMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(ErrorBarMask::from_bits_truncate);
    let colorful = get("Colorful")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let monochrome = get("Monochrome")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let text = get("Text")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let highlight = get("Highlight")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let average = get("Average")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let mask_from_flags = if colorful.is_some()
        || monochrome.is_some()
        || text.is_some()
        || highlight.is_some()
        || average.is_some()
    {
        let mut mask = ErrorBarMask::empty();
        if colorful.unwrap_or(false) {
            mask |= ErrorBarMask::COLORFUL;
        }
        if monochrome.unwrap_or(false) {
            mask |= ErrorBarMask::MONOCHROME;
        }
        if text.unwrap_or(false) {
            mask |= ErrorBarMask::TEXT;
        }
        if highlight.unwrap_or(false) {
            mask |= ErrorBarMask::HIGHLIGHT;
        }
        if average.unwrap_or(false) {
            mask |= ErrorBarMask::AVERAGE;
        }
        Some(mask)
    } else {
        None
    };
    options.error_bar_active_mask = mask_from_key
        .or(mask_from_flags)
        .unwrap_or_else(|| error_bar_mask_from_style(options.error_bar, options.error_bar_text));
    options.error_bar = error_bar_style_from_mask(options.error_bar_active_mask);
    options.error_bar_text = error_bar_text_from_mask(options.error_bar_active_mask);
    options.error_bar_up = get("ErrorBarUp")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_up, |v| v != 0);
    options.error_bar_multi_tick = get("ErrorBarMultiTick")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_multi_tick, |v| v != 0);
    options.error_bar_trim = get("ErrorBarTrim")
        .and_then(|s| ErrorBarTrim::from_str(&s).ok())
        .unwrap_or(options.error_bar_trim);
    options.center_tick = get("CenterTick")
        .and_then(|s| parse_profile_bool(&s))
        .unwrap_or(options.center_tick);
    options.short_average_error_bar_enabled = get("ShortAverageErrorBar")
        .and_then(|s| parse_profile_bool(&s))
        .or_else(|| {
            get("LongAvgTickOnly")
                .and_then(|s| parse_profile_bool(&s))
                .map(|long_only| !long_only)
        })
        .unwrap_or(options.short_average_error_bar_enabled);
    options.average_error_bar_intensity = get("AverageErrorBarIntensity")
        .or_else(|| get("HighlightZoom"))
        .and_then(|s| s.trim().trim_end_matches('x').trim().parse::<f32>().ok())
        .map(clamp_average_error_bar_intensity)
        .unwrap_or(options.average_error_bar_intensity);
    options.average_error_bar_interval_ms = get("AverageErrorBarIntervalMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_average_error_bar_interval_ms)
        .or_else(|| {
            get("HighlightAverageMs")
                .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
                .filter(|&ms| ms > 0)
                .map(clamp_average_error_bar_interval_ms)
        })
        .unwrap_or(options.average_error_bar_interval_ms);
    options.long_error_bar_enabled = get("LongErrorBar")
        .and_then(|s| s.trim().parse::<i32>().ok())
        .map_or(options.long_error_bar_enabled, |v| v != 0);
    options.long_error_bar_intensity = get("LongErrorBarIntensity")
        .and_then(|s| s.trim().trim_end_matches('x').trim().parse::<f32>().ok())
        .map(clamp_long_error_bar_intensity)
        .unwrap_or(options.long_error_bar_intensity);
    options.long_error_bar_threshold_ms = get("LongErrorBarThresholdMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_long_error_bar_threshold_ms)
        .unwrap_or(options.long_error_bar_threshold_ms);
    options.long_error_bar_min_samples = get("LongErrorBarMinSamples")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(clamp_long_error_bar_min_samples)
        .unwrap_or(options.long_error_bar_min_samples);
}

pub fn append_player_options_section(
    content: &mut String,
    section: &str,
    options: &PlayerOptionsData,
) {
    content.push_str(&format!("[{section}]\n"));
    content.push_str(&format!("BackgroundFilter={}\n", options.background_filter));
    content.push_str(&format!("ScrollSpeed={}\n", options.scroll_speed));
    content.push_str(&format!(
        "NoCmodAlternative={}\n",
        options.no_cmod_alternative
    ));
    content.push_str(&format!("Scroll={}\n", options.scroll_option));
    content.push_str(&format!("Turn={}\n", options.turn_option));
    content.push_str(&format!(
        "InsertMask={}\n",
        options.insert_active_mask.bits()
    ));
    content.push_str(&format!(
        "RemoveMask={}\n",
        options.remove_active_mask.bits()
    ));
    content.push_str(&format!("HoldsMask={}\n", options.holds_active_mask.bits()));
    content.push_str(&format!(
        "AccelEffectsMask={}\n",
        options.accel_effects_active_mask.bits()
    ));
    content.push_str(&format!(
        "VisualEffectsMask={}\n",
        options.visual_effects_active_mask.bits()
    ));
    content.push_str(&format!(
        "AppearanceEffectsMask={}\n",
        options.appearance_effects_active_mask.bits()
    ));
    content.push_str(&format!("AttackMode={}\n", options.attack_mode));
    content.push_str(&format!("HideLightType={}\n", options.hide_light_type));
    content.push_str(&format!(
        "RescoreEarlyHits={}\n",
        i32::from(options.rescore_early_hits)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffJudgments={}\n",
        i32::from(options.hide_early_dw_judgments)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffFlash={}\n",
        i32::from(options.hide_early_dw_flash)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffColumnFlash={}\n",
        i32::from(options.hide_early_dw_column_flash)
    ));
    content.push_str(&format!("TimingWindows={}\n", options.timing_windows));
    content.push_str(&format!(
        "HideTargets={}\n",
        i32::from(options.hide_targets)
    ));
    content.push_str(&format!("HideSongBG={}\n", i32::from(options.hide_song_bg)));
    content.push_str(&format!("HideCombo={}\n", i32::from(options.hide_combo)));
    content.push_str(&format!(
        "HideLifebar={}\n",
        i32::from(options.hide_lifebar)
    ));
    content.push_str(&format!("HideScore={}\n", i32::from(options.hide_score)));
    content.push_str(&format!("HideDanger={}\n", i32::from(options.hide_danger)));
    content.push_str(&format!(
        "HideComboExplosions={}\n",
        i32::from(options.hide_combo_explosions)
    ));
    content.push_str(&format!(
        "HideUsername={}\n",
        i32::from(options.hide_username)
    ));
    content.push_str(&format!(
        "ColumnFlashOnMiss={}\n",
        i32::from(options.column_flash_on_miss)
    ));
    content.push_str(&format!(
        "ColumnFlashMask={}\n",
        options.column_flash_mask.bits()
    ));
    content.push_str(&format!(
        "ColumnFlashBrightness={}\n",
        options.column_flash_brightness
    ));
    content.push_str(&format!("ColumnFlashSize={}\n", options.column_flash_size));
    content.push_str(&format!(
        "SubtractiveScoring={}\n",
        i32::from(options.subtractive_scoring)
    ));
    content.push_str(&format!("Pacemaker={}\n", i32::from(options.pacemaker)));
    content.push_str(&format!(
        "NPSGraphAtTop={}\n",
        i32::from(options.nps_graph_at_top)
    ));
    content.push_str(&format!(
        "TransparentDensityGraphBackground={}\n",
        i32::from(options.transparent_density_graph_bg)
    ));
    content.push_str(&format!(
        "SmxFsrDisplay={}\n",
        i32::from(options.smx_fsr_display)
    ));
    content.push_str(&format!(
        "SmxPadInputDisplay={}\n",
        i32::from(options.smx_pad_input_display)
    ));
    content.push_str(&format!("MiniIndicator={}\n", options.mini_indicator));
    content.push_str(&format!(
        "MiniIndicatorScoreType={}\n",
        options.mini_indicator_score_type
    ));
    content.push_str(&format!(
        "MiniIndicatorSubtractiveDisplay={}\n",
        options.mini_indicator_subtractive_display
    ));
    content.push_str(&format!(
        "MiniIndicatorSize={}\n",
        options.mini_indicator_size
    ));
    content.push_str(&format!(
        "MiniIndicatorColor={}\n",
        options.mini_indicator_color
    ));
    content.push_str(&format!(
        "MiniIndicatorPosition={}\n",
        options.mini_indicator_position
    ));
    content.push_str(&format!(
        "ReverseScroll={}\n",
        i32::from(options.reverse_scroll)
    ));
    content.push_str(&format!(
        "ShowFaPlusWindow={}\n",
        i32::from(options.show_fa_plus_window)
    ));
    content.push_str(&format!(
        "ShowExScore={}\n",
        i32::from(options.show_ex_score)
    ));
    content.push_str(&format!(
        "ShowHardEXScore={}\n",
        i32::from(options.show_hard_ex_score)
    ));
    content.push_str(&format!(
        "ShowFaPlusPane={}\n",
        i32::from(options.show_fa_plus_pane)
    ));
    content.push_str(&format!(
        "SmallerWhite={}\n",
        i32::from(options.fa_plus_10ms_blue_window)
    ));
    content.push_str(&format!(
        "SplitWhites={}\n",
        i32::from(options.split_15_10ms)
    ));
    content.push_str(&format!(
        "TrackEarlyJudgments={}\n",
        i32::from(options.track_early_judgments)
    ));
    content.push_str(&format!(
        "ScaleScatterplot={}\n",
        i32::from(options.scale_scatterplot)
    ));
    content.push_str(&format!(
        "ScatterplotMaxWindow={}\n",
        options.scatterplot_max_window
    ));
    content.push_str(&format!("ScorePosition={}\n", options.score_position));
    content.push_str(&format!("ScoreDisplay={}\n", options.score_display_mode));
    content.push_str(&format!(
        "CustomFantasticWindow={}\n",
        i32::from(options.custom_fantastic_window)
    ));
    content.push_str(&format!(
        "CustomFantasticWindowMs={}\n",
        options.custom_fantastic_window_ms
    ));
    content.push_str(&format!(
        "PadLightBrightness={}\n",
        options.pad_light_brightness
    ));
    content.push_str(&format!(
        "JudgmentTilt={}\n",
        i32::from(options.judgment_tilt)
    ));
    content.push_str(&format!("ColumnCues={}\n", i32::from(options.column_cues)));
    content.push_str(&format!(
        "MeasureCues={}\n",
        i32::from(options.measure_cues)
    ));
    content.push_str(&format!(
        "JudgmentBack={}\n",
        i32::from(options.judgment_back)
    ));
    content.push_str(&format!(
        "ErrorMSDisplay={}\n",
        i32::from(options.error_ms_display)
    ));
    content.push_str(&format!(
        "DisplayScorebox={}\n",
        i32::from(options.display_scorebox)
    ));
    content.push_str(&format!(
        "LiveTimingStats={}\n",
        i32::from(options.live_timing_stats)
    ));
    content.push_str(&format!(
        "LiveTimingStatsMask={}\n",
        options.live_timing_stats_mask.bits()
    ));
    content.push_str(&format!("RainbowMax={}\n", i32::from(options.rainbow_max)));
    content.push_str(&format!(
        "ResponsiveColors={}\n",
        i32::from(options.responsive_colors)
    ));
    content.push_str(&format!(
        "ShowLifePercent={}\n",
        i32::from(options.show_life_percent)
    ));
    content.push_str(&format!("TiltMultiplier={}\n", options.tilt_multiplier));
    content.push_str(&format!(
        "TiltMinThresholdMs={}\n",
        options.tilt_min_threshold_ms
    ));
    content.push_str(&format!(
        "TiltMaxThresholdMs={}\n",
        options.tilt_max_threshold_ms
    ));
    content.push_str(&format!("ErrorBar={}\n", options.error_bar));
    content.push_str(&format!(
        "ErrorBarText={}\n",
        i32::from(options.error_bar_text)
    ));
    content.push_str(&format!(
        "TextErrorBarScalable={}\n",
        i32::from(options.text_error_bar_scalable)
    ));
    content.push_str(&format!(
        "TextErrorBar10ms={}\n",
        i32::from(options.text_error_bar_scalable)
    ));
    content.push_str(&format!(
        "TextErrorBarThresholdMs={}\n",
        clamp_text_error_bar_threshold_ms(options.text_error_bar_threshold_ms)
    ));
    content.push_str(&format!(
        "ErrorBarMask={}\n",
        options.error_bar_active_mask.bits()
    ));
    content.push_str(&format!(
        "Colorful={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::COLORFUL)
        )
    ));
    content.push_str(&format!(
        "Monochrome={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::MONOCHROME)
        )
    ));
    content.push_str(&format!(
        "Text={}\n",
        i32::from(options.error_bar_active_mask.contains(ErrorBarMask::TEXT))
    ));
    content.push_str(&format!(
        "Highlight={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::HIGHLIGHT)
        )
    ));
    content.push_str(&format!(
        "Average={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::AVERAGE)
        )
    ));
    content.push_str(&format!("ErrorBarUp={}\n", i32::from(options.error_bar_up)));
    content.push_str(&format!(
        "ErrorBarMultiTick={}\n",
        i32::from(options.error_bar_multi_tick)
    ));
    content.push_str(&format!("ErrorBarTrim={}\n", options.error_bar_trim));
    content.push_str(&format!("CenterTick={}\n", i32::from(options.center_tick)));
    content.push_str(&format!(
        "ShortAverageErrorBar={}\n",
        i32::from(options.short_average_error_bar_enabled)
    ));
    content.push_str(&format!(
        "AverageErrorBarIntensity={:.2}\n",
        clamp_average_error_bar_intensity(options.average_error_bar_intensity)
    ));
    content.push_str(&format!(
        "AverageErrorBarIntervalMs={}\n",
        clamp_average_error_bar_interval_ms(options.average_error_bar_interval_ms)
    ));
    content.push_str(&format!(
        "LongErrorBar={}\n",
        i32::from(options.long_error_bar_enabled)
    ));
    content.push_str(&format!(
        "LongErrorBarIntensity={:.2}\n",
        clamp_long_error_bar_intensity(options.long_error_bar_intensity)
    ));
    content.push_str(&format!(
        "LongErrorBarThresholdMs={}\n",
        clamp_long_error_bar_threshold_ms(options.long_error_bar_threshold_ms)
    ));
    content.push_str(&format!(
        "LongErrorBarMinSamples={}\n",
        clamp_long_error_bar_min_samples(options.long_error_bar_min_samples)
    ));
    content.push_str(&format!("StepStatistics={}\n", options.step_statistics));
    content.push_str(&format!("StepStatsExtra={}\n", options.step_stats_extra));
    content.push_str(&format!("TargetScore={}\n", options.target_score));
    content.push_str(&format!("LifeMeterType={}\n", options.lifemeter_type));
    content.push_str(&format!("MeasureCounter={}\n", options.measure_counter));
    content.push_str(&format!(
        "MeasureCounterLookahead={}\n",
        options.measure_counter_lookahead
    ));
    content.push_str(&format!(
        "MeasureCounterLeft={}\n",
        i32::from(options.measure_counter_left)
    ));
    content.push_str(&format!(
        "MeasureCounterUp={}\n",
        i32::from(options.measure_counter_up)
    ));
    content.push_str(&format!(
        "MeasureCounterVert={}\n",
        i32::from(options.measure_counter_vert)
    ));
    content.push_str(&format!("BrokenRun={}\n", i32::from(options.broken_run)));
    content.push_str(&format!("RunTimer={}\n", i32::from(options.run_timer)));
    content.push_str(&format!("MeasureLines={}\n", options.measure_lines));
    content.push_str(&format!(
        "HoldJudgmentGraphic={}\n",
        options.hold_judgment_graphic
    ));
    content.push_str(&format!("HeldGraphic={}\n", options.held_miss_graphic));
    content.push_str(&format!("JudgmentGraphic={}\n", options.judgment_graphic));
    content.push_str(&format!("ComboFont={}\n", options.combo_font));
    content.push_str(&format!("ComboColors={}\n", options.combo_colors));
    content.push_str(&format!("ComboMode={}\n", options.combo_mode));
    content.push_str(&format!(
        "CarryComboBetweenSongs={}\n",
        i32::from(options.carry_combo_between_songs)
    ));
    content.push_str(&format!("NoteSkin={}\n", options.noteskin));
    content.push_str(&format!(
        "MineSkin={}\n",
        options.mine_noteskin.as_ref().map_or("", NoteSkin::as_str)
    ));
    content.push_str(&format!(
        "ReceptorSkin={}\n",
        options
            .receptor_noteskin
            .as_ref()
            .map_or("", NoteSkin::as_str)
    ));
    content.push_str(&format!(
        "TapExplosionSkin={}\n",
        options
            .tap_explosion_noteskin
            .as_ref()
            .map_or("", NoteSkin::as_str)
    ));
    content.push_str(&format!(
        "TapExplosionMask={}\n",
        options.tap_explosion_active_mask.bits()
    ));
    content.push_str(&format!(
        "TapExplosionMaskVersion={}\n",
        TAP_EXPLOSION_MASK_VERSION
    ));
    content.push_str(&format!("MiniPercent={}\n", options.mini_percent));
    content.push_str(&format!("Spacing={}\n", options.spacing_percent));
    content.push_str(&format!("Perspective={}\n", options.perspective));
    content.push_str(&format!(
        "NoteFieldOffsetX={}\n",
        options.note_field_offset_x
    ));
    content.push_str(&format!(
        "NoteFieldOffsetY={}\n",
        options.note_field_offset_y
    ));
    content.push_str(&format!("JudgmentOffsetX={}\n", options.judgment_offset_x));
    content.push_str(&format!("JudgmentOffsetY={}\n", options.judgment_offset_y));
    content.push_str(&format!("ComboOffsetX={}\n", options.combo_offset_x));
    content.push_str(&format!("ComboOffsetY={}\n", options.combo_offset_y));
    content.push_str(&format!("ErrorBarOffsetX={}\n", options.error_bar_offset_x));
    content.push_str(&format!("ErrorBarOffsetY={}\n", options.error_bar_offset_y));
    content.push_str(&format!("VisualDelayMs={}\n", options.visual_delay_ms));
    content.push_str(&format!(
        "GlobalOffsetShiftMs={}\n",
        options.global_offset_shift_ms
    ));
    content.push('\n');
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub display_name: String,
    pub player_initials: String,
    // Profile stats (Simply Love / StepMania semantics).
    pub weight_pounds: i32,
    pub birth_year: i32,
    pub calories_burned_today: f32,
    pub calories_burned_day: String,
    pub ignore_step_count_calories: bool,
    pub groovestats_api_key: String,
    pub groovestats_is_pad_player: bool,
    pub groovestats_username: String,
    pub arrowcloud_api_key: String,
    // Style-scoped player options are stored per chart family below.
    // These top-level fields hold the snapshot currently applied for the
    // active session play style so existing read paths can stay simple.
    pub background_filter: BackgroundFilter,
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub held_miss_graphic: HeldMissGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub current_combo: u32,
    pub known_pack_names: HashSet<String>,
    pub favorites: HashSet<String>,
    pub favorited_packs: HashSet<String>,
    pub noteskin: NoteSkin,
    pub mine_noteskin: Option<NoteSkin>,
    pub receptor_noteskin: Option<NoteSkin>,
    pub tap_explosion_noteskin: Option<NoteSkin>,
    pub tap_explosion_active_mask: TapExplosionMask,
    pub avatar_path: Option<PathBuf>,
    pub avatar_texture_key: Option<String>,
    pub scroll_speed: ScrollSpeedSetting,
    pub no_cmod_alternative: NoCmodAlternative,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    // zmod uncommon modifiers (ScreenPlayerOptions3).
    // Bit order mirrors row choice order in metrics.ini.
    pub insert_active_mask: InsertMask,
    pub remove_active_mask: RemoveMask,
    pub holds_active_mask: HoldsMask,
    pub accel_effects_active_mask: AccelEffectsMask,
    pub visual_effects_active_mask: VisualEffectsMask,
    pub appearance_effects_active_mask: AppearanceEffectsMask,
    pub attack_mode: AttackMode,
    pub hide_light_type: HideLightType,
    // Allow early Decent/WayOff hits to be rescored to better judgments.
    pub rescore_early_hits: bool,
    // Visual behavior for early Decent/Way Off hits (Simply Love semantics).
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
    pub hide_early_dw_column_flash: bool,
    pub timing_windows: TimingWindowsOption,
    // FA+ visual options (Simply Love semantics).
    // These do not change core timing semantics; they only affect HUD/UX.
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    // 10ms blue Fantastic window for FA+ window display (Arrow Cloud: "SmallerWhite").
    pub fa_plus_10ms_blue_window: bool,
    // zmod SplitWhites: keep the 15ms blue FA+ judgment base and overlay the
    // white Fantastic art for 10ms-15ms hits. Visual only.
    pub split_15_10ms: bool,
    // Track and display per-column early judgment counts on evaluation (zmod/Arrow Cloud semantics).
    pub track_early_judgments: bool,
    // Constrain the evaluation scatter plot's vertical scale to a Great
    // upper cap and a Fantastic lower floor (zmod's `ScaleGraph`-style
    // toggle). Off uses the original behavior of an Excellent floor with
    // no upper cap.
    pub scale_scatterplot: bool,
    // Hard cap for the evaluation scatter plot's vertical scale. When
    // anything other than `Off`, this overrides `scale_scatterplot`'s
    // tier-snapped behavior and clamps the worst-window ms to the
    // selected judgment tier (Chris's SL `ScaleGraph`-per-tier semantics).
    pub scatterplot_max_window: ScatterplotMaxWindow,
    pub score_position: ScorePosition,
    pub score_display_mode: ScoreDisplayMode,
    // Custom blue Fantastic window in milliseconds (1..22), shared by FA+ W0 and H.EX split.
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    /// Pad-light brightness 0..=100. Seeded from the StepManiaX machine default
    /// when the profile is created; the player adjusts it in Player Options.
    pub pad_light_brightness: u8,
    // Judgment tilt (Simply Love semantics).
    pub judgment_tilt: bool,
    pub column_cues: bool,
    pub measure_cues: bool,
    // zmod ExtraAesthetics: draw judgments/error timing HUD behind notes.
    pub judgment_back: bool,
    // zmod ExtraAesthetics: offset indicator (ErrorMSDisplay).
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    pub live_timing_stats: bool,
    pub live_timing_stats_mask: LiveTimingStatsMask,
    // zmod LifeBarOptions (Arrow Cloud semantics).
    pub rainbow_max: bool,
    pub responsive_colors: bool,
    pub show_life_percent: bool,
    pub tilt_multiplier: f32,
    pub tilt_min_threshold_ms: u32,
    pub tilt_max_threshold_ms: u32,
    // Error bar (zmod semantics): each bit toggles one submodule in the
    // SelectMultiple row (Colorful/Monochrome/Text/Highlight/Average).
    pub error_bar_active_mask: ErrorBarMask,
    pub error_bar: ErrorBarStyle,
    // Backward-compatible text flag written to profile.ini.
    pub error_bar_text: bool,
    // Optional Text error bar mode that surfaces hits beyond a configured
    // threshold independently of the active judgment windows.
    pub text_error_bar_scalable: bool,
    pub text_error_bar_threshold_ms: u32,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub center_tick: bool,
    pub short_average_error_bar_enabled: bool,
    pub average_error_bar_intensity: f32,
    pub average_error_bar_interval_ms: u32,
    pub long_error_bar_enabled: bool,
    pub long_error_bar_intensity: f32,
    pub long_error_bar_threshold_ms: u32,
    pub long_error_bar_min_samples: u32,
    pub step_statistics: StepStatisticsMask,
    pub step_stats_extra: StepStatsExtra,
    pub target_score: TargetScoreSetting,
    pub lifemeter_type: LifeMeterType,
    pub measure_counter: MeasureCounter,
    pub measure_counter_lookahead: u8,
    pub measure_counter_left: bool,
    pub measure_counter_up: bool,
    pub measure_counter_vert: bool,
    pub broken_run: bool,
    pub run_timer: bool,
    pub measure_lines: MeasureLines,
    // "Hide" options (Simply Love semantics).
    pub hide_targets: bool,
    pub hide_song_bg: bool,
    pub hide_combo: bool,
    pub hide_lifebar: bool,
    pub hide_score: bool,
    pub hide_danger: bool,
    pub hide_combo_explosions: bool,
    pub hide_username: bool,
    // Gameplay extras (Simply Love semantics).
    pub column_flash_on_miss: bool,
    pub column_flash_mask: ColumnFlashMask,
    pub column_flash_brightness: ColumnFlashBrightness,
    pub column_flash_size: ColumnFlashSize,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub transparent_density_graph_bg: bool,
    pub smx_fsr_display: bool,
    pub smx_pad_input_display: bool,
    pub mini_indicator: MiniIndicator,
    pub mini_indicator_score_type: MiniIndicatorScoreType,
    pub mini_indicator_subtractive_display: MiniIndicatorSubtractiveDisplay,
    pub mini_indicator_size: MiniIndicatorSize,
    pub mini_indicator_color: MiniIndicatorColor,
    pub mini_indicator_position: MiniIndicatorPosition,
    // Mini modifier as a percentage, mirroring Simply Love semantics.
    // 0 = normal size, 100 = 100% Mini (smaller), negative values enlarge.
    pub mini_percent: i32,
    /// Horizontal spacing between note columns as a percentage (zmod parity).
    /// 0 = noteskin default, +N% scales lateral column offsets by
    /// `1 + N/100`. Range -100..=100 (capped on read to stay sane).
    pub spacing_percent: i32,
    pub perspective: Perspective,
    // NoteField positional offsets (Simply Love semantics).
    // X is non-negative and interpreted relative to player side:
    // for P1, positive values move the field left.
    pub note_field_offset_x: i32,
    // Y is applied directly to the notefield and related HUD,
    // positive values move everything down.
    pub note_field_offset_y: i32,
    // Independent HUD element offsets in logical pixels.
    // Positive X = right, positive Y = down.
    pub judgment_offset_x: i32,
    pub judgment_offset_y: i32,
    pub combo_offset_x: i32,
    pub combo_offset_y: i32,
    pub error_bar_offset_x: i32,
    pub error_bar_offset_y: i32,
    // Per-player visual delay (Simply Love semantics). Stored in milliseconds.
    // Negative values shift arrows upwards; positive values shift them down.
    pub visual_delay_ms: i32,
    // Per-player timing shift applied on top of machine global offset. Stored in milliseconds.
    pub global_offset_shift_ms: i32,
    pub player_options_singles: PlayerOptionsData,
    pub player_options_doubles: PlayerOptionsData,
    // Persisted "last played" selections so future sessions can reopen
    // SelectMusic on the most recently played chart for each chart family.
    // Singles is shared by Single and Versus. Double uses its own entry.
    pub last_played_singles: LastPlayed,
    pub last_played_doubles: LastPlayed,
    pub last_played_course_singles: LastPlayedCourse,
    pub last_played_course_doubles: LastPlayedCourse,
}

impl Default for Profile {
    fn default() -> Self {
        let player_options = PlayerOptionsData::default();
        Self {
            display_name: "Player 1".to_string(),
            player_initials: "P1".to_string(),
            weight_pounds: 0,
            birth_year: 0,
            calories_burned_today: 0.0,
            calories_burned_day: String::new(),
            ignore_step_count_calories: false,
            groovestats_api_key: String::new(),
            groovestats_is_pad_player: false,
            groovestats_username: String::new(),
            arrowcloud_api_key: String::new(),
            background_filter: player_options.background_filter,
            hold_judgment_graphic: player_options.hold_judgment_graphic.clone(),
            held_miss_graphic: player_options.held_miss_graphic.clone(),
            judgment_graphic: player_options.judgment_graphic.clone(),
            combo_font: player_options.combo_font,
            combo_colors: player_options.combo_colors,
            combo_mode: player_options.combo_mode,
            carry_combo_between_songs: player_options.carry_combo_between_songs,
            current_combo: 0,
            known_pack_names: HashSet::new(),
            favorites: HashSet::new(),
            favorited_packs: HashSet::new(),
            noteskin: player_options.noteskin.clone(),
            mine_noteskin: player_options.mine_noteskin.clone(),
            receptor_noteskin: player_options.receptor_noteskin.clone(),
            tap_explosion_noteskin: player_options.tap_explosion_noteskin.clone(),
            tap_explosion_active_mask: player_options.tap_explosion_active_mask,
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: player_options.scroll_speed,
            no_cmod_alternative: player_options.no_cmod_alternative,
            scroll_option: player_options.scroll_option,
            reverse_scroll: player_options.reverse_scroll,
            turn_option: player_options.turn_option,
            insert_active_mask: player_options.insert_active_mask,
            remove_active_mask: player_options.remove_active_mask,
            holds_active_mask: player_options.holds_active_mask,
            accel_effects_active_mask: player_options.accel_effects_active_mask,
            visual_effects_active_mask: player_options.visual_effects_active_mask,
            appearance_effects_active_mask: player_options.appearance_effects_active_mask,
            attack_mode: player_options.attack_mode,
            hide_light_type: player_options.hide_light_type,
            rescore_early_hits: player_options.rescore_early_hits,
            hide_early_dw_judgments: player_options.hide_early_dw_judgments,
            hide_early_dw_flash: player_options.hide_early_dw_flash,
            hide_early_dw_column_flash: player_options.hide_early_dw_column_flash,
            timing_windows: player_options.timing_windows,
            show_fa_plus_window: player_options.show_fa_plus_window,
            show_ex_score: player_options.show_ex_score,
            show_hard_ex_score: player_options.show_hard_ex_score,
            show_fa_plus_pane: player_options.show_fa_plus_pane,
            fa_plus_10ms_blue_window: player_options.fa_plus_10ms_blue_window,
            split_15_10ms: player_options.split_15_10ms,
            track_early_judgments: player_options.track_early_judgments,
            scale_scatterplot: player_options.scale_scatterplot,
            scatterplot_max_window: player_options.scatterplot_max_window,
            score_position: player_options.score_position,
            score_display_mode: player_options.score_display_mode,
            custom_fantastic_window: player_options.custom_fantastic_window,
            custom_fantastic_window_ms: player_options.custom_fantastic_window_ms,
            pad_light_brightness: player_options.pad_light_brightness,
            judgment_tilt: player_options.judgment_tilt,
            column_cues: player_options.column_cues,
            measure_cues: player_options.measure_cues,
            judgment_back: player_options.judgment_back,
            error_ms_display: player_options.error_ms_display,
            display_scorebox: player_options.display_scorebox,
            live_timing_stats: player_options.live_timing_stats,
            live_timing_stats_mask: player_options.live_timing_stats_mask,
            rainbow_max: player_options.rainbow_max,
            responsive_colors: player_options.responsive_colors,
            show_life_percent: player_options.show_life_percent,
            tilt_multiplier: player_options.tilt_multiplier,
            tilt_min_threshold_ms: player_options.tilt_min_threshold_ms,
            tilt_max_threshold_ms: player_options.tilt_max_threshold_ms,
            error_bar: player_options.error_bar,
            error_bar_active_mask: player_options.error_bar_active_mask,
            error_bar_text: player_options.error_bar_text,
            text_error_bar_scalable: player_options.text_error_bar_scalable,
            text_error_bar_threshold_ms: player_options.text_error_bar_threshold_ms,
            error_bar_up: player_options.error_bar_up,
            error_bar_multi_tick: player_options.error_bar_multi_tick,
            error_bar_trim: player_options.error_bar_trim,
            center_tick: player_options.center_tick,
            short_average_error_bar_enabled: player_options.short_average_error_bar_enabled,
            average_error_bar_intensity: player_options.average_error_bar_intensity,
            average_error_bar_interval_ms: player_options.average_error_bar_interval_ms,
            long_error_bar_enabled: player_options.long_error_bar_enabled,
            long_error_bar_intensity: player_options.long_error_bar_intensity,
            long_error_bar_threshold_ms: player_options.long_error_bar_threshold_ms,
            long_error_bar_min_samples: player_options.long_error_bar_min_samples,
            step_statistics: player_options.step_statistics,
            step_stats_extra: player_options.step_stats_extra,
            target_score: player_options.target_score,
            lifemeter_type: player_options.lifemeter_type,
            measure_counter: player_options.measure_counter,
            measure_counter_lookahead: player_options.measure_counter_lookahead,
            measure_counter_left: player_options.measure_counter_left,
            measure_counter_up: player_options.measure_counter_up,
            measure_counter_vert: player_options.measure_counter_vert,
            broken_run: player_options.broken_run,
            run_timer: player_options.run_timer,
            measure_lines: player_options.measure_lines,
            hide_targets: player_options.hide_targets,
            hide_song_bg: player_options.hide_song_bg,
            hide_combo: player_options.hide_combo,
            hide_lifebar: player_options.hide_lifebar,
            hide_score: player_options.hide_score,
            hide_danger: player_options.hide_danger,
            hide_combo_explosions: player_options.hide_combo_explosions,
            hide_username: player_options.hide_username,
            column_flash_on_miss: player_options.column_flash_on_miss,
            column_flash_mask: player_options.column_flash_mask,
            column_flash_brightness: player_options.column_flash_brightness,
            column_flash_size: player_options.column_flash_size,
            subtractive_scoring: player_options.subtractive_scoring,
            pacemaker: player_options.pacemaker,
            nps_graph_at_top: player_options.nps_graph_at_top,
            transparent_density_graph_bg: player_options.transparent_density_graph_bg,
            smx_fsr_display: player_options.smx_fsr_display,
            smx_pad_input_display: player_options.smx_pad_input_display,
            mini_indicator: player_options.mini_indicator,
            mini_indicator_score_type: player_options.mini_indicator_score_type,
            mini_indicator_subtractive_display: player_options.mini_indicator_subtractive_display,
            mini_indicator_size: player_options.mini_indicator_size,
            mini_indicator_color: player_options.mini_indicator_color,
            mini_indicator_position: player_options.mini_indicator_position,
            mini_percent: player_options.mini_percent,
            spacing_percent: player_options.spacing_percent,
            perspective: player_options.perspective,
            note_field_offset_x: player_options.note_field_offset_x,
            note_field_offset_y: player_options.note_field_offset_y,
            judgment_offset_x: player_options.judgment_offset_x,
            judgment_offset_y: player_options.judgment_offset_y,
            combo_offset_x: player_options.combo_offset_x,
            combo_offset_y: player_options.combo_offset_y,
            error_bar_offset_x: player_options.error_bar_offset_x,
            error_bar_offset_y: player_options.error_bar_offset_y,
            visual_delay_ms: player_options.visual_delay_ms,
            global_offset_shift_ms: player_options.global_offset_shift_ms,
            player_options_singles: player_options.clone(),
            player_options_doubles: player_options,
            last_played_singles: LastPlayed::default(),
            last_played_doubles: LastPlayed::default(),
            last_played_course_singles: LastPlayedCourse::default(),
            last_played_course_doubles: LastPlayedCourse::default(),
        }
    }
}

impl Profile {
    pub fn score_import_api_key(&self, endpoint: ScoreImportEndpoint) -> &str {
        match endpoint {
            ScoreImportEndpoint::GrooveStats | ScoreImportEndpoint::BoogieStats => {
                self.groovestats_api_key.trim()
            }
            ScoreImportEndpoint::ArrowCloud => self.arrowcloud_api_key.trim(),
        }
    }

    pub fn score_import_username(&self, endpoint: ScoreImportEndpoint) -> &str {
        if endpoint.requires_username() {
            self.groovestats_username.trim()
        } else {
            ""
        }
    }

    pub fn has_score_import_credentials(&self, endpoint: ScoreImportEndpoint) -> bool {
        !self.score_import_api_key(endpoint).is_empty()
            && (!endpoint.requires_username() || !self.score_import_username(endpoint).is_empty())
    }

    pub fn set_last_played(
        &mut self,
        style: PlayStyle,
        song_music_path: Option<String>,
        chart_hash: Option<String>,
        difficulty_index: usize,
    ) -> bool {
        let last_played = self.last_played_mut(style);
        if last_played.song_music_path == song_music_path
            && last_played.chart_hash == chart_hash
            && last_played.difficulty_index == difficulty_index
        {
            return false;
        }
        last_played.song_music_path = song_music_path;
        last_played.chart_hash = chart_hash;
        last_played.difficulty_index = difficulty_index;
        true
    }

    pub fn set_last_played_course(
        &mut self,
        style: PlayStyle,
        course_path: Option<String>,
        difficulty_name: Option<String>,
    ) -> bool {
        let last_played = self.last_played_course_mut(style);
        if last_played.course_path == course_path && last_played.difficulty_name == difficulty_name
        {
            return false;
        }
        last_played.course_path = course_path;
        last_played.difficulty_name = difficulty_name;
        true
    }

    pub fn add_stage_calories_for_day(&mut self, day: &str, calories_burned: f32) -> bool {
        let mut changed = false;
        if self.calories_burned_day.trim() != day {
            self.calories_burned_day = day.to_string();
            self.calories_burned_today = 0.0;
            changed = true;
        }

        if !self.ignore_step_count_calories && calories_burned.is_finite() && calories_burned >= 0.0
        {
            let calories = (self.calories_burned_today + calories_burned).max(0.0);
            changed |= set_f32_if_changed(&mut self.calories_burned_today, calories);
        }
        changed
    }

    pub fn set_player_initials(&mut self, initials: &str) -> bool {
        let initials = sanitize_player_initials(initials);
        if initials.is_empty() || self.player_initials == initials {
            return false;
        }
        self.player_initials = initials;
        true
    }

    pub fn set_scroll_option(&mut self, setting: ScrollOption) -> bool {
        let reverse_enabled = setting.contains(ScrollOption::Reverse);
        if self.scroll_option == setting && self.reverse_scroll == reverse_enabled {
            return false;
        }
        self.scroll_option = setting;
        self.reverse_scroll = reverse_enabled;
        true
    }

    pub fn set_gameplay_extras(
        &mut self,
        column_flash_on_miss: bool,
        subtractive_scoring: bool,
        pacemaker: bool,
        nps_graph_at_top: bool,
    ) -> bool {
        if self.column_flash_on_miss == column_flash_on_miss
            && self.subtractive_scoring == subtractive_scoring
            && self.pacemaker == pacemaker
            && self.nps_graph_at_top == nps_graph_at_top
        {
            return false;
        }
        self.column_flash_on_miss = column_flash_on_miss;
        self.subtractive_scoring = subtractive_scoring;
        self.pacemaker = pacemaker;
        self.nps_graph_at_top = nps_graph_at_top;
        if subtractive_scoring {
            self.mini_indicator = MiniIndicator::SubtractiveScoring;
        } else if pacemaker {
            self.mini_indicator = MiniIndicator::Pacemaker;
        } else if matches!(
            self.mini_indicator,
            MiniIndicator::SubtractiveScoring | MiniIndicator::Pacemaker
        ) {
            self.mini_indicator = MiniIndicator::None;
        }
        true
    }

    pub fn set_column_flash_mask(&mut self, mask: ColumnFlashMask) -> bool {
        if self.column_flash_mask == mask {
            return false;
        }
        self.column_flash_mask = mask;
        true
    }

    pub fn set_early_dw_options(
        &mut self,
        hide_judgments: bool,
        hide_flash: bool,
        hide_column_flash: bool,
    ) -> bool {
        if self.hide_early_dw_judgments == hide_judgments
            && self.hide_early_dw_flash == hide_flash
            && self.hide_early_dw_column_flash == hide_column_flash
        {
            return false;
        }
        self.hide_early_dw_judgments = hide_judgments;
        self.hide_early_dw_flash = hide_flash;
        self.hide_early_dw_column_flash = hide_column_flash;
        true
    }

    pub fn set_hide_options(
        &mut self,
        hide_targets: bool,
        hide_song_bg: bool,
        hide_combo: bool,
        hide_lifebar: bool,
        hide_score: bool,
        hide_danger: bool,
        hide_combo_explosions: bool,
        hide_username: bool,
    ) -> bool {
        if self.hide_targets == hide_targets
            && self.hide_song_bg == hide_song_bg
            && self.hide_combo == hide_combo
            && self.hide_lifebar == hide_lifebar
            && self.hide_score == hide_score
            && self.hide_danger == hide_danger
            && self.hide_combo_explosions == hide_combo_explosions
            && self.hide_username == hide_username
        {
            return false;
        }
        self.hide_targets = hide_targets;
        self.hide_song_bg = hide_song_bg;
        self.hide_combo = hide_combo;
        self.hide_lifebar = hide_lifebar;
        self.hide_score = hide_score;
        self.hide_danger = hide_danger;
        self.hide_combo_explosions = hide_combo_explosions;
        self.hide_username = hide_username;
        true
    }

    pub fn set_tilt_thresholds(&mut self, min_ms: u32, max_ms: u32) -> bool {
        let min_ms = clamp_tilt_threshold_ms(min_ms);
        let max_ms = clamp_tilt_threshold_ms(max_ms).max(min_ms);
        if self.tilt_min_threshold_ms == min_ms && self.tilt_max_threshold_ms == max_ms {
            return false;
        }
        self.tilt_min_threshold_ms = min_ms;
        self.tilt_max_threshold_ms = max_ms;
        true
    }

    pub fn set_error_bar_mask(&mut self, mask: ErrorBarMask) -> bool {
        if self.error_bar_active_mask == mask {
            return false;
        }
        self.error_bar_active_mask = mask;
        self.error_bar = error_bar_style_from_mask(mask);
        self.error_bar_text = error_bar_text_from_mask(mask);
        true
    }

    pub fn set_note_field_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.note_field_offset_x,
            offset.clamp(NOTE_FIELD_OFFSET_X_MIN, NOTE_FIELD_OFFSET_X_MAX),
        )
    }

    pub fn set_note_field_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.note_field_offset_y,
            offset.clamp(NOTE_FIELD_OFFSET_Y_MIN, NOTE_FIELD_OFFSET_Y_MAX),
        )
    }

    pub fn set_judgment_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.judgment_offset_x,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_judgment_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.judgment_offset_y,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_combo_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.combo_offset_x,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_combo_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.combo_offset_y,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_error_bar_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.error_bar_offset_x,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_error_bar_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.error_bar_offset_y,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_mini_percent(&mut self, percent: i32) -> bool {
        set_i32_if_changed(
            &mut self.mini_percent,
            percent.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX),
        )
    }

    pub fn set_spacing_percent(&mut self, percent: i32) -> bool {
        set_i32_if_changed(
            &mut self.spacing_percent,
            percent.clamp(SPACING_PERCENT_MIN, SPACING_PERCENT_MAX),
        )
    }

    pub fn set_visual_delay_ms(&mut self, ms: i32) -> bool {
        set_i32_if_changed(
            &mut self.visual_delay_ms,
            ms.clamp(VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX),
        )
    }

    pub fn set_global_offset_shift_ms(&mut self, ms: i32) -> bool {
        set_i32_if_changed(
            &mut self.global_offset_shift_ms,
            ms.clamp(VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX),
        )
    }

    pub fn set_tilt_multiplier(&mut self, multiplier: f32) -> bool {
        if !multiplier.is_finite() {
            return false;
        }
        set_f32_if_changed(&mut self.tilt_multiplier, multiplier)
    }

    pub fn set_custom_fantastic_window_ms(&mut self, ms: u8) -> bool {
        set_u8_if_changed(
            &mut self.custom_fantastic_window_ms,
            clamp_custom_fantastic_window_ms(ms),
        )
    }

    pub fn set_pad_light_brightness(&mut self, percent: u8) -> bool {
        set_u8_if_changed(
            &mut self.pad_light_brightness,
            clamp_pad_light_brightness(percent),
        )
    }

    pub fn set_average_error_bar_intensity(&mut self, intensity: f32) -> bool {
        set_f32_if_changed(
            &mut self.average_error_bar_intensity,
            clamp_average_error_bar_intensity(intensity),
        )
    }

    pub fn set_average_error_bar_interval_ms(&mut self, ms: u32) -> bool {
        set_u32_if_changed(
            &mut self.average_error_bar_interval_ms,
            clamp_average_error_bar_interval_ms(ms),
        )
    }

    pub fn set_text_error_bar_threshold_ms(&mut self, ms: u32) -> bool {
        set_u32_if_changed(
            &mut self.text_error_bar_threshold_ms,
            clamp_text_error_bar_threshold_ms(ms),
        )
    }

    pub fn set_long_error_bar_intensity(&mut self, intensity: f32) -> bool {
        set_f32_if_changed(
            &mut self.long_error_bar_intensity,
            clamp_long_error_bar_intensity(intensity),
        )
    }

    pub fn set_long_error_bar_threshold_ms(&mut self, ms: u32) -> bool {
        set_u32_if_changed(
            &mut self.long_error_bar_threshold_ms,
            clamp_long_error_bar_threshold_ms(ms),
        )
    }

    pub fn set_long_error_bar_min_samples(&mut self, n: u32) -> bool {
        set_u32_if_changed(
            &mut self.long_error_bar_min_samples,
            clamp_long_error_bar_min_samples(n),
        )
    }

    pub fn set_error_bar_options(&mut self, up: bool, multi_tick: bool) -> bool {
        if self.error_bar_up == up && self.error_bar_multi_tick == multi_tick {
            return false;
        }
        self.error_bar_up = up;
        self.error_bar_multi_tick = multi_tick;
        true
    }

    pub fn set_measure_counter_lookahead(&mut self, lookahead: u8) -> bool {
        set_u8_if_changed(&mut self.measure_counter_lookahead, lookahead.min(4))
    }

    pub fn set_measure_counter_options(
        &mut self,
        left: bool,
        up: bool,
        vert: bool,
        broken_run: bool,
        run_timer: bool,
    ) -> bool {
        if self.measure_counter_left == left
            && self.measure_counter_up == up
            && self.measure_counter_vert == vert
            && self.broken_run == broken_run
            && self.run_timer == run_timer
        {
            return false;
        }
        self.measure_counter_left = left;
        self.measure_counter_up = up;
        self.measure_counter_vert = vert;
        self.broken_run = broken_run;
        self.run_timer = run_timer;
        true
    }

    #[inline(always)]
    pub const fn calculated_weight_pounds(&self) -> i32 {
        resolved_weight_pounds(self.weight_pounds)
    }

    #[inline(always)]
    pub const fn age_years_for(&self, current_year: i32) -> i32 {
        age_years_for_birth_year(self.birth_year, current_year)
    }

    #[inline(always)]
    pub fn age_years(&self) -> i32 {
        self.age_years_for(Local::now().year())
    }

    #[inline(always)]
    pub fn resolved_mine_noteskin(&self) -> &NoteSkin {
        resolve_noteskin_choice(self.mine_noteskin.as_ref(), &self.noteskin)
    }

    #[inline(always)]
    pub fn resolved_receptor_noteskin(&self) -> &NoteSkin {
        resolve_noteskin_choice(self.receptor_noteskin.as_ref(), &self.noteskin)
    }

    #[inline(always)]
    pub fn tap_explosion_noteskin_hidden(&self) -> bool {
        tap_explosion_skin_hidden(self.tap_explosion_noteskin.as_ref())
    }

    #[inline(always)]
    pub fn resolved_tap_explosion_noteskin(&self) -> Option<&NoteSkin> {
        resolve_tap_explosion_skin(self.tap_explosion_noteskin.as_ref(), &self.noteskin)
    }

    #[inline(always)]
    pub fn tap_explosion_window_enabled(&self, window: &str) -> bool {
        tap_explosion_mask_enabled(self.tap_explosion_active_mask, window)
    }

    #[inline(always)]
    pub fn current_player_options(&self) -> PlayerOptionsData {
        PlayerOptionsData {
            background_filter: self.background_filter,
            hold_judgment_graphic: self.hold_judgment_graphic.clone(),
            held_miss_graphic: self.held_miss_graphic.clone(),
            judgment_graphic: self.judgment_graphic.clone(),
            combo_font: self.combo_font,
            combo_colors: self.combo_colors,
            combo_mode: self.combo_mode,
            carry_combo_between_songs: self.carry_combo_between_songs,
            noteskin: self.noteskin.clone(),
            mine_noteskin: self.mine_noteskin.clone(),
            receptor_noteskin: self.receptor_noteskin.clone(),
            tap_explosion_noteskin: self.tap_explosion_noteskin.clone(),
            tap_explosion_active_mask: self.tap_explosion_active_mask,
            scroll_speed: self.scroll_speed,
            no_cmod_alternative: self.no_cmod_alternative,
            scroll_option: self.scroll_option,
            reverse_scroll: self.reverse_scroll,
            turn_option: self.turn_option,
            insert_active_mask: self.insert_active_mask,
            remove_active_mask: self.remove_active_mask,
            holds_active_mask: self.holds_active_mask,
            accel_effects_active_mask: self.accel_effects_active_mask,
            visual_effects_active_mask: self.visual_effects_active_mask,
            appearance_effects_active_mask: self.appearance_effects_active_mask,
            attack_mode: self.attack_mode,
            hide_light_type: self.hide_light_type,
            rescore_early_hits: self.rescore_early_hits,
            hide_early_dw_judgments: self.hide_early_dw_judgments,
            hide_early_dw_flash: self.hide_early_dw_flash,
            hide_early_dw_column_flash: self.hide_early_dw_column_flash,
            timing_windows: self.timing_windows,
            show_fa_plus_window: self.show_fa_plus_window,
            show_ex_score: self.show_ex_score,
            show_hard_ex_score: self.show_hard_ex_score,
            show_fa_plus_pane: self.show_fa_plus_pane,
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
            split_15_10ms: self.split_15_10ms,
            track_early_judgments: self.track_early_judgments,
            scale_scatterplot: self.scale_scatterplot,
            scatterplot_max_window: self.scatterplot_max_window,
            score_position: self.score_position,
            score_display_mode: self.score_display_mode,
            custom_fantastic_window: self.custom_fantastic_window,
            custom_fantastic_window_ms: self.custom_fantastic_window_ms,
            pad_light_brightness: self.pad_light_brightness,
            judgment_tilt: self.judgment_tilt,
            column_cues: self.column_cues,
            measure_cues: self.measure_cues,
            judgment_back: self.judgment_back,
            error_ms_display: self.error_ms_display,
            display_scorebox: self.display_scorebox,
            live_timing_stats: self.live_timing_stats,
            live_timing_stats_mask: self.live_timing_stats_mask,
            rainbow_max: self.rainbow_max,
            responsive_colors: self.responsive_colors,
            show_life_percent: self.show_life_percent,
            tilt_multiplier: self.tilt_multiplier,
            tilt_min_threshold_ms: self.tilt_min_threshold_ms,
            tilt_max_threshold_ms: self.tilt_max_threshold_ms,
            error_bar_active_mask: self.error_bar_active_mask,
            error_bar: self.error_bar,
            error_bar_text: self.error_bar_text,
            text_error_bar_scalable: self.text_error_bar_scalable,
            text_error_bar_threshold_ms: self.text_error_bar_threshold_ms,
            error_bar_up: self.error_bar_up,
            error_bar_multi_tick: self.error_bar_multi_tick,
            error_bar_trim: self.error_bar_trim,
            center_tick: self.center_tick,
            short_average_error_bar_enabled: self.short_average_error_bar_enabled,
            average_error_bar_intensity: self.average_error_bar_intensity,
            average_error_bar_interval_ms: self.average_error_bar_interval_ms,
            long_error_bar_enabled: self.long_error_bar_enabled,
            long_error_bar_intensity: self.long_error_bar_intensity,
            long_error_bar_threshold_ms: self.long_error_bar_threshold_ms,
            long_error_bar_min_samples: self.long_error_bar_min_samples,
            step_statistics: self.step_statistics,
            step_stats_extra: self.step_stats_extra,
            target_score: self.target_score,
            lifemeter_type: self.lifemeter_type,
            measure_counter: self.measure_counter,
            measure_counter_lookahead: self.measure_counter_lookahead,
            measure_counter_left: self.measure_counter_left,
            measure_counter_up: self.measure_counter_up,
            measure_counter_vert: self.measure_counter_vert,
            broken_run: self.broken_run,
            run_timer: self.run_timer,
            measure_lines: self.measure_lines,
            hide_targets: self.hide_targets,
            hide_song_bg: self.hide_song_bg,
            hide_combo: self.hide_combo,
            hide_lifebar: self.hide_lifebar,
            hide_score: self.hide_score,
            hide_danger: self.hide_danger,
            hide_combo_explosions: self.hide_combo_explosions,
            hide_username: self.hide_username,
            column_flash_on_miss: self.column_flash_on_miss,
            column_flash_mask: self.column_flash_mask,
            column_flash_brightness: self.column_flash_brightness,
            column_flash_size: self.column_flash_size,
            subtractive_scoring: self.subtractive_scoring,
            pacemaker: self.pacemaker,
            nps_graph_at_top: self.nps_graph_at_top,
            transparent_density_graph_bg: self.transparent_density_graph_bg,
            smx_fsr_display: self.smx_fsr_display,
            smx_pad_input_display: self.smx_pad_input_display,
            mini_indicator: self.mini_indicator,
            mini_indicator_score_type: self.mini_indicator_score_type,
            mini_indicator_subtractive_display: self.mini_indicator_subtractive_display,
            mini_indicator_size: self.mini_indicator_size,
            mini_indicator_color: self.mini_indicator_color,
            mini_indicator_position: self.mini_indicator_position,
            mini_percent: self.mini_percent,
            spacing_percent: self.spacing_percent,
            perspective: self.perspective,
            note_field_offset_x: self.note_field_offset_x,
            note_field_offset_y: self.note_field_offset_y,
            judgment_offset_x: self.judgment_offset_x,
            judgment_offset_y: self.judgment_offset_y,
            combo_offset_x: self.combo_offset_x,
            combo_offset_y: self.combo_offset_y,
            error_bar_offset_x: self.error_bar_offset_x,
            error_bar_offset_y: self.error_bar_offset_y,
            visual_delay_ms: self.visual_delay_ms,
            global_offset_shift_ms: self.global_offset_shift_ms,
        }
    }

    fn apply_player_options(&mut self, options: &PlayerOptionsData) {
        self.background_filter = options.background_filter;
        self.hold_judgment_graphic = options.hold_judgment_graphic.clone();
        self.held_miss_graphic = options.held_miss_graphic.clone();
        self.judgment_graphic = options.judgment_graphic.clone();
        self.combo_font = options.combo_font;
        self.combo_colors = options.combo_colors;
        self.combo_mode = options.combo_mode;
        self.carry_combo_between_songs = options.carry_combo_between_songs;
        self.noteskin = options.noteskin.clone();
        self.mine_noteskin.clone_from(&options.mine_noteskin);
        self.receptor_noteskin
            .clone_from(&options.receptor_noteskin);
        self.tap_explosion_noteskin
            .clone_from(&options.tap_explosion_noteskin);
        self.tap_explosion_active_mask = options.tap_explosion_active_mask;
        self.scroll_speed = options.scroll_speed;
        self.no_cmod_alternative = options.no_cmod_alternative;
        self.scroll_option = options.scroll_option;
        self.reverse_scroll = options.reverse_scroll;
        self.turn_option = options.turn_option;
        self.insert_active_mask = options.insert_active_mask;
        self.remove_active_mask = options.remove_active_mask;
        self.holds_active_mask = options.holds_active_mask;
        self.accel_effects_active_mask = options.accel_effects_active_mask;
        self.visual_effects_active_mask = options.visual_effects_active_mask;
        self.appearance_effects_active_mask = options.appearance_effects_active_mask;
        self.attack_mode = options.attack_mode;
        self.hide_light_type = options.hide_light_type;
        self.rescore_early_hits = options.rescore_early_hits;
        self.hide_early_dw_judgments = options.hide_early_dw_judgments;
        self.hide_early_dw_flash = options.hide_early_dw_flash;
        self.hide_early_dw_column_flash = options.hide_early_dw_column_flash;
        self.timing_windows = options.timing_windows;
        self.show_fa_plus_window = options.show_fa_plus_window;
        self.show_ex_score = options.show_ex_score;
        self.show_hard_ex_score = options.show_hard_ex_score;
        self.show_fa_plus_pane = options.show_fa_plus_pane;
        self.fa_plus_10ms_blue_window = options.fa_plus_10ms_blue_window;
        self.split_15_10ms = options.split_15_10ms;
        self.track_early_judgments = options.track_early_judgments;
        self.scale_scatterplot = options.scale_scatterplot;
        self.scatterplot_max_window = options.scatterplot_max_window;
        self.score_position = options.score_position;
        self.score_display_mode = options.score_display_mode;
        self.custom_fantastic_window = options.custom_fantastic_window;
        self.custom_fantastic_window_ms = options.custom_fantastic_window_ms;
        self.pad_light_brightness = options.pad_light_brightness;
        self.judgment_tilt = options.judgment_tilt;
        self.column_cues = options.column_cues;
        self.measure_cues = options.measure_cues;
        self.judgment_back = options.judgment_back;
        self.error_ms_display = options.error_ms_display;
        self.display_scorebox = options.display_scorebox;
        self.live_timing_stats = options.live_timing_stats;
        self.live_timing_stats_mask = options.live_timing_stats_mask;
        self.rainbow_max = options.rainbow_max;
        self.responsive_colors = options.responsive_colors;
        self.show_life_percent = options.show_life_percent;
        self.tilt_multiplier = options.tilt_multiplier;
        self.tilt_min_threshold_ms = options.tilt_min_threshold_ms;
        self.tilt_max_threshold_ms = options.tilt_max_threshold_ms;
        self.error_bar_active_mask = options.error_bar_active_mask;
        self.error_bar = options.error_bar;
        self.error_bar_text = options.error_bar_text;
        self.text_error_bar_scalable = options.text_error_bar_scalable;
        self.text_error_bar_threshold_ms = options.text_error_bar_threshold_ms;
        self.error_bar_up = options.error_bar_up;
        self.error_bar_multi_tick = options.error_bar_multi_tick;
        self.error_bar_trim = options.error_bar_trim;
        self.center_tick = options.center_tick;
        self.short_average_error_bar_enabled = options.short_average_error_bar_enabled;
        self.average_error_bar_intensity = options.average_error_bar_intensity;
        self.average_error_bar_interval_ms = options.average_error_bar_interval_ms;
        self.long_error_bar_enabled = options.long_error_bar_enabled;
        self.long_error_bar_intensity = options.long_error_bar_intensity;
        self.long_error_bar_threshold_ms = options.long_error_bar_threshold_ms;
        self.long_error_bar_min_samples = options.long_error_bar_min_samples;
        self.step_statistics = options.step_statistics;
        self.step_stats_extra = options.step_stats_extra;
        self.target_score = options.target_score;
        self.lifemeter_type = options.lifemeter_type;
        self.measure_counter = options.measure_counter;
        self.measure_counter_lookahead = options.measure_counter_lookahead;
        self.measure_counter_left = options.measure_counter_left;
        self.measure_counter_up = options.measure_counter_up;
        self.measure_counter_vert = options.measure_counter_vert;
        self.broken_run = options.broken_run;
        self.run_timer = options.run_timer;
        self.measure_lines = options.measure_lines;
        self.hide_targets = options.hide_targets;
        self.hide_song_bg = options.hide_song_bg;
        self.hide_combo = options.hide_combo;
        self.hide_lifebar = options.hide_lifebar;
        self.hide_score = options.hide_score;
        self.hide_danger = options.hide_danger;
        self.hide_combo_explosions = options.hide_combo_explosions;
        self.hide_username = options.hide_username;
        self.column_flash_on_miss = options.column_flash_on_miss;
        self.column_flash_mask = options.column_flash_mask;
        self.column_flash_brightness = options.column_flash_brightness;
        self.column_flash_size = options.column_flash_size;
        self.subtractive_scoring = options.subtractive_scoring;
        self.pacemaker = options.pacemaker;
        self.nps_graph_at_top = options.nps_graph_at_top;
        self.transparent_density_graph_bg = options.transparent_density_graph_bg;
        self.smx_fsr_display = options.smx_fsr_display;
        self.smx_pad_input_display = options.smx_pad_input_display;
        self.mini_indicator = options.mini_indicator;
        self.mini_indicator_score_type = options.mini_indicator_score_type;
        self.mini_indicator_subtractive_display = options.mini_indicator_subtractive_display;
        self.mini_indicator_size = options.mini_indicator_size;
        self.mini_indicator_color = options.mini_indicator_color;
        self.mini_indicator_position = options.mini_indicator_position;
        self.mini_percent = options.mini_percent;
        self.spacing_percent = options.spacing_percent;
        self.perspective = options.perspective;
        self.note_field_offset_x = options.note_field_offset_x;
        self.note_field_offset_y = options.note_field_offset_y;
        self.judgment_offset_x = options.judgment_offset_x;
        self.judgment_offset_y = options.judgment_offset_y;
        self.combo_offset_x = options.combo_offset_x;
        self.combo_offset_y = options.combo_offset_y;
        self.error_bar_offset_x = options.error_bar_offset_x;
        self.error_bar_offset_y = options.error_bar_offset_y;
        self.visual_delay_ms = options.visual_delay_ms;
        self.global_offset_shift_ms = options.global_offset_shift_ms;
    }

    #[inline(always)]
    pub const fn player_options(&self, style: PlayStyle) -> &PlayerOptionsData {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &self.player_options_singles,
            PlayStyle::Double => &self.player_options_doubles,
        }
    }

    #[inline(always)]
    pub fn player_options_mut(&mut self, style: PlayStyle) -> &mut PlayerOptionsData {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &mut self.player_options_singles,
            PlayStyle::Double => &mut self.player_options_doubles,
        }
    }

    pub fn store_current_player_options(&mut self, style: PlayStyle) {
        let options = self.current_player_options();
        *self.player_options_mut(style) = options;
    }

    pub fn store_current_player_options_for_all_styles(&mut self) {
        let options = self.current_player_options();
        self.player_options_singles = options.clone();
        self.player_options_doubles = options;
    }

    pub fn apply_player_options_for_style(&mut self, style: PlayStyle) {
        let options = self.player_options(style).clone();
        self.apply_player_options(&options);
    }

    #[inline(always)]
    pub const fn last_played(&self, style: PlayStyle) -> &LastPlayed {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &self.last_played_singles,
            PlayStyle::Double => &self.last_played_doubles,
        }
    }

    #[inline(always)]
    pub fn last_played_mut(&mut self, style: PlayStyle) -> &mut LastPlayed {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &mut self.last_played_singles,
            PlayStyle::Double => &mut self.last_played_doubles,
        }
    }

    #[inline(always)]
    pub const fn last_played_course(&self, style: PlayStyle) -> &LastPlayedCourse {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &self.last_played_course_singles,
            PlayStyle::Double => &self.last_played_course_doubles,
        }
    }

    #[inline(always)]
    pub fn last_played_course_mut(&mut self, style: PlayStyle) -> &mut LastPlayedCourse {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &mut self.last_played_course_singles,
            PlayStyle::Double => &mut self.last_played_course_doubles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_style_reports_chart_type() {
        assert_eq!(PlayStyle::Single.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Versus.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Double.chart_type(), "dance-double");
        assert_eq!(PlayStyle::Single.cols_per_player(), 4);
        assert_eq!(PlayStyle::Versus.cols_per_player(), 4);
        assert_eq!(PlayStyle::Double.cols_per_player(), 8);
        assert_eq!(PlayStyle::Single.player_count(), 1);
        assert_eq!(PlayStyle::Versus.player_count(), 2);
        assert_eq!(PlayStyle::Double.player_count(), 1);
        assert_eq!(PlayStyle::Single.total_cols(), 4);
        assert_eq!(PlayStyle::Versus.total_cols(), 8);
        assert_eq!(PlayStyle::Double.total_cols(), 8);
    }

    #[test]
    fn defaults_match_single_player_session() {
        assert_eq!(PLAYER_SLOTS, 2);
        assert_eq!(DEFAULT_WEIGHT_POUNDS, 120);
        assert_eq!(DEFAULT_BIRTH_YEAR, 1995);
        assert_eq!(PLAYER_INITIALS_MAX_LEN, 4);
        assert_eq!((HUD_OFFSET_MIN, HUD_OFFSET_MAX), (-250, 250));
        assert_eq!((SPACING_PERCENT_MIN, SPACING_PERCENT_MAX), (-100, 100));
        assert_eq!((MINI_PERCENT_MIN, MINI_PERCENT_MAX), (-100, 150));
        assert_eq!((NOTE_FIELD_OFFSET_X_MIN, NOTE_FIELD_OFFSET_X_MAX), (0, 50));
        assert_eq!(
            (NOTE_FIELD_OFFSET_Y_MIN, NOTE_FIELD_OFFSET_Y_MAX),
            (-50, 50)
        );
        assert_eq!((VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX), (-100, 100));
        assert_eq!((TILT_THRESHOLD_MIN_MS, TILT_THRESHOLD_MAX_MS), (0, 100));
        assert_eq!(
            (TILT_MIN_THRESHOLD_DEFAULT_MS, TILT_MAX_THRESHOLD_DEFAULT_MS),
            (0, 50)
        );
        assert_eq!(
            (
                CUSTOM_FANTASTIC_WINDOW_MIN_MS,
                CUSTOM_FANTASTIC_WINDOW_MAX_MS,
                CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS
            ),
            (1, 22, 10)
        );
        assert_eq!(
            (
                TEXT_ERROR_BAR_THRESHOLD_MS_MIN,
                TEXT_ERROR_BAR_THRESHOLD_MS_MAX,
                TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT
            ),
            (1, 50, 10)
        );
        assert_eq!(PlayStyle::default(), PlayStyle::Single);
        assert_eq!(PlayMode::default(), PlayMode::Regular);
        assert_eq!(PlayerSide::default(), PlayerSide::P1);
        assert_eq!(TimingTickMode::default(), TimingTickMode::Off);

        let options = PlayerOptionsData::default();
        assert_eq!(options.scroll_speed, ScrollSpeedSetting::default());
        assert!(options.carry_combo_between_songs);
        assert!(options.rescore_early_hits);
        assert!(options.display_scorebox);
        assert!(options.short_average_error_bar_enabled);
        assert!(options.long_error_bar_enabled);
        assert!(!options.text_error_bar_scalable);
        assert_eq!(
            options.text_error_bar_threshold_ms,
            TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT
        );
        assert!(options.step_statistics.is_empty());
        assert_eq!(options.step_stats_extra, StepStatsExtra::None);
        assert_eq!(options.score_position, ScorePosition::Normal);
        assert_eq!(options.score_display_mode, ScoreDisplayMode::Normal);
        assert_eq!(options.measure_counter_lookahead, 2);
        assert!(options.measure_counter_left);
        assert_eq!(options.tap_explosion_active_mask, TapExplosionMask::all());
        assert_eq!(options.column_flash_mask, DEFAULT_COLUMN_FLASH_MASK);
        assert_eq!(
            options.column_flash_brightness,
            ColumnFlashBrightness::Normal
        );
        assert_eq!(options.column_flash_size, ColumnFlashSize::Default);
    }

    #[test]
    fn score_import_credentials_select_endpoint_fields() {
        let mut profile = Profile::default();
        profile.groovestats_api_key = " gs-key ".to_string();
        profile.groovestats_username = " player ".to_string();
        profile.arrowcloud_api_key = " ac-key ".to_string();

        assert_eq!(
            profile.score_import_api_key(ScoreImportEndpoint::GrooveStats),
            "gs-key"
        );
        assert_eq!(
            profile.score_import_api_key(ScoreImportEndpoint::BoogieStats),
            "gs-key"
        );
        assert_eq!(
            profile.score_import_api_key(ScoreImportEndpoint::ArrowCloud),
            "ac-key"
        );
        assert_eq!(
            profile.score_import_username(ScoreImportEndpoint::GrooveStats),
            "player"
        );
        assert_eq!(
            profile.score_import_username(ScoreImportEndpoint::ArrowCloud),
            ""
        );
        assert!(profile.has_score_import_credentials(ScoreImportEndpoint::GrooveStats));
        assert!(profile.has_score_import_credentials(ScoreImportEndpoint::ArrowCloud));

        profile.groovestats_username.clear();
        assert!(!profile.has_score_import_credentials(ScoreImportEndpoint::GrooveStats));
        assert!(profile.has_score_import_credentials(ScoreImportEndpoint::ArrowCloud));
    }

    #[test]
    fn profile_last_played_updates_style_entry() {
        let mut profile = Profile::default();

        assert!(profile.set_last_played(
            PlayStyle::Single,
            Some("Songs/Pack/Song.ogg".to_string()),
            Some("hash-a".to_string()),
            4,
        ));
        assert_eq!(
            profile
                .last_played(PlayStyle::Versus)
                .song_music_path
                .as_deref(),
            Some("Songs/Pack/Song.ogg")
        );
        assert_eq!(
            profile.last_played(PlayStyle::Single).chart_hash.as_deref(),
            Some("hash-a")
        );
        assert_eq!(profile.last_played(PlayStyle::Single).difficulty_index, 4);
        assert!(!profile.set_last_played(
            PlayStyle::Versus,
            Some("Songs/Pack/Song.ogg".to_string()),
            Some("hash-a".to_string()),
            4,
        ));

        assert!(profile.set_last_played(
            PlayStyle::Double,
            Some("Songs/Pack/Double.ogg".to_string()),
            None,
            1,
        ));
        assert_eq!(
            profile
                .last_played(PlayStyle::Double)
                .song_music_path
                .as_deref(),
            Some("Songs/Pack/Double.ogg")
        );
        assert_eq!(
            profile
                .last_played(PlayStyle::Single)
                .song_music_path
                .as_deref(),
            Some("Songs/Pack/Song.ogg")
        );
    }

    #[test]
    fn profile_last_played_course_updates_style_entry() {
        let mut profile = Profile::default();

        assert!(profile.set_last_played_course(
            PlayStyle::Single,
            Some("Courses/Course.crs".to_string()),
            Some("Hard".to_string()),
        ));
        assert_eq!(
            profile
                .last_played_course(PlayStyle::Versus)
                .course_path
                .as_deref(),
            Some("Courses/Course.crs")
        );
        assert_eq!(
            profile
                .last_played_course(PlayStyle::Single)
                .difficulty_name
                .as_deref(),
            Some("Hard")
        );
        assert!(!profile.set_last_played_course(
            PlayStyle::Versus,
            Some("Courses/Course.crs".to_string()),
            Some("Hard".to_string()),
        ));

        assert!(profile.set_last_played_course(
            PlayStyle::Double,
            Some("Courses/Double.crs".to_string()),
            Some("Challenge".to_string()),
        ));
        assert!(profile.set_last_played_course(PlayStyle::Double, None, None));
        assert_eq!(
            profile.last_played_course(PlayStyle::Double).course_path,
            None
        );
    }

    #[test]
    fn profile_stage_calories_reset_day_and_ignore_invalid() {
        let mut profile = Profile {
            calories_burned_day: "2026-06-01".to_string(),
            calories_burned_today: 12.0,
            ..Profile::default()
        };

        assert!(profile.add_stage_calories_for_day("2026-06-02", 3.5));
        assert_eq!(profile.calories_burned_day, "2026-06-02");
        assert!((profile.calories_burned_today - 3.5).abs() < 1e-6);

        assert!(!profile.add_stage_calories_for_day("2026-06-02", f32::NAN));
        assert!((profile.calories_burned_today - 3.5).abs() < 1e-6);

        profile.ignore_step_count_calories = true;
        assert!(!profile.add_stage_calories_for_day("2026-06-02", 10.0));
        assert!((profile.calories_burned_today - 3.5).abs() < 1e-6);
    }

    #[test]
    fn profile_player_initials_sanitize_and_skip_empty() {
        let mut profile = Profile::default();

        assert!(profile.set_player_initials("a b-c"));
        assert_eq!(profile.player_initials, "ABC");
        assert!(!profile.set_player_initials("abc"));
        assert!(!profile.set_player_initials("    "));
        assert_eq!(profile.player_initials, "ABC");
    }

    #[test]
    fn profile_scroll_option_updates_reverse_flag() {
        let mut profile = Profile::default();

        assert!(profile.set_scroll_option(ScrollOption::Reverse));
        assert_eq!(profile.scroll_option, ScrollOption::Reverse);
        assert!(profile.reverse_scroll);
        assert!(!profile.set_scroll_option(ScrollOption::Reverse));

        assert!(profile.set_scroll_option(ScrollOption::Normal));
        assert_eq!(profile.scroll_option, ScrollOption::Normal);
        assert!(!profile.reverse_scroll);
    }

    #[test]
    fn profile_gameplay_extras_sync_mini_indicator() {
        let mut profile = Profile::default();

        assert!(profile.set_gameplay_extras(false, true, false, false));
        assert!(profile.subtractive_scoring);
        assert_eq!(profile.mini_indicator, MiniIndicator::SubtractiveScoring);

        assert!(profile.set_gameplay_extras(false, false, true, false));
        assert!(!profile.subtractive_scoring);
        assert!(profile.pacemaker);
        assert_eq!(profile.mini_indicator, MiniIndicator::Pacemaker);

        assert!(profile.set_gameplay_extras(false, false, false, false));
        assert_eq!(profile.mini_indicator, MiniIndicator::None);
        assert!(!profile.set_gameplay_extras(false, false, false, false));
    }

    #[test]
    fn profile_grouped_visibility_options_update_together() {
        let mut profile = Profile::default();

        assert!(profile.set_early_dw_options(true, true, true));
        assert!(profile.hide_early_dw_judgments);
        assert!(profile.hide_early_dw_flash);
        assert!(profile.hide_early_dw_column_flash);
        assert!(!profile.set_early_dw_options(true, true, true));

        assert!(profile.set_hide_options(true, true, false, true, false, true, true, true));
        assert!(profile.hide_targets);
        assert!(profile.hide_song_bg);
        assert!(!profile.hide_combo);
        assert!(profile.hide_lifebar);
        assert!(!profile.hide_score);
        assert!(profile.hide_danger);
        assert!(profile.hide_combo_explosions);
        assert!(profile.hide_username);
        assert!(!profile.set_hide_options(true, true, false, true, false, true, true, true));
    }

    #[test]
    fn profile_tilt_thresholds_clamp_and_order() {
        let mut profile = Profile::default();

        assert!(profile.set_tilt_thresholds(120, 10));
        assert_eq!(profile.tilt_min_threshold_ms, TILT_THRESHOLD_MAX_MS);
        assert_eq!(profile.tilt_max_threshold_ms, TILT_THRESHOLD_MAX_MS);
        assert!(!profile.set_tilt_thresholds(120, 10));
    }

    #[test]
    fn profile_error_bar_mask_syncs_legacy_fields() {
        let mut profile = Profile::default();
        let mask = ErrorBarMask::MONOCHROME | ErrorBarMask::TEXT;

        assert!(profile.set_error_bar_mask(mask));
        assert_eq!(profile.error_bar_active_mask, mask);
        assert_eq!(profile.error_bar, ErrorBarStyle::Monochrome);
        assert!(profile.error_bar_text);
        assert!(!profile.set_error_bar_mask(mask));
    }

    #[test]
    fn profile_position_offsets_clamp_ranges() {
        let mut profile = Profile::default();

        assert!(profile.set_note_field_offset_x(NOTE_FIELD_OFFSET_X_MAX + 1));
        assert_eq!(profile.note_field_offset_x, NOTE_FIELD_OFFSET_X_MAX);
        assert!(!profile.set_note_field_offset_x(NOTE_FIELD_OFFSET_X_MAX + 1));

        assert!(profile.set_note_field_offset_y(NOTE_FIELD_OFFSET_Y_MIN - 1));
        assert_eq!(profile.note_field_offset_y, NOTE_FIELD_OFFSET_Y_MIN);

        assert!(profile.set_judgment_offset_x(HUD_OFFSET_MAX + 1));
        assert_eq!(profile.judgment_offset_x, HUD_OFFSET_MAX);
        assert!(profile.set_judgment_offset_y(HUD_OFFSET_MIN - 1));
        assert_eq!(profile.judgment_offset_y, HUD_OFFSET_MIN);

        assert!(profile.set_combo_offset_x(HUD_OFFSET_MAX + 1));
        assert_eq!(profile.combo_offset_x, HUD_OFFSET_MAX);
        assert!(profile.set_combo_offset_y(HUD_OFFSET_MIN - 1));
        assert_eq!(profile.combo_offset_y, HUD_OFFSET_MIN);

        assert!(profile.set_error_bar_offset_x(HUD_OFFSET_MAX + 1));
        assert_eq!(profile.error_bar_offset_x, HUD_OFFSET_MAX);
        assert!(profile.set_error_bar_offset_y(HUD_OFFSET_MIN - 1));
        assert_eq!(profile.error_bar_offset_y, HUD_OFFSET_MIN);
    }

    #[test]
    fn profile_percent_and_timing_offsets_clamp_ranges() {
        let mut profile = Profile::default();

        assert!(profile.set_mini_percent(MINI_PERCENT_MAX + 1));
        assert_eq!(profile.mini_percent, MINI_PERCENT_MAX);
        assert!(!profile.set_mini_percent(MINI_PERCENT_MAX + 1));

        assert!(profile.set_spacing_percent(SPACING_PERCENT_MIN - 1));
        assert_eq!(profile.spacing_percent, SPACING_PERCENT_MIN);

        assert!(profile.set_visual_delay_ms(VISUAL_DELAY_MS_MAX + 1));
        assert_eq!(profile.visual_delay_ms, VISUAL_DELAY_MS_MAX);

        assert!(profile.set_global_offset_shift_ms(VISUAL_DELAY_MS_MIN - 1));
        assert_eq!(profile.global_offset_shift_ms, VISUAL_DELAY_MS_MIN);
    }

    #[test]
    fn profile_tilt_multiplier_rejects_non_finite() {
        let mut profile = Profile::default();

        assert!(!profile.set_tilt_multiplier(f32::NAN));
        assert_eq!(profile.tilt_multiplier, 1.0);
        assert!(!profile.set_tilt_multiplier(f32::INFINITY));
        assert_eq!(profile.tilt_multiplier, 1.0);

        assert!(profile.set_tilt_multiplier(1.25));
        assert_eq!(profile.tilt_multiplier, 1.25);
        assert!(!profile.set_tilt_multiplier(1.25));
    }

    #[test]
    fn profile_error_bar_numeric_settings_normalize() {
        let mut profile = Profile::default();

        assert!(profile.set_custom_fantastic_window_ms(CUSTOM_FANTASTIC_WINDOW_MAX_MS + 1));
        assert_eq!(
            profile.custom_fantastic_window_ms,
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert!(!profile.set_custom_fantastic_window_ms(CUSTOM_FANTASTIC_WINDOW_MAX_MS + 1));

        assert!(profile.set_average_error_bar_intensity(1.13));
        assert!((profile.average_error_bar_intensity - 1.25).abs() < 1e-6);
        assert!(!profile.set_average_error_bar_intensity(1.13));

        assert!(profile.set_average_error_bar_interval_ms(149));
        assert_eq!(profile.average_error_bar_interval_ms, 100);

        assert!(profile.set_text_error_bar_threshold_ms(999));
        assert_eq!(
            profile.text_error_bar_threshold_ms,
            TEXT_ERROR_BAR_THRESHOLD_MS_MAX
        );

        assert!(profile.set_long_error_bar_intensity(1.13));
        assert!((profile.long_error_bar_intensity - 1.25).abs() < 1e-6);

        assert!(profile.set_long_error_bar_threshold_ms(LONG_ERROR_BAR_THRESHOLD_MS_MAX + 1));
        assert_eq!(
            profile.long_error_bar_threshold_ms,
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );

        assert!(profile.set_long_error_bar_min_samples(0));
        assert_eq!(
            profile.long_error_bar_min_samples,
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
    }

    #[test]
    fn error_bar_options_load_legacy_flags_and_numeric_aliases() {
        let mut options = PlayerOptionsData::default();
        let values = [
            ("Colorful", "1"),
            ("Text", "1"),
            ("CenterTick", "1"),
            ("LongAvgTickOnly", "1"),
            ("HighlightZoom", "1.13x"),
            ("HighlightAverageMs", "149ms"),
            ("TextErrorBar10ms", "1"),
            ("TextErrorBarThresholdMs", "999ms"),
            ("LongErrorBar", "0"),
            ("LongErrorBarIntensity", "1.95x"),
            ("LongErrorBarThresholdMs", "9999ms"),
            ("LongErrorBarMinSamples", "0"),
        ];

        load_error_bar_options(&mut options, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        });

        assert!(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::COLORFUL)
        );
        assert!(options.error_bar_active_mask.contains(ErrorBarMask::TEXT));
        assert_eq!(options.error_bar, ErrorBarStyle::Colorful);
        assert!(options.error_bar_text);
        assert!(options.center_tick);
        assert!(!options.short_average_error_bar_enabled);
        assert!((options.average_error_bar_intensity - 1.25).abs() < 1e-6);
        assert_eq!(options.average_error_bar_interval_ms, 100);
        assert!(options.text_error_bar_scalable);
        assert_eq!(
            options.text_error_bar_threshold_ms,
            TEXT_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert!(!options.long_error_bar_enabled);
        assert!((options.long_error_bar_intensity - 2.0).abs() < 1e-6);
        assert_eq!(
            options.long_error_bar_threshold_ms,
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert_eq!(
            options.long_error_bar_min_samples,
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
    }

    #[test]
    fn visual_player_options_load_graphics_noteskins_and_offsets() {
        let mut options = PlayerOptionsData::default();
        let values = [
            ("BackgroundFilter", "50"),
            ("HoldJudgmentGraphic", "itg2"),
            ("HeldGraphic", "none"),
            ("JudgmentGraphic", "custom.png"),
            ("ComboFont", "BebasNeue"),
            ("ComboColors", "RainbowScroll"),
            ("ComboMode", "CurrentCombo"),
            ("ComboContinuesBetweenSongs", "1"),
            ("NoteSkin", "default"),
            ("MineSkin", "metal"),
            ("ReceptorSkin", "cyber"),
            ("TapExplosionSkin", "none"),
            ("TapExplosionMask", "63"),
            ("TapExplosionMaskVersion", "1"),
            ("MiniPercent", "42"),
            ("Spacing", "-7"),
            ("Perspective", "Incoming"),
            ("NoteFieldOffsetX", "12"),
            ("NoteFieldOffsetY", "-13"),
            ("JudgmentOffsetX", "14"),
            ("JudgmentOffsetY", "-15"),
            ("ComboOffsetX", "16"),
            ("ComboOffsetY", "-17"),
            ("ErrorBarOffsetX", "18"),
            ("ErrorBarOffsetY", "-19"),
            ("VisualDelay", "21ms"),
            ("GlobalOffsetShiftMs", "-22ms"),
        ];

        load_visual_player_options(&mut options, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        });

        assert_eq!(
            options.background_filter,
            BackgroundFilter::from_percent(50)
        );
        assert_eq!(
            options.hold_judgment_graphic.as_str(),
            "hold_judgements/ITG2 1x2 (doubleres).png"
        );
        assert_eq!(options.held_miss_graphic.as_str(), "None");
        assert_eq!(options.judgment_graphic.as_str(), "judgements/custom.png");
        assert_eq!(options.combo_font, ComboFont::BebasNeue);
        assert_eq!(options.combo_colors, ComboColors::RainbowScroll);
        assert_eq!(options.combo_mode, ComboMode::CurrentCombo);
        assert!(options.carry_combo_between_songs);
        assert_eq!(options.noteskin, NoteSkin::new("default"));
        assert_eq!(options.mine_noteskin, Some(NoteSkin::new("metal")));
        assert_eq!(options.receptor_noteskin, Some(NoteSkin::new("cyber")));
        assert_eq!(options.tap_explosion_noteskin, Some(NoteSkin::new("none")));
        assert_eq!(options.tap_explosion_active_mask, TapExplosionMask::all());
        assert_eq!(options.mini_percent, 42);
        assert_eq!(options.spacing_percent, -7);
        assert_eq!(options.perspective, Perspective::Incoming);
        assert_eq!(options.note_field_offset_x, 12);
        assert_eq!(options.note_field_offset_y, -13);
        assert_eq!(options.judgment_offset_x, 14);
        assert_eq!(options.judgment_offset_y, -15);
        assert_eq!(options.combo_offset_x, 16);
        assert_eq!(options.combo_offset_y, -17);
        assert_eq!(options.error_bar_offset_x, 18);
        assert_eq!(options.error_bar_offset_y, -19);
        assert_eq!(options.visual_delay_ms, 21);
        assert_eq!(options.global_offset_shift_ms, -22);
    }

    #[test]
    fn timing_feedback_options_load_legacy_aliases_and_clamps() {
        let mut options = PlayerOptionsData::default();
        let values = [
            ("ShowFaPlusWindow", "1"),
            ("ShowExScore", "1"),
            ("ShowHardEXScore", "1"),
            ("ShowFaPlusPane", "1"),
            ("SmallerWhite", "1"),
            ("Split1510ms", "1"),
            ("TrackEarlyJudgments", "1"),
            ("ScatterplotGreatMax", "1"),
            ("ScatterplotMaxWindow", "Excellent"),
            ("ScorePosition", "Step Statistics"),
            ("ScoreDisplay", "Predictive"),
            ("CustomFantasticWindow", "1"),
            ("CustomFantasticWindowMs", "23"),
            ("JudgmentTilt", "1"),
            ("ColumnCues", "1"),
            ("MeasureCues", "1"),
            ("JudgmentBack", "1"),
            ("ErrorMSDisplay", "1"),
            ("DisplayScorebox", "1"),
            ("LiveTimingStats", "1"),
            ("LiveTimingStatsMask", "3"),
            ("RainbowMax", "1"),
            ("ResponsiveColors", "1"),
            ("ShowLifePercent", "1"),
            ("TiltMultiplier", "1.5"),
            ("TiltCutoffMs", "99ms"),
            ("TiltMaxThresholdMs", "50ms"),
        ];

        load_timing_feedback_options(&mut options, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        });

        assert!(options.show_fa_plus_window);
        assert!(options.show_ex_score);
        assert!(options.show_hard_ex_score);
        assert!(options.show_fa_plus_pane);
        assert!(options.fa_plus_10ms_blue_window);
        assert!(options.split_15_10ms);
        assert!(options.track_early_judgments);
        assert!(options.scale_scatterplot);
        assert_eq!(
            options.scatterplot_max_window,
            ScatterplotMaxWindow::Excellent
        );
        assert_eq!(options.score_position, ScorePosition::StepStatistics);
        assert_eq!(options.score_display_mode, ScoreDisplayMode::Predictive);
        assert!(options.custom_fantastic_window);
        assert_eq!(
            options.custom_fantastic_window_ms,
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert!(options.judgment_tilt);
        assert!(options.column_cues);
        assert!(options.measure_cues);
        assert!(options.judgment_back);
        assert!(options.error_ms_display);
        assert!(options.display_scorebox);
        assert!(options.live_timing_stats);
        assert_eq!(
            options.live_timing_stats_mask,
            LiveTimingStatsMask::MEAN | LiveTimingStatsMask::MEAN_ABS
        );
        assert!(options.rainbow_max);
        assert!(options.responsive_colors);
        assert!(options.show_life_percent);
        assert!((options.tilt_multiplier - 1.5).abs() < f32::EPSILON);
        assert_eq!(options.tilt_min_threshold_ms, 99);
        assert_eq!(options.tilt_max_threshold_ms, 99);

        let mut legacy = PlayerOptionsData::default();
        load_timing_feedback_options(&mut legacy, |key| {
            (key == "LiveTimingStats").then(|| "1".to_string())
        });
        assert!(legacy.live_timing_stats);
        assert_eq!(legacy.live_timing_stats_mask, LiveTimingStatsMask::all());
    }

    #[test]
    fn profile_error_bar_and_measure_counter_options_update() {
        let mut profile = Profile::default();

        assert!(profile.set_error_bar_options(true, true));
        assert!(profile.error_bar_up);
        assert!(profile.error_bar_multi_tick);
        assert!(!profile.set_error_bar_options(true, true));

        assert!(profile.set_measure_counter_lookahead(9));
        assert_eq!(profile.measure_counter_lookahead, 4);
        assert!(!profile.set_measure_counter_lookahead(9));

        assert!(profile.set_measure_counter_options(false, true, true, true, true));
        assert!(!profile.measure_counter_left);
        assert!(profile.measure_counter_up);
        assert!(profile.measure_counter_vert);
        assert!(profile.broken_run);
        assert!(profile.run_timer);
        assert!(!profile.set_measure_counter_options(false, true, true, true, true));
    }

    #[test]
    fn player_side_indices_and_joined_masks_are_stable() {
        assert_eq!(PLAYER_SLOTS, 2);
        assert_eq!(DEFAULT_PROFILE_ID, "00000000");
        assert_eq!(LOCAL_PROFILE_MAX_ID, 99_999_999);
        assert_eq!(player_side_index(PlayerSide::P1), 0);
        assert_eq!(player_side_index(PlayerSide::P2), 1);
        assert_eq!(player_side_number(PlayerSide::P1), 1);
        assert_eq!(player_side_number(PlayerSide::P2), 2);
        assert_eq!(player_side_for_index(0), PlayerSide::P1);
        assert_eq!(player_side_for_index(1), PlayerSide::P2);
        assert_eq!(player_side_for_index(2), PlayerSide::P1);
        assert_eq!(SESSION_JOINED_MASK_P1, 1 << 0);
        assert_eq!(SESSION_JOINED_MASK_P2, 1 << 1);
        assert_eq!(
            player_side_joined_mask(PlayerSide::P1),
            SESSION_JOINED_MASK_P1
        );
        assert_eq!(
            player_side_joined_mask(PlayerSide::P2),
            SESSION_JOINED_MASK_P2
        );

        let mask = joined_player_mask(true, false);
        assert!(player_side_is_joined(mask, PlayerSide::P1));
        assert!(!player_side_is_joined(mask, PlayerSide::P2));

        let mask = joined_player_mask(false, true);
        assert!(!player_side_is_joined(mask, PlayerSide::P1));
        assert!(player_side_is_joined(mask, PlayerSide::P2));
    }

    #[test]
    fn play_style_follows_join_count() {
        assert_eq!(
            play_style_for_joined(PlayStyle::Single, true, true),
            PlayStyle::Versus
        );
        assert_eq!(
            play_style_for_joined(PlayStyle::Double, true, true),
            PlayStyle::Versus
        );
        assert_eq!(
            play_style_for_joined(PlayStyle::Double, true, false),
            PlayStyle::Double
        );
        assert_eq!(
            play_style_for_joined(PlayStyle::Versus, false, true),
            PlayStyle::Single
        );
    }

    #[test]
    fn runtime_player_p2_includes_single_player_styles() {
        assert!(!runtime_player_is_p2(PlayStyle::Single, PlayerSide::P1));
        assert!(runtime_player_is_p2(PlayStyle::Single, PlayerSide::P2));
        assert!(!runtime_player_is_p2(PlayStyle::Double, PlayerSide::P1));
        assert!(runtime_player_is_p2(PlayStyle::Double, PlayerSide::P2));
        assert!(!runtime_player_is_p2(PlayStyle::Versus, PlayerSide::P2));
        assert!(!is_single_p2_side(PlayStyle::Single, PlayerSide::P1));
        assert!(is_single_p2_side(PlayStyle::Single, PlayerSide::P2));
        assert!(!is_single_p2_side(PlayStyle::Double, PlayerSide::P2));
        assert!(!is_single_p2_side(PlayStyle::Versus, PlayerSide::P2));
        assert_eq!(runtime_player_index(PlayStyle::Single, PlayerSide::P2), 0);
        assert_eq!(runtime_player_index(PlayStyle::Double, PlayerSide::P2), 0);
        assert_eq!(runtime_player_index(PlayStyle::Versus, PlayerSide::P1), 0);
        assert_eq!(runtime_player_index(PlayStyle::Versus, PlayerSide::P2), 1);

        assert_eq!(
            runtime_player_side(PlayStyle::Single, PlayerSide::P2, 0),
            PlayerSide::P2
        );
        assert_eq!(
            runtime_player_side(PlayStyle::Double, PlayerSide::P1, 1),
            PlayerSide::P1
        );
        assert_eq!(
            runtime_player_side(PlayStyle::Versus, PlayerSide::P1, 0),
            PlayerSide::P1
        );
        assert_eq!(
            runtime_player_side(PlayStyle::Versus, PlayerSide::P1, 1),
            PlayerSide::P2
        );
    }

    #[test]
    fn local_profile_ids_reject_pathlike_or_empty_values() {
        assert!(is_local_profile_id("00000000"));
        assert!(is_local_profile_id("Player One"));
        assert!(is_local_profile_id(&"a".repeat(64)));

        assert!(!is_local_profile_id(""));
        assert!(!is_local_profile_id("."));
        assert!(!is_local_profile_id(".."));
        assert!(!is_local_profile_id("a/b"));
        assert!(!is_local_profile_id("a\\b"));
        assert!(!is_local_profile_id("a\0b"));
        assert!(!is_local_profile_id(&"a".repeat(65)));
    }

    #[test]
    fn profile_id_sorting_is_case_insensitive_with_stable_tiebreak() {
        let mut ids = ["beta", "Alpha", "alpha", "Beta", "00000000"];
        ids.sort_by(|a, b| cmp_profile_ids_case_insensitive(a, b));
        assert_eq!(ids, ["00000000", "Alpha", "alpha", "Beta", "beta"]);
    }

    #[test]
    fn profile_stats_roundtrip_preserves_current_combo_and_known_packs() {
        let stats = ProfileStats {
            current_combo: 12,
            known_pack_names: ["Beta", "Alpha"].into_iter().map(str::to_owned).collect(),
        };

        let bytes = encode_profile_stats(&stats).expect("profile stats should encode");
        let decoded = decode_profile_stats(&bytes).expect("profile stats should decode");
        assert_eq!(decoded, stats);

        let (raw, _) =
            bincode::decode_from_slice::<ProfileStatsV1, _>(&bytes, bincode::config::standard())
                .expect("encoded stats should use v1 shape");
        assert_eq!(
            raw.known_pack_names,
            vec!["Alpha".to_string(), "Beta".to_string()]
        );
    }

    #[test]
    fn favorites_content_trims_ignores_empty_lines_and_dedupes() {
        let favorites = parse_favorites_content(" abc123 \n\nxyz789\nabc123\n   \n");

        assert_eq!(favorites.len(), 2);
        assert!(favorites.contains("abc123"));
        assert!(favorites.contains("xyz789"));
    }

    #[test]
    fn favorites_content_renders_sorted_without_trailing_newline() {
        let favorites = HashSet::from([
            "xyz789".to_string(),
            "abc123".to_string(),
            "mid456".to_string(),
        ]);

        assert_eq!(
            render_favorites_content(&favorites),
            "abc123\nmid456\nxyz789"
        );
        assert_eq!(render_favorites_content(&HashSet::new()), "");
    }

    #[test]
    fn favorited_packs_content_trims_ignores_empty_lines_and_dedupes() {
        let packs = parse_favorited_packs_content(" Tachyon Alpha \n\nIn The Groove\nTachyon Alpha\n   \n");

        assert_eq!(packs.len(), 2);
        assert!(packs.contains("Tachyon Alpha"));
        assert!(packs.contains("In The Groove"));
    }

    #[test]
    fn favorited_packs_content_renders_case_insensitive_sorted() {
        let packs = HashSet::from([
            "zebra mix".to_string(),
            "Alpha Pack".to_string(),
            "midpack".to_string(),
        ]);

        assert_eq!(
            render_favorited_packs_content(&packs),
            "Alpha Pack\nmidpack\nzebra mix"
        );
        assert_eq!(render_favorited_packs_content(&HashSet::new()), "");
    }

    #[test]
    fn known_pack_names_add_only_new_entries() {
        let mut known = HashSet::from(["Alpha".to_string()]);

        assert!(add_known_pack_names(&mut known, ["Alpha", "Beta"]));
        assert_eq!(known.len(), 2);
        assert!(known.contains("Alpha"));
        assert!(known.contains("Beta"));

        assert!(!add_known_pack_names(&mut known, ["Alpha", "Beta"]));
    }

    #[test]
    fn unknown_pack_names_reports_scanned_packs_not_in_profile() {
        let known = HashSet::from(["Alpha".to_string()]);
        let scanned = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
        let unknown = unknown_pack_names(&known, &scanned);

        assert_eq!(unknown.len(), 2);
        assert!(unknown.contains("Beta"));
        assert!(unknown.contains("Gamma"));
        assert!(!unknown.contains("Alpha"));
    }

    #[test]
    fn profile_stats_decode_accepts_legacy_combo_payload() {
        let bytes = bincode::encode_to_vec(
            LegacyProfileStatsV1 {
                version: PROFILE_STATS_VERSION_V1,
                current_combo: 42,
            },
            bincode::config::standard(),
        )
        .expect("legacy stats should encode");

        let stats = decode_profile_stats(&bytes).expect("legacy stats should decode");
        assert_eq!(stats.current_combo, 42);
        assert!(stats.known_pack_names.is_empty());
    }

    #[test]
    fn profile_stats_decode_rejects_unsupported_version() {
        let bytes = bincode::encode_to_vec(
            ProfileStatsV1 {
                version: PROFILE_STATS_VERSION_V1 + 1,
                current_combo: 0,
                known_pack_names: Vec::new(),
            },
            bincode::config::standard(),
        )
        .expect("stats should encode");

        assert_eq!(
            decode_profile_stats(&bytes),
            Err(ProfileStatsDecodeError::UnsupportedVersion(
                PROFILE_STATS_VERSION_V1 + 1
            ))
        );
    }

    #[test]
    fn next_local_profile_id_prefers_append_then_wraps_to_gaps() {
        assert_eq!(next_local_profile_id(Vec::new()), Some("00000000".into()));
        assert_eq!(
            next_local_profile_id(vec![0, 1, 1, 2]),
            Some("00000003".into())
        );
        assert_eq!(next_local_profile_id(vec![0, 2]), Some("00000003".into()));
        assert_eq!(
            next_local_profile_id(vec![0, LOCAL_PROFILE_MAX_ID]),
            Some("00000001".into())
        );
    }

    #[test]
    fn next_local_profile_number_reports_full_small_ranges() {
        assert_eq!(next_local_profile_number(vec![0, 1, 2], 2), None);
        assert_eq!(next_local_profile_number(vec![0, 2], 2), Some(1));
        assert_eq!(next_local_profile_number(vec![0, 9], 2), Some(1));
    }

    #[test]
    fn profile_display_name_rewrite_updates_existing_userprofile_key() {
        let src = "[userprofile]\nDisplayName=Old\nPlayerInitials=OLD\n";
        let out = rewrite_profile_display_name_content(src, "New Name");
        assert_eq!(
            out,
            "[userprofile]\nDisplayName=New Name\nPlayerInitials=OLD\n"
        );
    }

    #[test]
    fn profile_display_name_rewrite_adds_missing_key_before_next_section() {
        let src = "[userprofile]\nPlayerInitials=OLD\n\n[Stats]\nCalories=0\n";
        let out = rewrite_profile_display_name_content(src, "New Name");
        assert_eq!(
            out,
            "[userprofile]\nPlayerInitials=OLD\n\nDisplayName=New Name\n[Stats]\nCalories=0\n"
        );
    }

    #[test]
    fn profile_display_name_rewrite_appends_missing_section() {
        let src = "[Stats]\nCalories=0\n";
        let out = rewrite_profile_display_name_content(src, "New Name");
        assert_eq!(
            out,
            "[Stats]\nCalories=0\n[userprofile]\nDisplayName=New Name\n"
        );

        let out = rewrite_profile_display_name_content("", "New Name");
        assert_eq!(out, "[userprofile]\nDisplayName=New Name\n");
    }

    #[test]
    fn profile_avatar_path_prefers_profile_png() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-profile-avatar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let avatar = dir.join("avatar.png");
        let profile = dir.join("profile.png");
        fs::write(&avatar, b"avatar").unwrap();

        assert_eq!(find_profile_avatar_path(&dir), Some(avatar.clone()));

        fs::write(&profile, b"profile").unwrap();
        assert_eq!(find_profile_avatar_path(&dir), Some(profile));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn active_profile_helpers_report_guest_or_local_id() {
        let guest = ActiveProfile::Guest;
        assert!(active_profile_is_guest(&guest));
        assert_eq!(active_profile_local_id(&guest), None);

        let local = ActiveProfile::Local {
            id: "00000042".to_string(),
        };
        assert!(!active_profile_is_guest(&local));
        assert_eq!(active_profile_local_id(&local), Some("00000042"));
    }

    #[test]
    fn long_error_bar_intensity_clamps_to_supported_range() {
        assert!((LONG_ERROR_BAR_INTENSITY_DEFAULT - 2.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.0) - 1.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(2.0) - 2.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(0.0) - LONG_ERROR_BAR_INTENSITY_MIN).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(5.0) - LONG_ERROR_BAR_INTENSITY_MAX).abs() < 1e-6);
        assert!(
            (clamp_long_error_bar_intensity(f32::NAN) - LONG_ERROR_BAR_INTENSITY_DEFAULT).abs()
                < 1e-6
        );
        assert!(
            (clamp_long_error_bar_intensity(f32::INFINITY) - LONG_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn long_error_bar_intensity_snaps_to_quarter_step_grid() {
        assert!((clamp_long_error_bar_intensity(1.10) - 1.00).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.13) - 1.25).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.40) - 1.50).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.75) - 1.75).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.95) - 2.00).abs() < 1e-6);
        let count = ((LONG_ERROR_BAR_INTENSITY_MAX - LONG_ERROR_BAR_INTENSITY_MIN)
            / LONG_ERROR_BAR_INTENSITY_STEP)
            .round() as usize
            + 1;
        assert_eq!(count, 13);
    }

    #[test]
    fn average_error_bar_intensity_clamps_to_supported_range() {
        assert!((AVERAGE_ERROR_BAR_INTENSITY_DEFAULT - 1.0).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.0) - 1.0).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(2.0) - 2.0).abs() < 1e-6);
        assert!(
            (clamp_average_error_bar_intensity(0.0) - AVERAGE_ERROR_BAR_INTENSITY_MIN).abs() < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(5.0) - AVERAGE_ERROR_BAR_INTENSITY_MAX).abs() < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(f32::NAN) - AVERAGE_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(f32::INFINITY)
                - AVERAGE_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn average_error_bar_intensity_snaps_to_quarter_step_grid() {
        assert!((clamp_average_error_bar_intensity(1.10) - 1.00).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.13) - 1.25).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.40) - 1.50).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.75) - 1.75).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.95) - 2.00).abs() < 1e-6);
        let count = ((AVERAGE_ERROR_BAR_INTENSITY_MAX - AVERAGE_ERROR_BAR_INTENSITY_MIN)
            / AVERAGE_ERROR_BAR_INTENSITY_STEP)
            .round() as usize
            + 1;
        assert_eq!(count, 5);
    }

    #[test]
    fn average_error_bar_interval_clamps_to_supported_range() {
        assert_eq!(AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT, 400);
        assert_eq!(clamp_average_error_bar_interval_ms(100), 100);
        assert_eq!(clamp_average_error_bar_interval_ms(2000), 2000);
        assert_eq!(
            clamp_average_error_bar_interval_ms(0),
            AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        );
        assert_eq!(
            clamp_average_error_bar_interval_ms(4000),
            AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
        );
    }

    #[test]
    fn average_error_bar_interval_snaps_to_100ms_step_grid() {
        assert_eq!(AVERAGE_ERROR_BAR_INTERVAL_MS_STEP, 100);
        assert_eq!(clamp_average_error_bar_interval_ms(149), 100);
        assert_eq!(clamp_average_error_bar_interval_ms(150), 200);
        assert_eq!(clamp_average_error_bar_interval_ms(349), 300);
        assert_eq!(clamp_average_error_bar_interval_ms(350), 400);
        assert_eq!(clamp_average_error_bar_interval_ms(1951), 2000);
    }

    #[test]
    fn profile_window_clamps_keep_supported_ranges() {
        assert_eq!(clamp_tilt_threshold_ms(0), 0);
        assert_eq!(clamp_tilt_threshold_ms(50), 50);
        assert_eq!(clamp_tilt_threshold_ms(101), TILT_THRESHOLD_MAX_MS);
        assert_eq!(
            clamp_custom_fantastic_window_ms(0),
            CUSTOM_FANTASTIC_WINDOW_MIN_MS
        );
        assert_eq!(clamp_custom_fantastic_window_ms(10), 10);
        assert_eq!(
            clamp_custom_fantastic_window_ms(23),
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert_eq!(
            clamp_long_error_bar_threshold_ms(0),
            LONG_ERROR_BAR_THRESHOLD_MS_MIN
        );
        assert_eq!(
            clamp_long_error_bar_threshold_ms(99),
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert_eq!(
            clamp_long_error_bar_min_samples(0),
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
        assert_eq!(
            clamp_long_error_bar_min_samples(99),
            LONG_ERROR_BAR_MIN_SAMPLES_MAX
        );
    }

    #[test]
    fn clamp_weight_pounds_preserves_unset_and_bounds_user_values() {
        assert_eq!(clamp_weight_pounds(0), 0);
        assert_eq!(clamp_weight_pounds(-50), 20);
        assert_eq!(clamp_weight_pounds(19), 20);
        assert_eq!(clamp_weight_pounds(120), 120);
        assert_eq!(clamp_weight_pounds(1001), 1000);
    }

    #[test]
    fn profile_stat_defaults_match_itg_fallbacks() {
        assert_eq!(resolved_weight_pounds(0), DEFAULT_WEIGHT_POUNDS);
        assert_eq!(resolved_weight_pounds(165), 165);
        assert_eq!(age_years_for_birth_year(0, 2026), 2026 - DEFAULT_BIRTH_YEAR);
        assert_eq!(age_years_for_birth_year(2000, 2026), 26);
    }

    #[test]
    fn tap_explosion_mask_maps_judgment_windows() {
        assert_eq!(
            tap_explosion_mask_for_window("W0"),
            Some(TapExplosionMask::FANTASTIC)
        );
        assert_eq!(
            tap_explosion_mask_for_window("W1"),
            Some(TapExplosionMask::FANTASTIC)
        );
        assert_eq!(
            tap_explosion_mask_for_window("W5"),
            Some(TapExplosionMask::WAY_OFF)
        );
        assert_eq!(
            tap_explosion_mask_for_window("Miss"),
            Some(TapExplosionMask::MISS)
        );
        assert_eq!(
            tap_explosion_mask_for_window("Held"),
            Some(TapExplosionMask::HELD)
        );
        assert_eq!(tap_explosion_mask_for_window("Holding"), None);
    }

    #[test]
    fn tap_explosion_mask_enabled_checks_window_flags() {
        let mask = TapExplosionMask::MISS | TapExplosionMask::HELD;
        assert!(tap_explosion_mask_enabled(mask, "Miss"));
        assert!(tap_explosion_mask_enabled(mask, "Held"));
        assert!(!tap_explosion_mask_enabled(mask, "W1"));
        assert!(!tap_explosion_mask_enabled(mask, "Holding"));
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
        assert_eq!(
            normalize_tap_explosion_mask(old_all.bits(), TAP_EXPLOSION_MASK_VERSION),
            old_all
        );
    }

    #[test]
    fn player_options_section_serializes_persisted_options() {
        let options = PlayerOptionsData {
            error_bar_active_mask: ErrorBarMask::COLORFUL | ErrorBarMask::TEXT,
            center_tick: true,
            average_error_bar_intensity: 1.13,
            long_error_bar_intensity: 1.95,
            text_error_bar_scalable: true,
            text_error_bar_threshold_ms: 17,
            tap_explosion_active_mask: TapExplosionMask::FANTASTIC | TapExplosionMask::MISS,
            score_position: ScorePosition::StepStatistics,
            score_display_mode: ScoreDisplayMode::Predictive,
            step_stats_extra: StepStatsExtra::CatJAM,
            column_flash_brightness: ColumnFlashBrightness::Dimmed,
            column_flash_size: ColumnFlashSize::Compact,
            mini_percent: 42,
            global_offset_shift_ms: -9,
            no_cmod_alternative: NoCmodAlternative::XMod,
            ..PlayerOptionsData::default()
        };

        let mut content = String::new();
        append_player_options_section(&mut content, "PlayerOptionsSingles", &options);

        assert!(content.starts_with("[PlayerOptionsSingles]\n"));
        assert!(content.contains("ErrorBarMask=5\n"));
        assert!(content.contains("CenterTick=1\n"));
        assert!(content.contains("Colorful=1\n"));
        assert!(content.contains("Text=1\n"));
        assert!(content.contains("TextErrorBarScalable=1\n"));
        assert!(content.contains("TextErrorBar10ms=1\n"));
        assert!(content.contains("TextErrorBarThresholdMs=17\n"));
        assert!(content.contains("AverageErrorBarIntensity=1.25\n"));
        assert!(content.contains("LongErrorBarIntensity=2.00\n"));
        assert!(content.contains("TapExplosionMask=65\n"));
        assert!(content.contains("ScorePosition=Step Statistics\n"));
        assert!(content.contains("ScoreDisplay=Predictive\n"));
        assert!(content.contains("StepStatsExtra=CatJAM\n"));
        assert!(content.contains("NoCmodAlternative=XMod\n"));
        assert!(content.contains("MiniIndicatorSubtractiveDisplay=Percent\n"));
        assert!(content.contains("MiniIndicatorPosition=Default\n"));
        assert!(content.contains(&format!(
            "TapExplosionMaskVersion={TAP_EXPLOSION_MASK_VERSION}\n"
        )));
        assert!(content.contains("HideEarlyDecentWayOffColumnFlash=0\n"));
        assert!(content.contains("ColumnFlashMask=64\n"));
        assert!(content.contains("ColumnFlashBrightness=Dimmed\n"));
        assert!(content.contains("ColumnFlashSize=Compact\n"));
        assert!(content.contains("MiniPercent=42\n"));
        assert!(content.contains("GlobalOffsetShiftMs=-9\n"));
    }

    #[test]
    fn no_cmod_alternative_parses_display_and_aliases() {
        // Display round-trips.
        for v in [
            NoCmodAlternative::None,
            NoCmodAlternative::XMod,
            NoCmodAlternative::MMod,
        ] {
            assert_eq!(NoCmodAlternative::from_str(&v.to_string()).unwrap(), v);
        }
        // Case-insensitive aliases and empty/off map to None.
        assert_eq!(
            NoCmodAlternative::from_str("xmod").unwrap(),
            NoCmodAlternative::XMod
        );
        assert_eq!(
            NoCmodAlternative::from_str("M").unwrap(),
            NoCmodAlternative::MMod
        );
        assert_eq!(
            NoCmodAlternative::from_str("").unwrap(),
            NoCmodAlternative::None
        );
        assert_eq!(
            NoCmodAlternative::from_str("off").unwrap(),
            NoCmodAlternative::None
        );
        assert!(NoCmodAlternative::from_str("zmod").is_err());
    }

    #[test]
    fn sanitize_player_initials_limits_to_four_ascii_chars() {
        assert_eq!(sanitize_player_initials("ab?c!de"), "AB?C");
        assert_eq!(sanitize_player_initials("a b-c_d"), "ABCD");
        assert_eq!(sanitize_player_initials(""), "");
        assert_eq!(PLAYER_INITIALS_MAX_LEN, 4);
    }

    #[test]
    fn initials_from_name_uses_two_char_fallbacks() {
        assert_eq!(initials_from_name("john smith"), "JOHN");
        assert_eq!(initials_from_name("a"), "A?");
        assert_eq!(initials_from_name("!!!"), "!!!");
        assert_eq!(initials_from_name(""), "??");
    }

    #[test]
    fn parse_profile_bool_accepts_legacy_boolean_spellings() {
        for value in ["1", "true", "yes", "on", " TRUE "] {
            assert_eq!(parse_profile_bool(value), Some(true));
        }
        for value in ["0", "false", "no", "off", " FALSE "] {
            assert_eq!(parse_profile_bool(value), Some(false));
        }
        assert_eq!(parse_profile_bool("maybe"), None);
    }

    #[test]
    fn groovestats_pad_player_requires_explicit_one() {
        assert!(parse_groovestats_is_pad_player(Some("1"), false));
        assert!(!parse_groovestats_is_pad_player(Some("0"), true));
        assert!(!parse_groovestats_is_pad_player(Some("2"), true));
        assert!(parse_groovestats_is_pad_player(Some("true"), true));
        assert!(!parse_groovestats_is_pad_player(Some("true"), false));
        assert!(parse_groovestats_is_pad_player(None, true));
        assert!(!parse_groovestats_is_pad_player(None, false));
    }

    #[test]
    fn parse_last_played_value_trims_empty_optional_fields() {
        assert_eq!(parse_last_played_value(None), None);
        assert_eq!(parse_last_played_value(Some("")), None);
        assert_eq!(parse_last_played_value(Some("   ")), None);
        assert_eq!(
            parse_last_played_value(Some(" Songs/Pack/Song.ogg ")),
            Some("Songs/Pack/Song.ogg".to_string())
        );
    }

    #[test]
    fn player_options_section_matches_style_storage() {
        assert_eq!(
            player_options_section(PlayStyle::Single),
            "PlayerOptionsSingles"
        );
        assert_eq!(
            player_options_section(PlayStyle::Versus),
            "PlayerOptionsSingles"
        );
        assert_eq!(
            player_options_section(PlayStyle::Double),
            "PlayerOptionsDoubles"
        );
    }

    #[test]
    fn hud_player_snapshot_defaults_to_guestless_unjoined() {
        let snapshot = GameplayHudPlayerSnapshot::default();
        assert!(!snapshot.joined);
        assert!(!snapshot.guest);
        assert_eq!(snapshot.display_name, "");
        assert_eq!(snapshot.avatar_texture_key, None);
    }

    #[test]
    fn last_played_defaults_to_medium_song_and_empty_course() {
        let last_song = LastPlayed::default();
        assert_eq!(last_song.song_music_path, None);
        assert_eq!(last_song.chart_hash, None);
        assert_eq!(last_song.difficulty_index, 2);

        let last_course = LastPlayedCourse::default();
        assert_eq!(last_course.course_path, None);
        assert_eq!(last_course.difficulty_name, None);
    }

    #[test]
    fn last_played_sections_render_empty_and_present_fields() {
        let mut content = String::new();
        append_last_played_section(&mut content, "LastPlayedSingles", &LastPlayed::default());
        assert_eq!(
            content,
            "[LastPlayedSingles]\nMusicPath=\nChartHash=\nDifficultyIndex=2\n\n"
        );

        content.clear();
        append_last_played_section(
            &mut content,
            "LastPlayedDoubles",
            &LastPlayed {
                song_music_path: Some("Songs/Pack/Song.ogg".to_string()),
                chart_hash: Some("abc123".to_string()),
                difficulty_index: 4,
            },
        );
        assert_eq!(
            content,
            "[LastPlayedDoubles]\nMusicPath=Songs/Pack/Song.ogg\nChartHash=abc123\nDifficultyIndex=4\n\n"
        );
    }

    #[test]
    fn last_played_section_loads_present_fields_and_defaults() {
        let default = LastPlayed {
            song_music_path: Some("fallback.ogg".to_string()),
            chart_hash: Some("fallbackhash".to_string()),
            difficulty_index: 3,
        };
        let values = [
            ("MusicPath", " Songs/Pack/Song.ogg "),
            ("ChartHash", "abc123"),
        ];

        let loaded = load_last_played_section(
            true,
            |key| {
                values
                    .iter()
                    .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
            },
            &default,
        )
        .expect("present section should load");

        assert_eq!(
            loaded.song_music_path,
            Some("Songs/Pack/Song.ogg".to_string())
        );
        assert_eq!(loaded.chart_hash, Some("abc123".to_string()));
        assert_eq!(loaded.difficulty_index, 3);
        assert_eq!(load_last_played_section(false, |_| None, &default), None);
    }

    #[test]
    fn last_played_course_sections_render_empty_and_present_fields() {
        let mut content = String::new();
        append_last_played_course_section(
            &mut content,
            "LastPlayedCourseSingles",
            &LastPlayedCourse::default(),
        );
        assert_eq!(
            content,
            "[LastPlayedCourseSingles]\nCoursePath=\nDifficultyName=\n\n"
        );

        content.clear();
        append_last_played_course_section(
            &mut content,
            "LastPlayedCourseDoubles",
            &LastPlayedCourse {
                course_path: Some("Courses/Test.crs".to_string()),
                difficulty_name: Some("Hard".to_string()),
            },
        );
        assert_eq!(
            content,
            "[LastPlayedCourseDoubles]\nCoursePath=Courses/Test.crs\nDifficultyName=Hard\n\n"
        );
    }

    #[test]
    fn last_played_course_section_loads_present_fields() {
        let values = [
            ("CoursePath", " Courses/Test.crs "),
            ("DifficultyName", "Hard"),
        ];

        let loaded = load_last_played_course_section(true, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        })
        .expect("present course section should load");

        assert_eq!(loaded.course_path, Some("Courses/Test.crs".to_string()));
        assert_eq!(loaded.difficulty_name, Some("Hard".to_string()));
        assert_eq!(load_last_played_course_section(false, |_| None), None);
    }

    #[test]
    fn hide_light_type_round_trips() {
        for setting in [
            HideLightType::NoHideLights,
            HideLightType::HideAllLights,
            HideLightType::HideMarqueeLights,
            HideLightType::HideBassLights,
        ] {
            assert_eq!(setting.to_string().parse::<HideLightType>(), Ok(setting));
        }
        assert!(HideLightType::from_str("unknown").is_err());
    }

    #[test]
    fn perspective_round_trips_and_reports_tilt_skew() {
        for (setting, skew) in [
            (Perspective::Overhead, (0.0, 0.0)),
            (Perspective::Hallway, (-1.0, 0.0)),
            (Perspective::Distant, (1.0, 0.0)),
            (Perspective::Incoming, (-1.0, 1.0)),
            (Perspective::Space, (1.0, 1.0)),
        ] {
            assert_eq!(setting.to_string().parse::<Perspective>(), Ok(setting));
            assert_eq!(setting.tilt_skew(), skew);
        }
        assert!(Perspective::from_str("flat").is_err());
    }

    #[test]
    fn turn_option_round_trips_and_accepts_aliases() {
        for setting in [
            TurnOption::None,
            TurnOption::Mirror,
            TurnOption::Left,
            TurnOption::Right,
            TurnOption::LRMirror,
            TurnOption::UDMirror,
            TurnOption::Shuffle,
            TurnOption::Blender,
            TurnOption::Random,
        ] {
            assert_eq!(setting.to_string().parse::<TurnOption>(), Ok(setting));
        }
        assert_eq!(TurnOption::from_str("NoTurn"), Ok(TurnOption::None));
        assert_eq!(
            TurnOption::from_str("super shuffle"),
            Ok(TurnOption::Blender)
        );
        assert_eq!(
            TurnOption::from_str("hyper shuffle"),
            Ok(TurnOption::Random)
        );
        assert!(TurnOption::from_str("up").is_err());
    }

    #[test]
    fn scroll_option_parses_and_formats_combined_flags() {
        for setting in [
            ScrollOption::Normal,
            ScrollOption::Reverse,
            ScrollOption::Split,
            ScrollOption::Alternate,
            ScrollOption::Cross,
            ScrollOption::Centered,
        ] {
            assert_eq!(setting.to_string().parse::<ScrollOption>(), Ok(setting));
        }

        let combined = ScrollOption::from_str("Reverse+Cross Centered").unwrap();
        assert!(combined.contains(ScrollOption::Reverse));
        assert!(combined.contains(ScrollOption::Cross));
        assert!(combined.contains(ScrollOption::Centered));
        assert_eq!(combined.to_string(), "Reverse+Cross+Centered");

        assert_eq!(
            ScrollOption::from_str("Normal,Reverse"),
            Ok(ScrollOption::Reverse)
        );
        assert!(ScrollOption::from_str("").is_err());
        assert!(ScrollOption::from_str("hidden").is_err());
    }

    #[test]
    fn combo_mode_round_trips() {
        for setting in [ComboMode::FullCombo, ComboMode::CurrentCombo] {
            assert_eq!(setting.to_string().parse::<ComboMode>(), Ok(setting));
        }
        assert!(ComboMode::from_str("sessioncombo").is_err());
    }

    #[test]
    fn combo_colors_round_trips() {
        for setting in [
            ComboColors::Glow,
            ComboColors::Solid,
            ComboColors::Rainbow,
            ComboColors::RainbowScroll,
            ComboColors::None,
        ] {
            assert_eq!(setting.to_string().parse::<ComboColors>(), Ok(setting));
        }
        assert!(ComboColors::from_str("flashing").is_err());
    }

    #[test]
    fn combo_font_round_trips_and_accepts_aliases() {
        for setting in [
            ComboFont::Wendy,
            ComboFont::ArialRounded,
            ComboFont::Asap,
            ComboFont::BebasNeue,
            ComboFont::SourceCode,
            ComboFont::Work,
            ComboFont::WendyCursed,
            ComboFont::Mega,
            ComboFont::None,
        ] {
            assert_eq!(setting.to_string().parse::<ComboFont>(), Ok(setting));
        }
        assert_eq!(ComboFont::from_str("bebasneue"), Ok(ComboFont::BebasNeue));
        assert_eq!(ComboFont::from_str("sourcecode"), Ok(ComboFont::SourceCode));
        assert_eq!(
            ComboFont::from_str("wendycursed"),
            Ok(ComboFont::WendyCursed)
        );
        assert!(ComboFont::from_str("comic sans").is_err());
    }

    #[test]
    fn target_score_setting_parses_legacy_forms() {
        for (raw, setting) in [
            ("cminus", TargetScoreSetting::CMinus),
            ("c", TargetScoreSetting::C),
            ("cplus", TargetScoreSetting::CPlus),
            ("bminus", TargetScoreSetting::BMinus),
            ("b", TargetScoreSetting::B),
            ("bplus", TargetScoreSetting::BPlus),
            ("aminus", TargetScoreSetting::AMinus),
            ("a", TargetScoreSetting::A),
            ("aplus", TargetScoreSetting::APlus),
            ("sminus", TargetScoreSetting::SMinus),
            ("", TargetScoreSetting::S),
            ("s", TargetScoreSetting::S),
            ("splus", TargetScoreSetting::SPlus),
            ("machine", TargetScoreSetting::MachineBest),
            ("machinebest", TargetScoreSetting::MachineBest),
            ("personal", TargetScoreSetting::PersonalBest),
            ("personalbest", TargetScoreSetting::PersonalBest),
        ] {
            assert_eq!(TargetScoreSetting::from_str(raw), Ok(setting));
        }

        // Preserve the existing punctuation-stripping parser behavior.
        assert_eq!(
            TargetScoreSetting::from_str("C-"),
            Ok(TargetScoreSetting::C)
        );
        assert_eq!(
            TargetScoreSetting::from_str("A+"),
            Ok(TargetScoreSetting::A)
        );
        assert_eq!(
            TargetScoreSetting::from_str("S-"),
            Ok(TargetScoreSetting::S)
        );
        assert!(TargetScoreSetting::from_str("ss").is_err());
    }

    #[test]
    fn error_bar_style_round_trips() {
        for setting in [
            ErrorBarStyle::None,
            ErrorBarStyle::Colorful,
            ErrorBarStyle::Monochrome,
            ErrorBarStyle::Text,
            ErrorBarStyle::Highlight,
            ErrorBarStyle::Average,
        ] {
            assert_eq!(setting.to_string().parse::<ErrorBarStyle>(), Ok(setting));
        }
        assert!(ErrorBarStyle::from_str("split").is_err());
    }

    #[test]
    fn live_timing_stats_mask_layout_is_stable() {
        assert_eq!(LiveTimingStatsMask::MEAN.bits(), 1 << 0);
        assert_eq!(LiveTimingStatsMask::MEAN_ABS.bits(), 1 << 1);
        assert_eq!(LiveTimingStatsMask::MAX.bits(), 1 << 2);
        assert_eq!(LiveTimingStatsMask::all().bits(), 0b0000_0111);
        assert_eq!(
            LiveTimingStatsMask::from_bits_truncate(u8::MAX),
            LiveTimingStatsMask::all()
        );
    }

    #[test]
    fn error_bar_mask_layout_is_stable() {
        assert_eq!(ErrorBarMask::COLORFUL.bits(), 1 << 0);
        assert_eq!(ErrorBarMask::MONOCHROME.bits(), 1 << 1);
        assert_eq!(ErrorBarMask::TEXT.bits(), 1 << 2);
        assert_eq!(ErrorBarMask::HIGHLIGHT.bits(), 1 << 3);
        assert_eq!(ErrorBarMask::AVERAGE.bits(), 1 << 4);
        assert_eq!(ErrorBarMask::all().bits(), 0b0001_1111);
        assert_eq!(
            ErrorBarMask::from_bits_truncate(u8::MAX),
            ErrorBarMask::all()
        );
    }

    #[test]
    fn error_bar_helpers_roundtrip_through_mask() {
        let mask = error_bar_mask_from_style(ErrorBarStyle::Colorful, true);
        assert!(mask.contains(ErrorBarMask::COLORFUL));
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);
        assert!(error_bar_text_from_mask(mask));

        let mask = ErrorBarMask::COLORFUL | ErrorBarMask::MONOCHROME;
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);

        let mask = error_bar_mask_from_style(ErrorBarStyle::Text, false);
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert!(!mask.contains(ErrorBarMask::COLORFUL));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(error_bar_text_from_mask(mask));

        let mask = error_bar_mask_from_style(ErrorBarStyle::None, false);
        assert!(mask.is_empty());
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(!error_bar_text_from_mask(mask));
    }

    #[test]
    fn appearance_effects_mask_layout_is_stable() {
        assert_eq!(AppearanceEffectsMask::HIDDEN.bits(), 1 << 0);
        assert_eq!(AppearanceEffectsMask::SUDDEN.bits(), 1 << 1);
        assert_eq!(AppearanceEffectsMask::STEALTH.bits(), 1 << 2);
        assert_eq!(AppearanceEffectsMask::BLINK.bits(), 1 << 3);
        assert_eq!(AppearanceEffectsMask::RANDOM_VANISH.bits(), 1 << 4);
        assert_eq!(AppearanceEffectsMask::all().bits(), 0b0001_1111);
        assert_eq!(
            AppearanceEffectsMask::from_bits_truncate(u8::MAX),
            AppearanceEffectsMask::all()
        );
    }

    #[test]
    fn accel_effects_mask_layout_is_stable() {
        assert_eq!(AccelEffectsMask::BOOST.bits(), 1 << 0);
        assert_eq!(AccelEffectsMask::BRAKE.bits(), 1 << 1);
        assert_eq!(AccelEffectsMask::WAVE.bits(), 1 << 2);
        assert_eq!(AccelEffectsMask::EXPAND.bits(), 1 << 3);
        assert_eq!(AccelEffectsMask::BOOMERANG.bits(), 1 << 4);
        assert_eq!(AccelEffectsMask::all().bits(), 0b0001_1111);
        assert_eq!(
            AccelEffectsMask::from_bits_truncate(u8::MAX),
            AccelEffectsMask::all()
        );
    }

    #[test]
    fn holds_mask_layout_is_stable() {
        assert_eq!(HoldsMask::PLANTED.bits(), 1 << 0);
        assert_eq!(HoldsMask::FLOORED.bits(), 1 << 1);
        assert_eq!(HoldsMask::TWISTER.bits(), 1 << 2);
        assert_eq!(HoldsMask::NO_ROLLS.bits(), 1 << 3);
        assert_eq!(HoldsMask::HOLDS_TO_ROLLS.bits(), 1 << 4);
        assert_eq!(HoldsMask::all().bits(), 0b0001_1111);
        assert_eq!(HoldsMask::from_bits_truncate(u8::MAX), HoldsMask::all());
    }

    #[test]
    fn visual_effects_mask_layout_is_stable() {
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
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX),
            VisualEffectsMask::all()
        );
    }

    #[test]
    fn insert_mask_layout_is_stable() {
        assert_eq!(InsertMask::WIDE.bits(), 1 << 0);
        assert_eq!(InsertMask::BIG.bits(), 1 << 1);
        assert_eq!(InsertMask::QUICK.bits(), 1 << 2);
        assert_eq!(InsertMask::BMRIZE.bits(), 1 << 3);
        assert_eq!(InsertMask::SKIPPY.bits(), 1 << 4);
        assert_eq!(InsertMask::ECHO.bits(), 1 << 5);
        assert_eq!(InsertMask::STOMP.bits(), 1 << 6);
        assert_eq!(InsertMask::all().bits(), 0b0111_1111);
        assert_eq!(InsertMask::from_bits_truncate(u8::MAX), InsertMask::all());
    }

    #[test]
    fn remove_mask_layout_is_stable() {
        assert_eq!(RemoveMask::LITTLE.bits(), 1 << 0);
        assert_eq!(RemoveMask::NO_MINES.bits(), 1 << 1);
        assert_eq!(RemoveMask::NO_HOLDS.bits(), 1 << 2);
        assert_eq!(RemoveMask::NO_JUMPS.bits(), 1 << 3);
        assert_eq!(RemoveMask::NO_HANDS.bits(), 1 << 4);
        assert_eq!(RemoveMask::NO_QUADS.bits(), 1 << 5);
        assert_eq!(RemoveMask::NO_LIFTS.bits(), 1 << 6);
        assert_eq!(RemoveMask::NO_FAKES.bits(), 1 << 7);
        assert_eq!(RemoveMask::all().bits(), u8::MAX);
        assert_eq!(RemoveMask::from_bits_truncate(u8::MAX), RemoveMask::all());
    }

    #[test]
    fn tap_explosion_mask_layout_is_stable() {
        assert_eq!(TapExplosionMask::FANTASTIC.bits(), 1 << 0);
        assert_eq!(TapExplosionMask::EXCELLENT.bits(), 1 << 1);
        assert_eq!(TapExplosionMask::GREAT.bits(), 1 << 2);
        assert_eq!(TapExplosionMask::DECENT.bits(), 1 << 3);
        assert_eq!(TapExplosionMask::WAY_OFF.bits(), 1 << 4);
        assert_eq!(TapExplosionMask::HELD.bits(), 1 << 5);
        assert_eq!(TapExplosionMask::MISS.bits(), 1 << 6);
        assert_eq!(TapExplosionMask::HOLDING.bits(), 1 << 7);
        assert_eq!(TapExplosionMask::all().bits(), u8::MAX);
        assert_eq!(
            TapExplosionMask::from_bits_truncate(u8::MAX),
            TapExplosionMask::all()
        );
    }

    #[test]
    fn column_flash_mask_layout_is_stable() {
        assert_eq!(ColumnFlashMask::BLUE_FANTASTIC.bits(), 1 << 0);
        assert_eq!(ColumnFlashMask::WHITE_FANTASTIC.bits(), 1 << 1);
        assert_eq!(ColumnFlashMask::EXCELLENT.bits(), 1 << 2);
        assert_eq!(ColumnFlashMask::GREAT.bits(), 1 << 3);
        assert_eq!(ColumnFlashMask::DECENT.bits(), 1 << 4);
        assert_eq!(ColumnFlashMask::WAY_OFF.bits(), 1 << 5);
        assert_eq!(ColumnFlashMask::MISS.bits(), 1 << 6);
        assert_eq!(ColumnFlashMask::all().bits(), 0b0111_1111);
        assert_eq!(
            ColumnFlashMask::from_bits_truncate(u8::MAX),
            ColumnFlashMask::all()
        );
        assert!(column_flash_mask_enabled(
            ColumnFlashMask::MISS,
            JudgeGrade::Miss,
            false
        ));
        assert!(!column_flash_mask_enabled(
            ColumnFlashMask::MISS,
            JudgeGrade::Great,
            false
        ));
        assert!(column_flash_mask_enabled(
            ColumnFlashMask::BLUE_FANTASTIC,
            JudgeGrade::Fantastic,
            true
        ));
        assert!(column_flash_mask_enabled(
            ColumnFlashMask::WHITE_FANTASTIC,
            JudgeGrade::Fantastic,
            false
        ));
    }

    #[test]
    fn column_flash_visual_options_round_trip() {
        for setting in [ColumnFlashBrightness::Normal, ColumnFlashBrightness::Dimmed] {
            assert_eq!(
                setting.to_string().parse::<ColumnFlashBrightness>(),
                Ok(setting)
            );
        }
        for setting in [ColumnFlashSize::Default, ColumnFlashSize::Compact] {
            assert_eq!(setting.to_string().parse::<ColumnFlashSize>(), Ok(setting));
        }
        assert_eq!(
            ColumnFlashBrightness::from_str("standard"),
            Ok(ColumnFlashBrightness::Normal)
        );
        assert_eq!(
            ColumnFlashBrightness::from_str("dim"),
            Ok(ColumnFlashBrightness::Dimmed)
        );
        assert_eq!(
            ColumnFlashSize::from_str("short"),
            Ok(ColumnFlashSize::Compact)
        );
        assert!(ColumnFlashBrightness::from_str("brightest").is_err());
        assert!(ColumnFlashSize::from_str("wide").is_err());
    }

    #[test]
    fn attack_mode_round_trips() {
        for setting in [AttackMode::Off, AttackMode::On, AttackMode::Random] {
            assert_eq!(setting.to_string().parse::<AttackMode>(), Ok(setting));
        }
        assert_eq!(AttackMode::from_str("NoAttacks"), Ok(AttackMode::Off));
        assert_eq!(AttackMode::from_str("normal"), Ok(AttackMode::On));
        assert_eq!(
            AttackMode::from_str("random attacks"),
            Ok(AttackMode::Random)
        );
        assert!(AttackMode::from_str("chaos").is_err());
    }

    #[test]
    fn score_position_round_trips_and_accepts_stepstats_alias() {
        for setting in [ScorePosition::Normal, ScorePosition::StepStatistics] {
            assert_eq!(setting.to_string().parse::<ScorePosition>(), Ok(setting));
        }
        assert_eq!(
            ScorePosition::from_str("stepstats"),
            Ok(ScorePosition::StepStatistics)
        );
        assert_eq!(ScorePosition::from_str("top"), Ok(ScorePosition::Normal));
        assert!(ScorePosition::from_str("middle").is_err());
    }

    #[test]
    fn score_display_mode_round_trips_and_accepts_prediction_alias() {
        for setting in [ScoreDisplayMode::Normal, ScoreDisplayMode::Predictive] {
            assert_eq!(setting.to_string().parse::<ScoreDisplayMode>(), Ok(setting));
        }
        assert_eq!(
            ScoreDisplayMode::from_str("prediction"),
            Ok(ScoreDisplayMode::Predictive)
        );
        assert_eq!(
            ScoreDisplayMode::from_str("actual"),
            Ok(ScoreDisplayMode::Normal)
        );
        assert!(ScoreDisplayMode::from_str("middle").is_err());
    }

    #[test]
    fn scatterplot_max_window_round_trips() {
        for setting in [
            ScatterplotMaxWindow::Off,
            ScatterplotMaxWindow::Fantastic,
            ScatterplotMaxWindow::Excellent,
            ScatterplotMaxWindow::Great,
        ] {
            assert_eq!(
                setting.to_string().parse::<ScatterplotMaxWindow>(),
                Ok(setting)
            );
        }
        assert_eq!(
            ScatterplotMaxWindow::from_str("autoscale"),
            Ok(ScatterplotMaxWindow::Off)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("fa"),
            Ok(ScatterplotMaxWindow::Fantastic)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("excellent max"),
            Ok(ScatterplotMaxWindow::Excellent)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("greatmax"),
            Ok(ScatterplotMaxWindow::Great)
        );
        assert!(ScatterplotMaxWindow::from_str("decent").is_err());
    }

    #[test]
    fn life_meter_type_round_trips() {
        for setting in [
            LifeMeterType::Standard,
            LifeMeterType::Surround,
            LifeMeterType::Vertical,
        ] {
            assert_eq!(setting.to_string().parse::<LifeMeterType>(), Ok(setting));
        }
        assert_eq!(LifeMeterType::from_str(""), Ok(LifeMeterType::Standard));
        assert!(LifeMeterType::from_str("horizontal").is_err());
    }

    #[test]
    fn error_bar_trim_round_trips() {
        for setting in [
            ErrorBarTrim::Off,
            ErrorBarTrim::Fantastic,
            ErrorBarTrim::Excellent,
            ErrorBarTrim::Great,
        ] {
            assert_eq!(setting.to_string().parse::<ErrorBarTrim>(), Ok(setting));
        }
        assert!(ErrorBarTrim::from_str("decent").is_err());
    }

    #[test]
    fn timing_windows_option_round_trips_and_reports_disabled_windows() {
        for (setting, disabled) in [
            (TimingWindowsOption::None, [false; 5]),
            (
                TimingWindowsOption::WayOffs,
                [false, false, false, false, true],
            ),
            (
                TimingWindowsOption::DecentsAndWayOffs,
                [false, false, false, true, true],
            ),
            (
                TimingWindowsOption::FantasticsAndExcellents,
                [true, true, false, false, false],
            ),
        ] {
            assert_eq!(
                setting.to_string().parse::<TimingWindowsOption>(),
                Ok(setting)
            );
            assert_eq!(setting.disabled_windows(), disabled);
        }
        assert_eq!(
            TimingWindowsOption::from_str("decents and way offs"),
            Ok(TimingWindowsOption::DecentsAndWayOffs)
        );
        assert_eq!(
            TimingWindowsOption::from_str("fantastics+excellents"),
            Ok(TimingWindowsOption::FantasticsAndExcellents)
        );
        assert!(TimingWindowsOption::from_str("misses").is_err());
    }

    #[test]
    fn step_statistics_mask_round_trips_and_accepts_legacy_aliases() {
        let mask = StepStatisticsMask::DENSITY_GRAPH
            | StepStatisticsMask::SONG_BANNER
            | StepStatisticsMask::JUDGMENT_COUNTER
            | StepStatisticsMask::STEP_COUNTS;

        assert_eq!(mask.to_string().parse::<StepStatisticsMask>(), Ok(mask));
        assert_eq!(
            StepStatisticsMask::from_str("target"),
            Ok(StepStatisticsMask::empty())
        );
        assert_eq!(
            StepStatisticsMask::from_str("stepstats"),
            Ok(StepStatisticsMask::all_widgets())
        );
        assert_eq!(
            StepStatisticsMask::from_str("Judgements Counter, Peak NPS"),
            Ok(StepStatisticsMask::JUDGMENT_COUNTER | StepStatisticsMask::PEAK_NPS)
        );
        assert_eq!(
            StepStatisticsMask::from_str("Judgements, Pack Info"),
            Ok(StepStatisticsMask::JUDGMENT_COUNTER | StepStatisticsMask::PACK_BANNER)
        );
        assert_eq!(
            StepStatisticsMask::from_str("Song Info, Pack Banner"),
            Ok(StepStatisticsMask::PACK_BANNER)
        );
        assert_eq!(
            StepStatisticsMask::from_str("Step Counts, GS Box"),
            Ok(StepStatisticsMask::STEP_COUNTS)
        );
        assert!(StepStatisticsMask::from_str("lanes").is_err());
    }

    #[test]
    fn step_stats_extra_round_trips_and_accepts_arrow_cloud_names() {
        for setting in [
            StepStatsExtra::None,
            StepStatsExtra::ErrorStats,
            StepStatsExtra::AmongUs,
            StepStatsExtra::BrodyQuest,
            StepStatsExtra::CatJAM,
            StepStatsExtra::CrabPls,
            StepStatsExtra::DancingDuck,
            StepStatsExtra::DonChan,
            StepStatsExtra::NyanCat,
            StepStatsExtra::Randomizer,
            StepStatsExtra::RinCat,
            StepStatsExtra::Snoop,
            StepStatsExtra::Sonic,
        ] {
            assert_eq!(setting.to_string().parse::<StepStatsExtra>(), Ok(setting));
        }
        assert_eq!(
            StepStatsExtra::from_str("DancingDuck"),
            Ok(StepStatsExtra::DancingDuck)
        );
        assert_eq!(
            StepStatsExtra::from_str("NyanCat"),
            Ok(StepStatsExtra::NyanCat)
        );
        assert_eq!(
            StepStatsExtra::from_str("RinCat"),
            Ok(StepStatsExtra::RinCat)
        );
        assert!(StepStatsExtra::from_str("lanes").is_err());
    }

    #[test]
    fn measure_counter_round_trips_and_reports_stream_thresholds() {
        for (setting, threshold, multiplier) in [
            (MeasureCounter::None, None, 1.0),
            (MeasureCounter::Eighth, Some(8), 1.0),
            (MeasureCounter::Twelfth, Some(12), 1.0),
            (MeasureCounter::Sixteenth, Some(16), 1.0),
            (MeasureCounter::TwentyFourth, Some(24), 1.5),
            (MeasureCounter::ThirtySecond, Some(32), 2.0),
        ] {
            assert_eq!(setting.to_string().parse::<MeasureCounter>(), Ok(setting));
            assert_eq!(setting.notes_threshold(), threshold);
            assert_eq!(setting.multiplier(), multiplier);
        }
        assert!(MeasureCounter::from_str("quarter").is_err());
    }

    #[test]
    fn measure_lines_round_trips() {
        for setting in [
            MeasureLines::Off,
            MeasureLines::Measure,
            MeasureLines::Quarter,
            MeasureLines::Eighth,
        ] {
            assert_eq!(setting.to_string().parse::<MeasureLines>(), Ok(setting));
        }
        assert!(MeasureLines::from_str("sixteenth").is_err());
    }

    #[test]
    fn mini_indicator_round_trips_and_accepts_aliases() {
        for setting in [
            MiniIndicator::None,
            MiniIndicator::SubtractiveScoring,
            MiniIndicator::PredictiveScoring,
            MiniIndicator::PaceScoring,
            MiniIndicator::RivalScoring,
            MiniIndicator::Pacemaker,
            MiniIndicator::StreamProg,
        ] {
            assert_eq!(setting.to_string().parse::<MiniIndicator>(), Ok(setting));
        }
        assert_eq!(
            MiniIndicator::from_str("subtractive"),
            Ok(MiniIndicator::SubtractiveScoring)
        );
        assert_eq!(
            MiniIndicator::from_str("stream progress"),
            Ok(MiniIndicator::StreamProg)
        );
        assert!(MiniIndicator::from_str("combo").is_err());
    }

    #[test]
    fn mini_indicator_score_type_round_trips_and_accepts_hex_alias() {
        for setting in [
            MiniIndicatorScoreType::Itg,
            MiniIndicatorScoreType::Ex,
            MiniIndicatorScoreType::HardEx,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorScoreType>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorScoreType::from_str("hex"),
            Ok(MiniIndicatorScoreType::HardEx)
        );
        assert!(MiniIndicatorScoreType::from_str("percent").is_err());
    }

    #[test]
    fn mini_indicator_size_round_trips_and_accepts_big_alias() {
        for setting in [MiniIndicatorSize::Default, MiniIndicatorSize::Large] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorSize>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorSize::from_str("big"),
            Ok(MiniIndicatorSize::Large)
        );
        assert!(MiniIndicatorSize::from_str("small").is_err());
    }

    #[test]
    fn mini_indicator_color_round_trips() {
        for setting in [
            MiniIndicatorColor::Default,
            MiniIndicatorColor::Detailed,
            MiniIndicatorColor::Combo,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorColor>(),
                Ok(setting)
            );
        }
        assert!(MiniIndicatorColor::from_str("rainbow").is_err());
    }

    #[test]
    fn mini_indicator_subtractive_display_round_trips() {
        for setting in [
            MiniIndicatorSubtractiveDisplay::Percent,
            MiniIndicatorSubtractiveDisplay::Points,
        ] {
            assert_eq!(
                setting
                    .to_string()
                    .parse::<MiniIndicatorSubtractiveDisplay>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorSubtractiveDisplay::from_str("dance points"),
            Ok(MiniIndicatorSubtractiveDisplay::Points)
        );
        assert!(MiniIndicatorSubtractiveDisplay::from_str("combo").is_err());
    }

    #[test]
    fn mini_indicator_position_round_trips() {
        for setting in [
            MiniIndicatorPosition::Default,
            MiniIndicatorPosition::UnderUpArrow,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorPosition>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorPosition::from_str("under up arrow"),
            Ok(MiniIndicatorPosition::UnderUpArrow)
        );
        assert!(MiniIndicatorPosition::from_str("score").is_err());
    }

    #[test]
    fn background_filter_default_matches_legacy_darkest_value() {
        assert_eq!(BackgroundFilter::default(), BackgroundFilter::DEFAULT);
        assert_eq!(BackgroundFilter::default().percent(), 95);
    }

    #[test]
    fn background_filter_from_percent_clamps_above_max() {
        assert_eq!(BackgroundFilter::from_percent(200).percent(), 100);
        assert_eq!(BackgroundFilter::from_i32(-5).percent(), 0);
        assert_eq!(BackgroundFilter::from_i32(250).percent(), 100);
    }

    #[test]
    fn background_filter_alpha_maps_percent_to_unit_range() {
        assert!((BackgroundFilter::from_percent(0).alpha() - 0.0).abs() < 1e-6);
        assert!((BackgroundFilter::from_percent(100).alpha() - 1.0).abs() < 1e-6);
        assert!((BackgroundFilter::from_percent(50).alpha() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn background_filter_migrates_legacy_enum_labels() {
        assert_eq!(
            BackgroundFilter::from_str("Off").unwrap(),
            BackgroundFilter::OFF
        );
        assert_eq!(
            BackgroundFilter::from_str("Dark").unwrap(),
            BackgroundFilter::from_percent(50)
        );
        assert_eq!(
            BackgroundFilter::from_str("DARKER").unwrap(),
            BackgroundFilter::from_percent(75)
        );
        assert_eq!(
            BackgroundFilter::from_str("darkest").unwrap(),
            BackgroundFilter::from_percent(95)
        );
    }

    #[test]
    fn background_filter_parses_numeric_with_optional_percent_suffix() {
        assert_eq!(
            BackgroundFilter::from_str("0").unwrap(),
            BackgroundFilter::OFF
        );
        assert_eq!(
            BackgroundFilter::from_str("42").unwrap(),
            BackgroundFilter::from_percent(42)
        );
        assert_eq!(
            BackgroundFilter::from_str("42%").unwrap(),
            BackgroundFilter::from_percent(42)
        );
        assert_eq!(
            BackgroundFilter::from_str("100").unwrap(),
            BackgroundFilter::from_percent(100)
        );
    }

    #[test]
    fn background_filter_rejects_out_of_range_or_garbage() {
        assert!(BackgroundFilter::from_str("101").is_err());
        assert!(BackgroundFilter::from_str("-1").is_err());
        assert!(BackgroundFilter::from_str("Dimmer").is_err());
        assert!(BackgroundFilter::from_str("").is_err());
    }

    #[test]
    fn background_filter_display_round_trips_through_from_str() {
        for v in [0u8, 1, 25, 50, 75, 95, 100] {
            let filter = BackgroundFilter::from_percent(v);
            let s = filter.to_string();
            let parsed = BackgroundFilter::from_str(&s).expect("must round-trip");
            assert_eq!(parsed, filter);
        }
    }

    #[test]
    fn noteskin_normalizes_names_and_preserves_none_choice() {
        assert_eq!(NoteSkin::default().as_str(), NoteSkin::CEL_NAME);
        assert_eq!(NoteSkin::new(" Default ").as_str(), NoteSkin::DEFAULT_NAME);
        assert_eq!(NoteSkin::none_choice().as_str(), NoteSkin::NONE_NAME);
        assert!(NoteSkin::from_str("").is_err());
    }

    #[test]
    fn noteskin_resolution_uses_override_or_fallback() {
        let fallback = NoteSkin::new("metal");
        let override_skin = NoteSkin::new("cyber");

        assert_eq!(resolve_noteskin_choice(None, &fallback), &fallback);
        assert_eq!(
            resolve_noteskin_choice(Some(&override_skin), &fallback),
            &override_skin
        );
    }

    #[test]
    fn tap_explosion_skin_resolution_hides_none_choice() {
        let fallback = NoteSkin::new("metal");
        let override_skin = NoteSkin::new("cyber");
        let hidden = NoteSkin::none_choice();

        assert!(!tap_explosion_skin_hidden(None));
        assert!(!tap_explosion_skin_hidden(Some(&override_skin)));
        assert!(tap_explosion_skin_hidden(Some(&hidden)));
        assert_eq!(resolve_tap_explosion_skin(None, &fallback), Some(&fallback));
        assert_eq!(
            resolve_tap_explosion_skin(Some(&override_skin), &fallback),
            Some(&override_skin)
        );
        assert_eq!(resolve_tap_explosion_skin(Some(&hidden), &fallback), None);
    }

    #[test]
    fn graphic_settings_normalize_stock_aliases_and_none() {
        assert_eq!(
            JudgmentGraphic::new("Wendy").as_str(),
            "judgements/Wendy 2x7 (doubleres).png"
        );
        assert_eq!(
            HoldJudgmentGraphic::new("itg2").as_str(),
            "hold_judgements/ITG2 1x2 (doubleres).png"
        );
        assert_eq!(HeldMissGraphic::new("none").as_str(), "None");
        assert_eq!(
            JudgmentGraphic::from_str("custom.png").unwrap().as_str(),
            "judgements/custom.png"
        );
        assert!(HoldJudgmentGraphic::from_str("").is_err());
    }
}
