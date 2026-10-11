use super::*;
use crate::menu_buffers_perf_support::compare;
use crate::screens::content_browser::state;
use deadsync_online::stepmaniaonline::{CatalogPhase, PackInfo, Snapshot};
use std::{hint::black_box, sync::Arc};

include!("menu_buffers_original.rs");

fn browser(query: &str, catalog_size: usize) -> State {
    let mut state = state::init();
    state.snapshot = Arc::new(Snapshot {
        phase: CatalogPhase::Ready,
        catalog: (0..catalog_size)
            .map(|n| PackInfo::new(n as u64, format!("Pack {n}"), 10, 1, None, None, None, None))
            .collect::<Vec<_>>()
            .into(),
        revision: 1,
        message: None,
        installs: Vec::new(),
    });
    focus_search(&mut state);
    state.query = String::with_capacity(512);
    state.query.push_str(query);
    state.query_idle = 7.0;
    state.caret_elapsed = 0.75;
    state.search_why.insert(1, "old reason".into());
    state.cursor = 2;
    state
}

fn assert_state(a: &State, b: &State) {
    assert_eq!(a.query, b.query);
    assert_eq!(a.query_idle, b.query_idle);
    assert_eq!(a.caret_elapsed, b.caret_elapsed);
    assert_eq!(a.tab_index, b.tab_index);
    assert_eq!(a.zone, b.zone);
    assert_eq!(a.cursor, b.cursor);
    assert_eq!(a.results, b.results);
    assert_eq!(a.can_load_more, b.can_load_more);
    assert_eq!(a.search_why, b.search_why);
    assert_eq!(a.nav_hold.is_some(), b.nav_hold.is_some());
    assert_eq!(a.pending_turn.is_some(), b.pending_turn.is_some());
}

fn key(code: KeyCode) -> RawKeyboardEvent {
    RawKeyboardEvent {
        code,
        pressed: true,
        repeat: false,
        timestamp: std::time::Instant::now(),
        host_nanos: 0,
    }
}

#[test]
fn menu_buffers_browser_input_matches_original_edits_and_gates() {
    for query in [
        "",
        "P",
        "日本語🎵",
        &"a".repeat(63),
        &"🎵".repeat(64),
        &"x".repeat(65),
    ] {
        for text in [
            "",
            "\0\n\t\r\u{85}",
            "ack",
            "a\0b\n🎵",
            "e\u{301}",
            &"日本語\n".repeat(100),
        ] {
            for gate in 0..7 {
                let mut a = browser(query, 8);
                let mut b = browser(query, 8);
                for s in [&mut a, &mut b] {
                    match gate {
                        1 => s.zone = Zone::Detail,
                        2 => s.removing = Some("Pack 1".into()),
                        3 => {
                            s.tab_index = 1;
                            s.zone = Zone::Tabs;
                        }
                        4 => {
                            s.tab_index = 1;
                            s.zone = Zone::List;
                        }
                        5 => s.reload_prompt = Some(ReloadPrompt::default()),
                        6 => sync_dialog::open(
                            s,
                            &state::InstalledPack {
                                name: "Pack 1".into(),
                                lower: "pack 1".into(),
                                songs: 10,
                                sync: SyncPref::Default,
                                banner: None,
                            },
                        ),
                        _ => {}
                    }
                }
                let (mut ea, mut eb) = (Vec::new(), Vec::new());
                assert_eq!(
                    original_handle_raw_key_event(&mut a, None, Some(text), &mut ea),
                    handle_raw_key_event(&mut b, None, Some(text), &mut eb)
                );
                assert_state(&a, &b);
                assert_eq!(format!("{ea:?}"), format!("{eb:?}"));
                for code in [
                    KeyCode::Backspace,
                    KeyCode::Escape,
                    KeyCode::Escape,
                    KeyCode::Enter,
                    KeyCode::Backspace,
                ] {
                    let event = key(code);
                    assert_eq!(
                        original_handle_raw_key_event(&mut a, Some(&event), None, &mut ea),
                        handle_raw_key_event(&mut b, Some(&event), None, &mut eb)
                    );
                    assert_state(&a, &b);
                    assert_eq!(format!("{ea:?}"), format!("{eb:?}"));
                }
            }
        }
    }
}

#[test]
fn menu_buffers_browser_full_query_and_rejected_text_do_not_allocate() {
    let mut state = browser(&"🎵".repeat(64), 8);
    let mut effects = Vec::new();
    let paste = "a🎵\n".repeat(4096);
    crate::perf::assert_no_churn(|| {
        assert!(handle_raw_key_event(
            &mut state,
            None,
            Some(&paste),
            &mut effects
        ));
    });
    assert_eq!(state.query.chars().count(), 64);
    assert_eq!(state.query_idle, 7.0);
    assert!(effects.is_empty());
    state.zone = Zone::Detail;
    crate::perf::assert_no_churn(|| {
        assert!(!handle_raw_key_event(
            &mut state,
            None,
            Some(&paste),
            &mut effects
        ));
    });
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_menu_buffers_input() {
    for (label, query, paste, size, blocked) in [
        ("single", "P".into(), "a".into(), 0, false),
        ("paste64", "".into(), "a".repeat(64), 0, false),
        ("unicode64", "".into(), "🎵".repeat(64), 0, false),
        ("long-paste", "P".into(), "a🎵\n".repeat(4096), 0, false),
        ("full", "a".repeat(64), "text".into(), 0, false),
        ("blocked", "P".into(), "a🎵\n".repeat(4096), 0, true),
        ("controls", "P".into(), "\0\r\n".into(), 0, false),
        ("catalog4096", "".into(), "Pack 42".into(), 4096, false),
    ] {
        let mut a = browser(&query, size);
        let mut b = browser(&query, 0);
        // Search the same immutable catalog to avoid measuring fixture layout.
        b.snapshot = Arc::clone(&a.snapshot);
        if blocked {
            a.zone = Zone::Detail;
            b.zone = Zone::Detail;
        }
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        compare(
            &format!("input/{label}"),
            || {
                a.query.clear();
                a.query.push_str(black_box(&query));
                ea.clear();
                black_box(original_handle_raw_key_event(
                    black_box(&mut a),
                    None,
                    Some(black_box(&paste)),
                    &mut ea,
                ));
                black_box(&a.query);
                black_box(&a.results);
            },
            || {
                b.query.clear();
                b.query.push_str(black_box(&query));
                eb.clear();
                black_box(handle_raw_key_event(
                    black_box(&mut b),
                    None,
                    Some(black_box(&paste)),
                    &mut eb,
                ));
                black_box(&b.query);
                black_box(&b.results);
            },
        );
    }
    for (label, code) in [
        ("backspace", KeyCode::Backspace),
        ("clear", KeyCode::Escape),
    ] {
        let query = "日本語 Pack";
        let event = key(code);
        let (mut a, mut b) = (browser(query, 0), browser(query, 0));
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        compare(
            &format!("input/{label}"),
            || {
                a.query.clear();
                a.query.push_str(black_box(query));
                ea.clear();
                black_box(original_handle_raw_key_event(
                    black_box(&mut a),
                    Some(&event),
                    None,
                    &mut ea,
                ));
                black_box(&a.query);
            },
            || {
                b.query.clear();
                b.query.push_str(black_box(query));
                eb.clear();
                black_box(handle_raw_key_event(
                    black_box(&mut b),
                    Some(&event),
                    None,
                    &mut eb,
                ));
                black_box(&b.query);
            },
        );
    }
}
