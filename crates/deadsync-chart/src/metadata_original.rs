// Frozen starting implementations, with method receivers made explicit.
use super::*;

pub fn display_full_title(song: &SongData, translit: bool) -> String {
    let title = song.display_title(translit);
    let subtitle = song.display_subtitle(translit);
    if subtitle.trim().is_empty() {
        title.to_string()
    } else {
        format!("{title} {subtitle}")
    }
}

pub fn standard_difficulty_index(difficulty_name: &str) -> Option<usize> {
    STANDARD_DIFFICULTY_NAMES
        .iter()
        .position(|name| difficulty_name.eq_ignore_ascii_case(name))
}

#[must_use]
pub fn edit_chart_indices_sorted(song: &SongData, chart_type: &str) -> Vec<usize> {
    let mut indices = Vec::with_capacity(song.charts.len());
    indices.extend(
        song.charts
            .iter()
            .enumerate()
            .filter_map(|(index, chart)| is_edit_chart(chart, chart_type).then_some(index)),
    );
    indices.sort_by(|&left, &right| edit_chart_cmp(&song.charts[left], &song.charts[right]));
    indices
}
