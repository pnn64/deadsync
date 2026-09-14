//! Paired comparisons against the frozen density builder from 0.5.1210.
use deadlib_render_core::MeshVertex;
use deadsync_theme_simply_love::screens::components::shared::density as current;
use std::hint::black_box;
use std::sync::Arc;

#[path = "density_build/baseline.rs"]
#[allow(dead_code)]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

struct Input {
    nps: Vec<f64>,
    seconds: Vec<f32>,
    peak: f64,
    first: f32,
    last: f32,
    width: f32,
    height: f32,
    desaturation: Option<f32>,
    alpha: f32,
}

macro_rules! cache {
    ($module:ident, $input:expr) => {{
        let i = $input;
        $module::build_density_histogram_cache(
            &i.nps,
            i.peak,
            &i.seconds,
            i.first,
            i.last,
            i.width,
            i.height,
            i.desaturation,
            i.alpha,
        )
    }};
}

macro_rules! mesh {
    ($module:ident, $input:expr, $offset:expr, $width:expr) => {{
        let i = $input;
        $module::build_density_histogram_mesh(
            &i.nps,
            i.peak,
            &i.seconds,
            i.first,
            i.last,
            i.width,
            i.height,
            $offset,
            $width,
            i.desaturation,
            i.alpha,
        )
    }};
}

fn fixture(size: usize, shape: &str) -> Input {
    Input {
        nps: (0..size)
            .map(|i| match shape {
                "flat" => 8.0,
                "blocks" => [0.0, 8.0, 12.0, 16.0][(i / 16) % 4],
                _ => ((i * 17) % 31 + 1) as f64,
            })
            .collect(),
        seconds: (0..size).map(|i| i as f32).collect(),
        peak: 32.0,
        first: 0.0,
        last: size.max(1) as f32,
        width: 854.0,
        height: 64.0,
        desaturation: Some(0.5),
        alpha: 0.65,
    }
}

fn bits(value: f32) -> u32 {
    // Floating-point arithmetic may change NaN sign/payload. All other values,
    // including infinities and signed zeros, must match exactly.
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

fn assert_mesh(actual: &[MeshVertex], expected: &[MeshVertex]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a.pos.map(bits), b.pos.map(bits), "position at {index}");
        assert_eq!(a.color.map(bits), b.color.map(bits), "color at {index}");
    }
}

fn compare(input: &Input) {
    let old = cache!(baseline, input);
    let new = cache!(current, input);
    assert_eq!(old.is_some(), new.is_some());
    for (offset, width) in [
        (0.0, 854.0),
        (0.25, 213.5),
        (427.0, 854.0),
        (-12.0, 100.0),
        (854.0, 1.0),
        (0.0, 0.0),
        (0.0, -1.0),
        (f32::NAN, 100.0),
    ] {
        let expected = mesh!(baseline, input, offset, width);
        let actual = mesh!(current, input, offset, width);
        assert_mesh(&actual, &expected);
        if let (Some(old), Some(new)) = (&old, &new) {
            assert_mesh(&new.mesh(offset, width), &old.mesh(offset, width));
            assert_mesh(&actual, &new.mesh(offset, width));
        }
    }
}

#[test]
fn builders_preserve_vertices_for_plateaus_clipping_and_short_time_arrays() {
    for size in [0, 1, 2, 3, 16, 65, 257, 4096] {
        for shape in ["flat", "blocks", "varied"] {
            let mut input = fixture(size, shape);
            compare(&input);
            input.seconds.truncate(size / 2);
            compare(&input);
            input.seconds.clear();
            compare(&input);
        }
    }
    let mut input = fixture(128, "flat");
    // Equal rounded heights must retain the preceding column's original color.
    input.nps = (0..128).map(|i| 8.0 + (i % 3) as f64 * 0.001).collect();
    input.seconds = (0..128).map(|i| (i / 2) as f32).collect();
    compare(&input);
    input.nps.fill(0.0);
    compare(&input);
    input.nps[127] = 1.0;
    compare(&input);
}

