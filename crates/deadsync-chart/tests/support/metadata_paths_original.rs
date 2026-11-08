//! Frozen production kernels from e77434c22.
use super::SongData;

pub fn step_type_lanes(step_type: &str) -> usize {
    let bytes = step_type.trim().as_bytes();
    let pump = |style: &[u8]| {
        bytes.len() == 5 + style.len()
            && matches!(bytes[4], b'-' | b'_')
            && bytes[..4].eq_ignore_ascii_case(b"pump")
            && bytes[5..].eq_ignore_ascii_case(style)
    };
    let dance = |style: &[u8]| {
        bytes.len() == 6 + style.len()
            && matches!(bytes[5], b'-' | b'_')
            && bytes[..5].eq_ignore_ascii_case(b"dance")
            && bytes[6..].eq_ignore_ascii_case(style)
    };
    // Partner charts span both pads, like doubles.
    if pump(b"double") || pump(b"couple") || pump(b"routine") {
        10
    } else if dance(b"double") || dance(b"couple") || dance(b"routine") {
        8
    } else if pump(b"single") {
        5
    } else {
        4
    }
}

pub fn is_no_cmod(song: &SongData) -> bool {
    let contains = |text: &str| {
        text.as_bytes()
            .windows(b"no cmod".len())
            .any(|window| window.eq_ignore_ascii_case(b"no cmod"))
    };
    if contains(&song.title) {
        return true;
    }
    if song.subtitle.trim().is_empty() {
        return false;
    }
    contains(&song.subtitle)
        || (song
            .title
            .as_bytes()
            .get(song.title.len().wrapping_sub(2)..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(b"no"))
            && song
                .subtitle
                .as_bytes()
                .get(..4)
                .is_some_and(|head| head.eq_ignore_ascii_case(b"cmod")))
}

pub fn has_standard_difficulty(song: &SongData, chart_type: &str, difficulty_index: usize) -> bool {
    let Some(target) = deadsync_chart::STANDARD_DIFFICULTY_NAMES.get(difficulty_index) else {
        return false;
    };
    song.charts.iter().any(|chart| {
        chart.chart_type.eq_ignore_ascii_case(chart_type)
            && chart.difficulty.eq_ignore_ascii_case(target)
    })
}
