use deadsync_simfile::runtime::{RuntimeSongConfig, load_song_for_scan_runtime_in};
use deadsync_simfile::runtime_cache::get_song_cache;
use deadsync_simfile::scan::{
    RuntimeScanAdapterEvent, RuntimeSongScanEnv, RuntimeSongScanEvent, SongLoadOptions,
    SongScanMode, scan_and_load_songs_with_progress_counts_runtime,
};
use deadsync_simfile::song::{ParseSongOptions, SongAnalyzer, SongParseScratch};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const SIMFILE: &str = "#TITLE:Missing Music;\n\
    #MUSIC:music.ogg;\n\
    #BPMS:0=120;\n\
    #NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n";

#[test]
fn library_reload_refreshes_added_and_removed_music_with_fastload_enabled() {
    for threads in [1, 2] {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-library-reload-{}-{nanos}-{threads}",
            std::process::id()
        ));
        let songs_root = root.join("Songs");
        let changed = songs_root.join("Pack/Changed/song.sm");
        let unchanged = songs_root.join("Pack/Unchanged/song.sm");
        for path in [&changed, &unchanged] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, SIMFILE).unwrap();
        }
        let music = changed.parent().unwrap().join("music.ogg");
        let cache_dir = root.join("cache");
        let parse_options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let analyzer = SongAnalyzer::new(&parse_options);
        let env = RuntimeSongScanEnv {
            base_root: songs_root,
            extra_song_roots: Vec::new(),
            additional_song_roots: Vec::new(),
            cache_dir: cache_dir.clone(),
            load_options: SongLoadOptions {
                fastload: true,
                cachesongs: true,
                global_offset_seconds: 0.0,
                song_parsing_threads: threads,
            },
            requested_threads: threads as u8,
        };
        let scan = |mode| {
            let mut stats = None;
            scan_and_load_songs_with_progress_counts_runtime(
                env.clone(),
                mode,
                &mut |_, _, _: &str, _: &str| {},
                SongParseScratch::default,
                |scratch, path, fastload, cachesongs, offset| {
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
                            capture_debug_logs: false,
                        },
                        |path| if path.is_some() { 12.0 } else { 0.0 },
                    )?;
                    Ok((song, hit))
                },
                |_| false,
                |event| {
                    if let RuntimeScanAdapterEvent::Song(RuntimeSongScanEvent::FinishedScan {
                        stats: loaded,
                        ..
                    }) = event
                    {
                        stats = Some(loaded);
                    }
                },
            );
            let stats = stats.unwrap();
            assert_eq!(stats.songs_failed, 0);
            stats
        };
        let song = || {
            get_song_cache()
                .iter()
                .flat_map(|pack| &pack.songs)
                .find(|song| song.simfile_path == changed)
                .unwrap()
                .clone()
        };
        let assert_music = |expected: Option<&Path>| {
            let song = song();
            assert_eq!(song.music_path.as_deref(), expected);
            assert_eq!(song.charts[0].music_path.as_deref(), expected);
            assert_eq!(
                song.music_length_seconds,
                if expected.is_some() { 12.0 } else { 0.0 }
            );
        };

        assert_eq!(scan(SongScanMode::Startup).songs_parsed, 2);
        assert_music(None);
        fs::write(&music, b"test music").unwrap();

        // FastLoad startup still trusts the cached absence of music.
        assert_eq!(scan(SongScanMode::Startup).songs_cache_hits, 2);
        assert_music(None);

        let stats = scan(SongScanMode::Reload);
        assert_eq!(stats.songs_parsed, 1, "reload must discover added music");
        assert_eq!(
            stats.songs_cache_hits, 1,
            "unchanged song should reuse cache"
        );
        assert_music(Some(&music));

        // Reload must persist the repaired metadata for later fast scans.
        assert_eq!(scan(SongScanMode::Startup).songs_cache_hits, 2);
        assert_music(Some(&music));

        fs::remove_file(&music).unwrap();
        assert_eq!(scan(SongScanMode::Reload).songs_parsed, 1);
        assert_music(None);

        fs::write(&music, b"test music").unwrap();
        assert_eq!(scan(SongScanMode::Reload).songs_parsed, 1);
        assert_music(Some(&music));
        assert_eq!(scan(SongScanMode::Reload).songs_cache_hits, 2);

        fs::remove_dir_all(root).unwrap();
    }
}
