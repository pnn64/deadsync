use deadsync_chart::SongData;
use deadsync_simfile::cache::build_song_meta;
use deadsync_simfile::song::{ParseSongOptions, parse_song_bytes};
use deadsync_theme_simply_love::screens::content_browser;
use deadsync_theme_simply_love::{
    SimplyLoveSyncOwner, SimplyLoveSyncRequest, SimplyLoveSyncTarget,
};
use std::path::Path;
use std::sync::Arc;

fn song(name: &str, extension: &str, charts: &[(&str, &str, usize)]) -> Arc<SongData> {
    let mut text = format!("#VERSION:0.83;\n#TITLE:{name};\n#BPMS:0=120;\n");
    for &(style, difficulty, lanes) in charts {
        let row = format!("1{}", "0".repeat(lanes - 1));
        if extension == "sm" {
            text.push_str(&format!(
                "#NOTES:{style}::{difficulty}:9:0,0,0,0,0:\n{row}\n;\n"
            ));
        } else {
            text.push_str(&format!("#NOTEDATA:;\n#STEPSTYPE:{style};\n#DIFFICULTY:{difficulty};\n#METER:9;\n#NOTES:\n{row}\n;\n"));
        }
    }
    let path = format!("Songs/No Singles/{name}/song.{extension}");
    let parsed = parse_song_bytes(
        text.as_bytes(),
        extension,
        Path::new(&path),
        &ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        |_| 0.0,
    )
    .unwrap();
    Arc::new(build_song_meta(parsed, 0.0))
}

fn measure(
    songs: &[Arc<SongData>],
    chart_type: &str,
    difficulty: usize,
) -> Option<Vec<SimplyLoveSyncTarget>> {
    deadsync_theme_simply_love::i18n::init_for_tests();
    let mut state = content_browser::init();
    let request = content_browser::begin_pack_measure(
        &mut state,
        "No Singles",
        songs,
        chart_type,
        difficulty,
    )?;
    let SimplyLoveSyncRequest::StartAnalysis { owner, targets, .. } = request else {
        panic!("pack measurement must start analysis");
    };
    assert_eq!(owner, SimplyLoveSyncOwner::ContentBrowserPack);
    Some(targets)
}

#[test]
fn pack_sync_styles_measures_packs_without_dance_singles() {
    for extension in ["sm", "ssc"] {
        for (style, lanes) in [("dance-double", 8), ("pump-single", 5), ("pump-double", 10)] {
            for difficulty in ["Hard", "Edit"] {
                let song = song(style, extension, &[(style, difficulty, lanes)]);
                assert_eq!(song.charts.len(), 1);
                let targets =
                    measure(&[Arc::clone(&song)], "dance-single", 3).unwrap_or_else(|| {
                        panic!("{style} {extension} {difficulty} pack was skipped in singles mode")
                    });
                assert_eq!(targets.len(), 1);
                assert_eq!(targets[0].chart_ix, 0);
                assert!(Arc::ptr_eq(&targets[0].song, &song));
            }
        }
    }
}

#[test]
fn pack_sync_styles_includes_each_song_in_a_mixed_pack_once() {
    let songs = [
        song(
            "Singles",
            "sm",
            &[
                ("dance-single", "Hard", 4),
                ("dance-single", "Challenge", 4),
            ],
        ),
        song("Doubles", "ssc", &[("dance-double", "Hard", 8)]),
        song("Pump", "sm", &[("pump-double", "Hard", 10)]),
    ];
    let targets = measure(&songs, "dance-single", 3).unwrap();
    assert_eq!(targets.len(), songs.len());
    for (target, song) in targets.iter().zip(&songs) {
        assert!(Arc::ptr_eq(&target.song, song));
        assert_eq!(target.chart_ix, 0);
    }
}

#[test]
fn pack_sync_styles_preserves_the_current_style_and_preferred_difficulty() {
    let song = song(
        "Mixed charts",
        "ssc",
        &[
            ("dance-double", "Hard", 8),
            ("dance-single", "Easy", 4),
            ("dance-single", "Challenge", 4),
            ("dance-single", "Hard", 4),
        ],
    );
    for (style, difficulty, expected) in [
        ("dance-single", 3, 3),
        ("DANCE-SINGLE", 4, 2),
        ("dance-single", 2, 1),
        ("dance-double", 4, 0),
    ] {
        let targets = measure(&[Arc::clone(&song)], style, difficulty).unwrap();
        assert_eq!(targets[0].chart_ix, expected);
    }
}

#[test]
fn pack_sync_styles_skips_charts_without_note_data() {
    let mut song = song(
        "Empty preferred chart",
        "ssc",
        &[("dance-single", "Hard", 4), ("dance-double", "Hard", 8)],
    );
    Arc::make_mut(&mut song).charts[0].has_note_data = false;
    let targets = measure(&[Arc::clone(&song)], "dance-single", 3).unwrap();
    assert_eq!(targets[0].chart_ix, 1);
    Arc::make_mut(&mut song).charts[1].has_note_data = false;
    assert!(measure(&[song], "dance-single", 3).is_none());
    assert!(measure(&[], "dance-single", 3).is_none());
}
