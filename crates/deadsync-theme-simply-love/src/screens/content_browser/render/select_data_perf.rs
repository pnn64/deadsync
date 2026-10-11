use super::*;
use crate::perf::measure;
use crate::select_data_support::compare;
use deadsync_online::smo_details::PackDetails;
use deadsync_online::stepmaniaonline::InstallSnapshot;
use std::hint::black_box;
use std::sync::Arc;

fn pack(kind: Option<&str>) -> PackInfo {
    PackInfo::new(
        7,
        "Example Pack".into(),
        78,
        671_612_928,
        None,
        kind.map(str::to_owned),
        None,
        None,
    )
}

fn state(date: Option<&str>, why: Option<&str>) -> State {
    let mut state = super::super::state::init();
    Arc::make_mut(&mut Arc::make_mut(&mut state.details).by_id).insert(
        7,
        PackDetails {
            date_added: date.map(str::to_owned),
            ..Default::default()
        },
    );
    if let Some(why) = why {
        state.search_why.insert(7, why.into());
    }
    state
}

#[test]
fn metadata_matches_original_for_all_optional_fields_and_boundaries() {
    for date in [
        None,
        Some(""),
        Some("2026-03-14"),
        Some("2026-12-00-extra"),
        Some("2026-13-01"),
        Some("year-1-4294967295"),
        Some("recently / Å猫"),
    ] {
        for why in [None, Some(""), Some("charter: Alice"), Some("Å猫 🎵")] {
            let state = state(date, why);
            for count in [0, 1, 9, 10, 99, 100, u32::MAX] {
                for bytes in [
                    0,
                    1023,
                    1024,
                    1_048_575,
                    1_048_576,
                    1_073_741_823,
                    1_073_741_824,
                    u64::MAX,
                ] {
                    let mut pack = pack(None);
                    pack.song_count = count;
                    pack.size_bytes = bytes;
                    let expected = select_data_original::meta_line(&state, &pack);
                    let actual = meta_line(&state, &pack);
                    assert_eq!(actual, expected);
                    assert_eq!(actual.len(), actual.capacity());
                }
            }
        }
    }
}

#[test]
fn metadata_avoids_field_vector_and_explanation_copy() {
    let long = "Charter Å猫 ".repeat(100);
    let state = state(Some("2026-03-14"), Some(&long));
    let pack = pack(None);
    let (original, before) = measure(|| select_data_original::meta_line(&state, &pack));
    let (current, after) = measure(|| meta_line(&state, &pack));
    assert_eq!(current, original);
    assert_eq!(after.allocs, 3);
    assert!(after.reallocs <= before.reallocs);
    assert!(after.allocs < before.allocs);
    assert!(after.allocated_bytes < before.allocated_bytes);
    assert!(after.peak_bytes < before.peak_bytes);
}

fn badge_actors(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo, original: bool) {
    if original {
        select_data_original::push_type_badge(actors, state, pack, 440.0, 180.0);
        select_data_original::push_status_badge(actors, state, pack, 456.0, 180.0);
    } else {
        push_type_badge(actors, state, pack, 440.0, 180.0);
        push_status_badge(actors, state, pack, 456.0, 180.0);
    }
}

fn normalized(mut actors: Vec<Actor>) -> String {
    for actor in &mut actors {
        if let Actor::Text { content, .. } = actor {
            *content = TextContent::Owned(content.as_str().to_owned());
        }
    }
    format!("{actors:?}")
}

fn set_install(state: &mut State, phase: Option<InstallPhase>, downloaded: u64, total: u64) {
    Arc::make_mut(&mut state.snapshot).installs = phase
        .map(|phase| InstallSnapshot {
            pack_id: 7,
            phase,
            downloaded_bytes: downloaded,
            total_bytes: total,
            message: Some("unused message".into()),
        })
        .into_iter()
        .collect();
}

