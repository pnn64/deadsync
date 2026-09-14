use std::{path::PathBuf, sync::Arc};
pub fn sample_chart(chart_type: &str) -> deadsync_chart::ChartData {
    deadsync_chart::ChartData {
        chart_type: chart_type.to_string(),
        difficulty: String::new(),
        description: String::new(),
        chart_name: String::new(),
        meter: 0,
        step_artist: String::new(),
        music_path: None,
        short_hash: String::new(),
        stats: deadsync_chart::ArrowStats::default(),
        tech_counts: deadsync_chart::TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: deadsync_chart::StaminaCounts::default(),
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
        has_note_data: false,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 0.0,
        max_bpm: 0.0,
    }
}

pub fn ranked_chart(hash: &str, chart_type: &str, chart_name: &str) -> deadsync_chart::ChartData {
    let mut chart = sample_chart(chart_type);
    chart.short_hash = hash.to_string();
    chart.chart_name = chart_name.to_string();
    chart.has_note_data = true;
    chart
}

pub fn song_with_charts(charts: Vec<deadsync_chart::ChartData>) -> Arc<deadsync_chart::SongData> {
    Arc::new(deadsync_chart::SongData {
        simfile_path: PathBuf::from("/Songs/ITL Online 2026/Example/song.ssc"),
        title: String::new(),
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
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts,
    })
}

pub fn song_pack(
    group_name: &str,
    charts: Vec<deadsync_chart::ChartData>,
) -> deadsync_chart::SongPack {
    deadsync_chart::SongPack {
        group_name: group_name.to_string(),
        name: group_name.to_string(),
        sort_title: String::new(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 0,
        sync_pref: deadsync_chart::SyncPref::Default,
        directory: PathBuf::new(),
        banner_path: None,
        songs: vec![song_with_charts(charts)],
    }
}
