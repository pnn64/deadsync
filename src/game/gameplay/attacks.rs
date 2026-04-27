use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::chart::{ChartData, GameplayChartData};
use crate::game::note::Note;
use crate::game::parsing::song_lua::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaCompileContext, SongLuaDifficulty,
    SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow,
    SongLuaOverlayActor, SongLuaOverlayEase, SongLuaOverlayMessageCommand, SongLuaOverlayState,
    SongLuaPlayerContext, SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit, compile_song_lua,
};
use crate::game::profile;
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::song::SongData;
use crate::game::timing::{ROWS_PER_BEAT, TimingData};
use log::{debug, info, trace, warn};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use super::{
    AccelEffects, AccelOverrides, AppearanceEffects, AppearanceOverrides, ChartAttackEffects,
    HOLDS_MASK_BIT_FLOORED, HOLDS_MASK_BIT_HOLDS_TO_ROLLS, HOLDS_MASK_BIT_NO_ROLLS,
    HOLDS_MASK_BIT_PLANTED, HOLDS_MASK_BIT_TWISTER, INSERT_MASK_BIT_BIG, INSERT_MASK_BIT_BMRIZE,
    INSERT_MASK_BIT_ECHO, INSERT_MASK_BIT_MINES, INSERT_MASK_BIT_QUICK, INSERT_MASK_BIT_SKIPPY,
    INSERT_MASK_BIT_STOMP, INSERT_MASK_BIT_WIDE, MAX_PLAYERS, PerspectiveEffects,
    PerspectiveOverrides, RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS, RANDOM_ATTACK_MOD_POOL,
    RANDOM_ATTACK_OVERLAP_SECONDS, RANDOM_ATTACK_RUN_TIME_SECONDS,
    RANDOM_ATTACK_START_SECONDS_INIT, REMOVE_MASK_BIT_LITTLE, REMOVE_MASK_BIT_NO_FAKES,
    REMOVE_MASK_BIT_NO_HANDS, REMOVE_MASK_BIT_NO_HOLDS, REMOVE_MASK_BIT_NO_JUMPS,
    REMOVE_MASK_BIT_NO_LIFTS, REMOVE_MASK_BIT_NO_MINES, REMOVE_MASK_BIT_NO_QUADS, ScrollEffects,
    ScrollOverrides, State, TurnRng, VisibilityEffects, VisibilityOverrides, VisualEffects,
    VisualOverrides, apply_hyper_shuffle, apply_super_shuffle_taps, apply_turn_permutation,
    apply_uncommon_masks_with_masks, song_lua_display_bpm_pair, sort_player_notes,
};

#[derive(Clone, Debug)]
struct ChartAttackWindow {
    start_second: f32,
    len_seconds: f32,
    mods: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AttackMaskWindow {
    pub(super) start_second: f32,
    pub(super) end_second: f32,
    pub(super) clear_all: bool,
    pub(super) chart: ChartAttackEffects,
    pub(super) accel: AccelOverrides,
    pub(super) visual: VisualOverrides,
    pub(super) appearance: AppearanceOverrides,
    pub(super) appearance_speed: AppearanceOverrides,
    pub(super) visibility: VisibilityOverrides,
    pub(super) scroll: ScrollOverrides,
    pub(super) perspective: PerspectiveOverrides,
    pub(super) scroll_speed: Option<ScrollSpeedSetting>,
    pub(super) mini_percent: Option<f32>,
}

#[derive(Clone, Debug)]
pub(super) struct SongLuaEaseMaskWindow {
    pub(super) start_second: f32,
    pub(super) end_second: f32,
    pub(super) sustain_end_second: f32,
    pub(super) target: SongLuaEaseMaskTarget,
    pub(super) from: f32,
    pub(super) to: f32,
    pub(super) easing: Option<String>,
    pub(super) opt1: Option<f32>,
    pub(super) opt2: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct SongLuaOverlayEaseWindowRuntime {
    pub overlay_index: usize,
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub cutoff_second: Option<f32>,
    pub from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
    pub to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct SongLuaOverlayMessageRuntime {
    pub event_second: f32,
    pub command_index: usize,
}

#[derive(Clone, Debug)]
pub struct SongLuaVisualLayerRuntime {
    pub start_second: f32,
    pub screen_width: f32,
    pub screen_height: f32,
    pub overlays: Vec<SongLuaOverlayActor>,
    pub overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime>,
    pub overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    pub overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    pub song_foreground: SongLuaCapturedActor,
    pub song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
}

fn extend_song_lua_sound_paths(out: &mut Vec<PathBuf>, paths: &[PathBuf]) {
    for path in paths {
        if !out.contains(path) {
            out.push(path.clone());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SongLuaEaseMaskTarget {
    AccelBoost,
    AccelBrake,
    AccelWave,
    AccelExpand,
    AccelBoomerang,
    VisualDrunk,
    VisualDizzy,
    VisualConfusion,
    VisualConfusionOffset,
    VisualFlip,
    VisualInvert,
    VisualTornado,
    VisualTipsy,
    VisualBumpy,
    VisualBeat,
    AppearanceHidden,
    AppearanceSudden,
    AppearanceStealth,
    AppearanceBlink,
    AppearanceRandomVanish,
    VisibilityDark,
    VisibilityBlind,
    VisibilityCover,
    ScrollReverse,
    ScrollSplit,
    ScrollAlternate,
    ScrollCross,
    ScrollCentered,
    PerspectiveTilt,
    PerspectiveSkew,
    ScrollSpeedX,
    ScrollSpeedC,
    ScrollSpeedM,
    MiniPercent,
    PlayerX,
    PlayerY,
    PlayerZ,
    PlayerRotationX,
    PlayerRotationZ,
    PlayerRotationY,
    PlayerSkewX,
    PlayerSkewY,
    PlayerZoom,
    PlayerZoomX,
    PlayerZoomY,
    PlayerZoomZ,
    ConfusionYOffsetY,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ParsedAttackMods {
    pub(super) insert_mask: u8,
    pub(super) remove_mask: u8,
    pub(super) holds_mask: u8,
    pub(super) turn_option: profile::TurnOption,
    pub(super) clear_all: bool,
    pub(super) accel: AccelOverrides,
    pub(super) visual: VisualOverrides,
    pub(super) appearance: AppearanceOverrides,
    pub(super) appearance_speed: AppearanceOverrides,
    pub(super) visibility: VisibilityOverrides,
    pub(super) scroll: ScrollOverrides,
    pub(super) perspective: PerspectiveOverrides,
    pub(super) scroll_speed: Option<ScrollSpeedSetting>,
    pub(super) mini_percent: Option<f32>,
}

impl Default for ParsedAttackMods {
    fn default() -> Self {
        Self {
            insert_mask: 0,
            remove_mask: 0,
            holds_mask: 0,
            turn_option: profile::TurnOption::None,
            clear_all: false,
            accel: AccelOverrides::default(),
            visual: VisualOverrides::default(),
            appearance: AppearanceOverrides::default(),
            appearance_speed: AppearanceOverrides::default(),
            visibility: VisibilityOverrides::default(),
            scroll: ScrollOverrides::default(),
            perspective: PerspectiveOverrides::default(),
            scroll_speed: None,
            mini_percent: None,
        }
    }
}

impl ParsedAttackMods {
    #[inline(always)]
    fn has_chart_effect(self) -> bool {
        self.insert_mask != 0
            || self.remove_mask != 0
            || self.holds_mask != 0
            || self.turn_option != profile::TurnOption::None
    }

    #[inline(always)]
    fn has_runtime_mask_effect(self) -> bool {
        self.clear_all
            || self.accel.any()
            || self.visual.any()
            || self.appearance.any()
            || self.visibility.any()
            || self.scroll.any()
            || self.perspective.any()
            || self.scroll_speed.is_some()
            || self.mini_percent.is_some()
    }
}

#[inline(always)]
pub(super) const fn turn_option_bits(turn: profile::TurnOption) -> u16 {
    match turn {
        profile::TurnOption::None => 0,
        profile::TurnOption::Mirror => 1 << 0,
        profile::TurnOption::Left => 1 << 1,
        profile::TurnOption::Right => 1 << 2,
        profile::TurnOption::LRMirror => 1 << 3,
        profile::TurnOption::UDMirror => 1 << 4,
        profile::TurnOption::Shuffle => 1 << 5,
        profile::TurnOption::Blender => 1 << 6,
        profile::TurnOption::Random => 1 << 7,
    }
}

fn parse_chart_attack_windows(raw: &str) -> Vec<ChartAttackWindow> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let upper = raw.to_ascii_uppercase();
    let mut starts = Vec::with_capacity(8);
    let mut scan = 0usize;
    while let Some(pos) = upper[scan..].find("TIME=") {
        let idx = scan + pos;
        starts.push(idx);
        scan = idx.saturating_add(5);
        if scan >= raw.len() {
            break;
        }
    }
    if starts.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(starts.len());
    for (i, start) in starts.iter().copied().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(raw.len());
        let chunk = &raw[start..end];
        let mut time = None;
        let mut len = None;
        let mut end_time = None;
        let mut mods = None;

        for part in chunk.split(':') {
            let part = part.trim();
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            let key = k.trim().to_ascii_uppercase();
            let value = v.trim().trim_end_matches(',').trim();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "TIME" => time = value.parse::<f32>().ok(),
                "LEN" => len = value.parse::<f32>().ok(),
                "END" => end_time = value.parse::<f32>().ok(),
                "MODS" => mods = Some(value.to_string()),
                _ => {}
            }
        }

        let (Some(start_second), Some(mods)) = (time, mods) else {
            continue;
        };
        if !start_second.is_finite() || mods.is_empty() {
            continue;
        }
        let mut len_seconds = len.unwrap_or(0.0);
        if let Some(end_second) = end_time
            && end_second.is_finite()
        {
            len_seconds = end_second - start_second;
        }
        if !len_seconds.is_finite() || len_seconds < 0.0 {
            len_seconds = 0.0;
        }
        out.push(ChartAttackWindow {
            start_second,
            len_seconds,
            mods,
        });
    }

    out
}

fn attack_token_key(token: &str) -> String {
    let mut key = String::with_capacity(token.len());
    for ch in token.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    while key.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        key.remove(0);
    }
    key
}

#[inline(always)]
fn parse_attack_scroll_override(token: &str) -> Option<ScrollSpeedSetting> {
    let trimmed = token.trim();
    let value = trimmed
        .strip_suffix('x')
        .or_else(|| trimmed.strip_suffix('X'))
        .and_then(|v| v.trim().parse::<f32>().ok());
    if let Some(v) = value.filter(|v| v.is_finite() && *v > 0.0) {
        return Some(ScrollSpeedSetting::XMod(v));
    }
    ScrollSpeedSetting::from_str(trimmed).ok()
}

#[inline(always)]
fn parse_attack_approach_prefix(token: &str) -> (f32, &str) {
    let token = token.trim();
    let Some(prefix) = token.split_ascii_whitespace().next() else {
        return (1.0, token);
    };
    if prefix.len() <= 1 || !prefix.starts_with('*') {
        return (1.0, token);
    }
    let Some(speed) = prefix[1..]
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
    else {
        return (1.0, token);
    };
    (speed.max(0.0), token[prefix.len()..].trim_start())
}

#[inline(always)]
fn attack_level(percent_value: Option<f32>) -> Option<f32> {
    let raw = percent_value.unwrap_or(100.0);
    raw.is_finite().then_some(raw / 100.0)
}

#[inline(always)]
fn parse_attack_percent_prefix(token: &str) -> (Option<f32>, &str) {
    let Some(idx) = token.find('%') else {
        return (None, token);
    };
    let value = token[..idx].trim().parse::<f32>().ok();
    (value, token[idx + 1..].trim())
}

#[inline(always)]
fn parse_attack_level_token(token: &str) -> (Option<f32>, &str) {
    let token = token.trim();
    if token.len() >= 3 && token[..3].eq_ignore_ascii_case("no ") {
        return (Some(0.0), token[3..].trim());
    }
    parse_attack_percent_prefix(token)
}

fn apply_runtime_mod(
    out: &mut ParsedAttackMods,
    key: &str,
    percent_value: Option<f32>,
    approach_speed: f32,
) {
    match key {
        "wide" => out.insert_mask |= INSERT_MASK_BIT_WIDE,
        "big" => out.insert_mask |= INSERT_MASK_BIT_BIG,
        "quick" => out.insert_mask |= INSERT_MASK_BIT_QUICK,
        "bmrize" => out.insert_mask |= INSERT_MASK_BIT_BMRIZE,
        "skippy" => out.insert_mask |= INSERT_MASK_BIT_SKIPPY,
        "echo" => out.insert_mask |= INSERT_MASK_BIT_ECHO,
        "stomp" => out.insert_mask |= INSERT_MASK_BIT_STOMP,
        "mines" => out.insert_mask |= INSERT_MASK_BIT_MINES,
        "little" => out.remove_mask |= REMOVE_MASK_BIT_LITTLE,
        "nomines" => out.remove_mask |= REMOVE_MASK_BIT_NO_MINES,
        "noholds" => out.remove_mask |= REMOVE_MASK_BIT_NO_HOLDS,
        "nojumps" => out.remove_mask |= REMOVE_MASK_BIT_NO_JUMPS,
        "nohands" => out.remove_mask |= REMOVE_MASK_BIT_NO_HANDS,
        "noquads" => out.remove_mask |= REMOVE_MASK_BIT_NO_QUADS,
        "nolifts" => out.remove_mask |= REMOVE_MASK_BIT_NO_LIFTS,
        "nofakes" => out.remove_mask |= REMOVE_MASK_BIT_NO_FAKES,
        "planted" => out.holds_mask |= HOLDS_MASK_BIT_PLANTED,
        "floored" => out.holds_mask |= HOLDS_MASK_BIT_FLOORED,
        "twister" => out.holds_mask |= HOLDS_MASK_BIT_TWISTER,
        "norolls" => out.holds_mask |= HOLDS_MASK_BIT_NO_ROLLS,
        "holdrolls" | "holdstorolls" => out.holds_mask |= HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        "mirror" => out.turn_option = profile::TurnOption::Mirror,
        "left" => out.turn_option = profile::TurnOption::Left,
        "right" => out.turn_option = profile::TurnOption::Right,
        "lrmirror" => out.turn_option = profile::TurnOption::LRMirror,
        "udmirror" => out.turn_option = profile::TurnOption::UDMirror,
        "shuffle" => out.turn_option = profile::TurnOption::Shuffle,
        "supershuffle" | "blender" => out.turn_option = profile::TurnOption::Blender,
        "hypershuffle" => out.turn_option = profile::TurnOption::Random,
        "reverse" => out.scroll.reverse = attack_level(percent_value),
        "split" => out.scroll.split = attack_level(percent_value),
        "alternate" => out.scroll.alternate = attack_level(percent_value),
        "cross" => out.scroll.cross = attack_level(percent_value),
        "centered" => out.scroll.centered = attack_level(percent_value),
        "boost" => out.accel.boost = attack_level(percent_value),
        "brake" => out.accel.brake = attack_level(percent_value),
        "wave" => out.accel.wave = attack_level(percent_value),
        "expand" => out.accel.expand = attack_level(percent_value),
        "boomerang" => out.accel.boomerang = attack_level(percent_value),
        "drunk" => out.visual.drunk = attack_level(percent_value),
        "dizzy" => out.visual.dizzy = attack_level(percent_value),
        "confusion" => out.visual.confusion = attack_level(percent_value),
        "confusionoffset" => out.visual.confusion_offset = attack_level(percent_value),
        "flip" => out.visual.flip = attack_level(percent_value),
        "invert" => out.visual.invert = attack_level(percent_value),
        "tornado" => out.visual.tornado = attack_level(percent_value),
        "tipsy" => out.visual.tipsy = attack_level(percent_value),
        "bumpy" | "bumpy1" | "bumpy2" | "bumpy3" | "bumpy4" => {
            out.visual.bumpy = attack_level(percent_value)
        }
        "beat" => out.visual.beat = attack_level(percent_value),
        "mini" | "tiny" => {
            let mini = percent_value.unwrap_or(100.0);
            if mini.is_finite() {
                out.mini_percent = Some(mini);
            }
        }
        "hidden" => {
            out.appearance.hidden = attack_level(percent_value);
            out.appearance_speed.hidden = Some(approach_speed);
        }
        "hiddenoffset" => {
            out.appearance.hidden_offset = attack_level(percent_value);
            out.appearance_speed.hidden_offset = Some(approach_speed);
        }
        "sudden" => {
            out.appearance.sudden = attack_level(percent_value);
            out.appearance_speed.sudden = Some(approach_speed);
        }
        "suddenoffset" => {
            out.appearance.sudden_offset = attack_level(percent_value);
            out.appearance_speed.sudden_offset = Some(approach_speed);
        }
        "stealth" => {
            out.appearance.stealth = attack_level(percent_value);
            out.appearance_speed.stealth = Some(approach_speed);
        }
        "blink" => {
            out.appearance.blink = attack_level(percent_value);
            out.appearance_speed.blink = Some(approach_speed);
        }
        "rvanish" | "randomvanish" | "reversevanish" => {
            out.appearance.random_vanish = attack_level(percent_value);
            out.appearance_speed.random_vanish = Some(approach_speed);
        }
        "dark" => out.visibility.dark = attack_level(percent_value),
        "blind" => out.visibility.blind = attack_level(percent_value),
        "cover" => out.visibility.cover = attack_level(percent_value),
        "overhead" => {
            out.perspective.tilt = Some(0.0);
            out.perspective.skew = Some(0.0);
        }
        "incoming" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(-level);
            out.perspective.skew = Some(level);
        }
        "space" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(level);
            out.perspective.skew = Some(level);
        }
        "hallway" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(-level);
            out.perspective.skew = Some(0.0);
        }
        "distant" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(level);
            out.perspective.skew = Some(0.0);
        }
        _ => {}
    }
}

