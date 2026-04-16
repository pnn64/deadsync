use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::screens::evaluation::ScoreInfo;
use std::sync::Arc;

pub fn build_modifiers_pane(
    score_info: &ScoreInfo,
    bar_center_x: f32,
    bar_width: f32,
) -> Vec<Actor> {
    build_modifiers_pane_with_text(score_info.mods_text.clone(), bar_center_x, bar_width)
}

fn build_modifiers_pane_with_text(
    mods_text: Arc<str>,
    bar_center_x: f32,
    bar_width: f32,
) -> Vec<Actor> {
    let frame_center_y = crate::engine::space::screen_center_y() + 200.5;
    let font_zoom = 0.7;

    // Simply Love places the modifiers text 10px from the bar's left edge.
    // (For a 300px bar this is equivalent to `center_x - 140`.)
    let text_x = bar_center_x - (bar_width * 0.5) + 10.0;
    let text_y = frame_center_y - 5.0;

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
            settext(mods_text):
            align(0.0, 0.0):
            xy(text_x, text_y):
            zoom(font_zoom):
            z(102):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::build_modifiers_pane_with_text;
    use crate::engine::present::actors::Actor;
    use std::sync::Arc;

    #[test]
    fn modifiers_pane_uses_supplied_mod_string() {
        let actors = build_modifiers_pane_with_text(
            Arc::<str>::from("M700, 40% Mini, Overhead, cel"),
            320.0,
            300.0,
        );
        let Some(Actor::Text { content, .. }) = actors
            .into_iter()
            .find(|actor| matches!(actor, Actor::Text { .. }))
        else {
            panic!("expected a text actor in the modifiers pane");
        };
        assert_eq!(content.as_str(), "M700, 40% Mini, Overhead, cel");
    }
}
