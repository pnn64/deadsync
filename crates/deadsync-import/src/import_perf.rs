use super::*;
use crate::perf_alloc;
use std::hint::black_box;


fn catalog(count: usize) -> Vec<SongPack> {
    let mut packs = library();
    packs[0].group_name = "Display Pack With A Long Mixed CASE Name".into();
    packs[0].directory = PathBuf::from("Songs/Folder Pack");
    packs[0].songs = (0..count)
        .map(|i| {
            let mut edit = chart("Edit", &format!("edit-{i}"));
            edit.description = "Named Edit".into();
            Arc::new(song(
                &format!("Songs/Folder Pack/Song {i:05} With Mixed CASE/chart.ssc"),
                vec![chart("Hard", &format!("hard-{i}")), edit],
            ))
        })
        .collect();
    packs
}

fn source(count: usize) -> ItgSource {
    ItgSource {
        guid: " 99F55B745304EBCF ".into(),
        editable: crate::itg::ItgEditable {
            display_name: "Imported Player".into(),
            ..Default::default()
        },
        songs: (0..count)
            .map(|i| ItgSongScores {
                dir: format!("Songs/Folder Pack/Song {i:05} With Mixed CASE/"),
                steps: vec![ItgStepsScores {
                    steps_type: "dance-single".into(),
                    difficulty: "Hard".into(),
                    high_scores: vec![imported_high_score("Tier03", 500); 8],
                    ..Default::default()
                }],
            })
            .collect(),
        favorites: (0..count)
            .map(|i| format!("Folder Pack/Song {i:05} With Mixed CASE"))
            .collect(),
        itl_json: Some("{\"fixture\":true}".into()),
        current_combo: 42,
        ..Default::default()
    }
}

fn duplicate_import(
    source: &ItgSource,
    packs: &[SongPack],
    options: &PlayerOptionsData,
) {
    macro_rules! run {
        ($run:path) => {
            black_box(
                $run(
                    source,
                    options,
                    options,
                    packs,
                    |_| Some("Existing Player".to_owned()),
                    |_| panic!("duplicate created a profile"),
                    |_, _, _| panic!("duplicate wrote scores"),
                    |_| panic!("duplicate deleted a profile"),
                    |_, _| panic!("duplicate wrote favorites"),
                    |_, _| panic!("duplicate wrote stats"),
                    |_, _| panic!("duplicate wrote ITL"),
                )
                .unwrap(),
            );
        };
    }
    run!(crate::pipeline::run_import);
}

#[test]
fn duplicate_import_allocation_cost_does_not_scale_with_library_or_scores() {
    let options = PlayerOptionsData::default();
    let packs = catalog(512);
    let source = source(512);
    let (_, after) = perf_alloc::measure(|| duplicate_import( &source, &packs, &options));
    let empty = ItgSource {
        guid: source.guid.clone(),
        editable: source.editable.clone(),
        ..Default::default()
    };
    let (_, small) = perf_alloc::measure(|| duplicate_import( &empty, &[], &options));
    assert_eq!(after, small);

}
