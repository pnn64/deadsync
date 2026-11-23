use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::render::BlendMode;
use deadlib_present::space::{screen_height, screen_width};

use crate::OpenItgDemonstration;
use crate::presentation::{Layout, WIDTH, quad, sprite};
use crate::resources::TUTORIAL_FONT;

const INSTRUCTIONS: [&str; 6] = [
    "Color Arrows\nscroll from\nlow to high.",
    "Step when a\nColor Arrow\noverlaps the\nTarget Arrows\nat the top.",
    "The\nTraffic Light\nhelps you\nunderstand\nthe timing.",
    "The direction\nof the arrow\nsays which\nPanel\nto step on.",
    "For arrows\nfacing Left,\nstep on the\nLeft Panel.",
    "For arrows\nfacing Up,\nstep on the\nUp Panel.",
];

pub(crate) fn push_actors(
    actors: &mut Vec<Actor>,
    demonstration: OpenItgDemonstration,
    elapsed: f32,
    music_time: f32,
    training_lights: [f32; 4],
) {
    let [width, height] = [screen_width(), screen_height()];
    let center = [width * 0.5, height * 0.5];
    let layout = Layout::centered(width, height);
    actors.push(
        sprite(
            "openitg/demo-gradient.png",
            center,
            [width, height],
            [1.0, 1.0, 1.0, 0.8],
            120,
        )
        .build(0),
    );
    if demonstration == OpenItgDemonstration::Autoplay {
        let scale = 0.95 + 0.05 * (music_time * std::f32::consts::TAU).cos();
        actors.push(
            sprite(
                "openitg/demo-logo.png",
                layout.position(320.0, 60.0),
                layout.size([128.0 * scale, 54.0 * scale]),
                [1.0; 4],
                121,
            )
            .build(0),
        );
        return;
    }

    let quarter_width = width / layout.size([4.0, 0.0])[0];
    let field_x = WIDTH * 0.5 - quarter_width;
    let instruction_x = WIDTH * 0.5 + quarter_width + 10.0;
    actors.push(
        sprite(
            "openitg/demo-light-frame.png",
            layout.position(320.0, 210.0),
            layout.size([76.0, 268.0]),
            [1.0; 4],
            121,
        )
        .build(0),
    );
    for (index, (key, alpha)) in [
        "openitg/demo-light-green.png",
        "openitg/demo-light-yellow.png",
        "openitg/demo-light-yellow.png",
        "openitg/demo-light-red.png",
    ]
    .into_iter()
    .zip(training_lights)
    .enumerate()
    {
        if alpha > 0.0 {
            let size = 64.0 * (1.0 + 0.4 * alpha);
            let mut light = sprite(
                key,
                layout.position(320.0, 114.0 + 64.0 * index as f32),
                layout.size([size; 2]),
                [1.0, 1.0, 1.0, alpha],
                122,
            );
            light.rotationz(45.0 * index as f32);
            light.blend(BlendMode::Add);
            actors.push(light.build(0));
        }
    }

    for (index, instruction) in INSTRUCTIONS.into_iter().enumerate() {
        let time = elapsed - (5.0 + 6.0 * index as f32);
        if !(0.0..5.8).contains(&time) {
            continue;
        }
        let slide = 1.0 - (time / 0.5).clamp(0.0, 1.0);
        let mut text = layout.text(
            [instruction_x + 2.0 * quarter_width * slide * slide, 240.0],
            0.5,
            instruction.into(),
            0.8,
            [1.0, 1.0, 1.0, ((5.8 - time) / 0.3).clamp(0.0, 1.0)],
        );
        text.font(TUTORIAL_FONT);
        text.horizalign(TextAlign::Center);
        text.z(124);
        actors.push(text.build(0));

        let blink = if time.rem_euclid(0.5) < 0.25 {
            1.0
        } else {
            0.0
        };
        let alpha =
            ((time - 1.0) / 0.3).clamp(0.0, 1.0) * ((5.6 - time) / 0.3).clamp(0.0, 1.0) * blink;
        let (key, position, size, rotation) = match index {
            0 => (
                "openitg/demo-focus-square.png",
                [field_x, 300.0],
                [294.0, 308.7],
                0.0,
            ),
            1 => (
                "openitg/demo-focus-rect.png",
                [field_x, 116.0],
                [328.0, 91.2],
                0.0,
            ),
            2 => (
                "openitg/demo-focus-rect.png",
                [320.0, 210.0],
                [328.0, 114.0],
                90.0,
            ),
            4 | 5 => (
                "openitg/demo-arrow.png",
                [instruction_x - 70.0, 320.0],
                [58.0, 60.0],
                if index == 4 { 0.0 } else { 90.0 },
            ),
            _ => continue,
        };
        if alpha > 0.0 {
            let mut focus = sprite(
                key,
                layout.position(position[0], position[1]),
                layout.size(size),
                [1.0, 1.0, 1.0, alpha],
                123,
            );
            focus.rotationz(rotation);
            actors.push(focus.build(0));
        }
    }

    let black_alpha = ((4.0 - elapsed) / 0.5).clamp(0.0, 1.0);
    if black_alpha > 0.0 {
        actors.push(quad(
            center,
            [width, height],
            [0.0, 0.0, 0.0, black_alpha],
            125,
        ));
    }
    let title_move = 1.0 - (1.0 - ((elapsed - 3.0) / 0.5).clamp(0.0, 1.0)).powi(2);
    let title_zoom = 1.0 - 0.3 * title_move;
    let mut title = sprite(
        "openitg/demo-instructions.png",
        layout.position(320.0, 240.0 - 200.0 * title_move),
        layout.size([536.0 * title_zoom, 84.0 * title_zoom]),
        [1.0; 4],
        126,
    );
    let scan = elapsed.clamp(0.0, 1.0) * 1.6;
    title.cropright((1.3 - scan).max(0.0));
    actors.push(title.build(0));
    if elapsed < 1.0 {
        let mut shine = sprite(
            "openitg/demo-instructions-white.png",
            center,
            layout.size([534.0 * 1.05, 82.0 * 1.05]),
            [1.0; 4],
            127,
        );
        shine.cropleft((scan - 0.3).max(0.0));
        shine.cropright((1.0 - scan).max(0.0));
        actors.push(shine.build(0));
    }
}
