use crate::act;
use crate::screens::gameplay::GameplayCoreState;
use deadlib_present::actors::Actor;
use deadlib_present::space::screen_height;

use super::notefield::gameplay_mods_text;

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
const TRANSITION_IN_DURATION: f32 = 2.0;
const SL_DISPLAY_MODS_HOLD_S: f32 = 5.0;
const MODS_FADE_S: f32 = 0.5;
const DISPLAY_MODS_ZOOM: f32 = 0.8;
pub(super) const DISPLAY_MODS_WRAP_WIDTH_PX: f32 = 125.0;
const DISPLAY_MODS_LINE_STEP: f32 = 15.0;
const DISPLAY_MODS_WARNING_W: f32 = 90.0;
const DISPLAY_MODS_WARNING_H: f32 = 30.0;
const DISPLAY_MODS_WARNING_ZOOM: f32 = 1.5;

#[derive(Clone, Copy)]
pub(super) struct DisplayModsFrame {
    pub hidden: bool,
    pub warn_cmod_for_itl_chart: bool,
    pub elapsed_screen_s: f32,
    pub playfield_center_x: f32,
    pub notefield_offset_y: f32,
}

/// Compose Simply Love's concrete DisplayMods chrome between the canonical
/// field and HUD passes.
pub(super) fn compose(
    actors: &mut Vec<Actor>,
    state: &GameplayCoreState,
    player_idx: usize,
    frame: DisplayModsFrame,
) {
    if frame.hidden {
        return;
    }
    let alpha = display_mods_alpha(frame.elapsed_screen_s);
    if alpha <= 0.0 {
        return;
    }

    let mods_text = gameplay_mods_text(state, player_idx);
    let mods_line_y = screen_height() * 0.25 * 1.3 + frame.notefield_offset_y;
    let mods_line_count = mods_text
        .split(", ")
        .filter(|part| !part.is_empty())
        .count()
        .max(1) as f32;
    if !mods_text.is_empty() {
        actors.push(act!(text:
            font("miso"): settext(mods_text):
            align(0.5, 0.0): xy(frame.playfield_center_x, mods_line_y):
            zoom(DISPLAY_MODS_ZOOM): wrapwidthpixels(DISPLAY_MODS_WRAP_WIDTH_PX): horizalign(center):
            shadowcolor(0.0, 0.0, 0.0, 1.0):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, alpha):
            z(84)
        ));
    }
    if frame.warn_cmod_for_itl_chart {
        let warning_y = mods_line_y + DISPLAY_MODS_LINE_STEP * mods_line_count;
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(frame.playfield_center_x, warning_y):
            setsize(DISPLAY_MODS_WARNING_W, DISPLAY_MODS_WARNING_H):
            diffuse(0.0, 0.0, 0.0, 0.8 * alpha):
            z(84)
        ));
        actors.push(act!(text:
            font("miso"): settext("CMod On"):
            align(0.5, 0.5): xy(frame.playfield_center_x, warning_y):
            zoom(DISPLAY_MODS_WARNING_ZOOM):
            diffuse(1.0, 0.0, 0.0, alpha):
            z(85)
        ));
    }
}

fn display_mods_alpha(elapsed_screen_s: f32) -> f32 {
    // Simply Love holds for 5s behind a 2s cover. DeadSync's cover uses the
    // same duration today; keep the adjustment explicit for parity if it changes.
    const SL_GAMEPLAY_IN_COVER_S: f32 = 2.0;
    let hold_adjust = (SL_GAMEPLAY_IN_COVER_S - TRANSITION_IN_DURATION).max(0.0);
    let hold_s = (SL_DISPLAY_MODS_HOLD_S - hold_adjust).max(0.0);
    if elapsed_screen_s <= hold_s {
        1.0
    } else if elapsed_screen_s < hold_s + MODS_FADE_S {
        let t = ((elapsed_screen_s - hold_s) / MODS_FADE_S).clamp(0.0, 1.0);
        (1.0 - t) * (1.0 - t)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::display_mods_alpha;

    #[test]
    fn display_mods_holds_then_decelerates_out() {
        assert_eq!(display_mods_alpha(5.0), 1.0);
        assert!((display_mods_alpha(5.25) - 0.25).abs() < f32::EPSILON);
        assert_eq!(display_mods_alpha(5.5), 0.0);
    }
}
