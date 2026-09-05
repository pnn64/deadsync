use crate::act;
use crate::screens::evaluation::ScoreInfo;
use deadlib_present::actors::{Actor, SizeSpec};
use std::cell::RefCell;
use std::sync::Arc;

use super::utils::eval_style_alpha;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ModifiersPaneCacheKey {
    center_x_bits: u32,
    width_bits: u32,
    transparent: bool,
}

#[derive(Clone)]
pub(crate) struct ModifiersPanePresentation {
    text: Arc<str>,
    cached: RefCell<Option<(ModifiersPaneCacheKey, Arc<[Actor]>)>>,
}

impl ModifiersPanePresentation {
    pub(crate) fn new(score_info: &ScoreInfo) -> Self {
        Self {
            text: Arc::clone(&score_info.mods_text),
            cached: RefCell::new(None),
        }
    }

    fn cached_actors(&self, bar_center_x: f32, bar_width: f32, transparent: bool) -> Arc<[Actor]> {
        let key = ModifiersPaneCacheKey {
            center_x_bits: bar_center_x.to_bits(),
            width_bits: bar_width.to_bits(),
            transparent,
        };
        if let Some((_, actors)) = self
            .cached
            .borrow()
            .as_ref()
            .filter(|(cached, _)| *cached == key)
        {
            return Arc::clone(actors);
        }

        let actors = Arc::from(build_modifiers_pane_with_text(
            Arc::clone(&self.text),
            bar_center_x,
            bar_width,
            transparent,
        ));
        *self.cached.borrow_mut() = Some((key, Arc::clone(&actors)));
        actors
    }
}

#[must_use]
pub fn build_modifiers_pane(
    score_info: &ScoreInfo,
    bar_center_x: f32,
    bar_width: f32,
    transparent: bool,
) -> Vec<Actor> {
    build_modifiers_pane_with_text(
        score_info.mods_text.clone(),
        bar_center_x,
        bar_width,
        transparent,
    )
}

/// Appends the modifiers bar directly into the screen's retained actor buffer.
pub fn push_modifiers_pane(
    out: &mut Vec<Actor>,
    score_info: &ScoreInfo,
    bar_center_x: f32,
    bar_width: f32,
    transparent: bool,
) {
    push_modifiers_pane_with_text(
        out,
        Arc::clone(&score_info.mods_text),
        bar_center_x,
        bar_width,
        transparent,
    );
}

pub(crate) fn push_cached_modifiers_pane(
    out: &mut Vec<Actor>,
    presentation: &ModifiersPanePresentation,
    bar_center_x: f32,
    bar_width: f32,
    transparent: bool,
) {
    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children: presentation.cached_actors(bar_center_x, bar_width, transparent),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

fn build_modifiers_pane_with_text(
    mods_text: Arc<str>,
    bar_center_x: f32,
    bar_width: f32,
    transparent: bool,
) -> Vec<Actor> {
    let mut out = Vec::with_capacity(2);
    push_modifiers_pane_with_text(&mut out, mods_text, bar_center_x, bar_width, transparent);
    out
}

fn push_modifiers_pane_with_text(
    out: &mut Vec<Actor>,
    mods_text: Arc<str>,
    bar_center_x: f32,
    bar_width: f32,
    transparent: bool,
) {
    let frame_center_y = deadlib_present::space::screen_center_y() + 200.5;
    let font_zoom = 0.7;

    // Simply Love places the modifiers text 10px from the bar's left edge.
    // (For a 300px bar this is equivalent to `center_x - 140`.)
    let text_x = bar_width.mul_add(-0.5, bar_center_x) + 10.0;
    let text_y = frame_center_y - 5.0;

    let bg = deadlib_present::color::rgba_hex("#1E282F");
    let bg_alpha = eval_style_alpha(transparent, 1.0, 0.75);
    out.reserve(2);
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_center_x, frame_center_y):
        zoomto(bar_width, 26.0):
        diffuse(bg[0], bg[1], bg[2], bg_alpha):
        z(101)
    ));
    out.push(act!(text:
        font("miso"):
        settext(mods_text):
        align(0.0, 0.0):
        xy(text_x, text_y):
        zoom(font_zoom):
        z(102):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));
}

#[cfg(test)]
mod tests {
    use super::build_modifiers_pane_with_text;
    use deadlib_present::actors::Actor;
    use std::sync::Arc;

    #[test]
    fn modifiers_pane_uses_supplied_mod_string() {
        let actors = build_modifiers_pane_with_text(
            Arc::<str>::from("M700, 40% Mini, Overhead, cel"),
            320.0,
            300.0,
            false,
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
