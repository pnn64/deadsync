use crate::act;
use crate::screens::gameplay::State;
use crate::step_stats_gifs::{
    GifRenderParams, ResolvedStepStatsExtra, gif_render_layout, resolve_extra,
};
use deadlib_present::actors::Actor;
use deadlib_present::space::{is_wide, screen_height, screen_width};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::PlayerSide;

const GIF_Z: i16 = 65;

pub fn resolve_random_extras(
    profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
) -> [ResolvedStepStatsExtra; MAX_PLAYERS] {
    std::array::from_fn(|player_idx| resolve_extra(&profiles[player_idx].step_stats_extra))
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

    let frame = layout.frame_at(
        state.current_beat(),
        state.gameplay.current_music_time_seconds(),
    );
    let [crop_left, crop_right, crop_top, crop_bottom] = layout.crop;
    if layout.crop != [0.0; 4] {
        actors.push(act!(sprite(layout.texture):
            align(layout.align_x, 0.5):
            xy(layout.x, layout.y):
            setstate(frame):
            zoom(layout.zoom):
            cropleft(crop_left):
            cropright(crop_right):
            croptop(crop_top):
            cropbottom(crop_bottom):
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
