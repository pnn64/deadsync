use super::*;
use crate::transforms::numeric_performance::allocations as perf;
use std::hint::black_box as bb;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "feedback_original.rs"]
mod original;
fn fingerprint(draws: &[FlatDraw]) -> Vec<u32> {
    let mut out = vec![draws.len() as u32];
    for draw in draws {
        let FlatDraw::Sprite(s) = draw else {
            panic!("feedback fixture unexpectedly emitted non-sprite output")
        };
        assert!(matches!(&s.source, SpriteSource::Solid));
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
    }
    out
}
fn run(which: u8, current: bool, draws: &mut Vec<FlatDraw>, r: &ErrorBarComposeRequest<'_>) {
    match (which, current) {
        (0, false) => original::average(draws, r),
        (0, true) => append_average(draws, r),
        (1, false) => original::long(draws, r),
        (1, true) => append_long_average(draws, r),
        (2, false) => original::compose(draws, *r),
        (2, true) => compose_error_bar(draws, *r),
        _ => unreachable!(),
    }
}
fn values() -> Vec<f32> {
    let mut out = Vec::new();
    for high in (0..=u16::MAX).step_by(31) {
        for low in [0, 1, 0x7fff, 0xffff] {
            out.push(f32::from_bits((u32::from(high) << 16) | low));
        }
    }
    for center in [
        0,
        1,
        0x007fffff,
        0x00800000,
        0.03f32.to_bits(),
        0.5f32.to_bits(),
        1.0f32.to_bits(),
        f32::MAX.to_bits(),
        f32::INFINITY.to_bits(),
        0x7fc00001,
    ] {
        for d in 0..=4 {
            for bits in [center.saturating_sub(d), center.saturating_add(d)] {
                out.extend([f32::from_bits(bits), f32::from_bits(bits | 0x80000000)]);
            }
        }
    }
    let mut seed = 0x73f41287u64;
    for _ in 0..8192 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        out.push(f32::from_bits(seed as u32));
    }
    out
}
fn cases(step: usize, mut visit: impl FnMut(&ErrorBarComposeRequest<'_>)) {
    for (i, f) in values().into_iter().step_by(step).enumerate() {
        let ticks = [Some(ErrorBarTick {
            started_at: 0.0,
            offset_s: if i % 7 == 0 { f } else { 0.01 },
            window: TimingWindow::W3,
        }); 8];
        let count = [0, 1, 8][i % 3];
        for flags in 0..16 {
            let state = ErrorBarState {
                average_ticks: &ticks[..count],
                average_bar_started_at: if flags & 1 != 0 { Some(0.0) } else { None },
                ..empty_state()
            };
            let mut r = request(
                ErrorBarModes {
                    average: flags & 2 != 0,
                    colorful: false,
                    monochrome: false,
                    highlight: false,
                },
                state,
            );
            r.short_average = flags & 4 != 0;
            r.multi_tick = flags & 8 != 0;
            r.long_average_visible = flags & 1 != 0;
            r.long_average_tick = if flags & 2 != 0 { ticks[0] } else { None };
            r.elapsed_s = if i % 3 == 0 { f } else { 0.1 };
            r.mini = f;
            r.long_average_intensity = f;
            r.position = [f, -f];
            r.average_y = f;
            r.max_window_ix = i;
            if i % 4 == 0 {
                r.timing_windows_s = [f; 5];
            }
            if i % 5 == 0 {
                r.style.average_width = f;
            }
            if i % 6 == 0 {
                r.style.colorful_tick_duration = f;
                r.style.long_average_tick_duration = f;
            }
            visit(&r);
        }
    }
}
#[test]
fn feedback_preserves_complete_sprite_bits_and_cold_allocations() {
    cases(1, |r| {
        for which in 0..3 {
            let mut a = Vec::with_capacity(32);
            let mut b = Vec::with_capacity(32);
            run(which, false, &mut a, r);
            run(which, true, &mut b, r);
            assert_eq!(fingerprint(&a), fingerprint(&b), "{which}/{r:?}");
            assert_eq!(a.capacity(), b.capacity());
        }
    });
    cases(257, |r| {
        for cap in [0, 1, 16] {
            for prefix in [false, true] {
                for which in 0..3 {
                    let mut a = Vec::with_capacity(cap);
                    let mut b = Vec::with_capacity(cap);
                    if prefix {
                        for out in [&mut a, &mut b] {
                            append_quad(out, [3.0, 7.0], [1.0, 2.0], [0.5; 4], -3);
                        }
                    }
                    let (_, old) = perf::measure(|| run(which, false, &mut a, r));
                    let (_, new) = perf::measure(|| run(which, true, &mut b, r));
                    assert_eq!(old, new, "churn {cap}/{prefix}/{which}/{r:?}");
                    assert_eq!(fingerprint(&a), fingerprint(&b));
                    assert_eq!(a.capacity(), b.capacity());
                }
            }
        }
    });
}
fn compare(label: &str, which: u8, r: &ErrorBarComposeRequest<'_>) {
    let mut draws = Vec::with_capacity(64);
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
    for (name, mini) in [
        ("zero", 0.0),
        ("negative-zero", -0.0),
        ("one", 1.0),
        ("negative", -0.5),
        ("subnormal", f32::from_bits(1)),
        ("nan", f32::NAN),
        ("large", 4.0),
        ("infinite", f32::INFINITY),
    ] {
        for (current, label) in [(false, "original"), (true, "current")] {
            let (_, c) = perf::measure(|| {
                bb(if current {
                    average_error_bar_mini_scale(bb(mini))
                } else {
                    original::average_error_bar_mini_scale(bb(mini))
                });
            });
            println!("scale/{name}/{label}: allocation {c:?}");
        }
        bench::compare(&format!("scale/{name}"), 100000, |current| {
            bb(if current {
                average_error_bar_mini_scale(bb(mini))
            } else {
                original::average_error_bar_mini_scale(bb(mini))
            });
        });
    }

    let ticks = [Some(ErrorBarTick {
        started_at: 0.75,
        offset_s: 0.01,
        window: TimingWindow::W3,
    }); 8];
    for (name, short, bar, elapsed, empty, scale) in [
        ("off", false, Some(0.75), 1.0, false, 0.02),
        ("missing", true, None, 1.0, false, 0.02),
        ("expired", true, Some(0.0), 1.0, false, 0.02),
        ("future", true, Some(2.0), 1.0, false, 0.02),
        ("active", true, Some(0.75), 1.0, false, 0.02),
        ("empty", true, Some(0.75), 1.0, true, 0.02),
        ("zero-width", true, Some(0.75), 1.0, false, 0.0),
        ("nan-width", true, Some(0.75), 1.0, false, f32::NAN),
    ] {
        let state = ErrorBarState {
            average_ticks: if empty { &[] } else { &ticks },
            average_bar_started_at: bar,
            ..empty_state()
        };
        let mut r = request(
            ErrorBarModes {
                average: true,
                ..ErrorBarModes::default()
            },
            state,
        );
        r.short_average = short;
        r.elapsed_s = elapsed;
        r.timing_windows_s = [scale; 5];
        compare(&format!("average/{name}"), 0, &r);
    }
    for (name, started, visible, present, scale) in [
        ("active", 0.75, true, true, 0.02),
        ("expired", 0.0, true, true, 0.02),
        ("hidden", 0.75, false, true, 0.02),
        ("missing", 0.75, true, false, 0.02),
        ("future", 2.0, true, true, 0.02),
        ("zero-width", 0.75, true, true, 0.0),
    ] {
        let mut r = request(
            ErrorBarModes {
                average: true,
                ..ErrorBarModes::default()
            },
            empty_state(),
        );
        r.long_average_visible = visible;
        r.long_average_tick = present.then_some(ErrorBarTick {
            started_at: started,
            offset_s: 0.01,
            window: TimingWindow::W3,
        });
        r.timing_windows_s = [scale; 5];
        compare(&format!("long/{name}"), 1, &r);
    }
    for (name, short, bar, long) in [
        ("off", false, Some(0.75), None),
        ("missing", true, None, None),
        ("expired", true, Some(0.0), Some(0.0)),
        ("active", true, Some(0.75), Some(0.75)),
        ("long", false, Some(0.75), Some(0.75)),
        ("future", true, Some(2.0), Some(2.0)),
    ] {
        let state = ErrorBarState {
            average_ticks: &ticks,
            average_bar_started_at: bar,
            ..empty_state()
        };
        let mut r = request(
            ErrorBarModes {
                average: true,
                ..ErrorBarModes::default()
            },
            state,
        );
        r.short_average = short;
        r.long_average_visible = true;
        r.long_average_tick = long.map(|started_at| ErrorBarTick {
            started_at,
            offset_s: 0.01,
            window: TimingWindow::W3,
        });
        compare(&format!("caller/{name}"), 2, &r);
    }
}
#[test]
#[ignore = "complete feedback sprite trace"]
fn dump_trace() {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(std::env::var_os("DEADSYNC_FEEDBACK_TRACE_OUT").expect("trace path"))
            .unwrap(),
    );
    cases(17, |r| {
        for which in 0..3 {
            let mut draws = Vec::with_capacity(32);
            run(which, true, &mut draws, r);
            let bits = fingerprint(&draws);
            out.write_all(&(bits.len() as u64).to_le_bytes()).unwrap();
            for n in bits {
                out.write_all(&n.to_le_bytes()).unwrap();
            }
        }
    });
    for f in values() {
        out.write_all(&f.to_bits().to_le_bytes()).unwrap();
        out.write_all(&average_error_bar_mini_scale(f).to_bits().to_le_bytes())
            .unwrap();
    }
    out.flush().unwrap();
}

