//! Paired behavior and allocation comparisons against the 0.5.1212 builders.
use deadlib_render_core::MeshVertex;
use deadsync_rules::timing::{self, HistogramMs, ScatterFoot, ScatterPoint};
use deadsync_theme::color::JudgmentPalette;
use deadsync_theme_simply_love::{color, screens::components::evaluation::eval_graphs as current};
use std::hint::black_box;
use std::sync::Arc;

#[path = "graph_mesh/baseline.rs"]
#[allow(dead_code)]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/screens/components/evaluation/utils.rs"]
#[allow(dead_code)]
mod utils;

const SCALES: [&str; 6] = ["itg", "ex", "hard", "arrow", "quant", "foot"];

macro_rules! scatter_scale {
    ($module:ident, $name:expr) => {
        match $name {
            "itg" => $module::ScatterPlotScale::Itg,
            "ex" => $module::ScatterPlotScale::Ex,
            "hard" => $module::ScatterPlotScale::HardEx,
            "arrow" => $module::ScatterPlotScale::Arrow,
            "quant" => $module::ScatterPlotScale::Quant,
            "foot" => $module::ScatterPlotScale::FootParity,
            _ => unreachable!(),
        }
    };
}

macro_rules! hist_scale {
    ($module:ident, $name:expr) => {
        match $name {
            "itg" => $module::TimingHistogramScale::Itg,
            "ex" => $module::TimingHistogramScale::Ex,
            "hard" => $module::TimingHistogramScale::HardEx,
            _ => unreachable!(),
        }
    };
}

struct ScatterInput {
    points: Vec<ScatterPoint>,
    first: f32,
    last: f32,
    fail: Option<f32>,
    dim: bool,
    width: f32,
    height: f32,
    worst: f32,
    scale: &'static str,
    palette: JudgmentPalette,
}

fn scatter(size: usize, shape: &str, scale: &'static str) -> ScatterInput {
    ScatterInput {
        points: (0..size)
            .map(|i| ScatterPoint {
                time_sec: i as f32 * 0.025,
                offset_ms: match shape {
                    "miss" => None,
                    "rejected" => Some(250.0),
                    "spread" => Some((i * 37 % 401) as f32 - 200.0),
                    "mixed" if i % 17 == 0 => None,
                    "mixed" => Some((i * 37 % 401) as f32 - 200.0),
                    _ => Some((i * 7 % 31) as f32 - 15.0),
                },
                direction_code: (i % 11) as u8,
                miss_because_held: i % 3 == 0,
                row_index: i,
                quantization_idx: (i % 12) as u8,
                parity_foot: [
                    ScatterFoot::Unknown,
                    ScatterFoot::Left,
                    ScatterFoot::Right,
                    ScatterFoot::Both,
                ][i % 4],
            })
            .collect(),
        first: -0.1,
        last: size.max(1) as f32 * 0.025,
        fail: Some(size as f32 * 0.015),
        dim: true,
        width: 854.0,
        height: 64.0,
        worst: 180.0,
        scale,
        palette: color::SIMPLY_LOVE_JUDGMENT_PALETTE,
    }
}

macro_rules! scatter_mesh {
    ($module:ident, $input:expr) => {{
        let i = $input;
        $module::build_scatter_mesh_with_palette(
            &i.points,
            i.first,
            i.last,
            i.fail,
            i.dim,
            i.width,
            i.height,
            i.worst,
            scatter_scale!($module, i.scale),
            i.palette,
        )
    }};
}

macro_rules! background {
    ($module:ident, $width:expr, $height:expr, $worst:expr, $scale:expr, $palette:expr) => {
        $module::build_scatter_background_mesh_with_palette(
            $width,
            $height,
            $worst,
            scatter_scale!($module, $scale),
            $palette,
        )
    };
}

macro_rules! histogram_mesh {
    ($module:ident, $hist:expr, $dims:expr, $scale:expr, $smooth:expr, $palette:expr) => {{
        let [pw, gh, ph] = $dims;
        $module::build_offset_histogram_mesh_with_palette(
            $hist,
            pw,
            gh,
            ph,
            hist_scale!($module, $scale),
            $smooth,
            $palette,
        )
    }};
}

fn bits(value: f32) -> u32 {
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

fn assert_mesh(actual: &[MeshVertex], expected: &[MeshVertex]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a.pos.map(bits), b.pos.map(bits), "position {index}");
        assert_eq!(a.color.map(bits), b.color.map(bits), "color {index}");
    }
}

fn custom_palette() -> JudgmentPalette {
    let colors = std::array::from_fn(|i| [i as f32 / 7.0, -0.0, 1.0 - i as f32 / 8.0, 0.4]);
    JudgmentPalette::new(colors, colors, colors)
}

