use deadsync_profile::StepStatsExtra;

const AMONG_US_FRAMES: [u16; 6] = [1, 2, 3, 4, 5, 0];
const AMONG_US_DELAYS: [f32; 6] = [0.125, 0.1875, 0.1875, 0.1875, 0.1875, 0.125];
const DONCHAN_FRAMES: [u16; 16] = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 3];
const DONCHAN_DELAYS: [f32; 16] = [0.5; 16];
const BOCCHI_FRAMES: u32 = 18;
const BOCCHI_DELAY: f32 = 1.0 / 9.0;
const RIN_CAT_FRAMES: [u16; 6] = [2, 3, 4, 5, 0, 1];
const RIN_CAT_DELAYS: [f32; 6] = [
    0.333_333_34,
    0.333_333_34,
    0.333_333_34,
    0.333_333_34,
    0.333_333_34,
    0.333_333_34,
];

pub const STEP_STATS_GIF_TEXTURES: [&str; 11] = [
    "step_stats_gifs/AmongUs 3x2.png",
    "step_stats_gifs/Bocchi 6x3.png",
    "step_stats_gifs/brodyquest 7x12.gif",
    "step_stats_gifs/catjam 11x14.png",
    "step_stats_gifs/CrabPls 8x8.png",
    "step_stats_gifs/Dancing Duck 8x14.png",
    "step_stats_gifs/DonChan 2x2.png",
    "step_stats_gifs/NyanCat 4x3.png",
    "step_stats_gifs/Rin Cat 2x3.png",
    "step_stats_gifs/snoop 8x8.png",
    "step_stats_gifs/Sonic 4x2.png",
];

#[derive(Clone, Copy)]
pub struct GifLayout {
    pub texture: &'static str,
    pub zoom: f32,
    pub crop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GifRenderParams {
    pub player_side: deadsync_profile::PlayerSide,
    pub wide: bool,
    pub aspect_ratio: f32,
    pub pane_x: f32,
    pub pane_y: f32,
    pub banner_data_zoom: f32,
    pub note_field_is_centered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GifRenderLayout {
    pub texture: &'static str,
    pub crop: bool,
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub align_x: f32,
}

pub fn resolve_random_extra(setting: StepStatsExtra) -> StepStatsExtra {
    if setting != StepStatsExtra::Randomizer {
        return setting;
    }
    let choices = StepStatsExtra::RANDOMIZER_CHOICES;
    let choice_idx = (rand::random::<u64>() as usize) % choices.len();
    choices[choice_idx]
}

pub fn gif_layout(extra: StepStatsExtra) -> Option<GifLayout> {
    Some(match extra {
        StepStatsExtra::AmongUs => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[0],
            zoom: 0.5,
            crop: true,
        },
        StepStatsExtra::Bocchi => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[1],
            zoom: 2.0,
            crop: true,
        },
        StepStatsExtra::BrodyQuest => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[2],
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::CatJAM => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[3],
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::CrabPls => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[4],
            zoom: 1.5,
            crop: true,
        },
        StepStatsExtra::DancingDuck => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[5],
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::DonChan => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[6],
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::NyanCat => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[7],
            zoom: 1.4,
            crop: true,
        },
        StepStatsExtra::RinCat => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[8],
            zoom: 0.6,
            crop: false,
        },
        StepStatsExtra::Snoop => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[9],
            zoom: 0.75,
            crop: true,
        },
        StepStatsExtra::Sonic => GifLayout {
            texture: STEP_STATS_GIF_TEXTURES[10],
            zoom: 2.0,
            crop: true,
        },
        StepStatsExtra::None | StepStatsExtra::ErrorStats | StepStatsExtra::Randomizer => {
            return None;
        }
    })
}

