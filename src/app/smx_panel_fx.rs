//! Per-frame diff that turns gameplay judgement and hold state into SMX panel light events.
//!
//! Runs on the app update path (sibling to the cabinet `GameplayLightTracker`). It only
//! compares small per-column state and pushes O(1) events to the SMX lights worker, so no
//! frame building or colour math happens on the gameplay/render path. The per-column
//! decisions are pure helpers (`tap_flash`, `hold_edge`, `hold_outcome_flash`, `mine_flash`)
//! so they can be unit-tested without constructing a whole gameplay `State`.

use deadsync_profile::{PlayStyle, PlayerSide, player_side_index, runtime_player_side};
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::note::HoldResult;
use deadsync_smx::panels::{PADS, Rgb, SmxPanelLights, smx_panel_for_col};

use crate::game::gameplay::{
    ColumnTapJudgment, HoldJudgmentRenderInfo, State, active_hold, active_hold_is_engaged,
    hold_judgment, last_tap_judgment, mine_started_at_screen_s,
};
use crate::game::profile;

const MAX_COLS: usize = deadsync_core::input::MAX_COLS;

/// Translate the chart-layout pad index from `smx_panel_for_col` (0 = first side,
/// 1 = second side) to the physical SMX slot the player's pad occupies.
///
/// A single player is always packed at chart pad 0 (`runtime_player_index`), but
/// may have joined as P2 - and a lone pad assigned to P2 sits at SDK slot 1 - so we
/// route by the runtime side, the same basis input uses (keymaps bind `P2_*` to
/// slot 1). Without this, a single P2 player's judgements would light frame pad 0
/// (slot 0, no pad) while the pad they stand on stays dark. Doubles drives both
/// pads (left = slot 0, right = slot 1), so it stays identity.
fn physical_slot(
    play_style: PlayStyle,
    session_side: PlayerSide,
    doubles: bool,
    chart_pad: usize,
) -> usize {
    if doubles {
        chart_pad
    } else {
        player_side_index(runtime_player_side(play_style, session_side, chart_pad))
    }
}

/// Sentinel `*_at_screen_s` meaning "nothing seen yet for this column".
const NO_EVENT: f32 = f32::NEG_INFINITY;

/// Tap flash durations, matching the on-screen column flash (`gameplay.rs:650`).
const FLASH_SECONDS_MISS: f32 = 0.16;
const FLASH_SECONDS_JUDGMENT: f32 = 0.33;
/// Fantastic colour for the bright FA+ inner window.
const PAD_FANTASTIC_WHITE: Rgb = [255, 255, 255];
/// Sustained colour shown while a freeze or roll is held (steady teal).
const HOLD_RGB: Rgb = [0, 160, 160];
/// Flash shown when a freeze or roll is completed.
const HOLD_OK_RGB: Rgb = [0, 220, 0];
/// Flash shown when a freeze or roll is dropped.
const HOLD_DROP_RGB: Rgb = [255, 0, 0];
/// Flash shown when a mine is hit (magenta, distinct from the warm grade colours).
const MINE_RGB: Rgb = [255, 0, 180];
/// Duration of the mine-hit flash.
const MINE_FLASH_SECONDS: f32 = 0.25;

/// Per-grade panel colour. Tuned for the SMX LED diffuser (saturated, well-separated hues)
/// rather than reusing the on-screen palette, which washes out on the pad; the SDK scales
/// output by ~0.67 on send. A `match` (not a table indexed by `judge_grade_ix`) so adding a
/// `JudgeGrade` is a compile error here instead of a silent out-of-range panic.
fn pad_grade_color(grade: JudgeGrade) -> Rgb {
    match grade {
        JudgeGrade::Fantastic => [0, 90, 255], // blue (white for the FA+ inner window)
        JudgeGrade::Excellent => [255, 140, 0], // orange
        JudgeGrade::Great => [0, 220, 0],      // green
        JudgeGrade::Decent => [170, 0, 255],   // purple
        JudgeGrade::WayOff => [255, 230, 0],   // yellow
        JudgeGrade::Miss => [255, 0, 0],       // red
    }
}

/// Flash duration for a judgement grade.
fn flash_duration(grade: JudgeGrade) -> f32 {
    match grade {
        JudgeGrade::Miss => FLASH_SECONDS_MISS,
        _ => FLASH_SECONDS_JUDGMENT,
    }
}

