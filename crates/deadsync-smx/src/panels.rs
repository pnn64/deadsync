//! StepManiaX pad panel lighting effects.
//!
//! Pure effect engine for the SMX 3x3 panels (`PanelFx`), the helper that maps a logical
//! column to a pad/panel, and the 30Hz worker thread (`SmxPanelLights`) that owns the
//! effect state and hands frames to the SDK. Building the RGB frame off the render thread
//! keeps the colour math and timers out of the per-frame path. The app-side diff and
//! palette live in `app::smx_panel_fx`.
//!
//! `PanelFx` composites layers over black: an optional full-pad background
//! animation (a preloaded GIF from `gifs::GifRegistry`, played realtime or locked
//! to the song beat) and per-panel effects on top of it. Compositing is
//! per-LED with black treated as transparent, so the layers blend through one
//! another instead of one layer fully replacing the panel: for each LED the
//! first non-black layer wins, top to bottom: GIF overlay (judgement/sustain),
//! press-feedback overlay, the background, then black. A judgement gif that
//! lights only some LEDs therefore lets the background show through the rest.
//! Press feedback is mutually exclusive with the game-event overlay: while a
//! judgement, freeze, or roll is active, the press overlay is suppressed (its
//! black LEDs reveal the background, not the press feedback).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::gifs::{FullPadAnim, PanelAnim, PanelFrame};

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

/// Map a logical column to the SMX pad slot and panel index.
///
/// Mirrors `pad_light_for_col` (app/mod.rs): handles singles, versus, and one-player
/// doubles via `cols_per_player` and `num_players`. Returns `None` for an out-of-range
/// column or a non-cardinal local column.
pub fn smx_panel_for_col(
    cols_per_player: usize,
    num_players: usize,
    column: usize,
) -> Option<(usize, usize)> {
    if cols_per_player == 0 {
        return None;
    }
    let local = column % cols_per_player;
    let (pad, local_col) = if cols_per_player >= 8 && num_players == 1 {
        // One human on two pads (doubles): the first four columns are the left pad.
        (if local < 4 { 0 } else { 1 }, local % 4)
    } else {
        let pad = column / cols_per_player;
        if pad >= PADS {
            return None;
        }
        (pad, local)
    };
    DIR_TO_PANEL.get(local_col).map(|&panel| (pad, panel))
}

/// Floor for frame durations when advancing playback, so a malformed zero
/// duration can't spin the advance loop forever. Decoded GIFs never hit this
/// (the decoder snaps non-positive delays to 1/30s).
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
struct Overlay {
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
    pub fn set_pad_blackout(&mut self, pad: usize, on: bool) {
        if pad < PADS {
            self.blackout[pad] = on;
        }
    }