pub(super) fn parse_attack_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let (approach_speed, token) = parse_attack_approach_prefix(token);
        if token.is_empty() {
            continue;
        }
        if let Some(scroll_speed) = parse_attack_scroll_override(token) {
            out.scroll_speed = Some(scroll_speed);
            continue;
        }
        let (percent_value, token_key) = parse_attack_level_token(token);
        let key = attack_token_key(token_key);
        if key.is_empty() {
            continue;
        }
        match key.as_str() {
            "clearall" => {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
            }
            _ => apply_runtime_mod(&mut out, key.as_str(), percent_value, approach_speed),
        }
    }
    out
}

#[inline(always)]
fn parse_song_lua_mod_amount(word: &str) -> Option<f32> {
    let word = word.trim();
    if word.eq_ignore_ascii_case("no") {
        return Some(0.0);
    }
    if let Some(value) = word.strip_suffix('%') {
        return value.trim().parse::<f32>().ok();
    }
    word.parse::<f32>().ok()
}

pub(super) fn parse_song_lua_runtime_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let parts: Vec<&str> = token
            .split_ascii_whitespace()
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }
        if parts.len() == 1 {
            if let Some(scroll_speed) = parse_attack_scroll_override(parts[0]) {
                out.scroll_speed = Some(scroll_speed);
                continue;
            }
            let key = attack_token_key(parts[0]);
            if key.is_empty() {
                continue;
            }
            if key == "clearall" {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
                continue;
            }
            apply_runtime_mod(&mut out, key.as_str(), Some(100.0), 1.0);
            continue;
        }

        if parts[0].starts_with('*') {
            let approach_speed = parse_attack_approach_prefix(parts[0]).0;
            if parts.len() == 2 {
                if let Some(scroll_speed) = parse_attack_scroll_override(parts[1]) {
                    out.scroll_speed = Some(scroll_speed);
                    continue;
                }
                let key = attack_token_key(parts[1]);
                if !key.is_empty() {
                    apply_runtime_mod(&mut out, key.as_str(), Some(100.0), approach_speed);
                }
                continue;
            }
            let key = attack_token_key(parts[2]);
            if key.is_empty() {
                continue;
            }
            let amount = parse_song_lua_mod_amount(parts[1]).unwrap_or(0.0);
            apply_runtime_mod(&mut out, key.as_str(), Some(amount), approach_speed);
            continue;
        }

        let key = attack_token_key(parts[1]);
        if key.is_empty() {
            continue;
        }
        let amount = parse_song_lua_mod_amount(parts[0]).unwrap_or(0.0);
        apply_runtime_mod(&mut out, key.as_str(), Some(amount), 1.0);
    }
    out
}

#[inline(always)]
fn random_attack_seed(base_seed: u64, player: usize, attacks_len: usize) -> u64 {
    base_seed
        ^ (0xC2B2_AE3D_27D4_EB4F_u64.wrapping_mul(player as u64 + 1))
        ^ (attacks_len as u64).wrapping_mul(0x9E37_79B9_u64)
}

fn build_random_attack_windows(
    song_length_seconds: f32,
    player: usize,
    base_seed: u64,
) -> Vec<ChartAttackWindow> {
    if !song_length_seconds.is_finite() || song_length_seconds <= 0.0 {
        return Vec::new();
    }
    let period = (RANDOM_ATTACK_RUN_TIME_SECONDS - RANDOM_ATTACK_OVERLAP_SECONDS).max(0.0);
    if period <= f32::EPSILON || RANDOM_ATTACK_MOD_POOL.is_empty() {
        return Vec::new();
    }
    let first_start =
        (period + RANDOM_ATTACK_START_SECONDS_INIT).max(RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS);
    if first_start >= song_length_seconds {
        return Vec::new();
    }

    let max_windows = ((song_length_seconds - first_start) / period)
        .floor()
        .max(0.0) as usize
        + 1;
    let mut out = Vec::with_capacity(max_windows);
    let mut rng = TurnRng::new(random_attack_seed(base_seed, player, max_windows));
    let mut start = first_start;
    while start < song_length_seconds {
        let mod_idx = rng.gen_range(RANDOM_ATTACK_MOD_POOL.len());
        out.push(ChartAttackWindow {
            start_second: start,
            len_seconds: RANDOM_ATTACK_RUN_TIME_SECONDS,
            mods: RANDOM_ATTACK_MOD_POOL[mod_idx].to_string(),
        });
        start += period;
    }
    out
}

