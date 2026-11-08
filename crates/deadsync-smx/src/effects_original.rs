use crate::gifs::{FullPadAnim, PanelAnim, PanelFrame};
use deadsync_score::Grade;
use std::sync::Arc;

/// Pads addressed by the SMX SDK (slot 0 and slot 1).
pub const PADS: usize = 2;
/// Panels per pad (3x3 grid).
pub const PANELS: usize = 9;
/// LEDs per panel in the 25-LED layout the SDK expects.
pub const LEDS_PER_PANEL: usize = 25;
/// Bytes for one pad: 9 panels * 25 LEDs * 3 (RGB).
pub const BYTES_PER_PAD: usize = PANELS * LEDS_PER_PANEL * 3;
/// Bytes for a full both-pads frame handed to `SmxManager::set_lights`.
pub const FRAME_BYTES: usize = PADS * BYTES_PER_PAD;

/// An RGB colour, raw 0..=255. The SDK applies its own brightness scale on send.
pub type Rgb = [u8; 3];

const BLACK: Rgb = [0, 0, 0];

/// L, D, U, R direction columns mapped to 3x3 grid panel indices
/// (panel names: UL,U,UR,L,C,R,DL,D,DR).
const DIR_TO_PANEL: [usize; 4] = [3, 7, 1, 5];
/// DL, UL, C, UR, DR Pump columns mapped to the 3x3 grid.
const PUMP_TO_PANEL: [usize; 5] = [6, 0, 4, 2, 8];

const MIN_FRAME_DURATION_S: f32 = 0.001;

/// How a playing background animation advances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clock {
    /// Advance by wall-clock time using the GIF's own frame durations.
    Realtime,
    /// Ignore frame durations and pick the frame from the song beat: one pass
    /// through the loop region spans this many beats, so the animation tracks
    /// tempo and BPM changes exactly.
    BeatLocked { beats_per_loop: f32 },
}

/// The active full-pad background animation and its playback position.
struct Background {
    anim: Arc<FullPadAnim>,
    clock: Clock,
    frame: usize,
    time_in_frame: f32,
}

/// How a panel overlay is driven by the player's panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayDrive {
    /// Tap judgement: plays once and clears itself. If the GIF has an outro
    /// segment and the panel is still pressed, it holds in its loop region
    /// until `release_overlay` lets the outro play.
    OneShot { pressed: bool },
    /// Freeze sustain: loops in its loop region while held, until
    /// `release_overlay` plays the outro (the grace window) or clears it. With
    /// `resume`, a fresh overlay starts at the loop region instead of the
    /// intro (a re-press whose outro already finished and cleared the overlay).
    Sustain { resume: bool },
    /// Roll sustain: a roll's life drains continuously and only a fresh step
    /// refills it, so playback runs forward through the loop into the outro
    /// (no wrapping) to show the drain, and holds on the last frame. Each step
    /// re-triggers this, snapping back to the loop start. With `resume`, a
    /// fresh overlay starts at the loop region instead of the intro.
    Roll { resume: bool },
}

/// A per-panel GIF animation playing over the background. See `OverlayDrive`
/// for the press/release-driven lifecycle.
pub(super) struct Overlay {
    anim: Arc<PanelAnim>,
    /// Sustain overlays loop until released even without an outro segment.
    sustain: bool,
    /// Roll overlays run forward through the loop into the outro (never
    /// wrapping) and hold on the last frame; each step resets them to the loop
    /// start. Distinct from a freeze `sustain`, which wraps in the loop.
    roll: bool,
    /// While engaged, playback wraps from `loop_end` back to `loop_frame`.
    /// Cleared by `release_overlay`; re-set when a sustain re-engages.
    engaged: bool,
    frame: usize,
    time_in_frame: f32,
}

/// One panel's effect state: an optional GIF overlay plus a low-priority
/// press-feedback overlay drawn only when nothing else claims the panel.
#[derive(Default)]
struct PanelState {
    overlay: Option<Overlay>,
    /// Generic "panel pressed" feedback, below the game-event overlay. Driven by
    /// the physical press so a press that hits no note still lights the panel.
    press_overlay: Option<Overlay>,
}

