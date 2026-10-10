use super::*;
use std::hint::black_box;
mod original {
    use super::*;
    include!("cache_perf_original.rs");
}
use crate::metadata_perf as allocations;
mod paired {
    include!("cache_paired_bench.rs");
}

fn fixture(chart_count: usize, rows: usize, measures: usize) -> SerializableSongData {
    let mut song = cached_song(Path::new("song.ssc"));
    song.charts = (0..chart_count)
        .map(|index| {
            let mut chart = test_serializable_chart("dance-single", "Hard", 0, None);
            chart.offset = index as f32 * 0.125;
            chart.notes = b"1000\n".repeat(rows);
            chart.row_to_beat = (0..rows).map(|row| row as f32 * 0.25).collect();
            chart.parsed_notes = (0..rows)
                .map(|row| CachedParsedNote {
                    row_index: row as u32,
                    column: (row % 4) as u8,
                    note_type: match row % 6 {
                        0 => CachedNoteType::Tap,
                        1 => CachedNoteType::Hold,
                        2 => CachedNoteType::Roll,
                        3 => CachedNoteType::Mine,
                        4 => CachedNoteType::Lift,
                        _ => CachedNoteType::Fake,
                    },
                    tail_row_index: (row % 6 == 1 || row % 6 == 2).then_some((row + 1) as u32),
                })
                .collect();
            chart.chart_attacks = Some("mod,0,1".repeat(8));
            chart.measure_nps_vec = vec![1.25; measures];
            chart.timing_segments.bpms = vec![(0.0, 120.0), (16.0, 180.0)];
            chart.timing_segments.stops = vec![(4.0, 0.5)];
            chart.timing_segments.delays = vec![(6.0, 0.25)];
            chart.timing_segments.warps = vec![(8.0, 2.0)];
            chart.timing_segments.fakes = vec![(12.0, 2.0)];
            chart.timing_segments.speeds = vec![CachedSpeedSegment {
                beat: 0.0,
                ratio: 1.5,
                delay: 0.5,
                unit: CachedSpeedUnit::Seconds,
            }];
            chart.timing_segments.scrolls = vec![(0.0, 1.25)];
            chart.timing_segments.time_signatures = vec![(0.0, 3, 4)];
            chart.timing_segments.tickcounts = vec![(0.0, 8)];
            chart.timing_segments.combos = vec![(0.0, 2, 1)];
            chart
        })
        .collect();
    song
}

fn same_charts(a: &[GameplayChartData], b: &[GameplayChartData]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        for beat in [-4.0, 0.0, 3.5, 4.0, 6.0, 8.0, 12.0, 16.0, 100.0] {
            assert_eq!(
                a.timing.get_time_for_beat(beat).to_bits(),
                b.timing.get_time_for_beat(beat).to_bits()
            );
            assert_eq!(
                a.timing.get_displayed_beat(beat).to_bits(),
                b.timing.get_displayed_beat(beat).to_bits()
            );
            assert_eq!(
                a.timing.is_judgable_at_beat(beat),
                b.timing.is_judgable_at_beat(beat)
            );
        }
    }
}

#[test]
fn moved_gameplay_matches_original_for_order_duplicates_offsets_and_empty_requests() {
    for rows in [0, 1, 128] {
        for requested in [vec![], vec![0], vec![2, 0], vec![2, 2, 0, 2, 1]] {
            for offset in [-0.25, 0.0, 0.125] {
                let mut song = fixture(3, rows, 16);
                let expected =
                    original::build_requested_gameplay_charts(&song, &requested, offset).unwrap();
                let actual = take_requested_gameplay_charts(&mut song, &requested, offset).unwrap();
                same_charts(&actual, &expected);
            }
        }
    }
}

#[test]
fn moved_gameplay_transfers_buffers_and_keeps_duplicate_mutations_independent() {
    let mut song = fixture(2, 128, 16);
    let notes = song.charts[1].notes.as_ptr();
    let rows = song.charts[1].row_to_beat.as_ptr();
    let attacks = song.charts[1].chart_attacks.as_ref().unwrap().as_ptr();
    let mut charts = take_requested_gameplay_charts(&mut song, &[1, 1], 0.0).unwrap();
    assert_eq!(charts[0].notes.as_ptr(), notes);
    assert_eq!(charts[0].row_to_beat.as_ptr(), rows);
    assert_eq!(charts[0].chart_attacks.as_ref().unwrap().as_ptr(), attacks);
    assert!(song.charts[1].notes.is_empty());
    assert!(!song.charts[0].notes.is_empty());
    let original_time = charts[1].timing.get_time_for_beat(16.0);
    charts[0].notes[0] = b'9';
    charts[0].row_to_beat[0] = 3.0;
    charts[0].chart_attacks.as_mut().unwrap().push('x');
    charts[0].timing.set_global_offset_seconds(0.25);
    assert_ne!(charts[0].notes, charts[1].notes);
    assert_eq!(charts[1].row_to_beat[0], 0.0);
    assert_ne!(charts[0].chart_attacks, charts[1].chart_attacks);
    assert_eq!(charts[1].timing.get_time_for_beat(16.0), original_time);
}

