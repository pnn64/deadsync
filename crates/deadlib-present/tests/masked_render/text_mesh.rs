//! Attributed/deformed text and colored mesh emission, using the live composer.
use super::*;

fn attributes(count: usize, overlap: bool) -> Vec<actors::TextAttribute> {
    (0..count)
        .map(|i| actors::TextAttribute {
            start: if overlap { 0 } else { (i * 17) % 64 },
            length: if overlap { 64 - i % 33 } else { 2 + i % 5 },
            color: [(i % 7) as f32 / 7.0, 0.5, 0.8, 0.75],
            vertex_colors: (i % 3 == 0).then_some([
                [0.1, 0.2, 0.3, 0.4],
                [0.5, 0.6, 0.7, 0.8],
                [0.9, 0.1, 0.2, 0.3],
                [0.4, 0.5, 0.6, 0.7],
            ]),
            glow: None,
        })
        .collect()
}

fn labels(count: usize, overlap: bool, distortion: f32, jitter: bool) -> Vec<actors::Actor> {
    let attrs: Arc<[actors::TextAttribute]> = Arc::from(attributes(count, overlap));
    let mut actors = text_scene(0);
    for (i, actor) in actors.iter_mut().enumerate() {
        if let actors::Actor::Text {
            content,
            attributes,
            distortion: amount,
            jitter: shake,
            clip,
            offset,
            ..
        } = actor
        {
            *content = actors::TextContent::Owned("A".repeat(64));
            *attributes = Arc::clone(&attrs).into();
            *amount = distortion;
            *shake = jitter;
            *clip = None;
            *offset = [0.0, i as f32 * 12.0];
        }
    }
    actors
}

fn mesh_vertices(count: usize) -> Vec<renderer::MeshVertex> {
    (0..count)
        .map(|i| renderer::MeshVertex {
            pos: [(i % 31) as f32 * 0.125, (i / 31) as f32 * 0.25],
            color: [(i % 7) as f32 * 0.125, 0.5, 0.75, (i % 5) as f32 * 0.25],
        })
        .collect()
}

fn mesh_scene() -> Vec<actors::Actor> {
    let vertices: Arc<[renderer::MeshVertex]> = Arc::from(mesh_vertices(96));
    (0..64)
        .map(|i| actors::Actor::Mesh {
            align: [0.0; 2],
            offset: [(i % 8) as f32 * 8.0, (i / 8) as f32 * 8.0],
            size: [SizeSpec::Px(1.25), SizeSpec::Px(0.75)],
            tint: [0.25, 0.5, 0.75, 0.8],
            vertices: Arc::clone(&vertices),
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        })
        .collect()
}

fn metrics() -> Metrics {
    Metrics {
        left: 0.0,
        right: 800.0,
        top: 600.0,
        bottom: 0.0,
    }
}

fn glyphs(
    builders: &mut Vec<TextMeshBatchBuilder>,
    pool: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    distortion: f32,
    jitter: bool,
) {
    if let Some(batch) = builders.first_mut() {
        batch.vertices.clear();
    }
    for i in 0..128 {
        push_transient_text_mesh_quad(
            builders,
            pool,
            TextPageId::from_index(0),
            i as f32 * 8.0,
            10.0,
            [8.0, 10.0],
            [0.5; 2],
            [0.0; 2],
            [[0.25, 0.5, 0.75, 0.8]; 4],
            jitter.then(|| text_jitter_offset(3, i)),
            black_box(distortion),
            i,
        );
    }
}