fn build_attack_windows_for_player(
    chart_attacks: Option<&str>,
    attack_mode: profile::AttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<ChartAttackWindow> {
    match attack_mode {
        profile::AttackMode::Off => Vec::new(),
        profile::AttackMode::On => chart_attacks
            .map(parse_chart_attack_windows)
            .unwrap_or_default(),
        profile::AttackMode::Random => {
            build_random_attack_windows(song_length_seconds, player, base_seed)
        }
    }
}

fn select_attack_mods(
    attacks: &[ChartAttackWindow],
    _attack_mode: profile::AttackMode,
    _player: usize,
    _base_seed: u64,
) -> Vec<ParsedAttackMods> {
    if attacks.is_empty() {
        return Vec::new();
    }
    attacks
        .iter()
        .map(|attack| parse_attack_mods(&attack.mods))
        .collect()
}

pub(super) fn build_attack_mask_windows_for_player(
    chart_attacks: Option<&str>,
    attack_mode: profile::AttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let attacks = build_attack_windows_for_player(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if attacks.is_empty() {
        return Vec::new();
    }
    let selected_mods = select_attack_mods(&attacks, attack_mode, player, base_seed);
    if selected_mods.is_empty() {
        return Vec::new();
    }
    let mut windows = Vec::with_capacity(attacks.len());
    for (attack, mods) in attacks.iter().zip(selected_mods.iter().copied()) {
        if !mods.has_runtime_mask_effect() && !mods.has_chart_effect() {
            continue;
        }
        let start_second = attack.start_second;
        let end_second = start_second + attack.len_seconds.max(0.0);
        if !start_second.is_finite() || !end_second.is_finite() || end_second <= start_second {
            continue;
        }
        windows.push(AttackMaskWindow {
            start_second,
            end_second,
            clear_all: mods.clear_all,
            chart: ChartAttackEffects {
                insert_mask: mods.insert_mask,
                remove_mask: mods.remove_mask,
                holds_mask: mods.holds_mask,
                turn_bits: turn_option_bits(mods.turn_option),
            },
            accel: mods.accel,
            visual: mods.visual,
            appearance: mods.appearance,
            appearance_speed: mods.appearance_speed,
            visibility: mods.visibility,
            scroll: mods.scroll,
            perspective: mods.perspective,
            scroll_speed: mods.scroll_speed,
            mini_percent: mods.mini_percent,
        });
    }
    windows
}

#[inline(always)]
fn song_lua_target_matches_player(target_player: Option<u8>, player: usize) -> bool {
    match target_player {
        Some(target) => usize::from(target) == player + 1,
        None => true,
    }
}

#[inline(always)]
fn song_lua_end_value(start: f32, limit: f32, span_mode: SongLuaSpanMode) -> f32 {
    match span_mode {
        SongLuaSpanMode::Len => start + limit.max(0.0),
        SongLuaSpanMode::End => limit,
    }
}

#[inline(always)]
fn song_lua_time_to_second(
    unit: SongLuaTimeUnit,
    value: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> f32 {
    match unit {
        SongLuaTimeUnit::Beat => timing_player.get_time_for_beat(value),
        SongLuaTimeUnit::Second => value - global_offset_seconds,
    }
}

fn song_lua_window_seconds(
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<(f32, f32)> {
    let end = song_lua_end_value(start, limit, span_mode);
    let start_second = song_lua_time_to_second(unit, start, timing_player, global_offset_seconds);
    let end_second = song_lua_time_to_second(unit, end, timing_player, global_offset_seconds);
    if !start_second.is_finite() || !end_second.is_finite() || end_second < start_second {
        return None;
    }
    Some((start_second, end_second))
}

fn song_lua_sustain_end_second(
    window: &SongLuaEaseWindow,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    end_second: f32,
) -> f32 {
    let Some(sustain) = window.sustain else {
        return end_second;
    };
    let sustain_value = match window.span_mode {
        SongLuaSpanMode::Len => {
            song_lua_end_value(window.start, window.limit, window.span_mode) + sustain
        }
        SongLuaSpanMode::End => sustain,
    };
    let sustain_end_second = song_lua_time_to_second(
        window.unit,
        sustain_value,
        timing_player,
        global_offset_seconds,
    );
    if sustain_end_second.is_finite() && sustain_end_second > end_second {
        sustain_end_second
    } else {
        end_second
    }
}

fn build_song_lua_constant_window(
    window: &SongLuaModWindow,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<AttackMaskWindow> {
    let (start_second, end_second) = song_lua_window_seconds(
        window.unit,
        window.start,
        window.limit,
        window.span_mode,
        timing_player,
        global_offset_seconds,
    )?;
    if end_second <= start_second {
        return None;
    }
    let mods = parse_song_lua_runtime_mods(&window.mods);
    if !mods.has_runtime_mask_effect() {
        return None;
    }
    Some(AttackMaskWindow {
        start_second,
        end_second,
        clear_all: mods.clear_all,
        chart: ChartAttackEffects::default(),
        accel: mods.accel,
        visual: mods.visual,
        appearance: mods.appearance,
        appearance_speed: mods.appearance_speed,
        visibility: mods.visibility,
        scroll: mods.scroll,
        perspective: mods.perspective,
        scroll_speed: mods.scroll_speed,
        mini_percent: mods.mini_percent,
    })
}

fn build_song_lua_constant_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let mut out = Vec::new();
    for window in &compiled.time_mods {
        if song_lua_target_matches_player(window.player, player)
            && let Some(window) =
                build_song_lua_constant_window(window, timing_player, global_offset_seconds)
        {
            out.push(window);
        }
    }
    for window in &compiled.beat_mods {
        if song_lua_target_matches_player(window.player, player)
            && let Some(window) =
                build_song_lua_constant_window(window, timing_player, global_offset_seconds)
        {
            out.push(window);
        }
    }
    out
}

#[inline(always)]
fn song_lua_normalized_value(value: f32) -> f32 {
    value / 100.0
}

fn push_song_lua_ease_target(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    target: SongLuaEaseMaskTarget,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) {
    out.push(SongLuaEaseMaskWindow {
        start_second,
        end_second,
        sustain_end_second,
        target,
        from,
        to,
        easing: easing.map(ToString::to_string),
        opt1,
        opt2,
    });
}

fn append_song_lua_ease_targets(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    target_name: &str,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> bool {
    let key = attack_token_key(target_name);
    if key.is_empty() {
        return false;
    }
    let pct_from = song_lua_normalized_value(from);
    let pct_to = song_lua_normalized_value(to);
    match key.as_str() {
        "boost" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AccelBoost,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "brake" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AccelBrake,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "wave" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AccelWave,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "expand" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AccelExpand,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "boomerang" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AccelBoomerang,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "drunk" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualDrunk,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "dizzy" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualDizzy,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "confusion" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualConfusion,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "confusionoffset" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualConfusionOffset,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "flip" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualFlip,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "invert" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualInvert,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "tornado" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualTornado,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "tipsy" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualTipsy,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "bumpy" | "bumpy1" | "bumpy2" | "bumpy3" | "bumpy4" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualBumpy,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "beat" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisualBeat,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "hidden" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AppearanceHidden,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "sudden" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AppearanceSudden,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "stealth" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AppearanceStealth,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "blink" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AppearanceBlink,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "rvanish" | "randomvanish" | "reversevanish" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::AppearanceRandomVanish,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "dark" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisibilityDark,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "blind" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisibilityBlind,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "cover" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::VisibilityCover,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "reverse" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollReverse,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "split" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollSplit,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "alternate" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollAlternate,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "cross" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollCross,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "centered" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollCentered,
            start_second,
            end_second,
            sustain_end_second,
            pct_from,
            pct_to,
            easing,
            opt1,
            opt2,
        ),
        "incoming" => {
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveTilt,
                start_second,
                end_second,
                sustain_end_second,
                -pct_from,
                -pct_to,
                easing,
                opt1,
                opt2,
            );
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveSkew,
                start_second,
                end_second,
                sustain_end_second,
                pct_from,
                pct_to,
                easing,
                opt1,
                opt2,
            );
        }
        "space" => {
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveTilt,
                start_second,
                end_second,
                sustain_end_second,
                pct_from,
                pct_to,
                easing,
                opt1,
                opt2,
            );
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveSkew,
                start_second,
                end_second,
                sustain_end_second,
                pct_from,
                pct_to,
                easing,
                opt1,
                opt2,
            );
        }
        "hallway" => {
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveTilt,
                start_second,
                end_second,
                sustain_end_second,
                -pct_from,
                -pct_to,
                easing,
                opt1,
                opt2,
            );
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveSkew,
                start_second,
                end_second,
                sustain_end_second,
                0.0,
                0.0,
                easing,
                opt1,
                opt2,
            );
        }
        "distant" => {
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveTilt,
                start_second,
                end_second,
                sustain_end_second,
                pct_from,
                pct_to,
                easing,
                opt1,
                opt2,
            );
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveSkew,
                start_second,
                end_second,
                sustain_end_second,
                0.0,
                0.0,
                easing,
                opt1,
                opt2,
            );
        }
        "overhead" => {
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveTilt,
                start_second,
                end_second,
                sustain_end_second,
                0.0,
                0.0,
                easing,
                opt1,
                opt2,
            );
            push_song_lua_ease_target(
                out,
                SongLuaEaseMaskTarget::PerspectiveSkew,
                start_second,
                end_second,
                sustain_end_second,
                0.0,
                0.0,
                easing,
                opt1,
                opt2,
            );
        }
        "xmod" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollSpeedX,
            start_second,
            end_second,
            sustain_end_second,
            from,
            to,
            easing,
            opt1,
            opt2,
        ),
        "cmod" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollSpeedC,
            start_second,
            end_second,
            sustain_end_second,
            from,
            to,
            easing,
            opt1,
            opt2,
        ),
        "mmod" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ScrollSpeedM,
            start_second,
            end_second,
            sustain_end_second,
            from,
            to,
            easing,
            opt1,
            opt2,
        ),
        "mini" | "tiny" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::MiniPercent,
            start_second,
            end_second,
            sustain_end_second,
            from,
            to,
            easing,
            opt1,
            opt2,
        ),
        "confusionyoffset" => push_song_lua_ease_target(
            out,
            SongLuaEaseMaskTarget::ConfusionYOffsetY,
            start_second,
            end_second,
            sustain_end_second,
            pct_from * (180.0 / std::f32::consts::PI),
            pct_to * (180.0 / std::f32::consts::PI),
            easing,
            opt1,
            opt2,
        ),
        _ => return false,
    }
    true
}

#[inline(always)]
fn song_lua_persistent_player_transform_target(target: SongLuaEaseMaskTarget) -> bool {
    matches!(
        target,
        SongLuaEaseMaskTarget::PlayerX
            | SongLuaEaseMaskTarget::PlayerY
            | SongLuaEaseMaskTarget::PlayerZ
            | SongLuaEaseMaskTarget::PlayerRotationX
            | SongLuaEaseMaskTarget::PlayerRotationZ
            | SongLuaEaseMaskTarget::PlayerRotationY
            | SongLuaEaseMaskTarget::PlayerSkewX
            | SongLuaEaseMaskTarget::PlayerSkewY
            | SongLuaEaseMaskTarget::PlayerZoom
            | SongLuaEaseMaskTarget::PlayerZoomX
            | SongLuaEaseMaskTarget::PlayerZoomY
            | SongLuaEaseMaskTarget::PlayerZoomZ
            | SongLuaEaseMaskTarget::ConfusionYOffsetY
    )
}

fn song_lua_extend_player_transform_tails(out: &mut [SongLuaEaseMaskWindow]) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    for i in 0..out.len() {
        let window = &out[i];
        if !song_lua_persistent_player_transform_target(window.target) {
            continue;
        }
        let default_end = if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
            window.sustain_end_second
        } else {
            f32::MAX
        };
        let cutoff_second = out
            .iter()
            .enumerate()
            .filter_map(|(j, other)| {
                if i == j
                    || other.target != window.target
                    || !other.start_second.is_finite()
                    || other.start_second <= window.start_second + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start_second)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        out[i].sustain_end_second =
            cutoff_second.map_or(default_end, |cutoff| default_end.min(cutoff));
    }
}

pub(super) fn build_song_lua_ease_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> (Vec<SongLuaEaseMaskWindow>, usize) {
    let mut out = Vec::new();
    let mut unsupported_targets = 0usize;
    for window in &compiled.eases {
        if !song_lua_target_matches_player(window.player, player) {
            continue;
        }
        let Some((start_second, end_second)) = song_lua_window_seconds(
            window.unit,
            window.start,
            window.limit,
            window.span_mode,
            timing_player,
            global_offset_seconds,
        ) else {
            continue;
        };
        let sustain_end_second =
            song_lua_sustain_end_second(window, timing_player, global_offset_seconds, end_second);
        if sustain_end_second <= start_second {
            continue;
        }
        match &window.target {
            SongLuaEaseTarget::Mod(target_name) => {
                if !append_song_lua_ease_targets(
                    &mut out,
                    start_second,
                    end_second,
                    sustain_end_second,
                    target_name,
                    window.from,
                    window.to,
                    window.easing.as_deref(),
                    window.opt1,
                    window.opt2,
                ) {
                    unsupported_targets += 1;
                    debug!(
                        "Unsupported gameplay lua ease target for player {}: target='{}' start={:.3} limit={:.3} span={:?} from={:.3} to={:.3} easing={:?}",
                        player + 1,
                        target_name,
                        window.start,
                        window.limit,
                        window.span_mode,
                        window.from,
                        window.to,
                        window.easing
                    );
                }
            }
            SongLuaEaseTarget::PlayerX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZ => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZ,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerRotationX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerRotationX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerRotationZ => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerRotationZ,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerRotationY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerRotationY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerSkewX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerSkewX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerSkewY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerSkewY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoom => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoom,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoomX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoomX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoomY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoomY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoomZ => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoomZ,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::Function => {}
        }
    }
    song_lua_extend_player_transform_tails(&mut out);
    (out, unsupported_targets)
}

pub(super) fn build_song_lua_overlay_ease_windows(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let mut out = Vec::new();
    for ease in &compiled.overlay_eases {
        let Some((start_second, end_second)) = song_lua_window_seconds(
            ease.unit,
            ease.start,
            ease.limit,
            ease.span_mode,
            timing_player,
            global_offset_seconds,
        ) else {
            continue;
        };
        if end_second < start_second {
            continue;
        }
        let sustain_end_second = SongLuaOverlayEaseWindowRuntime::sustain_end_second(
            ease,
            timing_player,
            global_offset_seconds,
            end_second,
        );
        let cutoff_second = song_lua_overlay_ease_cutoff_second(
            compiled,
            ease,
            timing_player,
            global_offset_seconds,
            start_second,
        );
        out.push(SongLuaOverlayEaseWindowRuntime {
            overlay_index: ease.overlay_index,
            start_second,
            end_second,
            sustain_end_second,
            cutoff_second,
            from: ease.from,
            to: ease.to,
            easing: ease.easing.clone(),
            opt1: ease.opt1,
            opt2: ease.opt2,
        });
    }
    out
}

