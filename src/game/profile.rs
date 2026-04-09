pub use super::scroll::ScrollSpeedSetting;
use crate::config::{self, SimpleIni, dirs};
use bincode::{Decode, Encode};
use chrono::{Datelike, Local};
use log::{debug, info, warn};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

mod update;

pub use update::*;

pub const DEFAULT_WEIGHT_POUNDS: i32 = 120;
pub const DEFAULT_BIRTH_YEAR: i32 = 1995;
// Shared player-option HUD offset range, in logical pixels.
pub const HUD_OFFSET_MIN: i32 = -250;
pub const HUD_OFFSET_MAX: i32 = 250;

#[inline(always)]
const fn clamp_weight_pounds(weight_pounds: i32) -> i32 {
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
        // Support both legacy single values ("Reverse") and combined values
        // like "Reverse+Cross" or "Reverse Cross".
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
            // "Normal" means no flags; combining it with others is treated as just the others.
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

pub const INSERT_ACTIVE_BITS: u8 = (1 << 7) - 1;
pub const REMOVE_ACTIVE_BITS: u8 = u8::MAX;
pub const HOLDS_ACTIVE_BITS: u8 = (1 << 5) - 1;
pub const ACCEL_EFFECTS_ACTIVE_BITS: u8 = (1 << 5) - 1;
pub const VISUAL_EFFECTS_ACTIVE_BITS: u16 = (1 << 10) - 1;
pub const APPEARANCE_EFFECTS_ACTIVE_BITS: u8 = (1 << 5) - 1;

#[inline(always)]
pub const fn normalize_insert_mask(mask: u8) -> u8 {
    mask & INSERT_ACTIVE_BITS
}

#[inline(always)]
pub const fn normalize_remove_mask(mask: u8) -> u8 {
    mask & REMOVE_ACTIVE_BITS
}

#[inline(always)]
pub const fn normalize_holds_mask(mask: u8) -> u8 {
    mask & HOLDS_ACTIVE_BITS
}

#[inline(always)]
pub const fn normalize_accel_effects_mask(mask: u8) -> u8 {
    mask & ACCEL_EFFECTS_ACTIVE_BITS
}

#[inline(always)]
pub const fn normalize_visual_effects_mask(mask: u16) -> u16 {
    mask & VISUAL_EFFECTS_ACTIVE_BITS
}

#[inline(always)]
pub const fn normalize_appearance_effects_mask(mask: u8) -> u8 {
    mask & APPEARANCE_EFFECTS_ACTIVE_BITS
}

// --- Profile Data ---
const DEFAULT_PROFILE_ID: &str = "00000000";
const PROFILE_STATS_VERSION_V1: u16 = 1;

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

fn parse_groovestats_is_pad_player(value: Option<String>, default: bool) -> bool {
    value
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default, |v| v == 1)
}

#[inline(always)]
fn arrowcloud_ini_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("arrowcloud.ini")
}

#[inline(always)]
fn find_profile_avatar_path(dir: &Path) -> Option<PathBuf> {
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

#[inline(always)]
fn profile_stats_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("stats.bin")
}

