use super::*;

#[inline(always)]
pub(super) fn format_total_songs_played(count: u32) -> String {
    let count_str = count.to_string();
    if count == 1 {
        tr_fmt(
            "SelectProfile",
            "SongPlayedSingular",
            &[("count", &count_str)],
        )
        .to_string()
    } else {
        tr_fmt(
            "SelectProfile",
            "SongPlayedPlural",
            &[("count", &count_str)],
        )
        .to_string()
    }
}
