use super::*;
use crate::{paired_bench, perf_alloc};
use std::{cell::RefCell, hint::black_box};

#[path = "import_original.rs"]
mod original;

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

fn observe_import(
    current: bool,
    source: &ItgSource,
    packs: &[SongPack],
    mode: u8,
) -> (String, Vec<String>) {
    let events = RefCell::new(Vec::new());
    let options = PlayerOptionsData::default();
    macro_rules! run {
        ($run:path) => {
            $run(
                source,
                &options,
                &options,
                packs,
                |guid| {
                    events.borrow_mut().push(format!("lookup:{guid}"));
                    (mode == 1).then(|| "Existing Player".to_owned())
                },
                |data| {
                    events.borrow_mut().push(format!(
                        "create:{}:{}:{:?}:{:?}",
                        data.guid, data.display_name, data.options_singles, data.options_doubles
                    ));
                    if mode == 3 {
                        Err(std::io::Error::other("fixture failure"))
                    } else {
                        Ok("new-profile".to_owned())
                    }
                },
                |id, initials, entries| {
                    for (hash, entry) in &entries {
                        events.borrow_mut().push(format!(
                            "score:{id}:{initials}:{hash}:{:?}",
                            encode_local_score_entry(entry)
                        ));
                    }
                    (entries.len(), mode == 2)
                },
                |id| events.borrow_mut().push(format!("delete:{id}")),
                |id, favorites| {
                    let mut favorites: Vec<_> = favorites.iter().collect();
                    favorites.sort();
                    events
                        .borrow_mut()
                        .push(format!("favorites:{id}:{favorites:?}"));
                },
                |id, combo| events.borrow_mut().push(format!("stats:{id}:{combo}")),
                |id, json| {
                    events.borrow_mut().push(format!("itl:{id}:{json}"));
                    7
                },
            )
        };
    }
    let result = if current {
        format!("{:?}", run!(crate::pipeline::run_import))
    } else {
        format!("{:?}", run!(original::pipeline::run_import))
    };
    (result, events.into_inner())
}

#[test]
fn import_callbacks_and_results_match_original_for_all_exit_paths() {
    let packs = catalog(12);
    let mut source = source(16);
    for guid in ["", "  ", " 99F55B745304EBCF "] {
        source.guid = guid.into();
        for mode in 0..4 {
            assert_eq!(
                observe_import(true, &source, &packs, mode),
                observe_import(false, &source, &packs, mode),
                "guid={guid:?}, mode={mode}"
            );
        }
    }
}

fn duplicate_import(
    current: bool,
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
    if current {
        run!(crate::pipeline::run_import);
    } else {
        run!(original::pipeline::run_import);
    }
}

#[test]
fn duplicate_import_allocation_cost_does_not_scale_with_library_or_scores() {
    let options = PlayerOptionsData::default();
    let packs = catalog(512);
    let source = source(512);
    let (_, before) = perf_alloc::measure(|| duplicate_import(false, &source, &packs, &options));
    let (_, after) = perf_alloc::measure(|| duplicate_import(true, &source, &packs, &options));
    let empty = ItgSource {
        guid: source.guid.clone(),
        editable: source.editable.clone(),
        ..Default::default()
    };
    let (_, small) = perf_alloc::measure(|| duplicate_import(true, &empty, &[], &options));
    assert_eq!(after, small);
    assert!(
        before.allocs > after.allocs + 4000,
        "{before:?} -> {after:?}"
    );
}