    /// Set or clear the background animation for a specific pad slot.
    /// Playback restarts from the first frame. Out-of-range pad index is ignored.
    pub fn set_background_for_pad(&mut self, pad: usize, background: Option<(Arc<FullPadAnim>, Clock)>) {
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
    pub fn set_beat(&mut self, beat: f32) {
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

    /// Advance all playback by `dt_s`, rebuild the reused both-pads RGB frame, and return
    /// it. Every panel is filled, so all 1350 bytes are overwritten and the buffer needs
    /// no clear.
    pub fn tick(&mut self, dt_s: f32) -> &[u8; FRAME_BYTES] {
        let dt = dt_s.max(0.0);
        self.update_beat_rate(dt);
        // Advance all backgrounds first, then composite each pad with its own.
        let mut bg_frames = [None::<usize>; PADS];
        for pad in 0..PADS {
            bg_frames[pad] = advance_background(&mut self.backgrounds[pad], self.beat, self.beat_rate, dt);
        }
        for pad in 0..PADS {
            for panel in 0..PANELS {
                let p = &mut self.panels[pad][panel];
                advance_overlay(&mut p.overlay, dt);
                advance_overlay(&mut p.press_overlay, dt);
                // Per-LED composite with black as transparent: the first
                // non-black layer wins (see `composite_led`), so a partly lit
                // gif lets the layers under it show through its black LEDs.
                let p = &*p;
                let base = pad * BYTES_PER_PAD + panel * (LEDS_PER_PANEL * 3);
                // When blacked out, suppress the background so overlays (e.g.
                // press feedback) still composite on top of black.
                let bg = if self.blackout[pad] { &None } else { &self.backgrounds[pad] };
                for led in 0..LEDS_PER_PANEL {
                    let rgb = composite_led(p, bg, bg_frames[pad], panel, led);
                    let o = base + led * 3;
                    self.frame[o..o + 3].copy_from_slice(&rgb);
                }
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
                self.beat_rate += (inst - self.beat_rate) * 0.2;
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
fn start_overlay(slot: &mut Option<Overlay>, anim: Arc<PanelAnim>, drive: OverlayDrive) {
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
fn advance_overlay(overlay: &mut Option<Overlay>, dt: f32) {
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

/// One LED's RGB out of a packed panel frame.
#[inline]
fn led_rgb(frame: &PanelFrame, led: usize) -> Rgb {
    let o = led * 3;
    [frame[o], frame[o + 1], frame[o + 2]]
}

/// Resolve one LED's colour by compositing the panel's layers top to bottom,
/// treating black as transparent: the first layer whose LED is non-black wins,
/// else black. Order: GIF overlay (judgement/sustain), press-feedback overlay,
/// the background.
///
/// The press-feedback layer is mutually exclusive with the game-event overlay:
/// a judgement, freeze, or roll fully owns the panel, so press feedback never
/// shows through its black LEDs; those fall straight through to the background.
fn composite_led(
    p: &PanelState,
    background: &Option<Background>,
    bg_frame: Option<usize>,
    panel: usize,
    led: usize,
) -> Rgb {
    if let Some(o) = &p.overlay {
        let c = led_rgb(&o.anim.frames[o.frame], led);
        if c != BLACK {
            return c;
        }
    }
    if p.overlay.is_none()
        && let Some(o) = &p.press_overlay
    {
        let c = led_rgb(&o.anim.frames[o.frame], led);
        if c != BLACK {
            return c;
        }
    }
    if let (Some(bg), Some(f)) = (background, bg_frame) {
        let c = led_rgb(&bg.anim.panels[panel][f], led);
        if c != BLACK {
            return c;
        }
    }
    BLACK
}

// 30Hz worker thread

/// One frame interval at ~30Hz. The SDK also throttles its sends to this rate and
/// coalesces to the newest frame, so this only governs how often we rebuild a frame.
const FRAME_INTERVAL: Duration = Duration::from_micros(33_333);

/// Messages from the app to the worker. Pad/panel/animation are resolved
/// app-side (including registry lookups) so the worker stays free of app policy
/// and style knowledge; animations travel as cheap `Arc` handles.
enum Ev {
    /// Set or clear the background animation for a specific pad slot.
    Background { pad: u8, background: Option<(Arc<FullPadAnim>, Clock)> },
    /// Latest song beat position for a `BeatLocked` background.
    Beat(f32),
    /// Play a GIF on one panel, over the background.
    Overlay {
        pad: u8,
        panel: u8,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    },
    /// Release a panel's GIF overlay (panel lift / freeze-roll disengage).
    OverlayRelease {
        pad: u8,
        panel: u8,
    },
    /// Play a GIF on a panel's low-priority press-feedback layer.
    PressOverlay {
        pad: u8,
        panel: u8,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    },
    /// Release a panel's press-feedback overlay (panel lift).
    PressRelease {
        pad: u8,
        panel: u8,
    },
    /// Enter (true) or leave (false) active panel effect ownership. Leaving hands the pad
    /// back to its firmware idle lighting.
    Active(bool),
    /// Force a pad slot to solid black (true) or restore normal compositing (false).
    Blackout { pad: u8, on: bool },
    /// Clear all per-panel overlays without affecting the
    /// background or the active state. Used when leaving gameplay while the worker
    /// stays alive for a background animation.
    ClearPanels,
    Shutdown,
}

/// Handle to the SMX panel lighting worker thread.
///
/// The app pushes events; the worker owns the effect state, ticks at 30Hz, and calls
/// `set_lights`. Every send is non-blocking and silently no-ops if the thread failed to
/// spawn, so callers never have to special-case "no pad".
pub struct SmxPanelLights {
    tx: Option<Sender<Ev>>,
    join: Option<JoinHandle<()>>,
}

impl Default for SmxPanelLights {
    fn default() -> Self {
        Self::new()
    }
}

impl SmxPanelLights {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("deadsync-smx-lights".to_owned())
            .spawn(move || run_worker(rx))
            .ok();
        let tx = join.as_ref().map(|_| tx);
        Self { tx, join }
    }

    /// Set or clear the background animation for a specific pad slot.
    pub fn set_background_for_pad(&self, pad: usize, background: Option<(Arc<FullPadAnim>, Clock)>) {
        self.send(Ev::Background { pad: pad as u8, background });
    }

    /// Update the song beat position driving a `BeatLocked` background.
    pub fn set_beat(&self, beat: f32) {
        self.send(Ev::Beat(beat));
    }

    /// Play a GIF on one panel over the background; see `OverlayDrive` for
    /// the one-shot/sustain and press/release semantics.
    pub fn play_overlay(
        &self,
        pad: usize,
        panel: usize,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    ) {
        self.send(Ev::Overlay {
            pad: pad as u8,
            panel: panel as u8,
            anim,
            drive,
        });
    }

    /// Release a panel's GIF overlay (panel lift / freeze-roll disengage).
    pub fn release_overlay(&self, pad: usize, panel: usize) {
        self.send(Ev::OverlayRelease {
            pad: pad as u8,
            panel: panel as u8,
        });
    }

    /// Play a GIF on a panel's low-priority press-feedback layer (generic
    /// "panel pressed" feedback shown when no game event claims the panel).
    pub fn play_press_overlay(
        &self,
        pad: usize,
        panel: usize,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    ) {
        self.send(Ev::PressOverlay {
            pad: pad as u8,
            panel: panel as u8,
            anim,
            drive,
        });
    }

    /// Release a panel's press-feedback overlay (panel lift).
    pub fn release_press_overlay(&self, pad: usize, panel: usize) {
        self.send(Ev::PressRelease {
            pad: pad as u8,
            panel: panel as u8,
        });
    }

    /// Mark whether panel effects are active. On `false` the worker clears the panels and
    /// hands the pad back to its firmware idle lighting.
    pub fn set_active(&self, active: bool) {
        self.send(Ev::Active(active));
    }

    /// Force pad slot `pad` to solid black (`on = true`) or restore normal compositing.
    /// Persists until explicitly cleared; use to blank an unused pad in single-player mode.
    pub fn set_pad_blackout(&self, pad: usize, on: bool) {
        self.send(Ev::Blackout { pad: pad as u8, on });
    }

    /// Clear all per-panel overlays without touching the
    /// background or the active state. Call when leaving gameplay while the worker
    /// stays alive for a background, so stale press overlays don't persist into
    /// the next screen.
    pub fn clear_panels(&self) {
        self.send(Ev::ClearPanels);
    }

    fn send(&self, ev: Ev) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(ev);
        }
    }
}

impl Drop for SmxPanelLights {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Ev::Active(false));
            let _ = tx.send(Ev::Shutdown);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(rx: Receiver<Ev>) {
    let mut fx = PanelFx::new();
    let mut active = false;
    let mut last_tick = Instant::now();

    'outer: loop {
        // Active: wake at most one frame out to decay flashes and keep the pad ours.
        // Idle: nothing to tick, so block until the next event instead of spinning at 30Hz.
        let next = if active {
            match rx.recv_timeout(FRAME_INTERVAL) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break 'outer,
            }
        } else {
            match rx.recv() {
                Ok(ev) => Some(ev),
                Err(_) => break 'outer,
            }
        };

        if let Some(ev) = next {
            let was_active = active;
            // Apply this event, then drain any burst queued behind it.
            if handle(&mut fx, &mut active, ev) {
                break 'outer;
            }
            while let Ok(ev) = rx.try_recv() {
                if handle(&mut fx, &mut active, ev) {
                    break 'outer;
                }
            }
            if active && !was_active {
                // Just woke from the idle block; start the frame clock fresh so the first
                // tick uses a normal dt instead of the whole idle gap.
                last_tick = Instant::now();
            }
        }

        if active {
            let now = Instant::now();
            let dt = now.saturating_duration_since(last_tick);
            if dt >= FRAME_INTERVAL {
                last_tick = now;
                send_lights(fx.tick(dt.as_secs_f32()));
            }
        }
    }

    // On exit, leave the panels dark and restore the pad firmware idle lighting.
    fx.clear_all();
    send_lights(fx.tick(0.0));
    reenable_auto();
}

/// Apply one event to the effect state. Returns `true` when the worker should stop.
fn handle(fx: &mut PanelFx, active: &mut bool, ev: Ev) -> bool {
    match ev {
        Ev::Background { pad, background } => fx.set_background_for_pad(pad.into(), background),
        Ev::Beat(beat) => fx.set_beat(beat),
        Ev::Overlay {
            pad,
            panel,
            anim,
            drive,
        } => fx.play_overlay(pad.into(), panel.into(), anim, drive),
        Ev::OverlayRelease { pad, panel } => fx.release_overlay(pad.into(), panel.into()),
        Ev::PressOverlay {
            pad,
            panel,
            anim,
            drive,
        } => fx.play_press_overlay(pad.into(), panel.into(), anim, drive),
        Ev::PressRelease { pad, panel } => fx.release_press_overlay(pad.into(), panel.into()),
        Ev::Blackout { pad, on } => fx.set_pad_blackout(pad.into(), on),
        Ev::ClearPanels => fx.clear_panels(),
        Ev::Active(a) => {
            *active = a;
            if a {
                // Entering a screen: drop stale per-panel effects but keep any
                // background the app set up for it.
                fx.clear_panels();
            } else {
                // Going idle: drop everything, push one black frame, and hand
                // the pad back to firmware.
                fx.clear_all();
                send_lights(fx.tick(0.0));
                reenable_auto();
            }
        }
        Ev::Shutdown => return true,
    }
    false
}

fn send_lights(frame: &[u8]) {
    let Some(m) = crate::manager() else { return };
    // Apply the user brightness as a final per-slot scale. 100/100 is an exact
    // identity, so skip the copy on the common full-brightness path. Otherwise scale
    // into a stack buffer (no heap on the 30Hz worker) and send that.
    let brightness = crate::light_brightness();
    if brightness == [100, 100] || frame.len() > FRAME_BYTES {
        m.set_lights(frame);
        return;
    }
    let mut buf = [0u8; FRAME_BYTES];
    let n = frame.len();
    buf[..n].copy_from_slice(frame);
    crate::apply_brightness(&mut buf[..n], brightness);
    m.set_lights(&buf[..n]);
}

fn reenable_auto() {
    if let Some(m) = crate::manager() {
        m.reenable_auto_lights();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn panel_base(pad: usize, panel: usize) -> usize {
        pad * BYTES_PER_PAD + panel * (LEDS_PER_PANEL * 3)
    }

    #[test]
    fn frame_is_correct_length_and_black_when_idle() {
        let mut fx = PanelFx::new();
        let frame = fx.tick(0.0);
        assert_eq!(frame.len(), 1350);
        assert!(frame.iter().all(|&b| b == 0));
    }

    #[test]
    fn overlay_fills_every_panel_led() {
        let mut fx = PanelFx::new();
        fx.play_overlay(
            0,
            3,
            panel_anim(&[10], 0),
            OverlayDrive::Sustain { resume: false },
        );
        let base = panel_base(0, 3);
        // First and last LED of the panel share the colour; neighbours stay dark.
        let frame = fx.tick(0.0);
        assert_eq!(&frame[base..base + 3], &[10, 10, 10]);
        let last = base + (LEDS_PER_PANEL - 1) * 3;
        assert_eq!(&frame[last..last + 3], &[10, 10, 10]);
        assert_eq!(led0(frame, 0, 4), 0);
    }

    #[test]
    fn clear_all_blacks_everything() {
        let mut fx = PanelFx::new();
        fx.play_overlay(0, 0, panel_anim(&[5], 0), OverlayDrive::Sustain { resume: false });
        fx.play_press_overlay(1, 8, panel_anim(&[6], 0), OverlayDrive::Sustain { resume: false });
        fx.clear_all();
        let frame = fx.tick(0.0);
        assert!(frame.iter().all(|&b| b == 0));
    }

    #[test]
    fn panel_mapping_singles() {
        // Singles: 4 cols, one player, all on pad 0; L,D,U,R -> 3,7,1,5.
        assert_eq!(smx_panel_for_col(4, 1, 0), Some((0, 3)));
        assert_eq!(smx_panel_for_col(4, 1, 1), Some((0, 7)));
        assert_eq!(smx_panel_for_col(4, 1, 2), Some((0, 1)));
        assert_eq!(smx_panel_for_col(4, 1, 3), Some((0, 5)));
    }

    #[test]
    fn panel_mapping_versus() {
        // Two players, 4 cols each: P1 -> pad 0, P2 -> pad 1.
        assert_eq!(smx_panel_for_col(4, 2, 0), Some((0, 3)));
        assert_eq!(smx_panel_for_col(4, 2, 3), Some((0, 5)));
        assert_eq!(smx_panel_for_col(4, 2, 4), Some((1, 3)));
        assert_eq!(smx_panel_for_col(4, 2, 7), Some((1, 5)));
    }

    #[test]
    fn panel_mapping_doubles() {
        // One human, 8 cols: first 4 on pad 0, next 4 on pad 1.
        assert_eq!(smx_panel_for_col(8, 1, 0), Some((0, 3)));
        assert_eq!(smx_panel_for_col(8, 1, 3), Some((0, 5)));
        assert_eq!(smx_panel_for_col(8, 1, 4), Some((1, 3)));
        assert_eq!(smx_panel_for_col(8, 1, 7), Some((1, 5)));
    }

    #[test]
    fn panel_mapping_rejects_bad_input() {
        assert_eq!(smx_panel_for_col(0, 1, 0), None);
        // A column beyond two pads' worth is out of range.
        assert_eq!(smx_panel_for_col(4, 2, 8), None);
    }

    #[test]
    fn worker_lifecycle_runs_without_a_pad() {
        // With no SMX manager initialized, set_lights and reenable are no-ops, so this
        // exercises the channel, thread, event handling, and a clean Drop/join.
        let lights = SmxPanelLights::new();
        lights.set_active(true);
        lights.set_background_for_pad(0, Some((bg_anim(&[1, 2], 0), Clock::Realtime)));
        lights.set_beat(1.5);
        lights.play_overlay(
            0,
            3,
            panel_anim(&[9], 0),
            OverlayDrive::OneShot { pressed: false },
        );
        lights.release_overlay(0, 3);
        lights.set_active(false);
        drop(lights); // joins the worker thread
    }

    // Compositor: background + overlay layers

    use crate::gifs::PANEL_RGB_BYTES;

    /// A full-pad animation whose frame `f` fills every LED of every panel
    /// with `values[f]`, 0.1s per frame.
    fn bg_anim(values: &[u8], loop_frame: usize) -> Arc<FullPadAnim> {
        Arc::new(FullPadAnim {
            panels: std::array::from_fn(|_| values.iter().map(|&v| [v; PANEL_RGB_BYTES]).collect()),
            durations: vec![0.1; values.len()],
            loop_frame,
            beats_per_loop: None,
        })
    }

    /// A per-panel animation filling every LED with `values[f]`, 0.1s per
    /// frame, with no outro (the loop region runs to the last frame).
    fn panel_anim(values: &[u8], loop_frame: usize) -> Arc<PanelAnim> {
        panel_anim_outro(values, loop_frame, values.len() - 1)
    }

    /// A per-panel animation with an explicit loop region `[loop_frame..=loop_end]`.
    fn panel_anim_outro(values: &[u8], loop_frame: usize, loop_end: usize) -> Arc<PanelAnim> {
        Arc::new(PanelAnim {
            frames: values.iter().map(|&v| [v; PANEL_RGB_BYTES]).collect(),
            durations: vec![0.1; values.len()],
            loop_frame,
            loop_end,
        })
    }

    /// First byte of a panel in the built frame (every test LED is uniform).
    fn led0(frame: &[u8; FRAME_BYTES], pad: usize, panel: usize) -> u8 {
        frame[panel_base(pad, panel)]
    }

    /// First byte of an arbitrary LED in a panel.
    fn led_byte(frame: &[u8; FRAME_BYTES], pad: usize, panel: usize, led: usize) -> u8 {
        frame[panel_base(pad, panel) + led * 3]
    }

    #[test]
    fn background_plays_loops_and_fills_both_pads() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10, 20], 0), Clock::Realtime)));
        fx.set_background_for_pad(1, Some((bg_anim(&[10, 20], 0), Clock::Realtime)));
        let frame = fx.tick(0.05);
        assert_eq!(led0(frame, 0, 0), 10);
        assert_eq!(led0(frame, 1, 8), 10);
        // Past the first frame's 0.1s: frame 1.
        let frame = fx.tick(0.1);
        assert_eq!(led0(frame, 0, 4), 20);
        // Past the last frame: loops back to frame 0.
        let frame = fx.tick(0.1);
        assert_eq!(led0(frame, 0, 4), 10);
    }

    #[test]
    fn background_loops_to_its_marker_frame() {
        let mut fx = PanelFx::new();
        // Frames 10,20,30 with the loop point at frame 1: the intro frame 10
        // must not repeat.
        fx.set_background_for_pad(0, Some((bg_anim(&[10, 20, 30], 1), Clock::Realtime)));
        let mut seen = Vec::new();
        for _ in 0..6 {
            seen.push(led0(fx.tick(0.1), 0, 0));
        }
        assert_eq!(seen, vec![20, 30, 20, 30, 20, 30]);
    }

    #[test]
    fn clearing_the_background_returns_to_black() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        assert_eq!(led0(fx.tick(0.0), 0, 0), 10);
        fx.set_background_for_pad(0, None);
        assert!(fx.tick(0.0).iter().all(|&b| b == 0));
    }

    #[test]
    fn beat_locked_background_maps_beats_to_frames() {
        let mut fx = PanelFx::new();
        // 4 frames over 2 beats: half a beat per frame, wall-clock dt ignored.
        fx.set_background_for_pad(0, Some((
            bg_anim(&[1, 2, 3, 4], 0),
            Clock::BeatLocked {
                beats_per_loop: 2.0,
            },
        )));
        for (beat, expected) in [
            (0.0, 1),
            (0.5, 2),
            (1.0, 3),
            (1.5, 4),
            (2.0, 1),  // wrapped
            (9.0, 3),  // phase 0.5 many loops in
            (-0.5, 4), // before beat 0: wraps backwards, stays in phase
        ] {
            fx.set_beat(beat);
            assert_eq!(led0(fx.tick(1.0), 0, 0), expected, "beat {beat}");
        }
    }

    #[test]
    fn beat_locked_background_loops_over_its_marker_region() {
        let mut fx = PanelFx::new();
        // Loop region is frames 1..4 (intro frame 0 excluded from the cycle).
        fx.set_background_for_pad(0, Some((
            bg_anim(&[1, 2, 3, 4], 1),
            Clock::BeatLocked {
                beats_per_loop: 3.0,
            },
        )));
        for (beat, expected) in [(0.0, 2), (1.0, 3), (2.0, 4), (3.0, 2)] {
            fx.set_beat(beat);
            assert_eq!(led0(fx.tick(1.0), 0, 0), expected, "beat {beat}");
        }
    }

    #[test]
    fn beat_locked_playback_halves_past_the_pad_frame_cap() {
        // 16 frames over 1 beat at 4 beats/s (240bpm) wants 64fps; the span
        // doubles twice to 4 beats (16fps, under the 30fps pad cap).
        let values: Vec<u8> = (1..=16).collect();
        let anim = bg_anim(&values, 0);
        // No tempo estimate: beat 2 is phase 0 of a 1-beat loop.
        assert_eq!(beat_frame(&anim, 1.0, 2.0, 0.0), 0);
        // Live tempo says 64fps: one pass now spans 4 beats, so beat 2 is
        // halfway through the frames.
        assert_eq!(beat_frame(&anim, 1.0, 2.0, 4.0), 8);
        // A tempo that fits keeps the authored span.
        assert_eq!(beat_frame(&anim, 1.0, 2.0, 1.0), 0);
    }

    #[test]
    fn one_shot_overlay_plays_once_then_reveals_the_background() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        fx.play_overlay(
            0,
            3,
            panel_anim(&[91, 92], 0),
            OverlayDrive::OneShot { pressed: false },
        );
        let frame = fx.tick(0.05);
        assert_eq!(led0(frame, 0, 3), 91);
        // Other panels keep showing the background while the overlay plays.
        assert_eq!(led0(frame, 0, 4), 10);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 92);
        // Past the last frame: the one-shot clears itself.
        assert_eq!(led0(fx.tick(0.1), 0, 3), 10);
    }

    #[test]
    fn looping_overlay_holds_until_ended() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(1, Some((bg_anim(&[10], 0), Clock::Realtime)));
        fx.play_overlay(1, 7, panel_anim(&[91, 92], 0), OverlayDrive::Sustain { resume: false });
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(led0(fx.tick(0.1), 1, 7));
        }
        assert_eq!(seen, vec![92, 91, 92, 91]);
        fx.release_overlay(1, 7);
        assert_eq!(led0(fx.tick(0.0), 1, 7), 10);
    }

    #[test]
    fn pressed_one_shot_with_outro_holds_its_loop_until_release() {
        let mut fx = PanelFx::new();
        // Intro frame 1, loop region [1..=2] (values 2,3), outro 4,5.
        fx.play_overlay(
            0,
            3,
            panel_anim_outro(&[1, 2, 3, 4, 5], 1, 2),
            OverlayDrive::OneShot { pressed: true },
        );
        let mut seen = vec![led0(fx.tick(0.05), 0, 3)];
        for _ in 0..4 {
            seen.push(led0(fx.tick(0.1), 0, 3));
        }
        // Intro once, then the loop region repeats while pressed.
        assert_eq!(seen, vec![1, 2, 3, 2, 3]);
        // One more loop frame, leaving playback mid-loop (frame value 2).
        assert_eq!(led0(fx.tick(0.1), 0, 3), 2);
        // Release mid-loop: playback snaps straight to the outro instead of
        // finishing the loop pass, plays it out, then clears itself.
        fx.release_overlay(0, 3);
        assert_eq!(led0(fx.tick(0.0), 0, 3), 4);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 5);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 0);
    }

    #[test]
    fn unpressed_one_shot_with_outro_plays_straight_through() {
        let mut fx = PanelFx::new();
        fx.play_overlay(
            0,
            3,
            panel_anim_outro(&[1, 2, 3], 0, 1),
            OverlayDrive::OneShot { pressed: false },
        );
        assert_eq!(led0(fx.tick(0.05), 0, 3), 1);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 2);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 3);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 0);
    }

    #[test]
    fn pressed_one_shot_without_outro_still_plays_once() {
        let mut fx = PanelFx::new();
        fx.play_overlay(
            0,
            3,
            panel_anim(&[91, 92], 0),
            OverlayDrive::OneShot { pressed: true },
        );
        assert_eq!(led0(fx.tick(0.05), 0, 3), 91);
        // Releasing a plain one-shot does not cut it short.
        fx.release_overlay(0, 3);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 92);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 0);
    }

    #[test]
    fn sustain_with_outro_releases_into_the_outro_and_reengages() {
        let mut fx = PanelFx::new();
        // Intro frame 1, single-frame loop region [1..=1], outro 3,4.
        let anim = panel_anim_outro(&[1, 2, 3, 4], 1, 1);
        fx.play_overlay(0, 5, anim.clone(), OverlayDrive::Sustain { resume: false });
        assert_eq!(led0(fx.tick(0.05), 0, 5), 1);
        // Holds on the loop frame while engaged.
        assert_eq!(led0(fx.tick(0.1), 0, 5), 2);
        assert_eq!(led0(fx.tick(0.1), 0, 5), 2);
        // Release: snaps straight to the outro.
        fx.release_overlay(0, 5);
        assert_eq!(led0(fx.tick(0.0), 0, 5), 3);
        // Re-press during the outro (freeze/roll cooldown): back to the loop.
        fx.play_overlay(0, 5, anim.clone(), OverlayDrive::Sustain { resume: false });
        assert_eq!(led0(fx.tick(0.0), 0, 5), 2);
        assert_eq!(led0(fx.tick(0.1), 0, 5), 2);
        // Release again: outro runs to the end and the overlay clears.
        fx.release_overlay(0, 5);
        assert_eq!(led0(fx.tick(0.0), 0, 5), 3);
        assert_eq!(led0(fx.tick(0.1), 0, 5), 4);
        assert_eq!(led0(fx.tick(0.1), 0, 5), 0);
        // A fresh engage after the overlay cleared restarts from the intro.
        fx.play_overlay(0, 5, anim, OverlayDrive::Sustain { resume: false });
        assert_eq!(led0(fx.tick(0.0), 0, 5), 1);
    }

    #[test]
    fn sustain_resume_starts_in_the_loop_region_not_the_intro() {
        let mut fx = PanelFx::new();
        // Intro frame 1, loop region [1..=2], outro 4. A resume engage (a
        // freeze re-press after its outro finished) skips the intro.
        let anim = panel_anim_outro(&[1, 2, 3, 4], 1, 2);
        fx.play_overlay(0, 5, anim, OverlayDrive::Sustain { resume: true });
        assert_eq!(led0(fx.tick(0.0), 0, 5), 2);
        // And it loops from there while engaged.
        assert_eq!(led0(fx.tick(0.1), 0, 5), 3);
        assert_eq!(led0(fx.tick(0.1), 0, 5), 2);
    }

    #[test]
    fn black_overlay_leds_are_transparent_and_reveal_lower_layers() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        // One overlay frame: LED 0 black (transparent), LED 1 lit (50).
        let mut f = [0u8; PANEL_RGB_BYTES];
        f[3] = 50;
        f[4] = 50;
        f[5] = 50;
        let anim = Arc::new(PanelAnim {
            frames: vec![f],
            durations: vec![0.1],
            loop_frame: 0,
            loop_end: 0,
        });
        fx.play_overlay(0, 3, anim, OverlayDrive::OneShot { pressed: true });
        let frame = fx.tick(0.0);
        // LED 0 is black in the overlay, so the background shows through.
        assert_eq!(led_byte(frame, 0, 3, 0), 10);
        // LED 1 is lit in the overlay, so the overlay wins there.
        assert_eq!(led_byte(frame, 0, 3, 1), 50);
    }

    #[test]
    fn an_active_overlay_suppresses_press_even_through_its_black_leds() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        // Press feedback fills the whole panel (would show 91 on its own).
        fx.play_press_overlay(0, 3, panel_anim(&[91], 0), OverlayDrive::Sustain {
            resume: false,
        });
        // A judgement overlay: LED 0 black, LED 1 lit (50).
        let mut f = [0u8; PANEL_RGB_BYTES];
        f[3] = 50;
        f[4] = 50;
        f[5] = 50;
        let anim = Arc::new(PanelAnim {
            frames: vec![f],
            durations: vec![0.1],
            loop_frame: 0,
            loop_end: 0,
        });
        fx.play_overlay(0, 3, anim, OverlayDrive::OneShot { pressed: true });
        let frame = fx.tick(0.0);
        // LED 0: the overlay is black there, but a judgement is active, so press
        // is suppressed and the background shows through (10), not the press (91).
        assert_eq!(led_byte(frame, 0, 3, 0), 10);
        // LED 1: the overlay is lit.
        assert_eq!(led_byte(frame, 0, 3, 1), 50);
    }

    #[test]
    fn press_overlay_sits_below_the_game_event_layers() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        // A press with no note lights the panel over the background.
        fx.play_press_overlay(0, 3, panel_anim(&[91, 92], 0), OverlayDrive::Sustain {
            resume: false,
        });
        assert_eq!(led0(fx.tick(0.05), 0, 3), 91);
        // A judgement overlay (a real hit) on the same panel draws over it.
        fx.play_overlay(0, 3, panel_anim(&[80], 0), OverlayDrive::OneShot { pressed: false });
        assert_eq!(led0(fx.tick(0.0), 0, 3), 80);
        // The one-shot clears; the still-held press feedback shows again.
        assert_eq!(led0(fx.tick(0.1), 0, 3), 92);
        // Release: the press feedback clears (no outro), revealing the background.
        fx.release_press_overlay(0, 3);
        assert_eq!(led0(fx.tick(1.0), 0, 3), 10);
    }

    #[test]
    fn roll_runs_forward_into_the_outro_and_holds_resetting_on_each_step() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        // Intro frame 1, loop region [1..=2] (values 2,3), outro 4,5.
        let anim = panel_anim_outro(&[1, 2, 3, 4, 5], 1, 2);
        // First step: intro, then it drains forward through the loop into the
        // outro (no wrapping) rather than looping like a freeze.
        fx.play_overlay(0, 3, anim.clone(), OverlayDrive::Roll { resume: false });
        let mut seen = vec![led0(fx.tick(0.05), 0, 3)];
        for _ in 0..5 {
            seen.push(led0(fx.tick(0.1), 0, 3));
        }
        // Intro 1, loop 2,3, outro 4,5, then holds on the last (drained) frame.
        assert_eq!(seen, vec![1, 2, 3, 4, 5, 5]);
        // A step snaps back to the loop start (life refilled), never the intro.
        fx.play_overlay(0, 3, anim, OverlayDrive::Roll { resume: true });
        assert_eq!(led0(fx.tick(0.0), 0, 3), 2);
        assert_eq!(led0(fx.tick(0.1), 0, 3), 3);
        // And it drains forward again from there.
        assert_eq!(led0(fx.tick(0.1), 0, 3), 4);
    }

    #[test]
    fn clear_panels_keeps_the_background_and_clear_all_drops_it() {
        let mut fx = PanelFx::new();
        fx.set_background_for_pad(0, Some((bg_anim(&[10], 0), Clock::Realtime)));
        fx.play_overlay(0, 1, panel_anim(&[91], 0), OverlayDrive::Sustain { resume: false });
        fx.play_press_overlay(0, 2, panel_anim(&[40], 0), OverlayDrive::Sustain { resume: false });

        fx.clear_panels();
        let frame = fx.tick(0.0);
        assert_eq!(led0(frame, 0, 1), 10);
        assert_eq!(led0(frame, 0, 2), 10);

        fx.clear_all();
        assert!(fx.tick(0.0).iter().all(|&b| b == 0));
    }

    #[test]
    fn overlay_on_out_of_range_panel_is_ignored() {
        let mut fx = PanelFx::new();
        fx.play_overlay(PADS, 0, panel_anim(&[91], 0), OverlayDrive::Sustain { resume: false });
        fx.play_overlay(0, PANELS, panel_anim(&[91], 0), OverlayDrive::Sustain { resume: false });
        assert!(fx.tick(0.0).iter().all(|&b| b == 0));
    }
}