#[test]
fn invalid_request_preserves_exact_error_and_source() {
    for requested in [vec![3], vec![1, 3, 0], vec![usize::MAX, 3]] {
        let mut song = fixture(3, 16, 0);
        let before = bincode::encode_to_vec(&song, bincode::config::standard()).unwrap();
        let expected =
            original::build_requested_gameplay_charts(&song, &requested, 0.0).unwrap_err();
        assert_eq!(
            take_requested_gameplay_charts(&mut song, &requested, 0.0).unwrap_err(),
            expected
        );
        assert_eq!(
            before,
            bincode::encode_to_vec(&song, bincode::config::standard()).unwrap()
        );
    }
}

#[test]
fn streamed_headers_match_original_bytes_and_decoded_metadata() {
    for count in [0, 1, 2, 16, 251] {
        for measures in [0, 1, 32] {
            let mut song = fixture(count, 16, measures);
            song.title = "Cache \u{00e9}\u{1f3b5}".into();
            song.song_timing = song.charts.first().map(|c| c.timing_segments.clone());
            for offset in [-0.25, 0.0, 0.125] {
                let mut old = vec![0xff; 37];
                let mut new = old.clone();
                let indices = vec![
                    CachedChartPayloadIndex {
                        offset: 251,
                        len: 65536
                    };
                    count
                ];
                original::encode_song_cache_header(&song, offset, 123456, &indices, &mut old)
                    .unwrap();
                encode_song_cache_header(&song, offset, 123456, &indices, &mut new).unwrap();
                assert_eq!(old, new);
                let (decoded, used) =
                    bincode::decode_from_slice::<CachedSong, _>(&new, bincode::config::standard())
                        .unwrap();
                assert_eq!(used, new.len());
                assert_eq!(decoded.data.charts.len(), count);
                assert_eq!(decoded.chart_payloads.len(), count);
            }
        }
    }
}

