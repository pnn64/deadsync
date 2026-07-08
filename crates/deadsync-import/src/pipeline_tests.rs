//! End-to-end integration test for the import pipeline: a fixture `Stats.xml`
//! string is parsed, resolved against an in-memory song library, and mapped into
//! [`LocalScoreEntry`] records, matching root import orchestration without
//! touching any global engine state or the filesystem.

use std::path::PathBuf;
use std::sync::Arc;

use deadsync_chart::{ChartData, SongData, SongPack};
use deadsync_profile::PlayerOptionsData;
use deadsync_score::{decode_local_score_entry, encode_local_score_entry, local_score_from_itg};

use super::itg::parse_song_scores;
use super::pipeline::{prepare_import, run_import};
use super::resolver::{ChartResolver, Resolution};
use super::xml;

const STATS_XML: &str = r#"<Stats>
  <SongScores>
    <Song Dir="Songs/My Pack/Cool Song/">
      <Steps StepsType="dance-single" Difficulty="Hard">
        <HighScoreList>
          <HighScore>
            <Grade>Tier03</Grade>
            <PercentDP>0.9421</PercentDP>
            <DateTime>2023-04-15 21:07:33</DateTime>
            <Modifiers>Overhead, 1.5xMusic, Reverse</Modifiers>
            <TapNoteScores>
              <W1>410</W1><W2>52</W2><W3>11</W3><W4>3</W4><W5>1</W5>
              <Miss>4</Miss><HitMine>2</HitMine><AvoidMine>7</AvoidMine>
            </TapNoteScores>
            <HoldNoteScores>
              <Held>18</Held><LetGo>2</LetGo><MissedHold>1</MissedHold>
            </HoldNoteScores>
          </HighScore>
        </HighScoreList>
      </Steps>
    </Song>
    <Song Dir="Songs/Missing Pack/Ghost Song/">
      <Steps StepsType="dance-single" Difficulty="Expert">
        <HighScoreList>
          <HighScore>
            <Grade>Tier01</Grade>
            <PercentDP>0.99</PercentDP>
            <DateTime>2023-04-16 10:00:00</DateTime>
          </HighScore>
        </HighScoreList>
      </Steps>
    </Song>
  </SongScores>
</Stats>"#;

fn chart(difficulty: &str, hash: &str) -> ChartData {
    ChartData {
        chart_type: "dance-single".into(),
        difficulty: difficulty.into(),
        description: String::new(),
        chart_name: String::new(),
        meter: 10,
        step_artist: String::new(),
        music_path: None,
        short_hash: hash.into(),
        stats: Default::default(),
        tech_counts: Default::default(),
        mines_nonfake: 0,
        stamina_counts: Default::default(),
        total_streams: 0,
        matrix_rating: 0.0,
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
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 150.0,
        max_bpm: 150.0,
    }
}