/// Per-pad, per-panel effect state plus per-pad background layers. Builds
/// the full both-pads RGB frame each tick.
pub struct PanelFx {
    panels: [[PanelState; PANELS]; PADS],
    backgrounds: [Option<Background>; PADS],
    /// Latest song beat position, used by a `BeatLocked` background.
    beat: f32,
    /// Beat position at the previous tick, for the tempo estimate.
    prev_beat: f32,
    /// Smoothed live tempo estimate in beats per second, used to cap a
    /// beat-locked background's frame rate (see `beat_frame`).
    beat_rate: f32,
    /// Per-slot blackout: when set, all frame bytes for that slot are zeroed after
    /// compositing, showing solid black regardless of any effects or background.
    /// Used to blank an unused pad in single-player mode.
    blackout: [bool; PADS],
    /// Reused output buffer. `tick` overwrites every byte each call, so it never needs
    /// clearing and we avoid re-zeroing 1350 bytes per frame.
    frame: [u8; FRAME_BYTES],
}

impl Default for PanelFx {
    fn default() -> Self {
        Self::new()
    }
}

impl PanelFx {
    #[must_use]
    pub fn new() -> Self {
        Self {
            panels: std::array::from_fn(|_| std::array::from_fn(|_| PanelState::default())),
            backgrounds: std::array::from_fn(|_| None),
            beat: 0.0,
            prev_beat: 0.0,
            beat_rate: 0.0,
            blackout: [false; PADS],
            frame: [0u8; FRAME_BYTES],
        }
    }

    /// Force a pad slot to solid black regardless of any effects or background.
    /// Persists across `clear_panels` / `clear_all` since it is a mode, not an effect.
    pub const fn set_pad_blackout(&mut self, pad: usize, on: bool) {
        if pad < PADS {
            self.blackout[pad] = on;
        }
    }

    /// Set or clear the background animation for a specific pad slot.
    /// Playback restarts from the first frame. Out-of-range pad index is ignored.
    pub fn set_background_for_pad(
        &mut self,
        pad: usize,
        background: Option<(Arc<FullPadAnim>, Clock)>,
    ) {
        if pad < PADS {
            self.backgrounds[pad] = background.map(|(anim, clock)| Background {
                anim,
                clock,
                frame: 0,
                time_in_frame: 0.0,
            });
        }
    }

    /// Update the song beat position driving a `BeatLocked` background.
    pub const fn set_beat(&mut self, beat: f32) {
        self.beat = beat;
    }

    /// Start a GIF animation on a panel, over the background. Out-of-range
    /// indices are ignored. Re-driving the animation a panel is already
    /// showing re-triggers it in place instead of restarting from the intro:
    /// a freeze sustain jumps from the outro back into the loop region, and a
    /// roll snaps back to the loop start on every step.
    pub fn play_overlay(
        &mut self,
        pad: usize,
        panel: usize,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    ) {
        if let Some(p) = self.panel_mut(pad, panel) {
            start_overlay(&mut p.overlay, anim, drive);
        }
    }

    /// Like `play_overlay`, but on the panel's low-priority press-feedback
    /// layer (drawn only when no game-event effect is showing).
    pub fn play_press_overlay(
        &mut self,
        pad: usize,
        panel: usize,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    ) {
        if let Some(p) = self.panel_mut(pad, panel) {
            start_overlay(&mut p.press_overlay, anim, drive);
        }
    }

    /// Release a panel's GIF overlay (panel lift / freeze-roll disengage): an
    /// overlay with an outro segment snaps from its loop region straight to
    /// the outro (rather than finishing the current loop pass) and clears
    /// itself when the outro ends, a sustain without one clears immediately
    /// (revealing the layers under it), and a plain one-shot is unaffected.
    pub fn release_overlay(&mut self, pad: usize, panel: usize) {
        if let Some(p) = self.panel_mut(pad, panel) {
            end_overlay(&mut p.overlay);
        }
    }

    /// Release a panel's press-feedback overlay (panel lift).
    pub fn release_press_overlay(&mut self, pad: usize, panel: usize) {
        if let Some(p) = self.panel_mut(pad, panel) {
            end_overlay(&mut p.press_overlay);
        }
    }

    /// Clear every per-panel overlay, keeping the background. Used when
    /// (re)entering active lighting on a screen.
    pub fn clear_panels(&mut self) {
        self.panels = std::array::from_fn(|_| std::array::from_fn(|_| PanelState::default()));
    }

