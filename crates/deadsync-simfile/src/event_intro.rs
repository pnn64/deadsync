use std::sync::Arc;

use deadsync_chart::SongData;

pub fn song_pack_group(song: &SongData) -> Option<&str> {
    song.simfile_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
}

fn itl_event_intro_name(pack_group: &str) -> Option<String> {
    let name = pack_group.trim();
    let lower = name.to_ascii_lowercase();
    let itl_pack = lower.contains("itl online ")
        || (lower.starts_with("itl ") && lower.chars().any(|c| c.is_ascii_digit()));
    if !itl_pack {
        return None;
    }

    // Personal ITL unlock packs are named "ITL Online <year> Unlocks - <username>".
    // Cut everything from the " Unlocks" marker onward (including any trailing
    // "- <username>") so the footer shows just the event name, e.g. "ITL Online 2026".
    const UNLOCKS_MARKER: &str = " unlocks";
    let name = match lower.find(UNLOCKS_MARKER) {
        Some(idx) => &name[..idx],
        None => name,
    };
    Some(name.trim().to_string())
}

pub fn event_intro_name_for_pack(pack_group: &str) -> Option<String> {
    let name = pack_group.trim();
    let lower = name.to_ascii_lowercase();
    if lower.contains("stamina rpg 10") || lower.contains("srpg10") {
        return Some("Stamina RPG 10".to_string());
    }
    if lower.contains("stamina rpg 9") || lower.contains("srpg9") {
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
    fn gameplay_event_intro_keeps_default_for_normal_pack() {
        let song = test_song("Songs/Test/Example/song.ssc", ["hard", "medium"]);
        assert_eq!(gameplay_event_intro_text(&song).as_ref(), "EVENT");
    }
}
