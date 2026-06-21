//! Per-frame diff that turns gameplay judgement and hold state into SMX panel light events.
//!
//! Runs on the app update path (sibling to the cabinet `GameplayLightTracker`). It only
//! compares small per-column state and pushes O(1) events to the SMX lights worker, so no
//! frame building or colour math happens on the gameplay/render path. The per-column
//! decisions are pure helpers (`tap_flash`, `hold_edge`, `hold_outcome_flash`, `mine_flash`)
//! so they can be unit-tested without constructing a whole gameplay `State`.

use std::sync::Arc;

use deadsync_core::note::NoteType;
use deadsync_gameplay::{ColumnTapJudgment, HoldJudgmentRenderInfo, active_hold_is_engaged};
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index, runtime_player_side};
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::note::HoldResult;
use deadsync_smx::gifs::{FullPadAnim, GifRegistry, PadSize, PanelAnim};
use deadsync_smx::panels::{Clock, OverlayDrive, PADS, Rgb, SmxPanelLights, smx_panel_for_col};

use crate::game::{GameplayCoreState, profile};

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

/// Tap flash durations, matching the on-screen column flash timing from gameplay runtime state.
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
/// `blue_fantastic` is the flag gameplay records on `ActiveColumnFlash`:
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

/// Resolved per-panel judgement animations from the GIF registry. Any slot left `None`
/// falls back to that event's solid colour, so a pack only has to supply the gifs it
/// cares about and judgements always stay visible.
#[derive(Default, Clone)]
pub struct JudgementGifs {
    pub fantastic_blue: Option<Arc<PanelAnim>>,
    pub fantastic_white: Option<Arc<PanelAnim>>,
    pub excellent: Option<Arc<PanelAnim>>,
    pub great: Option<Arc<PanelAnim>>,
    pub decent: Option<Arc<PanelAnim>>,
    pub way_off: Option<Arc<PanelAnim>>,
    pub miss: Option<Arc<PanelAnim>>,
    pub mine: Option<Arc<PanelAnim>>,
    /// Successful freeze/roll/lift release.
    pub ok: Option<Arc<PanelAnim>>,
    /// Failed (dropped) freeze/roll/lift.
    pub bad: Option<Arc<PanelAnim>>,
    /// Looping sustain while a freeze is engaged.
    pub freeze: Option<Arc<PanelAnim>>,
    /// Looping sustain while a roll is engaged.
    pub roll: Option<Arc<PanelAnim>>,
    /// Generic press feedback: a panel pressed with no note of its own. Drawn
    /// below the judgement and sustain layers, so a real hit overrides it.
    pub press: Option<Arc<PanelAnim>>,
}

impl JudgementGifs {
    /// Resolve the standard judgement names from a registry through the usual
    /// pack-then-size fallback. `_25` is the baseline both pad layouts render.
    pub fn resolve(registry: &GifRegistry, pack: Option<&str>) -> Self {
        let j = |name: &str| registry.judgement(pack, name, PadSize::Leds25);
        Self {
            fantastic_blue: j("fantastic_blue"),
            fantastic_white: j("fantastic_white"),
            excellent: j("excellent"),
            great: j("great"),
            decent: j("decent"),
            way_off: j("way_off"),
            miss: j("miss"),
            mine: j("mine"),
            ok: j("ok"),
            bad: j("bad"),
            freeze: j("freeze"),
            roll: j("roll"),
            press: j("press"),
        }
    }

    /// The one-shot animation for a tap grade, honouring the FA+ white/blue split.
    fn for_grade(&self, grade: JudgeGrade, blue_fantastic: bool) -> Option<&Arc<PanelAnim>> {
        match grade {
            JudgeGrade::Fantastic if blue_fantastic => self.fantastic_blue.as_ref(),
            JudgeGrade::Fantastic => self.fantastic_white.as_ref(),
            JudgeGrade::Excellent => self.excellent.as_ref(),
            JudgeGrade::Great => self.great.as_ref(),
            JudgeGrade::Decent => self.decent.as_ref(),
            JudgeGrade::WayOff => self.way_off.as_ref(),
            JudgeGrade::Miss => self.miss.as_ref(),
        }
    }
}

/// The looping sustain animation for an engaged hold, by its note kind.
fn sustain_anim(gifs: &JudgementGifs, kind: Option<NoteType>) -> Option<&Arc<PanelAnim>> {
    match kind {
        Some(NoteType::Hold) => gifs.freeze.as_ref(),
        Some(NoteType::Roll) => gifs.roll.as_ref(),
        _ => None,
    }
}