pub fn gif_render_layout(
    extra: StepStatsExtra,
    params: GifRenderParams,
) -> Option<GifRenderLayout> {
    let layout = gif_layout(extra)?;
    let side_sign = match params.player_side {
        deadsync_profile::PlayerSide::P1 => 1.0,
        deadsync_profile::PlayerSide::P2 => -1.0,
    };
    let mut local_x = if params.note_field_is_centered {
        -12.0
    } else {
        -25.0
    } * params.aspect_ratio
        * side_sign;
    if params.wide && params.aspect_ratio < 1.7 {
        local_x += 5.5;
    }

    let base_zoom = if params.wide && !params.note_field_is_centered {
        0.5
    } else {
        0.4
    };
    let actor_frame_zoom = base_zoom * params.banner_data_zoom;
    let mut x = params.pane_x + local_x * params.banner_data_zoom;
    let mut y = params.pane_y + -57.0 * params.banner_data_zoom;
    let mut zoom = actor_frame_zoom * layout.zoom;
    let mut align_x = 0.5;

    if extra == StepStatsExtra::BrodyQuest {
        let pn_sign = match params.player_side {
            deadsync_profile::PlayerSide::P1 => -1.0,
            deadsync_profile::PlayerSide::P2 => 1.0,
        };
        align_x = 0.5 + 0.5 * pn_sign;
        x += if params.wide { 220.0 } else { 150.0 } * pn_sign * actor_frame_zoom;
        y += -40.0 * actor_frame_zoom;
        zoom = actor_frame_zoom * if params.wide { 1.3 } else { 0.3 };
    }

    Some(GifRenderLayout {
        texture: layout.texture,
        crop: layout.crop,
        x,
        y,
        zoom,
        align_x,
    })
}

pub fn frame_for_extra(extra: StepStatsExtra, beat: f32) -> u32 {
    match extra {
        StepStatsExtra::AmongUs => mixed_frame(beat, &AMONG_US_FRAMES, &AMONG_US_DELAYS),
        StepStatsExtra::Bocchi => uniform_rotated_frame(beat, BOCCHI_FRAMES, 0, BOCCHI_DELAY),
        StepStatsExtra::BrodyQuest => uniform_rotated_frame(beat, 84, 0, 0.095_238_1),
        StepStatsExtra::CatJAM => uniform_rotated_frame(beat, 151, 9, 0.086_092_72),
        StepStatsExtra::CrabPls => uniform_rotated_frame(beat, 59, 5, 0.067_796_61),
        StepStatsExtra::DancingDuck => uniform_rotated_frame(beat, 111, 1, 0.144_144_15),
        StepStatsExtra::DonChan => mixed_frame(beat, &DONCHAN_FRAMES, &DONCHAN_DELAYS),
        StepStatsExtra::NyanCat => uniform_rotated_frame(beat, 12, 6, 0.166_666_67),
        StepStatsExtra::RinCat => mixed_frame(beat, &RIN_CAT_FRAMES, &RIN_CAT_DELAYS),
        StepStatsExtra::Snoop => uniform_rotated_frame(beat, 58, 3, 0.068_965_52),
        StepStatsExtra::Sonic => uniform_rotated_frame(beat, 8, 0, 0.125),
        StepStatsExtra::None | StepStatsExtra::ErrorStats | StepStatsExtra::Randomizer => 0,
    }
}

fn uniform_rotated_frame(beat: f32, count: u32, first: u32, delay: f32) -> u32 {
    let total = delay * count as f32;
    let phase = beat_phase(beat, total);
    let frame_idx = (phase / delay)
        .floor()
        .clamp(0.0, count.saturating_sub(1) as f32) as u32;
    (first + frame_idx) % count
}

fn mixed_frame(beat: f32, frames: &[u16], delays: &[f32]) -> u32 {
    if frames.is_empty() || frames.len() != delays.len() {
        return 0;
    }
    let total = delays.iter().copied().sum::<f32>();
    let mut phase = beat_phase(beat, total);
    for (&frame, &delay) in frames.iter().zip(delays.iter()) {
        if phase < delay {
            return frame as u32;
        }
        phase -= delay;
    }
    frames[0] as u32
}

