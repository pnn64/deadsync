//! Per-frame diff that turns gameplay judgment and hold state into SMX panel light events.
//!
//! Runs on the app update path (sibling to the cabinet `GameplayLightTracker`). It only
//! compares small per-column state and pushes O(1) events to the SMX lights worker, so no
//! frame building or colour math happens on the gameplay/render path. The per-column
//! decisions are pure helpers (`tap_flash`, `hold_edge`, `hold_outcome_flash`, `mine_flash`)
//! so they can be unit-tested without constructing a whole gameplay `State`.

use std::sync::Arc;

use crate::gifs::FullPadAnim;
use crate::panel_fx::{
    JudgementGifs, NO_EVENT, hold_edge, hold_outcome_event, mine_event, sustain_anim,
    sustain_drive, tap_event,
};
use crate::panels::{Clock, OverlayDrive, PADS, SmxPanelLights, smx_panel_for_col};
use deadsync_core::note::NoteType;
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState, active_hold_is_engaged};
use deadsync_rules::note::HoldResult;

const MAX_COLS: usize = deadsync_core::input::MAX_COLS;

/// Whether `update` started a gif overlay on a column's panel for the current sustain,
/// so the matching release is sent when it disengages.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum HoldFx {
    #[default]
    None,
    Overlay,
}

/// Owns the panel-lighting worker handle plus the per-column "last seen" trackers used to
/// detect new judgements and hold transitions by diffing gameplay `State` each frame.
///
/// `Default::default()` spawns the worker thread (via `SmxPanelLights::new`); construct one
/// per `App` and keep it for the app's lifetime.
pub struct SmxPanelDriver {
    lights: SmxPanelLights,
    /// Gameplay judgement effects are running (the original "active").
    gameplay_active: bool,
    /// The worker owns the pad lights (gameplay effects or a background).
    worker_active: bool,
    /// Per-slot background currently applied to the worker, for change detection.
    backgrounds: [Option<(Arc<FullPadAnim>, Clock)>; PADS],
    /// Per-slot judgement animations; empty (all `None`) means no panel effects at all.
    judgement_gifs: [JudgementGifs; PADS],
    notes_ptr: usize,
    /// Chart pad index -> physical SMX slot, resolved once per activation from the
    /// session play style and side so the per-frame loop stays branch-free.
    slot_for_pad: [usize; PADS],
    prev_flash: [f32; MAX_COLS],
    prev_engaged: [bool; MAX_COLS],
    prev_hold_fx: [HoldFx; MAX_COLS],
    prev_hold_judged: [f32; MAX_COLS],
    prev_mine: [f32; MAX_COLS],
    /// Physical per-column press state, to release a tap overlay holding in
    /// its loop region when the panel lifts.
    prev_pressed: [bool; MAX_COLS],
}

impl Default for SmxPanelDriver {
    fn default() -> Self {
        Self {
            lights: SmxPanelLights::new(),
            gameplay_active: false,
            worker_active: false,
            backgrounds: std::array::from_fn(|_| None),
            judgement_gifs: std::array::from_fn(|_| JudgementGifs::default()),
            notes_ptr: 0,
            slot_for_pad: std::array::from_fn(|pad| pad),
            prev_flash: [NO_EVENT; MAX_COLS],
            prev_engaged: [false; MAX_COLS],
            prev_hold_fx: [HoldFx::None; MAX_COLS],
            prev_hold_judged: [NO_EVENT; MAX_COLS],
            prev_mine: [NO_EVENT; MAX_COLS],
            prev_pressed: [false; MAX_COLS],
        }
    }
}