#[inline(always)]
fn parse_last_played_value(value: Option<String>) -> Option<String> {
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
fn write_player_options(content: &mut String, section: &str, options: &PlayerOptionsData) {
    content.push_str(&format!("[{section}]\n"));
    content.push_str(&format!("BackgroundFilter={}\n", options.background_filter));
    content.push_str(&format!("ScrollSpeed={}\n", options.scroll_speed));
    content.push_str(&format!("Scroll={}\n", options.scroll_option));
    content.push_str(&format!("Turn={}\n", options.turn_option));
    content.push_str(&format!("InsertMask={}\n", options.insert_active_mask));
    content.push_str(&format!("RemoveMask={}\n", options.remove_active_mask));
    content.push_str(&format!("HoldsMask={}\n", options.holds_active_mask));
    content.push_str(&format!(
        "AccelEffectsMask={}\n",
        options.accel_effects_active_mask
    ));
    content.push_str(&format!(
        "VisualEffectsMask={}\n",
        options.visual_effects_active_mask
    ));
    content.push_str(&format!(
        "AppearanceEffectsMask={}\n",
        options.appearance_effects_active_mask
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
        "ColumnFlashOnMiss={}\n",
        i32::from(options.column_flash_on_miss)
    ));
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
    content.push_str(&format!("MiniIndicator={}\n", options.mini_indicator));
    content.push_str(&format!(
        "MiniIndicatorScoreType={}\n",
        options.mini_indicator_score_type
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
        "CustomFantasticWindow={}\n",
        i32::from(options.custom_fantastic_window)
    ));
    content.push_str(&format!(
        "CustomFantasticWindowMs={}\n",
        options.custom_fantastic_window_ms
    ));
    content.push_str(&format!(
        "JudgmentTilt={}\n",
        i32::from(options.judgment_tilt)
    ));
    content.push_str(&format!("ColumnCues={}\n", i32::from(options.column_cues)));
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
    content.push_str(&format!("ErrorBar={}\n", options.error_bar));
    content.push_str(&format!(
        "ErrorBarText={}\n",
        i32::from(options.error_bar_text)
    ));
    content.push_str(&format!("ErrorBarMask={}\n", options.error_bar_active_mask));
    content.push_str(&format!(
        "Colorful={}\n",
        i32::from((options.error_bar_active_mask & ERROR_BAR_BIT_COLORFUL) != 0)
    ));
    content.push_str(&format!(
        "Monochrome={}\n",
        i32::from((options.error_bar_active_mask & ERROR_BAR_BIT_MONOCHROME) != 0)
    ));
    content.push_str(&format!(
        "Text={}\n",
        i32::from((options.error_bar_active_mask & ERROR_BAR_BIT_TEXT) != 0)
    ));
    content.push_str(&format!(
        "Highlight={}\n",
        i32::from((options.error_bar_active_mask & ERROR_BAR_BIT_HIGHLIGHT) != 0)
    ));
    content.push_str(&format!(
        "Average={}\n",
        i32::from((options.error_bar_active_mask & ERROR_BAR_BIT_AVERAGE) != 0)
    ));
    content.push_str(&format!("ErrorBarUp={}\n", i32::from(options.error_bar_up)));
    content.push_str(&format!(
        "ErrorBarMultiTick={}\n",
        i32::from(options.error_bar_multi_tick)
    ));
    content.push_str(&format!("ErrorBarTrim={}\n", options.error_bar_trim));
    content.push_str(&format!(
        "DataVisualizations={}\n",
        options.data_visualizations
    ));
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
    content.push_str(&format!("MiniPercent={}\n", options.mini_percent));
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
    options.background_filter = profile_conf
        .get(section, "BackgroundFilter")
        .and_then(|s| BackgroundFilter::from_str(&s).ok())
        .unwrap_or(options.background_filter);
    options.hold_judgment_graphic = profile_conf
        .get(section, "HoldJudgmentGraphic")
        .and_then(|s| HoldJudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.hold_judgment_graphic.clone());
    options.judgment_graphic = profile_conf
        .get(section, "JudgmentGraphic")
        .and_then(|s| JudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.judgment_graphic.clone());
    options.combo_font = profile_conf
        .get(section, "ComboFont")
        .and_then(|s| ComboFont::from_str(&s).ok())
        .unwrap_or(options.combo_font);
    options.combo_colors = profile_conf
        .get(section, "ComboColors")
        .and_then(|s| ComboColors::from_str(&s).ok())
        .unwrap_or(options.combo_colors);
    options.combo_mode = profile_conf
        .get(section, "ComboMode")
        .and_then(|s| ComboMode::from_str(&s).ok())
        .unwrap_or(options.combo_mode);
    options.carry_combo_between_songs = profile_conf
        .get(section, "CarryComboBetweenSongs")
        .or_else(|| profile_conf.get(section, "ComboContinuesBetweenSongs"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.carry_combo_between_songs, |v| v != 0);
    options.noteskin = profile_conf
        .get(section, "NoteSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok())
        .unwrap_or_else(|| options.noteskin.clone());
    options.mine_noteskin = profile_conf
        .get(section, "MineSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok());
    options.receptor_noteskin = profile_conf
        .get(section, "ReceptorSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok());
    options.tap_explosion_noteskin = profile_conf
        .get(section, "TapExplosionSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok());
    options.mini_percent = profile_conf
        .get(section, "MiniPercent")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.mini_percent);
    options.perspective = profile_conf
        .get(section, "Perspective")
        .and_then(|s| Perspective::from_str(&s).ok())
        .unwrap_or(options.perspective);
    options.note_field_offset_x = profile_conf
        .get(section, "NoteFieldOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_x);
    options.note_field_offset_y = profile_conf
        .get(section, "NoteFieldOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_y);
    options.judgment_offset_x = profile_conf
        .get(section, "JudgmentOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_x);
    options.judgment_offset_y = profile_conf
        .get(section, "JudgmentOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_y);
    options.combo_offset_x = profile_conf
        .get(section, "ComboOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_x);
    options.combo_offset_y = profile_conf
        .get(section, "ComboOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_y);
    options.error_bar_offset_x = profile_conf
        .get(section, "ErrorBarOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_x);
    options.error_bar_offset_y = profile_conf
        .get(section, "ErrorBarOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_y);
    options.visual_delay_ms = profile_conf
        .get(section, "VisualDelayMs")
        .or_else(|| profile_conf.get(section, "VisualDelay"))
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.visual_delay_ms);
    options.global_offset_shift_ms = profile_conf
        .get(section, "GlobalOffsetShiftMs")
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.global_offset_shift_ms);
    options.show_fa_plus_window = profile_conf
        .get(section, "ShowFaPlusWindow")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.show_fa_plus_window, |v| v != 0);
    options.show_ex_score = profile_conf
        .get(section, "ShowExScore")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.show_ex_score, |v| v != 0);
    options.show_hard_ex_score = profile_conf
        .get(section, "ShowHardEXScore")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.show_hard_ex_score, |v| v != 0);
    options.show_fa_plus_pane = profile_conf
        .get(section, "ShowFaPlusPane")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.show_fa_plus_pane, |v| v != 0);
    options.fa_plus_10ms_blue_window = profile_conf
        .get(section, "SmallerWhite")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.fa_plus_10ms_blue_window, |v| v != 0);
    options.split_15_10ms = profile_conf
        .get(section, "SplitWhites")
        .or_else(|| profile_conf.get(section, "Split1510ms"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.split_15_10ms, |v| v != 0);
    options.track_early_judgments = profile_conf
        .get(section, "TrackEarlyJudgments")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.track_early_judgments, |v| v != 0);
    options.custom_fantastic_window = profile_conf
        .get(section, "CustomFantasticWindow")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.custom_fantastic_window, |v| v != 0);
    options.custom_fantastic_window_ms = profile_conf
        .get(section, "CustomFantasticWindowMs")
        .and_then(|s| s.parse::<u8>().ok())
        .map(clamp_custom_fantastic_window_ms)
        .unwrap_or(options.custom_fantastic_window_ms);
    options.judgment_tilt = profile_conf
        .get(section, "JudgmentTilt")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.judgment_tilt, |v| v != 0);
    options.column_cues = profile_conf
        .get(section, "ColumnCues")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.column_cues, |v| v != 0);
    options.judgment_back = profile_conf
        .get(section, "JudgmentBack")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.judgment_back, |v| v != 0);
    options.error_ms_display = profile_conf
        .get(section, "ErrorMSDisplay")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_ms_display, |v| v != 0);
    options.display_scorebox = profile_conf
        .get(section, "DisplayScorebox")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.display_scorebox, |v| v != 0);
    options.rainbow_max = profile_conf
        .get(section, "RainbowMax")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.rainbow_max, |v| v != 0);
    options.responsive_colors = profile_conf
        .get(section, "ResponsiveColors")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.responsive_colors, |v| v != 0);
    options.show_life_percent = profile_conf
        .get(section, "ShowLifePercent")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.show_life_percent, |v| v != 0);
    options.tilt_multiplier = profile_conf
        .get(section, "TiltMultiplier")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(options.tilt_multiplier);
    options.error_bar = profile_conf
        .get(section, "ErrorBar")
        .and_then(|s| ErrorBarStyle::from_str(&s).ok())
        .unwrap_or(options.error_bar);
    options.error_bar_text = profile_conf
        .get(section, "ErrorBarText")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_text, |v| v != 0);
    let mask_from_key = profile_conf
        .get(section, "ErrorBarMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(normalize_error_bar_mask);
    let colorful = profile_conf
        .get(section, "Colorful")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let monochrome = profile_conf
        .get(section, "Monochrome")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let text = profile_conf
        .get(section, "Text")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let highlight = profile_conf
        .get(section, "Highlight")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let average = profile_conf
        .get(section, "Average")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let mask_from_flags = if colorful.is_some()
        || monochrome.is_some()
        || text.is_some()
        || highlight.is_some()
        || average.is_some()
    {
        let mut mask: u8 = 0;
        if colorful.unwrap_or(false) {
            mask |= ERROR_BAR_BIT_COLORFUL;
        }
        if monochrome.unwrap_or(false) {
            mask |= ERROR_BAR_BIT_MONOCHROME;
        }
        if text.unwrap_or(false) {
            mask |= ERROR_BAR_BIT_TEXT;
        }
        if highlight.unwrap_or(false) {
            mask |= ERROR_BAR_BIT_HIGHLIGHT;
        }
        if average.unwrap_or(false) {
            mask |= ERROR_BAR_BIT_AVERAGE;
        }
        Some(normalize_error_bar_mask(mask))
    } else {
        None
    };
    options.error_bar_active_mask = mask_from_key
        .or(mask_from_flags)
        .unwrap_or_else(|| error_bar_mask_from_style(options.error_bar, options.error_bar_text));
    options.error_bar = error_bar_style_from_mask(options.error_bar_active_mask);
    options.error_bar_text = error_bar_text_from_mask(options.error_bar_active_mask);
    options.error_bar_up = profile_conf
        .get(section, "ErrorBarUp")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_up, |v| v != 0);
    options.error_bar_multi_tick = profile_conf
        .get(section, "ErrorBarMultiTick")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_multi_tick, |v| v != 0);
    options.error_bar_trim = profile_conf
        .get(section, "ErrorBarTrim")
        .and_then(|s| ErrorBarTrim::from_str(&s).ok())
        .unwrap_or(options.error_bar_trim);
    options.data_visualizations = profile_conf
        .get(section, "DataVisualizations")
        .and_then(|s| DataVisualizations::from_str(&s).ok())
        .unwrap_or(options.data_visualizations);
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
    options.turn_option = profile_conf
        .get(section, "Turn")
        .and_then(|s| TurnOption::from_str(&s).ok())
        .unwrap_or(options.turn_option);
    options.insert_active_mask = profile_conf
        .get(section, "InsertMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(normalize_insert_mask)
        .unwrap_or(options.insert_active_mask);
    options.remove_active_mask = profile_conf
        .get(section, "RemoveMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(normalize_remove_mask)
        .unwrap_or(options.remove_active_mask);
    options.holds_active_mask = profile_conf
        .get(section, "HoldsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(normalize_holds_mask)
        .unwrap_or(options.holds_active_mask);
    options.accel_effects_active_mask = profile_conf
        .get(section, "AccelEffectsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(normalize_accel_effects_mask)
        .unwrap_or(options.accel_effects_active_mask);
    options.visual_effects_active_mask = profile_conf
        .get(section, "VisualEffectsMask")
        .and_then(|s| s.parse::<u16>().ok())
        .map(normalize_visual_effects_mask)
        .unwrap_or(options.visual_effects_active_mask);
    options.appearance_effects_active_mask = profile_conf
        .get(section, "AppearanceEffectsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(normalize_appearance_effects_mask)
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
    options.column_flash_on_miss = profile_conf
        .get(section, "ColumnFlashOnMiss")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.column_flash_on_miss, |v| v != 0);
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
    if !has_any {
        return None;
    }

    Some(LastPlayed {
        song_music_path: parse_last_played_value(profile_conf.get(section, "MusicPath")),
        chart_hash: parse_last_played_value(profile_conf.get(section, "ChartHash")),
        difficulty_index: profile_conf
            .get(section, "DifficultyIndex")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default.difficulty_index),
    })
}

#[inline(always)]
fn write_last_played(content: &mut String, section: &str, last_played: &LastPlayed) {
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

#[inline(always)]
fn profile_stats_tmp_path(id: &str) -> PathBuf {
    local_profile_dir(id).join("stats.bin.tmp")
}

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

#[derive(Debug, Clone, Default)]
struct ProfileStats {
    current_combo: u32,
    known_pack_names: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundFilter {
    Off,
    Dark,
    Darker,
    #[default]
    Darkest,
}

impl FromStr for BackgroundFilter {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "dark" => Ok(Self::Dark),
            "darker" => Ok(Self::Darker),
            "darkest" => Ok(Self::Darkest),
            _ => Err(format!("'{s}' is not a valid BackgroundFilter setting")),
        }
    }
}

impl core::fmt::Display for BackgroundFilter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Dark => write!(f, "Dark"),
            Self::Darker => write!(f, "Darker"),
            Self::Darkest => write!(f, "Darkest"),
        }
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

pub const ERROR_BAR_BIT_COLORFUL: u8 = 1 << 0;
pub const ERROR_BAR_BIT_MONOCHROME: u8 = 1 << 1;
pub const ERROR_BAR_BIT_TEXT: u8 = 1 << 2;
pub const ERROR_BAR_BIT_HIGHLIGHT: u8 = 1 << 3;
pub const ERROR_BAR_BIT_AVERAGE: u8 = 1 << 4;
pub const ERROR_BAR_ACTIVE_BITS: u8 = ERROR_BAR_BIT_COLORFUL
    | ERROR_BAR_BIT_MONOCHROME
    | ERROR_BAR_BIT_TEXT
    | ERROR_BAR_BIT_HIGHLIGHT
    | ERROR_BAR_BIT_AVERAGE;

#[inline(always)]
pub const fn normalize_error_bar_mask(mask: u8) -> u8 {
    mask & ERROR_BAR_ACTIVE_BITS
}

#[inline(always)]
pub const fn error_bar_mask_from_style(style: ErrorBarStyle, text: bool) -> u8 {
    let mut mask = if text { ERROR_BAR_BIT_TEXT } else { 0 };
    mask |= match style {
        ErrorBarStyle::None => 0,
        ErrorBarStyle::Colorful => ERROR_BAR_BIT_COLORFUL,
        ErrorBarStyle::Monochrome => ERROR_BAR_BIT_MONOCHROME,
        ErrorBarStyle::Text => ERROR_BAR_BIT_TEXT,
        ErrorBarStyle::Highlight => ERROR_BAR_BIT_HIGHLIGHT,
        ErrorBarStyle::Average => ERROR_BAR_BIT_AVERAGE,
    };
    normalize_error_bar_mask(mask)
}

#[inline(always)]
pub const fn error_bar_style_from_mask(mask: u8) -> ErrorBarStyle {
    let mask = normalize_error_bar_mask(mask);
    if (mask & ERROR_BAR_BIT_COLORFUL) != 0 {
        ErrorBarStyle::Colorful
    } else if (mask & ERROR_BAR_BIT_MONOCHROME) != 0 {
        ErrorBarStyle::Monochrome
    } else if (mask & ERROR_BAR_BIT_HIGHLIGHT) != 0 {
        ErrorBarStyle::Highlight
    } else if (mask & ERROR_BAR_BIT_AVERAGE) != 0 {
        ErrorBarStyle::Average
    } else {
        ErrorBarStyle::None
    }
}

#[inline(always)]
pub const fn error_bar_text_from_mask(mask: u8) -> bool {
    (normalize_error_bar_mask(mask) & ERROR_BAR_BIT_TEXT) != 0
}

pub const CUSTOM_FANTASTIC_WINDOW_MIN_MS: u8 = 1;
pub const CUSTOM_FANTASTIC_WINDOW_MAX_MS: u8 = 22;
pub const CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS: u8 = 10;

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
pub enum DataVisualizations {
    #[default]
    None,
    TargetScoreGraph,
    StepStatistics,
}

impl FromStr for DataVisualizations {
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
            "targetscoregraph" | "targetscore" | "target" => Ok(Self::TargetScoreGraph),
            "stepstatistics" | "stepstats" => Ok(Self::StepStatistics),
            other => Err(format!(
                "'{other}' is not a valid DataVisualizations setting"
            )),
        }
    }
}

