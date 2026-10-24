use crate::act;
use crate::fonts::machine_font_key;
use crate::i18n::tr;
use crate::screens::gameplay::State;
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadsync_rules::golf::GolfScore;
use deadsync_theme::FontRole;
use std::cell::RefCell;

thread_local! {
    static PAR_TEXT: RefCell<TextCache<(Option<u64>, u64)>> = RefCell::new(text_cache_with_capacity(128));
    static FEEDBACK_TEXT: RefCell<TextCache<(u32, u64)>> = RefCell::new(text_cache_with_capacity(128));
}

pub(crate) const COLOR: [f32; 4] = [0.2, 0.8, 0.2, 1.0];

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
    let Some((_, par)) = score(state, player) else {
        return;
    };
    let font = machine_font_key(state.machine_font(), FontRole::Normal);
    let revision = crate::i18n::revision();
    let label = cached_text(&PAR_TEXT, (par.map(f64::to_bits), revision), 256, || {
        par.map_or_else(
            || format!("{} -", tr("Golf", "Par")),
            |p| format!("{} {p:.2}", tr("Golf", "Par")),
        )
    });
    actors.push(act!(text: font(font): settext(label): align(1.0, 0.0):
        xy(position[0], position[1] + 3.0): zoom(zoom): diffuse(0.2, 0.8, 0.2, 1.0): z(91)
    ));
    if !state.profiles()[player].golf_stroke_feedback {
        return;
    }
    let Some((strokes, time)) = state.players()[player].golf_feedback else {
        return;
    };
    let age = state.total_elapsed_in_screen() - time;
    if !(0.0..0.7).contains(&age) {
        return;
    }
    let alpha = ((0.7 - age) / 0.3).clamp(0.0, 1.0);
    let t = (strokes as f32 / 65.0).clamp(0.0, 1.0).sqrt();
    let text = cached_text(&FEEDBACK_TEXT, (strokes, revision), 256, || {
        if strokes == 0 {
            tr("Golf", "Ace").to_string()
        } else {
            format!("+{strokes}")
        }
    });
    actors.push(act!(text: font(font): settext(text): align(1.0, 0.5):
        xy(position[0], position[1] + 23.0): zoom(zoom):
        diffuse(0.2 + 0.8 * t, 1.0 - 0.85 * t, 0.2 - 0.05 * t, alpha): z(91)
    ));
}
