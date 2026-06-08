//! Per-frame diff that turns gameplay judgement and hold state into SMX panel light events.
//!
//! Runs on the app update path (sibling to the cabinet `GameplayLightTracker`). It only
//! compares small per-column state and pushes O(1) events to the SMX lights worker, so no
//! frame building or colour math happens on the gameplay/render path. The per-column
//! decisions are pure helpers (`tap_flash`, `hold_edge`, `hold_outcome_flash`, `mine_flash`)
//! so they can be unit-tested without constructing a whole gameplay `State`.

use deadsync_rules::note::HoldResult;

use crate::engine::lights::smx_panels::{
    FLASH_SECONDS_JUDGMENT, Rgb, SmxPanelLights, flash_color, flash_duration, smx_panel_for_col,
};
use crate::game::gameplay::{
    ColumnTapJudgment, HoldJudgmentRenderInfo, State, active_hold_is_engaged,
};

const MAX_COLS: usize = deadsync_core::input::MAX_COLS;

/// Sentinel `*_at_screen_s` meaning "nothing seen yet for this column".
const NO_EVENT: f32 = f32::NEG_INFINITY;

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

/// Owns the panel-lighting worker handle plus the per-column "last seen" trackers used to
/// detect new judgements and hold transitions by diffing gameplay `State` each frame.
///
/// `Default::default()` spawns the worker thread (via `SmxPanelLights::new`); construct one
/// per `App` and keep it for the app's lifetime.
pub struct SmxPanelDriver {
    lights: SmxPanelLights,
    active: bool,
    notes_ptr: usize,
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
        let notes_ptr = state.notes.as_ptr() as usize;
        if !self.active || notes_ptr != self.notes_ptr {
            self.activate(notes_ptr);
        }

        let cpp = state.cols_per_player;
        let np = state.num_players;
        let cols = cpp.saturating_mul(np).min(MAX_COLS);
        for col in 0..cols {
            // Resolve the pad/panel once. The trackers below still update even when a column
            // maps to no panel, so events are consumed rather than replayed later.
            let panel = smx_panel_for_col(cpp, np, col);

            if let Some((color, dur)) =
                tap_flash(state.last_tap_judgments[col], &mut self.prev_flash[col])
            {
                if let Some((pad, p)) = panel {
                    self.lights.flash(pad, p, color, dur);
                }
            }

            let engaged = state.active_holds[col]
                .as_ref()
                .is_some_and(active_hold_is_engaged);
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
                hold_outcome_flash(state.hold_judgments[col], &mut self.prev_hold_judged[col])
            {
                if let Some((pad, p)) = panel {
                    self.lights.flash(pad, p, color, FLASH_SECONDS_JUDGMENT);
                }
            }

            let mine_at = state.mine_explosions[col]
                .as_ref()
                .map(|m| m.started_at_screen_s);
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

    fn activate(&mut self, notes_ptr: usize) {
        self.active = true;
        self.notes_ptr = notes_ptr;
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
    use deadsync_rules::judgment::JudgeGrade;

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
