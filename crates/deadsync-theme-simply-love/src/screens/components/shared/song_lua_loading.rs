use super::loading_bar::{self, LoadingBarParams};
use crate::{act, color, i18n::tr};
use deadlib_present::{
    actors::Actor,
    space::{screen_center_x, screen_center_y, screen_width, widescale},
};
use std::sync::Arc;

/// Match startup's loading bar while a song's finite Lua playback is prepared.
pub fn push(actors: &mut Vec<Actor>, title: Arc<str>, fraction: f32, active_color_index: i32) {
    let cx = screen_center_x();
    let cy = screen_center_y() + 34.0;
    let fill = color::decorative_rgba(active_color_index);
    actors.push(act!(text:
        font("miso"):
        settext(tr("SongLuaLoading", "Baking")):
        align(0.5, 0.5): xy(cx, cy - 72.0):
        zoom(1.05): horizalign(center): z(110.0)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(title):
        align(0.5, 0.5): xy(cx, cy - 42.0):
        zoom(0.95): maxwidth(screen_width() * 0.9): horizalign(center): z(110.0)
    ));
    actors.push(loading_bar::build(LoadingBarParams {
        align: [0.5, 0.5],
        offset: [cx, cy],
        width: widescale(360.0, 520.0),
        height: 30.0,
        progress: fraction,
        label: format!("{:.0}%", fraction.clamp(0.0, 1.0) * 100.0).into(),
        fill_rgba: [fill[0], fill[1], fill[2], 1.0],
        bg_rgba: [0.0, 0.0, 0.0, 1.0],
        border_rgba: [1.0, 1.0, 1.0, 1.0],
        text_rgba: [1.0, 1.0, 1.0, 1.0],
        text_zoom: 0.9,
        z: 110,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;

    #[test]
    fn bake_progress_bar_uses_startup_dimensions_color_and_proportional_fill() {
        for (fraction, expected) in [(0.0, 0.0), (0.5, 0.5), (1.5, 1.0)] {
            let mut actors = Vec::new();
            push(&mut actors, Arc::from("A long song title"), fraction, 5);
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Text { content, .. } if content.as_str() == "A long song title")));
            let Actor::Frame {
                size: [SizeSpec::Px(width), SizeSpec::Px(height)],
                children,
                ..
            } = &actors[2]
            else {
                panic!("startup-style loading bar");
            };
            assert_eq!(*height, 30.0);
            let text = children.last().unwrap();
            assert!(matches!(text, Actor::Text { content, .. }
                if content.as_str() == format!("{:.0}%", expected * 100.0)));
            if expected == 0.0 {
                assert_eq!(children.len(), 3);
            } else {
                let Actor::Sprite {
                    scale: [fill_width, _],
                    tint,
                    ..
                } = &children[2]
                else {
                    panic!("filled portion of loading bar");
                };
                assert_eq!(*fill_width, (width - 4.0) * expected);
                assert_eq!(*tint, color::decorative_rgba(5));
            }
        }
    }
}