    /// Full reset: every per-panel effect plus all backgrounds. Used when
    /// handing the pad back to its firmware lighting.
    pub fn clear_all(&mut self) {
        self.clear_panels();
        self.backgrounds = std::array::from_fn(|_| None);
    }

    /// Drop one pad slot's per-panel effects, keeping its background.
    pub fn clear_panels_for_pad(&mut self, pad: usize) {
        if pad < PADS {
            self.panels[pad] = std::array::from_fn(|_| PanelState::default());
        }
    }

    /// Full reset of one pad slot: its per-panel effects plus its background,
    /// leaving the other pad untouched. Used when handing a single pad back to
    /// its firmware lighting while its neighbour keeps animating.
    pub fn clear_pad(&mut self, pad: usize) {
        self.clear_panels_for_pad(pad);
        if pad < PADS {
            self.backgrounds[pad] = None;
        }
    }

    /// Advance all playback by `dt_s`, rebuild the reused both-pads RGB frame, and return
    /// it. Every panel is filled, so all 1350 bytes are overwritten and the buffer needs
    /// no clear.
    pub fn tick(&mut self, dt_s: f32) -> &[u8; FRAME_BYTES] {
        let dt = dt_s.max(0.0);
        self.update_beat_rate(dt);
        // Advance all backgrounds first, then composite each pad with its own.
        let mut bg_frames = [None::<usize>; PADS];
        for pad in 0..PADS {
            bg_frames[pad] =
                advance_background(&mut self.backgrounds[pad], self.beat, self.beat_rate, dt);
        }
        for pad in 0..PADS {
            for panel in 0..PANELS {
                let p = &mut self.panels[pad][panel];
                advance_overlay(&mut p.overlay, dt);
                advance_overlay(&mut p.press_overlay, dt);
                let p = &*p;
                let base = pad * BYTES_PER_PAD + panel * (LEDS_PER_PANEL * 3);
                let top = p
                    .overlay
                    .as_ref()
                    .map(|overlay| &overlay.anim.frames[overlay.frame])
                    .or_else(|| {
                        p.press_overlay
                            .as_ref()
                            .map(|overlay| &overlay.anim.frames[overlay.frame])
                    });
                // A blackout suppresses only the background; feedback still
                // composites over black. Resolve each layer once per panel.
                let background = (!self.blackout[pad])
                    .then_some(bg_frames[pad])
                    .flatten()
                    .and_then(|frame| {
                        self.backgrounds[pad]
                            .as_ref()
                            .map(|background| &background.anim.panels[panel][frame])
                    });
                compose_panel(
                    &mut self.frame[base..base + LEDS_PER_PANEL * 3],
                    top,
                    background,
                );
            }
        }
        &self.frame
    }

    fn panel_mut(&mut self, pad: usize, panel: usize) -> Option<&mut PanelState> {
        self.panels.get_mut(pad)?.get_mut(panel)
    }

    /// Track a smoothed beats-per-second estimate from the beat positions fed
    /// via `set_beat`. Backward jumps (a preview loop restart, a new song) and
    /// absurd rates are ignored rather than folded into the estimate.
    fn update_beat_rate(&mut self, dt: f32) {
        if dt > 0.0 {
            let inst = (self.beat - self.prev_beat) / dt;
            // 40 beats/s = 2400bpm, beyond any real chart.
            if (0.0..=40.0).contains(&inst) {
                self.beat_rate = (inst - self.beat_rate).mul_add(0.2, self.beat_rate);
            }
        }
        self.prev_beat = self.beat;
    }
}

/// Advance one frame counter through `durations`, wrapping to `loop_frame`
/// after the last frame. Shared by the background and looping overlays.
fn advance_looping(
    frame: &mut usize,
    time_in_frame: &mut f32,
    dt: f32,
    durations: &[f32],
    loop_frame: usize,
) {
    *time_in_frame += dt;
    while *time_in_frame >= durations[*frame].max(MIN_FRAME_DURATION_S) {
        *time_in_frame -= durations[*frame].max(MIN_FRAME_DURATION_S);
        *frame += 1;
        if *frame >= durations.len() {
            *frame = loop_frame.min(durations.len() - 1);
        }
    }
}

