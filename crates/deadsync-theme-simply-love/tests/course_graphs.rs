//! Frozen-parent comparisons for life sampling and course mesh assembly.
use deadlib_render_core::MeshVertex;
use deadsync_theme_simply_love::{screens::components::shared::density, views};
use std::hint::black_box;
use std::sync::Arc;
use views::CourseGraphStage;

// Supply the same imports when compiling the private production graph module.
mod screens {
    pub mod components {
        pub mod shared {
            pub use deadsync_theme_simply_love::screens::components::shared::density;
        }
    }
}
#[path = "course_graphs/baseline.rs"]
#[allow(dead_code)]
mod baseline;
#[path = "../src/screens/evaluation/graph_build.rs"]
#[allow(dead_code)]
mod current;
#[path = "course_graphs/fixture.rs"]
mod fixture;
#[path = "course_graphs/density_baseline.rs"]
#[allow(dead_code)]
mod old_density;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn stage(size: usize, shape: &str) -> CourseGraphStage {
    let mut stage = fixture::empty_stage(size.max(1) as f32);
    let chart = Arc::make_mut(&mut stage.chart);
    chart.max_nps = 32.0;
    chart.measure_nps_vec = (0..size)
        .map(|i| match shape {
            "flat" => 8.0,
            "blocks" => [0.0, 8.0, 12.0, 16.0][(i / 16) % 4],
            _ => ((i * 17) % 31 + 1) as f64,
        })
        .collect();
    chart.measure_seconds_vec = (0..size).map(|i| i as f32).collect();
    stage
}

