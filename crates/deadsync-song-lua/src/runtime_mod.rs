use std::collections::HashMap;

use crate::LUA_PLAYERS;
use crate::SongLuaCompileInfo;
use crate::SongLuaEaseTarget;
use crate::SongLuaEaseWindow;

use crate::SongLuaTimeUnit;

use crate::push_unique_compile_detail;

#[derive(Clone)]
pub struct RuntimeModEaseEntry {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub easing: String,
    pub to: f32,
    pub target: String,
    pub start_val: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
    pub player: Option<u8>,
    pub add: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeOverlayCaptureKey {
    pub function: usize,
    pub unit: SongLuaTimeUnit,
    pub start: u32,
    pub limit: u32,
    pub easing: String,
    pub target: String,
    pub from: u32,
    pub to: u32,
    pub opt1: Option<u32>,
    pub opt2: Option<u32>,
}

/// Collects exact-unique runtime-mod entries in first-seen order. The
/// compilation-only lookup stays inline through 1,024 source entries and
/// retains at most 256 output slots before measured growth is necessary.
#[must_use]
pub fn runtime_mod_entry_players(player: Option<u8>) -> Vec<usize> {
    runtime_mod_player_indices(player).collect()
}

fn runtime_mod_player_indices(player: Option<u8>) -> std::ops::Range<usize> {
    match player {
        Some(player) if (1..=LUA_PLAYERS as u8).contains(&player) => {
            let player = (player - 1) as usize;
            player..player + 1
        }
        _ => 0..LUA_PLAYERS,
    }
}

#[must_use]
pub fn runtime_mod_key(target: &str) -> String {
    target.to_ascii_lowercase()
}

fn runtime_mod_initial_value(key: &str) -> f32 {
    if matches!(key, "zoom" | "zoomx" | "zoomy" | "zoomz") {
        1.0
    } else {
        0.0
    }
}

pub fn runtime_mod_start_value(
    current: &mut HashMap<String, f32>,
    key: &str,
    entry: &RuntimeModEaseEntry,
) -> f32 {
    entry.start_val.unwrap_or_else(|| {
        *current
            .entry(key.to_string())
            .or_insert_with(|| runtime_mod_initial_value(key))
    })
}

#[must_use]
pub fn runtime_mod_end_value(from: f32, entry: &RuntimeModEaseEntry) -> f32 {
    if entry.add { from + entry.to } else { entry.to }
}

fn runtime_mod_column_key(key: &str, prefix: &str) -> bool {
    key.strip_prefix(prefix)
        .and_then(|suffix| suffix.parse::<usize>().ok())
        .is_some_and(|column| (1..=16).contains(&column))
}

#[must_use]
pub fn runtime_player_option_ease_target(key: &str, original: &str) -> Option<SongLuaEaseTarget> {
    if runtime_mod_column_key(key, "stealth")
        || runtime_mod_column_key(key, "bumpy")
        || runtime_mod_column_key(key, "dark")
        || runtime_mod_column_key(key, "tiny")
        || runtime_mod_column_key(key, "movex")
        || runtime_mod_column_key(key, "movey")
        || runtime_mod_column_key(key, "movez")
        || runtime_mod_column_key(key, "confusionoffset")
    {
        return Some(SongLuaEaseTarget::Mod(original.to_string()));
    }
    // Actor transforms are owned by the authored node function. In Xero they
    // can target Player, NoteField, or a wrapper; they are not PlayerOptions.
    Some(match key {
        "boost"
        | "brake"
        | "wave"
        | "waveperiod"
        | "expand"
        | "expandperiod"
        | "noattack"
        | "randattack"
        | "noattacks"
        | "randomattacks"
        | "boomerang"
        | "drunk"
        | "drunkoffset"
        | "drunkspeed"
        | "drunkperiod"
        | "tipsyoffset"
        | "tipsyspeed"
        | "dizzy"
        | "twirl"
        | "roll"
        | "failsetting"
        | "modtimersetting"
        | "modtimermult"
        | "modtimeroffset"
        | "parabolax"
        | "attenuatex"
        | "parabolay"
        | "attenuatey"
        | "bumpyx"
        | "bumpyxoffset"
        | "bumpyxperiod"
        | "tanbumpy"
        | "tanbumpyoffset"
        | "tanbumpyperiod"
        | "tanbumpyx"
        | "tanbumpyxoffset"
        | "tanbumpyxperiod"
        | "drunkz"
        | "drunkzoffset"
        | "drunkzspeed"
        | "drunkzperiod"
        | "tandrunk"
        | "tandrunkoffset"
        | "tandrunkspeed"
        | "tandrunkperiod"
        | "tandrunkz"
        | "tandrunkzoffset"
        | "tandrunkzspeed"
        | "tandrunkzperiod"
        | "dizzyholds"
        | "zbuffer"
        | "cosecant"
        | "drawsize"
        | "drawsizeback"
        | "digital"
        | "digitalsteps"
        | "digitaloffset"
        | "digitalperiod"
        | "zigzag"
        | "zigzagz"
        | "zigzagoffset"
        | "zigzagzoffset"
        | "zigzagperiod"
        | "zigzagzperiod"
        | "square"
        | "squareoffset"
        | "squareperiod"
        | "squarez"
        | "squarezoffset"
        | "squarezperiod"
        | "xmode"
        | "bounce"
        | "bounceperiod"
        | "bounceoffset"
        | "tornadoperiod"
        | "tornadooffset"
        | "parabolaz"
        | "attenuatez"
        | "confusion"
        | "confusionoffset"
        | "confusionxoffset"
        | "confusionx"
        | "confusiony"
        | "flip"
        | "invert"
        | "tornado"
        | "tipsy"
        | "bumpy"
        | "bumpyoffset"
        | "bumpyperiod"
        | "pulseinner"
        | "pulseouter"
        | "pulseperiod"
        | "pulseoffset"
        | "beat"
        | "beatperiod"
        | "shrinklinear"
        | "shrinkmult"
        | "bouncez"
        | "bouncezoffset"
        | "bouncezperiod"
        | "digitalz"
        | "digitalzoffset"
        | "digitalzperiod"
        | "digitalzsteps"
        | "tornadoz"
        | "tornadozoffset"
        | "tornadozperiod"
        | "sawtooth"
        | "sawtoothperiod"
        | "sawtoothz"
        | "sawtoothzperiod"
        | "beatoffset"
        | "beatmult"
        | "beaty"
        | "beatyoffset"
        | "beatymult"
        | "beatyperiod"
        | "beatz"
        | "beatzoffset"
        | "beatzmult"
        | "beatzperiod"
        | "randomspeed"
        | "hidden"
        | "hiddenoffset"
        | "sudden"
        | "suddenoffset"
        | "stealth"
        | "stealthtype"
        | "stealthpastreceptors"
        | "blink"
        | "rvanish"
        | "randomvanish"
        | "reversevanish"
        | "dark"
        | "blind"
        | "cover"
        | "reverse"
        | "split"
        | "alternate"
        | "cross"
        | "centered"
        | "tilt"
        | "skew"
        | "incoming"
        | "space"
        | "hallway"
        | "distant"
        | "overhead"
        | "xmod"
        | "cmod"
        | "mmod"
        | "tiny"
        | "mini"
        | "confusionyoffset" => SongLuaEaseTarget::Mod(original.to_string()),
        _ => return None,
    })
}

pub fn extend_runtime_mod_sustains(windows: &mut [SongLuaEaseWindow]) {
    const DEFAULT_SUSTAIN_BEATS: f32 = 1_000_000.0;
    const SAME_TICK_EPSILON: f32 = 0.001;

    // Typical and unusually dense mod charts fit inline, including 7th Gear.
    // Pathological charts spill once during compilation, then release the index.
    let mut order = smallvec::SmallVec::<[usize; 1024]>::with_capacity(windows.len());
    order.extend(0..windows.len());
    order.sort_unstable_by(|&left, &right| {
        windows[left]
            .player
            .cmp(&windows[right].player)
            .then_with(|| windows[left].target.cmp(&windows[right].target))
            .then_with(|| windows[left].start.total_cmp(&windows[right].start))
            .then_with(|| left.cmp(&right))
    });

    let mut group_start = 0;
    while group_start < order.len() {
        let first = order[group_start];
        let mut group_end = group_start + 1;
        while group_end < order.len()
            && windows[order[group_end]].player == windows[first].player
            && windows[order[group_end]].target == windows[first].target
        {
            group_end += 1;
        }
        for position in group_start..group_end {
            let index = order[position];
            let start = windows[index].start;
            let threshold = start + SAME_TICK_EPSILON;
            let later = &order[position + 1..group_end];
            let next = later.partition_point(|&other| windows[other].start <= threshold);
            let next_start = later
                .get(next)
                .map_or(DEFAULT_SUSTAIN_BEATS, |&other| windows[other].start);
            let end = start + windows[index].limit;
            if next_start > end + SAME_TICK_EPSILON {
                windows[index].sustain = Some(next_start - end);
            }
        }
        group_start = group_end;
    }
}

pub fn record_unsupported_xero_overlay_function_ease(
    info: &mut SongLuaCompileInfo,
    entry: &RuntimeModEaseEntry,
    from: f32,
    to: f32,
    probe_methods: &[String],
) -> String {
    info.unsupported_function_eases += 1;
    let detail = format!(
        "xero node '{}' unit={:?} start={:.3} limit={:.3} from={:.3} to={:.3} \
         easing={:?} probe_methods={:?}",
        entry.target, entry.unit, entry.start, entry.limit, from, to, entry.easing, probe_methods,
    );
    push_unique_compile_detail(&mut info.unsupported_function_ease_captures, detail.clone());
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_player_list_keeps_invalid_player_fallback_behavior() {
        assert_eq!(runtime_mod_entry_players(Some(1)), vec![0]);
        assert_eq!(runtime_mod_entry_players(Some(2)), vec![1]);
        assert_eq!(runtime_mod_entry_players(None), vec![0, 1]);
        assert_eq!(runtime_mod_entry_players(Some(0)), vec![0, 1]);
        assert_eq!(runtime_mod_entry_players(Some(3)), vec![0, 1]);
    }
}