#[test]
fn scatter_preserves_clipping_colors_misses_and_nonfinite_boundaries() {
    let mut input = scatter(128, "mixed", "itg");
    let mut offsets = vec![
        None,
        Some(-0.0),
        Some(0.0),
        Some(f32::NAN),
        Some(f32::NEG_INFINITY),
        Some(f32::INFINITY),
    ];
    for bound in timing::effective_windows_ms().into_iter().chain([
        timing::FA_PLUS_W0_MS,
        timing::FA_PLUS_W010_MS,
        180.0,
    ]) {
        for off in [bound.next_down(), bound, bound.next_up()] {
            offsets.extend([Some(off), Some(-off)]);
        }
    }
    for (i, point) in input.points.iter_mut().enumerate() {
        point.offset_ms = offsets[i % offsets.len()];
        point.time_sec = [
            -1.0,
            -0.0,
            0.0,
            1.0,
            3.0,
            f32::NAN,
            f32::NEG_INFINITY,
            f32::INFINITY,
        ][i % 8];
        point.direction_code = [0, 1, 4, 5, 8, 9, 255][i % 7];
    }
    for scale in SCALES {
        input.scale = scale;
        for palette in [color::SIMPLY_LOVE_JUDGMENT_PALETTE, custom_palette()] {
            input.palette = palette;
            for fail in [
                None,
                Some(-0.0),
                Some(1.0),
                Some(f32::NAN),
                Some(f32::INFINITY),
            ] {
                input.fail = fail;
                for dim in [false, true] {
                    input.dim = dim;
                    assert_mesh(
                        &scatter_mesh!(current, &input),
                        &scatter_mesh!(baseline, &input),
                    );
                }
            }
        }
    }
}

#[test]
fn scatter_preserves_layout_gates_and_long_chart_order() {
    for scale in SCALES {
        for size in [
            0, 1, 64, 255, 256, 257, 1023, 1024, 1025, 4095, 4096, 4097, 8192,
        ] {
            for shape in ["hits", "spread", "miss", "rejected", "mixed"] {
                let input = scatter(size, shape, scale);
                assert_mesh(
                    &scatter_mesh!(current, &input),
                    &scatter_mesh!(baseline, &input),
                );
            }
        }
        for field in 0..5 {
            for value in [
                -1.0,
                -0.0,
                0.0,
                0.5,
                1.5,
                f32::NAN,
                f32::NEG_INFINITY,
                f32::INFINITY,
            ] {
                let mut input = scatter(32, "mixed", scale);
                match field {
                    0 => input.width = value,
                    1 => input.height = value,
                    2 => input.worst = value,
                    3 => input.first = value,
                    _ => input.last = value,
                }
                assert_mesh(
                    &scatter_mesh!(current, &input),
                    &scatter_mesh!(baseline, &input),
                );
            }
        }
    }
    for prefix in [0, 1, 4095, 4096, 4097, 8191, 8192] {
        let mut input = scatter(8192, "hits", "hard");
        for point in &mut input.points[..prefix] {
            point.offset_ms = Some(250.0);
        }
        assert_mesh(
            &scatter_mesh!(current, &input),
            &scatter_mesh!(baseline, &input),
        );
    }
}

#[test]
fn background_bands_preserve_every_vertex_and_palette() {
    for scale in SCALES {
        for palette in [color::SIMPLY_LOVE_JUDGMENT_PALETTE, custom_palette()] {
            for field in 0..3 {
                for value in [
                    -1.0,
                    -0.0,
                    0.0,
                    0.5,
                    1.5,
                    10.0,
                    23.0,
                    44.5,
                    180.0,
                    f32::NAN,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                ] {
                    let mut args = [854.0, 64.0, 180.0];
                    args[field] = value;
                    let [w, h, worst] = args;
                    assert_mesh(
                        &background!(current, w, h, worst, scale, palette),
                        &background!(baseline, w, h, worst, scale, palette),
                    );
                }
            }
        }
    }
}

fn histogram(shape: &str) -> HistogramMs {
    let bins: Vec<_> = (-180_i32..=180)
        .filter(|i| match shape {
            "sparse" => i % 41 == 0,
            "center" => *i == 0,
            _ => true,
        })
        .map(|i| (i, ((i * 13).unsigned_abs() % 100) + 1))
        .collect();
    // A full centered smoothing domain, as produced by the timing builder.
    let smoothed = (-180_i32..=180)
        .map(|i| (i, ((i * 7).unsigned_abs() % 100) as f32 + 0.125))
        .collect();
    HistogramMs {
        max_count: 100,
        bins,
        smoothed,
        worst_observed_ms: 180.0,
        worst_window_ms: 180.0,
    }
}

#[test]
fn histogram_segments_preserve_raw_and_smoothed_geometry() {
    for shape in ["center", "sparse", "dense"] {
        let mut hist = histogram(shape);
        for scale in ["itg", "ex", "hard"] {
            for smooth in [false, true] {
                for palette in [color::SIMPLY_LOVE_JUDGMENT_PALETTE, custom_palette()] {
                    for field in 0..3 {
                        for value in [-1.0, -0.0, 0.0, 0.5, 180.0, f32::NAN, f32::INFINITY] {
                            let mut dims = [300.0, 141.0, 180.0];
                            dims[field] = value;
                            assert_mesh(
                                &histogram_mesh!(current, &hist, dims, scale, smooth, palette),
                                &histogram_mesh!(baseline, &hist, dims, scale, smooth, palette),
                            );
                        }
                    }
                }
            }
        }
        for (_, value) in &mut hist.smoothed {
            *value = f32::NAN;
        }
        assert_mesh(
            &histogram_mesh!(
                current,
                &hist,
                [300.0, 141.0, 180.0],
                "itg",
                true,
                custom_palette()
            ),
            &histogram_mesh!(
                baseline,
                &hist,
                [300.0, 141.0, 180.0],
                "itg",
                true,
                custom_palette()
            ),
        );
    }
}