fn group_song_lua_overlay_eases(
    overlay_count: usize,
    overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime>,
) -> (
    Vec<SongLuaOverlayEaseWindowRuntime>,
    Vec<std::ops::Range<usize>>,
) {
    let mut buckets = vec![Vec::new(); overlay_count];
    for ease in overlay_eases {
        if let Some(bucket) = buckets.get_mut(ease.overlay_index) {
            bucket.push(ease);
        }
    }
    let total_len = buckets.iter().map(Vec::len).sum();
    let mut flat = Vec::with_capacity(total_len);
    let mut ranges = Vec::with_capacity(overlay_count);
    for mut bucket in buckets {
        bucket.sort_by(|left, right| {
            left.start_second
                .total_cmp(&right.start_second)
                .then_with(|| left.end_second.total_cmp(&right.end_second))
                .then_with(|| left.sustain_end_second.total_cmp(&right.sustain_end_second))
        });
        let start = flat.len();
        flat.extend(bucket);
        ranges.push(start..flat.len());
    }
    (flat, ranges)
}

fn build_song_lua_overlay_message_events(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<Vec<SongLuaOverlayMessageRuntime>> {
    compiled
        .overlays
        .iter()
        .map(|overlay| {
            build_song_lua_actor_message_events(
                &compiled.messages,
                &overlay.message_commands,
                timing_player,
                global_offset_seconds,
            )
        })
        .collect()
}

fn build_song_lua_actor_message_events(
    messages: &[SongLuaMessageEvent],
    commands: &[SongLuaOverlayMessageCommand],
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<SongLuaOverlayMessageRuntime> {
    let mut out = Vec::new();
    for message in messages {
        let event_second = song_lua_time_to_second(
            SongLuaTimeUnit::Beat,
            message.beat,
            timing_player,
            global_offset_seconds,
        );
        if !event_second.is_finite() {
            continue;
        }
        let Some(command_index) = commands
            .iter()
            .position(|command| command.message.eq_ignore_ascii_case(&message.message))
        else {
            continue;
        };
        out.push(SongLuaOverlayMessageRuntime {
            event_second,
            command_index,
        });
    }
    out
}

fn song_lua_overlay_ease_cutoff_second(
    compiled: &CompiledSongLua,
    ease: &SongLuaOverlayEase,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    start_second: f32,
) -> Option<f32> {
    const SAME_TICK_CUTOFF_EPSILON: f32 = 0.001;

    let overlay = compiled.overlays.get(ease.overlay_index)?;
    let mut cutoff_second: Option<f32> = None;
    for event in &compiled.messages {
        let event_second = song_lua_time_to_second(
            SongLuaTimeUnit::Beat,
            event.beat,
            timing_player,
            global_offset_seconds,
        );
        if !event_second.is_finite() || event_second < start_second {
            continue;
        }
        let Some(command) = overlay
            .message_commands
            .iter()
            .find(|command| command.message.eq_ignore_ascii_case(&event.message))
        else {
            continue;
        };
        for block in &command.blocks {
            if !song_lua_overlay_delta_overlaps(&ease.from, &block.delta)
                && !song_lua_overlay_delta_overlaps(&ease.to, &block.delta)
            {
                continue;
            }
            let block_second = event_second + block.start.max(0.0);
            if !block_second.is_finite() || block_second <= start_second + SAME_TICK_CUTOFF_EPSILON
            {
                continue;
            }
            cutoff_second = Some(match cutoff_second {
                Some(current) => current.min(block_second),
                None => block_second,
            });
        }
    }
    cutoff_second
}

fn song_lua_overlay_delta_overlaps(
    left: &crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
    right: &crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
) -> bool {
    macro_rules! overlap {
        ($field:ident) => {
            if left.$field.is_some() && right.$field.is_some() {
                return true;
            }
        };
    }
    overlap!(x);
    overlap!(y);
    overlap!(z);
    overlap!(halign);
    overlap!(valign);
    overlap!(text_align);
    overlap!(uppercase);
    overlap!(shadow_len);
    overlap!(shadow_color);
    overlap!(glow);
    overlap!(diffuse);
    overlap!(visible);
    overlap!(cropleft);
    overlap!(cropright);
    overlap!(croptop);
    overlap!(cropbottom);
    overlap!(fadeleft);
    overlap!(faderight);
    overlap!(fadetop);
    overlap!(fadebottom);
    overlap!(mask_source);
    overlap!(mask_dest);
    overlap!(zoom);
    overlap!(zoom_x);
    overlap!(zoom_y);
    overlap!(zoom_z);
    overlap!(basezoom);
    overlap!(basezoom_x);
    overlap!(basezoom_y);
    overlap!(rot_x_deg);
    overlap!(rot_y_deg);
    overlap!(rot_z_deg);
    overlap!(skew_x);
    overlap!(skew_y);
    overlap!(blend);
    overlap!(vibrate);
    overlap!(effect_magnitude);
    overlap!(effect_mode);
    overlap!(effect_color1);
    overlap!(effect_color2);
    overlap!(effect_period);
    overlap!(effect_timing);
    overlap!(vert_spacing);
    overlap!(wrap_width_pixels);
    overlap!(max_width);
    overlap!(max_height);
    overlap!(max_w_pre_zoom);
    overlap!(max_h_pre_zoom);
    overlap!(texture_wrapping);
    overlap!(texcoord_offset);
    overlap!(custom_texture_rect);
    overlap!(texcoord_velocity);
    overlap!(size);
    overlap!(stretch_rect);
    false
}

impl SongLuaOverlayEaseWindowRuntime {
    fn sustain_end_second(
        ease: &SongLuaOverlayEase,
        timing_player: &TimingData,
        global_offset_seconds: f32,
        end_second: f32,
    ) -> f32 {
        let Some(sustain) = ease.sustain else {
            return end_second;
        };
        let sustain_value = match ease.span_mode {
            SongLuaSpanMode::Len => {
                song_lua_end_value(ease.start, ease.limit, ease.span_mode) + sustain
            }
            SongLuaSpanMode::End => sustain,
        };
        let sustain_end_second = song_lua_time_to_second(
            ease.unit,
            sustain_value,
            timing_player,
            global_offset_seconds,
        );
        if sustain_end_second.is_finite() && sustain_end_second > end_second {
            sustain_end_second
        } else {
            end_second
        }
    }
}

#[inline(always)]
fn song_lua_difficulty_from_chart(difficulty: &str) -> SongLuaDifficulty {
    if difficulty.eq_ignore_ascii_case("beginner") {
        SongLuaDifficulty::Beginner
    } else if difficulty.eq_ignore_ascii_case("easy") || difficulty.eq_ignore_ascii_case("basic") {
        SongLuaDifficulty::Easy
    } else if difficulty.eq_ignore_ascii_case("medium")
        || difficulty.eq_ignore_ascii_case("standard")
    {
        SongLuaDifficulty::Medium
    } else if difficulty.eq_ignore_ascii_case("hard")
        || difficulty.eq_ignore_ascii_case("difficult")
    {
        SongLuaDifficulty::Hard
    } else if difficulty.eq_ignore_ascii_case("edit") {
        SongLuaDifficulty::Edit
    } else {
        SongLuaDifficulty::Challenge
    }
}

#[inline(always)]
fn song_lua_speedmod_from_setting(speed: ScrollSpeedSetting) -> SongLuaSpeedMod {
    match speed {
        ScrollSpeedSetting::XMod(value) => SongLuaSpeedMod::X(value),
        ScrollSpeedSetting::CMod(value) => SongLuaSpeedMod::C(value),
        ScrollSpeedSetting::MMod(value) => SongLuaSpeedMod::M(value),
    }
}

fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    profile: &profile::Profile,
    play_style: profile::PlayStyle,
    player_side: profile::PlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let centered_one_side =
        num_players == 1 && play_style == profile::PlayStyle::Single && center_1player_notefield;
    let centered_both_sides = num_players == 1 && play_style == profile::PlayStyle::Double;
    let p2_side = if num_players == 1 {
        play_style == profile::PlayStyle::Single && player_side == profile::PlayerSide::P2
    } else {
        player_index == 1
    };
    let base_center_x = if num_players == 2 {
        if p2_side {
            screen_center_x() + (clamped_width * 0.25)
        } else {
            screen_center_x() - (clamped_width * 0.25)
        }
    } else if centered_both_sides || centered_one_side {
        screen_center_x()
    } else if p2_side {
        screen_center_x() + (clamped_width * 0.25)
    } else {
        screen_center_x() - (clamped_width * 0.25)
    };
    if num_players == 1 && (centered_both_sides || centered_one_side) {
        screen_center_x()
    } else {
        let offset_sign = if p2_side { 1.0 } else { -1.0 };
        base_center_x + offset_sign * (profile.note_field_offset_x.clamp(0, 50) as f32)
    }
}

fn build_song_lua_compile_context(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    scroll_speed: &[ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    machine_global_offset_seconds: f32,
) -> SongLuaCompileContext {
    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();
    let screen_width = screen_width();
    let screen_height = screen_height();
    let center_1player_notefield = crate::config::get().center_1player_notefield;
    let mut context = SongLuaCompileContext::new(
        song.simfile_path
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_default(),
        song.title.clone(),
    );
    context.song_display_bpms =
        song_lua_display_bpm_pair(song, charts.first().map(|chart| chart.as_ref()));
    context.song_music_rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    context.music_length_seconds = song.music_length_seconds.max(song.precise_last_second());
    context.style_name = match play_style {
        profile::PlayStyle::Single => "single",
        profile::PlayStyle::Versus => "versus",
        profile::PlayStyle::Double => "double",
    }
    .to_string();
    context.global_offset_seconds = machine_global_offset_seconds;
    context.screen_width = screen_width;
    context.screen_height = screen_height;
    context.confusion_offset_available = true;
    context.confusion_available = true;
    context.amod_available = false;
    context.players = std::array::from_fn(|player| SongLuaPlayerContext {
        enabled: player < num_players,
        difficulty: if player < num_players {
            song_lua_difficulty_from_chart(&charts[player].difficulty)
        } else {
            SongLuaDifficulty::default_enabled()
        },
        display_bpms: if player < num_players {
            song_lua_display_bpm_pair(song, Some(charts[player].as_ref()))
        } else {
            [60.0, 60.0]
        },
        speedmod: if player < num_players {
            song_lua_speedmod_from_setting(scroll_speed[player])
        } else {
            SongLuaSpeedMod::default()
        },
        noteskin_name: if player < num_players {
            player_profiles[player].noteskin.to_string()
        } else {
            crate::game::profile::NoteSkin::default().to_string()
        },
        screen_x: if player < num_players {
            song_lua_compile_player_screen_x(
                num_players,
                player,
                &player_profiles[player],
                play_style,
                player_side,
                center_1player_notefield,
            )
        } else {
            screen_center_x()
        },
        screen_y: screen_center_y(),
    });
    context
}

#[inline(always)]
fn offset_song_lua_overlay_eases(eases: &mut [SongLuaOverlayEaseWindowRuntime], delta: f32) {
    if !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return;
    }
    for ease in eases {
        ease.start_second += delta;
        ease.end_second += delta;
        ease.sustain_end_second += delta;
        ease.cutoff_second = ease.cutoff_second.map(|cutoff| cutoff + delta);
    }
}

#[inline(always)]
fn offset_song_lua_message_events(events: &mut [SongLuaOverlayMessageRuntime], delta: f32) {
    if !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return;
    }
    for event in events {
        event.event_second += delta;
    }
}

