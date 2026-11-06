use super::*;
use crate::transforms::numeric_performance::allocations as perf;
use std::hint::black_box as bb;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "combo_original.rs"]
mod original;

fn fingerprint(draws: &[FlatDraw]) -> Vec<u32> {
    let mut out = vec![draws.len() as u32];
    for d in draws {
        match d {
            FlatDraw::Sprite(s) => {
                out.push(0);
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
                    u32::from(s.blend == BlendMode::Add),
                    s.z as u16 as u32,
                ]);
                let SpriteSource::Texture(key) = &s.source else {
                    panic!("expected texture")
                };
                out.push(key.len() as u32);
                out.extend(key.bytes().map(u32::from));
            }
            FlatDraw::PreparedU32(n) => {
                out.push(1);
                out.extend(
                    n.align
                        .into_iter()
                        .chain(n.offset)
                        .chain(n.color)
                        .chain(n.scale)
                        .chain(n.shadow_len)
                        .chain(n.shadow_color)
                        .map(f32::to_bits),
                );
                out.extend([
                    u32::from(n.slot),
                    u32::from(n.align_text == TextAlign::Center),
                    n.z as u16 as u32,
                    u32::from(n.blend == BlendMode::Alpha),
                ]);
                for text in [n.font, n.text.as_str()] {
                    out.push(text.len() as u32);
                    out.extend(text.bytes().map(u32::from));
                }
            }
            _ => panic!("unexpected combo output"),
        }
    }
    out
}
fn run(which: u8, current: bool, out: &mut Vec<FlatDraw>, r: &ComboFeedbackRequest<'_>) {
    match (which, current) {
        (0, false) => original::compose_combo_milestones(out, r),
        (0, true) => compose_combo_milestones(out, r),
        (1, false) => original::compose_combo_number(out, r),
        (1, true) => compose_combo_number(out, r),
        (3, false) => original::append_hundred(
            out,
            r,
            r.milestone_assets.unwrap(),
            r.milestones[0].elapsed,
            1.0,
        ),
        (3, true) => append_hundred(
            out,
            r,
            r.milestone_assets.unwrap(),
            r.milestones[0].elapsed,
            1.0,
        ),
        (2, _) => {
            run(0, current, out, r);
            run(1, current, out, r);
        }
        _ => unreachable!(),
    }
}
fn cases(step: usize, mut visit: impl FnMut(&ComboFeedbackRequest<'_>)) {
    let assets = assets();
    let mut values = vec![
        0.0,
        -0.0,
        0.1,
        0.4,
        0.5,
        0.6,
        0.7,
        0.8,
        1.0,
        -1.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0xffc01234),
        f32::MAX,
        f32::MIN,
        f32::from_bits(1),
    ];
    for center in [0.4f32, 0.5, 0.6, 0.7] {
        for delta in 1..=3 {
            values.extend([
                f32::from_bits(center.to_bits() - delta),
                f32::from_bits(center.to_bits() + delta),
            ]);
        }
    }
    let mut seed = 0x71fc3624u64;
    for _ in 0..2048 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        values.push(f32::from_bits(seed as u32));
    }
    for (i, f) in values.into_iter().step_by(step).enumerate() {
        let milestones = [
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Hundred,
                elapsed: f,
            },
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Thousand,
                elapsed: f,
            },
        ];
        for flags in 0..32 {
            let mut r = request(
                &milestones[..[0, 1, 2][i % 3]],
                if flags & 1 == 0 { Some(&assets) } else { None },
            );
            r.show = flags & 2 == 0;
            r.font = if flags & 4 == 0 {
                Some("combo-font")
            } else {
                None
            };
            r.combo = [0, 3, 4, 100, u32::MAX][i % 5];
            r.miss_combo = [0, 3, 4, u32::MAX][i % 4];
            r.mini = f;
            if flags & 8 != 0 {
                r.style.threshold = [0, 1, 4, u32::MAX][i % 4];
                r.style.number_zoom = f;
                r.number_xy = [f, -f];
                r.milestone_xy = [-f, f];
                r.player_color = [f; 4];
                r.combo_color = [f; 4];
                r.number_text_slot = i as u8;
            }
            if flags & 16 != 0 {
                r.style.burst_duration = f;
                r.style.mini_duration = f;
                r.style.hundred_start_zoom = f;
                r.style.hundred_end_zoom = -f;
                r.style.shadow_len = f;
            }
            visit(&r);
        }
    }
}
#[test]
fn complete_output_bits_match_original() {
    cases(1, |r| {
        for which in 0..3 {
            let mut a = Vec::with_capacity(16);
            let mut b = Vec::with_capacity(16);
            run(which, false, &mut a, r);
            run(which, true, &mut b, r);
            assert_eq!(fingerprint(&a), fingerprint(&b), "which {which}");
            assert_eq!(a.capacity(), b.capacity());
        }
    });
}
#[test]
fn cold_allocations_and_prefix_match_original() {
    cases(131, |r| {
        for which in 0..3 {
            for cap in [0, 1, 16] {
                for prefix in [false, true] {
                    let mut a = Vec::with_capacity(cap);
                    let mut b = Vec::with_capacity(cap);
                    if prefix {
                        let mut p = request(&[], None);
                        p.combo = 100;
                        compose_combo_number(&mut a, &p);
                        compose_combo_number(&mut b, &p);
                    }
                    let (_, old) = perf::measure(|| run(which, false, &mut a, r));
                    let (_, new) = perf::measure(|| run(which, true, &mut b, r));
                    assert_eq!(old, new);
                    assert_eq!(fingerprint(&a), fingerprint(&b));
                    assert_eq!(a.capacity(), b.capacity());
                }
            }
        }
    });
}
fn compare(label: &str, which: u8, r: &ComboFeedbackRequest<'_>) {
    let mut draws = Vec::with_capacity(16);
    for (current, name) in [(false, "original"), (true, "current")] {
        draws.clear();
        let (_, c) = perf::measure(|| run(which, current, &mut draws, r));
        println!("{label}/{name}: allocation {c:?}");
    }
    bench::compare(label, 30000, |current| {
        draws.clear();
        run(which, current, bb(&mut draws), bb(r));
        bb(&draws);
    });
}
#[test]
#[ignore]
fn benchmark() {
    let assets = assets();
    for (name, elapsed) in [("early", 0.1), ("late", 0.55)] {
        let ms = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed,
        }];
        let r = request(&ms, Some(&assets));
        compare(&format!("hundred/{name}"), 3, &r);
    }
    for (mini_name, mini) in [("zero", 0.0), ("positive", 0.5)] {
        let active = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.1,
        }];
        let late = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.55,
        }];
        let thousand = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Thousand,
            elapsed: 0.2,
        }];
        for (name, ms, present, show) in [
            ("missing", active.as_slice(), false, true),
            ("empty", &[][..], true, true),
            ("hidden", active.as_slice(), true, false),
            ("active", active.as_slice(), true, true),
            ("hundred-late", late.as_slice(), true, true),
            ("thousand", thousand.as_slice(), true, true),
        ] {
            let mut r = request(ms, if present { Some(&assets) } else { None });
            r.mini = mini;
            r.show = show;
            compare(&format!("milestones/{name}-{mini_name}"), 0, &r);
        }
        for (name, font, combo, miss, show) in [
            ("missing", None, 100, 0, true),
            ("below", Some("combo-font"), 3, 0, true),
            ("hidden", Some("combo-font"), 100, 0, false),
            ("active", Some("combo-font"), 100, 0, true),
            ("miss", Some("combo-font"), 100, 4, true),
        ] {
            let mut r = request(&[], None);
            r.mini = mini;
            r.font = font;
            r.combo = combo;
            r.miss_combo = miss;
            r.show = show;
            compare(&format!("number/{name}-{mini_name}"), 1, &r);
        }
        let mut r = request(&active, Some(&assets));
        r.mini = mini;
        r.combo = 100;
        compare(&format!("caller/active-{mini_name}"), 2, &r);
    }
}
#[test]
#[ignore]
fn dump_trace() {
    use std::io::Write;
    let path = std::env::var("DEADSYNC_COMBO_TRACE_OUT").expect("trace path");
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    cases(1, |r| {
        for which in 0..3 {
            for current in [false, true] {
                let mut d = Vec::with_capacity(16);
                run(which, current, &mut d, r);
                let words = fingerprint(&d);
                out.write_all(&(words.len() as u32).to_le_bytes()).unwrap();
                for word in words {
                    out.write_all(&word.to_le_bytes()).unwrap();
                }
            }
        }
    });
    out.flush().unwrap();
}

