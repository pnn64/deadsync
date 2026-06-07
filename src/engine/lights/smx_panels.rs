//! StepManiaX pad panel lighting effects.
//!
//! Pure effect engine for the SMX 3x3 panels (`PanelFx`), the helpers that map a gameplay
//! column to a pad/panel and a judgement to a colour, and the 30Hz worker thread
//! (`SmxPanelLights`) that owns the effect state and hands frames to the SDK. Building the
//! RGB frame off the render thread keeps the colour math and timers out of the gameplay
//! per-frame path. The app-side diff that feeds the worker lives in `app::smx_panel_fx`.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use deadsync_rules::judgment::{JudgeGrade, judge_grade_ix};

use crate::engine::smx;

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

/// Tap flash durations, matching the on-screen column flash (`gameplay.rs:650`).
pub const FLASH_SECONDS_MISS: f32 = 0.16;
pub const FLASH_SECONDS_JUDGMENT: f32 = 0.33;

/// Per-grade panel colours, indexed by `judge_grade_ix`. Tuned for the SMX LED diffuser
/// (saturated, well-separated hues) rather than reusing the on-screen palette, which washes
/// out on the pad. The SDK scales output by ~0.67 on send.
const PAD_GRADE_RGB: [Rgb; 6] = [
    [0, 90, 255],  // Fantastic (blue; white for the FA+ inner window)
    [255, 140, 0], // Excellent (orange)
    [0, 220, 0],   // Great (green)
    [170, 0, 255], // Decent (purple)
    [255, 230, 0], // Way Off (yellow)
    [255, 0, 0],   // Miss (red)
];
/// Fantastic colour for the bright FA+ inner window.
const PAD_FANTASTIC_WHITE: Rgb = [255, 255, 255];

/// L, D, U, R direction columns mapped to 3x3 grid panel indices
/// (panel names: UL,U,UR,L,C,R,DL,D,DR).
const DIR_TO_PANEL: [usize; 4] = [3, 7, 1, 5];

/// Map a gameplay column to the SMX pad slot and panel index.
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

/// Flash duration for a judgement grade.
pub fn flash_duration(grade: JudgeGrade) -> f32 {
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
pub fn flash_color(grade: JudgeGrade, blue_fantastic: bool) -> Rgb {
    if grade == JudgeGrade::Fantastic && !blue_fantastic {
        PAD_FANTASTIC_WHITE
    } else {
        PAD_GRADE_RGB[judge_grade_ix(grade)]
    }
}

/// One panel's effect state: an optional sustained colour plus a decaying flash.
#[derive(Clone, Copy, Default)]
struct PanelState {
    hold: Option<Rgb>,
    flash_color: Rgb,
    flash_remaining_s: f32,
}

impl PanelState {
    fn render(&self) -> Rgb {
        if self.flash_remaining_s > 0.0 {
            self.flash_color
        } else if let Some(c) = self.hold {
            c
        } else {
            BLACK
        }
    }
}

/// Per-pad, per-panel effect state. Holds sustained colours and decaying flashes, and
/// builds the full both-pads RGB frame.
pub struct PanelFx {
    panels: [[PanelState; PANELS]; PADS],
}

impl Default for PanelFx {
    fn default() -> Self {
        Self::new()
    }
}

impl PanelFx {
    pub fn new() -> Self {
        Self {
            panels: [[PanelState::default(); PANELS]; PADS],
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

    /// Clear every flash and hold.
    pub fn clear_all(&mut self) {
        self.panels = [[PanelState::default(); PANELS]; PADS];
    }

    /// Advance flash timers by `dt_s` and build the both-pads RGB frame.
    pub fn tick(&mut self, dt_s: f32) -> [u8; FRAME_BYTES] {
        let dt = dt_s.max(0.0);
        let mut frame = [0u8; FRAME_BYTES];
        for pad in 0..PADS {
            for panel in 0..PANELS {
                let p = &mut self.panels[pad][panel];
                p.flash_remaining_s = (p.flash_remaining_s - dt).max(0.0);
                put_panel(&mut frame, pad, panel, p.render());
            }
        }
        frame
    }

    fn panel_mut(&mut self, pad: usize, panel: usize) -> Option<&mut PanelState> {
        self.panels.get_mut(pad)?.get_mut(panel)
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

// ─── 30Hz worker thread ──────────────────────────────────────────────────────

/// One frame interval at ~30Hz. The SDK also throttles its sends to this rate and
/// coalesces to the newest frame, so this only governs how often we rebuild a frame.
const FRAME_INTERVAL: Duration = Duration::from_micros(33_333);

/// Messages from the app diff to the worker. Small and `Copy`; pad/panel/colour are
/// resolved app-side so the worker stays free of gameplay and style knowledge.
#[derive(Clone, Copy, Debug)]
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
    /// Enter (true) or leave (false) the gameplay screens. Leaving hands the pad back to
    /// its firmware idle lighting.
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

    /// Mark whether gameplay is active. On `false` the worker clears the panels and hands
    /// the pad back to its firmware idle lighting.
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
        // Block up to one frame for the next event, then drain any burst behind it.
        match rx.recv_timeout(FRAME_INTERVAL) {
            Ok(ev) => {
                if handle(&mut fx, &mut active, ev) {
                    break 'outer;
                }
                while let Ok(ev) = rx.try_recv() {
                    if handle(&mut fx, &mut active, ev) {
                        break 'outer;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break 'outer,
        }

        let now = Instant::now();
        let dt = now.saturating_duration_since(last_tick);
        if dt >= FRAME_INTERVAL {
            last_tick = now;
            if active {
                let frame = fx.tick(dt.as_secs_f32());
                send_lights(&frame);
            }
        }
    }

    // On exit, leave the panels dark and restore the pad firmware idle lighting.
    fx.clear_all();
    let frame = fx.tick(0.0);
    send_lights(&frame);
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
        Ev::Active(a) => {
            *active = a;
            fx.clear_all();
            if !a {
                // Going idle: push one black frame and hand the pad back to firmware.
                let frame = fx.tick(0.0);
                send_lights(&frame);
                reenable_auto();
            }
        }
        Ev::Shutdown => return true,
    }
    false
}

fn send_lights(frame: &[u8]) {
    if let Some(m) = smx::manager() {
        m.set_lights(frame);
    }
}

fn reenable_auto() {
    if let Some(m) = smx::manager() {
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
    fn flash_color_uses_pad_palette() {
        // Normal (blue) Fantastic uses the blue pad colour, not white.
        assert_eq!(flash_color(JudgeGrade::Fantastic, true), PAD_GRADE_RGB[0]);
        // Bright (inner FA+) Fantastic is white.
        assert_eq!(
            flash_color(JudgeGrade::Fantastic, false),
            PAD_FANTASTIC_WHITE
        );
        // Other grades use their pad palette colour.
        assert_eq!(
            flash_color(JudgeGrade::Miss, false),
            PAD_GRADE_RGB[judge_grade_ix(JudgeGrade::Miss)]
        );
        // Every grade colour is distinct.
        for i in 0..PAD_GRADE_RGB.len() {
            for j in (i + 1)..PAD_GRADE_RGB.len() {
                assert_ne!(PAD_GRADE_RGB[i], PAD_GRADE_RGB[j]);
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
        lights.set_active(false);
        drop(lights); // joins the worker thread
    }
}