/// Advance the background and return the frame index to draw this tick
/// (`None` when there is no background).
fn advance_background(
    background: &mut Option<Background>,
    beat: f32,
    beat_rate: f32,
    dt: f32,
) -> Option<usize> {
    let bg = background.as_mut()?;
    match bg.clock {
        Clock::Realtime => {
            advance_looping(
                &mut bg.frame,
                &mut bg.time_in_frame,
                dt,
                &bg.anim.durations,
                bg.anim.loop_frame,
            );
            Some(bg.frame)
        }
        Clock::BeatLocked { beats_per_loop } => {
            Some(beat_frame(&bg.anim, beats_per_loop, beat, beat_rate))
        }
    }
}

/// Frame rate ceiling for beat-locked playback. The pads top out at 30fps;
/// the extra margin keeps a gif authored at exactly 30fps from flapping
/// between spans on beat-rate estimation noise.
const MAX_BEAT_FPS: f32 = 31.0;

/// Map a song beat position to a background frame: one pass through the loop
/// region (`loop_frame..end`) spans `beats_per_loop` beats. Negative beats
/// (before the first beat) wrap, so the animation is always in phase.
///
/// `beat_rate` is the live tempo estimate in beats per second. When it would
/// push the gif past `MAX_BEAT_FPS` (the pads cap at 30fps), the span doubles
/// (one pass covers 2x, 4x, ... beats) until the frame rate fits: half-time
/// playback that stays on the beat instead of silently dropping frames.
fn beat_frame(anim: &FullPadAnim, beats_per_loop: f32, beat: f32, beat_rate: f32) -> usize {
    let len = anim.durations.len();
    let start = anim.loop_frame.min(len - 1);
    let region = len - start;
    let mut span = beats_per_loop.max(f32::EPSILON);
    if beat_rate.is_finite() && beat_rate > 0.0 {
        let mut doublings = 0;
        while region as f32 * beat_rate / span > MAX_BEAT_FPS && doublings < 6 {
            span *= 2.0;
            doublings += 1;
        }
    }
    let phase = (beat / span).rem_euclid(1.0);
    start + ((phase * region as f32) as usize).min(region - 1)
}

/// Start (or re-trigger) an overlay in `slot`. Re-driving the animation the
/// slot is already showing re-triggers it in place instead of restarting from
/// the intro: a freeze sustain jumps from the outro back into the loop region,
/// and a roll snaps back to the loop start on every step.
pub(super) fn start_overlay(slot: &mut Option<Overlay>, anim: Arc<PanelAnim>, drive: OverlayDrive) {
    if let Some(o) = slot
        && Arc::ptr_eq(&o.anim, &anim)
    {
        let loop_frame = o.anim.loop_frame.min(o.anim.loop_end);
        match drive {
            // Freeze re-press: re-engage, jumping back to the loop only if the
            // outro had already started.
            OverlayDrive::Sustain { .. } if o.sustain => {
                o.engaged = true;
                if o.frame > o.anim.loop_end {
                    o.frame = loop_frame;
                    o.time_in_frame = 0.0;
                }
                return;
            }
            // Roll step: always snap back to the loop start (life refilled).
            OverlayDrive::Roll { .. } if o.roll => {
                o.frame = loop_frame;
                o.time_in_frame = 0.0;
                return;
            }
            _ => {}
        }
    }
    let (sustain, roll, engaged, resume) = match drive {
        OverlayDrive::OneShot { pressed } => (false, false, pressed, false),
        OverlayDrive::Sustain { resume } => (true, false, true, resume),
        OverlayDrive::Roll { resume } => (false, true, true, resume),
    };
    let frame = if resume {
        anim.loop_frame.min(anim.frames.len() - 1)
    } else {
        0
    };
    *slot = Some(Overlay {
        anim,
        sustain,
        roll,
        engaged,
        frame,
        time_in_frame: 0.0,
    });
}

/// End an overlay in `slot`: one with an outro snaps from its loop region
/// straight to the outro (then clears when it finishes), a sustain without one
/// clears immediately, and a plain one-shot is unaffected.
fn end_overlay(slot: &mut Option<Overlay>) {
    let Some(o) = slot else {
        return;
    };
    if o.anim.has_outro() {
        o.engaged = false;
        if o.frame >= o.anim.loop_frame && o.frame <= o.anim.loop_end {
            o.frame = o.anim.loop_end + 1;
            o.time_in_frame = 0.0;
        }
    } else if o.sustain {
        *slot = None;
    }
}