/// The worker drive for a sustained hold's overlay, by note kind: a freeze
/// holds in its loop and plays the outro on release (`Sustain`), a roll runs
/// forward into the outro to show its continuous drain and resets on each step
/// (`Roll`). `resume` starts a re-triggered overlay at the loop region.
fn sustain_drive(kind: Option<NoteType>, resume: bool) -> OverlayDrive {
    match kind {
        Some(NoteType::Roll) => OverlayDrive::Roll { resume },
        _ => OverlayDrive::Sustain { resume },
    }
}

/// What `update` started on a column's panel for the current sustain, so the matching
/// end call is sent when it disengages (an overlay and a colour hold end differently).
#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum HoldFx {
    #[default]
    None,
    Color,
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
    /// Background currently applied to the worker, for change detection.
    background: Option<(Arc<FullPadAnim>, Clock)>,
    /// Judgement animations; empty (all `None`) means solid colours throughout.
    judgement_gifs: JudgementGifs,
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
            background: None,
            judgement_gifs: JudgementGifs::default(),
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
    pub fn update(&mut self, state: &GameplayCoreState) {
        // Re-arm on entering gameplay or when the chart's note buffer changes (a restart or
        // a new song), so stale `*_at_screen_s` values do not swallow the first event.
        let notes_ptr = state.notes().as_ptr() as usize;
        if !self.gameplay_active || notes_ptr != self.notes_ptr {
            self.activate(state);
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
            {
                match self.judgement_gifs.for_grade(grade, blue) {
                    Some(anim) => {
                        self.lights.play_overlay(
                            pad,
                            p,
                            anim.clone(),
                            OverlayDrive::OneShot { pressed },
                        );
                    }
                    None => {
                        self.lights
                            .flash(pad, p, flash_color(grade, blue), flash_duration(grade));
                    }
                }
            }

            let engaged = state.active_hold(col).is_some_and(active_hold_is_engaged);
            let engage_edge = hold_edge(engaged, &mut self.prev_engaged[col]);
            if let Some(now_engaged) = engage_edge
                && let Some((pad, p)) = panel
            {
                if now_engaged {
                    let kind = state.active_hold(col).map(|h| h.note_type);
                    match sustain_anim(&self.judgement_gifs, kind) {
                        Some(anim) => {
                            // First engage: play the intro, then loop (freeze) or
                            // drain forward into the outro (roll).
                            self.lights.play_overlay(
                                pad,
                                p,
                                anim.clone(),
                                sustain_drive(kind, false),
                            );
                            self.prev_hold_fx[col] = HoldFx::Overlay;
                        }
                        None => {
                            self.lights.hold_start(pad, p, HOLD_RGB);
                            self.prev_hold_fx[col] = HoldFx::Color;
                        }
                    }
                } else {
                    // End whichever effect this column's engage started; the hold
                    // state may already be gone, so we use the recorded kind.
                    match self.prev_hold_fx[col] {
                        HoldFx::Overlay => self.lights.release_overlay(pad, p),
                        HoldFx::Color => self.lights.hold_end(pad, p),
                        HoldFx::None => {}
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
                        && let Some(anim) = sustain_anim(&self.judgement_gifs, kind)
                    {
                        self.lights
                            .play_overlay(pad, p, anim.clone(), OverlayDrive::Roll { resume: true });
                    }
                } else if released {
                    self.lights.release_overlay(pad, p);
                } else if press_edge == Some(true)
                    && let Some(anim) = sustain_anim(&self.judgement_gifs, kind)
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
                let (anim, color) = match result {
                    HoldResult::Held => (self.judgement_gifs.ok.as_ref(), HOLD_OK_RGB),
                    _ => (self.judgement_gifs.bad.as_ref(), HOLD_DROP_RGB),
                };
                match anim {
                    Some(anim) => {
                        self.lights.play_overlay(
                            pad,
                            p,
                            anim.clone(),
                            OverlayDrive::OneShot { pressed },
                        );
                    }
                    None => self.lights.flash(pad, p, color, FLASH_SECONDS_JUDGMENT),
                }
            }

            let mine_at = state.mine_started_at_screen_s(col);
            if mine_event(mine_at, &mut self.prev_mine[col])
                && let Some((pad, p)) = panel
            {
                match &self.judgement_gifs.mine {
                    Some(anim) => {
                        self.lights.play_overlay(
                            pad,
                            p,
                            anim.clone(),
                            OverlayDrive::OneShot { pressed },
                        );
                    }
                    None => self.lights.flash(pad, p, MINE_RGB, MINE_FLASH_SECONDS),
                }
            }

            // Generic press feedback (SMX-style "you touched the panel"): a
            // physical press with no note of its own. Driven on its own low
            // layer so a real hit's judgement/sustain draws over it. Skipped
            // while a freeze/roll is engaged, since that owns the panel through
            // its own overlay. Gif-only: a pack without a `press` gif gets no
            // feedback here (the game events still flash as before).
            if let Some(press_anim) = self.judgement_gifs.press.clone()
                && !engaged
                && let Some((pad, p)) = panel
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
            self.sync_worker();
        }
    }

    /// Show, swap, or clear the full-pad background animation. Deduplicates, so calling
    /// every frame with the screen's resolved background is cheap; only an actual change
    /// is sent to the worker.
    pub fn set_background(&mut self, background: Option<(Arc<FullPadAnim>, Clock)>) {
        let unchanged = match (&self.background, &background) {
            (None, None) => true,
            (Some((a, ca)), Some((b, cb))) => Arc::ptr_eq(a, b) && ca == cb,
            _ => false,
        };
        if unchanged {
            return;
        }
        self.background = background.clone();
        self.lights.set_background(background);
        self.sync_worker();
    }

    /// Swap the judgement animation set (resolved app-side from the registry). Takes
    /// effect from the next event; in-flight overlays play out as started.
    pub fn set_judgement_gifs(&mut self, gifs: JudgementGifs) {
        self.judgement_gifs = gifs;
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
            if let Some(anim) = self.judgement_gifs.press.clone() {
                self.lights
                    .play_press_overlay(pad, panel, anim, OverlayDrive::Sustain { resume: false });
            }
        } else {
            self.lights.release_press_overlay(pad, panel);
        }
    }

    /// Feed the current song beat position. Dropped unless the active background is
    /// beat-locked, so callers can push it every frame without flooding the worker.
    pub fn set_beat(&self, beat: f32) {
        if matches!(self.background, Some((_, Clock::BeatLocked { .. }))) {
            self.lights.set_beat(beat);
        }
    }

    fn activate(&mut self, state: &GameplayCoreState) {
        self.gameplay_active = true;
        self.notes_ptr = state.notes().as_ptr() as usize;
        // Resolve the chart-pad -> physical-slot map once per song (the session play style
        // and side are fixed for the run), keeping the per-frame loop off the session lock.
        let play_style = profile::get_session_play_style();
        let session_side = profile::get_session_player_side();
        let doubles = state.cols_per_player() >= 8 && state.num_players() == 1;
        self.slot_for_pad =
            std::array::from_fn(|pad| physical_slot(play_style, session_side, doubles, pad));
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
    /// `self.background` is already `None` whenever that happens, so the driver and
    /// worker stay in step.
    fn sync_worker(&mut self) {
        let want = self.gameplay_active || self.background.is_some();
        if want != self.worker_active {
            self.worker_active = want;
            self.lights.set_active(want);
        }
    }
}