fn build_song_lua_visual_layer_runtime(
    song: &SongData,
    start_beat: f32,
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    machine_global_offset_seconds: f32,
) -> Option<SongLuaVisualLayerRuntime> {
    let start_second = song_lua_time_to_second(
        SongLuaTimeUnit::Beat,
        start_beat,
        timing_player,
        machine_global_offset_seconds,
    );
    if !start_second.is_finite() {
        warn!(
            "Skipping song lua visual layer for '{}' at beat {:.3}: invalid start time",
            song.title, start_beat
        );
        return None;
    }

    let mut overlay_eases =
        build_song_lua_overlay_ease_windows(compiled, timing_player, machine_global_offset_seconds);
    offset_song_lua_overlay_eases(&mut overlay_eases, start_second);
    let (overlay_eases, overlay_ease_ranges) =
        group_song_lua_overlay_eases(compiled.overlays.len(), overlay_eases);

    let mut overlay_events = build_song_lua_overlay_message_events(
        compiled,
        timing_player,
        machine_global_offset_seconds,
    );
    for events in &mut overlay_events {
        offset_song_lua_message_events(events, start_second);
    }

    let mut song_foreground_events = build_song_lua_actor_message_events(
        &compiled.messages,
        &compiled.song_foreground.message_commands,
        timing_player,
        machine_global_offset_seconds,
    );
    offset_song_lua_message_events(&mut song_foreground_events, start_second);

    Some(SongLuaVisualLayerRuntime {
        start_second,
        screen_width: compiled.screen_width,
        screen_height: compiled.screen_height,
        overlays: compiled.overlays.clone(),
        overlay_eases,
        overlay_ease_ranges,
        overlay_events,
        song_foreground: compiled.song_foreground.clone(),
        song_foreground_events,
    })
}

pub(super) fn build_song_lua_runtime_windows(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    scroll_speed: &[ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    machine_global_offset_seconds: f32,
    player_global_offset_shift_seconds: &[f32; MAX_PLAYERS],
) -> (
    [Vec<AttackMaskWindow>; MAX_PLAYERS],
    [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS],
    Vec<SongLuaOverlayActor>,
    Vec<SongLuaOverlayEaseWindowRuntime>,
    Vec<std::ops::Range<usize>>,
    Vec<Vec<SongLuaOverlayMessageRuntime>>,
    Vec<SongLuaVisualLayerRuntime>,
    Vec<SongLuaVisualLayerRuntime>,
    [SongLuaCapturedActor; MAX_PLAYERS],
    [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS],
    SongLuaCapturedActor,
    Vec<SongLuaOverlayMessageRuntime>,
    [bool; MAX_PLAYERS],
    Vec<PathBuf>,
    f32,
    f32,
) {
    let mut constant_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut ease_windows: [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut overlays = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut overlay_ease_ranges = Vec::new();
    let mut overlay_events = Vec::new();
    let mut background_visual_layers = Vec::new();
    let mut foreground_visual_layers = Vec::new();
    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();
    let center_1player_notefield = crate::config::get().center_1player_notefield;
    // Default player actor x/y must match StepMania's (SCREEN_CENTER_X, SCREEN_CENTER_Y)
    // origin so that, when no song.lua override is present, the gameplay player
    // transform path produces a zero translation. Without this, every non-lua song
    // would translate the playfield by (-playfield_center_x, +screen_center_y),
    // shoving it up and to the left.
    let default_player_actor = |player_index: usize| SongLuaCapturedActor {
        initial_state: SongLuaOverlayState {
            x: if player_index < num_players {
                song_lua_compile_player_screen_x(
                    num_players,
                    player_index,
                    &player_profiles[player_index],
                    play_style,
                    player_side,
                    center_1player_notefield,
                )
            } else {
                screen_center_x()
            },
            y: screen_center_y(),
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let mut player_actors: [SongLuaCapturedActor; MAX_PLAYERS] =
        std::array::from_fn(default_player_actor);
    let mut player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut song_foreground = SongLuaCapturedActor::default();
    let mut song_foreground_events = Vec::new();
    let mut hidden_players = [false; MAX_PLAYERS];
    let mut sound_paths = Vec::new();
    let screen_width = screen_width();
    let screen_height = screen_height();

    let primary_entry = song
        .foreground_lua_changes
        .iter()
        .find(|change| change.start_beat <= 0.0 && change.path.is_file());

    if primary_entry.is_none()
        && song.background_lua_changes.is_empty()
        && song.foreground_lua_changes.is_empty()
    {
        return (
            constant_windows,
            ease_windows,
            overlays,
            overlay_eases,
            overlay_ease_ranges,
            overlay_events,
            background_visual_layers,
            foreground_visual_layers,
            player_actors,
            player_events,
            song_foreground,
            song_foreground_events,
            hidden_players,
            sound_paths,
            screen_width,
            screen_height,
        );
    }

    let context = build_song_lua_compile_context(
        song,
        charts,
        num_players,
        player_profiles,
        scroll_speed,
        music_rate,
        machine_global_offset_seconds,
    );

    let mut out_screen_width = screen_width;
    let mut out_screen_height = screen_height;

    if let Some(entry) = primary_entry {
        let compiled = match compile_song_lua(&entry.path, &context) {
            Ok(compiled) => compiled,
            Err(err) => {
                warn!(
                    "Failed to compile gameplay lua for '{}' from '{}': {}",
                    song.title,
                    entry.path.display(),
                    err,
                );
                return (
                    constant_windows,
                    ease_windows,
                    overlays,
                    overlay_eases,
                    overlay_ease_ranges,
                    overlay_events,
                    background_visual_layers,
                    foreground_visual_layers,
                    player_actors,
                    player_events,
                    song_foreground,
                    song_foreground_events,
                    hidden_players,
                    sound_paths,
                    screen_width,
                    screen_height,
                );
            }
        };
        extend_song_lua_sound_paths(&mut sound_paths, &compiled.sound_paths);
        overlays = compiled.overlays.clone();
        let overlay_runtime_eases = build_song_lua_overlay_ease_windows(
            &compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        );
        (overlay_eases, overlay_ease_ranges) =
            group_song_lua_overlay_eases(compiled.overlays.len(), overlay_runtime_eases);
        overlay_events = build_song_lua_overlay_message_events(
            &compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        );
        player_actors[..compiled.player_actors.len()].clone_from_slice(&compiled.player_actors);
        for (player, actor) in compiled.player_actors.iter().enumerate() {
            player_events[player] = build_song_lua_actor_message_events(
                &compiled.messages,
                &actor.message_commands,
                timing_players[0].as_ref(),
                machine_global_offset_seconds,
            );
        }
        song_foreground = compiled.song_foreground.clone();
        song_foreground_events = build_song_lua_actor_message_events(
            &compiled.messages,
            &compiled.song_foreground.message_commands,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        );
        hidden_players[..compiled.hidden_players.len()].copy_from_slice(&compiled.hidden_players);

        let mut unsupported_targets = 0usize;
        let mut total_constant = 0usize;
        let mut total_eases = 0usize;
        for player in 0..num_players {
            let player_global_offset_seconds =
                machine_global_offset_seconds + player_global_offset_shift_seconds[player];
            constant_windows[player] = build_song_lua_constant_windows_for_player(
                &compiled,
                timing_players[player].as_ref(),
                player,
                player_global_offset_seconds,
            );
            let (player_eases, player_unsupported_targets) = build_song_lua_ease_windows_for_player(
                &compiled,
                timing_players[player].as_ref(),
                player,
                player_global_offset_seconds,
            );
            unsupported_targets += player_unsupported_targets;
            total_constant += constant_windows[player].len();
            total_eases += player_eases.len();
            ease_windows[player] = player_eases;
        }

        if total_constant > 0
            || total_eases > 0
            || !overlays.is_empty()
            || !overlay_eases.is_empty()
            || !compiled.messages.is_empty()
            || !compiled.sound_paths.is_empty()
            || compiled.info.unsupported_perframes > 0
            || compiled.info.unsupported_function_eases > 0
            || compiled.info.unsupported_function_actions > 0
            || !compiled.info.skipped_message_command_captures.is_empty()
            || unsupported_targets > 0
        {
            info!(
                "Compiled gameplay lua for '{}' (constants={}, eases={}, overlay_eases={}, overlays={}, messages={}, sound_assets={}, unsupported_targets={}, function_eases={}, function_actions={}, perframes={}, skipped_message_commands={}).",
                song.title,
                total_constant,
                total_eases,
                overlay_eases.len(),
                overlays.len(),
                compiled.messages.len(),
                compiled.sound_paths.len(),
                unsupported_targets,
                compiled.info.unsupported_function_eases,
                compiled.info.unsupported_function_actions,
                compiled.info.unsupported_perframes,
                compiled.info.skipped_message_command_captures.len(),
            );
            log_song_lua_runtime_debug(
                song.title.as_str(),
                &compiled,
                &overlay_eases,
                &compiled.messages,
                &hidden_players,
                total_constant,
                total_eases,
                unsupported_targets,
            );
        }

        out_screen_width = compiled.screen_width;
        out_screen_height = compiled.screen_height;
    }

    for change in &song.background_lua_changes {
        let compiled = match compile_song_lua(&change.path, &context) {
            Ok(compiled) => compiled,
            Err(err) => {
                warn!(
                    "Failed to compile background lua layer for '{}' from '{}': {}",
                    song.title,
                    change.path.display(),
                    err,
                );
                continue;
            }
        };
        extend_song_lua_sound_paths(&mut sound_paths, &compiled.sound_paths);
        if let Some(layer) = build_song_lua_visual_layer_runtime(
            song,
            change.start_beat,
            &compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        ) {
            background_visual_layers.push(layer);
        }
    }

    for change in song.foreground_lua_changes.iter().filter(|change| {
        change.path.is_file()
            && !primary_entry.is_some_and(|primary| {
                change.start_beat.to_bits() == primary.start_beat.to_bits()
                    && change.path == primary.path
            })
    }) {
        let compiled = match compile_song_lua(&change.path, &context) {
            Ok(compiled) => compiled,
            Err(err) => {
                warn!(
                    "Failed to compile foreground lua layer for '{}' from '{}': {}",
                    song.title,
                    change.path.display(),
                    err,
                );
                continue;
            }
        };
        extend_song_lua_sound_paths(&mut sound_paths, &compiled.sound_paths);
        if let Some(layer) = build_song_lua_visual_layer_runtime(
            song,
            change.start_beat,
            &compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        ) {
            foreground_visual_layers.push(layer);
        }
    }

    (
        constant_windows,
        ease_windows,
        overlays,
        overlay_eases,
        overlay_ease_ranges,
        overlay_events,
        background_visual_layers,
        foreground_visual_layers,
        player_actors,
        player_events,
        song_foreground,
        song_foreground_events,
        hidden_players,
        sound_paths,
        out_screen_width,
        out_screen_height,
    )
}

fn log_song_lua_runtime_debug(
    song_title: &str,
    compiled: &CompiledSongLua,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    messages: &[SongLuaMessageEvent],
    hidden_players: &[bool; MAX_PLAYERS],
    total_constant: usize,
    total_eases: usize,
    unsupported_targets: usize,
) {
    debug!(
        "Song lua runtime detail for '{}': entry='{}' screen_space={:.1}x{:.1} hidden_players={:?} constants={} eases={} overlay_eases={} overlays={} messages={} sound_assets={} unsupported_targets={} unsupported_function_eases={} unsupported_function_actions={} unsupported_perframes={} skipped_message_commands={}",
        song_title,
        compiled.entry_path.display(),
        compiled.screen_width,
        compiled.screen_height,
        hidden_players,
        total_constant,
        total_eases,
        overlay_eases.len(),
        compiled.overlays.len(),
        messages.len(),
        compiled.sound_paths.len(),
        unsupported_targets,
        compiled.info.unsupported_function_eases,
        compiled.info.unsupported_function_actions,
        compiled.info.unsupported_perframes,
        compiled.info.skipped_message_command_captures.len(),
    );

    let mut message_counts = BTreeMap::<&str, usize>::new();
    for event in messages {
        *message_counts.entry(event.message.as_str()).or_default() += 1;
    }
    if !message_counts.is_empty() {
        debug!(
            "Song lua message kinds for '{}': {}",
            song_title,
            message_counts
                .iter()
                .map(|(message, count)| format!("{message}x{count}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !compiled.sound_paths.is_empty() {
        debug!(
            "Song lua sound assets for '{}': {}",
            song_title,
            compiled
                .sound_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    if !compiled.info.skipped_message_command_captures.is_empty() {
        debug!(
            "Song lua skipped message command captures for '{}': {}",
            song_title,
            compiled.info.skipped_message_command_captures.join(" | ")
        );
    }
    if !compiled
        .info
        .unsupported_function_action_captures
        .is_empty()
    {
        debug!(
            "Song lua unsupported function action captures for '{}': {}",
            song_title,
            compiled
                .info
                .unsupported_function_action_captures
                .join(" | ")
        );
    }
    if !compiled.info.unsupported_function_ease_captures.is_empty() {
        debug!(
            "Song lua unsupported function ease captures for '{}': {}",
            song_title,
            compiled.info.unsupported_function_ease_captures.join(" | ")
        );
    }
    if !compiled.info.unsupported_perframe_captures.is_empty() {
        debug!(
            "Song lua unsupported perframe captures for '{}': {}",
            song_title,
            compiled.info.unsupported_perframe_captures.join(" | ")
        );
    }

    for (index, overlay) in compiled.overlays.iter().enumerate() {
        let message_names = overlay
            .message_commands
            .iter()
            .map(|command| format!("{}({})", command.message, command.blocks.len()))
            .collect::<Vec<_>>();
        debug!(
            "Song lua overlay[{index}] for '{}': kind={:?} name={:?} parent={:?} visible={} xy=({:.1},{:.1}) zoom={:.3}/{:.3}/{:.3} rot=({:.1},{:.1},{:.1}) alpha={:.3} msgs=[{}]",
            song_title,
            overlay.kind,
            overlay.name,
            overlay.parent_index,
            overlay.initial_state.visible,
            overlay.initial_state.x,
            overlay.initial_state.y,
            overlay.initial_state.basezoom,
            overlay.initial_state.zoom_x,
            overlay.initial_state.zoom_y,
            overlay.initial_state.rot_x_deg,
            overlay.initial_state.rot_y_deg,
            overlay.initial_state.rot_z_deg,
            overlay.initial_state.diffuse[3],
            message_names.join(", ")
        );
    }

    for (index, ease) in overlay_eases.iter().enumerate() {
        trace!(
            "Song lua overlay_ease[{index}] for '{}': overlay={} start_s={:.3} end_s={:.3} sustain_end_s={:.3} cutoff_s={:?} easing={:?} from={:?} to={:?}",
            song_title,
            ease.overlay_index,
            ease.start_second,
            ease.end_second,
            ease.sustain_end_second,
            ease.cutoff_second,
            ease.easing,
            ease.from,
            ease.to
        );
    }
    for (index, event) in messages.iter().enumerate() {
        trace!(
            "Song lua message[{index}] for '{}': beat={:.3} message='{}' persists={}",
            song_title, event.beat, event.message, event.persists
        );
    }
}

#[inline(always)]
fn song_lua_lerp_unclamped(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn song_lua_pow_in(t: f32, power: f32) -> f32 {
    t.powf(power)
}

#[inline(always)]
fn song_lua_pow_out(t: f32, power: f32) -> f32 {
    1.0 - (1.0 - t).powf(power)
}

#[inline(always)]
fn song_lua_pow_in_out(t: f32, power: f32) -> f32 {
    if t < 0.5 {
        0.5 * (2.0 * t).powf(power)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - t)).powf(power)
    }
}

#[inline(always)]
fn song_lua_pow_out_in(t: f32, power: f32) -> f32 {
    if t < 0.5 {
        0.5 * song_lua_pow_out(t * 2.0, power)
    } else {
        0.5 + 0.5 * song_lua_pow_in((t * 2.0) - 1.0, power)
    }
}

fn song_lua_out_bounce(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984_375
    }
}

#[inline(always)]
fn song_lua_in_bounce(t: f32) -> f32 {
    1.0 - song_lua_out_bounce(1.0 - t)
}

#[inline(always)]
fn song_lua_in_out_bounce(t: f32) -> f32 {
    if t < 0.5 {
        0.5 * song_lua_in_bounce(t * 2.0)
    } else {
        0.5 + 0.5 * song_lua_out_bounce((t * 2.0) - 1.0)
    }
}

pub(crate) fn song_lua_ease_factor(
    easing: Option<&str>,
    t: f32,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let overshoot = opt1.filter(|v| v.is_finite()).unwrap_or(1.70158);
    let elastic_period = opt1.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.3);
    let elastic_tau = std::f32::consts::TAU / elastic_period;
    match easing.unwrap_or("linear") {
        "linear" => t,
        "inQuad" => song_lua_pow_in(t, 2.0),
        "outQuad" => song_lua_pow_out(t, 2.0),
        "inOutQuad" => song_lua_pow_in_out(t, 2.0),
        "outInQuad" => song_lua_pow_out_in(t, 2.0),
        "inCubic" => song_lua_pow_in(t, 3.0),
        "outCubic" => song_lua_pow_out(t, 3.0),
        "inOutCubic" => song_lua_pow_in_out(t, 3.0),
        "outInCubic" => song_lua_pow_out_in(t, 3.0),
        "inQuart" => song_lua_pow_in(t, 4.0),
        "outQuart" => song_lua_pow_out(t, 4.0),
        "inOutQuart" => song_lua_pow_in_out(t, 4.0),
        "outInQuart" => song_lua_pow_out_in(t, 4.0),
        "inQuint" => song_lua_pow_in(t, 5.0),
        "outQuint" => song_lua_pow_out(t, 5.0),
        "inOutQuint" => song_lua_pow_in_out(t, 5.0),
        "outInQuint" => song_lua_pow_out_in(t, 5.0),
        "inSine" => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        "outSine" => (t * std::f32::consts::FRAC_PI_2).sin(),
        "inOutSine" => -((std::f32::consts::PI * t).cos() - 1.0) * 0.5,
        "outInSine" => {
            if t < 0.5 {
                0.5 * ((t * std::f32::consts::PI).sin())
            } else {
                0.5 + 0.5 * (1.0 - (((t * 2.0) - 1.0) * std::f32::consts::FRAC_PI_2).cos())
            }
        }
        "inExpo" => {
            if t <= 0.0 {
                0.0
            } else {
                2.0_f32.powf((10.0 * t) - 10.0)
            }
        }
        "outExpo" => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * t)
            }
        }
        "inOutExpo" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                0.5 * 2.0_f32.powf((20.0 * t) - 10.0)
            } else {
                1.0 - (0.5 * 2.0_f32.powf((-20.0 * t) + 10.0))
            }
        }
        "outInExpo" => {
            if t < 0.5 {
                0.5 * (1.0 - 2.0_f32.powf(-20.0 * t))
            } else if t >= 1.0 {
                1.0
            } else {
                0.5 + 0.5 * 2.0_f32.powf((20.0 * t) - 20.0)
            }
        }
        "inCirc" => 1.0 - (1.0 - (t * t)).sqrt(),
        "outCirc" => (1.0 - ((t - 1.0) * (t - 1.0))).sqrt(),
        "inOutCirc" => {
            if t < 0.5 {
                0.5 * (1.0 - (1.0 - 4.0 * t * t).sqrt())
            } else {
                0.5 * ((1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0))).sqrt() + 1.0)
            }
        }
        "outInCirc" => {
            if t < 0.5 {
                0.5 * (1.0 - ((2.0 * t - 1.0) * (2.0 * t - 1.0))).sqrt()
            } else {
                0.5 + 0.5 * (1.0 - (1.0 - ((2.0 * t - 1.0) * (2.0 * t - 1.0))).sqrt())
            }
        }
        "inElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                let u = t - 1.0;
                -(2.0_f32.powf(10.0 * u)) * ((u - elastic_period * 0.25) * elastic_tau).sin()
            }
        }
        "outElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                2.0_f32.powf(-10.0 * t) * ((t - elastic_period * 0.25) * elastic_tau).sin() + 1.0
            }
        }
        "inOutElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                let u = (2.0 * t) - 1.0;
                -0.5 * 2.0_f32.powf(10.0 * u) * ((u - elastic_period * 0.375) * elastic_tau).sin()
            } else {
                let u = (2.0 * t) - 1.0;
                0.5 * 2.0_f32.powf(-10.0 * u) * ((u - elastic_period * 0.375) * elastic_tau).sin()
                    + 1.0
            }
        }
        "outInElastic" => {
            if t < 0.5 {
                0.5 * song_lua_ease_factor(Some("outElastic"), t * 2.0, opt1, opt2)
            } else {
                0.5 + 0.5 * song_lua_ease_factor(Some("inElastic"), (t * 2.0) - 1.0, opt1, opt2)
            }
        }
        "inBack" => t * t * (((overshoot + 1.0) * t) - overshoot),
        "outBack" => {
            let u = t - 1.0;
            (u * u * (((overshoot + 1.0) * u) + overshoot)) + 1.0
        }
        "inOutBack" => {
            let s = overshoot * 1.525;
            if t < 0.5 {
                let u = 2.0 * t;
                0.5 * (u * u * (((s + 1.0) * u) - s))
            } else {
                let u = (2.0 * t) - 2.0;
                0.5 * (u * u * (((s + 1.0) * u) + s) + 2.0)
            }
        }
        "outInBack" => {
            if t < 0.5 {
                0.5 * song_lua_ease_factor(Some("outBack"), t * 2.0, opt1, opt2)
            } else {
                0.5 + 0.5 * song_lua_ease_factor(Some("inBack"), (t * 2.0) - 1.0, opt1, opt2)
            }
        }
        "inBounce" => song_lua_in_bounce(t),
        "outBounce" => song_lua_out_bounce(t),
        "inOutBounce" => song_lua_in_out_bounce(t),
        "outInBounce" => {
            if t < 0.5 {
                0.5 * song_lua_out_bounce(t * 2.0)
            } else {
                0.5 + 0.5 * song_lua_in_bounce((t * 2.0) - 1.0)
            }
        }
        _ => t,
    }
}