/// Colour for a tap judgement flash.
///
/// `blue_fantastic` is the flag gameplay records on `ActiveColumnFlash` (`gameplay.rs:6772`):
/// `true` for the blue (outer) Fantastic, `false` for the bright FA+ inner window (white). All
/// other grades use the pad palette. The pad uses its own saturated palette rather than the
/// on-screen colours, which wash out on the LED diffuser.
fn flash_color(grade: JudgeGrade, blue_fantastic: bool) -> Rgb {
    if grade == JudgeGrade::Fantastic && !blue_fantastic {
        PAD_FANTASTIC_WHITE
    } else {
        pad_grade_color(grade)
    }
}

/// Owns the panel-lighting worker handle plus the per-column "last seen" trackers used to
/// detect new judgements and hold transitions by diffing gameplay `State` each frame.
///
/// `Default::default()` spawns the worker thread (via `SmxPanelLights::new`); construct one
/// per `App` and keep it for the app's lifetime.
pub struct SmxPanelDriver {
    lights: SmxPanelLights,
    active: bool,
    notes_ptr: usize,
    /// Chart pad index -> physical SMX slot, resolved once per activation from the
    /// session play style and side so the per-frame loop stays branch-free.
    slot_for_pad: [usize; PADS],
    prev_flash: [f32; MAX_COLS],
    prev_engaged: [bool; MAX_COLS],
    prev_hold_judged: [f32; MAX_COLS],
    prev_mine: [f32; MAX_COLS],
}

impl Default for SmxPanelDriver {
    fn default() -> Self {
        Self {
            lights: SmxPanelLights::new(),
            active: false,
            notes_ptr: 0,
            slot_for_pad: std::array::from_fn(|pad| pad),
            prev_flash: [NO_EVENT; MAX_COLS],
            prev_engaged: [false; MAX_COLS],
            prev_hold_judged: [NO_EVENT; MAX_COLS],
            prev_mine: [NO_EVENT; MAX_COLS],
        }
    }
}

impl SmxPanelDriver {
    /// Called each frame while on a gameplay screen with the feature enabled. Diffs the
    /// per-column flash, active-hold, hold-judgement, and mine state and emits panel events.
    pub fn update(&mut self, state: &State) {
        // Re-arm on entering gameplay or when the chart's note buffer changes (a restart or
        // a new song), so stale `*_at_screen_s` values do not swallow the first event.
        let notes_ptr = crate::game::gameplay::notes(state).as_ptr() as usize;
        if !self.active || notes_ptr != self.notes_ptr {
            self.activate(state);
        }

        let cpp = crate::game::gameplay::cols_per_player(state);
        let np = crate::game::gameplay::num_players(state);
        let cols = cpp.saturating_mul(np).min(MAX_COLS);
        for col in 0..cols {
            // Resolve the panel once and translate the chart pad index to the physical SMX
            // slot the player's pad sits on (a single P2 player packs at chart pad 0 but
            // stands on slot 1). The trackers below still update even when a column maps to
            // no panel, so events are consumed rather than replayed later.
            let panel = smx_panel_for_col(cpp, np, col).map(|(pad, p)| (self.slot_for_pad[pad], p));

            if let Some((color, dur)) =
                tap_flash(last_tap_judgment(state, col), &mut self.prev_flash[col])
            {
                if let Some((pad, p)) = panel {
                    self.lights.flash(pad, p, color, dur);
                }
            }

            let engaged = active_hold(state, col).is_some_and(active_hold_is_engaged);
            if let Some(now_engaged) = hold_edge(engaged, &mut self.prev_engaged[col]) {
                if let Some((pad, p)) = panel {
                    if now_engaged {
                        self.lights.hold_start(pad, p, HOLD_RGB);
                    } else {
                        self.lights.hold_end(pad, p);
                    }
                }
            }

            if let Some(color) =
                hold_outcome_flash(hold_judgment(state, col), &mut self.prev_hold_judged[col])
            {
                if let Some((pad, p)) = panel {
                    self.lights.flash(pad, p, color, FLASH_SECONDS_JUDGMENT);
                }
            }

            let mine_at = mine_started_at_screen_s(state, col);
            if let Some(color) = mine_flash(mine_at, &mut self.prev_mine[col]) {
                if let Some((pad, p)) = panel {
                    self.lights.flash(pad, p, color, MINE_FLASH_SECONDS);
                }
            }
        }
    }

    /// Called when leaving gameplay or when the feature is disabled. Clears the panels and
    /// hands the pad back to its firmware idle lighting.
    pub fn deactivate(&mut self) {
        if self.active {
            self.active = false;
            self.lights.set_active(false);
        }
    }

