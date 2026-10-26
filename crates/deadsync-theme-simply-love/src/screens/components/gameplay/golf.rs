use crate::act;
use crate::fonts::machine_font_key;
use crate::i18n::tr;
use crate::screens::gameplay::State;
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadsync_config::theme::MachineFont;
use deadsync_profile::PlayerSide;
use deadsync_rules::golf::GolfScore;
use deadsync_theme::FontRole;
use std::cell::RefCell;

thread_local! {
    static PAR_TEXT: RefCell<TextCache<(Option<u64>, u64)>> = RefCell::new(text_cache_with_capacity(128));
    static FEEDBACK_TEXT: RefCell<TextCache<(u32, u64)>> = RefCell::new(text_cache_with_capacity(128));
}

pub(crate) const COLOR: [f32; 4] = [0.2, 0.8, 0.2, 1.0];

pub(crate) fn format_par(par: Option<f64>) -> String {
    par.map_or_else(
        || "-".to_owned(),
        |par| {
            format!("{par:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned()
        },
    )
}

pub(crate) fn push_par_badge(
    actors: &mut Vec<Actor>,
    machine_font: MachineFont,
    par: Option<f64>,
    position: [f32; 2],
) {
    let label = cached_text(
        &PAR_TEXT,
        (par.map(f64::to_bits), crate::i18n::revision()),
        256,
        || format_par(par),
    );
    actors.push(act!(quad:
        align(0.5, 0.5): xy(position[0], position[1]): zoomto(30.0, 30.0):
        diffuse(COLOR[0], COLOR[1], COLOR[2], 1.0): z(90)
    ));
    actors.push(
        act!(text: font(machine_font_key(machine_font, FontRole::Bold)):
            settext(label): align(0.5, 0.5): xy(position[0], position[1] - 4.0):
            zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0): z(90)
        ),
    );
    actors.push(
        act!(text: font(machine_font_key(machine_font, FontRole::Normal)):
            settext(tr("Golf", "Par")): align(0.5, 0.5): xy(position[0], position[1] + 9.5):
            zoom(0.5): diffuse(0.0, 0.0, 0.0, 1.0): z(90)
        ),
    );
}

pub(crate) fn score(state: &State, player: usize) -> Option<(GolfScore, Option<f64>)> {
    let profile = state.profiles().get(player)?;
    if !profile.show_golf_score {
        return None;
    }
    let chart = state.chart(player)?;
    let par = deadsync_chart::golf::par(&state.song().simfile_path, &chart.difficulty);
    if par.is_none() && !profile.golf_all_charts {
        return None;
    }
    Some((state.player(player)?.golf_score(), par))
}

pub(crate) fn push_details(
    actors: &mut Vec<Actor>,
    state: &State,
    player: usize,
    position: [f32; 2],
    zoom: f32,
) {
    if score(state, player).is_none() {
        return;
    }
    if !state.profiles()[player].golf_stroke_feedback {
        return;
    }
    let Some((strokes, time)) = state.players()[player].golf_feedback else {
        return;
    };
    let age = state.total_elapsed_in_screen() - time;
    push_feedback(
        actors,
        strokes,
        age,
        crate::screens::gameplay::runtime_profile_side(state, player),
        position,
        zoom,
    );
}