/// Advance a panel overlay; one that plays past its last frame clears itself
/// to reveal the layers under it, except a roll, which holds on the last frame
/// (its drained state) until a step resets it or the note resolves.
pub(super) fn advance_overlay(overlay: &mut Option<Overlay>, dt: f32) {
    let Some(o) = overlay else { return };
    o.time_in_frame += dt;
    while o.time_in_frame >= o.anim.durations[o.frame].max(MIN_FRAME_DURATION_S) {
        let last = o.anim.frames.len() - 1;
        let loop_end = o.anim.loop_end.min(last);
        // A freeze sustain wraps at the loop end while engaged, as does an
        // engaged one-shot with an outro held back; a roll never wraps (it
        // runs forward into the outro to show its drain).
        let wraps = !o.roll && o.engaged && (o.sustain || o.anim.has_outro());
        if o.frame == loop_end && wraps {
            o.time_in_frame -= o.anim.durations[o.frame].max(MIN_FRAME_DURATION_S);
            o.frame = o.anim.loop_frame.min(loop_end);
        } else if o.frame >= last {
            // A roll holds on its last (fully drained) frame; everything else
            // clears to reveal the layers under it.
            if o.roll {
                o.time_in_frame = 0.0;
                return;
            }
            *overlay = None;
            return;
        } else {
            o.time_in_frame -= o.anim.durations[o.frame].max(MIN_FRAME_DURATION_S);
            o.frame += 1;
        }
    }
}

/// Composite already-resolved packed panel frames. A single layer is a direct
/// 75-byte copy; only the overlay-plus-background case needs per-LED selection.
pub(super) fn compose_panel(
    out: &mut [u8],
    top: Option<&PanelFrame>,
    background: Option<&PanelFrame>,
) {
    match (top, background) {
        (None, None) => out.fill(0),
        (Some(frame), None) | (None, Some(frame)) => out.copy_from_slice(frame),
        (Some(top), Some(background)) => {
            for ((out, top), background) in out
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(top.as_chunks::<3>().0)
                .zip(background.as_chunks::<3>().0)
            {
                *out = if *top == BLACK { *background } else { *top };
            }
        }
    }
}

pub fn results_role_candidates(grade: Grade, difficulty: Option<&str>) -> Vec<String> {
    let mut suffixes = vec![grade.gif_suffix()];
    if let Some(base) = grade.gif_base() {
        suffixes.push(base.gif_suffix());
    }
    let mut out = Vec::with_capacity(2 * suffixes.len() + 1);
    for suffix in suffixes {
        if let Some(diff) = difficulty {
            out.push(format!("results@{diff}@{suffix}"));
        }
        out.push(format!("results@{suffix}"));
    }
    if let Some(diff) = difficulty {
        out.push(format!("results@{diff}"));
    }
    out
}

pub(super) fn state_bytes(fx: &PanelFx) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [fx.beat, fx.prev_beat, fx.beat_rate] {
        out.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    for pad in 0..PADS {
        out.push(u8::from(fx.blackout[pad]));
        match &fx.backgrounds[pad] {
            None => out.push(0),
            Some(bg) => {
                out.push(1);
                match bg.clock {
                    Clock::Realtime => out.push(0),
                    Clock::BeatLocked { beats_per_loop } => {
                        out.push(1);
                        out.extend_from_slice(&beats_per_loop.to_bits().to_le_bytes());
                    }
                }
                out.extend_from_slice(&(bg.frame as u64).to_le_bytes());
                out.extend_from_slice(&bg.time_in_frame.to_bits().to_le_bytes());
            }
        }
        for panel in &fx.panels[pad] {
            for slot in [&panel.overlay, &panel.press_overlay] {
                match slot {
                    None => out.push(0),
                    Some(overlay) => {
                        out.extend_from_slice(&[
                            1,
                            u8::from(overlay.sustain),
                            u8::from(overlay.roll),
                            u8::from(overlay.engaged),
                        ]);
                        out.extend_from_slice(&(overlay.frame as u64).to_le_bytes());
                        out.extend_from_slice(&overlay.time_in_frame.to_bits().to_le_bytes());
                    }
                }
            }
        }
    }
    out
}
