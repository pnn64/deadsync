use deadlib_present::actors::Actor;
use deadsync_theme::views::{GameplayHudMotion, GameplayLifeView};

use crate::presentation::Layout;

// The supplied tutorial chart, transcribed into native scene data. The first
// four rows are played, and the final six rows demonstrate misses.
const STEPS: [(f32, u8); 10] = [
    (16.0, 2),
    (18.0, 4),
    (20.0, 1),
    (22.0, 9),
    (24.0, 1),
    (24.125, 2),
    (24.25, 8),
    (24.375, 2),
    (24.5, 4),
    (24.625, 1),
];

pub(crate) fn beat_at(elapsed: f32) -> f32 {
    let mut stopped = 0.0;
    for beat in [16.0, 18.0, 20.0, 22.0] {
        let start = beat * 0.6 + stopped;
        if elapsed < start {
            return (elapsed - stopped) / 0.6;
        }
        if elapsed < start + 2.0 {
            return beat;
        }
        stopped += 2.0;
    }
    (elapsed - stopped) / 0.6
}

fn time_at(beat: f32) -> f32 {
    beat * 0.6
        + [16.0, 18.0, 20.0, 22.0]
            .into_iter()
            .filter(|stop| *stop < beat)
            .count() as f32
            * 2.0
}