struct Assets {
    root: PathBuf,
    file: String,
}
impl Assets {
    fn new() -> Self {
        let root = test_dir("data-churn-assets");
        let path = root.join("valid-\u{00e9}.png");
        fs::write(&path, b"asset").unwrap();
        Self {
            root,
            file: path.to_str().unwrap().to_owned(),
        }
    }
}
impl Drop for Assets {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn path_fixture(file: &str, count: usize) -> CachedSong {
    let mut data = build_cached_song_meta(&fixture(count, 0, 0), 0.0);
    data.banner_path = Some(file.into());
    data.background_path = Some(file.into());
    data.cdtitle_path = Some(file.into());
    data.music_path = Some(file.into());
    let bg = SerializableSongBackgroundChange {
        start_beat: 0.0,
        target: SerializableSongBackgroundChangeTarget::File(file.into()),
        rate: 1.0,
        effect: String::new(),
        file2: Some(file.into()),
        transition: String::new(),
        color1: None,
        color2: None,
    };
    data.background_changes = vec![bg.clone(); count];
    data.background_layer2_changes = vec![bg; count];
    data.foreground_changes = vec![
        SerializableSongForegroundChange {
            start_beat: 0.0,
            path: file.into()
        };
        count
    ];
    data.background_lua_changes = vec![
        SerializableSongBackgroundLuaChange {
            start_beat: 0.0,
            path: file.into()
        };
        count
    ];
    data.foreground_lua_changes = vec![
        SerializableSongForegroundLuaChange {
            start_beat: 0.0,
            path: file.into()
        };
        count
    ];
    for chart in &mut data.charts {
        chart.music_path = Some(file.into());
    }
    CachedSong {
        cache_version: SONG_CACHE_VERSION,
        rssp_version: rssp::RSSP_VERSION.into(),
        mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
        directory_hash: 0,
        data,
        chart_payloads: vec![],
    }
}
fn replace_path(song: &mut CachedSong, slot: usize, value: &str) {
    let d = &mut song.data;
    match slot {
        0 => {
            d.background_changes[0].target =
                SerializableSongBackgroundChangeTarget::File(value.into())
        }
        1 => d.background_changes[0].file2 = Some(value.into()),
        2 => {
            d.background_layer2_changes[0].target =
                SerializableSongBackgroundChangeTarget::File(value.into())
        }
        3 => d.background_layer2_changes[0].file2 = Some(value.into()),
        4 => d.foreground_changes[0].path = value.into(),
        5 => d.foreground_lua_changes[0].path = value.into(),
        6 => d.background_lua_changes[0].path = value.into(),
        7 => d.charts[0].music_path = Some(value.into()),
        8 => d.banner_path = Some(value.into()),
        9 => d.background_path = Some(value.into()),
        10 => d.cdtitle_path = Some(value.into()),
        11 => d.music_path = Some(value.into()),
        _ => unreachable!(),
    }
}

#[test]
fn path_validation_matches_original_for_all_path_categories_and_invalid_values() {
    let assets = Assets::new();
    for count in [1, 8] {
        let base = path_fixture(&assets.file, count);
        assert!(cached_song_paths_exist(&base));
        for slot in 0..12 {
            for value in [
                "",
                " \t ",
                assets.root.to_str().unwrap(),
                assets.root.join("missing").to_str().unwrap(),
            ] {
                let mut song = path_fixture(&assets.file, count);
                replace_path(&mut song, slot, value);
                assert_eq!(
                    cached_song_paths_exist(&song),
                    original::cached_song_paths_exist(&song)
                );
                assert!(!cached_song_paths_exist(&song));
            }
        }
    }
}

#[test]
fn path_validation_retains_optional_animation_random_and_trim_semantics() {
    let assets = Assets::new();
    let mut song = path_fixture(&format!("  {} \t", assets.file), 2);
    assert!(cached_song_paths_exist(&song));
    for target in [
        SerializableSongBackgroundChangeTarget::Animation("not-a-file".into()),
        SerializableSongBackgroundChangeTarget::Random,
        SerializableSongBackgroundChangeTarget::NoSongBg,
    ] {
        song.data.background_changes[0].target = target;
        song.data.background_changes[0].file2 = None;
        song.data.banner_path = None;
        assert_eq!(
            cached_song_paths_exist(&song),
            original::cached_song_paths_exist(&song)
        );
        assert!(cached_song_paths_exist(&song));
    }
}

#[test]
fn gameplay_and_header_allocate_less() {
    let mut song = fixture(8, 16384, 128);
    let (_, old) = allocations::measure(|| {
        original::build_requested_gameplay_charts(&song, &[6, 1], 0.125).unwrap()
    });
    let (_, new) =
        allocations::measure(|| take_requested_gameplay_charts(&mut song, &[6, 1], 0.125).unwrap());
    assert!(new.allocated_bytes < old.allocated_bytes);
    assert!(new.allocs < old.allocs);
    println!("gameplay allocations: original {old:?}; current {new:?}");
    let song = fixture(64, 128, 512);
    let mut buffer = Vec::new();
    original::encode_song_cache_header(&song, 0.125, 42, &[], &mut buffer).unwrap();
    let (_, old) = allocations::measure(|| {
        original::encode_song_cache_header(&song, 0.125, 42, &[], &mut buffer).unwrap()
    });
    let (_, new) = allocations::measure(|| {
        encode_song_cache_header(&song, 0.125, 42, &[], &mut buffer).unwrap()
    });
    assert_eq!(new.allocs + 1, old.allocs);
    assert!(new.allocated_bytes < old.allocated_bytes);
    assert_eq!(new.reallocs, old.reallocs);
    assert!(new.peak_bytes < old.peak_bytes);
    println!("header allocations: original {old:?}; current {new:?}");
}

// Inputs are prepared before each timed batch. Both variants include source
// destruction, as the parse fallback drops its owned song before returning the
// gameplay charts. The benchmark also drops those outputs inside the timer.
fn compare_gameplay(label: &str, song: &SerializableSongData, requested: &[usize]) {
    let mut times = [Vec::new(), Vec::new()];
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let inputs: Vec<_> = (0..16).map(|_| song.clone()).collect();
            let total = if variant == 0 {
                let start = Instant::now();
                for input in inputs {
                    let output = original::build_requested_gameplay_charts(
                        black_box(&input),
                        black_box(requested),
                        0.125,
                    )
                    .unwrap();
                    drop(input);
                    drop(black_box(output));
                }
                start.elapsed().as_nanos()
            } else {
                let start = Instant::now();
                for mut input in inputs {
                    let output = take_requested_gameplay_charts(
                        black_box(&mut input),
                        black_box(requested),
                        0.125,
                    )
                    .unwrap();
                    drop(input);
                    drop(black_box(output));
                }
                start.elapsed().as_nanos()
            };
            if sample > 0 {
                times[variant].push(total as f64 / 16.0);
            }
        }
    }
    for t in &mut times {
        t.sort_by(f64::total_cmp);
    }
    println!(
        "{label}: original {:.2} ns/op, current {:.2} ns/op, {:.2}x throughput",
        times[0][4],
        times[1][4],
        times[0][4] / times[1][4]
    );
}

