pub mod lobby_overlay;
pub mod music_wheel;
pub mod screen_bars;
pub mod select_music_menu;
pub mod select_pane;
pub mod step_artist_bar;

use deadlib_present::actors::{Actor, SizeSpec};
use std::sync::Arc;

/// Appends one immutable overlay tree while keeping its children shared.
pub(crate) fn push_retained_overlay(actors: &mut Vec<Actor>, children: Arc<[Actor]>) {
    actors.reserve(1);
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}