pub(super) fn song_lua_ease_window_value(window: &SongLuaEaseMaskWindow, now: f32) -> Option<f32> {
    if !now.is_finite()
        || !window.start_second.is_finite()
        || !window.sustain_end_second.is_finite()
        || !window.from.is_finite()
        || !window.to.is_finite()
        || now < window.start_second
        || now >= window.sustain_end_second
    {
        return None;
    }
    if !window.end_second.is_finite()
        || window.end_second <= window.start_second
        || now >= window.end_second
    {
        return Some(window.to);
    }
    let duration = window.end_second - window.start_second;
    if duration <= f32::EPSILON {
        return Some(window.to);
    }
    let factor = song_lua_ease_factor(
        window.easing.as_deref(),
        (now - window.start_second) / duration,
        window.opt1,
        window.opt2,
    );
    let value = song_lua_lerp_unclamped(window.from, window.to, factor);
    if value.is_finite() {
        Some(value)
    } else {
        Some(window.to)
    }
}

pub(super) fn song_lua_apply_eased_target(
    target: SongLuaEaseMaskTarget,
    value: f32,
    accel: &mut AccelOverrides,
    visual: &mut VisualOverrides,
    appearance: &mut AppearanceEffects,
    visibility: &mut VisibilityOverrides,
    scroll: &mut ScrollOverrides,
    perspective: &mut PerspectiveOverrides,
    scroll_speed: &mut Option<ScrollSpeedSetting>,
    mini_percent: &mut Option<f32>,
    player_x: &mut Option<f32>,
    player_y: &mut Option<f32>,
    player_z: &mut Option<f32>,
    player_rotation_x: &mut Option<f32>,
    player_rotation_z: &mut Option<f32>,
    player_rotation_y: &mut Option<f32>,
    player_skew_x: &mut Option<f32>,
    player_skew_y: &mut Option<f32>,
    player_zoom_x: &mut Option<f32>,
    player_zoom_y: &mut Option<f32>,
    player_zoom_z: &mut Option<f32>,
    player_confusion_y_offset: &mut Option<f32>,
) {
    if !value.is_finite() {
        return;
    }
    match target {
        SongLuaEaseMaskTarget::AccelBoost => accel.boost = Some(value),
        SongLuaEaseMaskTarget::AccelBrake => accel.brake = Some(value),
        SongLuaEaseMaskTarget::AccelWave => accel.wave = Some(value),
        SongLuaEaseMaskTarget::AccelExpand => accel.expand = Some(value),
        SongLuaEaseMaskTarget::AccelBoomerang => accel.boomerang = Some(value),
        SongLuaEaseMaskTarget::VisualDrunk => visual.drunk = Some(value),
        SongLuaEaseMaskTarget::VisualDizzy => visual.dizzy = Some(value),
        SongLuaEaseMaskTarget::VisualConfusion => visual.confusion = Some(value),
        SongLuaEaseMaskTarget::VisualConfusionOffset => visual.confusion_offset = Some(value),
        SongLuaEaseMaskTarget::VisualFlip => visual.flip = Some(value),
        SongLuaEaseMaskTarget::VisualInvert => visual.invert = Some(value),
        SongLuaEaseMaskTarget::VisualTornado => visual.tornado = Some(value),
        SongLuaEaseMaskTarget::VisualTipsy => visual.tipsy = Some(value),
        SongLuaEaseMaskTarget::VisualBumpy => visual.bumpy = Some(value),
        SongLuaEaseMaskTarget::VisualBeat => visual.beat = Some(value),
        SongLuaEaseMaskTarget::AppearanceHidden => appearance.hidden = value,
        SongLuaEaseMaskTarget::AppearanceSudden => appearance.sudden = value,
        SongLuaEaseMaskTarget::AppearanceStealth => appearance.stealth = value,
        SongLuaEaseMaskTarget::AppearanceBlink => appearance.blink = value,
        SongLuaEaseMaskTarget::AppearanceRandomVanish => appearance.random_vanish = value,
        SongLuaEaseMaskTarget::VisibilityDark => visibility.dark = Some(value),
        SongLuaEaseMaskTarget::VisibilityBlind => visibility.blind = Some(value),
        SongLuaEaseMaskTarget::VisibilityCover => visibility.cover = Some(value),
        SongLuaEaseMaskTarget::ScrollReverse => scroll.reverse = Some(value),
        SongLuaEaseMaskTarget::ScrollSplit => scroll.split = Some(value),
        SongLuaEaseMaskTarget::ScrollAlternate => scroll.alternate = Some(value),
        SongLuaEaseMaskTarget::ScrollCross => scroll.cross = Some(value),
        SongLuaEaseMaskTarget::ScrollCentered => scroll.centered = Some(value),
        SongLuaEaseMaskTarget::PerspectiveTilt => perspective.tilt = Some(value),
        SongLuaEaseMaskTarget::PerspectiveSkew => perspective.skew = Some(value),
        SongLuaEaseMaskTarget::ScrollSpeedX => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::XMod(value));
            }
        }
        SongLuaEaseMaskTarget::ScrollSpeedC => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::CMod(value));
            }
        }
        SongLuaEaseMaskTarget::ScrollSpeedM => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::MMod(value));
            }
        }
        SongLuaEaseMaskTarget::MiniPercent => *mini_percent = Some(value),
        SongLuaEaseMaskTarget::PlayerX => *player_x = Some(value),
        SongLuaEaseMaskTarget::PlayerY => *player_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZ => *player_z = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationX => *player_rotation_x = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationZ => *player_rotation_z = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationY => *player_rotation_y = Some(value),
        SongLuaEaseMaskTarget::PlayerSkewX => *player_skew_x = Some(value),
        SongLuaEaseMaskTarget::PlayerSkewY => *player_skew_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZoom => {
            *player_zoom_x = Some(value);
            *player_zoom_y = Some(value);
            *player_zoom_z = Some(value);
        }
        SongLuaEaseMaskTarget::PlayerZoomX => *player_zoom_x = Some(value),
        SongLuaEaseMaskTarget::PlayerZoomY => *player_zoom_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZoomZ => *player_zoom_z = Some(value),
        SongLuaEaseMaskTarget::ConfusionYOffsetY => *player_confusion_y_offset = Some(value),
    }
}

