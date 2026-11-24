use deadlib_present::actors::{Actor, TextAlign, TextContent};
use deadlib_present::anim;
use deadsync_theme::views::AttractRecords;

use crate::presentation::Layout;
use crate::resources::{RANKING_FONT, WHEEL_FONT};

pub(crate) const PAGE_SECONDS: f32 = 5.0;

pub(crate) fn push_ranking(actors: &mut Vec<Actor>, records: &AttractRecords, elapsed: f32) {
    let layout = Layout::current();
    actors.push(
        layout
            .sprite(
                "stepmania-395/attract-ranking",
                [320.0, 240.0],
                [640.0, 480.0],
                -100,
            )
            .build(0),
    );
    let page = (elapsed / PAGE_SECONDS) as usize;
    let time = elapsed % PAGE_SECONDS;
    let record = records.songs.get(page);
    let category = record.map_or("Recent Scores", |record| record.title.as_ref());
    let chart_type = record.map_or("dance-single", |record| record.chart_type.as_ref());
    let (kind, color) = match chart_type {
        "dance-double" => ("Double", [1.0, 0.3, 0.3, 1.0]),
        "pump-double" => ("Double", [1.0, 0.3, 0.3, 1.0]),
        _ => ("Single", [0.6, 0.6, 1.0, 1.0]),
    };
    let fade = (1.0 - ((time - 4.0) / 0.5).clamp(0.0, 1.0)).max(0.0);
    let shift = 640.0 * (1.0 - anim::bounceend_p(time.clamp(0.0, 1.0)));
    for (value, at) in [
        (category, [517.0 + shift, 32.0]),
        (kind, [520.0 + shift, 60.0]),
    ] {
        let content = TextContent::inline_str(value).unwrap_or_else(|| value.to_owned().into());
        let mut heading = layout.text(WHEEL_FONT, content, at, 1.0);
        heading.alpha(fade);
        heading.maxwidth(230.0 * layout.scale);
        actors.push(heading.build(0));
    }

    // The supplied theme's category table is reused with real chart leaderboards.
    // The game stores percentages rather than legacy category numeric scores.
    for index in 0..5 {
        let delay = 0.2 + index as f32 * 0.1;
        let slide = 640.0 * (1.0 - anim::bounceend_p((time - delay).clamp(0.0, 1.0)));
        let y = 140.0 + index as f32 * 60.0;
        let mut bullet = layout.sprite(
            "stepmania-395/attract-bullets",
            [100.0 + slide, y],
            [58.0, 50.0],
            1,
        );
        bullet.customtexturerect([0.0, index as f32 / 5.0, 1.0, (index + 1) as f32 / 5.0]);
        bullet.alpha(fade);
        actors.push(bullet.build(0));
        let row = record.and_then(|record| record.leaderboard.get(index));
        let name = row.map_or("STEP", |row| row.name.as_str());
        let score = row
            .and_then(|row| TextContent::inline_format(format_args!("{:06.2}%", row.score * 0.01)))
            .unwrap_or_else(|| "000000000".into());
        let rgba = [color[0], color[1], color[2], fade];
        push_text(
            actors,
            layout,
            TextContent::inline_str(name).unwrap_or_else(|| name.to_owned().into()),
            [180.0 + slide, y],
            0.0,
            rgba,
            280.0,
        );
        push_text(actors, layout, score, [520.0 + slide, y], 1.0, rgba, 200.0);
    }
}

fn push_text(
    actors: &mut Vec<Actor>,
    layout: Layout,
    value: TextContent,
    at: [f32; 2],
    align: f32,
    color: [f32; 4],
    maxwidth: f32,
) {
    let mut text = layout.text(RANKING_FONT, value, at, 1.0);
    text.align(align, 0.5);
    text.horizalign(if align == 0.0 {
        TextAlign::Left
    } else if align == 1.0 {
        TextAlign::Right
    } else {
        TextAlign::Center
    });
    text.diffuse(color);
    text.shadowlength(0.0);
    text.maxwidth(maxwidth * layout.scale);
    text.z(2);
    actors.push(text.build(0));
}
