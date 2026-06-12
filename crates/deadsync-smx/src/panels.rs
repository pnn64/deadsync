//! StepManiaX pad panel lighting effects.
//!
//! Pure effect engine for the SMX 3x3 panels (`PanelFx`), the helper that maps a logical
//! column to a pad/panel, and the 30Hz worker thread (`SmxPanelLights`) that owns the
//! effect state and hands frames to the SDK. Building the RGB frame off the render thread
//! keeps the colour math and timers out of the per-frame path. The app-side diff and
//! palette live in `app::smx_panel_fx`.
//!
//! `PanelFx` composites two layers over black: an optional full-pad background
//! animation (a preloaded GIF from `gifs::GifRegistry`, played realtime or locked
//! to the song beat) and per-panel effects on top of it. Per panel, the priority
//! is: GIF overlay (judgement animation), then solid flash, then solid hold, then
//! the background, then black.

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
    /// Freeze/roll sustain: loops in its loop region until `release_overlay`,
    /// which plays the outro if the GIF has one and clears otherwise. With
    /// `resume`, a fresh overlay starts at the loop region instead of the
    /// intro (a freeze/roll re-press within the grace period whose outro
    /// already finished and cleared the overlay).
    Sustain { resume: bool },
}

/// A per-panel GIF animation playing over the background. See `OverlayDrive`
/// for the press/release-driven lifecycle.
struct Overlay {
    anim: Arc<PanelAnim>,
    /// Sustain overlays loop until released even without an outro segment.
    sustain: bool,
    /// While engaged, playback wraps from `loop_end` back to `loop_frame`.
    /// Cleared by `release_overlay`; re-set when a sustain re-engages.
    engaged: bool,
    frame: usize,
    time_in_frame: f32,
}

/// One panel's effect state: an optional GIF overlay, a sustained colour, and
/// a decaying flash.
#[derive(Default)]
struct PanelState {
    overlay: Option<Overlay>,
    hold: Option<Rgb>,
    flash_color: Rgb,
    flash_remaining_s: f32,
}

