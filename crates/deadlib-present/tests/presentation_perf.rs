//! Before/after presentation benchmarks; production APIs are exercised unchanged.
use deadlib_present::{color, font, line};
use deadlib_render_core::MeshVertex;
use std::hint::black_box;
use std::sync::Arc;

#[path = "presentation_perf/font_baseline.rs"]
mod old_font;
#[path = "presentation_perf/line_baseline.rs"]
mod old_line;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn points(count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .map(|i| [i as f32 * 0.75, ((i * 7919 % 83) as f32 - 41.0) * 0.125])
        .collect()
}

fn assert_vertices(actual: &[MeshVertex], expected: &[MeshVertex]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            a.pos.map(f32::to_bits),
            b.pos.map(f32::to_bits),
            "position {i}"
        );
        assert_eq!(
            a.color.map(f32::to_bits),
            b.color.map(f32::to_bits),
            "color {i}"
        );
    }
}

fn compare_meshes(points: &[[f32; 2]], offset: f32, width: f32, thickness: f32, feather: f32) {
    let color = [0.0, -0.0, 0.37, 0.625];
    let (mut old, mut new, mut reusable, mut old_reusable) = (None, None, None, None);
    old_line::update_line_mesh(&mut old, points, offset, width, thickness, feather, color);
    line::update_line_mesh(&mut new, points, offset, width, thickness, feather, color);
    old_line::update_line_mesh_reusable(
        &mut old_reusable,
        points,
        offset,
        width,
        thickness,
        feather,
        color,
    );
    line::update_line_mesh_reusable(
        &mut reusable,
        points,
        offset,
        width,
        thickness,
        feather,
        color,
    );
    assert_eq!(new.is_some(), old.is_some());
    assert_eq!(reusable.is_some(), old_reusable.is_some());
    if let (Some(new), Some(old)) = (new, old) {
        assert_vertices(&new, &old);
    }
    if let (Some(new), Some(old)) = (reusable, old_reusable) {
        assert_vertices(&new, &old);
    }
}