#[test]
fn resolver_matches_original_for_aliases_case_unicode_edits_and_misses() {
    let mut packs = catalog(513);
    for len in [1, 7, 8, 15, 16, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
        for name in [
            "A".repeat(len),
            format!("{}\u{00c4}\u{6771}Mixed", "Z".repeat(len)),
        ] {
            packs[0].songs.push(Arc::new(song(
                &format!("Songs/Folder Pack/{name}/a.ssc"),
                vec![chart("Hard", &name)],
            )));
        }
    }
    // Equal and case-variant source names, plus a nested group alias.
    for i in [0, 1, 256, 512] {
        let mut duplicate = (*packs[0].songs[i]).clone();
        duplicate.simfile_path = PathBuf::from(
            duplicate
                .simfile_path
                .to_str()
                .unwrap()
                .to_ascii_lowercase(),
        );
        duplicate.title = format!("case variant {i}");
        packs[0].songs.push(Arc::new(duplicate));
    }
    let mut unicode = library();
    unicode[0].group_name = "\u{00c4}\u{6771} PACK".into();
    unicode[0].directory = "Songs/\u{00c4}\u{6771} FOLDER".into();
    packs.extend(unicode);
    let mut nested = catalog(2);
    nested[0].group_name = "Nested".into();
    nested[0].directory = "Songs/Nested".into();
    packs.extend(nested);
    let before = original::resolver::ChartResolver::build(&packs);
    let after = ChartResolver::build(&packs);
    for pack in &packs {
        for song in &pack.songs {
            let folder = song
                .simfile_path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
            for alias in [
                &pack.group_name,
                pack.directory.file_name().unwrap().to_str().unwrap(),
            ] {
                for path in [
                    format!("Songs/{alias}/{folder}/"),
                    format!("AdditionalSongs\\{alias}\\{folder}"),
                    format!("{alias}/{folder}").to_ascii_lowercase(),
                    format!("Songs/{alias}/Nested/{folder}"),
                ] {
                    assert_eq!(
                        after.resolve_song(&path).map(std::ptr::from_ref),
                        before.resolve_song(&path).map(std::ptr::from_ref),
                        "{path}"
                    );
                    for (kind, diff, desc) in [
                        ("dance-single", "hard", ""),
                        ("DANCE-SINGLE", "Edit", "named edit"),
                        ("dance-single", "Edit", ""),
                        ("dance-double", "Hard", ""),
                        ("dance-single", "Easy", ""),
                    ] {
                        assert_eq!(
                            format!("{:?}", after.resolve(&path, kind, diff, desc)),
                            format!("{:?}", before.resolve(&path, kind, diff, desc)),
                            "{path}"
                        );
                    }
                }
            }
        }
    }
    for path in [
        "",
        "Songs",
        "Songs/Missing/Unknown",
        "///",
        "Folder Pack/Missing",
    ] {
        assert_eq!(
            after.resolve_song(path).map(std::ptr::from_ref),
            before.resolve_song(path).map(std::ptr::from_ref)
        );
    }
    for prefix in [
        "",
        "Songs/",
        "AdditionalSongs\\",
        " Outer /",
        "Songs/Outer/Inner/",
    ] {
        for pack in [
            "Folder Pack",
            "Nested",
            "Missing",
            "Songs",
            "AdditionalSongs",
            "folder pack",
        ] {
            for middle in ["/", "/Folder Pack/", "\\ missing \\ deeper \\", "/// \\ "] {
                for folder in [
                    "Song 00000 With Mixed CASE",
                    "Song 00010 With Mixed CASE",
                    "Unknown",
                ] {
                    let path = format!(" {prefix}{pack}{middle}{folder} / ");
                    assert_eq!(
                        after.resolve_song(&path).map(std::ptr::from_ref),
                        before.resolve_song(&path).map(std::ptr::from_ref),
                        "{path}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_duplicate_import() {
    let options = PlayerOptionsData::default();
    for count in [32, 2000] {
        let packs = catalog(count);
        let source = source(count);
        for current in [false, true] {
            let (_, churn) =
                perf_alloc::measure(|| duplicate_import(current, &source, &packs, &options));
            println!("duplicate-{count} current={current}: {churn:?}");
        }
        paired_bench::compare(
            &format!("duplicate-{count}"),
            if count < 100 { 1024 } else { 64 },
            |current| duplicate_import(current, black_box(&source), black_box(&packs), &options),
        );
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_resolver() {
    for count in [32, 2000] {
        let packs = catalog(count);
        paired_bench::compare(
            &format!("resolver-build-{count}"),
            if count < 100 { 4096 } else { 256 },
            |current| {
                if current {
                    black_box(ChartResolver::build(black_box(&packs)));
                } else {
                    black_box(original::resolver::ChartResolver::build(black_box(&packs)));
                }
            },
        );
        let before = original::resolver::ChartResolver::build(&packs);
        let after = ChartResolver::build(&packs);
        let queries: Vec<_> = (0..count)
            .map(|i| {
                format!(
                    "Songs/Display Pack With A Long Mixed CASE Name/Song {i:05} With Mixed CASE/"
                )
            })
            .collect();
        paired_bench::compare(
            &format!("resolver-lookup-{count}"),
            if count < 100 { 8192 } else { 512 },
            |current| {
                for query in &queries {
                    if current {
                        black_box(after.resolve_song(black_box(query)));
                    } else {
                        black_box(before.resolve_song(black_box(query)));
                    }
                }
            },
        );
        perf_alloc::assert_no_churn(|| {
            for query in &queries {
                black_box(after.resolve_song(query));
            }
        });
        for case in ["miss", "fallback", "deep-fallback"] {
            let queries: Vec<_> = (0..count).map(|i| match case {
                "miss" => format!("Songs/Folder Pack/Missing Song {i:05}/"),
                "fallback" => format!("Songs/Folder Pack/Subfolder/Song {i:05} With Mixed CASE/"),
                _ => format!("Songs/Folder Pack/A/B/C/D/E/F/G/H/I/J/K/L/Subfolder/Song {i:05} With Mixed CASE/"),
            }).collect();
            paired_bench::compare(
                &format!("resolver-{case}-{count}"),
                if count < 100 { 8192 } else { 512 },
                |current| {
                    for query in &queries {
                        if current {
                            black_box(after.resolve_song(black_box(query)));
                        } else {
                            black_box(before.resolve_song(black_box(query)));
                        }
                    }
                },
            );
            perf_alloc::assert_no_churn(|| {
                for query in &queries {
                    black_box(after.resolve_song(query));
                }
            });
        }
    }
    let mut packs = catalog(32);
    packs[0].group_name = "P".into();
    packs[0].directory = "Songs/P".into();
    for (i, song) in packs[0].songs.iter_mut().enumerate() {
        Arc::make_mut(song).simfile_path = format!("Songs/P/S{i:02}/a.ssc").into();
    }
    let before = original::resolver::ChartResolver::build(&packs);
    let after = ChartResolver::build(&packs);
    let queries: Vec<_> = (0..32).map(|i| format!("P/S{i:02}")).collect();
    paired_bench::compare("resolver-tiny-keys-32", 65536, |current| {
        for query in &queries {
            if current {
                black_box(after.resolve_song(black_box(query)));
            } else {
                black_box(before.resolve_song(black_box(query)));
            }
        }
    });
}