const NO_TICKS: [Option<ErrorBarTick>; 0] = [];
const NO_FLASHES: [Option<f32>; 6] = [None; 6];
fn style() -> ErrorBarStyle {
    ErrorBarStyle {
        colorful_width: 160.0,
        colorful_height: 10.0,
        colorful_border_size: 4.0,
        average_width: 325.0,
        average_height: 7.0,
        average_tick_padding: 4.0,
        monochrome_width: 240.0,
        monochrome_border_size: 2.0,
        monochrome_center_width: 2.0,
        monochrome_line_width: 1.0,
        tick_width: 2.0,
        colorful_tick_duration: 0.5,
        monochrome_tick_duration: 0.75,
        average_tick_extra_height: 75.0,
        monochrome_background_alpha: 0.5,
        line_alpha: 0.3,
        lines_fade_start: 2.5,
        lines_fade_duration: 0.5,
        label_fade_duration: 0.5,
        label_hold: 2.0,
        label_x_ratio: 0.25,
        label_zoom: 0.7,
        center_tick_width: 1.0,
        highlight_inactive_alpha: 0.3,
        offset_indicator_duration: 0.5,
        offset_indicator_gap: 6.0,
        offset_indicator_zoom: 0.25,
        offset_indicator_shadow_len: 1.0,
        long_average_tick_duration: 0.5,
        long_average_tick_extra_height: 65.0,
        long_average_tick_width: 1.0,
        text_duration: 0.5,
        text_x_offset: 40.0,
        text_zoom: 0.25,
        text_shadow_len: 1.0,
        background_color: [0.0, 0.0, 0.0, 1.0],
        monochrome_center_color: [0.5, 0.5, 0.5, 1.0],
        monochrome_line_color: [1.0, 1.0, 1.0, 1.0],
        label_color: [1.0, 1.0, 1.0, 1.0],
        colorful_tick_color: [0.698, 0.0, 0.0, 1.0],
        average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
        long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
        text_early_color: [0.024, 0.416, 0.957, 1.0],
        text_late_color: [1.0, 0.353, 0.306, 1.0],
        text_scaled_early_color: [0.0, 0.318, 0.859, 1.0],
        text_scaled_late_color: [1.0, 0.086, 0.02, 1.0],
        palette: ErrorBarPalette {
            fantastic_blue: [0.129, 0.8, 0.91, 1.0],
            fa_plus_white: [1.0, 1.0, 1.0, 1.0],
            excellent: [0.886, 0.612, 0.094, 1.0],
            great: [0.4, 0.788, 0.333, 1.0],
            decent: [0.706, 0.361, 1.0, 1.0],
            way_off: [0.788, 0.522, 0.369, 1.0],
        },
        label_font: "game",
        offset_indicator_font: "wendy",
        text_font: "wendy",
        early_label: "Early",
        late_label: "Late",
        front_layers: ErrorBarLayers {
            background: 180,
            band: 181,
            line: 182,
            tick: 183,
            text: 184,
        },
        back_layers: ErrorBarLayers {
            background: 86,
            band: 87,
            line: 88,
            tick: 89,
            text: 90,
        },
        average_z: 88,
    }
}
fn empty_state() -> ErrorBarState<'static> {
    ErrorBarState {
        mono_ticks: &NO_TICKS,
        color_ticks: &NO_TICKS,
        average_ticks: &NO_TICKS,
        color_bar_started_at: None,
        average_bar_started_at: None,
        flash_early: &NO_FLASHES,
        flash_late: &NO_FLASHES,
    }
}
fn offset_text(offset_ms: f32) -> InlineText {
    InlineText::format(format_args!("{offset_ms:.1}"))
        .expect("test offset fits prepared inline text")
}
fn text_label(early: bool, scaled: bool) -> &'static str {
    match (early, scaled) {
        (true, true) => "Fast",
        (true, false) => "Early",
        (false, true) => "Slow",
        (false, false) => "Late",
    }
}
fn request(modes: ErrorBarModes, state: ErrorBarState<'_>) -> ErrorBarComposeRequest<'_> {
    ErrorBarComposeRequest {
        style: style(),
        modes,
        state,
        visible: true,
        elapsed_s: 1.0,
        position: [100.0, 200.0],
        average_y: 150.0,
        max_height: 30.0,
        mini: 0.0,
        timing_windows_s: [0.02, 0.04, 0.08, 0.12, 0.16],
        blue_fantastic_window_s: None,
        max_window_ix: 0,
        show_fa_plus: false,
        judgment_back: false,
        monochrome_background: true,
        multi_tick: false,
        short_average: true,
        center_tick: true,
        has_error_bar: true,
        offset_indicator: None,
        offset_indicator_visible: false,
        offset_indicator_position: [100.0, 150.0],
        offset_text,
        long_average_tick: None,
        long_average_visible: false,
        long_average_intensity: 1.0,
        text: None,
        frame_text_slot: 0,
        text_visible: false,
        text_centered: false,
        text_label,
    }
}

#[test]
fn mini_scale_preserves_full_float_bit_patterns() {
    for high in 0..=u16::MAX {
        for low in [0u32, 1, 0x7fff, 0xffff] {
            let f = f32::from_bits((u32::from(high) << 16) | low);
            assert_eq!(
                original::average_error_bar_mini_scale(f).to_bits(),
                average_error_bar_mini_scale(f).to_bits(),
                "{:08x}",
                f.to_bits()
            );
        }
    }
    let mut seed = 0x73fdea18u64;
    for _ in 0..100000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let f = f32::from_bits(seed as u32);
        assert_eq!(
            original::average_error_bar_mini_scale(f).to_bits(),
            average_error_bar_mini_scale(f).to_bits(),
            "{:08x}",
            f.to_bits()
        );
    }
}
