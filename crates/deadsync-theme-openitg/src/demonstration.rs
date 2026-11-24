pub(crate) mod training;

use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::render::BlendMode;
use deadlib_present::space::{current_metrics, menu_perspective, screen_height, screen_width};
use glam::{Mat4, Vec3};

use crate::OpenItgDemonstration;
use crate::presentation::{Layout, WIDTH, quad, sprite};
use crate::resources::{TITLE_FONT, TUTORIAL_FONT};

const INSTRUCTIONS: [&str; 6] = [
    "Color Arrows\nscroll from\nlow to high.",
    "Step when a\nColor Arrow\noverlaps the\nTarget Arrows\nat the top.",
    "The\nTraffic Light\nhelps you\nunderstand\nthe timing.",
    "The direction\nof the arrow\nsays which\nPanel\nto step on.",
    "For arrows\nfacing Left,\nstep on the\nLeft Panel.",
    "For arrows\nfacing Up,\nstep on the\nUp Panel.",
];
const INSTRUCTION_ZOOM: f32 = 0.8;

/// The scheduled chart crossings under the current music clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenItgTrainingFeedback {
    /// Most recent crossing as a left, down, up, right panel mask and its age.
    pub step: Option<(u8, f32)>,
    pub panel_ages: [Option<f32>; 4],
    pub light_ages: [Option<f32>; 4],
    pub dot_progress: [Option<f32>; 3],
}

pub fn push_training_underlay(actors: &mut Vec<Actor>, feedback: OpenItgTrainingFeedback) {
    let layout = Layout::centered(screen_width(), screen_height());
    let center = layout.position(320.0, 210.0);
    const DEPTH: i16 = -94;
    actors.push(
        sprite(
            "openitg/demo-light-frame.png",
            center,
            layout.size([76.0, 268.0]),
            [1.0; 4],
            DEPTH,
        )
        .build(0),
    );
    for (index, (key, age)) in [
        "openitg/demo-light-green.png",
        "openitg/demo-light-yellow.png",
        "openitg/demo-light-yellow.png",
        "openitg/demo-light-red.png",
    ]
    .into_iter()
    .zip(feedback.light_ages)
    .enumerate()
    {
        if let Some(age) = age.filter(|age| *age < 0.2) {
            let alpha = 1.0 - age / 0.2;
            let size = 64.0 * (1.0 + 0.4 * alpha);
            let mut light = sprite(
                key,
                layout.position(320.0, 114.0 + 64.0 * index as f32),
                layout.size([size; 2]),
                [1.0, 1.0, 1.0, alpha],
                DEPTH + 1,
            );
            light.rotationz(-45.0 * index as f32);
            light.blend(BlendMode::Add);
            actors.push(light.build(0));
        }
    }
    for (index, progress) in feedback.dot_progress.into_iter().enumerate() {
        if let Some(progress) = progress {
            let y = 178.0 + 64.0 * (index as f32 - progress);
            actors.push(
                sprite(
                    "openitg/demo-light-dot.png",
                    layout.position(320.0, y),
                    layout.size([16.0; 2]),
                    [1.0, 1.0, 1.0, 0.5],
                    DEPTH + 2,
                )
                .build(0),
            );
        }
    }
    if let Some((panels, age)) = feedback.step.filter(|(_, age)| *age < 0.6) {
        let label = match panels {
            1 => "Left!",
            2 => "Down!",
            4 => "Up!",
            8 => "Right!",
            _ => "Jump!",
        };
        let zoom = 1.0 + 0.4 * (1.0 - age / 0.2).max(0.0);
        let mut text = layout.text([320.0, 114.0], 0.5, label.into(), zoom, [1.0; 4]);
        text.font(TITLE_FONT);
        text.maxwidth(layout.size([70.0, 0.0])[0]);
        text.z(DEPTH + 3);
        actors.push(text.build(0));
    }
    push_pad(
        actors,
        layout,
        [160.0, 280.0],
        1.2,
        0.8,
        feedback
            .panel_ages
            .map(|age| age.map_or(0.0, |age| (1.0 - age / 0.3).max(0.0))),
        DEPTH,
    );
}

fn push_pad(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    zoom: f32,
    opacity: f32,
    panels: [f32; 4],
    depth: i16,
) {
    let origin = layout.position(x, y);
    let viewport = [screen_width(), screen_height()];
    let camera = menu_perspective(viewport, origin, 45.0).map(|(projection, view)| {
        let metrics = current_metrics();
        let pivot = Vec3::new(origin[0], origin[1], 0.0);
        let world_to_layout = Mat4::from_translation(Vec3::new(-metrics.left, metrics.top, 0.0))
            * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        projection
            * view
            * Mat4::from_translation(pivot)
            * Mat4::from_rotation_x((-20.0_f32).to_radians())
            * Mat4::from_translation(-pivot)
            * world_to_layout
    });
    if let Some(view_proj) = camera {
        actors.push(Actor::CameraPush { view_proj });
    }
    actors.push(
        sprite(
            "openitg/demo-platform.png",
            layout.position(x, 7.0_f32.mul_add(zoom, y)),
            layout.size([144.0 * zoom, 158.0 * zoom]),
            [0.6, 0.6, 0.6, opacity],
            depth,
        )
        .build(0),
    );
    for ([dx, dy], alpha) in [[-45.0_f32, 0.0], [0.0, 45.0], [0.0, -45.0], [45.0, 0.0]]
        .into_iter()
        .zip(panels)
    {
        if alpha > 0.0 {
            let size = 52.0 * zoom * (1.0 + 0.1 * alpha);
            let mut glow = sprite(
                "openitg/demo-panel-glow.png",
                layout.position(dx.mul_add(zoom, x), dy.mul_add(zoom, y)),
                layout.size([size; 2]),
                [1.0, 1.0, 1.0, alpha],
                depth + 1,
            );
            glow.blend(BlendMode::Add);
            actors.push(glow.build(0));
        }
    }
    if camera.is_some() {
        actors.push(Actor::CameraPop);
    }
}

pub(crate) fn push_actors(
    actors: &mut Vec<Actor>,
    demonstration: OpenItgDemonstration,
    elapsed: f32,
    music_time: f32,
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
    for (index, instruction) in INSTRUCTIONS.into_iter().enumerate() {
        let time = elapsed - (5.0 + 6.0 * index as f32);
        if !(0.0..5.8).contains(&time) {
            continue;
        }
        let slide = 1.0 - (time / 0.5).clamp(0.0, 1.0);
        // Four blank lines in the reference center these captions above the pad.
        let caption_y = if index >= 4 {
            240.0 - 2.0 * 44.0 * INSTRUCTION_ZOOM
        } else {
            240.0
        };
        let mut text = layout.text(
            [
                instruction_x + 2.0 * quarter_width * slide * slide,
                caption_y,
            ],
            0.5,
            instruction.into(),
            INSTRUCTION_ZOOM,
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
        let fade = ((time - 1.0) / 0.3).clamp(0.0, 1.0) * ((5.6 - time) / 0.3).clamp(0.0, 1.0);
        let alpha = fade * blink;
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
                if index == 4 { 0.0 } else { -90.0 },
            ),
            _ => continue,
        };
        if fade > 0.0 && index >= 4 {
            let mut panels = [0.0; 4];
            panels[if index == 4 { 0 } else { 2 }] = alpha;
            push_pad(actors, layout, [530.0, 320.0], 0.8, fade, panels, 123);
        }
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
            [0.0, 0.0],
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
