use std::sync::Arc;

use deadsync_chart::SongData;

#[must_use]
pub fn song_pack_group(song: &SongData) -> Option<&str> {
    song.simfile_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
}

pub fn is_srpg_event_group(pack_group: &str) -> bool {
    let name = pack_group.trim().as_bytes();
    name.iter().any(u8::is_ascii_digit)
        && (name
            .windows(b"stamina rpg".len())
            .any(|window| window.eq_ignore_ascii_case(b"stamina rpg"))
            || name
                .windows(b"srpg".len())
                .any(|window| window.eq_ignore_ascii_case(b"srpg")))
}

pub fn is_srpg_event_song(song: &SongData) -> bool {
    song_pack_group(song).is_some_and(is_srpg_event_group)
}

#[inline]
fn ascii_case_insensitive_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

fn itl_event_intro_name(pack_group: &str) -> Option<String> {
    let name = pack_group.trim();
    let bytes = name.as_bytes();
    let itl_pack = ascii_case_insensitive_find(bytes, b"itl online ").is_some()
        || (bytes
            .get(..b"itl ".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"itl "))
            && bytes.iter().any(u8::is_ascii_digit));
    if !itl_pack {
        return None;
    }

    // Personal ITL unlock packs are named "ITL Online <year> Unlocks - <username>".
    // Cut everything from the " Unlocks" marker onward (including any trailing
    // "- <username>") so the footer shows just the event name, e.g. "ITL Online 2026".
    const UNLOCKS_MARKER: &str = " unlocks";
    let name = match ascii_case_insensitive_find(bytes, UNLOCKS_MARKER.as_bytes()) {
        Some(idx) => &name[..idx],
        None => name,
    };
    Some(name.trim().to_string())
}

#[must_use]
pub fn event_intro_name_for_pack(pack_group: &str) -> Option<String> {
    let name = pack_group.trim();
    let bytes = name.as_bytes();
    if ascii_case_insensitive_find(bytes, b"stamina rpg 10").is_some()
        || ascii_case_insensitive_find(bytes, b"srpg10").is_some()
    {
        return Some("Stamina RPG 10".to_string());
    }
    if ascii_case_insensitive_find(bytes, b"stamina rpg 9").is_some()
        || ascii_case_insensitive_find(bytes, b"srpg9").is_some()
    {
        return Some("Stamina RPG 9".to_string());
    }
    itl_event_intro_name(name)
}

pub fn gameplay_event_intro_text(song: &SongData) -> Arc<str> {
    song_pack_group(song)
        .and_then(event_intro_name_for_pack)
        .map(Arc::from)
        .unwrap_or_else(|| Arc::from("EVENT"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
    use std::path::PathBuf;

    fn test_song(path: &str, hashes: [&str; 2]) -> SongData {
        SongData {
            simfile_path: PathBuf::from(path),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            translit_artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: vec![test_chart(hashes[0]), test_chart(hashes[1])],
        }
    }

    fn test_chart(hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Hard".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    #[test]
    fn gameplay_event_intro_uses_itl_pack_name() {
        let song = test_song("Songs/ITL Online 2026/Example/song.ssc", ["hard", "medium"]);
        assert_eq!(gameplay_event_intro_text(&song).as_ref(), "ITL Online 2026");
    }

    #[test]
    fn gameplay_event_intro_strips_itl_unlocks_suffix() {
        let song = test_song(
            "Songs/ITL Online 2026 Unlocks/Example/song.ssc",
            ["hard", "medium"],
        );
        assert_eq!(gameplay_event_intro_text(&song).as_ref(), "ITL Online 2026");
    }

    #[test]
    fn gameplay_event_intro_strips_itl_unlocks_username_suffix() {
        let song = test_song(
            "Songs/ITL Online 2026 Unlocks - iamchris4life/Example/song.ssc",
            ["hard", "medium"],
        );
        assert_eq!(gameplay_event_intro_text(&song).as_ref(), "ITL Online 2026");
    }

    #[test]
    fn gameplay_event_intro_uses_srpg_name() {
        let song = test_song("Songs/Stamina RPG 9/Example/song.ssc", ["hard", "medium"]);
        assert_eq!(gameplay_event_intro_text(&song).as_ref(), "Stamina RPG 9");
    }

    #[test]
    fn itl_intro_names_match_committed_behavior() {
        let cases = [
            ("ITL Online 2026", Some("ITL Online 2026")),
            (
                "  itl ONLINE 2026 UnLoCkS - Player  ",
                Some("itl ONLINE 2026"),
            ),
            ("prefix ITL Online 2025", Some("prefix ITL Online 2025")),
            ("ITL Community 17", Some("ITL Community 17")),
            ("ITL Online", None),
            ("ITL Community", None),
            ("Stamina RPG 10", None),
            (
                "\u{00c9}t\u{00e9} ITL Online 2024",
                Some("\u{00c9}t\u{00e9} ITL Online 2024"),
            ),
        ];

        for (pack, expected) in cases {
            let actual = itl_event_intro_name(pack);
            assert_eq!(actual.as_deref(), expected, "case: {pack:?}");
        }
    }

    #[test]
    fn event_intro_names_match_committed_behavior() {
        let cases = [
            ("Stamina RPG 10 Unlocks", Some("Stamina RPG 10")),
            ("prefix SRPG10 suffix", Some("Stamina RPG 10")),
            ("STAMINA rpg 9", Some("Stamina RPG 9")),
            ("srpg9 unlocks", Some("Stamina RPG 9")),
            ("ITL Online 2026 Unlocks", Some("ITL Online 2026")),
            ("ITL 2025", Some("ITL 2025")),
            ("Regular Pack 12", None),
            ("Stamina RPG Songs", None),
            ("\u{00c9}t\u{00e9} SRPG10", Some("Stamina RPG 10")),
        ];

        for (pack, expected) in cases {
            let actual = event_intro_name_for_pack(pack);
            assert_eq!(actual.as_deref(), expected, "case: {pack:?}");
        }
    }

    #[test]
    fn srpg_event_detection_accepts_current_event_names() {
        assert!(is_srpg_event_group("Stamina RPG 10"));
        assert!(is_srpg_event_group("Stamina RPG 10 Unlocks"));
        assert!(is_srpg_event_group("SRPG10"));
        assert!(is_srpg_event_group("SRPG9"));
        assert!(!is_srpg_event_group("ITL Online 2026"));
        assert!(!is_srpg_event_group("Stamina RPG Songs"));
        assert!(!is_srpg_event_group("RPG Songs"));
    }

    #[test]
    fn gameplay_event_intro_keeps_default_for_normal_pack() {
        let song = test_song("Songs/Test/Example/song.ssc", ["hard", "medium"]);
        assert_eq!(gameplay_event_intro_text(&song).as_ref(), "EVENT");
    }
}