#[test]
#[ignore = "paired release benchmark; run serially with --nocapture"]
fn benchmark_cache_data_churn() {
    for rows in [0, 128, 16384, 65536] {
        let song = fixture(3, rows, 32);
        for (label, requested) in [
            ("single", vec![1]),
            ("two", vec![2, 0]),
            ("duplicate", vec![1, 1]),
        ] {
            compare_gameplay(&format!("gameplay/{rows}/{label}"), &song, &requested);
        }
    }
    for (count, rows, measures) in [
        (0, 0, 0),
        (1, 0, 0),
        (1, 128, 32),
        (8, 128, 512),
        (64, 128, 512),
    ] {
        let song = fixture(count, rows, measures);
        let mut old = Vec::new();
        let mut new = Vec::new();
        original::encode_song_cache_header(&song, 0.125, 42, &[], &mut old).unwrap();
        encode_song_cache_header(&song, 0.125, 42, &[], &mut new).unwrap();
        paired::compare(
            &format!("header/{count}/{rows}/{measures}"),
            16,
            || {
                original::encode_song_cache_header(
                    black_box(&song),
                    0.125,
                    42,
                    &[],
                    black_box(&mut old),
                )
                .unwrap();
                black_box(&old);
            },
            || {
                encode_song_cache_header(black_box(&song), 0.125, 42, &[], black_box(&mut new))
                    .unwrap();
                black_box(&new);
            },
        );
    }
    let assets = Assets::new();
    for count in [1, 8, 64] {
        for slot in [None, Some(0), Some(4), Some(8), Some(11)] {
            let mut song = path_fixture(&assets.file, count);
            if let Some(slot) = slot {
                replace_path(
                    &mut song,
                    slot,
                    assets.root.join("missing").to_str().unwrap(),
                );
            }
            paired::compare(
                &format!("paths/{count}/{slot:?}"),
                4,
                || {
                    black_box(original::cached_song_paths_exist(black_box(&song)));
                },
                || {
                    black_box(cached_song_paths_exist(black_box(&song)));
                },
            );
        }
    }
}
#[test]
fn parse_fallback_writes_intact_cache_before_moving_gameplay() {
    let assets = Assets::new();
    let simfile = assets.root.join("source.sm");
    fs::write(&simfile, "#TITLE:Move regression;\n#BPMS:0=120;\n#OFFSET:0.25;\n#NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n").unwrap();
    let cache_dir = assets.root.join("cache");
    let parse_options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
    let mut options = GameplayChartLoadOptions {
        cache_dir: &cache_dir,
        parse_options: &parse_options,
        allow_cache_read: false,
        allow_cache_write: true,
        verify_cache_freshness: false,
        global_offset_seconds: 0.125,
    };
    let song = build_song_meta(cached_song(&simfile), 0.125);
    let parsed = load_gameplay_charts_with_options(&song, &[0, 0], &options, |_| 10.0).unwrap();
    assert_eq!(parsed.report.source, GameplayChartLoadSource::Parse);
    assert!(parsed.report.warnings.is_empty());
    assert!(!parsed.charts[0].parsed_notes.is_empty());
    options.allow_cache_read = true;
    let cached = load_gameplay_charts_with_options(&song, &[0, 0], &options, |_| {
        panic!("must load written cache")
    })
    .unwrap();
    assert_eq!(
        cached.report.source,
        GameplayChartLoadSource::Cache {
            verify_freshness: false
        }
    );
    assert!(cached.report.warnings.is_empty());
    same_charts(&parsed.charts, &cached.charts);
}