#[test]
fn badges_preserve_text_color_geometry_order_and_visibility() {
    let mut state = state(None, None);
    for query in ["", "find"] {
        state.query = query.into();
        for kind in [
            None,
            Some(""),
            Some("  MiXeD  "),
            Some("DdR"),
            Some("KEYBOARD"),
            Some("pad"),
            Some("null"),
            Some("n/a"),
            Some("猫"),
            Some("KEYBOARD"),
        ] {
            let pack = pack(kind);
            for phase in [
                None,
                Some(InstallPhase::Queued),
                Some(InstallPhase::Downloading),
                Some(InstallPhase::Extracting),
                Some(InstallPhase::Installed),
                Some(InstallPhase::Error),
            ] {
                for (downloaded, total) in [
                    (0, 0),
                    (0, 100),
                    (42, 100),
                    (100, 100),
                    (101, 100),
                    (u64::MAX, 1),
                ] {
                    set_install(&mut state, phase, downloaded, total);
                    for installed in [false, true] {
                        state.installed.clear();
                        if installed {
                            state.installed.push(super::super::state::InstalledPack {
                                name: pack.name.clone(),
                                lower: pack.name.to_lowercase(),
                                songs: 78,
                                sync: SyncPref::Default,
                                banner: None,
                            });
                        }
                        let mut original = Vec::new();
                        let mut current = Vec::new();
                        badge_actors(&mut original, &state, &pack, true);
                        badge_actors(&mut current, &state, &pack, false);
                        assert_eq!(normalized(current), normalized(original));
                    }
                }
            }
        }
    }
}

#[test]
fn visible_queued_and_numeric_badges_need_no_temporary_allocations() {
    let mut state = state(None, None);
    state.query = "find".into();
    let pack = pack(Some("  MiXeD  "));
    let mut actors = Vec::with_capacity(2);
    for phase in [
        InstallPhase::Queued,
        InstallPhase::Downloading,
        InstallPhase::Extracting,
        InstallPhase::Installed,
        InstallPhase::Error,
    ] {
        set_install(&mut state, Some(phase), u64::MAX, 1);
        crate::perf::assert_no_churn(|| badge_actors(&mut actors, &state, &pack, false));
        assert_eq!(actors.len(), 2);
        assert!(actors.iter().all(|actor| matches!(
            actor,
            Actor::Text {
                content: TextContent::Static(_) | TextContent::Inline(_),
                ..
            }
        )));
        actors.clear();
    }
}

#[test]
#[ignore = "paired release benchmark; run serially with --nocapture"]
fn benchmark_select_data_metadata() {
    let long = "charter: Å猫 ".repeat(100);
    for (label, count, date, why) in [
        ("metadata/basic", 78, None, None),
        (
            "metadata/full",
            78,
            Some("2026-03-14"),
            Some("charter: Alice"),
        ),
        (
            "metadata/long-explanation",
            78,
            Some("2026-03-14"),
            Some(long.as_str()),
        ),
        ("metadata/zero-songs", 0, None, None),
        ("metadata/empty-fields", 0, Some(""), Some("")),
    ] {
        let state = state(date, why);
        let mut pack = pack(None);
        pack.song_count = count;
        compare(
            label,
            || {
                black_box(select_data_original::meta_line(
                    black_box(&state),
                    black_box(&pack),
                ));
            },
            || {
                black_box(meta_line(black_box(&state), black_box(&pack)));
            },
        );
    }
}