fn style() -> ComboFeedbackStyle {
    ComboFeedbackStyle {
        threshold: 4,
        milestone_z: 89,
        number_z: 90,
        number_zoom: 0.75,
        shadow_len: 1.0,
        miss_color: [1.0, 0.0, 0.0, 1.0],
        burst_duration: 0.5,
        burst_start_zoom: 2.0,
        burst_end_zoom: 1.0,
        burst_start_alpha: 0.5,
        burst_rotation_deg: 90.0,
        hundred_start_zoom: 0.25,
        hundred_end_zoom: 2.0,
        hundred_start_alpha: 0.6,
        hundred_start_rotation_deg: 10.0,
        mini_duration: 0.4,
        mini_start_zoom: 0.25,
        mini_end_zoom: 1.8,
        mini_start_alpha: 1.0,
        mini_start_rotation_deg: 10.0,
        thousand_start_zoom: 0.25,
        thousand_end_zoom: 3.0,
        thousand_start_alpha: 0.7,
        thousand_x_travel: 100.0,
    }
}

fn source(name: &str) -> SpriteSource {
    SpriteSource::Texture(Arc::from(name))
}

fn sprite(name: &str, native_size: [f32; 2], zoom_scale: f32) -> ComboMilestoneSprite {
    ComboMilestoneSprite {
        source: source(name),
        native_size,
        zoom_scale,
    }
}

fn assets() -> ComboMilestoneAssets {
    ComboMilestoneAssets {
        burst: sprite("burst", [10.0, 20.0], 1.0),
        hundred: sprite("hundred", [30.0, 40.0], 2.0),
        hundred_mini: sprite("hundred-mini", [50.0, 60.0], 3.0),
        thousand: sprite("thousand", [70.0, 80.0], 1.5),
    }
}

fn request<'a>(
    milestones: &'a [ActiveComboMilestone],
    assets: Option<&'a ComboMilestoneAssets>,
) -> ComboFeedbackRequest<'a> {
    ComboFeedbackRequest {
        style: style(),
        show: true,
        milestone_assets: assets,
        milestones,
        combo: 0,
        miss_combo: 0,
        number_xy: [310.0, 265.0],
        milestone_xy: [320.0, 265.0],
        mini: 0.0,
        player_color: [0.2, 0.4, 0.8, 0.25],
        combo_color: [0.1, 0.8, 0.3, 0.9],
        font: Some("combo-font"),
        number_text_slot: 0,
    }
}