pub(crate) fn push_tutorial(actors: &mut Vec<Actor>, elapsed: f32) {
    let layout = Layout::current();
    let beat = beat_at(elapsed);
    let phase = (-50.0 * elapsed).rem_euclid(128.0);
    for row in -1..5 {
        for col in -1..6 {
            actors.push(
                layout
                    .sprite(
                        "stepmania-395/training-background",
                        [col as f32 * 128.0 + phase, row as f32 * 128.0 + phase],
                        [128.0; 2],
                        -100,
                    )
                    .build(0),
            );
        }
    }
    for (column, rotation) in [-90.0, 0.0, 180.0, 90.0].into_iter().enumerate() {
        let x = 384.0 + column as f32 * 64.0;
        let fraction = beat.rem_euclid(1.0);
        let frame = if !(0.1..0.9).contains(&fraction) {
            0.0
        } else {
            0.5
        };
        let mut receptor =
            layout.sprite("stepmania-395/training-receptor", [x, 96.0], [64.0; 2], 1);
        receptor.customtexturerect([frame, 0.0, frame + 0.5, 1.0]);
        receptor.rotationz(rotation);
        actors.push(receptor.build(0));
        for (index, &(step, mask)) in STEPS.iter().enumerate() {
            if mask & (1 << column) == 0 {
                continue;
            }
            let hit_time = time_at(step);
            if elapsed >= hit_time + if index < 4 { 0.0 } else { 0.18 } {
                continue;
            }
            let y = 96.0 + (step - beat) * 64.0;
            if !(-32.0..=512.0).contains(&y) {
                continue;
            }
            let vivid = ((step.fract() - 0.0625) * 4.0).round() * 0.25;
            let frame = ((beat * 0.25 + vivid).rem_euclid(1.0) * 16.0).floor() as usize;
            let (u, v) = ((frame % 4) as f32 * 0.25, (frame / 4) as f32 * 0.25);
            let mut note = layout.sprite("stepmania-395/training-note", [x, y], [64.0; 2], 2);
            note.customtexturerect([u, v, u + 0.25, v + 0.25]);
            note.rotationz(rotation);
            actors.push(note.build(0));
        }
    }

    let perfects = STEPS[..4]
        .iter()
        .filter(|(step, _)| elapsed >= time_at(*step))
        .count();
    let misses = STEPS[4..]
        .iter()
        .filter(|(step, _)| elapsed >= time_at(*step) + 0.18)
        .count();
    let life = (0.448 + perfects as f32 * 0.008 - misses as f32 * 0.08).clamp(0.0, 1.0);
    crate::gameplay::push_life_at(
        actors,
        GameplayLifeView {
            p2: false,
            level: life,
            failed: false,
            hot: false,
            song_beat: beat,
            percent_label: None,
            motion: GameplayHudMotion::Settled,
        },
        [
            480.0,
            40.0 - 60.0 * (1.0 - ((elapsed - 2.4) / 0.2).clamp(0.0, 1.0)),
        ],
    );

    let travel = ((elapsed - 2.1) / 0.3).clamp(0.0, 1.0);
    let mut title = layout.sprite(
        "stepmania-395/training-howtoplay",
        [320.0 - 150.0 * travel, 240.0 - 180.0 * travel],
        [480.0, 96.0],
        110,
    );
    title.zoom(if elapsed < 0.3 {
        4.0 - elapsed * 10.0
    } else {
        1.0 - 0.5 * travel
    });
    title.alpha((elapsed / 0.3).clamp(0.0, 1.0));
    actors.push(title.build(0));
    if elapsed < 5.0 {
        let mut feet = layout.sprite(
            "stepmania-395/training-feet",
            [
                320.0 - 640.0 * (1.0 - ((elapsed - 2.4) / 0.3).clamp(0.0, 1.0)).powi(2),
                240.0,
            ],
            [480.0, 64.0],
            110,
        );
        feet.zoomy(1.0 - ((elapsed - 4.7) / 0.3).clamp(0.0, 1.0));
        actors.push(feet.build(0));
    }
    for (start, at, jump, sideways) in [
        (6.0, [290.0, 110.0], false, true),
        (9.7, [448.0, 180.0], false, false),
        (12.7, [512.0, 180.0], false, false),
        (15.7, [384.0, 180.0], false, false),
        (18.7, [480.0, 180.0], true, false),
        (22.7, [548.0, 110.0], false, false),
    ] {
        let duration = if start == 6.0 { 2.0 } else { 1.7 };
        if !(start..start + duration).contains(&elapsed) {
            continue;
        }
        let bob = 20.0 * (elapsed * std::f32::consts::TAU).sin();
        let mut hand = layout.sprite(
            if jump {
                "stepmania-395/training-jumphands"
            } else {
                "stepmania-395/training-taphand"
            },
            [
                at[0] + if sideways { bob } else { 0.0 },
                at[1] + if sideways { 0.0 } else { bob },
            ],
            if jump { [210.0, 86.0] } else { [64.0, 96.0] },
            110,
        );
        if sideways {
            hand.rotationz(-270.0);
            let mut hand = hand.build(0);
            if let Actor::Sprite { flip_x, .. } = &mut hand {
                *flip_x = true;
            }
            actors.push(hand);
        } else {
            actors.push(hand.build(0));
        }
        let key = if start == 22.7 {
            "stepmania-395/training-missmessage"
        } else if jump {
            "stepmania-395/training-jumpmessage"
        } else {
            "stepmania-395/training-tapmessage"
        };
        let size = if start == 22.7 {
            [322.0, 120.0]
        } else if jump {
            [270.0, 116.0]
        } else {
            [250.0, 120.0]
        };
        actors.push(layout.sprite(key, [480.0, 280.0], size, 110).build(0));
    }
    if (24.4..25.0).contains(&elapsed) {
        actors.push(
            layout
                .sprite(
                    "stepmania-395/training-missmessage",
                    [480.0, 280.0],
                    [322.0, 120.0],
                    110,
                )
                .build(0),
        );
    }
    crate::training_pad::push_pad(actors, elapsed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_stops_keep_arrows_at_the_authored_beats() {
        for (beat, start) in [(16.0, 9.6), (18.0, 12.8), (20.0, 16.0), (22.0, 19.2)] {
            assert!((time_at(beat) - start).abs() < 0.0001);
            for elapsed in [start, start + 1.0, start + 1.999] {
                assert!((beat_at(elapsed) - beat).abs() < 0.0001);
            }
            assert!(beat_at(start + 2.1) > beat);
        }
        assert!((time_at(24.0) - 22.4).abs() < 0.0001);
    }
}