#[test]
fn graph_storage_eliminates_band_scratch_and_bounds_scatter_reservations() {
    let palette = color::SIMPLY_LOVE_JUDGMENT_PALETTE;
    for scale in ["itg", "ex", "hard"] {
        perf::assert_reduced_churn(
            || {
                black_box(background!(baseline, 854.0, 64.0, 180.0, scale, palette));
            },
            || {
                black_box(background!(current, 854.0, 64.0, 180.0, scale, palette));
            },
        );
    }
    for shape in ["hits", "spread", "miss", "rejected", "mixed"] {
        let input = scatter(8192, shape, "hard");
        let old = scatter_mesh!(baseline, &input);
        let current = scatter_mesh!(current, &input);
        assert_mesh(&current, &old);
        assert_eq!(current.capacity(), current.len());
        assert!(current.capacity() <= old.capacity());
        perf::assert_churn_budget(
            usize::from(!current.is_empty()),
            current.len() * size_of::<MeshVertex>(),
            || {
                black_box(scatter_mesh!(current, &input));
            },
        );
    }
    let rejected = scatter(8192, "rejected", "itg");
    perf::assert_no_churn(|| {
        black_box(scatter_mesh!(current, &rejected));
    });
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut old = || perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    let mut new = || perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        new();
        old();
    } else {
        old();
        new();
    }
}

fn retained(mesh: Vec<MeshVertex>) -> Option<Arc<[MeshVertex]>> {
    (!mesh.is_empty()).then(|| Arc::from(mesh.into_boxed_slice()))
}

#[test]
#[ignore = "manual release comparison; --ignored --test-threads=1 --nocapture"]
fn benchmark_graph_mesh() {
    let palette = color::SIMPLY_LOVE_JUDGMENT_PALETTE;
    for scale in ["itg", "ex", "hard", "arrow"] {
        for worst in [10.0, 180.0] {
            pair(
                &format!("bands_{scale}_{}", worst as u32),
                512,
                1,
                || {
                    background!(
                        baseline,
                        black_box(854.0),
                        64.0,
                        worst,
                        scale,
                        black_box(palette)
                    )
                },
                || {
                    background!(
                        current,
                        black_box(854.0),
                        64.0,
                        worst,
                        scale,
                        black_box(palette)
                    )
                },
            );
        }
    }
    for size in [0, 64, 256, 1024, 4096, 8192, 65536] {
        for shape in ["hits", "mixed", "rejected"] {
            for scale in ["itg", "hard"] {
                let input = scatter(size, shape, scale);
                pair(
                    &format!("scatter_{scale}_{shape}_{size}"),
                    if size > 8192 {
                        8
                    } else if size > 64 {
                        32
                    } else {
                        256
                    },
                    size.max(1),
                    || scatter_mesh!(baseline, black_box(&input)),
                    || scatter_mesh!(current, black_box(&input)),
                );
            }
        }
    }
    for scale in ["ex", "arrow", "quant", "foot"] {
        let input = scatter(8192, "mixed", scale);
        pair(
            &format!("scatter_{scale}_mixed_8192"),
            32,
            8192,
            || scatter_mesh!(baseline, black_box(&input)),
            || scatter_mesh!(current, black_box(&input)),
        );
    }
    for scale in ["itg", "hard"] {
        let input = scatter(8192, "miss", scale);
        pair(
            &format!("scatter_{scale}_miss_8192"),
            32,
            8192,
            || scatter_mesh!(baseline, black_box(&input)),
            || scatter_mesh!(current, black_box(&input)),
        );
    }
    for shape in ["hits", "mixed", "rejected"] {
        let input = scatter(8192, shape, "hard");
        pair(
            &format!("retained_hard_{shape}_8192"),
            32,
            8192,
            || retained(scatter_mesh!(baseline, black_box(&input))),
            || retained(scatter_mesh!(current, black_box(&input))),
        );
    }
    for shape in ["center", "sparse", "dense"] {
        let hist = histogram(shape);
        for scale in ["itg", "ex", "hard"] {
            for smooth in [false, true] {
                pair(
                    &format!(
                        "hist_{scale}_{shape}_{}",
                        if smooth { "smooth" } else { "raw" }
                    ),
                    128,
                    hist.bins.len().max(1),
                    || {
                        histogram_mesh!(
                            baseline,
                            black_box(&hist),
                            [300.0, 141.0, 180.0],
                            scale,
                            smooth,
                            black_box(palette)
                        )
                    },
                    || {
                        histogram_mesh!(
                            current,
                            black_box(&hist),
                            [300.0, 141.0, 180.0],
                            scale,
                            smooth,
                            black_box(palette)
                        )
                    },
                );
            }
        }
    }
}