#[test]
#[ignore = "release CPU/allocation benchmark"]
fn text_mesh_bench() {
    println!(
        "LAYOUT compose_scratch={}",
        std::mem::size_of::<ComposeScratch>()
    );
    for (name, amount, jitter) in [
        ("glyph_distortion_128", 0.35, true),
        ("glyph_jitter_control_128", 0.0, true),
        ("glyph_plain_control_128", 0.0, false),
    ] {
        let mut builders = Vec::new();
        let mut pool = Vec::new();
        measure(name, 128, || {
            glyphs(&mut builders, &mut pool, amount, jitter);
            black_box(&builders);
        });
    }
    let vertices = mesh_vertices(4096);
    for (name, transform, tint) in [
        (
            "mesh_append_tinted",
            Matrix4::from_rotation_z(0.3),
            [0.25, 0.5, 0.75, 0.8],
        ),
        (
            "mesh_append_white_control",
            Matrix4::from_translation(Vector3::new(12.0, 34.0, 0.0))
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0)),
            [1.0; 4],
        ),
    ] {
        let mut out = Vec::new();
        measure(name, vertices.len(), || {
            out.clear();
            append_mesh_vertices(
                &mut out,
                black_box(&transform),
                black_box(tint),
                black_box(&vertices),
            );
            black_box(&out);
        });
    }
    for (name, count) in [
        ("mesh_append_batches_96", 96),
        ("mesh_append_small_control", 6),
    ] {
        let vertices = mesh_vertices(count);
        let transform = Matrix4::from_translation(Vector3::new(12.0, 34.0, 0.0))
            * Matrix4::from_scale(Vector3::new(1.25, -0.75, 1.0));
        let mut out = Vec::new();
        measure(name, 64 * count, || {
            out.clear();
            for _ in 0..64 {
                append_mesh_vertices(
                    &mut out,
                    black_box(&transform),
                    [0.25, 0.5, 0.75, 0.8],
                    black_box(&vertices),
                );
            }
            black_box(&out);
        });
    }
    let fonts = fixture_fonts();
    for (name, units, actors) in [
        ("compose_attributes_32", 32, labels(32, false, 0.0, false)),
        ("compose_overlap_32", 32, labels(64, true, 0.0, false)),
        ("compose_distortion_32", 32, labels(0, false, 0.35, true)),
        (
            "compose_plain_text_control",
            32,
            labels(0, false, 0.0, false),
        ),
        ("compose_meshes_64", 64, mesh_scene()),
    ] {
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        measure(name, units, || {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                black_box(&actors),
                [0.0; 4],
                &metrics(),
                &fonts,
                0.25,
                &mut text,
                &mut scratch,
                &Textures,
            );
            black_box(&frame);
            scratch.recycle_frame(&mut frame);
        });
        let stats = scratch.storage_stats();
        let attr_capacity = COMPOSE_STORAGE_NAMES
            .iter()
            .position(|&name| name == "text_attributes")
            .map_or(0, |i| {
                stats.capacities[i] as usize * std::mem::size_of::<usize>()
            });
        println!("ATTR_STORAGE {name} bytes={attr_capacity}");
    }
}

#[test]
#[ignore = "writes explicit old/new output to DEADSYNC_RENDER_SNAPSHOT"]
fn text_mesh_snapshot() {
    let mut out = String::new();
    let fonts = fixture_fonts();
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    for count in [0, 1, 8, 9, 32, 64, 129, 3, 0] {
        for overlap in [false, true] {
            for amount in [0.0, 0.35] {
                let mut actors = labels(count, overlap, amount, true);
                actors.truncate(2);
                let mut frame = build_screen_cached_with_scratch_and_texture_context(
                    &actors,
                    [0.0; 4],
                    &metrics(),
                    &fonts,
                    0.25,
                    &mut text,
                    &mut scratch,
                    &Textures,
                );
                reuse::record_frame(&mut out, &frame);
                scratch.recycle_frame(&mut frame);
            }
        }
    }
    for amount in [-0.35, 0.0, 1e-7, 1e-6, 0.35, 1.0, f32::NAN, f32::INFINITY] {
        for jitter in [false, true] {
            let mut builders = Vec::new();
            glyphs(&mut builders, &mut Vec::new(), amount, jitter);
            for v in &builders[0].vertices {
                writeln!(
                    out,
                    "glyph {:?} {:?} {:?} {:?}",
                    v.pos.map(f32::to_bits),
                    v.uv.map(f32::to_bits),
                    v.color.map(f32::to_bits),
                    v.tex_matrix_scale.map(f32::to_bits)
                )
                .expect("write fixture");
            }
        }
    }
    let vertices = mesh_vertices(129);
    for transform in [
        Matrix4::IDENTITY,
        Matrix4::from_rotation_z(0.3),
        Matrix4::from_scale(Vector3::new(1.2, -0.7, 0.5)),
        Matrix4::from_translation(Vector3::new(-5.0, 7.0, 0.0)),
    ] {
        for tint in [[1.0; 4], [0.25, 0.5, 0.75, 0.8], [0.0; 4]] {
            for count in [0, 1, 3, 31, 129] {
                let mut mesh = mesh_vertices(2);
                append_mesh_vertices(&mut mesh, &transform, tint, &vertices[..count]);
                for v in mesh {
                    writeln!(
                        out,
                        "mesh {:?} {:?}",
                        v.pos.map(f32::to_bits),
                        v.color.map(f32::to_bits)
                    )
                    .expect("write fixture");
                }
            }
        }
    }
    let mut frame = build_screen_cached_with_scratch_and_texture_context(
        &mesh_scene(),
        [0.0; 4],
        &metrics(),
        &fonts,
        0.25,
        &mut text,
        &mut scratch,
        &Textures,
    );
    reuse::record_frame(&mut out, &frame);
    scratch.recycle_frame(&mut frame);
    std::fs::write(
        std::env::var_os("DEADSYNC_RENDER_SNAPSHOT").expect("snapshot path"),
        out,
    )
    .expect("write snapshot");
}