fn history(size: usize) -> Vec<(f32, f32)> {
    (0..size)
        .map(|i| {
            (
                (i as f32 - size as f32 * 0.25) * 0.25,
                ((i * 17) % 101) as f32 / 100.0,
            )
        })
        .collect()
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

macro_rules! one_shot {
    ($module:ident, $stage:expr) => {{
        let stage = $stage;
        let chart = &stage.chart;
        $module::build_density_histogram_mesh(
            &chart.measure_nps_vec,
            32.0,
            &chart.measure_seconds_vec,
            chart.first_second,
            stage.song_last_second.max(chart.first_second + 0.001),
            854.0,
            64.0,
            0.0,
            854.0,
            Some(0.5),
            0.65,
        )
    }};
}

fn old_stage(stage: &CourseGraphStage, x: f32) -> Vec<MeshVertex> {
    let mut mesh = one_shot!(old_density, stage);
    for vertex in &mut mesh {
        vertex.pos[0] += x;
    }
    mesh
}

fn append(
    scratch: &mut density::DensityHistScratch,
    out: &mut Vec<MeshVertex>,
    stage: &CourseGraphStage,
    x: f32,
) {
    let chart = &stage.chart;
    scratch.append_mesh(
        out,
        &chart.measure_nps_vec,
        32.0,
        &chart.measure_seconds_vec,
        chart.first_second,
        stage.song_last_second.max(chart.first_second + 0.001),
        854.0,
        64.0,
        x,
        Some(0.5),
        0.65,
    );
}

#[test]
fn prepared_life_samples_preserve_scalar_boundaries_and_nonfinite_records() {
    let mut histories: Vec<_> = [0, 1, 2, 8, 128, 8192].map(history).into();
    histories.extend([
        vec![(-2.0, 0.5), (1.0, 0.2), (6.0, 1.0)],
        vec![(0.0, 1.0), (1.0, 0.5), (1.0, 0.25), (1.000_001, 0.7)],
        vec![(0.0, -0.0), (1.0, f32::NAN), (2.0, f32::INFINITY)],
        vec![(0.0, 0.5), (f32::NAN, 0.3), (f32::INFINITY, -1.0)],
        vec![(3.0, 0.7), (1.0, 0.2), (-3.0, 2.0), (4.0, 0.1)],
    ]);
    let boundaries = [
        f32::NEG_INFINITY,
        -3.0,
        -0.0,
        0.0,
        0.000_001,
        1.0,
        1.000_001,
        2.0,
        6.0,
        3000.0,
        f32::INFINITY,
        f32::NAN,
    ];
    for records in histories {
        for start in boundaries {
            let sampler = current::LifeRecordSampler::new(&records, start);
            for time in boundaries.into_iter().chain(records.iter().map(|p| p.0)) {
                assert_eq!(
                    bits(sampler.sample(time)),
                    bits(baseline::life_record_lerp_at(&records, start, time)),
                    "start {start:?}, sample {time:?}"
                );
            }
        }
    }
}

#[test]
fn complete_life_graphs_preserve_points_and_layout_gates() {
    for size in [0, 1, 8, 128, 8192] {
        let records = history(size);
        for field in 0..5 {
            for value in [-1.0, -0.0, 0.0, 0.001, 1.0, 64.0, f32::NAN, f32::INFINITY] {
                let mut args = [0.0, -2.0, 3000.0, 854.0, 64.0];
                args[field] = value;
                let [start, first, last, width, height] = args;
                let old = baseline::graph_display_life_points(
                    &records, start, first, last, width, height,
                );
                let new =
                    current::graph_display_life_points(&records, start, first, last, width, height);
                assert_eq!(old.is_some(), new.is_some());
                if let (Some(old), Some(new)) = (old, new) {
                    assert_eq!(old.map(|p| p.map(bits)), new.map(|p| p.map(bits)));
                }
            }
        }
    }
}

#[test]
fn scratch_appends_preserve_prefixes_and_match_owning_density_builders() {
    let sentinel = MeshVertex {
        pos: [-0.0, 123.0],
        color: [0.25; 4],
    };
    let mut scratch = density::DensityHistScratch::default();
    let mut old = vec![sentinel];
    let mut new = old.clone();
    for size in [4096, 1, 0, 65, 2, 128, 4096] {
        for shape in ["flat", "blocks", "varied"] {
            let mut input = stage(size, shape);
            for time_len in [size, size / 2, 0] {
                Arc::make_mut(&mut input.chart)
                    .measure_seconds_vec
                    .truncate(time_len);
                assert_mesh(&one_shot!(density, &input), &one_shot!(old_density, &input));
                for x in [-0.0, 123.25, f32::NAN] {
                    old.truncate(1);
                    new.truncate(1);
                    old.extend(old_stage(&input, x));
                    append(&mut scratch, &mut new, &input, x);
                    assert_mesh(&new, &old);
                }
            }
        }
    }
    let mut input = stage(64, "varied");
    let chart = Arc::make_mut(&mut input.chart);
    chart.measure_nps_vec = (0..64)
        .map(|i| [8.0, f64::NAN, f64::INFINITY, -0.0, 0.0][i % 5])
        .collect();
    chart.measure_seconds_vec = (0..64).map(|i| (i / 2) as f32).collect();
    new.clear();
    append(&mut scratch, &mut new, &input, -3.0);
    assert_mesh(&new, &old_stage(&input, -3.0));
}

fn compare_course(stages: &[CourseGraphStage], width: f32, height: f32, rate: f32) {
    let old = baseline::build_course_density_graph_mesh(stages, width, height, rate);
    let new = current::build_course_density_graph_mesh(stages, width, height, rate);
    assert_eq!(old.is_some(), new.is_some());
    if let (Some(old), Some(new)) = (old, new) {
        assert_mesh(&new, &old);
    }
}

#[test]
fn course_meshes_preserve_stage_offsets_peaks_rates_and_empty_stages() {
    for count in [0, 1, 4, 16] {
        let mut stages: Vec<_> = (0..count)
            .map(|i| stage([0, 1, 33, 257][i % 4], ["flat", "blocks", "varied"][i % 3]))
            .collect();
        for (i, stage) in stages.iter_mut().enumerate() {
            Arc::make_mut(&mut stage.chart).max_nps = [8.0, 32.0, f64::NAN, f64::INFINITY][i % 4];
        }
        for field in 0..3 {
            for value in [
                -1.0,
                -0.0,
                0.0,
                0.001,
                0.5,
                1.0,
                1.5,
                854.0,
                f32::NAN,
                f32::INFINITY,
            ] {
                let mut args = [854.0, 64.0, 1.5];
                args[field] = value;
                compare_course(&stages, args[0], args[1], args[2]);
            }
        }
    }
    let mut stages: Vec<_> = (0..8)
        .map(|i| stage(65, ["flat", "blocks", "varied"][i % 3]))
        .collect();
    for (index, stage) in stages.iter_mut().enumerate() {
        stage.song_last_second = [
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            10.0,
            65.0,
            f32::MAX,
            f32::MAX,
        ][index];
        Arc::make_mut(&mut stage.chart).first_second = [-1.0, 0.0, 2.0, f32::NAN][index % 4];
    }
    compare_course(&stages, 854.0, 64.0, f32::MIN_POSITIVE);
    compare_course(&stages[..6], 854.0, 64.0, 1.5);
    for shape in ["flat", "blocks", "varied"] {
        compare_course(&[stage(4096, shape), stage(128, shape)], 854.0, 64.0, 1.5);
    }
}

#[test]
fn column_reuse_reduces_churn_and_warmed_appends_allocate_nothing() {
    let records = history(8192);
    perf::assert_no_churn(|| {
        black_box(current::graph_display_life_points(
            black_box(&records),
            0.0,
            -2.0,
            1800.0,
            854.0,
            64.0,
        ));
    });
    for shape in ["flat", "blocks", "varied"] {
        let input = stage(4096, shape);
        let mut scratch = density::DensityHistScratch::default();
        let mut out = Vec::new();
        append(&mut scratch, &mut out, &input, 0.0);
        perf::assert_reduced_churn(
            || {
                black_box(old_stage(black_box(&input), 0.0));
            },
            || {
                let mut out = Vec::new();
                append(&mut scratch, &mut out, black_box(&input), 0.0);
                black_box(out);
            },
        );
        let capacity = out.capacity();
        perf::assert_no_churn(|| {
            out.clear();
            append(
                black_box(&mut scratch),
                black_box(&mut out),
                black_box(&input),
                0.0,
            );
        });
        assert_eq!(out.capacity(), capacity);
        assert_mesh(&out, &old_stage(&input, 0.0));
        let stages = vec![input; 4];
        perf::assert_reduced_churn(
            || {
                black_box(baseline::build_course_density_graph_mesh(
                    black_box(&stages),
                    854.0,
                    64.0,
                    1.5,
                ));
            },
            || {
                black_box(current::build_course_density_graph_mesh(
                    black_box(&stages),
                    854.0,
                    64.0,
                    1.5,
                ));
            },
        );
    }
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

#[test]
#[ignore = "manual release comparison; --ignored --test-threads=1 --nocapture"]
fn benchmark_course_graphs() {
    for size in [0, 1, 8, 128, 8192, 65536] {
        let records = history(size);
        let last = records.last().map_or(10.0, |r| r.0.max(1.0) + 1.0);
        pair(
            &format!("life_points_{size}"),
            256,
            if size == 0 { 1 } else { 100 },
            || {
                baseline::graph_display_life_points(
                    black_box(&records),
                    0.0,
                    -2.0,
                    last,
                    854.0,
                    64.0,
                )
            },
            || {
                current::graph_display_life_points(
                    black_box(&records),
                    0.0,
                    -2.0,
                    last,
                    854.0,
                    64.0,
                )
            },
        );
    }
    for size in [64, 4096] {
        for shape in ["flat", "blocks", "varied"] {
            let input = stage(size, shape);
            let iterations = if size > 64 { 32 } else { 256 };
            pair(
                &format!("owning_{shape}_{size}"),
                iterations,
                size,
                || one_shot!(old_density, black_box(&input)),
                || one_shot!(density, black_box(&input)),
            );
            let mut scratch = density::DensityHistScratch::default();
            let mut out = Vec::new();
            append(&mut scratch, &mut out, &input, 0.0);
            pair(
                &format!("scratch_{shape}_{size}"),
                iterations,
                size,
                || old_stage(black_box(&input), 0.0),
                || {
                    let mut out = Vec::new();
                    append(black_box(&mut scratch), &mut out, black_box(&input), 0.0);
                    out
                },
            );
            pair(
                &format!("warm_append_{shape}_{size}"),
                iterations,
                size,
                || old_stage(black_box(&input), 0.0),
                || {
                    out.clear();
                    append(black_box(&mut scratch), &mut out, black_box(&input), 0.0);
                    black_box(&out);
                },
            );
        }
    }
    pair(
        "course_empty",
        256,
        1,
        || baseline::build_course_density_graph_mesh(black_box(&[]), 854.0, 64.0, 1.5),
        || current::build_course_density_graph_mesh(black_box(&[]), 854.0, 64.0, 1.5),
    );
    for (count, size) in [(1, 64), (4, 64), (4, 1024), (16, 1024)] {
        for shape in ["flat", "blocks", "varied"] {
            let stages: Vec<_> = (0..count).map(|_| stage(size, shape)).collect();
            let iterations = if count * size > 4096 { 8 } else { 64 };
            pair(
                &format!("course_{count}x{size}_{shape}"),
                iterations,
                count * size,
                || baseline::build_course_density_graph_mesh(black_box(&stages), 854.0, 64.0, 1.5),
                || current::build_course_density_graph_mesh(black_box(&stages), 854.0, 64.0, 1.5),
            );
        }
    }
    let stages: Vec<_> = [0, 2, 64, 1024, 1, 4096, 64, 0, 257]
        .into_iter()
        .enumerate()
        .map(|(i, size)| stage(size, ["flat", "blocks", "varied"][i % 3]))
        .collect();
    let units = stages.iter().map(|s| s.chart.measure_nps_vec.len()).sum();
    pair(
        "course_mixed",
        16,
        units,
        || baseline::build_course_density_graph_mesh(black_box(&stages), 854.0, 64.0, 1.5),
        || current::build_course_density_graph_mesh(black_box(&stages), 854.0, 64.0, 1.5),
    );
}