#[inline(always)]
fn beat_to_note_row_index(beat: f32) -> usize {
    let rows_per_beat = ROWS_PER_BEAT.max(1) as f32;
    (beat.max(0.0) * rows_per_beat).round() as usize
}

fn apply_attack_turn_mod(
    notes: &mut [Note],
    col_offset: usize,
    cols: usize,
    turn_option: profile::TurnOption,
    seed: u64,
    player: usize,
) {
    if notes.is_empty() || turn_option == profile::TurnOption::None {
        return;
    }
    let note_range = (0usize, notes.len());
    match turn_option {
        profile::TurnOption::None => {}
        profile::TurnOption::Blender => {
            apply_turn_permutation(
                notes,
                note_range,
                col_offset,
                cols,
                profile::TurnOption::Shuffle,
                seed,
            );
            apply_super_shuffle_taps(
                notes,
                note_range,
                col_offset,
                cols,
                seed ^ (0xD00D_F00D_u64.wrapping_mul(player as u64 + 1)),
            );
        }
        profile::TurnOption::Random => {
            apply_hyper_shuffle(
                notes,
                note_range,
                col_offset,
                cols,
                seed ^ (0xA5A5_5A5A_u64.wrapping_mul(player as u64 + 1)),
            );
        }
        other => {
            apply_turn_permutation(notes, note_range, col_offset, cols, other, seed);
        }
    }
}

fn apply_chart_attack_window(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    start_row: usize,
    end_row: usize,
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
    if notes.is_empty() || end_row < start_row || !mods.has_chart_effect() {
        return;
    }
    let mut in_range = Vec::with_capacity(notes.len());
    let mut out_range = Vec::with_capacity(notes.len());
    for note in notes.drain(..) {
        if note.row_index >= start_row && note.row_index <= end_row {
            in_range.push(note);
        } else {
            out_range.push(note);
        }
    }
    if in_range.is_empty() {
        *notes = out_range;
        return;
    }

    apply_uncommon_masks_with_masks(
        &mut in_range,
        mods.insert_mask,
        mods.remove_mask,
        mods.holds_mask,
        timing_player,
        col_offset,
        cols,
        &out_range,
        Some((start_row, end_row)),
        player,
    );
    apply_attack_turn_mod(
        &mut in_range,
        col_offset,
        cols,
        mods.turn_option,
        turn_seed,
        player,
    );

    out_range.extend(in_range);
    *notes = out_range;
    sort_player_notes(notes);
}

fn apply_chart_attacks_for_player(
    notes: &mut Vec<Note>,
    chart_attacks: Option<&str>,
    attack_mode: profile::AttackMode,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) {
    let attacks = build_attack_windows_for_player(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if attacks.is_empty() {
        return;
    }
    let selected_mods = select_attack_mods(&attacks, attack_mode, player, base_seed);
    if selected_mods.is_empty() {
        if attack_mode == profile::AttackMode::Random {
            debug!(
                "Player {} selected RandomAttacks, but no random attack windows were generated.",
                player + 1,
            );
        }
        return;
    }
    for (i, (attack, mods)) in attacks
        .iter()
        .zip(selected_mods.iter().copied())
        .enumerate()
    {
        if !mods.has_chart_effect() {
            continue;
        }
        let start_beat = timing_player.get_beat_for_time(attack.start_second);
        let end_beat = timing_player.get_beat_for_time(attack.start_second + attack.len_seconds);
        let start_row = beat_to_note_row_index(start_beat);
        let end_row = beat_to_note_row_index(end_beat);
        if end_row < start_row {
            continue;
        }
        let turn_seed = base_seed
            ^ (0x9E37_79B9_u64.wrapping_mul(player as u64 + 1))
            ^ ((i as u64).wrapping_mul(0xA5A5_5A5A_u64));
        apply_chart_attack_window(
            notes,
            timing_player,
            col_offset,
            cols,
            player,
            start_row,
            end_row,
            mods,
            turn_seed,
        );
    }
}

#[inline(always)]
pub(super) fn has_chart_attacks(chart: &GameplayChartData, profile: &profile::Profile) -> bool {
    match profile.attack_mode {
        profile::AttackMode::Off => false,
        profile::AttackMode::On => chart
            .chart_attacks
            .as_deref()
            .is_some_and(|raw| !raw.trim().is_empty()),
        profile::AttackMode::Random => true,
    }
}

#[inline(always)]
pub(super) fn player_changes_chart(chart: &GameplayChartData, profile: &profile::Profile) -> bool {
    super::has_uncommon_masks(profile)
        || profile.turn_option != profile::TurnOption::None
        || has_chart_attacks(chart, profile)
}

pub(super) fn apply_chart_attacks_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
) {
    if num_players == 0
        || !(0..num_players)
            .any(|player| has_chart_attacks(&gameplay_charts[player], &player_profiles[player]))
    {
        return;
    }
    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];

    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        if !has_chart_attacks(&gameplay_charts[player], &player_profiles[player]) {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }
        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_chart_attacks_for_player(
            &mut player_notes,
            gameplay_charts[player].chart_attacks.as_deref(),
            player_profiles[player].attack_mode,
            timing_players[player].as_ref(),
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
            base_seed,
            song_length_seconds,
        );
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if num_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }

    *notes = transformed;
    *note_ranges = transformed_ranges;
}

#[inline(always)]
pub(super) fn base_appearance_effects(profile: &profile::Profile) -> AppearanceEffects {
    AppearanceEffects::from_mask(profile.appearance_effects_active_mask.bits())
}

