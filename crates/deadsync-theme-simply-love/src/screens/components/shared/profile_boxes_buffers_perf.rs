use super::*;

#[test]
fn profile_count_preserves_localized_singular_plural_and_large_counts() {
    for (count, key) in [
        (0, "SongPlayedPlural"),
        (1, "SongPlayedSingular"),
        (2, "SongPlayedPlural"),
        (u32::MAX, "SongPlayedPlural"),
    ] {
        let digits = count.to_string();
        assert_eq!(
            format_total_songs_played(count),
            tr_fmt("SelectProfile", key, &[("count", &digits)])
        );
    }
    for count in (0..=1000).chain([9_999, 10_000, 999_999, u32::MAX]) {
        assert!(format_total_songs_played(count).contains(&count.to_string()));
    }
}
