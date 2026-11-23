use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::views::{GameplayHud, GameplayLifeView, GameplayScoreView};

use crate::presentation::Layout;

pub const GAMEPLAY_HUD: GameplayHud<Actor> = GameplayHud {
    standard_life: push_life_bar,
    percentage_score: push_score,
};

fn push_life_bar(actors: &mut Vec<Actor>, life: GameplayLifeView) {
    draw_life_bar(
        actors,
        Layout::centered(screen_width(), screen_height()),
        life,
    );
}

fn draw_life_bar(actors: &mut Vec<Actor>, layout: Layout, life: GameplayLifeView) {
    let x = if life.p2 { 616.0 } else { 24.0 };
    let frame = if life.failed {
        [0.39, 0.23, 0.26, 1.0]
    } else {
        [0.56, 0.65, 0.72, 1.0]
    };
    actors.push(layout.panel(x - 20.0, 113.0, [40.0, 314.0], frame, 90));
    actors.push(layout.panel(x - 17.0, 116.0, [34.0, 308.0], [0.07, 0.11, 0.16, 1.0], 91));
    actors.push(layout.panel(x - 9.0, 134.0, [18.0, 272.0], [0.12, 0.18, 0.23, 1.0], 92));

    if !life.failed && life.level > 0.0 {
        let height = 272.0 * life.level.min(1.0);
        let fill = if life.hot {
            [1.0, 0.31, 0.11, 1.0]
        } else {
            [0.17, 0.77, 0.87, 1.0]
        };
        actors.push(layout.panel(x - 9.0, 406.0 - height, [18.0, height], fill, 93));
        actors.push(layout.panel(
            x - 6.0,
            406.0 - height,
            [3.0, height],
            [1.0, 1.0, 1.0, 0.3],
            94,
        ));
    }

    if let Some(percent) = life.percent_label {
        let text = TextContent::inline_format(format_args!("{percent:.1}%"));
        if let Some(text) = text {
            let mut label = layout.text([x, 415.0], 0.5, text, 0.6, [0.92, 0.97, 1.0, 1.0]);
            label.z(94);
            actors.push(label.build(0));
        }
    }
}

fn push_score(actors: &mut Vec<Actor>, score: GameplayScoreView) {
    draw_score(
        actors,
        Layout::centered(screen_width(), screen_height()),
        score,
    );
}

fn draw_score(actors: &mut Vec<Actor>, layout: Layout, score: GameplayScoreView) {
    let x = match (score.p2, score.doubles) {
        (false, false) => 160.0,
        (false, true) => 200.0,
        (true, false) => 478.0,
        (true, true) => 440.0,
    };
    let rgba = if score.ex {
        score.rgba
    } else if score.p2 {
        [0.40, 0.96, 0.37, score.rgba[3]]
    } else {
        [0.98, 0.75, 0.10, score.rgba[3]]
    };
    let centi = (score.percent.max(0.0) * 100.0) as u32;
    let whole = centi / 100;
    let fraction = centi % 100;
    if let Some(text) = TextContent::inline_format(format_args!("{whole}.{fraction:02}%")) {
        let mut label = layout.text([x, 56.0], 0.5, text, 1.1, rgba);
        label.z(90);
        actors.push(label.build(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;

    #[test]
    fn standard_life_uses_physical_side_and_bottom_up_fill() {
        for (width, p2, x) in [(640.0, false, 24.0), (854.0, true, 723.0)] {
            let layout = Layout::centered(width, 480.0);
            let life = GameplayLifeView {
                p2,
                level: 0.25,
                failed: false,
                hot: false,
                percent_label: Some(25.0),
            };
            let mut actors = Vec::new();
            draw_life_bar(&mut actors, layout, life);
            assert!(matches!(
                &actors[3],
                Actor::Sprite {
                    offset,
                    size: [SizeSpec::Px(18.0), SizeSpec::Px(68.0)],
                    ..
                } if *offset == [x - 9.0, 338.0]
            ));
            assert!(matches!(
                actors.last(),
                Some(Actor::Text { offset, content, .. })
                    if *offset == [x, 415.0] && content.as_str() == "25.0%"
            ));

            actors.clear();
            draw_life_bar(
                &mut actors,
                layout,
                GameplayLifeView {
                    failed: true,
                    percent_label: None,
                    ..life
                },
            );
            assert_eq!(actors.len(), 3);
        }
    }

    #[test]
    fn percentage_uses_stage_score_and_keeps_ex_tint() {
        let mut actors = Vec::new();
        let score = GameplayScoreView {
            p2: true,
            doubles: true,
            percent: 98.7899,
            rgba: [0.2, 0.5, 1.0, 0.65],
            ex: true,
        };
        draw_score(&mut actors, Layout::centered(640.0, 480.0), score);
        assert!(matches!(
            actors.as_slice(),
            [Actor::Text { offset, content, color, .. }]
                if *offset == [440.0, 56.0] && *color == score.rgba && content.as_str() == "98.78%"
        ));
    }
}