#[inline(always)]
fn apply_appearance_target(
    target: &mut AppearanceEffects,
    speed: &mut AppearanceEffects,
    overrides: AppearanceOverrides,
    override_speeds: AppearanceOverrides,
) {
    if let Some(value) = overrides.hidden {
        target.hidden = value;
        speed.hidden = override_speeds.hidden.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.hidden_offset {
        target.hidden_offset = value;
        speed.hidden_offset = override_speeds.hidden_offset.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.sudden {
        target.sudden = value;
        speed.sudden = override_speeds.sudden.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.sudden_offset {
        target.sudden_offset = value;
        speed.sudden_offset = override_speeds.sudden_offset.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.stealth {
        target.stealth = value;
        speed.stealth = override_speeds.stealth.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.blink {
        target.blink = value;
        speed.blink = override_speeds.blink.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.random_vanish {
        target.random_vanish = value;
        speed.random_vanish = override_speeds.random_vanish.unwrap_or(1.0).max(0.0);
    }
}

#[inline(always)]
fn approach_appearance_effects(
    current: &mut AppearanceEffects,
    target: AppearanceEffects,
    speed: AppearanceEffects,
    delta_time: f32,
) {
    let delta_time = delta_time.max(0.0);
    super::approach_f32(
        &mut current.hidden,
        target.hidden,
        delta_time * speed.hidden,
    );
    super::approach_f32(
        &mut current.hidden_offset,
        target.hidden_offset,
        delta_time * speed.hidden_offset,
    );
    super::approach_f32(
        &mut current.sudden,
        target.sudden,
        delta_time * speed.sudden,
    );
    super::approach_f32(
        &mut current.sudden_offset,
        target.sudden_offset,
        delta_time * speed.sudden_offset,
    );
    super::approach_f32(
        &mut current.stealth,
        target.stealth,
        delta_time * speed.stealth,
    );
    super::approach_f32(&mut current.blink, target.blink, delta_time * speed.blink);
    super::approach_f32(
        &mut current.random_vanish,
        target.random_vanish,
        delta_time * speed.random_vanish,
    );
}

pub(super) fn refresh_active_attack_masks(state: &mut State, delta_time: f32) {
    for player in 0..state.num_players {
        let now = state.current_music_time_visible[player];
        let mut clear_all = false;
        let mut chart = ChartAttackEffects::default();
        let mut accel = AccelOverrides::default();
        let mut visual = VisualOverrides::default();
        let mut appearance_target = base_appearance_effects(&state.player_profiles[player]);
        let mut appearance_speed = AppearanceEffects::approach_speeds();
        let mut visibility = VisibilityOverrides::default();
        let mut scroll = ScrollOverrides::default();
        let mut perspective = PerspectiveOverrides::default();
        let mut scroll_speed = None;
        let mut mini_percent = None;
        let mut player_x = None;
        let mut player_y = None;
        let mut player_z = None;
        let mut player_rotation_x = None;
        let mut player_rotation_z = None;
        let mut player_rotation_y = None;
        let mut player_skew_x = None;
        let mut player_skew_y = None;
        let mut player_zoom_x = None;
        let mut player_zoom_y = None;
        let mut player_zoom_z = None;
        let mut player_confusion_y_offset = None;
        for window in &state.attack_mask_windows[player] {
            if now >= window.start_second && now < window.end_second {
                if window.clear_all {
                    clear_all = true;
                    accel = AccelOverrides::default();
                    visual = VisualOverrides::default();
                    appearance_target = AppearanceEffects::default();
                    appearance_speed = AppearanceEffects::approach_speeds();
                    visibility = VisibilityOverrides::default();
                    scroll = ScrollOverrides::default();
                    perspective = PerspectiveOverrides::default();
                    scroll_speed = None;
                    mini_percent = None;
                }
                chart.insert_mask |= window.chart.insert_mask;
                chart.remove_mask |= window.chart.remove_mask;
                chart.holds_mask |= window.chart.holds_mask;
                chart.turn_bits |= window.chart.turn_bits;
                if let Some(v) = window.accel.boost {
                    accel.boost = Some(v);
                }
                if let Some(v) = window.accel.brake {
                    accel.brake = Some(v);
                }
                if let Some(v) = window.accel.wave {
                    accel.wave = Some(v);
                }
                if let Some(v) = window.accel.expand {
                    accel.expand = Some(v);
                }
                if let Some(v) = window.accel.boomerang {
                    accel.boomerang = Some(v);
                }
                if let Some(v) = window.visual.drunk {
                    visual.drunk = Some(v);
                }
                if let Some(v) = window.visual.dizzy {
                    visual.dizzy = Some(v);
                }
                if let Some(v) = window.visual.confusion {
                    visual.confusion = Some(v);
                }
                if let Some(v) = window.visual.confusion_offset {
                    visual.confusion_offset = Some(v);
                }
                if let Some(v) = window.visual.flip {
                    visual.flip = Some(v);
                }
                if let Some(v) = window.visual.invert {
                    visual.invert = Some(v);
                }
                if let Some(v) = window.visual.tornado {
                    visual.tornado = Some(v);
                }
                if let Some(v) = window.visual.tipsy {
                    visual.tipsy = Some(v);
                }
                if let Some(v) = window.visual.bumpy {
                    visual.bumpy = Some(v);
                }
                if let Some(v) = window.visual.beat {
                    visual.beat = Some(v);
                }
                apply_appearance_target(
                    &mut appearance_target,
                    &mut appearance_speed,
                    window.appearance,
                    window.appearance_speed,
                );
                if let Some(v) = window.visibility.dark {
                    visibility.dark = Some(v);
                }
                if let Some(v) = window.visibility.blind {
                    visibility.blind = Some(v);
                }
                if let Some(v) = window.visibility.cover {
                    visibility.cover = Some(v);
                }
                if let Some(v) = window.scroll.reverse {
                    scroll.reverse = Some(v);
                }
                if let Some(v) = window.scroll.split {
                    scroll.split = Some(v);
                }
                if let Some(v) = window.scroll.alternate {
                    scroll.alternate = Some(v);
                }
                if let Some(v) = window.scroll.cross {
                    scroll.cross = Some(v);
                }
                if let Some(v) = window.scroll.centered {
                    scroll.centered = Some(v);
                }
                if let Some(v) = window.perspective.tilt {
                    perspective.tilt = Some(v);
                }
                if let Some(v) = window.perspective.skew {
                    perspective.skew = Some(v);
                }
                if let Some(speed) = window.scroll_speed {
                    scroll_speed = Some(speed);
                }
                if let Some(mini) = window.mini_percent.filter(|v| v.is_finite()) {
                    mini_percent = Some(mini.clamp(-100.0, 150.0));
                }
            }
        }
        state.attack_target_appearance[player] = appearance_target;
        state.attack_speed_appearance[player] = appearance_speed;
        approach_appearance_effects(
            &mut state.attack_current_appearance[player],
            appearance_target,
            appearance_speed,
            delta_time,
        );
        let mut appearance = state.attack_current_appearance[player];
        for window in &state.song_lua_ease_windows[player] {
            if let Some(value) = song_lua_ease_window_value(window, now) {
                song_lua_apply_eased_target(
                    window.target,
                    value,
                    &mut accel,
                    &mut visual,
                    &mut appearance,
                    &mut visibility,
                    &mut scroll,
                    &mut perspective,
                    &mut scroll_speed,
                    &mut mini_percent,
                    &mut player_x,
                    &mut player_y,
                    &mut player_z,
                    &mut player_rotation_x,
                    &mut player_rotation_z,
                    &mut player_rotation_y,
                    &mut player_skew_x,
                    &mut player_skew_y,
                    &mut player_zoom_x,
                    &mut player_zoom_y,
                    &mut player_zoom_z,
                    &mut player_confusion_y_offset,
                );
            }
        }
        if let Some(mini) = mini_percent.filter(|v| v.is_finite()) {
            mini_percent = Some(mini.clamp(-100.0, 150.0));
        }
        state.active_attack_clear_all[player] = clear_all;
        state.active_attack_chart[player] = chart;
        state.active_attack_accel[player] = accel;
        state.active_attack_visual[player] = visual;
        state.active_attack_appearance[player] = appearance;
        state.active_attack_visibility[player] = visibility;
        state.active_attack_scroll[player] = scroll;
        state.active_attack_perspective[player] = perspective;
        state.active_attack_scroll_speed[player] = scroll_speed;
        state.active_attack_mini_percent[player] = mini_percent;
        state.song_lua_player_x[player] = player_x.filter(|v| v.is_finite());
        state.song_lua_player_y[player] = player_y.filter(|v| v.is_finite());
        state.song_lua_player_z[player] = player_z.filter(|v| v.is_finite()).unwrap_or(0.0);
        state.song_lua_player_rotation_x[player] =
            player_rotation_x.filter(|v| v.is_finite()).unwrap_or(0.0);
        state.song_lua_player_rotation_z[player] =
            player_rotation_z.filter(|v| v.is_finite()).unwrap_or(0.0);
        state.song_lua_player_rotation_y[player] =
            player_rotation_y.filter(|v| v.is_finite()).unwrap_or(0.0);
        state.song_lua_player_skew_x[player] =
            player_skew_x.filter(|v| v.is_finite()).unwrap_or(0.0);
        state.song_lua_player_skew_y[player] =
            player_skew_y.filter(|v| v.is_finite()).unwrap_or(0.0);
        state.song_lua_player_zoom_x[player] =
            player_zoom_x.filter(|v| v.is_finite()).unwrap_or(1.0);
        state.song_lua_player_zoom_y[player] =
            player_zoom_y.filter(|v| v.is_finite()).unwrap_or(1.0);
        state.song_lua_player_zoom_z[player] =
            player_zoom_z.filter(|v| v.is_finite()).unwrap_or(1.0);
        state.song_lua_player_confusion_y_offset[player] = player_confusion_y_offset
            .filter(|v| v.is_finite())
            .unwrap_or(0.0);
    }
}

#[inline(always)]
fn merge_attack_value(base: f32, attack: Option<f32>) -> f32 {
    attack.filter(|v| v.is_finite()).unwrap_or(base)
}

#[inline(always)]
fn player_attack_base_cleared(state: &State, player_idx: usize) -> bool {
    player_idx < state.num_players && state.active_attack_clear_all[player_idx]
}

#[inline(always)]
pub fn effective_accel_effects_for_player(state: &State, player_idx: usize) -> AccelEffects {
    if player_idx >= state.num_players {
        return AccelEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        AccelEffects::default()
    } else {
        AccelEffects::from_mask(
            state.player_profiles[player_idx]
                .accel_effects_active_mask
                .bits(),
        )
    };
    let attack = state.active_attack_accel[player_idx];
    AccelEffects {
        boost: merge_attack_value(base.boost, attack.boost),
        brake: merge_attack_value(base.brake, attack.brake),
        wave: merge_attack_value(base.wave, attack.wave),
        expand: merge_attack_value(base.expand, attack.expand),
        boomerang: merge_attack_value(base.boomerang, attack.boomerang),
    }
}

#[inline(always)]
pub fn effective_visual_effects_for_player(state: &State, player_idx: usize) -> VisualEffects {
    if player_idx >= state.num_players {
        return VisualEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        VisualEffects::default()
    } else {
        VisualEffects::from_mask(
            state.player_profiles[player_idx]
                .visual_effects_active_mask
                .bits(),
        )
    };
    let attack = state.active_attack_visual[player_idx];
    VisualEffects {
        drunk: merge_attack_value(base.drunk, attack.drunk),
        dizzy: merge_attack_value(base.dizzy, attack.dizzy),
        confusion: merge_attack_value(base.confusion, attack.confusion),
        confusion_offset: merge_attack_value(base.confusion_offset, attack.confusion_offset),
        big: base.big,
        flip: merge_attack_value(base.flip, attack.flip),
        invert: merge_attack_value(base.invert, attack.invert),
        tornado: merge_attack_value(base.tornado, attack.tornado),
        tipsy: merge_attack_value(base.tipsy, attack.tipsy),
        bumpy: merge_attack_value(base.bumpy, attack.bumpy),
        beat: merge_attack_value(base.beat, attack.beat),
    }
}

#[inline(always)]
pub fn effective_appearance_effects_for_player(
    state: &State,
    player_idx: usize,
) -> AppearanceEffects {
    if player_idx >= state.num_players {
        return AppearanceEffects::default();
    }
    state.active_attack_appearance[player_idx]
}

#[inline(always)]
pub fn effective_visibility_effects_for_player(
    state: &State,
    player_idx: usize,
) -> VisibilityEffects {
    if player_idx >= state.num_players {
        return VisibilityEffects::default();
    }
    let attack = state.active_attack_visibility[player_idx];
    VisibilityEffects {
        dark: merge_attack_value(0.0, attack.dark),
        blind: merge_attack_value(0.0, attack.blind),
        cover: merge_attack_value(0.0, attack.cover),
    }
}

#[inline(always)]
pub fn active_chart_attack_effects_for_player(
    state: &State,
    player_idx: usize,
) -> ChartAttackEffects {
    if player_idx >= state.num_players {
        return ChartAttackEffects::default();
    }
    state.active_attack_chart[player_idx]
}

#[inline(always)]
pub fn effective_scroll_effects_for_player(state: &State, player_idx: usize) -> ScrollEffects {
    if player_idx >= state.num_players {
        return ScrollEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        ScrollEffects::default()
    } else {
        ScrollEffects::from_option(state.player_profiles[player_idx].scroll_option)
    };
    let attack = state.active_attack_scroll[player_idx];
    ScrollEffects {
        reverse: merge_attack_value(base.reverse, attack.reverse),
        split: merge_attack_value(base.split, attack.split),
        alternate: merge_attack_value(base.alternate, attack.alternate),
        cross: merge_attack_value(base.cross, attack.cross),
        centered: merge_attack_value(base.centered, attack.centered),
    }
}

#[inline(always)]
pub fn effective_perspective_effects_for_player(
    state: &State,
    player_idx: usize,
) -> PerspectiveEffects {
    if player_idx >= state.num_players {
        return PerspectiveEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        PerspectiveEffects::default()
    } else {
        PerspectiveEffects::from_perspective(state.player_profiles[player_idx].perspective)
    };
    let attack = state.active_attack_perspective[player_idx];
    PerspectiveEffects {
        tilt: merge_attack_value(base.tilt, attack.tilt),
        skew: merge_attack_value(base.skew, attack.skew),
    }
}

#[inline(always)]
pub(super) fn effective_visual_mask_for_player(state: &State, player_idx: usize) -> u16 {
    effective_visual_effects_for_player(state, player_idx).to_mask()
}

#[inline(always)]
pub fn effective_mini_percent_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return 0.0;
    }
    state.active_attack_mini_percent[player_idx]
        .filter(|v| v.is_finite())
        .unwrap_or_else(|| {
            if player_attack_base_cleared(state, player_idx) {
                0.0
            } else {
                state.player_profiles[player_idx].mini_percent as f32
            }
        })
}

/// Multiplier applied to the noteskin's per-column lateral offsets to
/// realise the Spacing player option (zmod parity, proportional model).
/// `1.0 + spacing_percent / 100`.
#[inline(always)]
pub fn effective_spacing_multiplier_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return 1.0;
    }
    spacing_multiplier_for_percent(state.player_profiles[player_idx].spacing_percent)
}

#[inline(always)]
pub fn spacing_multiplier_for_percent(spacing_percent: i32) -> f32 {
    1.0 + (spacing_percent.clamp(profile::SPACING_PERCENT_MIN, profile::SPACING_PERCENT_MAX) as f32)
        / 100.0
}

#[inline(always)]
pub fn effective_scroll_speed_for_player(state: &State, player_idx: usize) -> ScrollSpeedSetting {
    if player_idx >= state.num_players {
        return ScrollSpeedSetting::default();
    }
    state.active_attack_scroll_speed[player_idx].unwrap_or_else(|| {
        if player_attack_base_cleared(state, player_idx) {
            ScrollSpeedSetting::default()
        } else {
            state.scroll_speed[player_idx]
        }
    })
}
