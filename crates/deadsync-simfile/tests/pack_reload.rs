use deadsync_simfile::cache::GameplayChartLoadSource;
use deadsync_simfile::runtime::{
    RuntimeSongConfig, load_gameplay_charts_runtime, load_song_for_scan_runtime_in,
};
use deadsync_simfile::runtime_cache::get_song_cache;
use deadsync_simfile::scan::{
    RuntimeScanAdapterEvent, RuntimeSongScanEnv, RuntimeSongScanEvent, SongLoadOptions,
    reload_song_dirs_with_progress_counts_runtime,
    scan_and_load_songs_with_progress_counts_runtime,
};
use deadsync_simfile::song::{ParseSongOptions, SongAnalyzer, SongParseScratch};
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const ORIGINAL: &str = "#TITLE:Beta Song;\n\
    #BPMS:0=120;\n\
    #NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n";
const ADDED_CHART: &str =
    "#NOTES:dance-single::Challenge:12:0,0,0,0,0:\n1100\n0011\n1100\n0011\n;\n";

#[test]
fn downloaded_pack_reload_refreshes_changed_charts_with_fastload_enabled() {
    for threads in [1, 2] {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-pack-reload-{}-{nanos}-{threads}",
            std::process::id()
        ));
        let songs_root = root.join("Songs");
        let pack_dir = songs_root.join("Beta Pack");
        let changed = pack_dir.join("Changed/song.sm");
        let unchanged = pack_dir.join("Unchanged/song.sm");
        let unrelated = songs_root.join("Other Pack/Other Song/song.sm");
        for path in [&changed, &unchanged, &unrelated] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, ORIGINAL).unwrap();
        }

        let cache_dir = root.join("cache");
        let parse_options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let analyzer = SongAnalyzer::new(&parse_options);
        let config = RuntimeSongConfig {
            fastload: true,
            cachesongs: true,
            global_offset_seconds: 0.0,
            capture_debug_logs: false,
        };
        let env = RuntimeSongScanEnv {
            base_root: songs_root,
            extra_song_roots: Vec::new(),
            additional_song_roots: Vec::new(),
            cache_dir: cache_dir.clone(),
            load_options: SongLoadOptions {
                fastload: config.fastload,
                cachesongs: config.cachesongs,
                global_offset_seconds: config.global_offset_seconds,
                song_parsing_threads: threads,
            },
            requested_threads: threads as u8,
        };
        let process = |scratch: &mut SongParseScratch, path, fastload, cachesongs, offset| {
            let (song, hit, _) = load_song_for_scan_runtime_in(
                path,
                &cache_dir,
                &parse_options,
                &analyzer,
                scratch,
                RuntimeSongConfig {
                    fastload,
                    cachesongs,
                    global_offset_seconds: offset,
                    ..config
                },
                |_| 0.0,
            )?;
            Ok((song, hit))
        };
        let mut progress = |_, _, _: &str, _: &str| {};
        scan_and_load_songs_with_progress_counts_runtime(
            env.clone(),
            &mut progress,
            SongParseScratch::default,
            process,
            |_| false,
            |_| {},
        );
        let find_song = |path: &std::path::Path| {
            get_song_cache()
                .iter()
                .flat_map(|pack| &pack.songs)
                .find(|song| song.simfile_path == path)
                .unwrap()
                .clone()
        };
        assert_eq!(find_song(&changed).charts.len(), 1);
        let unrelated_before = find_song(&unrelated);

        // Archive updates can preserve modification times. Adding a difficulty
        // still changes the file size and must invalidate the existing cache.
        let modified = fs::metadata(&changed).unwrap().modified().unwrap();
        fs::write(&changed, format!("{ORIGINAL}{ADDED_CHART}")).unwrap();
        fs::File::options()
            .write(true)
            .open(&changed)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();

        let mut stats = None;
        reload_song_dirs_with_progress_counts_runtime(
            env,
            &[pack_dir],
            &mut progress,
            SongParseScratch::default,
            process,
            |_| false,
            |event| {
                if let RuntimeScanAdapterEvent::Song(RuntimeSongScanEvent::FinishedReload {
                    stats: loaded,
                    ..
                }) = event
                {
                    stats = Some(loaded);
                }
            },
        );

        let updated = find_song(&changed);
        assert_eq!(updated.charts.len(), 2, "added difficulty must be visible");
        let chart_ix = updated
            .charts
            .iter()
            .position(|chart| chart.difficulty == "Challenge")
            .unwrap();
        let stats = stats.unwrap();
        assert_eq!(stats.songs_parsed, 1);
        assert_eq!(
            stats.songs_cache_hits, 1,
            "unchanged song should reuse cache"
        );
        assert_eq!(stats.songs_failed, 0);
        assert!(Arc::ptr_eq(&unrelated_before, &find_song(&unrelated)));

        // Both future scans and gameplay must see the rewritten disk cache.
        let (cached, hit) =
            process(&mut SongParseScratch::default(), changed, true, true, 0.0).unwrap();
        assert!(hit);
        assert_eq!(cached.charts.len(), 2);
        let gameplay = load_gameplay_charts_runtime(
            &updated,
            &[chart_ix],
            &cache_dir,
            &parse_options,
            config,
            |_| false,
            |_| panic!("updated chart payload should be cached"),
        )
        .unwrap();
        assert!(matches!(
            gameplay.report.source,
            GameplayChartLoadSource::Cache { .. }
        ));
        assert_eq!(gameplay.charts.len(), 1);
        assert_eq!(gameplay.charts[0].notes, b"1100\n0011\n1100\n0011");

        fs::remove_dir_all(root).unwrap();
    }
}