impl SmxPanelDriver {
    /// Called each frame while on a gameplay screen with the feature enabled. Diffs the
    /// per-column flash, active-hold, hold-judgement, and mine state and emits panel events.
    pub fn update<Profile, OverlayActor, CapturedActor, StateDelta>(
        &mut self,
        state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
        slot_for_pad: [usize; PADS],
    ) where
        Profile: GameplayProfileData,
    {
        // Re-arm on entering gameplay or when the chart's note buffer changes (a restart or
        // a new song), so stale `*_at_screen_s` values do not swallow the first event.
        let notes_ptr = state.notes().as_ptr() as usize;
        if !self.gameplay_active || notes_ptr != self.notes_ptr || self.slot_for_pad != slot_for_pad
        {
            self.activate(state, slot_for_pad);
        }

        let cpp = state.cols_per_player();
        let np = state.num_players();
        let cols = cpp.saturating_mul(np).min(MAX_COLS);
        for col in 0..cols {
            // Resolve the panel once and translate the chart pad index to the physical SMX
            // slot the player's pad sits on. The trackers below still update even when a
            // column maps to no panel, so events are consumed rather than replayed later.
            let panel = smx_panel_for_col(cpp, np, col).map(|(pad, p)| (self.slot_for_pad[pad], p));
            let pressed = state.lane_is_pressed(col);
            let press_edge = hold_edge(pressed, &mut self.prev_pressed[col]);
            let released = press_edge == Some(false);

            // Panel lift: let a tap overlay holding in its loop region play its outro.
            // Sustains are handled below instead, by the engage/disengage edges plus
            // the press-edge tracking for the freeze/roll grace windows.
            if released
                && self.prev_hold_fx[col] == HoldFx::None
                && let Some((pad, p)) = panel
            {
                self.lights.release_overlay(pad, p);
            }

            if let Some((grade, blue)) =
                tap_event(state.last_tap_judgment(col), &mut self.prev_flash[col])
                && let Some((pad, p)) = panel
                && let Some(anim) = self.judgement_gifs[pad].for_grade(grade, blue)
            {
                self.lights
                    .play_overlay(pad, p, anim.clone(), OverlayDrive::OneShot { pressed });
            }

            let engaged = state.active_hold(col).is_some_and(active_hold_is_engaged);
            let engage_edge = hold_edge(engaged, &mut self.prev_engaged[col]);
            if let Some(now_engaged) = engage_edge
                && let Some((pad, p)) = panel
            {
                if now_engaged {
                    let kind = state.active_hold(col).map(|h| h.note_type);
                    // First engage: play the intro, then loop (freeze) or drain
                    // into the outro (roll). No gif = no effect on this column.
                    if let Some(anim) = sustain_anim(&self.judgement_gifs[pad], kind) {
                        self.lights
                            .play_overlay(pad, p, anim.clone(), sustain_drive(kind, false));
                        self.prev_hold_fx[col] = HoldFx::Overlay;
                    }
                } else {
                    if self.prev_hold_fx[col] == HoldFx::Overlay {
                        self.lights.release_overlay(pad, p);
                    }
                    self.prev_hold_fx[col] = HoldFx::None;
                }
            }

            // Sustained holds stay engaged while the engage edge is quiet, so
            // drive their overlay from the physical press instead. A freeze and
            // a roll differ: a freeze holds full life while pressed and only
            // drains on a lift, so a lift plays the outro (the grace window) and
            // a re-press snaps back to the loop. A roll's life drains
            // continuously and only a step refills it, so the overlay drains
            // forward on its own and each step (press edge) snaps it back to the
            // loop start; a lift does nothing.
            if engage_edge.is_none()
                && engaged
                && self.prev_hold_fx[col] == HoldFx::Overlay
                && let Some((pad, p)) = panel
            {
                let kind = state.active_hold(col).map(|h| h.note_type);
                if kind == Some(NoteType::Roll) {
                    if press_edge == Some(true)
                        && let Some(anim) = sustain_anim(&self.judgement_gifs[pad], kind)
                    {
                        self.lights.play_overlay(
                            pad,
                            p,
                            anim.clone(),
                            OverlayDrive::Roll { resume: true },
                        );
                    }
                } else if released {
                    self.lights.release_overlay(pad, p);
                } else if press_edge == Some(true)
                    && let Some(anim) = sustain_anim(&self.judgement_gifs[pad], kind)
                {
                    self.lights.play_overlay(
                        pad,
                        p,
                        anim.clone(),
                        OverlayDrive::Sustain { resume: true },
                    );
                }
            }

            if let Some(result) =
                hold_outcome_event(state.hold_judgment(col), &mut self.prev_hold_judged[col])
                && let Some((pad, p)) = panel
            {
                let anim = match result {
                    HoldResult::Held => self.judgement_gifs[pad].ok.as_ref(),
                    _ => self.judgement_gifs[pad].bad.as_ref(),
                };
                if let Some(anim) = anim {
                    self.lights.play_overlay(
                        pad,
                        p,
                        anim.clone(),
                        OverlayDrive::OneShot { pressed },
                    );
                }
            }

            if mine_event(
                state.mine_started_at_screen_s(col),
                &mut self.prev_mine[col],
            ) && let Some((pad, p)) = panel
                && let Some(anim) = &self.judgement_gifs[pad].mine
            {
                self.lights
                    .play_overlay(pad, p, anim.clone(), OverlayDrive::OneShot { pressed });
            }

            // Generic press feedback (SMX-style "you touched the panel"): a
            // physical press with no note of its own. Driven on its own low
            // layer so a real hit's judgement/sustain draws over it. Skipped
            // while a freeze/roll is engaged, since that owns the panel through
            // its own overlay. Gif-only: a pack without a `press` gif gets no
            // feedback here (the game events still flash as before).
            if !engaged
                && let Some((pad, p)) = panel
                && let Some(press_anim) = self.judgement_gifs[pad].press.clone()
            {
                if press_edge == Some(true) {
                    self.lights.play_press_overlay(
                        pad,
                        p,
                        press_anim,
                        OverlayDrive::Sustain { resume: false },
                    );
                } else if released {
                    self.lights.release_press_overlay(pad, p);
                }
            }
        }
    }

