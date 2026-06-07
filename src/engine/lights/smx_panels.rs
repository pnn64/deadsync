//! StepManiaX pad panel lighting effects.
//!
//! Pure effect engine for the SMX 3x3 panels: per-panel flash and sustained-hold state,
//! plus the helpers that map a gameplay column to a pad/panel and a judgement to a colour.
//! Building the RGB frame here (and, in a later phase, on a 30Hz worker thread) keeps the
//! colour math and timers off the gameplay and render paths.
//!
//! The 30Hz worker thread and the app-side diff that feed this engine are added in later
//! phases; for now this is a self-contained, unit-tested module.
#![allow(dead_code)] // Items are wired into the worker + app in later phases; remove then.

use deadsync_rules::judgment::{JudgeGrade, judge_grade_ix};

use crate::engine::present::color::{JUDGMENT_FA_PLUS_WHITE_RGBA, JUDGMENT_RGBA};

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

/// Colour for a tap judgement flash, mirroring the on-screen column flash.
///
/// `blue_fantastic` is the flag gameplay records on `ActiveColumnFlash` (`gameplay.rs:6772`):
/// it is `true` for the blue (outer) Fantastic and `false` for the bright FA+ inner window,
/// which shows white. All other grades use their palette colour.
pub fn flash_color(grade: JudgeGrade, blue_fantastic: bool) -> Rgb {
    if grade == JudgeGrade::Fantastic && !blue_fantastic {
        rgba_to_rgb(JUDGMENT_FA_PLUS_WHITE_RGBA)
    } else {
        rgba_to_rgb(JUDGMENT_RGBA[judge_grade_ix(grade)])
    }
}

/// Convert a normalized f32 RGBA (0..=1) to raw u8 RGB, dropping alpha.
fn rgba_to_rgb(c: [f32; 4]) -> Rgb {
    [chan(c[0]), chan(c[1]), chan(c[2])]
}

fn chan(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
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
    fn flash_color_matches_screen_rules() {
        // Normal (blue) Fantastic uses the blue palette colour, not white.
        let blue = flash_color(JudgeGrade::Fantastic, true);
        assert_eq!(
            blue,
            rgba_to_rgb(JUDGMENT_RGBA[judge_grade_ix(JudgeGrade::Fantastic)])
        );
        // Bright (inner FA+) Fantastic is white.
        let white = flash_color(JudgeGrade::Fantastic, false);
        assert_eq!(white, rgba_to_rgb(JUDGMENT_FA_PLUS_WHITE_RGBA));
        // Other grades use their palette colour regardless of the flag.
        let miss = flash_color(JudgeGrade::Miss, false);
        assert_eq!(miss, rgba_to_rgb(JUDGMENT_RGBA[judge_grade_ix(JudgeGrade::Miss)]));
    }

    #[test]
    fn flash_duration_miss_is_shorter() {
        assert_eq!(flash_duration(JudgeGrade::Miss), FLASH_SECONDS_MISS);
        assert_eq!(flash_duration(JudgeGrade::Fantastic), FLASH_SECONDS_JUDGMENT);
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
}
