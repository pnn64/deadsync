//! Per-frame diff that turns gameplay judgement and hold state into SMX panel light events.
//!
//! Runs on the app update path (sibling to the cabinet `GameplayLightTracker`). It only
//! compares small per-column state and pushes O(1) events to the SMX lights worker, so no
//! frame building or colour math happens on the gameplay/render path.

use deadsync_rules::note::HoldResult;

use crate::engine::lights::smx_panels::{
    FLASH_SECONDS_JUDGMENT, Rgb, SmxPanelLights, flash_color, flash_duration, smx_panel_for_col,
};
use crate::game::gameplay::{State, active_hold_is_engaged};

const MAX_COLS: usize = deadsync_core::input::MAX_COLS;

/// Sentinel `started_at_screen_s` meaning "nothing seen yet for this column".
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
pub struct SmxPanelDriver {
    lights: SmxPanelLights,
    active: bool,
    notes_ptr: usize,
    prev_flash: [f32; MAX_COLS],
    prev_engaged: [bool; MAX_COLS],
    prev_hold_judged: [f32; MAX_COLS],
    prev_mine: [bool; MAX_COLS],
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
            prev_mine: [false; MAX_COLS],
        }
    }
}

impl SmxPanelDriver {
    /// Called each frame while on a gameplay screen with the feature enabled. Diffs the
    /// per-column flash, active-hold, hold-judgement, and mine state and emits panel events.
    pub fn update(&mut self, state: &State) {
        // Re-arm on entering gameplay or when the chart's note buffer changes (a restart or
        // a new song), so stale `started_at_screen_s` values do not swallow the first event.
        let notes_ptr = state.notes.as_ptr() as usize;
        if !self.active || notes_ptr != self.notes_ptr {
            self.activate(notes_ptr);
        }

        let cpp = state.cols_per_player;
        let np = state.num_players;
        let cols = cpp.saturating_mul(np).min(MAX_COLS);
        for col in 0..cols {
            self.tap(state, cpp, np, col);
            self.hold(state, cpp, np, col);
            self.hold_outcome(state, cpp, np, col);
            self.mine(state, cpp, np, col);
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
        self.prev_mine = [false; MAX_COLS];
        self.lights.set_active(true);
    }

    fn tap(&mut self, state: &State, cpp: usize, np: usize, col: usize) {
        match state.last_tap_judgments[col] {
            Some(judged) if judged.at_screen_s != self.prev_flash[col] => {
                self.prev_flash[col] = judged.at_screen_s;
                if let Some((pad, panel)) = smx_panel_for_col(cpp, np, col) {
                    self.lights.flash(
                        pad,
                        panel,
                        flash_color(judged.grade, judged.blue_fantastic),
                        flash_duration(judged.grade),
                    );
                }
            }
            None => self.prev_flash[col] = NO_EVENT,
            _ => {}
        }
    }

    fn hold(&mut self, state: &State, cpp: usize, np: usize, col: usize) {
        let engaged = state.active_holds[col]
            .as_ref()
            .is_some_and(active_hold_is_engaged);
        if engaged == self.prev_engaged[col] {
            return;
        }
        self.prev_engaged[col] = engaged;
        let Some((pad, panel)) = smx_panel_for_col(cpp, np, col) else {
            return;
        };
        if engaged {
            self.lights.hold_start(pad, panel, HOLD_RGB);
        } else {
            self.lights.hold_end(pad, panel);
        }
    }

    fn hold_outcome(&mut self, state: &State, cpp: usize, np: usize, col: usize) {
        match state.hold_judgments[col] {
            Some(judged) if judged.started_at_screen_s != self.prev_hold_judged[col] => {
                self.prev_hold_judged[col] = judged.started_at_screen_s;
                let color = match judged.result {
                    HoldResult::Held => HOLD_OK_RGB,
                    HoldResult::LetGo => HOLD_DROP_RGB,
                    HoldResult::Missed => return,
                };
                if let Some((pad, panel)) = smx_panel_for_col(cpp, np, col) {
                    self.lights.flash(pad, panel, color, FLASH_SECONDS_JUDGMENT);
                }
            }
            None => self.prev_hold_judged[col] = NO_EVENT,
            _ => {}
        }
    }

    fn mine(&mut self, state: &State, cpp: usize, np: usize, col: usize) {
        // `mine_explosions[col]` is set (ungated) when a mine is hit and clears when the
        // explosion finishes, so a None->Some transition is a fresh hit.
        let hit = state.mine_explosions[col].is_some();
        if hit && !self.prev_mine[col] {
            if let Some((pad, panel)) = smx_panel_for_col(cpp, np, col) {
                self.lights.flash(pad, panel, MINE_RGB, MINE_FLASH_SECONDS);
            }
        }
        self.prev_mine[col] = hit;
    }
}
