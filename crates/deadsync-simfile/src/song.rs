use crate::artwork::resolve_song_artwork_like_itg;
use crate::cache::{
    CachedParsedNote, SerializableChartData, SerializableSongBackgroundChange,
    SerializableSongData, parse_chart_display_bpm,
};
use crate::changes::{
    extract_background_lua_changes, extract_foreground_changes, extract_foreground_lua_changes,
    resolve_background_changes_from_roots, resolve_background_layer2_changes_from_roots,
    simfile_uses_lua,
};
use crate::media::resolve_song_asset_path_like_itg;
use crate::notes::{parse_chart_notes, step_type_lanes};
use crate::stats::build_stamina_counts;
use crate::timing::{parse_time_signatures, timing_segments_from_rssp};
use rssp::{AnalysisOptions, SimfileSummary, analyze};
use std::fs;
use std::path::{Path, PathBuf};

pub const SONG_ANALYSIS_MONO_THRESHOLD: usize = 6;

pub struct ParseSongOptions {
    pub mono_threshold: usize,
    pub song_movie_roots: Vec<PathBuf>,
    pub random_movie_roots: Vec<PathBuf>,
    pub bg_animation_roots: Vec<PathBuf>,
}

pub fn parse_song_file(
    path: &Path,
    options: &ParseSongOptions,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    let simfile_data = fs::read(path).map_err(|e| format!("Could not read file: {e}"))?;
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let analysis_options = AnalysisOptions {
        mono_threshold: options.mono_threshold,
        ..AnalysisOptions::default()
    };
    let summary = analyze(&simfile_data, extension, &analysis_options)?;
    let simfile_dir = path
        .parent()
        .ok_or_else(|| "Could not determine simfile directory".to_string())?;
    let song_music_path = resolve_music_path(simfile_dir, &summary.music_path);
    let music_length_seconds = final_music_len(&summary, music_len(song_music_path.as_deref()));
    Ok(build_song_data(
        path,
        simfile_dir,
        &simfile_data,
        summary,
        song_music_path,
        music_length_seconds,
        options,
    ))
}

fn build_song_data(
    path: &Path,
    simfile_dir: &Path,
    simfile_data: &[u8],
    summary: SimfileSummary,
    song_music_path: Option<PathBuf>,
    music_length_seconds: f32,
    options: &ParseSongOptions,
) -> SerializableSongData {
    let charts = build_charts(&summary, simfile_dir, song_music_path.as_deref());
    let artwork = resolve_song_artwork_like_itg(
        simfile_dir,
        simfile_data,
        &summary.banner_path,
        &summary.background_path,
        &summary.cdtitle_path,
        &summary.jacket_path,
    );
    let has_lua = simfile_uses_lua(simfile_dir, simfile_data, &summary.background_path);
    let background_lua_changes =
        extract_background_lua_changes(simfile_dir, simfile_data, &summary.background_path);
    let foreground_changes = extract_foreground_changes(simfile_dir, simfile_data);
    let foreground_lua_changes = extract_foreground_lua_changes(simfile_dir, simfile_data);
    let background_changes = resolve_background_changes_from_roots(
        simfile_dir,
        simfile_data,
        &options.song_movie_roots,
        &options.random_movie_roots,
    )
    .iter()
    .map(SerializableSongBackgroundChange::from)
    .collect();
    let background_layer2_changes = resolve_background_layer2_changes_from_roots(
        simfile_dir,
        simfile_data,
        &options.song_movie_roots,
        &options.random_movie_roots,
        &options.bg_animation_roots,
    )
    .iter()
    .map(SerializableSongBackgroundChange::from)
    .collect();

    SerializableSongData {
        simfile_path: path.to_string_lossy().into_owned(),
        title: summary.title_str,
        subtitle: summary.subtitle_str,
        translit_title: summary.titletranslit_str,
        translit_subtitle: summary.subtitletranslit_str,
        artist: summary.artist_str,
        genre: summary.genre_str,
        banner_path: artwork
            .banner_path
            .map(|p| p.to_string_lossy().into_owned()),
        background_path: artwork
            .background_path
            .map(|p| p.to_string_lossy().into_owned()),
        background_changes,
        background_layer2_changes,
        foreground_changes,
        background_lua_changes,
        foreground_lua_changes,
        has_lua,
        cdtitle_path: artwork
            .cdtitle_path
            .map(|p| p.to_string_lossy().into_owned()),
        display_bpm: summary.display_bpm_str,
        offset: summary.offset as f32,
        sample_start: (summary.sample_start > 0.0).then_some(summary.sample_start as f32),
        sample_length: (summary.sample_length > 0.0).then_some(summary.sample_length as f32),
        min_bpm: summary.min_bpm,
        max_bpm: summary.max_bpm,
        normalized_bpms: summary.normalized_bpms,
        music_path: song_music_path.map(|p| p.to_string_lossy().into_owned()),
        music_length_seconds,
        first_second: 0.0,
        total_length_seconds: summary.total_length,
        precise_last_second_seconds: summary.total_length.max(0) as f32,
        charts,
    }
}