    fn activate(&mut self, state: &State) {
        self.active = true;
        self.notes_ptr = crate::game::gameplay::notes(state).as_ptr() as usize;
        // Resolve the chart-pad -> physical-slot map once per song (the session play style
        // and side are fixed for the run), keeping the per-frame loop off the session lock.
        let play_style = profile::get_session_play_style();
        let session_side = profile::get_session_player_side();
        let doubles = crate::game::gameplay::cols_per_player(state) >= 8
            && crate::game::gameplay::num_players(state) == 1;
        self.slot_for_pad =
            std::array::from_fn(|pad| physical_slot(play_style, session_side, doubles, pad));
        self.prev_flash = [NO_EVENT; MAX_COLS];
        self.prev_engaged = [false; MAX_COLS];
        self.prev_hold_judged = [NO_EVENT; MAX_COLS];
        self.prev_mine = [NO_EVENT; MAX_COLS];
        self.lights.set_active(true);
    }
}

/// Decide a tap flash for a column. Records the judgement time so the same one is not
/// re-flashed, and re-arms (sentinel) when the column currently has no judgement.
fn tap_flash(judged: Option<ColumnTapJudgment>, prev: &mut f32) -> Option<(Rgb, f32)> {
    match judged {
        Some(j) if j.at_screen_s != *prev => {
            *prev = j.at_screen_s;
            Some((
                flash_color(j.grade, j.blue_fantastic),
                flash_duration(j.grade),
            ))
        }
        None => {
            *prev = NO_EVENT;
            None
        }
        _ => None,
    }
}

/// Decide a freeze/roll engage transition: `Some(true)` to start the sustained colour,
/// `Some(false)` to clear it, `None` when nothing changed.
fn hold_edge(engaged: bool, prev: &mut bool) -> Option<bool> {
    if engaged == *prev {
        None
    } else {
        *prev = engaged;
        Some(engaged)
    }
}

/// Decide a freeze/roll outcome flash. Held flashes OK, dropped flashes drop, missed
/// consumes the event but shows nothing.
fn hold_outcome_flash(judged: Option<HoldJudgmentRenderInfo>, prev: &mut f32) -> Option<Rgb> {
    match judged {
        Some(j) if j.started_at_screen_s != *prev => {
            *prev = j.started_at_screen_s;
            match j.result {
                HoldResult::Held => Some(HOLD_OK_RGB),
                HoldResult::LetGo => Some(HOLD_DROP_RGB),
                HoldResult::Missed => None,
            }
        }
        None => {
            *prev = NO_EVENT;
            None
        }
        _ => None,
    }
}

