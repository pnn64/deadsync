use crate::act;
use crate::screens::gameplay::State;
use crate::step_stats_gifs::{
    GifRenderParams, ResolvedStepStatsExtra, gif_render_layout, resolve_extra,
};
use deadlib_present::actors::Actor;
use deadsync_core::input::MAX_PLAYERS;

const GIF_Z: i16 = 65;

pub fn resolve_random_extras(
    profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
) -> [ResolvedStepStatsExtra; MAX_PLAYERS] {
    std::array::from_fn(|player_idx| resolve_extra(&profiles[player_idx].step_stats_extra))
}

pub fn push_step_stats_extra(
    actors: &mut Vec<Actor>,
    state: &State,
    player_idx: usize,
    params: GifRenderParams,
) {
    let Some(extra) = state.step_stats_extra_resolved.get(player_idx).copied() else {
        return;
    };
    let Some(layout) = gif_render_layout(extra, params) else {
        return;
    };

    let frame = layout.frame_at(
        state.current_beat(),
        state.gameplay.current_music_time_seconds(),
    );
    actors.push(build_gif_actor(layout, frame));
}

fn build_gif_actor(layout: crate::step_stats_gifs::GifRenderLayout, frame: u32) -> Actor {
    let [crop_left, crop_right, crop_top, crop_bottom] = layout.crop;
    if layout.crop != [0.0; 4] {
        act!(sprite_static(layout.texture):
            align(layout.align_x, 0.5):
            xy(layout.x, layout.y):
            setstate(frame):
            zoom(layout.zoom):
            cropleft(crop_left):
            cropright(crop_right):
            croptop(crop_top):
            cropbottom(crop_bottom):
            z(GIF_Z)
        )
    } else {
        act!(sprite_static(layout.texture):
            align(layout.align_x, 0.5):
            xy(layout.x, layout.y):
            setstate(frame):
            zoom(layout.zoom):
            z(GIF_Z)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step_stats_gifs::{GifRenderParams, gif_render_layout};
    use deadlib_present::actors::SpriteSource;
    use deadsync_profile::{PlayerSide, StepStatsExtra};

    fn assert_actor(
        actor: Actor,
        layout: crate::step_stats_gifs::GifRenderLayout,
        frame: u32,
        static_source: bool,
    ) {
        let Actor::Sprite {
            align,
            offset,
            source,
            z,
            cell,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            scale,
            ..
        } = actor
        else {
            panic!("Step Stats GIF should build a sprite");
        };
        assert_eq!(align, [layout.align_x, 0.5]);
        assert_eq!(offset, [layout.x, layout.y]);
        assert_eq!(z, GIF_Z);
        assert_eq!(cell, Some((frame, u32::MAX)));
        assert_eq!([cropleft, cropright, croptop, cropbottom], layout.crop);
        assert_eq!(scale, [layout.zoom; 2]);
        assert_eq!(source.texture_key(), Some(layout.texture));
        assert_eq!(
            matches!(source, SpriteSource::TextureStatic(_)),
            static_source
        );
    }

    #[test]
    fn static_actor_uses_layout_without_owned_texture_key() {
        let extra = resolve_extra(&StepStatsExtra::gif("AmongUs"));
        let layout = gif_render_layout(
            extra,
            GifRenderParams {
                player_side: PlayerSide::P1,
                wide: true,
                aspect_ratio: 16.0 / 9.0,
                pane_x: 100.0,
                pane_y: 200.0,
                banner_data_zoom: 0.8,
                note_field_is_centered: false,
            },
        )
        .expect("AmongUs should have a render layout");
        let frame = layout.frame_at(12.25, 4.75);

        assert_actor(build_gif_actor(layout, frame), layout, frame, true);
    }
}
