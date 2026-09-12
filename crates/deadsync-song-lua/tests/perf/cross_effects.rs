use super::*;
use std::hint::black_box;

#[path = "cross_effects_baseline.rs"]
mod baseline;

fn block(index: usize) -> SongLuaOverlayCommandBlock {
    SongLuaOverlayCommandBlock {
        start: index as f32,
        duration: 0.5,
        easing: Some(format!("ease_{index}")),
        opt1: Some(0.25),
        opt2: None,
        delta: SongLuaOverlayStateDelta {
            x: Some(index as f32),
            ..Default::default()
        },
    }
}

fn capture(count: usize, kind: &str) -> SongLuaFunctionActionCapture {
    let mut capture = SongLuaFunctionActionCapture::default();
    for i in 0..count {
        if kind != "blocks" {
            capture.overlay_aux.push((i * 2, i as f32));
        }
        if kind != "aux" {
            capture.overlay_blocks.push((
                i * 2 + usize::from(kind == "disjoint"),
                if kind == "empty_blocks" {
                    Vec::new()
                } else {
                    vec![block(i)]
                },
            ));
        }
    }
    if kind == "unordered" {
        capture.overlay_aux.reverse();
        capture.overlay_blocks.reverse();
    }
    if kind == "duplicates" {
        capture.overlay_aux.extend(capture.overlay_aux.clone());
        capture
            .overlay_blocks
            .extend(capture.overlay_blocks.clone());
    }
    capture
}

fn effects(
    capture: &SongLuaFunctionActionCapture,
    source: usize,
    old: bool,
) -> Vec<(usize, Vec<SongLuaOverlayCommandBlock>, Option<f32>)> {
    if old {
        baseline::cross_actor_effects(capture, source)
    } else {
        cross_actor_effects(capture, source)
    }
}

fn comparable(
    effects: Vec<(usize, Vec<SongLuaOverlayCommandBlock>, Option<f32>)>,
) -> Vec<(usize, Vec<SongLuaOverlayCommandBlock>, Option<u32>)> {
    effects
        .into_iter()
        .map(|(i, b, a)| (i, b, a.map(f32::to_bits)))
        .collect()
}

#[test]
fn lua_capture_cross_effects_match_ordered_disjoint_and_fallback_inputs() {
    for count in [0, 1, 2, 16, 65] {
        for kind in [
            "aux",
            "blocks",
            "both",
            "disjoint",
            "empty_blocks",
            "unordered",
            "duplicates",
        ] {
            let capture = capture(count, kind);
            for source in [0, 1, 2, count, usize::MAX] {
                let old = comparable(effects(&capture, source, true));
                let new = comparable(effects(&capture, source, false));
                assert_eq!(old, new, "{count}/{kind}/{source}");
                assert!(new.windows(2).all(|p| p[0].0 < p[1].0));
                assert!(new.iter().all(|(i, _, _)| *i != source));
            }
        }
    }
}

#[test]
fn lua_capture_cross_effects_preserve_last_writes_and_float_bits() {
    let mut state = 0x79d1_3e26_d45b_8109u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2000 {
        let mut capture = SongLuaFunctionActionCapture::default();
        for _ in 0..next() % 40 {
            let index = (next() % 17) as usize;
            let value = match next() % 6 {
                0 => -0.0,
                1 => f32::NAN,
                2 => f32::INFINITY,
                3 => f32::NEG_INFINITY,
                _ => next() as f32,
            };
            capture.overlay_aux.push((index, value));
        }
        for _ in 0..next() % 40 {
            capture.overlay_blocks.push((
                (next() % 17) as usize,
                if next() % 3 == 0 {
                    Vec::new()
                } else {
                    vec![block(next() as usize % 1000)]
                },
            ));
        }
        let source = next() as usize % 19;
        assert_eq!(
            comparable(effects(&capture, source, true)),
            comparable(effects(&capture, source, false))
        );
        // Exercise the ordered path with the same unusual aux values too.
        capture.overlay_aux.sort_by_key(|(index, _)| *index);
        capture.overlay_aux.dedup_by_key(|(index, _)| *index);
        capture.overlay_blocks.sort_by_key(|(index, _)| *index);
        capture.overlay_blocks.dedup_by_key(|(index, _)| *index);
        assert_eq!(
            comparable(effects(&capture, source, true)),
            comparable(effects(&capture, source, false))
        );
    }
    let capture = SongLuaFunctionActionCapture {
        overlay_aux: vec![(4, 1.0), (4, -0.0)],
        overlay_blocks: vec![(4, vec![block(1)]), (4, Vec::new())],
        ..Default::default()
    };
    assert_eq!(
        comparable(effects(&capture, 0, false)),
        vec![(4, Vec::new(), Some((-0.0f32).to_bits()))]
    );
}

#[test]
fn lua_capture_cross_effects_keep_owned_blocks_and_avoid_temporary_tree_churn() {
    let aux = capture(64, "aux");
    let capture = capture(1, "both");
    let original = capture.clone();
    let mut out = effects(&capture, usize::MAX, false);
    out[0].1[0].easing.as_mut().unwrap().push_str(" changed");
    out[0].1[0].delta.x = Some(99.0);
    assert_eq!(capture, original);
    crate::perf::assert_no_churn(|| drop(effects(&capture, 0, false)));
    crate::perf::assert_no_churn(|| {
        drop(effects(&SongLuaFunctionActionCapture::default(), 0, false))
    });
    let size = std::mem::size_of::<(usize, Vec<SongLuaOverlayCommandBlock>, Option<f32>)>();
    crate::perf::assert_churn_budget(1, 64 * size, || drop(effects(&aux, usize::MAX, false)));
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_capture_bench_cross_effects() {
    for (count, kind) in [
        (0, "both"),
        (1, "both"),
        (8, "both"),
        (64, "both"),
        (512, "both"),
        (64, "aux"),
        (512, "aux"),
        (64, "blocks"),
        (64, "disjoint"),
        (64, "empty_blocks"),
        (64, "unordered"),
        (64, "duplicates"),
    ] {
        let capture = capture(count, kind);
        let source = usize::MAX;
        assert_eq!(
            effects(&capture, source, true),
            effects(&capture, source, false)
        );
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("effects_{kind}_{count}/{}", if old { "old" } else { "new" }),
                256,
                (capture.overlay_aux.len() + capture.overlay_blocks.len()).max(1),
                || {
                    drop(black_box(effects(
                        black_box(&capture),
                        source,
                        black_box(old),
                    )))
                },
            );
        }
    }
}