/// Decide a mine-hit flash, keyed by hit time so a second hit on the same column while an
/// earlier explosion is still active is still caught.
fn mine_flash(hit_at: Option<f32>, prev: &mut f32) -> Option<Rgb> {
    match hit_at {
        Some(ts) if ts != *prev => {
            *prev = ts;
            Some(MINE_RGB)
        }
        None => {
            *prev = NO_EVENT;
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(grade: JudgeGrade, blue_fantastic: bool, at: f32) -> ColumnTapJudgment {
        ColumnTapJudgment {
            grade,
            blue_fantastic,
            at_screen_s: at,
        }
    }

    fn hold(result: HoldResult, at: f32) -> HoldJudgmentRenderInfo {
        HoldJudgmentRenderInfo {
            result,
            started_at_screen_s: at,
        }
    }

    #[test]
    fn physical_slot_routes_single_p2_to_slot_1() {
        use PlayStyle::*;
        use PlayerSide::*;
        // Single player on either side packs at chart pad 0, but the slot follows the
        // side they joined: P1 -> slot 0, P2 -> slot 1 (the lone pad relocated to slot 1).
        assert_eq!(physical_slot(Single, P1, false, 0), 0);
        assert_eq!(physical_slot(Single, P2, false, 0), 1);
        // Versus: chart pad already equals the slot (P1 left pad, P2 right pad), unchanged.
        assert_eq!(physical_slot(Versus, P1, false, 0), 0);
        assert_eq!(physical_slot(Versus, P1, false, 1), 1);
        // Doubles owns both pads regardless of side, so left/right stay slot 0/1.
        assert_eq!(physical_slot(Double, P2, true, 0), 0);
        assert_eq!(physical_slot(Double, P2, true, 1), 1);
    }

    #[test]
    fn tap_flash_fires_once_per_new_judgment() {
        let mut prev = NO_EVENT;
        // First judgement fires and records its time.
        assert!(tap_flash(Some(tap(JudgeGrade::Great, false, 1.0)), &mut prev).is_some());
        assert_eq!(prev, 1.0);
        // Same timestamp does not re-fire.
        assert!(tap_flash(Some(tap(JudgeGrade::Great, false, 1.0)), &mut prev).is_none());
        // A new timestamp fires again.
        assert!(tap_flash(Some(tap(JudgeGrade::Miss, false, 2.0)), &mut prev).is_some());
        assert_eq!(prev, 2.0);
    }

    #[test]
    fn tap_flash_none_rearms() {
        let mut prev = 5.0;
        assert!(tap_flash(None, &mut prev).is_none());
        assert_eq!(prev, NO_EVENT);
        // After re-arm a judgement at any time reads as new.
        assert!(tap_flash(Some(tap(JudgeGrade::Decent, false, 0.0)), &mut prev).is_some());
    }

    #[test]
    fn tap_flash_color_and_duration_match_grade() {
        let mut prev = NO_EVENT;
        let (color, dur) = tap_flash(Some(tap(JudgeGrade::Miss, false, 1.0)), &mut prev).unwrap();
        assert_eq!(color, flash_color(JudgeGrade::Miss, false));
        assert_eq!(dur, flash_duration(JudgeGrade::Miss));
    }

    #[test]
    fn flash_color_uses_pad_palette() {
        use JudgeGrade::*;
        // Normal (blue) Fantastic uses the blue pad colour, not white.
        assert_eq!(flash_color(Fantastic, true), pad_grade_color(Fantastic));
        // Bright (inner FA+) Fantastic is white.
        assert_eq!(flash_color(Fantastic, false), PAD_FANTASTIC_WHITE);
        // Other grades use their pad palette colour.
        assert_eq!(flash_color(Miss, false), pad_grade_color(Miss));
        // Every grade colour is distinct so judgements stay readable on the pad.
        let all = [Fantastic, Excellent, Great, Decent, WayOff, Miss];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(pad_grade_color(all[i]), pad_grade_color(all[j]));
            }
        }
    }

    #[test]
    fn flash_duration_miss_is_shorter() {
        assert_eq!(flash_duration(JudgeGrade::Miss), FLASH_SECONDS_MISS);
        assert_eq!(
            flash_duration(JudgeGrade::Fantastic),
            FLASH_SECONDS_JUDGMENT
        );
        assert_eq!(flash_duration(JudgeGrade::WayOff), FLASH_SECONDS_JUDGMENT);
    }

    #[test]
    fn hold_edge_reports_only_transitions() {
        let mut prev = false;
        assert_eq!(hold_edge(false, &mut prev), None);
        assert_eq!(hold_edge(true, &mut prev), Some(true));
        assert_eq!(hold_edge(true, &mut prev), None);
        assert_eq!(hold_edge(false, &mut prev), Some(false));
    }

    #[test]
    fn hold_outcome_flash_maps_result() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_flash(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            Some(HOLD_OK_RGB)
        );
        // Drop at a new time.
        assert_eq!(
            hold_outcome_flash(Some(hold(HoldResult::LetGo, 2.0)), &mut prev),
            Some(HOLD_DROP_RGB)
        );
        // Missed consumes the event (records its time) but shows nothing.
        assert_eq!(
            hold_outcome_flash(Some(hold(HoldResult::Missed, 3.0)), &mut prev),
            None
        );
        assert_eq!(prev, 3.0);
    }

    #[test]
    fn hold_outcome_flash_ignores_repeat_and_rearms() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_flash(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            Some(HOLD_OK_RGB)
        );
        assert_eq!(
            hold_outcome_flash(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            None
        );
        assert_eq!(hold_outcome_flash(None, &mut prev), None);
        assert_eq!(prev, NO_EVENT);
    }

    #[test]
    fn mine_flash_catches_consecutive_hits() {
        let mut prev = NO_EVENT;
        // First hit.
        assert_eq!(mine_flash(Some(1.0), &mut prev), Some(MINE_RGB));
        // Same explosion (same hit time) does not re-fire.
        assert_eq!(mine_flash(Some(1.0), &mut prev), None);
        // A second hit while the first explosion may still be active re-fires.
        assert_eq!(mine_flash(Some(1.5), &mut prev), Some(MINE_RGB));
        // Explosion ended; re-arm.
        assert_eq!(mine_flash(None, &mut prev), None);
        assert_eq!(prev, NO_EVENT);
    }
}