pub(crate) fn push_feedback(
    actors: &mut Vec<Actor>,
    strokes: u32,
    age: f32,
    side: PlayerSide,
    position: [f32; 2],
    score_zoom: f32,
) {
    if !(0.0..0.7).contains(&age) {
        return;
    }
    let alpha = ((0.7 - age) / 0.3).clamp(0.0, 1.0);
    let t = (strokes as f32 / 65.0).clamp(0.0, 1.0).sqrt();
    let text = cached_text(
        &FEEDBACK_TEXT,
        (strokes, crate::i18n::revision()),
        256,
        || {
            if strokes == 0 {
                tr("Golf", "Ace").to_string()
            } else {
                format!("+{strokes}")
            }
        },
    );
    // Match the Lua's P1/P2 offsets and Common Normal font. Scale with the
    // counter when it moves into the smaller step-statistics layout.
    let scale = score_zoom / 0.5;
    let offset_x = if side == PlayerSide::P2 { -108.0 } else { 48.0 };
    actors.push(act!(text: font("miso"): settext(text): align(1.0, 0.0):
        xy(position[0] + offset_x * scale, position[1]): zoom(scale):
        diffuse(0.2 + 0.8 * t, 1.0 - 0.85 * t, 0.2 - 0.05 * t, alpha): z(91)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_badge_matches_the_supplied_theme() {
        crate::tests::init_paths();
        crate::i18n::init_for_tests();
        for (par, expected) in [
            (None, "-"),
            (Some(3.0), "3"),
            (Some(5.6), "5.6"),
            (Some(2.125), "2.12"),
        ] {
            let mut actors = Vec::new();
            push_par_badge(&mut actors, MachineFont::Wendy, par, [84.0, 56.0]);
            assert_eq!(actors.len(), 3);
            let Actor::Text {
                content,
                offset,
                scale,
                color,
                ..
            } = &actors[1]
            else {
                panic!("par number")
            };
            assert_eq!(content.as_str(), expected);
            assert_eq!(*offset, [84.0, 52.0]);
            assert_eq!(*scale, [0.4; 2]);
            assert_eq!(*color, [0.0, 0.0, 0.0, 1.0]);
            let Actor::Text {
                content,
                offset,
                scale,
                ..
            } = &actors[2]
            else {
                panic!("par label")
            };
            assert_eq!(content.as_str(), "Par");
            assert_eq!(*offset, [84.0, 65.5]);
            assert_eq!(*scale, [0.5; 2]);
        }
    }

    #[test]
    fn stroke_feedback_matches_theme_position_scale_color_and_fade() {
        crate::tests::init_paths();
        crate::i18n::init_for_tests();
        for (side, x) in [(PlayerSide::P1, 248.0), (PlayerSide::P2, 92.0)] {
            for (strokes, label, color) in [
                (0, "ACE", [0.2, 1.0, 0.2]),
                (100, "+100", [1.0, 0.15, 0.15]),
            ] {
                for (age, alpha) in [(0.0, 1.0), (0.4, 1.0), (0.55, 0.5)] {
                    let mut actors = Vec::new();
                    push_feedback(&mut actors, strokes, age, side, [200.0, 56.0], 0.5);
                    let Actor::Text {
                        content,
                        font,
                        align,
                        offset,
                        scale,
                        color: actual,
                        ..
                    } = &actors[0]
                    else {
                        panic!("feedback")
                    };
                    assert_eq!(content.as_str(), label);
                    assert_eq!(*font, "miso");
                    assert_eq!(*align, [1.0, 0.0]);
                    assert_eq!(*offset, [x, 56.0]);
                    assert_eq!(*scale, [1.0; 2]);
                    for i in 0..3 {
                        assert!((actual[i] - color[i]).abs() < 0.0001);
                    }
                    assert!((actual[3] - alpha).abs() < 0.0001);
                }
            }
        }
        let mut actors = Vec::new();
        push_feedback(&mut actors, 6, 0.0, PlayerSide::P1, [200.0, 90.0], 0.25);
        let Actor::Text { offset, scale, .. } = &actors[0] else {
            panic!("compact feedback")
        };
        assert_eq!(*offset, [224.0, 90.0]);
        assert_eq!(*scale, [0.5; 2]);
        for age in [-0.1, 0.7, 1.0] {
            let mut actors = Vec::new();
            push_feedback(&mut actors, 0, age, PlayerSide::P1, [200.0, 56.0], 0.5);
            assert!(actors.is_empty());
        }
    }
}
