use crate::act;
use crate::screens::gameplay::State;
use deadlib_present::actors::Actor;
use deadlib_present::space::{is_wide, screen_height, screen_width};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayerSide, StepStatsExtra};
use deadsync_theme::step_stats_gifs::{
    GifRenderParams, frame_for_extra, gif_render_layout, resolve_random_extra,
};

const CROP: f32 = 0.02;
const GIF_Z: i16 = 65;

pub fn resolve_random_extras(
    profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
) -> [StepStatsExtra; MAX_PLAYERS] {
    std::array::from_fn(|player_idx| resolve_random_extra(profiles[player_idx].step_stats_extra))
}

pub fn push_step_stats_extra(
    actors: &mut Vec<Actor>,
    state: &State,
    player_side: PlayerSide,
    player_idx: usize,
    pane_x: f32,
    pane_y: f32,
    banner_data_zoom: f32,
    note_field_is_centered: bool,
) {
    let Some(extra) = state.step_stats_extra_resolved.get(player_idx).copied() else {
        return;
    };
    let Some(layout) = gif_render_layout(
        extra,
        GifRenderParams {
            player_side,
            wide: is_wide(),
            aspect_ratio: screen_width() / screen_height().max(1.0),
            pane_x,
            pane_y,
            banner_data_zoom,
            note_field_is_centered,
        },
    ) else {
        return;
    };

    let frame = frame_for_extra(extra, state.current_beat());
    if layout.crop {
        actors.push(act!(sprite(layout.texture):
            align(layout.align_x, 0.5):
            xy(layout.x, layout.y):
            setstate(frame):
            zoom(layout.zoom):
            cropleft(CROP):
            cropright(CROP):
            croptop(CROP):
            cropbottom(CROP):
            z(GIF_Z)
        ));
    } else {
        actors.push(act!(sprite(layout.texture):
            align(layout.align_x, 0.5):
            xy(layout.x, layout.y):
            setstate(frame):
            zoom(layout.zoom):
            z(GIF_Z)
        ));
    }
}
