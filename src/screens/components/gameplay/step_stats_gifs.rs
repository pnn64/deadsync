use crate::act;
use crate::screens::gameplay::State;
use deadlib_present::actors::Actor;
use deadlib_present::space::{is_wide, screen_height, screen_width};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayerSide, StepStatsExtra};

const CROP: f32 = 0.02;
const GIF_Z: i16 = 65;

const AMONG_US_FRAMES: [u16; 6] = [1, 2, 3, 4, 5, 0];
const AMONG_US_DELAYS: [f32; 6] = [0.125, 0.1875, 0.1875, 0.1875, 0.1875, 0.125];
const DONCHAN_FRAMES: [u16; 16] = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 3];
const DONCHAN_DELAYS: [f32; 16] = [0.5; 16];
const RIN_CAT_FRAMES: [u16; 6] = [2, 3, 4, 5, 0, 1];
const RIN_CAT_DELAYS: [f32; 6] = [
    0.333_333_333_333_333_3,
    0.333_333_333_333_333_3,
    0.333_333_333_333_333_4,
    0.333_333_333_333_333_3,
    0.333_333_333_333_333_3,
    0.333_333_333_333_333_4,
];

#[derive(Clone, Copy)]
struct GifLayout {
    texture: &'static str,
    zoom: f32,
    crop: bool,
}

pub fn resolve_random_extras(
    profiles: &[deadsync_profile::Profile; MAX_PLAYERS],
) -> [StepStatsExtra; MAX_PLAYERS] {
    std::array::from_fn(|player_idx| {
        let setting = profiles[player_idx].step_stats_extra;
        if setting != StepStatsExtra::Randomizer {
            return setting;
        }
        let choices = StepStatsExtra::RANDOMIZER_CHOICES;
        let choice_idx = (rand::random::<u64>() as usize) % choices.len();
        choices[choice_idx]
    })
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
    let Some(layout) = gif_layout(extra) else {
        return;
    };

    let wide = is_wide();
    let ar = screen_width() / screen_height().max(1.0);
    let side_sign = match player_side {
        PlayerSide::P1 => 1.0,
        PlayerSide::P2 => -1.0,
    };
    let mut local_x = if note_field_is_centered { -12.0 } else { -25.0 } * ar * side_sign;
    if wide && ar < 1.7 {
        local_x += 5.5;
    }

    let base_zoom = if wide && !note_field_is_centered {
        0.5
    } else {
        0.4
    };
    let actor_frame_zoom = base_zoom * banner_data_zoom;
    let mut x = pane_x + local_x * banner_data_zoom;
    let mut y = pane_y + -57.0 * banner_data_zoom;
    let mut zoom = actor_frame_zoom * layout.zoom;
    let mut align_x = 0.5;

    if extra == StepStatsExtra::BrodyQuest {
        let pn_sign = match player_side {
            PlayerSide::P1 => -1.0,
            PlayerSide::P2 => 1.0,
        };
        align_x = 0.5 + 0.5 * pn_sign;
        x += (if wide { 220.0 } else { 150.0 }) * pn_sign * actor_frame_zoom;
        y += -40.0 * actor_frame_zoom;
        zoom = actor_frame_zoom * (if wide { 1.3 } else { 0.3 });
    }

    let frame = frame_for_extra(extra, state.current_beat);
    if layout.crop {
        actors.push(act!(sprite(layout.texture):
            align(align_x, 0.5):
            xy(x, y):
            setstate(frame):
            zoom(zoom):
            cropleft(CROP):
            cropright(CROP):
            croptop(CROP):
            cropbottom(CROP):
            z(GIF_Z)
        ));
    } else {
        actors.push(act!(sprite(layout.texture):
            align(align_x, 0.5):
            xy(x, y):
            setstate(frame):
            zoom(zoom):
            z(GIF_Z)
        ));
    }
}

fn gif_layout(extra: StepStatsExtra) -> Option<GifLayout> {
    Some(match extra {
        StepStatsExtra::AmongUs => GifLayout {
            texture: "step_stats_gifs/AmongUs 3x2.png",
            zoom: 0.5,
            crop: true,
        },
        StepStatsExtra::BrodyQuest => GifLayout {
            texture: "step_stats_gifs/brodyquest 7x12.gif",
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::CatJAM => GifLayout {
            texture: "step_stats_gifs/catjam 11x14.png",
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::CrabPls => GifLayout {
            texture: "step_stats_gifs/CrabPls 8x8.png",
            zoom: 1.5,
            crop: true,
        },
        StepStatsExtra::DancingDuck => GifLayout {
            texture: "step_stats_gifs/Dancing Duck 8x14.png",
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::DonChan => GifLayout {
            texture: "step_stats_gifs/DonChan 2x2.png",
            zoom: 1.0,
            crop: true,
        },
        StepStatsExtra::NyanCat => GifLayout {
            texture: "step_stats_gifs/NyanCat 4x3.png",
            zoom: 1.4,
            crop: true,
        },
        StepStatsExtra::RinCat => GifLayout {
            texture: "step_stats_gifs/Rin Cat 2x3.png",
            zoom: 0.6,
            crop: false,
        },
        StepStatsExtra::Snoop => GifLayout {
            texture: "step_stats_gifs/snoop 8x8.png",
            zoom: 0.75,
            crop: true,
        },
        StepStatsExtra::Sonic => GifLayout {
            texture: "step_stats_gifs/Sonic 4x2.png",
            zoom: 2.0,
            crop: true,
        },
        StepStatsExtra::None | StepStatsExtra::ErrorStats | StepStatsExtra::Randomizer => {
            return None;
        }
    })
}

fn frame_for_extra(extra: StepStatsExtra, beat: f32) -> u32 {
    match extra {
        StepStatsExtra::AmongUs => mixed_frame(beat, &AMONG_US_FRAMES, &AMONG_US_DELAYS),
        StepStatsExtra::BrodyQuest => uniform_rotated_frame(beat, 84, 0, 0.095_238_095_238_095_2),
        StepStatsExtra::CatJAM => uniform_rotated_frame(beat, 151, 9, 0.086_092_715_231_788_1),
        StepStatsExtra::CrabPls => uniform_rotated_frame(beat, 59, 5, 0.067_796_610_169_491_5),
        StepStatsExtra::DancingDuck => uniform_rotated_frame(beat, 111, 1, 0.144_144_144),
        StepStatsExtra::DonChan => mixed_frame(beat, &DONCHAN_FRAMES, &DONCHAN_DELAYS),
        StepStatsExtra::NyanCat => uniform_rotated_frame(beat, 12, 6, 0.166_666_67),
        StepStatsExtra::RinCat => mixed_frame(beat, &RIN_CAT_FRAMES, &RIN_CAT_DELAYS),
        StepStatsExtra::Snoop => uniform_rotated_frame(beat, 58, 3, 0.068_965_517_24),
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
    fn randomizer_choices_are_renderable_gifs_only() {
        assert_eq!(StepStatsExtra::RANDOMIZER_CHOICES.len(), 10);
        for choice in StepStatsExtra::RANDOMIZER_CHOICES {
            assert!(choice.renderable());
        }
    }
}