impl core::fmt::Display for DataVisualizations {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::TargetScoreGraph => write!(f, "Target Score Graph"),
            Self::StepStatistics => write!(f, "Step Statistics"),
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
        let lower = trimmed.to_ascii_lowercase();
        Some(lower)
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
            Self::None => write!(f, "None"),
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

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerOptionsData {
    pub background_filter: BackgroundFilter,
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub noteskin: NoteSkin,
    pub mine_noteskin: Option<NoteSkin>,
    pub receptor_noteskin: Option<NoteSkin>,
    pub tap_explosion_noteskin: Option<NoteSkin>,
    pub scroll_speed: ScrollSpeedSetting,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    pub insert_active_mask: u8,
    pub remove_active_mask: u8,
    pub holds_active_mask: u8,
    pub accel_effects_active_mask: u8,
    pub visual_effects_active_mask: u16,
    pub appearance_effects_active_mask: u8,
    pub attack_mode: AttackMode,
    pub hide_light_type: HideLightType,
    pub rescore_early_hits: bool,
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
    pub timing_windows: TimingWindowsOption,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub track_early_judgments: bool,
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    pub judgment_tilt: bool,
    pub column_cues: bool,
    pub judgment_back: bool,
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    pub rainbow_max: bool,
    pub responsive_colors: bool,
    pub show_life_percent: bool,
    pub tilt_multiplier: f32,
    pub error_bar_active_mask: u8,
    pub error_bar: ErrorBarStyle,
    pub error_bar_text: bool,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub data_visualizations: DataVisualizations,
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
    pub column_flash_on_miss: bool,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub transparent_density_graph_bg: bool,
    pub mini_indicator: MiniIndicator,
    pub mini_indicator_score_type: MiniIndicatorScoreType,
    pub mini_percent: i32,
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
        judgment_graphic: JudgmentGraphic::default(),
        combo_font: ComboFont::default(),
        combo_colors: ComboColors::default(),
        combo_mode: ComboMode::default(),
        carry_combo_between_songs: true,
        noteskin: NoteSkin::default(),
        mine_noteskin: None,
        receptor_noteskin: None,
        tap_explosion_noteskin: None,
        scroll_speed: ScrollSpeedSetting::default(),
        scroll_option: ScrollOption::default(),
        reverse_scroll: false,
        turn_option: TurnOption::default(),
        insert_active_mask: 0,
        remove_active_mask: 0,
        holds_active_mask: 0,
        accel_effects_active_mask: 0,
        visual_effects_active_mask: 0,
        appearance_effects_active_mask: 0,
        attack_mode: AttackMode::default(),
        hide_light_type: HideLightType::default(),
        rescore_early_hits: true,
        hide_early_dw_judgments: false,
        hide_early_dw_flash: false,
        timing_windows: TimingWindowsOption::default(),
        show_fa_plus_window: false,
        show_ex_score: false,
        show_hard_ex_score: false,
        show_fa_plus_pane: false,
        fa_plus_10ms_blue_window: false,
        split_15_10ms: false,
        track_early_judgments: false,
        custom_fantastic_window: false,
        custom_fantastic_window_ms: CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS,
        judgment_tilt: false,
        column_cues: false,
        judgment_back: false,
        error_ms_display: false,
        display_scorebox: true,
        rainbow_max: false,
        responsive_colors: false,
        show_life_percent: false,
        tilt_multiplier: 1.0,
        error_bar_active_mask: error_bar_mask_from_style(ErrorBarStyle::default(), false),
        error_bar: ErrorBarStyle::default(),
        error_bar_text: false,
        error_bar_up: false,
        error_bar_multi_tick: false,
        error_bar_trim: ErrorBarTrim::default(),
        data_visualizations: DataVisualizations::default(),
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
        column_flash_on_miss: false,
        subtractive_scoring: false,
        pacemaker: false,
        nps_graph_at_top: false,
        transparent_density_graph_bg: false,
        mini_indicator: MiniIndicator::None,
        mini_indicator_score_type: MiniIndicatorScoreType::Itg,
        mini_percent: 0,
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
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub current_combo: u32,
    pub known_pack_names: HashSet<String>,
    pub noteskin: NoteSkin,
    pub mine_noteskin: Option<NoteSkin>,
    pub receptor_noteskin: Option<NoteSkin>,
    pub tap_explosion_noteskin: Option<NoteSkin>,
    pub avatar_path: Option<PathBuf>,
    pub avatar_texture_key: Option<String>,
    pub scroll_speed: ScrollSpeedSetting,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    // zmod uncommon modifiers (ScreenPlayerOptions3).
    // Bit order mirrors row choice order in metrics.ini.
    pub insert_active_mask: u8,
    pub remove_active_mask: u8,
    pub holds_active_mask: u8,
    pub accel_effects_active_mask: u8,
    pub visual_effects_active_mask: u16,
    pub appearance_effects_active_mask: u8,
    pub attack_mode: AttackMode,
    pub hide_light_type: HideLightType,
    // Allow early Decent/WayOff hits to be rescored to better judgments.
    pub rescore_early_hits: bool,
    // Visual behavior for early Decent/Way Off hits (Simply Love semantics).
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
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
    // Custom blue Fantastic window in milliseconds (1..22), shared by FA+ W0 and H.EX split.
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    // Judgment tilt (Simply Love semantics).
    pub judgment_tilt: bool,
    pub column_cues: bool,
    // zmod ExtraAesthetics: draw judgments/error timing HUD behind notes.
    pub judgment_back: bool,
    // zmod ExtraAesthetics: offset indicator (ErrorMSDisplay).
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    // zmod LifeBarOptions (Arrow Cloud semantics).
    pub rainbow_max: bool,
    pub responsive_colors: bool,
    pub show_life_percent: bool,
    pub tilt_multiplier: f32,
    // Error bar (zmod semantics): each bit toggles one submodule in the
    // SelectMultiple row (Colorful/Monochrome/Text/Highlight/Average).
    pub error_bar_active_mask: u8,
    // Backward-compatible primary style string written to profile.ini.
    pub error_bar: ErrorBarStyle,
    // Backward-compatible text flag written to profile.ini.
    pub error_bar_text: bool,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub data_visualizations: DataVisualizations,
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
    // Gameplay extras (Simply Love semantics).
    pub column_flash_on_miss: bool,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub transparent_density_graph_bg: bool,
    pub mini_indicator: MiniIndicator,
    pub mini_indicator_score_type: MiniIndicatorScoreType,
    // Mini modifier as a percentage, mirroring Simply Love semantics.
    // 0 = normal size, 100 = 100% Mini (smaller), negative values enlarge.
    pub mini_percent: i32,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPlayed {
    pub song_music_path: Option<String>,
    pub chart_hash: Option<String>,
    pub difficulty_index: usize,
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

impl Default for Profile {
    fn default() -> Self {
        let player_options = default_player_options();
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
            judgment_graphic: player_options.judgment_graphic.clone(),
            combo_font: player_options.combo_font,
            combo_colors: player_options.combo_colors,
            combo_mode: player_options.combo_mode,
            carry_combo_between_songs: player_options.carry_combo_between_songs,
            current_combo: 0,
            known_pack_names: HashSet::new(),
            noteskin: player_options.noteskin.clone(),
            mine_noteskin: player_options.mine_noteskin.clone(),
            receptor_noteskin: player_options.receptor_noteskin.clone(),
            tap_explosion_noteskin: player_options.tap_explosion_noteskin.clone(),
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: player_options.scroll_speed,
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
            timing_windows: player_options.timing_windows,
            show_fa_plus_window: player_options.show_fa_plus_window,
            show_ex_score: player_options.show_ex_score,
            show_hard_ex_score: player_options.show_hard_ex_score,
            show_fa_plus_pane: player_options.show_fa_plus_pane,
            fa_plus_10ms_blue_window: player_options.fa_plus_10ms_blue_window,
            split_15_10ms: player_options.split_15_10ms,
            track_early_judgments: player_options.track_early_judgments,
            custom_fantastic_window: player_options.custom_fantastic_window,
            custom_fantastic_window_ms: player_options.custom_fantastic_window_ms,
            judgment_tilt: player_options.judgment_tilt,
            column_cues: player_options.column_cues,
            judgment_back: player_options.judgment_back,
            error_ms_display: player_options.error_ms_display,
            display_scorebox: player_options.display_scorebox,
            rainbow_max: player_options.rainbow_max,
            responsive_colors: player_options.responsive_colors,
            show_life_percent: player_options.show_life_percent,
            tilt_multiplier: player_options.tilt_multiplier,
            error_bar: player_options.error_bar,
            error_bar_active_mask: player_options.error_bar_active_mask,
            error_bar_text: player_options.error_bar_text,
            error_bar_up: player_options.error_bar_up,
            error_bar_multi_tick: player_options.error_bar_multi_tick,
            error_bar_trim: player_options.error_bar_trim,
            data_visualizations: player_options.data_visualizations,
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
            column_flash_on_miss: player_options.column_flash_on_miss,
            subtractive_scoring: player_options.subtractive_scoring,
            pacemaker: player_options.pacemaker,
            nps_graph_at_top: player_options.nps_graph_at_top,
            transparent_density_graph_bg: player_options.transparent_density_graph_bg,
            mini_indicator: player_options.mini_indicator,
            mini_indicator_score_type: player_options.mini_indicator_score_type,
            mini_percent: player_options.mini_percent,
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
        }
    }
}

impl Profile {
    #[inline(always)]
    pub const fn calculated_weight_pounds(&self) -> i32 {
        if self.weight_pounds == 0 {
            DEFAULT_WEIGHT_POUNDS
        } else {
            self.weight_pounds
        }
    }

    #[inline(always)]
    pub const fn age_years_for(&self, current_year: i32) -> i32 {
        if self.birth_year == 0 {
            current_year - DEFAULT_BIRTH_YEAR
        } else {
            current_year - self.birth_year
        }
    }

    #[inline(always)]
    pub fn age_years(&self) -> i32 {
        self.age_years_for(Local::now().year())
    }

    #[inline(always)]
    pub fn resolved_mine_noteskin(&self) -> &NoteSkin {
        self.mine_noteskin.as_ref().unwrap_or(&self.noteskin)
    }

    #[inline(always)]
    pub fn resolved_receptor_noteskin(&self) -> &NoteSkin {
        self.receptor_noteskin.as_ref().unwrap_or(&self.noteskin)
    }

    #[inline(always)]
    pub fn tap_explosion_noteskin_hidden(&self) -> bool {
        self.tap_explosion_noteskin
            .as_ref()
            .is_some_and(NoteSkin::is_none_choice)
    }

    #[inline(always)]
    pub fn resolved_tap_explosion_noteskin(&self) -> Option<&NoteSkin> {
        if self.tap_explosion_noteskin_hidden() {
            None
        } else {
            Some(
                self.tap_explosion_noteskin
                    .as_ref()
                    .unwrap_or(&self.noteskin),
            )
        }
    }

    #[inline(always)]
    pub fn current_player_options(&self) -> PlayerOptionsData {
        PlayerOptionsData {
            background_filter: self.background_filter,
            hold_judgment_graphic: self.hold_judgment_graphic.clone(),
            judgment_graphic: self.judgment_graphic.clone(),
            combo_font: self.combo_font,
            combo_colors: self.combo_colors,
            combo_mode: self.combo_mode,
            carry_combo_between_songs: self.carry_combo_between_songs,
            noteskin: self.noteskin.clone(),
            mine_noteskin: self.mine_noteskin.clone(),
            receptor_noteskin: self.receptor_noteskin.clone(),
            tap_explosion_noteskin: self.tap_explosion_noteskin.clone(),
            scroll_speed: self.scroll_speed,
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
            timing_windows: self.timing_windows,
            show_fa_plus_window: self.show_fa_plus_window,
            show_ex_score: self.show_ex_score,
            show_hard_ex_score: self.show_hard_ex_score,
            show_fa_plus_pane: self.show_fa_plus_pane,
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
            split_15_10ms: self.split_15_10ms,
            track_early_judgments: self.track_early_judgments,
            custom_fantastic_window: self.custom_fantastic_window,
            custom_fantastic_window_ms: self.custom_fantastic_window_ms,
            judgment_tilt: self.judgment_tilt,
            column_cues: self.column_cues,
            judgment_back: self.judgment_back,
            error_ms_display: self.error_ms_display,
            display_scorebox: self.display_scorebox,
            rainbow_max: self.rainbow_max,
            responsive_colors: self.responsive_colors,
            show_life_percent: self.show_life_percent,
            tilt_multiplier: self.tilt_multiplier,
            error_bar_active_mask: self.error_bar_active_mask,
            error_bar: self.error_bar,
            error_bar_text: self.error_bar_text,
            error_bar_up: self.error_bar_up,
            error_bar_multi_tick: self.error_bar_multi_tick,
            error_bar_trim: self.error_bar_trim,
            data_visualizations: self.data_visualizations,
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
            column_flash_on_miss: self.column_flash_on_miss,
            subtractive_scoring: self.subtractive_scoring,
            pacemaker: self.pacemaker,
            nps_graph_at_top: self.nps_graph_at_top,
            transparent_density_graph_bg: self.transparent_density_graph_bg,
            mini_indicator: self.mini_indicator,
            mini_indicator_score_type: self.mini_indicator_score_type,
            mini_percent: self.mini_percent,
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
        self.scroll_speed = options.scroll_speed;
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
        self.timing_windows = options.timing_windows;
        self.show_fa_plus_window = options.show_fa_plus_window;
        self.show_ex_score = options.show_ex_score;
        self.show_hard_ex_score = options.show_hard_ex_score;
        self.show_fa_plus_pane = options.show_fa_plus_pane;
        self.fa_plus_10ms_blue_window = options.fa_plus_10ms_blue_window;
        self.split_15_10ms = options.split_15_10ms;
        self.track_early_judgments = options.track_early_judgments;
        self.custom_fantastic_window = options.custom_fantastic_window;
        self.custom_fantastic_window_ms = options.custom_fantastic_window_ms;
        self.judgment_tilt = options.judgment_tilt;
        self.column_cues = options.column_cues;
        self.judgment_back = options.judgment_back;
        self.error_ms_display = options.error_ms_display;
        self.display_scorebox = options.display_scorebox;
        self.rainbow_max = options.rainbow_max;
        self.responsive_colors = options.responsive_colors;
        self.show_life_percent = options.show_life_percent;
        self.tilt_multiplier = options.tilt_multiplier;
        self.error_bar_active_mask = options.error_bar_active_mask;
        self.error_bar = options.error_bar;
        self.error_bar_text = options.error_bar_text;
        self.error_bar_up = options.error_bar_up;
        self.error_bar_multi_tick = options.error_bar_multi_tick;
        self.error_bar_trim = options.error_bar_trim;
        self.data_visualizations = options.data_visualizations;
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
        self.column_flash_on_miss = options.column_flash_on_miss;
        self.subtractive_scoring = options.subtractive_scoring;
        self.pacemaker = options.pacemaker;
        self.nps_graph_at_top = options.nps_graph_at_top;
        self.transparent_density_graph_bg = options.transparent_density_graph_bg;
        self.mini_indicator = options.mini_indicator;
        self.mini_indicator_score_type = options.mini_indicator_score_type;
        self.mini_percent = options.mini_percent;
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
}

const PLAYER_SLOTS: usize = 2;

#[inline(always)]
const fn side_ix(side: PlayerSide) -> usize {
    match side {
        PlayerSide::P1 => 0,
        PlayerSide::P2 => 1,
    }
}

// Global statics for the loaded player profiles.
static PROFILES: std::sync::LazyLock<Mutex<[Profile; PLAYER_SLOTS]>> =
    std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Profile::default())));

// --- Session-scoped state (not persisted) ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveProfile {
    Guest,
    Local { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl PlayStyle {
    pub const fn chart_type(self) -> &'static str {
        match self {
            Self::Single | Self::Versus => "dance-single",
            Self::Double => "dance-double",
        }
    }
}

#[inline(always)]
pub(crate) const fn player_options_section(style: PlayStyle) -> &'static str {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

pub const GUEST_SCROLL_SPEED: ScrollSpeedSetting = ScrollSpeedSetting::MMod(250.0);

const SESSION_JOINED_MASK_P1: u8 = 1 << 0;
const SESSION_JOINED_MASK_P2: u8 = 1 << 1;

#[inline(always)]
const fn side_joined_mask(side: PlayerSide) -> u8 {
    match side {
        PlayerSide::P1 => SESSION_JOINED_MASK_P1,
        PlayerSide::P2 => SESSION_JOINED_MASK_P2,
    }
}

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
        joined_mask: SESSION_JOINED_MASK_P1,
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
    matches!(
        &lock_session().active_profiles[side_ix(side)],
        ActiveProfile::Guest
    )
}

