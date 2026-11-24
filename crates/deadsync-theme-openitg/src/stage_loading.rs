use std::sync::Arc;

use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};

use crate::evaluation::push_arcade_backdrop;
use crate::gameplay::round_artwork;
use crate::presentation::{Layout, quad, sprite};
use crate::resources::{MENU_HEADING_FONT, MENU_NORMAL_FONT};

const ROUND_GLOWS: [&str; 6] = [
    "openitg/loading-round-1-glow.png",
    "openitg/loading-round-2-glow.png",
    "openitg/loading-round-3-glow.png",
    "openitg/loading-round-4-glow.png",
    "openitg/loading-round-5-glow.png",
    "openitg/loading-round-6-glow.png",
];

/// Match the Stage cover while the shared song preparation worker is busy.
pub fn push_stage_loading_actors(
    actors: &mut Vec<Actor>,
    title: &Arc<str>,
    stage_number: usize,
    course: bool,
    elapsed: f32,
    progress: f32,
) {
    let viewport = [screen_width(), screen_height()];
    let layout = Layout::centered(viewport[0], viewport[1]);
    let darken = (elapsed * 2.0).clamp(0.0, 1.0);
    if darken < 1.0 {
        push_arcade_backdrop(actors, viewport, elapsed);
    }
    actors.push(quad(
        [0.0; 2],
        viewport,
        [0.0, 0.0, 0.0, darken * darken],
        -10,
    ));

    let round = if course {
        Some((
            "openitg/loading-course.png",
            "openitg/loading-course-glow.png",
            [458.0, 256.0],
        ))
    } else {
        round_artwork(stage_number).map(|(key, size)| (key, ROUND_GLOWS[stage_number - 1], size))
    };
    let position = layout.position(320.0, 240.0);
    let sweep = elapsed.clamp(0.0, 1.0);
    if let Some((key, glow, size)) = round {
        let mut word = sprite(key, position, layout.size(size), [1.0; 4], -2);
        word.cropright(1.3 - sweep * 1.6);
        actors.push(word.build(0));
        if sweep < 1.0 {
            let mut edge = sprite(
                glow,
                position,
                layout.size(size.map(|axis| axis * 1.05)),
                [1.0; 4],
                -1,
            );
            edge.cropleft(sweep * 1.3 - 0.3);
            edge.cropright(1.0 - sweep * 1.3);
            edge.fadeleft(0.1);
            edge.faderight(0.1);
            actors.push(edge.build(0));
        }
    } else {
        let content = TextContent::inline_format(format_args!("ROUND {stage_number}")).unwrap();
        let mut heading = layout.text([320.0, 240.0], 0.5, content, 1.7, [1.0, 1.0, 1.0, sweep]);
        heading.font(MENU_HEADING_FONT);
        actors.push(heading.build(0));
    }

    let alpha = (elapsed / 0.3).clamp(0.0, 1.0);
    let credit_layout = layout.translated_x(-layout.position(0.0, 0.0)[0]);
    let mut title = credit_layout.text(
        [60.0, 380.0],
        0.0,
        title.into(),
        0.75,
        [1.0, 1.0, 1.0, alpha],
    );
    title.font(MENU_NORMAL_FONT);
    title.shadowlength(2.0);
    title.maxwidth(viewport[0] - layout.size([120.0, 0.0])[0]);
    actors.push(title.build(0));
    let progress =
        TextContent::inline_format(format_args!("{:.0}%", progress.clamp(0.0, 1.0) * 100.0))
            .unwrap();
    let mut progress =
        credit_layout.text([60.0, 404.0], 0.0, progress, 0.5, [1.0, 1.0, 1.0, alpha]);
    progress.font(MENU_NORMAL_FONT);
    actors.push(progress.build(0));
}