/// Per-pad, per-panel effect state plus the shared background layer. Builds
/// the full both-pads RGB frame each tick.
pub struct PanelFx {
    panels: [[PanelState; PANELS]; PADS],
    background: Option<Background>,
    /// Latest song beat position, used by a `BeatLocked` background.
    beat: f32,
    /// Beat position at the previous tick, for the tempo estimate.
    prev_beat: f32,
    /// Smoothed live tempo estimate in beats per second, used to cap a
    /// beat-locked background's frame rate (see `beat_frame`).
    beat_rate: f32,
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
            background: None,
            beat: 0.0,
            prev_beat: 0.0,
            beat_rate: 0.0,
            frame: [0u8; FRAME_BYTES],
        }
    }

    /// Start a transient flash on a panel. Out-of-range indices are ignored.
    pub fn flash(&mut self, pad: usize, panel: usize, color: Rgb, dur_s: f32) {
        if let Some(p) = self.panel_mut(pad, panel) {
            p.flash_color = color;
            p.flash_remaining_s = dur_s.max(0.0);
        }
    }

    /// Set a sustained colour on a panel (an active freeze or roll).
    pub fn hold_start(&mut self, pad: usize, panel: usize, color: Rgb) {
        if let Some(p) = self.panel_mut(pad, panel) {
            p.hold = Some(color);
        }
    }

    /// Clear a panel's sustained colour.
    pub fn hold_end(&mut self, pad: usize, panel: usize) {
        if let Some(p) = self.panel_mut(pad, panel) {
            p.hold = None;
        }
    }

    /// Set or clear the full-pad background animation (both pads show it).
    /// Playback restarts from the first frame.
    pub fn set_background(&mut self, background: Option<(Arc<FullPadAnim>, Clock)>) {
        self.background = background.map(|(anim, clock)| Background {
            anim,
            clock,
            frame: 0,
            time_in_frame: 0.0,
        });
    }

    /// Update the song beat position driving a `BeatLocked` background.
    pub fn set_beat(&mut self, beat: f32) {
        self.beat = beat;
    }

    /// Start a GIF animation on a panel, over the background. Out-of-range
    /// indices are ignored. A sustain re-playing the animation its panel is
    /// already showing (a freeze/roll re-press during the release cooldown)
    /// re-engages it, jumping from the outro back into the loop region
    /// instead of restarting from the intro.
    pub fn play_overlay(
        &mut self,
        pad: usize,
        panel: usize,
        anim: Arc<PanelAnim>,
        drive: OverlayDrive,
    ) {
        let Some(p) = self.panel_mut(pad, panel) else {
            return;
        };
        if matches!(drive, OverlayDrive::Sustain { .. })
            && let Some(o) = &mut p.overlay
            && o.sustain
            && Arc::ptr_eq(&o.anim, &anim)
        {
            o.engaged = true;
            if o.frame > o.anim.loop_end {
                o.frame = o.anim.loop_frame.min(o.anim.loop_end);
                o.time_in_frame = 0.0;
            }
            return;
        }
        let (sustain, engaged, resume) = match drive {
            OverlayDrive::OneShot { pressed } => (false, pressed, false),
            OverlayDrive::Sustain { resume } => (true, true, resume),
        };
        let frame = if resume {
            anim.loop_frame.min(anim.frames.len() - 1)
        } else {
            0
        };
        p.overlay = Some(Overlay {
            anim,
            sustain,
            engaged,
            frame,
            time_in_frame: 0.0,
        });
    }

    /// Release a panel's GIF overlay (panel lift / freeze-roll disengage): an
    /// overlay with an outro segment snaps from its loop region straight to
    /// the outro (rather than finishing the current loop pass) and clears
    /// itself when the outro ends, a sustain without one clears immediately
    /// (revealing the layers under it), and a plain one-shot is unaffected.
    pub fn release_overlay(&mut self, pad: usize, panel: usize) {
        let Some(p) = self.panel_mut(pad, panel) else {
            return;
        };
        let Some(o) = &mut p.overlay else {
            return;
        };
        if o.anim.has_outro() {
            o.engaged = false;
            if o.frame >= o.anim.loop_frame && o.frame <= o.anim.loop_end {
                o.frame = o.anim.loop_end + 1;
                o.time_in_frame = 0.0;
            }
        } else if o.sustain {
            p.overlay = None;
        }
    }

    /// Clear every per-panel effect (overlays, flashes, holds), keeping the
    /// background. Used when (re)entering active lighting on a screen.
    pub fn clear_panels(&mut self) {
        self.panels = std::array::from_fn(|_| std::array::from_fn(|_| PanelState::default()));
    }

    /// Full reset: every per-panel effect plus the background. Used when
    /// handing the pad back to its firmware lighting.
    pub fn clear_all(&mut self) {
        self.clear_panels();
        self.background = None;
    }

    /// Advance all playback by `dt_s`, rebuild the reused both-pads RGB frame, and return
    /// it. Every panel is filled, so all 1350 bytes are overwritten and the buffer needs
    /// no clear.
    pub fn tick(&mut self, dt_s: f32) -> &[u8; FRAME_BYTES] {
        let dt = dt_s.max(0.0);
        self.update_beat_rate(dt);
        let bg_frame = advance_background(&mut self.background, self.beat, self.beat_rate, dt);
        for pad in 0..PADS {
            for panel in 0..PANELS {
                let p = &mut self.panels[pad][panel];
                p.flash_remaining_s = (p.flash_remaining_s - dt).max(0.0);
                advance_overlay(&mut p.overlay, dt);
                // Layer priority: overlay, flash, hold, background, black.
                if let Some(o) = &p.overlay {
                    put_panel_rgb(&mut self.frame, pad, panel, &o.anim.frames[o.frame]);
                } else if p.flash_remaining_s > 0.0 {
                    put_panel(&mut self.frame, pad, panel, p.flash_color);
                } else if let Some(c) = p.hold {
                    put_panel(&mut self.frame, pad, panel, c);
                } else if let (Some(bg), Some(f)) = (&self.background, bg_frame) {
                    put_panel_rgb(&mut self.frame, pad, panel, &bg.anim.panels[panel][f]);
                } else {
                    put_panel(&mut self.frame, pad, panel, BLACK);
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

/// Advance a panel overlay; one that plays past its last frame clears itself
/// to reveal the layers under it.
fn advance_overlay(overlay: &mut Option<Overlay>, dt: f32) {
    let Some(o) = overlay else { return };
    o.time_in_frame += dt;
    while o.time_in_frame >= o.anim.durations[o.frame].max(MIN_FRAME_DURATION_S) {
        o.time_in_frame -= o.anim.durations[o.frame].max(MIN_FRAME_DURATION_S);
        let last = o.anim.frames.len() - 1;
        let loop_end = o.anim.loop_end.min(last);
        // An engaged overlay wraps at the loop end when there is an outro to
        // hold back, and a sustain wraps there regardless; everything else
        // runs through to the last frame and clears.
        let wraps = o.engaged && (o.sustain || o.anim.has_outro());
        if o.frame == loop_end && wraps {
            o.frame = o.anim.loop_frame.min(loop_end);
        } else if o.frame >= last {
            *overlay = None;
            return;
        } else {
            o.frame += 1;
        }
    }
}

/// Write a uniform colour into all LEDs of one panel in the frame buffer.
/// A uniform fill is order-independent, so the SDK's per-panel LED ordering does not matter.
pub fn put_panel(frame: &mut [u8; FRAME_BYTES], pad: usize, panel: usize, color: Rgb) {
    if pad >= PADS || panel >= PANELS {
        return;
    }
    let base = pad * BYTES_PER_PAD + panel * (LEDS_PER_PANEL * 3);
    for led in 0..LEDS_PER_PANEL {
        let o = base + led * 3;
        frame[o..o + 3].copy_from_slice(&color);
    }
}

/// Write one panel's decoded GIF LEDs (already in the SDK's outer-then-inner
/// order) into the frame buffer.
pub fn put_panel_rgb(frame: &mut [u8; FRAME_BYTES], pad: usize, panel: usize, rgb: &PanelFrame) {
    if pad >= PADS || panel >= PANELS {
        return;
    }
    let base = pad * BYTES_PER_PAD + panel * (LEDS_PER_PANEL * 3);
    frame[base..base + rgb.len()].copy_from_slice(rgb);
}

// 30Hz worker thread

/// One frame interval at ~30Hz. The SDK also throttles its sends to this rate and
/// coalesces to the newest frame, so this only governs how often we rebuild a frame.
const FRAME_INTERVAL: Duration = Duration::from_micros(33_333);

/// Messages from the app to the worker. Pad/panel/colour/animation are resolved
/// app-side (including registry lookups) so the worker stays free of app policy
/// and style knowledge; animations travel as cheap `Arc` handles.
enum Ev {
    Flash {
        pad: u8,
        panel: u8,
        color: Rgb,
        dur_s: f32,
    },
    HoldStart {
        pad: u8,
        panel: u8,
        color: Rgb,
    },
    HoldEnd {
        pad: u8,
        panel: u8,
    },
    /// Set or clear the full-pad background animation.
    Background(Option<(Arc<FullPadAnim>, Clock)>),
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
    /// Enter (true) or leave (false) active panel effect ownership. Leaving hands the pad
    /// back to its firmware idle lighting.
    Active(bool),
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

    /// Flash a panel its judgement colour for `dur_s` seconds.
    pub fn flash(&self, pad: usize, panel: usize, color: Rgb, dur_s: f32) {
        self.send(Ev::Flash {
            pad: pad as u8,
            panel: panel as u8,
            color,
            dur_s,
        });
    }

    /// Light a panel a sustained colour (an active freeze or roll).
    pub fn hold_start(&self, pad: usize, panel: usize, color: Rgb) {
        self.send(Ev::HoldStart {
            pad: pad as u8,
            panel: panel as u8,
            color,
        });
    }

    /// Clear a panel's sustained colour.
    pub fn hold_end(&self, pad: usize, panel: usize) {
        self.send(Ev::HoldEnd {
            pad: pad as u8,
            panel: panel as u8,
        });
    }

    /// Set or clear the full-pad background animation (both pads show it).
    pub fn set_background(&self, background: Option<(Arc<FullPadAnim>, Clock)>) {
        self.send(Ev::Background(background));
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

    /// Mark whether panel effects are active. On `false` the worker clears the panels and
    /// hands the pad back to its firmware idle lighting.
    pub fn set_active(&self, active: bool) {
        self.send(Ev::Active(active));
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
        Ev::Flash {
            pad,
            panel,
            color,
            dur_s,
        } => fx.flash(pad.into(), panel.into(), color, dur_s),
        Ev::HoldStart { pad, panel, color } => fx.hold_start(pad.into(), panel.into(), color),
        Ev::HoldEnd { pad, panel } => fx.hold_end(pad.into(), panel.into()),
        Ev::Background(bg) => fx.set_background(bg),
        Ev::Beat(beat) => fx.set_beat(beat),
        Ev::Overlay {
            pad,
            panel,
            anim,
            drive,
        } => fx.play_overlay(pad.into(), panel.into(), anim, drive),
        Ev::OverlayRelease { pad, panel } => fx.release_overlay(pad.into(), panel.into()),
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
    fn flash_shows_on_all_panel_leds_then_decays() {
        let mut fx = PanelFx::new();
        fx.flash(0, 3, [10, 20, 30], 0.1);
        let base = panel_base(0, 3);
        // Within the flash window: first and last LED of the panel share the colour.
        let frame = fx.tick(0.05);
        assert_eq!(&frame[base..base + 3], &[10, 20, 30]);
        let last = base + (LEDS_PER_PANEL - 1) * 3;
        assert_eq!(&frame[last..last + 3], &[10, 20, 30]);
        // Past the remaining flash time: back to black.
        let frame = fx.tick(0.1);
        assert_eq!(&frame[base..base + 3], &[0, 0, 0]);
    }

    #[test]
    fn hold_persists_until_cleared() {
        let mut fx = PanelFx::new();
        fx.hold_start(1, 7, [1, 2, 3]);
        let base = panel_base(1, 7);
        for _ in 0..10 {
            let frame = fx.tick(0.1);
            assert_eq!(&frame[base..base + 3], &[1, 2, 3]);
        }
        fx.hold_end(1, 7);
        let frame = fx.tick(0.1);
        assert_eq!(&frame[base..base + 3], &[0, 0, 0]);
    }

    #[test]
    fn flash_overrides_hold_then_reveals_hold() {
        let mut fx = PanelFx::new();
        fx.hold_start(0, 5, [1, 1, 1]);
        fx.flash(0, 5, [9, 9, 9], 0.1);
        let base = panel_base(0, 5);
        let frame = fx.tick(0.05);
        assert_eq!(&frame[base..base + 3], &[9, 9, 9]);
        let frame = fx.tick(0.1);
        assert_eq!(&frame[base..base + 3], &[1, 1, 1]);
    }

    #[test]
    fn clear_all_blacks_everything() {
        let mut fx = PanelFx::new();
        fx.hold_start(0, 0, [5, 5, 5]);
        fx.flash(1, 8, [6, 6, 6], 1.0);
        fx.clear_all();
        let frame = fx.tick(0.0);
        assert!(frame.iter().all(|&b| b == 0));
    }

    #[test]
    fn out_of_range_pad_or_panel_is_ignored() {
        let mut fx = PanelFx::new();
        fx.flash(PADS, 0, [1, 2, 3], 1.0);
        fx.flash(0, PANELS, [1, 2, 3], 1.0);
        fx.hold_start(5, 5, [1, 2, 3]);
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
        lights.flash(0, 3, [1, 2, 3], 0.05);
        lights.hold_start(0, 5, [4, 5, 6]);
        lights.hold_end(0, 5);
        lights.set_background(Some((bg_anim(&[1, 2], 0), Clock::Realtime)));
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

    #[test]
    fn background_plays_loops_and_fills_both_pads() {
        let mut fx = PanelFx::new();
        fx.set_background(Some((bg_anim(&[10, 20], 0), Clock::Realtime)));
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
        fx.set_background(Some((bg_anim(&[10, 20, 30], 1), Clock::Realtime)));
        let mut seen = Vec::new();
        for _ in 0..6 {
            seen.push(led0(fx.tick(0.1), 0, 0));
        }
        assert_eq!(seen, vec![20, 30, 20, 30, 20, 30]);
    }

    #[test]
    fn clearing_the_background_returns_to_black() {
        let mut fx = PanelFx::new();
        fx.set_background(Some((bg_anim(&[10], 0), Clock::Realtime)));
        assert_eq!(led0(fx.tick(0.0), 0, 0), 10);
        fx.set_background(None);
        assert!(fx.tick(0.0).iter().all(|&b| b == 0));
    }

    #[test]
    fn beat_locked_background_maps_beats_to_frames() {
        let mut fx = PanelFx::new();
        // 4 frames over 2 beats: half a beat per frame, wall-clock dt ignored.
        fx.set_background(Some((
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
        fx.set_background(Some((
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
        fx.set_background(Some((bg_anim(&[10], 0), Clock::Realtime)));
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
        fx.set_background(Some((bg_anim(&[10], 0), Clock::Realtime)));
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
    fn overlay_outranks_flash_and_hold_over_background() {
        let mut fx = PanelFx::new();
        fx.set_background(Some((bg_anim(&[10], 0), Clock::Realtime)));
        fx.hold_start(0, 5, [30, 30, 30]);
        fx.flash(0, 5, [40, 40, 40], 1.0);
        fx.play_overlay(0, 5, panel_anim(&[91], 0), OverlayDrive::Sustain { resume: false });
        // Overlay wins over the flash and hold.
        assert_eq!(led0(fx.tick(0.01), 0, 5), 91);
        fx.release_overlay(0, 5);
        // Then the flash, then the hold, then the background.
        assert_eq!(led0(fx.tick(0.01), 0, 5), 40);
        assert_eq!(led0(fx.tick(2.0), 0, 5), 30);
        fx.hold_end(0, 5);
        assert_eq!(led0(fx.tick(0.0), 0, 5), 10);
    }

    #[test]
    fn clear_panels_keeps_the_background_and_clear_all_drops_it() {
        let mut fx = PanelFx::new();
        fx.set_background(Some((bg_anim(&[10], 0), Clock::Realtime)));
        fx.play_overlay(0, 1, panel_anim(&[91], 0), OverlayDrive::Sustain { resume: false });
        fx.flash(0, 2, [40, 40, 40], 1.0);

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