#[test]
fn builders_preserve_invalid_dimensions_colors_and_nonfinite_samples() {
    for value in [-1.0, -0.0, 0.0, 0.001, 1.0, 64.0, f32::NAN] {
        for field in 0..7 {
            let mut input = fixture(33, "blocks");
            match field {
                0 => input.width = value,
                1 => input.height = value,
                2 => input.first = value,
                3 => input.last = value,
                4 => input.desaturation = Some(value),
                5 => input.alpha = value,
                _ => input.peak = value as f64,
            }
            compare(&input);
        }
    }
    let mut input = fixture(65, "varied");
    input.desaturation = None;
    input.nps = (0..65)
        .map(|i| {
            [
                -1.0,
                -0.0,
                8.0,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                0.0,
            ][i % 7]
        })
        .collect();
    compare(&input);
    // Reuse keys follow the arithmetic's f32 input, including repeated NaNs
    // and distinct f64 values that convert to the same positive f32.
    let repeated = [
        8.0,
        f64::from_bits(8.0_f64.to_bits() + 1),
        f64::NAN,
        f64::NAN,
        -0.0,
        0.0,
        0.0,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    input.nps = (0..65).map(|i| repeated[i % repeated.len()]).collect();
    compare(&input);
    input.peak = f64::INFINITY;
    compare(&input);
}

#[test]
fn reusable_meshes_match_through_growth_shrink_empty_and_partial_segments() {
    let input = fixture(257, "varied");
    let old_cache = cache!(baseline, &input).unwrap();
    let new_cache = cache!(current, &input).unwrap();
    for initial_len in [0, 1, 5, 6, 7, 11, 36, 4096] {
        let initial = vec![
            MeshVertex {
                pos: [-123.0, 456.0],
                color: [0.125; 4]
            };
            initial_len
        ];
        let mut old = Some(Arc::new(initial.clone()));
        let mut new = Some(Arc::new(initial));
        let mut old_fixed = None;
        let mut new_fixed = None;
        for (offset, width) in [
            (0.0, 30.0),
            (4.25, 213.5),
            (0.0, 854.0),
            (400.0, 3.0),
            (900.0, 50.0),
            (0.0, 854.0),
            (0.0, 0.0),
            (0.0, 100.0),
        ] {
            baseline::update_density_hist_mesh_reusable(&mut old, Some(&old_cache), offset, width);
            current::update_density_hist_mesh_reusable(&mut new, Some(&new_cache), offset, width);
            assert_eq!(old.is_some(), new.is_some());
            if let (Some(old), Some(new)) = (&old, &new) {
                assert_mesh(new, old);
            }
            baseline::update_density_hist_mesh(&mut old_fixed, Some(&old_cache), offset, width);
            current::update_density_hist_mesh(&mut new_fixed, Some(&new_cache), offset, width);
            assert_eq!(old_fixed.is_some(), new_fixed.is_some());
            if let (Some(old), Some(new)) = (&old_fixed, &new_fixed) {
                assert_mesh(new, old);
            }
        }
        current::update_density_hist_mesh_reusable(&mut new, None, 0.0, 854.0);
        assert!(new.is_none());
    }
}

#[test]
fn retained_frames_and_weak_owners_are_not_modified_by_updates() {
    let input = fixture(65, "varied");
    let cache = cache!(current, &input).unwrap();
    let mut mesh = None;
    current::update_density_hist_mesh_reusable(&mut mesh, Some(&cache), 0.0, 854.0);
    let held = Arc::clone(mesh.as_ref().unwrap());
    let expected = held.as_ref().clone();
    let weak = Arc::downgrade(&held);
    for step in 0..50 {
        current::update_density_hist_mesh_reusable(&mut mesh, Some(&cache), step as f32, 200.0);
        assert_mesh(&held, &expected);
        assert_mesh(weak.upgrade().as_ref().unwrap(), &expected);
    }
    drop(held);
    assert!(weak.upgrade().is_none());
    let weak = Arc::downgrade(mesh.as_ref().unwrap());
    current::update_density_hist_mesh_reusable(&mut mesh, Some(&cache), 0.0, 854.0);
    assert!(weak.upgrade().is_none());
    assert_mesh(mesh.as_ref().unwrap(), &cache.mesh(0.0, 854.0));
}

#[test]
fn one_shot_meshes_reduce_churn_and_warmed_updates_have_zero_churn() {
    for size in [257, 4096] {
        for shape in ["flat", "blocks", "varied"] {
            let input = fixture(size, shape);
            perf::assert_reduced_churn(
                || {
                    black_box(mesh!(baseline, black_box(&input), 0.0, 854.0));
                },
                || {
                    black_box(mesh!(current, black_box(&input), 0.0, 854.0));
                },
            );
        }
    }
    let input = fixture(257, "varied");
    let cache = cache!(current, &input).unwrap();
    let mut mesh = None;
    current::update_density_hist_mesh_reusable(&mut mesh, Some(&cache), 0.0, 854.0);
    let capacity = mesh.as_ref().unwrap().capacity();
    perf::assert_no_churn(|| {
        for width in [20.0, 400.0, 854.0, 20.0, 854.0] {
            current::update_density_hist_mesh_reusable(
                black_box(&mut mesh),
                Some(black_box(&cache)),
                0.0,
                width,
            );
        }
    });
    assert_eq!(mesh.as_ref().unwrap().capacity(), capacity);
    assert_mesh(mesh.as_ref().unwrap(), &cache.mesh(0.0, 854.0));
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
fn benchmark_density_build() {
    for size in [0, 1, 64, 4096] {
        for shape in ["flat", "blocks", "varied"] {
            let input = fixture(size, shape);
            let iterations = if size > 64 { 64 } else { 512 };
            pair(
                &format!("one_shot_{shape}_{size}"),
                iterations,
                size.max(1),
                || mesh!(baseline, black_box(&input), 0.0, 854.0),
                || mesh!(current, black_box(&input), 0.0, 854.0),
            );
            pair(
                &format!("cache_{shape}_{size}"),
                iterations,
                size.max(1),
                || cache!(baseline, black_box(&input)),
                || cache!(current, black_box(&input)),
            );
        }
    }
    for size in [64, 4096] {
        let input = fixture(size, "varied");
        let old_cache = cache!(baseline, &input).unwrap();
        let new_cache = cache!(current, &input).unwrap();
        let iterations = if size > 64 { 64 } else { 512 };
        pair(
            &format!("cached_mesh_{size}"),
            iterations,
            size,
            || black_box(&old_cache).mesh(0.0, 854.0),
            || black_box(&new_cache).mesh(0.0, 854.0),
        );
        pair(
            &format!("reusable_cold_{size}"),
            iterations,
            size,
            || {
                let mut mesh = None;
                baseline::update_density_hist_mesh_reusable(
                    &mut mesh,
                    Some(black_box(&old_cache)),
                    0.0,
                    854.0,
                );
                mesh
            },
            || {
                let mut mesh = None;
                current::update_density_hist_mesh_reusable(
                    &mut mesh,
                    Some(black_box(&new_cache)),
                    0.0,
                    854.0,
                );
                mesh
            },
        );
        let mut old = None;
        let mut new = None;
        baseline::update_density_hist_mesh_reusable(&mut old, Some(&old_cache), 0.0, 854.0);
        current::update_density_hist_mesh_reusable(&mut new, Some(&new_cache), 0.0, 854.0);
        pair(
            &format!("reusable_steady_{size}"),
            iterations,
            size,
            || {
                baseline::update_density_hist_mesh_reusable(
                    black_box(&mut old),
                    Some(black_box(&old_cache)),
                    0.0,
                    854.0,
                );
                black_box(&old);
            },
            || {
                current::update_density_hist_mesh_reusable(
                    black_box(&mut new),
                    Some(black_box(&new_cache)),
                    0.0,
                    854.0,
                );
                black_box(&new);
            },
        );
        let widths = [106.75, 427.0, 854.0];
        let segments: usize = widths
            .iter()
            .map(|&w| old_cache.mesh(0.0, w).len() / 6)
            .sum();
        pair(
            &format!("reusable_growing_{size}"),
            iterations,
            segments,
            || {
                for &width in black_box(&widths) {
                    baseline::update_density_hist_mesh_reusable(
                        &mut old,
                        Some(black_box(&old_cache)),
                        0.0,
                        width,
                    );
                }
                black_box(&old);
            },
            || {
                for &width in black_box(&widths) {
                    current::update_density_hist_mesh_reusable(
                        &mut new,
                        Some(black_box(&new_cache)),
                        0.0,
                        width,
                    );
                }
                black_box(&new);
            },
        );
        let old_held = Arc::clone(old.as_ref().unwrap());
        let new_held = Arc::clone(new.as_ref().unwrap());
        pair(
            &format!("reusable_shared_{size}"),
            iterations,
            size,
            || {
                let mut mesh = Some(Arc::clone(&old_held));
                baseline::update_density_hist_mesh_reusable(
                    &mut mesh,
                    Some(black_box(&old_cache)),
                    0.0,
                    854.0,
                );
                mesh
            },
            || {
                let mut mesh = Some(Arc::clone(&new_held));
                current::update_density_hist_mesh_reusable(
                    &mut mesh,
                    Some(black_box(&new_cache)),
                    0.0,
                    854.0,
                );
                mesh
            },
        );
    }
}
