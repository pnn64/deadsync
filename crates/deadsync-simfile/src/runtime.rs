use crate::cache::{
    GameplayChartLoadLogEntry, GameplayChartLoadOptions, GameplayChartLoadReport,
    GameplayChartLoadResult, RuntimeSongLoadLogEntry, RuntimeSongLoadOptions,
    load_gameplay_charts_with_options, load_song_with_cache_options,
    load_sync_analysis_chart_with_options,
};
use crate::runtime_cache::reload_song_in_cache_with;
use crate::song::ParseSongOptions;
use deadsync_chart::SongData;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeSongConfig {
    pub fastload: bool,
    pub cachesongs: bool,
    pub global_offset_seconds: f32,
}

fn simfile_group_name(simfile_path: &Path) -> Option<&str> {
    simfile_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
}

pub fn song_group_is_never_cached(
    simfile_path: &Path,
    mut group_is_never_cached: impl FnMut(&str) -> bool,
) -> bool {
    simfile_group_name(simfile_path).is_some_and(&mut group_is_never_cached)
}

pub fn gameplay_chart_load_options<'a>(
    song: &SongData,
    cache_dir: &'a Path,
    parse_options: &'a ParseSongOptions,
    config: RuntimeSongConfig,
    mut group_is_never_cached: impl FnMut(&str) -> bool,
) -> GameplayChartLoadOptions<'a> {
    let never_cache = song_group_is_never_cached(&song.simfile_path, &mut group_is_never_cached);
    GameplayChartLoadOptions {
        cache_dir,
        parse_options,
        allow_cache_read: (config.fastload || config.cachesongs) && !never_cache,
        allow_cache_write: config.cachesongs && !never_cache,
        verify_cache_freshness: !config.fastload,
        global_offset_seconds: config.global_offset_seconds,
    }
}

pub fn sync_analysis_chart_load_options<'a>(
    song: &SongData,
    cache_dir: &'a Path,
    parse_options: &'a ParseSongOptions,
    config: RuntimeSongConfig,
    mut group_is_never_cached: impl FnMut(&str) -> bool,
) -> GameplayChartLoadOptions<'a> {
    let never_cache = song_group_is_never_cached(&song.simfile_path, &mut group_is_never_cached);
    GameplayChartLoadOptions {
        cache_dir,
        parse_options,
        allow_cache_read: (config.fastload || config.cachesongs) && !never_cache,
        allow_cache_write: false,
        verify_cache_freshness: !config.fastload,
        global_offset_seconds: 0.0,
    }
}

pub fn load_gameplay_charts_runtime(
    song: &SongData,
    requested_chart_ixs: &[usize],
    cache_dir: &Path,
    parse_options: &ParseSongOptions,
    config: RuntimeSongConfig,
    group_is_never_cached: impl FnMut(&str) -> bool,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<GameplayChartLoadResult, String> {
    let options = gameplay_chart_load_options(
        song,
        cache_dir,
        parse_options,
        config,
        group_is_never_cached,
    );
    load_gameplay_charts_with_options(song, requested_chart_ixs, &options, music_len)
}

pub fn load_sync_analysis_chart_runtime(
    song: &SongData,
    chart_ix: usize,
    cache_dir: &Path,
    parse_options: &ParseSongOptions,
    config: RuntimeSongConfig,
    group_is_never_cached: impl FnMut(&str) -> bool,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<GameplayChartLoadResult, String> {
    let options = sync_analysis_chart_load_options(
        song,
        cache_dir,
        parse_options,
        config,
        group_is_never_cached,
    );
    load_sync_analysis_chart_with_options(song, chart_ix, &options, music_len)
}

pub fn load_song_for_scan_runtime(
    simfile_path: PathBuf,
    cache_dir: &Path,
    parse_options: &ParseSongOptions,
    config: RuntimeSongConfig,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<(SongData, bool, Vec<RuntimeSongLoadLogEntry>), String> {
    let options = RuntimeSongLoadOptions {
        cache_dir,
        parse_options,
        fastload: config.fastload,
        cachesongs: config.cachesongs,
        verify_cache_freshness: !config.fastload,
        global_offset_seconds: config.global_offset_seconds,
    };
    let result = load_song_with_cache_options(&simfile_path, &options, music_len)?;
    Ok((result.song, result.cache_hit, result.log_entries))
}

pub fn reload_song_in_cache_runtime(
    simfile_path: &Path,
    cache_dir: &Path,
    parse_options: &ParseSongOptions,
    config: RuntimeSongConfig,
    mut group_is_never_cached: impl FnMut(&str) -> bool,
    mut music_len: impl FnMut(Option<&Path>) -> f32,
    mut on_song_load_log: impl FnMut(RuntimeSongLoadLogEntry),
) -> Result<Arc<SongData>, String> {
    let cachesongs =
        config.cachesongs && !song_group_is_never_cached(simfile_path, &mut group_is_never_cached);
    let load_config = RuntimeSongConfig {
        fastload: false,
        cachesongs,
        global_offset_seconds: config.global_offset_seconds,
    };
    reload_song_in_cache_with(simfile_path, |path| {
        let (song, _cache_hit, log_entries) = load_song_for_scan_runtime(
            path.to_path_buf(),
            cache_dir,
            parse_options,
            load_config,
            &mut music_len,
        )?;
        for entry in log_entries {
            on_song_load_log(entry);
        }
        Ok(song)
    })
}

pub fn gameplay_chart_load_log_entries_from_report(
    song: &SongData,
    report: &GameplayChartLoadReport,
) -> Vec<GameplayChartLoadLogEntry> {
    crate::cache::gameplay_chart_load_log_entries(song, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_group_is_never_cached_reads_pack_folder() {
        let path = Path::new("Songs/My Pack/My Song/chart.sm");
        assert!(song_group_is_never_cached(path, |group| group == "My Pack"));
        assert!(!song_group_is_never_cached(path, |group| group == "Other"));
    }

    #[test]
    fn song_group_is_never_cached_ignores_shallow_paths() {
        assert!(!song_group_is_never_cached(Path::new("chart.sm"), |_| true));
        assert!(!song_group_is_never_cached(
            Path::new("Song/chart.sm"),
            |_| true
        ));
    }
}