#[test]
fn line_geometry_matches_previous_bits_for_clipping_joins_and_degeneracy() {
    let mut cases = vec![
        vec![],
        vec![[0.0, 0.0]],
        vec![[0.0, -0.0], [1.0, 0.0]],
        vec![[0.0, 0.0]; 4],
        vec![[0.0, 0.0], [0.0, 0.0], [1.0, 2.0], [1.0, 2.0]],
        vec![[0.0, 0.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        vec![[0.0, 0.0], [0.000001, 0.0], [1.0, 2.0]],
        vec![[0.0, 0.0], [1.0, 10000.0], [2.0, 0.0]],
    ];
    cases.extend([2, 3, 17, 256].map(points));
    for points in cases {
        for (offset, width) in [
            (-3.0, 200.0),
            (0.0, 0.0),
            (0.25, 0.4),
            (0.25, 8.75),
            (1.5, 17.25),
            (400.0, 20.0),
        ] {
            for (thickness, feather) in [
                (0.0, 0.5),
                (0.25, -1.0),
                (2.0, 0.0),
                (2.0, 0.5),
                (0.1, 8.0),
                (f32::INFINITY, 0.5),
            ] {
                compare_meshes(&points, offset, width, thickness, feather);
            }
        }
    }
    // Nonfinite y values are accepted by the reusable API; preserve NaN bits.
    for y in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        let points = [[0.0, 0.0], [1.0, y], [2.0, 1.0]];
        let (mut old, mut new) = (None, None);
        old_line::update_line_mesh_reusable(&mut old, &points, 0.0, 4.0, 2.0, 0.5, [1.0; 4]);
        line::update_line_mesh_reusable(&mut new, &points, 0.0, 4.0, 2.0, 0.5, [1.0; 4]);
        assert_vertices(new.as_ref().unwrap(), old.as_ref().unwrap());
    }
}

#[test]
fn line_updates_retain_capacity_and_preserve_shared_frames() {
    let points = points(256);
    let (mut old, mut new) = (None, None);
    for (offset, width) in [(0.0, 1000.0), (0.25, 30.0), (1.0, 190.0), (0.0, 10.0)] {
        old_line::update_line_mesh_reusable(&mut old, &points, offset, width, 2.0, 0.5, [1.0; 4]);
        line::update_line_mesh_reusable(&mut new, &points, offset, width, 2.0, 0.5, [1.0; 4]);
        assert_vertices(new.as_ref().unwrap(), old.as_ref().unwrap());
    }
    let ptr = new.as_ref().unwrap().as_ptr();
    perf::assert_no_churn(|| {
        line::update_line_mesh_reusable(&mut new, &points, 0.0, 1000.0, 2.0, 0.5, [1.0; 4])
    });
    assert_eq!(ptr, new.as_ref().unwrap().as_ptr());
    let previous = new.as_ref().unwrap().clone();
    let previous_vertices = previous.to_vec();
    line::update_line_mesh_reusable(&mut new, &points, 0.5, 64.0, 3.0, 1.0, [0.5; 4]);
    assert_vertices(&previous, &previous_vertices);
    assert!(!Arc::ptr_eq(&previous, new.as_ref().unwrap()));
}

#[test]
fn immutable_line_mesh_preserves_unique_and_shared_updates() {
    let points = points(256);
    let mut mesh = None;
    let bytes = (points.len() - 1) * 18 * std::mem::size_of::<MeshVertex>() + 16;
    perf::assert_churn_budget(2, bytes * 2, || {
        line::update_line_mesh(&mut mesh, &points, 0.0, 1000.0, 2.0, 0.5, [1.0; 4])
    });
    let ptr = mesh.as_ref().unwrap().as_ptr();
    perf::assert_no_churn(|| {
        line::update_line_mesh(&mut mesh, &points, 0.0, 1000.0, 2.0, 0.5, [1.0; 4])
    });
    assert_eq!(ptr, mesh.as_ref().unwrap().as_ptr());
    let previous = mesh.as_ref().unwrap().clone();
    let previous_vertices = previous.to_vec();
    perf::assert_churn_budget(2, bytes * 2, || {
        line::update_line_mesh(&mut mesh, &points, 0.0, 1000.0, 3.0, 0.5, [0.5; 4])
    });
    assert_vertices(&previous, &previous_vertices);
    let mut old = None;
    old_line::update_line_mesh(&mut old, &points, 0.0, 1000.0, 3.0, 0.5, [0.5; 4]);
    assert_vertices(mesh.as_ref().unwrap(), old.as_ref().unwrap());
}

fn glyph(advance: i32) -> font::Glyph {
    font::Glyph {
        texture_key: Arc::from("font texture"),
        stroke_texture_key: Some(Arc::from("stroke")),
        tex_rect: [advance as f32; 4],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        size: [8.0, 12.0],
        offset: [0.0, -0.0],
        advance: advance as f32,
        advance_i32: advance,
    }
}

fn fonts(count: usize) -> font::FontMap {
    let names: Vec<&'static str> = (0..count)
        .map(|i| &*Box::leak(format!("font-{i:03}").into_boxed_str()))
        .collect();
    let mut fonts = font::FontMap::default();
    for (i, &name) in names.iter().enumerate() {
        let mut glyph_map = font::GlyphMap::default();
        for code in 0u8..128 {
            if i % 4 == 0 || code as usize % 3 == i % 3 {
                glyph_map.insert(char::from(code), glyph(code as i32 + i as i32));
            }
        }
        glyph_map.insert('\u{65e5}', glyph(128 + i as i32));
        fonts.insert(
            name,
            font::Font {
                glyph_map,
                ascii_glyphs: Box::new(std::array::from_fn(|_| None)),
                default_glyph: (i % 2 == 0).then(|| glyph(-1)),
                line_spacing: 12,
                height: 10,
                fallback_font_name: if i % 4 == 0 { None } else { Some(names[i - 1]) },
                cache_tag: 0,
                chain_key: 0,
                default_stroke_color: [0.0; 4],
                stroke_texture_map: Default::default(),
                texture_hints_map: Default::default(),
            },
        );
    }
    fonts
}

fn assert_fonts(actual: &font::FontMap, expected: &font::FontMap) {
    assert_eq!(actual.len(), expected.len());
    for (name, a) in actual {
        let b = &expected[name];
        assert_eq!(a.cache_tag, b.cache_tag);
        assert_eq!(a.chain_key, b.chain_key);
        // Debug includes every glyph field, map entries are unchanged by refresh.
        assert_eq!(
            format!("{:?}", a.ascii_glyphs),
            format!("{:?}", b.ascii_glyphs)
        );
        for c in "abc \u{65e5}\u{00e9}\u{1f3b5}".chars() {
            assert_eq!(
                format!("{:?}", font::find_glyph(a, c, actual)),
                format!("{:?}", font::find_glyph(b, c, expected))
            );
        }
        assert_eq!(
            font::measure_line_width_logical(a, "A Z\u{65e5}\u{1f3b5}", actual),
            font::measure_line_width_logical(b, "A Z\u{65e5}\u{1f3b5}", expected)
        );
    }
}

#[test]
fn font_refresh_matches_previous_after_fallback_and_source_changes() {
    for count in [0, 1, 12, 32, 33] {
        let mut new = fonts(count);
        let mut old = new.clone();
        for phase in 0..6 {
            for map in [&mut new, &mut old] {
                if let Some(font) = map.get_mut("font-000") {
                    match phase {
                        1 => {
                            font.glyph_map.remove(&'A');
                            font.default_glyph = None;
                        }
                        2 => {
                            font.glyph_map.insert('A', glyph(999));
                            font.cache_tag = u64::MAX;
                        }
                        3 => {
                            font.fallback_font_name = Some("missing-font");
                            font.default_glyph = Some(glyph(77));
                        }
                        4 => {
                            font.glyph_map.clear();
                            font.cache_tag = 0;
                        }
                        _ => {}
                    }
                }
                if phase == 5 {
                    map.remove("font-001");
                }
            }
            font::refresh_chain_keys(&mut new);
            old_font::refresh_chain_keys(&mut old);
            assert_fonts(&new, &old);
        }
    }
}

#[test]
fn font_refresh_retains_ascii_tables_without_churn() {
    for count in [1, 24, 32, 33, 96] {
        let mut fonts = fonts(count);
        font::refresh_chain_keys(&mut fonts);
        let pointers: Vec<_> = fonts
            .iter()
            .map(|(name, font)| (*name, std::ptr::from_ref(font.ascii_glyphs.as_ref())))
            .collect();
        if count <= 32 {
            perf::assert_no_churn(|| font::refresh_chain_keys(&mut fonts));
        } else {
            perf::assert_churn_budget(1, count * std::mem::size_of::<&str>(), || {
                font::refresh_chain_keys(&mut fonts)
            });
        }
        for (name, ptr) in pointers {
            assert_eq!(ptr, std::ptr::from_ref(fonts[name].ascii_glyphs.as_ref()));
        }
    }
}

#[test]
fn growing_and_sparse_lines_match_previous_without_buffer_churn() {
    let mut points = points(4096);
    for sparse in [false, true] {
        if sparse {
            for (i, point) in points.iter_mut().enumerate() {
                *point = [(i / 16) as f32, (i / 16 % 7) as f32];
            }
        }
        let (mut old, mut new) = (None, None);
        for count in [4096, 128, 4096, 3, 2048] {
            old_line::update_line_mesh_reusable(
                &mut old,
                &points[..count],
                0.0,
                10000.0,
                2.0,
                0.5,
                [1.0; 4],
            );
            line::update_line_mesh_reusable(
                &mut new,
                &points[..count],
                0.0,
                10000.0,
                2.0,
                0.5,
                [1.0; 4],
            );
            assert_eq!(new.is_some(), old.is_some());
            if let (Some(new), Some(old)) = (&new, &old) {
                assert_vertices(new, old);
            }
        }
        // Reserve the largest window before checking steady-state updates.
        line::update_line_mesh_reusable(&mut new, &points, 0.0, 10000.0, 2.0, 0.5, [1.0; 4]);
        perf::assert_no_churn(|| {
            line::update_line_mesh_reusable(
                &mut new,
                &points[..128],
                0.0,
                10000.0,
                2.0,
                0.5,
                [1.0; 4],
            );
            line::update_line_mesh_reusable(&mut new, &points, 0.0, 10000.0, 2.0, 0.5, [1.0; 4]);
        });
    }
}

#[test]
fn font_refresh_preserves_resolvable_cycles() {
    let mut new = fonts(4);
    new.get_mut("font-000").unwrap().fallback_font_name = Some("font-003");
    let mut old = new.clone();
    font::refresh_chain_keys(&mut new);
    old_font::refresh_chain_keys(&mut old);
    // Every ASCII code resolves before revisiting a font. Check tables directly;
    // absent non-ASCII characters in cycles do not terminate in either version.
    for (name, actual) in &new {
        assert_eq!(actual.chain_key, old[name].chain_key);
        assert_eq!(
            format!("{:?}", actual.ascii_glyphs),
            format!("{:?}", old[name].ascii_glyphs)
        );
    }
}

#[test]
#[ignore = "release CPU/allocation benchmark; --ignored --nocapture --test-threads=1"]
fn presentation_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for count in [2, 256, 4096] {
        let points = points(count);
        for warm in [true, false] {
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                let label = format!(
                    "line_{}_{}_{}",
                    if warm { "warm" } else { "create" },
                    count,
                    if old { "old" } else { "new" }
                );
                if warm {
                    let update = if old {
                        old_line::update_line_mesh_reusable
                    } else {
                        line::update_line_mesh_reusable
                    };
                    let update = black_box(update);
                    let mut mesh = None;
                    update(&mut mesh, &points, 0.0, 10000.0, 2.0, 0.5, [1.0; 4]);
                    perf::measure_sampled(&label, 200, count - 1, || {
                        update(
                            black_box(&mut mesh),
                            black_box(&points),
                            0.0,
                            10000.0,
                            2.0,
                            0.5,
                            [1.0; 4],
                        );
                        black_box(&mesh);
                    });
                } else {
                    let update = if old {
                        old_line::update_line_mesh_reusable
                    } else {
                        line::update_line_mesh_reusable
                    };
                    let update = black_box(update);
                    perf::measure_sampled(&label, 200, count - 1, || {
                        let mut mesh = None;
                        update(
                            &mut mesh,
                            black_box(&points),
                            0.0,
                            10000.0,
                            2.0,
                            0.5,
                            [1.0; 4],
                        );
                        black_box(mesh)
                    });
                }
            }
        }
    }
    // A retained graph shrinks and then grows, or repeats many identical samples.
    for sparse in [false, true] {
        let points: Vec<_> = points(4096)
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                if sparse {
                    [(i / 16) as f32, (i / 16 % 7) as f32]
                } else {
                    p
                }
            })
            .collect();
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let update = black_box(if old {
                old_line::update_line_mesh_reusable
            } else {
                line::update_line_mesh_reusable
            });
            let mut mesh = None;
            update(&mut mesh, &points, 0.0, 10000.0, 2.0, 0.5, [1.0; 4]);
            perf::measure_sampled(
                &format!(
                    "line_{}_4096_{}",
                    if sparse { "sparse" } else { "regrow" },
                    if old { "old" } else { "new" }
                ),
                200,
                if sparse { 255 } else { 4095 + 127 },
                || {
                    if !sparse {
                        update(
                            black_box(&mut mesh),
                            black_box(&points[..128]),
                            0.0,
                            10000.0,
                            2.0,
                            0.5,
                            [1.0; 4],
                        );
                    }
                    update(
                        black_box(&mut mesh),
                        black_box(&points),
                        0.0,
                        10000.0,
                        2.0,
                        0.5,
                        [1.0; 4],
                    );
                    black_box(&mesh);
                },
            );
        }
    }
    for count in [1, 24, 96] {
        let fixture = fonts(count);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let mut fonts = fixture.clone();
            let refresh = if old {
                old_font::refresh_chain_keys
            } else {
                font::refresh_chain_keys
            };
            let refresh = black_box(refresh);
            refresh(&mut fonts);
            perf::measure_sampled(
                &format!("font_refresh_{count}_{}", if old { "old" } else { "new" }),
                100,
                count * 128,
                || {
                    refresh(black_box(&mut fonts));
                    black_box(&fonts);
                },
            );
        }
    }
}
