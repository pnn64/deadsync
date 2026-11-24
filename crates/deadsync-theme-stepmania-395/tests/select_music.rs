use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use deadlib_assets::register_texture_dims;
use deadlib_present::actors::Actor;
use deadsync_chart::ChartData;
use deadsync_theme::select_music::{
    SELECT_MUSIC_WHEEL_ROWS, SelectMusicDifficultyRow, SelectMusicPlayerView, SelectMusicPrompt,
    SelectMusicSongView, SelectMusicView, SelectMusicWheelRow, WheelBannerFade,
};
use deadsync_theme::selection::{MusicWheelPresentation, WheelMediaFrame};
use deadsync_theme_stepmania_395::PackageMusic;

#[test]
fn authored_wheel_uses_live_rows_banners_and_selected_chart() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    fs::write(path.join("metrics.ini"), concat!(
        "[ScreenSelectMusic]\nWheelX=380\nWheelY=220\n",
        "BannerX=140\nBannerY=160\nBannerWidth=200\nBannerHeight=80\n",
        "BannerOnCommand=addx,-100;bounceend,0.5;addx,100\nBannerOffCommand=linear,0.4;addx,-100\n",
        "BannerFrameX=151\nBannerFrameY=164\n",
        "DifficultyIconP1X=72\nDifficultyIconP1Y=278\nMeterP1X=140\nMeterP1Y=358\n",
        "BPMDisplayX=150\nBPMDisplayY=110\n",
        "[MusicWheel]\nNumWheelItems=5\nItemSpacingY=40\nItemCurveX=100\nShowRandom=1\n",
        "[MusicWheelItem]\nSongNameX=-20\nSongNameY=0\nSectionX=-28\nSectionY=1\nSectionOnCommand=maxwidth,110\n",
        "[FadingBanner]\nBannerFadeOffCommand=diffusealpha,1;stoptweening;linear,0.25;diffusealpha,0\n",
        "[DifficultyMeter]\nShowFeet=1\nNumFeetInMeter=4\nMaxFeetInMeter=6\n",
    )).unwrap();
    let fonts = path.join("Fonts");
    fs::create_dir(&fonts).unwrap();
    fs::write(fonts.join("MusicWheelItem text.ini"), "[Char Widths]\n").unwrap();
    let graphics = path.join("Graphics");
    fs::create_dir(&graphics).unwrap();
    for (name, [w, h]) in [
        ("MusicWheelItem song.png", [190, 27]),
        ("MusicWheelItem section.png", [192, 30]),
        ("MusicWheel highlight.png", [194, 31]),
        ("ScreenSelectMusic banner frame.png", [220, 90]),
        ("ScreenSelectMusic banner mask.png", [200, 80]),
        ("ScreenSelectMusic difficulty icons 1x6.png", [20, 120]),
        ("DifficultyMeter bar 2x1.png", [16, 14]),
    ] {
        image::RgbaImage::from_pixel(w, h, image::Rgba([32, 48, 72, 255]))
            .save(graphics.join(name))
            .unwrap();
    }
    let prepared = PackageMusic::open(path, None).unwrap();
    assert_eq!(prepared.fonts.len(), 2);
    assert_eq!(prepared.textures.len(), 7);
    let mut wheel = prepared.presentation;
    assert!(wheel.show_random_entries());
    assert_eq!(wheel.banner_crossfade(), Some(Duration::from_millis(250)));
    assert_eq!(wheel.player_options_exit_seconds(), Some(0.4));
    let start = Instant::now();
    wheel.enter_wheel(start);
    let primary: Arc<str> = Arc::from("fixture/music/current");
    let previous: Arc<str> = Arc::from("fixture/music/previous");
    register_texture_dims(&primary, 400, 400);
    let mut rows = [None; SELECT_MUSIC_WHEEL_ROWS];
    let middle = SELECT_MUSIC_WHEEL_ROWS / 2;
    rows[middle] = Some(SelectMusicWheelRow {
        title: "Paper Moon",
        song_count: None,
        expanded: false,
        action: None,
    });
    rows[middle + 1] = Some(SelectMusicWheelRow {
        title: "Studio Set",
        song_count: Some(4),
        expanded: false,
        action: None,
    });
    rows[middle + 5] = Some(SelectMusicWheelRow {
        title: "Outside window",
        song_count: None,
        expanded: false,
        action: None,
    });
    let chart = example_chart();
    let make_view = || SelectMusicView {
        rows,
        wheel_offset: 0.5,
        wheel_locked: false,
        selection_beat: 0.0,
        song: Some(SelectMusicSongView {
            title: "Paper Moon",
            subtitle: "",
            artist: "",
            display_bpm: "132",
        }),
        banner_texture: Some(&primary),
        players: std::array::from_fn(|index| SelectMusicPlayerView {
            joined: index == 0,
            name: "",
            chart: Some(&chart),
            step_index: 3,
            local_records: Default::default(),
        }),
        difficulty_rows: std::array::from_fn(|index| SelectMusicDifficultyRow {
            step_index: index,
            chart: None,
        }),
        stage_number: None,
        music_rate: 1.0,
        prompt: SelectMusicPrompt::Hidden,
    };
    let mut actors = Vec::new();
    wheel.push_music(
        &mut actors,
        make_view(),
        WheelMediaFrame {
            now: start + Duration::from_secs(1),
            exit_elapsed: 0.0,
            outgoing: [
                Some(WheelBannerFade {
                    texture: &previous,
                    elapsed_seconds: 0.125,
                }),
                None,
                None,
                None,
            ],
        },
    );
    let [Actor::Frame { children, .. }] = actors.as_slice() else {
        panic!("expected song selection frame");
    };
    assert!(
        matches!(text(children, "Paper Moon"), Some(Actor::Text { offset, .. }) if *offset == [361.0,240.0])
    );
    assert!(
        matches!(text(children, "Studio Set"), Some(Actor::Text { offset, max_width: Some(110.0), .. }) if *offset == [363.0,281.0])
    );
    assert!(text(children, "Outside window").is_none());
    assert!(
        matches!(text(children, "132"), Some(Actor::Text { offset, .. }) if *offset == [150.0,110.0])
    );
    assert!(children.iter().any(|actor| matches!(actor, Actor::Sprite { mask_source: true, offset, .. } if *offset == [140.0,160.0])));
    assert!(children.iter().any(|actor| matches!(actor, Actor::Sprite { source, uv_rect: Some([0.0, top, 1.0, bottom]), mask_dest: true, .. }
        if source.texture_key() == Some(primary.as_ref()) && (*top - 0.3).abs() < 0.00001 && (*bottom - 0.7).abs() < 0.00001)));
    assert!(children.iter().any(|actor| matches!(actor, Actor::Sprite { source, tint, .. } if source.texture_key() == Some(previous.as_ref()) && tint[3] == 0.5)));
    assert!(children.iter().any(|actor| matches!(actor, Actor::Sprite { offset, uv_rect: Some([0.0,0.5,1.0,lower]), .. } if *offset == [72.0,278.0] && *lower == 4.0 / 6.0)));
    assert_eq!(children.iter().filter(|actor| matches!(actor, Actor::Sprite { offset, uv_rect: Some([0.0,0.0,0.5,1.0]), .. } if offset[1] == 358.0)).count(), 3);

    actors.clear();
    wheel.push_music(
        &mut actors,
        make_view(),
        WheelMediaFrame {
            now: start + Duration::from_secs(1),
            exit_elapsed: 0.2,
            outgoing: [None; 4],
        },
    );
    let [Actor::Frame { children, .. }] = actors.as_slice() else {
        panic!("expected song selection frame during exit");
    };
    assert!(
        children
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, offset, .. }
        if source.texture_key() == Some(primary.as_ref()) && *offset == [90.0, 160.0]))
    );
}

fn text<'a>(actors: &'a [Actor], expected: &str) -> Option<&'a Actor> {
    actors
        .iter()
        .find(|actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == expected))
}

fn example_chart() -> ChartData {
    ChartData {
        chart_type: "dance-single".to_owned(),
        difficulty: "Hard".to_owned(),
        meter: 3,
        description: String::new(),
        chart_name: String::new(),
        step_artist: String::new(),
        music_file: String::new(),
        music_path: None,
        short_hash: String::new(),
        stats: Default::default(),
        tech_counts: Default::default(),
        mines_nonfake: 0,
        stamina_counts: Default::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        matrix_profile: Box::new([]),
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 1,
        measure_nps_vec: vec![],
        measure_seconds_vec: vec![],
        first_second: 0.0,
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 132.0,
        max_bpm: 132.0,
    }
}