fn build_charts(
    summary: &SimfileSummary,
    simfile_dir: &Path,
    song_music_path: Option<&Path>,
) -> Vec<SerializableChartData> {
    let global_time_signatures = summary.normalized_time_signatures.clone();
    let allow_steps_timing =
        rssp::timing::steps_timing_allowed(summary.ssc_version, summary.timing_format);
    summary
        .charts
        .iter()
        .map(|chart| {
            let lanes = step_type_lanes(&chart.step_type_str);
            let parsed_notes = parse_chart_notes(&chart.minimized_note_data, lanes);
            let chart_time_signatures = chart
                .chart_time_signatures
                .as_deref()
                .filter(|s| !s.trim().is_empty());
            let global_time_signatures = (!global_time_signatures.trim().is_empty())
                .then_some(global_time_signatures.as_str());
            let time_signature_tag = if allow_steps_timing && chart.chart_has_own_timing {
                chart_time_signatures
            } else if allow_steps_timing {
                chart_time_signatures.or(global_time_signatures)
            } else {
                global_time_signatures
            };
            let mut timing_segments = timing_segments_from_rssp(chart.timing_segments.as_ref());
            timing_segments.time_signatures = parse_time_signatures(time_signature_tag);
            let stamina_counts = build_stamina_counts(chart);
            SerializableChartData {
                chart_type: chart.step_type_str.clone(),
                difficulty: chart.difficulty_str.clone(),
                description: chart.description_str.clone(),
                chart_name: chart.chart_name_str.clone(),
                meter: chart.rating_str.parse().unwrap_or(0),
                step_artist: chart.step_artist_str.clone(),
                music_path: chart_music_path(simfile_dir, song_music_path, &chart.music_path),
                notes: chart.minimized_note_data.clone(),
                parsed_notes: parsed_notes.iter().map(CachedParsedNote::from).collect(),
                row_to_beat: chart.row_to_beat.clone(),
                timing_segments: (&timing_segments).into(),
                short_hash: chart.short_hash.clone(),
                stats: (&chart.stats).into(),
                tech_counts: (&chart.tech_counts).into(),
                mines_nonfake: chart.mines_nonfake,
                stamina_counts: (&stamina_counts).into(),
                total_streams: chart.total_streams,
                total_measures: chart.total_measures,
                matrix_rating: chart.matrix_rating,
                max_nps: chart.max_nps,
                sn_detailed_breakdown: chart.sn_detailed_breakdown.clone(),
                sn_partial_breakdown: chart.sn_partial_breakdown.clone(),
                sn_simple_breakdown: chart.sn_simple_breakdown.clone(),
                detailed_breakdown: chart.detailed_breakdown.clone(),
                partial_breakdown: chart.partial_breakdown.clone(),
                simple_breakdown: chart.simple_breakdown.clone(),
                measure_nps_vec: chart.measure_nps_vec.clone(),
                chart_attacks: chart.chart_attacks.clone(),
                display_bpm: parse_chart_display_bpm(chart.chart_display_bpm.as_deref()),
                min_bpm: min_chart_bpm(&timing_segments.bpms),
                max_bpm: max_chart_bpm(&timing_segments.bpms),
            }
        })
        .collect()
}

fn chart_music_path(
    simfile_dir: &Path,
    song_music_path: Option<&Path>,
    chart_music_tag: &str,
) -> Option<String> {
    if chart_music_tag.trim().is_empty() {
        return song_music_path.map(|path| path.to_string_lossy().into_owned());
    }
    resolve_music_path(simfile_dir, chart_music_tag).map(|path| path.to_string_lossy().into_owned())
}

fn resolve_music_path(simfile_dir: &Path, music_tag: &str) -> Option<PathBuf> {
    resolve_song_asset_path_like_itg(simfile_dir, music_tag)
        .or_else(|| rssp::assets::resolve_music_path_like_itg(simfile_dir, music_tag))
}

fn final_music_len(summary: &SimfileSummary, decoded_len: f32) -> f32 {
    let chart_length_seconds = summary.total_length.max(0) as f32;
    if decoded_len > 0.0 && chart_length_seconds > 0.0 && decoded_len < chart_length_seconds - 10.0
    {
        chart_length_seconds
    } else {
        decoded_len
    }
}

fn min_chart_bpm(bpms: &[(f32, f32)]) -> f64 {
    bpms.iter()
        .map(|&(_, bpm)| f64::from(bpm))
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .fold(f64::INFINITY, f64::min)
        .min(f64::MAX)
}

fn max_chart_bpm(bpms: &[(f32, f32)]) -> f64 {
    bpms.iter()
        .map(|&(_, bpm)| f64::from(bpm))
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .fold(0.0_f64, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_song_payload_with_injected_music_length() {
        let root = test_dir("payload");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let simfile = song_dir.join("song.sm");
        let music = song_dir.join("music.ogg");
        fs::write(&music, b"stub").unwrap();
        fs::write(
            &simfile,
            b"#TITLE:Payload;\n\
              #ARTIST:Artist;\n\
              #MUSIC:music.ogg;\n\
              #BPMS:0.000=60.000;\n\
              #OFFSET:0.000;\n\
              #NOTES:\n\
              dance-single:\n\
              :\n\
              Challenge:\n\
              1:\n\
              0.000,0.000,0.000,0.000,0.000:\n\
              1000\n\
              ;",
        )
        .unwrap();
        let options = ParseSongOptions {
            mono_threshold: 6,
            song_movie_roots: Vec::new(),
            random_movie_roots: Vec::new(),
            bg_animation_roots: Vec::new(),
        };

        let song = parse_song_file(&simfile, &options, |_| 12.5).unwrap();

        assert_eq!(song.title, "Payload");
        assert_eq!(song.artist, "Artist");
        assert_eq!(PathBuf::from(song.music_path.unwrap()), music);
        assert_eq!(song.music_length_seconds, 12.5);
        assert_eq!(song.charts.len(), 1);
        assert_eq!(song.charts[0].meter, 1);
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-song-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
