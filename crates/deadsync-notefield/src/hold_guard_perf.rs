use super::*;
use crate::transforms::numeric_performance::allocations as perf;
use std::hint::black_box as bb;
use std::sync::Arc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "hold_guard_original.rs"]
mod original;
fn sprite() -> IndicatorSprite {
    IndicatorSprite {
        source: source("hold-guard"),
        frame_size: [64.0, 32.0],
        frame_cols: 1,
        frame_rows: 2,
        scale: 1.0,
    }
}
fn fingerprint(draws: &[FlatDraw]) -> Vec<u32> {
    let mut out = vec![draws.len() as u32];
    for d in draws {
        let FlatDraw::Sprite(s) = d else {
            panic!("expected sprite")
        };
        out.extend(
            s.center
                .into_iter()
                .chain([s.world_z])
                .chain(s.size)
                .chain(s.tint)
                .chain(s.glow)
                .chain(s.uv_rect)
                .chain(s.fade)
                .chain([s.rot_x_deg, s.rot_y_deg, s.rot_z_deg])
                .map(f32::to_bits),
        );
        out.extend([
            u32::from(s.flip_x),
            u32::from(s.flip_y),
            u32::from(s.blend == BlendMode::Alpha),
            s.z as u16 as u32,
        ]);
        let key = s.source.texture_key().unwrap();
        out.push(key.len() as u32);
        out.extend(key.bytes().map(u32::from));
    }
    out
}
fn run(which: u8, current: bool, draws: &mut Vec<FlatDraw>, r: &JudgmentFeedbackRequest<'_>) {
    if which == 1 {
        if r.blind {
            return;
        }
        append_tap_judgment(draws, r);
    }
    if current {
        append_hold_indicators(draws, r)
    } else {
        original::append_hold_indicators(draws, r)
    }
}
fn cases(step: usize, mut visit: impl FnMut(&JudgmentFeedbackRequest<'_>)) {
    let sprite = sprite();
    let offsets = [-96.0; MAX_COLS];
    let invert = [0.0; MAX_COLS];
    let tornado = [TornadoBounds::default(); MAX_COLS];
    let reverse = [0.0; MAX_COLS];
    let mut serial = 0;
    for cols in [0, 1, 4, MAX_COLS, MAX_COLS + 4] {
        for count in [0, 1, 4, MAX_COLS] {
            for assets in 0..4 {
                for pattern in 0..7 {
                    for prepared in [false, true] {
                        for reverse_count in [0, 1, 4, MAX_COLS] {
                            serial += 1;
                            if serial % step != 0 {
                                continue;
                            }
                            let mut misses = [None; MAX_COLS];
                            let mut holds = [None; MAX_COLS];
                            for i in 0..MAX_COLS {
                                if (pattern == 1 && i == 0)
                                    || (pattern == 2 && i == MAX_COLS - 1)
                                    || pattern >= 3
                                {
                                    if pattern != 4 {
                                        misses[i] = Some(HeldMissRenderInfo {
                                            started_at_screen_s: if pattern == 5 {
                                                0.0
                                            } else if pattern == 6 {
                                                f32::NAN
                                            } else {
                                                2.0
                                            },
                                        });
                                    }
                                    if pattern != 3 {
                                        holds[i] = Some(HoldJudgmentRenderInfo {
                                            started_at_screen_s: if pattern == 5 {
                                                0.0
                                            } else if pattern == 6 {
                                                f32::NAN
                                            } else {
                                                2.0
                                            },
                                            result: if i % 2 == 0 {
                                                HoldResult::Held
                                            } else {
                                                HoldResult::LetGo
                                            },
                                        });
                                    }
                                }
                            }
                            let mut r = empty_request(&misses[..count], &holds[..count]);
                            r.num_cols = cols;
                            r.column_reverse_percent = &reverse[..reverse_count];
                            r.held_miss_sprite = if assets & 1 != 0 { Some(&sprite) } else { None };
                            r.hold_sprite = if assets & 2 != 0 { Some(&sprite) } else { None };
                            if pattern == 2 {
                                r.visual.invert = 0.4;
                                r.visual.tornado = 0.5;
                                r.visual.drunk = 0.3;
                            }
                            if prepared {
                                r.prepared_geometry = Some(HoldIndicatorGeometry {
                                    col_offsets: &offsets,
                                    invert: &invert,
                                    tornado: &tornado,
                                    beat_factor: 0.2,
                                });
                            }
                            visit(&r);
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn empty_feedback_guard_preserves_all_sprite_bits_and_order() {
    cases(1, |r| {
        for which in 0..2 {
            let mut a = Vec::with_capacity(64);
            let mut b = Vec::with_capacity(64);
            run(which, false, &mut a, r);
            run(which, true, &mut b, r);
            assert_eq!(fingerprint(&a), fingerprint(&b));
            assert_eq!(a.capacity(), b.capacity());
        }
    });
}
#[test]
fn feedback_guard_preserves_cold_allocations_and_existing_prefix() {
    cases(53, |r| {
        for cap in [0, 1, 64] {
            for prefix in [false, true] {
                let mut a = Vec::with_capacity(cap);
                let mut b = Vec::with_capacity(cap);
                if prefix {
                    for out in [&mut a, &mut b] {
                        append_sprite(
                            out,
                            SpriteSource::Solid,
                            [1.0, 2.0],
                            [3.0, 4.0],
                            7,
                            0.0,
                            [0.0, 0.0, 1.0, 1.0],
                            [1.0; 4],
                        );
                    }
                }
                let (_, old) = perf::measure(|| run(0, false, &mut a, r));
                let (_, new) = perf::measure(|| run(0, true, &mut b, r));
                assert_eq!(old, new);
                assert_eq!(a.capacity(), b.capacity());
                if prefix {
                    a.remove(0);
                    b.remove(0);
                }
                assert_eq!(fingerprint(&a), fingerprint(&b));
            }
        }
    });
}
fn compare(name: &str, which: u8, r: &JudgmentFeedbackRequest<'_>) {
    let mut draws = Vec::with_capacity(64);
    for (current, label) in [(false, "original"), (true, "current")] {
        draws.clear();
        let (_, c) = perf::measure(|| run(which, current, &mut draws, r));
        println!("{name}/{label}: allocation {c:?}");
    }
    bench::compare(name, 10000, |current| {
        draws.clear();
        run(which, current, bb(&mut draws), bb(r));
        bb(&draws);
    });
}
#[test]
#[ignore]
fn benchmark() {
    let sprite = sprite();
    let offsets = [-96.0; MAX_COLS];
    let invert = [0.0; MAX_COLS];
    let tornado = [TornadoBounds::default(); MAX_COLS];
    let reverse = [0.0; MAX_COLS];
    for (name, cols, prepared, active, expired, asset_bits, effects) in [
        ("empty", 4, false, false, false, 3, false),
        ("empty-wide", MAX_COLS, false, false, false, 3, false),
        ("empty-effects", 4, false, false, false, 3, true),
        ("active", 4, false, true, false, 3, true),
        ("late", MAX_COLS, false, true, false, 3, false),
        ("expired", 4, false, true, true, 3, false),
        ("missing-assets", 4, false, true, false, 0, false),
        ("zero-cols", 0, false, false, false, 3, false),
        ("prepared-empty", 4, true, false, false, 3, false),
        ("prepared-active", 4, true, true, false, 3, false),
        ("prepared-expired", 4, true, true, true, 3, false),
        ("unmatched", 4, false, true, false, 2, false),
    ] {
        let mut misses = [None; MAX_COLS];
        let mut holds = [None; MAX_COLS];
        if active {
            let i = if name == "late" { MAX_COLS - 1 } else { 0 };
            misses[i] = Some(HeldMissRenderInfo {
                started_at_screen_s: 2.0,
            });
            if name != "unmatched" {
                holds[i] = Some(HoldJudgmentRenderInfo {
                    started_at_screen_s: 2.0,
                    result: HoldResult::Held,
                });
            }
        }
        let mut r = empty_request(&misses, &holds);
        r.num_cols = cols;
        r.column_reverse_percent = &reverse;
        r.held_miss_sprite = if asset_bits & 1 != 0 {
            Some(&sprite)
        } else {
            None
        };
        r.hold_sprite = if asset_bits & 2 != 0 {
            Some(&sprite)
        } else {
            None
        };
        if expired {
            r.elapsed_screen = 10.0;
        }
        if effects {
            r.visual.invert = 0.4;
            r.visual.tornado = 0.5;
            r.visual.drunk = 0.3;
        }
        if prepared {
            r.prepared_geometry = Some(HoldIndicatorGeometry {
                col_offsets: &offsets,
                invert: &invert,
                tornado: &tornado,
                beat_factor: 0.2,
            });
        }
        compare(&format!("holds/{name}"), 0, &r);
        if name == "empty" || name == "active" {
            compare(&format!("hold-caller/{name}"), 1, &r);
        }
    }
}
#[test]
#[ignore]
fn dump_trace() {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(std::env::var("DEADSYNC_HOLD_GUARD_TRACE_OUT").unwrap()).unwrap(),
    );
    cases(1, |r| {
        for which in 0..2 {
            for current in [false, true] {
                let mut d = Vec::with_capacity(64);
                run(which, current, &mut d, r);
                let words = fingerprint(&d);
                out.write_all(&(words.len() as u32).to_le_bytes()).unwrap();
                for w in words {
                    out.write_all(&w.to_le_bytes()).unwrap();
                }
            }
        }
    });
    out.flush().unwrap();
}

fn style() -> JudgmentFeedbackStyle {
    JudgmentFeedbackStyle {
        tap_front_z: 200,
        tap_back_z: 95,
        split_overlay_alpha: 0.5,
        held_miss_normal_y: -50.0,
        held_miss_reverse_y: 110.0,
        held_miss_z: 196,
        hold_normal_y: -90.0,
        hold_reverse_y: 90.0,
        hold_z: 195,
        hold_initial_zoom: 25.6 / 140.0,
        hold_final_zoom: 32.0 / 140.0,
    }
}

fn source(name: &str) -> SpriteSource {
    SpriteSource::Texture(Arc::from(name))
}

fn empty_request<'a>(
    held_misses: &'a [Option<HeldMissRenderInfo>],
    hold_judgments: &'a [Option<HoldJudgmentRenderInfo>],
) -> JudgmentFeedbackRequest<'a> {
    JudgmentFeedbackRequest {
        style: style(),
        blind: false,
        elapsed_screen: 2.2,
        tap: None,
        tap_sprite: None,
        tap_xy: [320.0, 150.0],
        judgment_back: false,
        judgment_zoom: 1.0,
        judgment_animation: TapJudgmentAnimation::Default,
        held_misses,
        held_miss_sprite: None,
        hold_judgments,
        hold_sprite: None,
        current_beat: 4.0,
        arrow_effect_time: 10.0,
        mini: 0.0,
        visual: VisualEffects::default(),
        noteskin_column_xs: Some(&[-96, -32, 32, 96]),
        prepared_geometry: None,
        num_cols: 4,
        spacing_multiplier: 1.0,
        field_zoom: 1.0,
        playfield_center_x: 320.0,
        screen_center_y: 240.0,
        screen_height: 480.0,
        field_center_y: 5.0,
        column_reverse_percent: &[0.0, 1.0, 0.0, 1.0],
    }
}
