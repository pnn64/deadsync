use super::*;


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
                    player: 0,
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


struct Assets {
    root: PathBuf,
}
impl Assets {
    fn new() -> Self {
        let root = test_dir("data-churn-assets");
        Self {
            root,
        }
    }
}
impl Drop for Assets {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
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