fn song(simfile_path: &str, charts: Vec<ChartData>) -> SongData {
    SongData {
        simfile_path: PathBuf::from(simfile_path),
        title: "Cool Song".into(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
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
        min_bpm: 150.0,
        max_bpm: 150.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts,
    }
}

fn library() -> Vec<SongPack> {
    vec![SongPack {
        group_name: "My Pack".into(),
        name: "My Pack".into(),
        sort_title: String::new(),
        translit_title: String::new(),
        series: String::new(),
        year: 0,
        sync_pref: deadsync_chart::SyncPref::Default,
        directory: PathBuf::from("Songs/My Pack"),
        banner_path: None,
        songs: vec![Arc::new(song(
            "Songs/My Pack/Cool Song/cool.ssc",
            vec![chart("Hard", "abc123def456")],
        ))],
    }]
}

#[test]
fn imports_stats_xml_against_library_end_to_end() {
    let root = xml::parse(STATS_XML).expect("parse Stats.xml");
    let songs = parse_song_scores(&root);
    assert_eq!(songs.len(), 2, "both <Song> blocks parsed");

    let packs = library();
    let resolver = ChartResolver::build(&packs);

    let mut imported: Vec<(String, deadsync_score::LocalScoreEntry)> = Vec::new();
    let mut song_not_found = 0usize;
    let mut chart_not_found = 0usize;
    let mut total = 0usize;

    for s in &songs {
        for steps in &s.steps {
            for hs in &steps.high_scores {
                total += 1;
                match resolver.resolve(
                    &s.dir,
                    &steps.steps_type,
                    &steps.difficulty,
                    &steps.description,
                ) {
                    Resolution::Found(hash) => {
                        let entry = local_score_from_itg(hs).expect("map high score");
                        imported.push((hash.to_string(), entry));
                    }
                    Resolution::SongNotFound => song_not_found += 1,
                    Resolution::ChartNotFound => chart_not_found += 1,
                }
            }
        }
    }

    assert_eq!(total, 2);
    assert_eq!(song_not_found, 1, "Ghost Song isn't in the library");
    assert_eq!(chart_not_found, 0);
    assert_eq!(imported.len(), 1, "Cool Song/Hard resolved and mapped");

    let (hash, entry) = &imported[0];
    assert_eq!(hash, "abc123def456");
    assert_eq!(entry.judgment_counts, [410, 52, 11, 3, 1, 4]);
    // Holds fold all hold-type tallies together: 18 + 2 + 1.
    assert_eq!(entry.holds_held, 18);
    assert_eq!(entry.holds_total, 21);
    // Mines: hit + avoided.
    assert_eq!(entry.mines_avoided, 7);
    assert_eq!(entry.mines_total, 9);
    assert!((entry.score_percent - 0.9421).abs() < 1e-6);
    assert_eq!(
        entry.ex_score_percent, 0.0,
        "EX not recoverable from Stats.xml"
    );
    assert_ne!(entry.played_at_ms, 0, "DateTime parsed");
    assert_eq!(entry.music_rate, 1.5, "music rate recovered from Modifiers");

    // The mapped entry must survive the on-disk bincode round-trip used by the
    // local-score writer.
    let bytes = encode_local_score_entry(entry).expect("encode");
    let decoded = decode_local_score_entry(&bytes).expect("decode");
    assert_eq!(&decoded, entry);
}

#[test]
fn resolves_favorite_song_to_chart_hashes() {
    let packs = library();
    let resolver = ChartResolver::build(&packs);

    // Simply Love favorites.txt stores "Pack/SongFolder" keys.
    let song = resolver
        .resolve_song("My Pack/Cool Song")
        .expect("favorite song resolves");
    let hashes: Vec<&str> = song.charts.iter().map(|c| c.short_hash.as_str()).collect();
    assert_eq!(hashes, vec!["abc123def456"]);

    assert!(
        resolver.resolve_song("Ghost Pack/Ghost Song").is_none(),
        "unknown favorite song does not resolve"
    );
}

#[test]
fn prepare_import_maps_scores_favorites_options_and_summary() {
    let root = xml::parse(STATS_XML).expect("parse Stats.xml");
    let mut source = super::itg::ItgSource::default();
    source.guid = "99f55b745304ebcf".to_string();
    source.editable.display_name = "Alice".to_string();
    source.editable.last_used_high_score_name = "itgmania-player".to_string();
    source.online.groovestats_api_key = "gs-key".to_string();
    source.online.arrowcloud_api_key = "ac-key".to_string();
    source.simply_love.insert("SpeedModType".into(), "C".into());
    source.simply_love.insert("SpeedMod".into(), "400".into());
    source.songs = parse_song_scores(&root);
    source.favorites = vec![
        "My Pack/Cool Song".to_string(),
        "Ghost Pack/Ghost Song".to_string(),
    ];
    source.itl_json = Some("{}".to_string());

    let packs = library();
    let prepared = prepare_import(
        &source,
        &PlayerOptionsData::default(),
        &PlayerOptionsData::default(),
        &packs,
    );

    assert!(!prepared.profile_guid.is_empty());
    assert_eq!(prepared.initials, "ITGM");
    assert_eq!(prepared.summary.display_name, "Alice");
    assert_eq!(prepared.summary.scores_total, 2);
    assert_eq!(prepared.summary.charts_song_not_found, 1);
    assert_eq!(prepared.summary.charts_chart_not_found, 0);
    assert_eq!(prepared.summary.scores_unmapped, 0);
    assert_eq!(prepared.summary.favorites_total, 2);
    assert_eq!(prepared.summary.favorites_imported, 1);
    assert_eq!(prepared.summary.favorites_song_not_found, 1);
    assert!(prepared.summary.simply_love_options_imported);
    assert!(prepared.summary.groovestats_imported);
    assert!(prepared.summary.arrowcloud_imported);
    assert!(prepared.summary.itl_present);
    assert_eq!(prepared.score_entries.len(), 1);
    assert_eq!(prepared.score_entries[0].0, "abc123def456");
    assert!(prepared.favorite_hashes.contains("abc123def456"));
    assert_eq!(
        prepared.options_singles.scroll_speed,
        deadsync_rules::scroll::ScrollSpeedSetting::CMod(400.0)
    );
}

#[test]
fn run_import_refuses_duplicate_profile_guid_before_writes() {
    let mut source = super::itg::ItgSource::default();
    source.guid = "99f55b745304ebcf".to_string();
    source.editable.display_name = "Alice".to_string();

    let packs = library();
    let summary = run_import(
        &source,
        &PlayerOptionsData::default(),
        &PlayerOptionsData::default(),
        &packs,
        |_| Some("Existing Alice".to_string()),
        |_| panic!("duplicate import must not create a profile"),
        |_, _, _| panic!("duplicate import must not write scores"),
        |_| panic!("duplicate import must not delete a profile"),
        |_, _| panic!("duplicate import must not write favorites"),
        |_, _| panic!("duplicate import must not write stats"),
        |_, _| panic!("duplicate import must not write ITL data"),
    )
    .expect("duplicate import should report summary");

    assert_eq!(summary.display_name, "Alice");
    assert_eq!(
        summary.already_imported_as.as_deref(),
        Some("Existing Alice")
    );
    assert!(summary.profile_id.is_empty());
}

#[test]
fn run_import_cleans_up_canceled_profile() {
    let root = xml::parse(STATS_XML).expect("parse Stats.xml");
    let mut source = super::itg::ItgSource::default();
    source.editable.display_name = "Alice".to_string();
    source.songs = parse_song_scores(&root);

    let packs = library();
    let mut deleted_profile = None;
    let summary = run_import(
        &source,
        &PlayerOptionsData::default(),
        &PlayerOptionsData::default(),
        &packs,
        |_| None,
        |_| Ok("profile-1".to_string()),
        |profile_id, initials, entries| {
            assert_eq!(profile_id, "profile-1");
            assert_eq!(initials, "ALIC");
            assert_eq!(entries.len(), 1);
            (0, true)
        },
        |profile_id| deleted_profile = Some(profile_id.to_string()),
        |_, _| panic!("canceled import must not write favorites"),
        |_, _| panic!("canceled import must not write stats"),
        |_, _| panic!("canceled import must not write ITL data"),
    )
    .expect("canceled import should report summary");

    assert!(summary.canceled);
    assert_eq!(summary.profile_id, "profile-1");
    assert_eq!(summary.scores_imported, 0);
    assert_eq!(deleted_profile.as_deref(), Some("profile-1"));
}
