use crate::act;
use crate::screens::evaluation::ScoreInfo;
use crate::ui::actors::Actor;
use crate::ui::color;

pub fn build_modifiers_pane(score_info: &ScoreInfo, bar_center_x: f32, bar_width: f32) -> Vec<Actor> {
    let frame_center_y = crate::core::space::screen_center_y() + 200.5;
    let font_zoom = 0.7;

    // Simply Love places the modifiers text 10px from the bar's left edge.
    // (For a 300px bar this is equivalent to `center_x - 140`.)
    let text_x = bar_center_x - (bar_width * 0.5) + 10.0;
    let text_y = frame_center_y - 5.0;

    let speed_mod_text = score_info.speed_mod.to_string();
    let mut parts = Vec::new();
    parts.push(speed_mod_text);
    // Show active scroll modifiers in a fixed order, matching Simply Love's
    // preference for listing Reverse before the perspective.
    let scroll = score_info.scroll_option;
    if scroll.contains(crate::game::profile::ScrollOption::Reverse) {
        parts.push("Reverse".to_string());
    }
    if scroll.contains(crate::game::profile::ScrollOption::Split) {
        parts.push("Split".to_string());
    }
    if scroll.contains(crate::game::profile::ScrollOption::Alternate) {
        parts.push("Alternate".to_string());
    }
    if scroll.contains(crate::game::profile::ScrollOption::Cross) {
        parts.push("Cross".to_string());
    }
    if scroll.contains(crate::game::profile::ScrollOption::Centered) {
        parts.push("Centered".to_string());
    }
    parts.push("Overhead".to_string());
    let final_text = parts.join(", ");

    let bg = color::rgba_hex("#1E282F");
    vec![
        act!(quad:
            align(0.5, 0.5):
            xy(bar_center_x, frame_center_y):
            zoomto(bar_width, 26.0):
            diffuse(bg[0], bg[1], bg[2], 1.0):
            z(101)
        ),
        act!(text:
            font("miso"):
            settext(final_text):
            align(0.0, 0.0):
            xy(text_x, text_y):
            zoom(font_zoom):
            z(102):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ),
    ]
}
