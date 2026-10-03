// Frozen from 0.5.1698; clones global timing before the owning conversion.
use super::*;

pub(super) fn build_song_data(
    mut summary: SimfileSummary,
    input: SongBuildInput<'_>,
) -> SerializableSongData {
    let SongBuildInput {
        path,
        simfile_dir,
        simfile_data,
        song_music_path,
        music_length_seconds,
        options,
        parsed_notes,
    } = input;
    let charts = build_charts(
        &mut summary,
        simfile_dir,
        song_music_path.as_deref(),
        parsed_notes,
    );
    let artwork = resolve_song_artwork_like_itg(
        simfile_dir,
        simfile_data,
        &summary.banner_path,
        &summary.background_path,
        &summary.cdtitle_path,
        &summary.jacket_path,
    );
    let background_lua_changes =
        extract_background_lua_change_set(simfile_dir, simfile_data, &summary.background_path);
    let foreground_changes = extract_foreground_change_sets(simfile_dir, simfile_data);
    let has_lua = background_lua_changes.uses_lua || foreground_changes.uses_lua;
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

    let last_second_hint = summary
        .last_second_hint
        .map(|seconds| seconds as f32)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .unwrap_or(0.0);
    SerializableSongData {
        simfile_path: path.to_string_lossy().into_owned(),
        title: summary.title_str,
        subtitle: summary.subtitle_str,
        translit_title: summary.titletranslit_str,
        translit_subtitle: summary.subtitletranslit_str,
        artist: summary.artist_str,
        translit_artist: summary.artisttranslit_str,
        genre: summary.genre_str,
        banner_path: artwork
            .banner_path
            .map(|p| p.to_string_lossy().into_owned()),
        background_path: artwork
            .background_path
            .map(|p| p.to_string_lossy().into_owned()),
        background_changes,
        background_layer2_changes,
        foreground_changes: foreground_changes.media,
        background_lua_changes: background_lua_changes.changes,
        foreground_lua_changes: foreground_changes.lua,
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
        song_timing: (has_lua
            && (!summary.global_timing_segments.stops.is_empty()
                || !summary.global_timing_segments.delays.is_empty()
                || !summary.global_timing_segments.warps.is_empty()))
        .then(|| {
            CachedTimingSegments::from_rssp_owned(
                summary.global_timing_segments.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        }),
        music_path: song_music_path.map(|p| p.to_string_lossy().into_owned()),
        music_length_seconds,
        first_second: 0.0,
        total_length_seconds: summary.total_length.max(last_second_hint as i32),
        precise_last_second_seconds: (summary.total_length.max(0) as f32).max(last_second_hint),
        last_second_hint,
        charts,
    }
}
