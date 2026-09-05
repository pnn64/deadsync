use super::*;
use std::hint::black_box;

fn blocks(dense: bool) -> Vec<SongLuaOverlayCommandBlock> {
    vec![SongLuaOverlayCommandBlock {
        start: 0.0,
        duration: 1.0,
        easing: Some("linear".into()),
        opt1: None,
        opt2: None,
        delta: if dense {
            SongLuaOverlayStateDelta {
                x: Some(100.0),
                y: Some(-50.0),
                z: Some(3.0),
                zoom: Some(1.5),
                rot_z_deg: Some(45.0),
                diffuse: Some([0.2, 0.4, 0.6, 0.8]),
                size: Some([100.0, 80.0]),
                vertex_colors: Some([[0.3, 0.5, 0.7, 0.9]; 4]),
                texture_filtering: Some(false),
                depth_test: Some(true),
                ..Default::default()
            }
        } else {
            SongLuaOverlayStateDelta {
                x: Some(100.0),
                ..Default::default()
            }
        },
    }]
}

#[test]
#[ignore = "manual release benchmark"]
fn hot_path_bench() {
    let initial = SongLuaOverlayState {
        x: 7.0,
        size: Some([20.0, 30.0]),
        ..Default::default()
    };
    for dense in [false, true] {
        let blocks = blocks(dense);
        let name = if dense {
            "lua_dense_blocks"
        } else {
            "lua_sparse_blocks"
        };
        crate::perf::measure(name, 1, || {
            black_box(overlay_state_after_blocks(
                black_box(initial),
                black_box(&blocks),
                black_box(0.375),
            ));
        });
    }
}

#[test]
#[ignore = "manual before/after output capture"]
fn hot_path_snapshot() {
    let mut output = String::new();
    for dense in [false, true] {
        let initial = SongLuaOverlayState {
            x: 7.0,
            size: Some([20.0, 30.0]),
            ..Default::default()
        };
        let blocks = blocks(dense);
        for t in [-1.0, 0.0, 0.125, 0.5, 1.0, 2.0, 0.375] {
            output.push_str(&format!(
                "{dense} {t} {:?}\n",
                overlay_state_after_blocks(initial, &blocks, t)
            ));
        }
    }
    std::fs::write(
        std::env::var_os("DEADSYNC_PERF_SNAPSHOT").expect("snapshot path"),
        output,
    )
    .unwrap();
}

#[test]
fn repeated_tweens_do_not_allocate_or_mutate_their_baseline() {
    let initial = SongLuaOverlayState {
        x: 20.0,
        y: 30.0,
        ..Default::default()
    };
    let blocks = blocks(false);
    crate::perf::assert_no_churn(|| {
        for _ in 0..100 {
            assert_eq!(overlay_state_after_blocks(initial, &blocks, 0.5).x, 60.0);
            assert_eq!(overlay_state_after_blocks(initial, &blocks, 0.25).x, 40.0);
        }
    });
    assert_eq!(initial.x, 20.0);
}