#[inline(always)]
fn machine_default_noteskin_value() -> NoteSkin {
    NoteSkin::new(&config::machine_default_noteskin())
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
            if matches!(
                &session.active_profiles[side_ix(side)],
                ActiveProfile::Guest
            ) {
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
        default_profile.store_current_player_options_for_all_styles();
        let mut content = String::new();
        write_player_options(
            &mut content,
            "PlayerOptionsSingles",
            &default_profile.player_options_singles,
        );
        write_player_options(
            &mut content,
            "PlayerOptionsDoubles",
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

    write_player_options(
        &mut content,
        "PlayerOptionsSingles",
        &profile.player_options_singles,
    );
    write_player_options(
        &mut content,
        "PlayerOptionsDoubles",
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

    write_last_played(
        &mut content,
        "LastPlayedSingles",
        &profile.last_played_singles,
    );
    write_last_played(
        &mut content,
        "LastPlayedDoubles",
        &profile.last_played_doubles,
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
    if let Ok((stats, _)) =
        bincode::decode_from_slice::<ProfileStatsV1, _>(bytes, bincode::config::standard())
    {
        if stats.version != PROFILE_STATS_VERSION_V1 {
            warn!(
                "Unsupported profile stats version {} in '{}'.",
                stats.version,
                path.display()
            );
            return None;
        }
        return Some(ProfileStats {
            current_combo: stats.current_combo,
            known_pack_names: stats.known_pack_names.into_iter().collect(),
        });
    }
    if let Ok((stats, _)) =
        bincode::decode_from_slice::<LegacyProfileStatsV1, _>(bytes, bincode::config::standard())
    {
        if stats.version != PROFILE_STATS_VERSION_V1 {
            warn!(
                "Unsupported profile stats version {} in '{}'.",
                stats.version,
                path.display()
            );
            return None;
        }
        return Some(ProfileStats {
            current_combo: stats.current_combo,
            known_pack_names: HashSet::new(),
        });
    }
    warn!("Failed to decode profile stats '{}'.", path.display());
    None
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
                let mut known_pack_names: Vec<String> =
                    profile.known_pack_names.into_iter().collect();
                known_pack_names.sort_unstable();
                Some((
                    id.clone(),
                    ProfileStatsV1 {
                        version: PROFILE_STATS_VERSION_V1,
                        current_combo: profile.current_combo,
                        known_pack_names,
                    },
                ))
            }
            ActiveProfile::Guest => None,
        }
    };
    let Some((profile_id, payload)) = maybe_payload else {
        return;
    };
    let Ok(buf) = bincode::encode_to_vec(payload, bincode::config::standard()) else {
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
        default_profile.store_current_player_options_for_all_styles();

        // Load profile.ini
        let mut profile_conf = SimpleIni::new();
        if profile_conf.load(&profile_ini).is_ok() {
            profile.display_name = profile_conf
                .get("userprofile", "DisplayName")
                .unwrap_or(default_profile.display_name.clone());
            profile.player_initials = profile_conf
                .get("userprofile", "PlayerInitials")
                .unwrap_or(default_profile.player_initials.clone());
            profile.player_options_singles = load_player_options(
                &profile_conf,
                "PlayerOptionsSingles",
                &default_profile.player_options_singles,
            )
            .unwrap_or_else(|| default_profile.player_options_singles.clone());
            profile.player_options_doubles = load_player_options(
                &profile_conf,
                "PlayerOptionsDoubles",
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

        // Load groovestats.ini
        let mut gs_conf = SimpleIni::new();
        if gs_conf.load(&groovestats_ini).is_ok() {
            profile.groovestats_api_key = gs_conf
                .get("GrooveStats", "ApiKey")
                .unwrap_or(default_profile.groovestats_api_key.clone());
            profile.groovestats_is_pad_player = parse_groovestats_is_pad_player(
                gs_conf.get("GrooveStats", "IsPadPlayer"),
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

#[derive(Debug, Clone, Default)]
pub struct GameplayHudPlayerSnapshot {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub avatar_texture_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameplayHudSnapshot {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub p1: GameplayHudPlayerSnapshot,
    pub p2: GameplayHudPlayerSnapshot,
}

pub fn gameplay_hud_snapshot() -> GameplayHudSnapshot {
    let (play_style, player_side, joined_mask, p1_guest, p2_guest) = {
        let session = lock_session();
        (
            session.play_style,
            session.player_side,
            session.joined_mask,
            matches!(
                &session.active_profiles[side_ix(PlayerSide::P1)],
                ActiveProfile::Guest
            ),
            matches!(
                &session.active_profiles[side_ix(PlayerSide::P2)],
                ActiveProfile::Guest
            ),
        )
    };
    let profiles = lock_profiles();
    let p1_profile = &profiles[side_ix(PlayerSide::P1)];
    let p2_profile = &profiles[side_ix(PlayerSide::P2)];
    GameplayHudSnapshot {
        play_style,
        player_side,
        p1: GameplayHudPlayerSnapshot {
            joined: joined_mask & SESSION_JOINED_MASK_P1 != 0,
            guest: p1_guest,
            display_name: p1_profile.display_name.clone(),
            avatar_texture_key: p1_profile.avatar_texture_key.clone(),
        },
        p2: GameplayHudPlayerSnapshot {
            joined: joined_mask & SESSION_JOINED_MASK_P2 != 0,
            guest: p2_guest,
            display_name: p2_profile.display_name.clone(),
            avatar_texture_key: p2_profile.avatar_texture_key.clone(),
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
    match &session.active_profiles[side_ix(side)] {
        ActiveProfile::Local { id } => Some(id.clone()),
        ActiveProfile::Guest => None,
    }
}

pub fn known_pack_names_for_local_profile(profile_id: &str) -> Option<HashSet<String>> {
    let session = lock_session();
    let profiles = lock_profiles();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let ActiveProfile::Local { id } = &session.active_profiles[side_ix(side)] else {
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
            let ActiveProfile::Local { id } = &session.active_profiles[side_ix(side)] else {
                continue;
            };
            if id != profile_id {
                continue;
            }
            let profile = &mut profiles[side_ix(side)];
            let mut changed = false;
            for name in &pack_names {
                changed |= profile.known_pack_names.insert((*name).to_owned());
            }
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
        out.extend(
            scanned_pack_names
                .iter()
                .filter(|name| !known_pack_names.contains(name.as_str()))
                .cloned(),
        );
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

pub struct LocalProfileSummary {
    pub id: String,
    pub display_name: String,
    pub avatar_path: Option<PathBuf>,
}

#[inline(always)]
fn is_local_profile_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s != "." && s != ".." && !s.contains(['/', '\\', '\0'])
}

#[inline(always)]
fn cmp_profile_ids_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
        .then_with(|| a.cmp(b))
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

const LOCAL_PROFILE_MAX_ID: u32 = 99_999_999;

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
    let mut nums = scan_local_profile_numbers();
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
    if next > LOCAL_PROFILE_MAX_ID {
        if first_free > LOCAL_PROFILE_MAX_ID {
            return Err(std::io::Error::other("Too many profiles"));
        }
        next = first_free;
    }
    Ok(format!("{next:08}"))
}

fn initials_from_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            if out.len() >= 2 {
                break;
            }
        }
    }
    match out.len() {
        0 => "??".to_string(),
        1 => {
            out.push('?');
            out
        }
        _ => out,
    }
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
    default_profile.store_current_player_options_for_all_styles();
    let initials = initials_from_name(name);
    let mut content = String::new();
    write_player_options(
        &mut content,
        "PlayerOptionsSingles",
        &default_profile.player_options_singles,
    );
    write_player_options(
        &mut content,
        "PlayerOptionsDoubles",
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

    fs::write(path, out)
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
    mask & side_joined_mask(side) != 0
}

pub fn is_session_side_guest(side: PlayerSide) -> bool {
    session_side_is_guest(side)
}

pub fn set_session_joined(p1: bool, p2: bool) {
    let mask = (u8::from(p1) * SESSION_JOINED_MASK_P1) | (u8::from(p2) * SESSION_JOINED_MASK_P2);
    lock_session().joined_mask = mask;
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
        DEFAULT_BIRTH_YEAR, DEFAULT_WEIGHT_POUNDS, LastPlayed, NoteSkin, PlayStyle, Profile,
        TimingWindowsOption, parse_groovestats_is_pad_player,
    };

    #[test]
    fn groovestats_is_pad_player_requires_explicit_one() {
        assert!(parse_groovestats_is_pad_player(
            Some("1".to_string()),
            false
        ));
        assert!(!parse_groovestats_is_pad_player(
            Some("0".to_string()),
            false
        ));
        assert!(!parse_groovestats_is_pad_player(
            Some("2".to_string()),
            false
        ));
        assert!(!parse_groovestats_is_pad_player(
            Some("255".to_string()),
            false
        ));
    }

    #[test]
    fn groovestats_is_pad_player_uses_default_on_invalid_value() {
        assert!(parse_groovestats_is_pad_player(None, true));
        assert!(!parse_groovestats_is_pad_player(None, false));
        assert!(parse_groovestats_is_pad_player(
            Some("abc".to_string()),
            true
        ));
        assert!(!parse_groovestats_is_pad_player(
            Some("abc".to_string()),
            false
        ));
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
        profile.global_offset_shift_ms = 7;
        profile.timing_windows = TimingWindowsOption::WayOffs;
        profile.receptor_noteskin = Some(NoteSkin::new("default"));
        profile.tap_explosion_noteskin = Some(NoteSkin::new("metal"));
        profile.store_current_player_options(PlayStyle::Single);

        profile.mini_percent = 62;
        profile.show_ex_score = false;
        profile.global_offset_shift_ms = -13;
        profile.timing_windows = TimingWindowsOption::FantasticsAndExcellents;
        profile.receptor_noteskin = Some(NoteSkin::new("cyber"));
        profile.tap_explosion_noteskin = None;
        profile.store_current_player_options(PlayStyle::Double);

        profile.apply_player_options_for_style(PlayStyle::Single);
        assert_eq!(profile.mini_percent, 18);
        assert!(profile.show_ex_score);
        assert_eq!(profile.global_offset_shift_ms, 7);
        assert_eq!(profile.timing_windows, TimingWindowsOption::WayOffs);
        assert_eq!(profile.receptor_noteskin, Some(NoteSkin::new("default")));
        assert_eq!(profile.tap_explosion_noteskin, Some(NoteSkin::new("metal")));

        profile.apply_player_options_for_style(PlayStyle::Double);
        assert_eq!(profile.mini_percent, 62);
        assert!(!profile.show_ex_score);
        assert_eq!(profile.global_offset_shift_ms, -13);
        assert_eq!(
            profile.timing_windows,
            TimingWindowsOption::FantasticsAndExcellents
        );
        assert_eq!(profile.receptor_noteskin, Some(NoteSkin::new("cyber")));
        assert_eq!(profile.tap_explosion_noteskin, None);
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
}