fn beat_phase(beat: f32, total: f32) -> f32 {
    if beat.is_finite() && total.is_finite() && total > 0.0 {
        beat.rem_euclid(total)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catjam_wraps_from_frame_nine_on_bgm_beat() {
        const CATJAM_DELAY: f32 = 0.086_092_715_231_788_1;
        assert_eq!(frame_for_extra(StepStatsExtra::CatJAM, 0.0), 9);
        assert_eq!(frame_for_extra(StepStatsExtra::CatJAM, CATJAM_DELAY), 10);
        assert_eq!(
            frame_for_extra(StepStatsExtra::CatJAM, 150.0 * CATJAM_DELAY),
            8
        );
        assert_eq!(
            frame_for_extra(StepStatsExtra::CatJAM, 151.0 * CATJAM_DELAY),
            9
        );
    }

    #[test]
    fn among_us_respects_mixed_delays() {
        assert_eq!(frame_for_extra(StepStatsExtra::AmongUs, 0.0), 1);
        assert_eq!(frame_for_extra(StepStatsExtra::AmongUs, 0.124), 1);
        assert_eq!(frame_for_extra(StepStatsExtra::AmongUs, 0.125), 2);
        assert_eq!(frame_for_extra(StepStatsExtra::AmongUs, 0.3125), 3);
        assert_eq!(frame_for_extra(StepStatsExtra::AmongUs, 0.875), 0);
        assert_eq!(frame_for_extra(StepStatsExtra::AmongUs, 1.0), 1);
    }

    #[test]
    fn bocchi_loops_eighteen_frames_over_two_bgm_beats() {
        assert_eq!(frame_for_extra(StepStatsExtra::Bocchi, 0.0), 0);
        assert_eq!(
            frame_for_extra(StepStatsExtra::Bocchi, BOCCHI_DELAY * 0.99),
            0
        );
        assert_eq!(frame_for_extra(StepStatsExtra::Bocchi, BOCCHI_DELAY), 1);
        assert_eq!(
            frame_for_extra(StepStatsExtra::Bocchi, 17.0 * BOCCHI_DELAY),
            17
        );
        assert_eq!(frame_for_extra(StepStatsExtra::Bocchi, 2.0), 0);
    }

    #[test]
    fn randomizer_choices_are_renderable_gifs_only() {
        assert_eq!(StepStatsExtra::RANDOMIZER_CHOICES.len(), 11);
        for choice in StepStatsExtra::RANDOMIZER_CHOICES {
            assert!(choice.renderable());
            assert!(gif_layout(choice).is_some());
        }
    }

    #[test]
    fn texture_list_matches_layouts() {
        for extra in StepStatsExtra::RANDOMIZER_CHOICES {
            let layout = gif_layout(extra).expect("renderable gif layout");
            assert!(STEP_STATS_GIF_TEXTURES.contains(&layout.texture));
        }
    }

    #[test]
    fn render_layout_places_p1_gif_near_step_stats_pane() {
        let layout = gif_render_layout(
            StepStatsExtra::CatJAM,
            GifRenderParams {
                player_side: deadsync_profile::PlayerSide::P1,
                wide: true,
                aspect_ratio: 16.0 / 9.0,
                pane_x: 100.0,
                pane_y: 200.0,
                banner_data_zoom: 2.0,
                note_field_is_centered: false,
            },
        )
        .unwrap();

        assert_eq!(layout.texture, "step_stats_gifs/catjam 11x14.png");
        assert!(layout.crop);
        assert_eq!(layout.align_x, 0.5);
        assert_eq!(layout.x, 100.0 - 25.0 * (16.0 / 9.0) * 2.0);
        assert_eq!(layout.y, 86.0);
        assert_eq!(layout.zoom, 1.0);
    }

    #[test]
    fn render_layout_offsets_brodyquest_by_player_side() {
        let p1 = gif_render_layout(
            StepStatsExtra::BrodyQuest,
            GifRenderParams {
                player_side: deadsync_profile::PlayerSide::P1,
                wide: true,
                aspect_ratio: 16.0 / 9.0,
                pane_x: 0.0,
                pane_y: 0.0,
                banner_data_zoom: 1.0,
                note_field_is_centered: false,
            },
        )
        .unwrap();
        let p2 = gif_render_layout(
            StepStatsExtra::BrodyQuest,
            GifRenderParams {
                player_side: deadsync_profile::PlayerSide::P2,
                wide: true,
                aspect_ratio: 16.0 / 9.0,
                pane_x: 0.0,
                pane_y: 0.0,
                banner_data_zoom: 1.0,
                note_field_is_centered: false,
            },
        )
        .unwrap();

        assert_eq!(p1.align_x, 0.0);
        assert_eq!(p2.align_x, 1.0);
        assert!(p1.x < 0.0);
        assert!(p2.x > 0.0);
        assert_eq!(p1.y, p2.y);
        assert_eq!(p1.zoom, p2.zoom);
    }
}