#[test]
#[ignore = "paired release benchmark; run serially with --nocapture"]
fn benchmark_select_data_badges() {
    for (label, query, kind, phase) in [
        (
            "badges/queued",
            "find",
            Some("MiXeD"),
            Some(InstallPhase::Queued),
        ),
        (
            "badges/downloading",
            "find",
            Some("KEYBOARD"),
            Some(InstallPhase::Downloading),
        ),
        (
            "badges/installed",
            "",
            Some("DDR"),
            Some(InstallPhase::Installed),
        ),
        ("badges/unavailable", "", Some("keyboard"), None),
        ("badges/unknown-type", "find", Some("猫"), None),
        ("badges/empty-type", "", None, None),
    ] {
        let mut state = state(None, None);
        state.query = query.into();
        set_install(&mut state, phase, 42, 100);
        let pack = pack(kind);
        let mut original = Vec::with_capacity(2);
        let mut current = Vec::with_capacity(2);
        badge_actors(&mut original, &state, &pack, true);
        badge_actors(&mut current, &state, &pack, false);
        assert_eq!(normalized(current.clone()), normalized(original.clone()));
        original.clear();
        current.clear();
        compare(
            label,
            || {
                badge_actors(
                    black_box(&mut original),
                    black_box(&state),
                    black_box(&pack),
                    true,
                );
                black_box(&original);
                original.clear();
            },
            || {
                badge_actors(
                    black_box(&mut current),
                    black_box(&state),
                    black_box(&pack),
                    false,
                );
                black_box(&current);
                current.clear();
            },
        );
    }
}

fn page_fixture() -> (State, Vec<PackInfo>) {
    let mut state = state(Some("2026-03-14"), Some("charter: Alice"));
    state.query = "Alice".into();
    let packs: Vec<_> = (0..lo::ROWS)
        .map(|index| {
            PackInfo::new(
                7 + index as u64,
                format!("Example Pack {index}"),
                78,
                671_612_928,
                None,
                Some(if index % 2 == 0 { "MiXeD" } else { "keyboard" }.into()),
                None,
                None,
            )
        })
        .collect();
    for pack in &packs {
        Arc::make_mut(&mut Arc::make_mut(&mut state.details).by_id).insert(
            pack.id,
            PackDetails {
                date_added: Some("2026-03-14".into()),
                ..Default::default()
            },
        );
        state.search_why.insert(pack.id, "charter: Alice".into());
        Arc::make_mut(&mut state.snapshot)
            .installs
            .push(InstallSnapshot {
                pack_id: pack.id,
                phase: if pack.id % 2 == 0 {
                    InstallPhase::Queued
                } else {
                    InstallPhase::Downloading
                },
                downloaded_bytes: 42,
                total_bytes: 100,
                message: None,
            });
    }
    (state, packs)
}

fn page_rows(actors: &mut Vec<Actor>, state: &State, packs: &[PackInfo], original: bool) {
    for (index, pack) in packs.iter().enumerate() {
        let at = RowPlacement {
            x: lo::LIST_X,
            y: 180.0 + index as f32 * lo::ROW_H,
            width: 440.0,
            art_cx: lo::ROW_ART_CX,
            art_w: lo::ROW_ART_W,
            text_x: lo::ROW_TEXT_X,
            selected: index == 1,
            focused: true,
            badges: true,
        };
        if original {
            select_data_original::push_row(actors, state, pack, &at, accent(state));
        } else {
            push_row(actors, state, pack, &at, accent(state));
        }
    }
}

#[test]
fn seven_browser_rows_preserve_the_complete_actor_sequence() {
    let (state, packs) = page_fixture();
    let mut original = Vec::with_capacity(lo::ROWS * 8);
    let mut current = Vec::with_capacity(lo::ROWS * 8);
    page_rows(&mut original, &state, &packs, true);
    page_rows(&mut current, &state, &packs, false);
    assert!(original.len() >= lo::ROWS * 5);
    assert_eq!(normalized(current), normalized(original));
}

#[test]
#[ignore = "paired release benchmark; run serially with --nocapture"]
fn benchmark_select_data_page() {
    let (state, packs) = page_fixture();
    let mut original = Vec::with_capacity(lo::ROWS * 8);
    let mut current = Vec::with_capacity(lo::ROWS * 8);
    compare(
        "page/7-rows",
        || {
            page_rows(
                black_box(&mut original),
                black_box(&state),
                black_box(&packs),
                true,
            );
            black_box(&original);
            original.clear();
        },
        || {
            page_rows(
                black_box(&mut current),
                black_box(&state),
                black_box(&packs),
                false,
            );
            black_box(&current);
            current.clear();
        },
    );
}