    /// Called when leaving gameplay or when judgement lighting is disabled. Ends the
    /// gameplay effects; the worker stays active (and the pad stays ours) while a
    /// background animation is showing, otherwise the pad returns to firmware lighting.
    pub fn deactivate(&mut self) {
        if self.gameplay_active {
            self.gameplay_active = false;
            // Flush stale per-panel effects (press overlays held during a transition)
            // even when the worker stays alive for a background animation.
            self.lights.clear_panels();
            self.sync_worker();
        }
    }

    /// Show, swap, or clear the background animation for a specific pad slot. Deduplicates,
    /// so calling every frame with the screen's resolved background is cheap; only an actual
    /// change is sent to the worker.
    pub fn set_background_for_pad(
        &mut self,
        pad: usize,
        background: Option<(Arc<FullPadAnim>, Clock)>,
    ) {
        if pad >= PADS {
            return;
        }
        let unchanged = match (&self.backgrounds[pad], &background) {
            (None, None) => true,
            (Some((a, ca)), Some((b, cb))) => Arc::ptr_eq(a, b) && ca == cb,
            _ => false,
        };
        if unchanged {
            return;
        }
        self.backgrounds[pad] = background.clone();
        self.lights.set_background_for_pad(pad, background);
        self.sync_worker();
    }

    /// Swap the judgement animation set for a specific pad slot (resolved app-side from
    /// the registry). Takes effect from the next event; in-flight overlays play out as started.
    pub fn set_judgement_gifs_for_pad(&mut self, pad: usize, gifs: JudgementGifs) {
        if pad < PADS {
            self.judgement_gifs[pad] = gifs;
        }
    }

    /// Force pad slot `pad` to solid black (`on = true`) or restore normal compositing.
    /// Call from the app layer to blank the unused pad in single-player mode.
    pub fn set_pad_blackout(&self, pad: usize, on: bool) {
        self.lights.set_pad_blackout(pad, on);
    }

    /// Handle a raw SMX panel press/release outside gameplay. Plays the `press` gif on
    /// contact and releases it on lift. No-op when the worker is idle or no press gif is
    /// configured; always safe to call regardless of current screen.
    pub fn on_raw_panel(&self, pad: usize, panel: usize, pressed: bool) {
        if !self.worker_active {
            return;
        }
        if pressed {
            if let Some(anim) = self.judgement_gifs.get(pad).and_then(|g| g.press.clone()) {
                self.lights.play_press_overlay(
                    pad,
                    panel,
                    anim,
                    OverlayDrive::Sustain { resume: false },
                );
            }
        } else {
            self.lights.release_press_overlay(pad, panel);
        }
    }

    /// Feed the current song beat position. Dropped unless the active background is
    /// beat-locked, so callers can push it every frame without flooding the worker.
    pub fn set_beat(&self, beat: f32) {
        if self
            .backgrounds
            .iter()
            .any(|b| matches!(b, Some((_, Clock::BeatLocked { .. }))))
        {
            self.lights.set_beat(beat);
        }
    }

    fn activate<Profile, OverlayActor, CapturedActor, StateDelta>(
        &mut self,
        state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
        slot_for_pad: [usize; PADS],
    ) where
        Profile: GameplayProfileData,
    {
        self.gameplay_active = true;
        self.notes_ptr = state.notes().as_ptr() as usize;
        self.slot_for_pad = slot_for_pad;
        self.prev_flash = [NO_EVENT; MAX_COLS];
        self.prev_engaged = [false; MAX_COLS];
        self.prev_hold_fx = [HoldFx::None; MAX_COLS];
        self.prev_hold_judged = [NO_EVENT; MAX_COLS];
        self.prev_mine = [NO_EVENT; MAX_COLS];
        self.prev_pressed = [false; MAX_COLS];
        // Always (re)send: entering active clears stale panel effects worker-side even
        // when the worker was already running for a background.
        self.worker_active = true;
        self.lights.set_active(true);
    }

    /// Activate or release the worker from the gameplay and background states. Releasing
    /// clears the worker (including its background copy) and restores firmware lighting;
    /// `self.backgrounds` is already all `None` whenever that happens, so the driver and
    /// worker stay in step.
    fn sync_worker(&mut self) {
        let want = self.gameplay_active || self.backgrounds.iter().any(|b| b.is_some());
        if want != self.worker_active {
            self.worker_active = want;
            self.lights.set_active(want);
        }
    }
}