#[test]
fn attributed_text_reuses_storage_and_preserves_corner_colors() {
    let fonts = fixture_fonts();
    let actors = labels(64, true, 0.35, true);
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    for _ in 0..3 {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.0; 4],
            &metrics(),
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
        );
        assert_eq!(frame.tmesh_geometries.len(), 32);
        assert_eq!(frame.tmesh_geometries[0].vertices.len(), 64 * 6);
        let vertices = frame.tmesh_geometries[0].vertices.as_ref();
        let expected = attributes(64, true)[63].colors();
        for (i, corner) in [0, 2, 3, 0, 3, 1].into_iter().enumerate() {
            assert_eq!(vertices[i].color, expected[corner]);
        }
        assert_eq!(vertices[0], vertices[3]);
        assert_eq!(vertices[2], vertices[4]);
        scratch.recycle_frame(&mut frame);
    }
    crate::HEAP.with(|cell| cell.set(Some(crate::HeapStats::default())));
    for _ in 0..8 {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.0; 4],
            &metrics(),
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
        );
        scratch.recycle_frame(&mut frame);
    }
    let heap = crate::HEAP
        .with(|cell| cell.replace(None))
        .expect("heap measurement enabled");
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
}

#[test]
fn changing_attribute_overlap_does_not_grow_warmed_storage() {
    let fonts = fixture_fonts();
    let actors = [labels(64, false, 0.0, false), labels(64, true, 0.0, false)];
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    for _ in 0..3 {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            &actors[0],
            [0.0; 4],
            &metrics(),
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
        );
        scratch.recycle_frame(&mut frame);
    }
    crate::HEAP.with(|cell| cell.set(Some(crate::HeapStats::default())));
    for actors in [&actors[1], &actors[0], &actors[1]] {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            actors,
            [0.0; 4],
            &metrics(),
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
        );
        scratch.recycle_frame(&mut frame);
    }
    let heap = crate::HEAP
        .with(|cell| cell.replace(None))
        .expect("heap measurement enabled");
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
}

#[test]
fn first_small_attributes_use_inline_storage() {
    let fonts = fixture_fonts();
    let actors = [labels(0, false, 0.0, true), labels(8, true, 0.0, true)];
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    for _ in 0..3 {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            &actors[0],
            [0.0; 4],
            &metrics(),
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
        );
        scratch.recycle_frame(&mut frame);
    }
    crate::HEAP.with(|cell| cell.set(Some(crate::HeapStats::default())));
    let mut frame = build_screen_cached_with_scratch_and_texture_context(
        &actors[1],
        [0.0; 4],
        &metrics(),
        &fonts,
        0.25,
        &mut text,
        &mut scratch,
        &Textures,
    );
    scratch.recycle_frame(&mut frame);
    let heap = crate::HEAP
        .with(|cell| cell.replace(None))
        .expect("heap measurement enabled");
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
}