/// Decide a new tap judgement for a column (the grade and its FA+ white/blue flag).
/// Records the judgement time so the same one is not re-fired, and re-arms (sentinel)
/// when the column currently has no judgement.
fn tap_event(judged: Option<ColumnTapJudgment>, prev: &mut f32) -> Option<(JudgeGrade, bool)> {
    match judged {
        Some(j) if j.at_screen_s != *prev => {
            *prev = j.at_screen_s;
            Some((j.grade, j.blue_fantastic))
        }
        None => {
            *prev = NO_EVENT;
            None
        }
        _ => None,
    }
}

/// Decide an edge on a boolean tracker (a freeze/roll engage or a physical panel
/// press): `Some(true)` on rise, `Some(false)` on fall, `None` when nothing changed.
fn hold_edge(engaged: bool, prev: &mut bool) -> Option<bool> {
    if engaged == *prev {
        None
    } else {
        *prev = engaged;
        Some(engaged)
    }
}

/// Decide a new freeze/roll outcome for a column. Held shows OK, dropped shows the
/// failure effect, missed consumes the event but shows nothing.
fn hold_outcome_event(
    judged: Option<HoldJudgmentRenderInfo>,
    prev: &mut f32,
) -> Option<HoldResult> {
    match judged {
        Some(j) if j.started_at_screen_s != *prev => {
            *prev = j.started_at_screen_s;
            match j.result {
                HoldResult::Held | HoldResult::LetGo => Some(j.result),
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

/// Decide a new mine hit, keyed by hit time so a second hit on the same column while an
/// earlier explosion is still active is still caught.
fn mine_event(hit_at: Option<f32>, prev: &mut f32) -> bool {
    match hit_at {
        Some(ts) if ts != *prev => {
            *prev = ts;
            true
        }
        None => {
            *prev = NO_EVENT;
            false
        }
        _ => false,
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
        assert!(tap_event(Some(tap(JudgeGrade::Great, false, 1.0)), &mut prev).is_some());
        assert_eq!(prev, 1.0);
        // Same timestamp does not re-fire.
        assert!(tap_event(Some(tap(JudgeGrade::Great, false, 1.0)), &mut prev).is_none());
        // A new timestamp fires again.
        assert!(tap_event(Some(tap(JudgeGrade::Miss, false, 2.0)), &mut prev).is_some());
        assert_eq!(prev, 2.0);
    }

    #[test]
    fn tap_flash_none_rearms() {
        let mut prev = 5.0;
        assert!(tap_event(None, &mut prev).is_none());
        assert_eq!(prev, NO_EVENT);
        // After re-arm a judgement at any time reads as new.
        assert!(tap_event(Some(tap(JudgeGrade::Decent, false, 0.0)), &mut prev).is_some());
    }

    #[test]
    fn tap_event_carries_grade_and_fa_plus_flag() {
        let mut prev = NO_EVENT;
        assert_eq!(
            tap_event(Some(tap(JudgeGrade::Miss, false, 1.0)), &mut prev),
            Some((JudgeGrade::Miss, false))
        );
        assert_eq!(
            tap_event(Some(tap(JudgeGrade::Fantastic, true, 2.0)), &mut prev),
            Some((JudgeGrade::Fantastic, true))
        );
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
    fn hold_outcome_event_maps_result() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            Some(HoldResult::Held)
        );
        // Drop at a new time.
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::LetGo, 2.0)), &mut prev),
            Some(HoldResult::LetGo)
        );
        // Missed consumes the event (records its time) but shows nothing.
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Missed, 3.0)), &mut prev),
            None
        );
        assert_eq!(prev, 3.0);
    }

    #[test]
    fn hold_outcome_event_ignores_repeat_and_rearms() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            Some(HoldResult::Held)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            None
        );
        assert_eq!(hold_outcome_event(None, &mut prev), None);
        assert_eq!(prev, NO_EVENT);
    }

    #[test]
    fn mine_event_catches_consecutive_hits() {
        let mut prev = NO_EVENT;
        // First hit.
        assert!(mine_event(Some(1.0), &mut prev));
        // Same explosion (same hit time) does not re-fire.
        assert!(!mine_event(Some(1.0), &mut prev));
        // A second hit while the first explosion may still be active re-fires.
        assert!(mine_event(Some(1.5), &mut prev));
        // Explosion ended; re-arm.
        assert!(!mine_event(None, &mut prev));
        assert_eq!(prev, NO_EVENT);
    }

    // Judgement animation selection

    fn anim(tag: u8) -> Arc<PanelAnim> {
        Arc::new(PanelAnim {
            frames: vec![[tag; deadsync_smx::gifs::PANEL_RGB_BYTES]],
            durations: vec![0.1],
            loop_frame: 0,
            loop_end: 0,
        })
    }

    #[test]
    fn for_grade_picks_the_right_animation_and_falls_back() {
        let gifs = JudgementGifs {
            fantastic_blue: Some(anim(1)),
            fantastic_white: Some(anim(2)),
            miss: Some(anim(3)),
            ..Default::default()
        };
        let frame0 = |a: Option<&Arc<PanelAnim>>| a.unwrap().frames[0][0];
        // The FA+ flag selects blue vs white Fantastic.
        assert_eq!(frame0(gifs.for_grade(JudgeGrade::Fantastic, true)), 1);
        assert_eq!(frame0(gifs.for_grade(JudgeGrade::Fantastic, false)), 2);
        assert_eq!(frame0(gifs.for_grade(JudgeGrade::Miss, false)), 3);
        // A grade without a gif yields None so the caller uses the solid colour.
        assert!(gifs.for_grade(JudgeGrade::Great, false).is_none());
    }

    #[test]
    fn sustain_anim_distinguishes_freeze_and_roll() {
        let gifs = JudgementGifs {
            freeze: Some(anim(1)),
            roll: Some(anim(2)),
            ..Default::default()
        };
        let frame0 = |a: Option<&Arc<PanelAnim>>| a.unwrap().frames[0][0];
        assert_eq!(frame0(sustain_anim(&gifs, Some(NoteType::Hold))), 1);
        assert_eq!(frame0(sustain_anim(&gifs, Some(NoteType::Roll))), 2);
        assert!(sustain_anim(&gifs, Some(NoteType::Tap)).is_none());
        assert!(sustain_anim(&gifs, None).is_none());
        // No gifs at all: both kinds fall back to the solid hold colour.
        assert!(sustain_anim(&JudgementGifs::default(), Some(NoteType::Hold)).is_none());
    }
}
