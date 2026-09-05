use super::*;
use std::fmt::Write;
use std::hint::black_box;

mod clipping;
mod reuse;
mod text_mesh;

fn quad() -> Vec<renderer::TexturedMeshVertex> {
    [
        [-4.0, -4.0],
        [4.0, -4.0],
        [4.0, 4.0],
        [-4.0, -4.0],
        [4.0, 4.0],
        [-4.0, 4.0],
    ]
    .map(|p| renderer::TexturedMeshVertex {
        pos: [p[0], p[1], 0.0],
        uv: [(p[0] + 4.0) / 8.0, (p[1] + 4.0) / 8.0],
        color: [0.25, 0.5, 0.75, 1.0],
        ..Default::default()
    })
    .to_vec()
}

fn mesh(vertices: renderer::TexturedMeshVertices, transform: Matrix4) -> EditableDraw {
    EditableDraw {
        object_type: EditablePayload::TexturedMesh {
            instance: renderer::TexturedMeshInstanceRaw::new(
                transform,
                [0.5, 0.75, 1.0, 0.8],
                [0.75, 0.5],
                [0.125, 0.25],
                [0.1, -0.2],
                true,
            ),
            vertices,
            geom_cache_key: 0,
            depth_test: true,
        },
        texture_handle: 17,
        blend: BlendMode::Add,
        z: 3,
        order: 9,
        camera: 0,
    }
}

fn sprite(angle: f32) -> renderer::SpriteInstanceRaw {
    let (s, c) = angle.sin_cos();
    renderer::SpriteInstanceRaw {
        center: [0.0, 0.0, 0.0, 1.0],
        size: [10.0, 10.0],
        rot_sin_cos: [s, c],
        tint: [0.25, 0.5, 0.75, 0.8],
        uv_scale: [-0.75, 0.5],
        uv_offset: [0.875, 0.25],
        local_offset: [0.25, -0.5],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 1.0,
    }
}

fn sprite_draw() -> EditableDraw {
    EditableDraw {
        object_type: EditablePayload::Sprite(0),
        texture_handle: 17,
        blend: BlendMode::Add,
        z: 3,
        order: 9,
        camera: 0,
    }
}

fn rect(left: f32, right: f32, bottom: f32, top: f32) -> WorldRect {
    WorldRect {
        left,
        right,
        bottom,
        top,
    }
}

#[test]
fn rotated_clip_keeps_recycled_storage_and_attributes() {
    let buffer = Vec::with_capacity(18);
    let ptr = buffer.as_ptr();
    let mut pool = vec![buffer];
    let mut sprites = vec![sprite(0.37)];
    let mut obj = sprite_draw();
    assert!(clip_sprite_object_to_world_rect_with_recycled(
        &mut obj,
        &mut sprites,
        rect(-3.0, 3.0, -3.0, 3.0),
        Some(&mut pool)
    ));
    let EditablePayload::TexturedMesh {
        instance,
        vertices,
        geom_cache_key,
        depth_test,
    } = &obj.object_type
    else {
        panic!("rotated clipping produces textured triangles");
    };
    assert_eq!(vertices.as_ptr(), ptr);
    assert!(pool.is_empty());
    assert_eq!(instance.transform(), Matrix4::IDENTITY);
    assert_eq!(instance.tint, sprites[0].tint);
    assert_eq!(instance.texture_mask, 1.0);
    assert_eq!((*geom_cache_key, *depth_test), (0, false));
    assert_eq!(
        (obj.texture_handle, obj.blend, obj.z, obj.order),
        (17, BlendMode::Add, 3, 9)
    );
    assert_eq!(vertices.len() % 3, 0);
    for v in vertices.as_ref() {
        assert!((-3.000001..=3.000001).contains(&v.pos[0]));
        assert!((-3.000001..=3.000001).contains(&v.pos[1]));
        assert_eq!(v.color, [1.0; 4]);
        assert_eq!(v.tex_matrix_scale, [1.0; 2]);
    }
    recycle_transient_object_vertices(obj.object_type, &mut pool);
    assert_eq!(pool[0].as_ptr(), ptr);
}

#[test]
fn partial_clip_recycles_source_but_keeps_output_live() {
    let source = quad();
    let source_ptr = source.as_ptr();
    let output = Vec::with_capacity(48);
    let output_ptr = output.as_ptr();
    let mut pool = vec![output];
    let mut obj = mesh(
        renderer::TexturedMeshVertices::Transient(source),
        Matrix4::IDENTITY,
    );
    assert!(clip_sprite_object_to_world_rect_with_recycled(
        &mut obj,
        &mut Vec::new(),
        rect(-3.0, 3.0, -3.0, 3.0),
        Some(&mut pool)
    ));
    let EditablePayload::TexturedMesh { vertices, .. } = &obj.object_type else {
        panic!("expected mesh")
    };
    assert_eq!(vertices.as_ptr(), output_ptr);
    assert_eq!(pool.len(), 1);
    assert_eq!(pool[0].as_ptr(), source_ptr);
    assert!(pool[0].is_empty());
    assert!(!vertices.is_empty());
}

#[test]
fn contained_and_rejected_meshes_keep_source_ownership() {
    for (clip, expected) in [
        (rect(-5.0, 5.0, -5.0, 5.0), true),
        (rect(10.0, 20.0, 10.0, 20.0), false),
        (rect(0.0, 0.0, -2.0, 2.0), false),
    ] {
        let source = quad();
        let ptr = source.as_ptr();
        let expected_vertices = source.clone();
        let mut obj = mesh(
            renderer::TexturedMeshVertices::Transient(source),
            Matrix4::IDENTITY,
        );
        let mut pool = vec![Vec::with_capacity(18)];
        assert_eq!(
            clip_sprite_object_to_world_rect_with_recycled(
                &mut obj,
                &mut Vec::new(),
                clip,
                Some(&mut pool)
            ),
            expected
        );
        let EditablePayload::TexturedMesh {
            vertices,
            depth_test,
            ..
        } = &obj.object_type
        else {
            panic!("expected mesh")
        };
        assert_eq!(vertices.as_ptr(), ptr);
        assert_eq!(vertices.as_ref(), expected_vertices);
        assert!(*depth_test);
        assert_eq!(pool.len(), 1);
    }
}

#[test]
fn multi_mask_ties_keep_first_candidate_for_all_storage_kinds() {
    let left = rect(-4.0, 0.0, -4.0, 4.0);
    let right = rect(0.0, 4.0, -4.0, 4.0);
    for masks in [[left, right], [right, left]] {
        for vertices in [
            renderer::TexturedMeshVertices::Transient(quad()),
            renderer::TexturedMeshVertices::Shared(Arc::from(quad())),
            renderer::TexturedMeshVertices::Reusable(Arc::new(quad())),
        ] {
            let mut obj = mesh(vertices, Matrix4::IDENTITY);
            let expected = clipped_sprite_object_to_world_rect(&obj, &[], masks[0], None, None)
                .expect("first half clips");
            assert!(clip_object_to_world_masks(
                &mut obj,
                &mut [],
                &masks,
                &mut Vec::new()
            ));
            let (
                EditablePayload::TexturedMesh {
                    vertices: actual,
                    instance: a,
                    ..
                },
                EditablePayload::TexturedMesh {
                    vertices: expected,
                    instance: b,
                    ..
                },
            ) = (&obj.object_type, &expected.object_type)
            else {
                panic!("expected clipped meshes")
            };
            assert_eq!(actual.as_ref(), expected.as_ref());
            assert_eq!(a, b);
        }
    }
}

#[test]
fn recycled_clipping_respects_pool_limit() {
    let mut pool: Vec<_> = (0..MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS)
        .map(|_| Vec::with_capacity(48))
        .collect();
    let mut obj = mesh(
        renderer::TexturedMeshVertices::Transient(quad()),
        Matrix4::IDENTITY,
    );
    assert!(clip_sprite_object_to_world_rect_with_recycled(
        &mut obj,
        &mut Vec::new(),
        rect(-3.0, 3.0, -3.0, 3.0),
        Some(&mut pool)
    ));
    assert_eq!(pool.len(), MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS);
    recycle_transient_object_vertices(obj.object_type, &mut pool);
    assert_eq!(pool.len(), MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS);
}

#[test]
fn warmed_masked_frames_have_no_heap_churn() {
    let metrics = Metrics {
        left: 0.0,
        right: 100.0,
        top: 100.0,
        bottom: 0.0,
    };
    let fonts = fixture_fonts();
    let mut rotated = vec![sprite_actor(0.0, true)];
    rotated.extend((0..64).map(|_| sprite_actor(23.0, false)));
    for actors in [rotated, text_scene(0), text_scene(8)] {
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        let mut frame = || {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                &actors,
                [0.0; 4],
                &metrics,
                &fonts,
                0.25,
                &mut text,
                &mut scratch,
                &Textures,
            );
            assert!(!frame.ops.is_empty());
            scratch.recycle_frame(&mut frame);
        };
        for _ in 0..10 {
            frame();
        }
        crate::HEAP.with(|c| c.set(Some(crate::HeapStats::default())));
        for _ in 0..32 {
            frame();
        }
        let heap = crate::HEAP
            .with(|c| c.replace(None))
            .expect("heap measurement enabled");
        assert_eq!(
            (heap.allocs, heap.reallocs, heap.frees),
            (0, 0, 0),
            "{heap:?}"
        );
    }
}

fn measure(name: &str, units: usize, mut run: impl FnMut()) {
    for _ in 0..100 {
        run();
    }
    const SAMPLES: usize = 41;
    const ITERS: usize = 100;
    let timing_iters = std::env::var("DEADSYNC_BENCH_ITERS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("positive benchmark iterations")
        })
        .unwrap_or(ITERS);
    assert!(timing_iters > 0);
    let mut times = [0.0f64; SAMPLES];
    let mut cycles = [0.0f64; SAMPLES];
    for i in 0..SAMPLES {
        let start = Instant::now();
        let cpu = crate::thread_cycles();
        for _ in 0..timing_iters {
            run();
        }
        cycles[i] = (crate::thread_cycles() - cpu) as f64 / timing_iters as f64;
        times[i] = start.elapsed().as_nanos() as f64 / timing_iters as f64;
    }
    crate::HEAP.with(|c| c.set(Some(crate::HeapStats::default())));
    for _ in 0..ITERS {
        run();
    }
    let heap = crate::HEAP
        .with(|c| c.replace(None))
        .expect("heap measurement enabled");
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    let ns = times[SAMPLES / 2];
    println!(
        "BENCH {name} ns={ns:.2} p95_ns={:.2} max_ns={:.2} cycles={:.2} Munits_s={:.3} allocs={:.2} reallocs={:.2} frees={:.2} alloc_bytes={:.2} freed_bytes={:.2}",
        times[SAMPLES * 95 / 100],
        times[SAMPLES - 1],
        cycles[SAMPLES / 2],
        units as f64 * 1000.0 / ns,
        heap.allocs as f64 / ITERS as f64,
        heap.reallocs as f64 / ITERS as f64,
        heap.frees as f64 / ITERS as f64,
        heap.allocated as f64 / ITERS as f64,
        heap.freed as f64 / ITERS as f64
    );
    if std::env::var_os("DEADSYNC_ASSERT_NO_ALLOC").is_some() {
        assert_eq!(
            (heap.allocs, heap.reallocs, heap.frees),
            (0, 0, 0),
            "{name}: {heap:?}"
        );
    }
}

fn report_pool(name: &str, pool: &[Vec<renderer::TexturedMeshVertex>]) {
    let bytes = pool
        .iter()
        .map(|v| v.capacity() * std::mem::size_of::<renderer::TexturedMeshVertex>())
        .sum::<usize>();
    println!("STORAGE {name} buffers={} vertex_bytes={bytes}", pool.len());
}

#[test]
#[ignore = "release CPU/allocation benchmark; run with --ignored --nocapture --test-threads=1"]
fn masked_render_bench() {
    let clip = rect(-3.0, 3.0, -3.0, 3.0);
    let mut pool = Vec::with_capacity(128);
    let mut sprites = vec![sprite(0.37)];
    measure("rotated_clip", 1, || {
        let mut obj = sprite_draw();
        black_box(clip_sprite_object_to_world_rect_with_recycled(
            &mut obj,
            black_box(&mut sprites),
            black_box(clip),
            Some(&mut pool),
        ));
        black_box(&obj);
        recycle_transient_object_vertices(obj.object_type, &mut pool);
    });
    report_pool("rotated_clip", &pool);

    let vertices = quad().repeat(8);
    let mut pool = Vec::with_capacity(128);
    measure("transient_clip", vertices.len(), || {
        let mut source = take_recycled_text_mesh_vertices(&mut pool);
        source.extend_from_slice(black_box(&vertices));
        let mut obj = mesh(
            renderer::TexturedMeshVertices::Transient(source),
            Matrix4::IDENTITY,
        );
        black_box(clip_sprite_object_to_world_rect_with_recycled(
            &mut obj,
            &mut Vec::new(),
            black_box(clip),
            Some(&mut pool),
        ));
        black_box(&obj);
        recycle_transient_object_vertices(obj.object_type, &mut pool);
    });
    report_pool("transient_clip", &pool);

    let shared: Arc<[renderer::TexturedMeshVertex]> = Arc::from(quad().repeat(256));
    let source = mesh(
        renderer::TexturedMeshVertices::Shared(shared),
        Matrix4::IDENTITY,
    );
    for (name, masks) in [
        ("multi_mask_reject", vec![rect(10.0, 12.0, 10.0, 12.0); 8]),
        (
            "multi_mask_mixed",
            vec![
                rect(10.0, 12.0, 10.0, 12.0),
                rect(-8.0, 8.0, -8.0, 8.0),
                clip,
                rect(10.0, 12.0, 10.0, 12.0),
                rect(-9.0, 9.0, -9.0, 9.0),
                clip,
            ],
        ),
    ] {
        let mut pool = Vec::with_capacity(128);
        measure(name, 1536, || {
            let mut obj = source.clone();
            black_box(clip_object_to_world_masks(
                &mut obj,
                &mut [],
                black_box(&masks),
                &mut pool,
            ));
            black_box(&obj);
            recycle_transient_object_vertices(obj.object_type, &mut pool);
        });
    }
    // Follow the live dispatch: one mask bypasses multi-mask area selection.
    let mut pool = Vec::with_capacity(128);
    measure("single_mask_control", 1536, || {
        let mut obj = source.clone();
        black_box(clip_sprite_object_to_world_rect_with_recycled(
            &mut obj,
            &mut Vec::new(),
            black_box(clip),
            Some(&mut pool),
        ));
        black_box(&obj);
        recycle_transient_object_vertices(obj.object_type, &mut pool);
    });
    composed_bench();
}

struct Textures;
impl TextureContext for Textures {
    fn texture_registry_generation(&self) -> u64 {
        1
    }
    fn texture_dims(&self, _: &str) -> Option<TextureMeta> {
        Some(TextureMeta { w: 16, h: 16 })
    }
    fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
        (1, 1)
    }
    fn texture_handle(&self, _: &str) -> u64 {
        17
    }
}

fn sprite_actor(angle: f32, mask_source: bool) -> actors::Actor {
    actors::Actor::Sprite {
        align: [0.5; 2],
        offset: [50.0; 2],
        world_z: 0.0,
        size: [SizeSpec::Px(if mask_source { 6.0 } else { 10.0 }); 2],
        source: actors::SpriteSource::Solid,
        tint: [1.0; 4],
        glow: [0.0; 4],
        skew: [0.0; 2],
        z: 0,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source,
        mask_dest: !mask_source,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: angle,
        local_offset: [0.0; 2],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.0,
        scale: [1.0; 2],
        shadow_len: [0.0; 2],
        shadow_color: [0.0; 4],
        effect: anim::EffectState::default(),
    }
}

fn composed_bench() {
    let metrics = Metrics {
        left: 0.0,
        right: 100.0,
        top: 100.0,
        bottom: 0.0,
    };
    for (name, angle, masked) in [
        ("compose_rotated_64", 23.0, true),
        ("compose_plain_64", 0.0, false),
    ] {
        let mut actors = vec![sprite_actor(0.0, true)];
        actors.extend((0..64).map(|_| {
            let mut actor = sprite_actor(angle, false);
            if let actors::Actor::Sprite { mask_dest, .. } = &mut actor {
                *mask_dest = masked;
            }
            actor
        }));
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        let fonts = font::FontMap::default();
        measure(name, 64, || {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                black_box(&actors),
                [0.0; 4],
                &metrics,
                &fonts,
                0.25,
                &mut text,
                &mut scratch,
                &Textures,
            );
            black_box(&frame);
            scratch.recycle_frame(&mut frame);
        });
        report_pool(name, &scratch.recycled_text_mesh_vertices);
    }
    let fonts = fixture_fonts();
    for (name, masks) in [
        ("compose_clipped_text_32", 0),
        ("compose_multi_mask_text_32", 8),
    ] {
        let actors = text_scene(masks);
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        measure(name, 32, || {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                black_box(&actors),
                [0.0; 4],
                &metrics,
                &fonts,
                0.25,
                &mut text,
                &mut scratch,
                &Textures,
            );
            black_box(&frame);
            scratch.recycle_frame(&mut frame);
        });
        report_pool(name, &scratch.recycled_text_mesh_vertices);
    }
}

fn fixture_fonts() -> font::FontMap {
    let glyph = font::Glyph {
        texture_key: Arc::from("glyph"),
        stroke_texture_key: None,
        tex_rect: [0.0, 0.0, 8.0, 8.0],
        uv_scale: [0.5; 2],
        uv_offset: [0.0; 2],
        size: [8.0, 10.0],
        offset: [0.0, -10.0],
        advance: 8.0,
        advance_i32: 8,
    };
    let mut ascii = std::array::from_fn(|_| None);
    ascii[b'A' as usize] = Some(glyph.clone());
    font::FontMap::from_iter([(
        "fixture",
        font::Font {
            glyph_map: font::GlyphMap::from_iter([('A', glyph)]),
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 1,
            chain_key: 1,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        },
    )])
}

fn text_scene(masks: usize) -> Vec<actors::Actor> {
    let mut actors: Vec<_> = (0..masks)
        .map(|i| {
            let mut actor = sprite_actor(0.0, true);
            if let actors::Actor::Sprite { offset, size, .. } = &mut actor {
                *offset = if i == 0 { [8.0, 5.0] } else { [90.0, 90.0] };
                *size = [SizeSpec::Px(16.0), SizeSpec::Px(10.0)];
            }
            actor
        })
        .collect();
    actors.extend((0..32).map(|_| actors::Actor::Text {
        align: [0.0; 2],
        offset: [0.0; 2],
        local_transform: Matrix4::IDENTITY,
        color: [0.25, 0.5, 0.75, 0.8],
        stroke_color: None,
        glow: [0.0; 4],
        font: "fixture",
        content: actors::TextContent::Owned("AAAAAAAA".into()),
        attributes: Default::default(),
        align_text: actors::TextAlign::Left,
        z: 0,
        scale: [1.0; 2],
        fit_width: None,
        fit_height: None,
        line_spacing: None,
        wrap_width_pixels: None,
        max_width: None,
        max_height: None,
        max_w_pre_zoom: false,
        max_h_pre_zoom: false,
        jitter: true,
        distortion: 0.0,
        clip: (masks == 0).then_some([0.0, 0.0, 16.0, 10.0]),
        mask_dest: masks != 0,
        blend: BlendMode::Alpha,
        shadow_len: [0.0; 2],
        shadow_color: [0.0; 4],
        effect: anim::EffectState::default(),
    }));
    actors
}

// Explicit fields, float bits, and draw order are recorded outside measurement.
// Run the same binary harness on each implementation and compare the files.
fn record_object(out: &mut String, obj: &EditableDraw, sprites: &[renderer::SpriteInstanceRaw]) {
    writeln!(
        out,
        "{} {:?} {} {} {}",
        obj.texture_handle, obj.blend, obj.z, obj.order, obj.camera
    )
    .unwrap();
    match &obj.object_type {
        EditablePayload::Sprite(i) => writeln!(out, "sprite {:?}", sprites[*i as usize]).unwrap(),
        EditablePayload::TexturedMesh {
            instance,
            vertices,
            geom_cache_key,
            depth_test,
        } => {
            writeln!(out, "mesh {instance:?} {geom_cache_key} {depth_test}").unwrap();
            for v in vertices.as_ref() {
                writeln!(
                    out,
                    "{:?} {:?} {:?} {:?}",
                    v.pos.map(f32::to_bits),
                    v.uv.map(f32::to_bits),
                    v.tex_matrix_scale.map(f32::to_bits),
                    v.color.map(f32::to_bits)
                )
                .unwrap();
            }
        }
        EditablePayload::Mesh { .. } => unreachable!("fixture uses textured meshes and sprites"),
    }
}

#[test]
#[ignore = "writes full old/new outputs to DEADSYNC_RENDER_SNAPSHOT"]
fn masked_render_snapshot() {
    let mut output = String::new();
    let clips = [
        rect(-3.0, 3.0, -3.0, 3.0),
        rect(-10.0, 10.0, -10.0, 10.0),
        rect(20.0, 30.0, 20.0, 30.0),
        rect(0.0, 0.0, -2.0, 2.0),
        rect(-5.0, 5.0, -1.0, 1.0),
    ];
    // Decimal samples deliberately offset from the cardinal rotations.
    for angle in [0.0, 0.37, 1.57, -0.83, 314.0 / 100.0] {
        for clip in clips {
            let mut sprites = vec![sprite(angle)];
            let mut obj = sprite_draw();
            let keep = clip_sprite_object_to_world_rect_with_recycled(
                &mut obj,
                &mut sprites,
                clip,
                Some(&mut Vec::new()),
            );
            writeln!(output, "sprite {angle} {clip:?} {keep}").unwrap();
            if keep {
                record_object(&mut output, &obj, &sprites);
            }
        }
    }
    let transforms = [
        Matrix4::IDENTITY,
        Matrix4::from_rotation_z(0.37),
        Matrix4::from_scale_rotation_translation(
            Vector3::new(-1.5, 0.5, 1.0),
            glam::Quat::IDENTITY,
            Vector3::new(1.0, -2.0, 0.0),
        ),
        Matrix4::from_cols(
            Vector4::new(1.0, 0.0, 0.0, 0.01),
            Vector4::Y,
            Vector4::Z,
            Vector4::W,
        ),
    ];
    for transform in transforms {
        for len in [0, 1, 2, 3, 6, 7, 48] {
            let vertices = quad().repeat(8);
            for clip in clips {
                let mut obj = mesh(
                    renderer::TexturedMeshVertices::Transient(vertices[..len].to_vec()),
                    transform,
                );
                let keep = clip_sprite_object_to_world_rect_with_recycled(
                    &mut obj,
                    &mut Vec::new(),
                    clip,
                    Some(&mut Vec::new()),
                );
                writeln!(output, "single {len} {clip:?} {keep}").unwrap();
                if keep {
                    record_object(&mut output, &obj, &[]);
                }
            }
            for masks in [
                clips.to_vec(),
                clips.iter().rev().copied().collect(),
                vec![clips[0]; 3],
                vec![],
            ] {
                let mut obj = mesh(
                    renderer::TexturedMeshVertices::Shared(Arc::from(&vertices[..len])),
                    transform,
                );
                let keep = clip_object_to_world_masks(&mut obj, &mut [], &masks, &mut Vec::new());
                writeln!(output, "multi {len} {masks:?} {keep}").unwrap();
                if keep {
                    record_object(&mut output, &obj, &[]);
                }
            }
        }
    }
    let metrics = Metrics {
        left: 0.0,
        right: 100.0,
        top: 100.0,
        bottom: 0.0,
    };
    let fonts = fixture_fonts();
    for actors in [
        vec![sprite_actor(0.0, true), sprite_actor(23.0, false)],
        text_scene(0),
        text_scene(8),
    ] {
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        for time in [0.0, 0.25, 0.5] {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                &actors,
                [0.0; 4],
                &metrics,
                &fonts,
                time,
                &mut text,
                &mut scratch,
                &Textures,
            );
            writeln!(
                output,
                "frame {time} {:?} {:?} {:?} {:?} {:?}",
                frame.clear_color,
                frame.cameras,
                frame.ops,
                frame.sprite_instances,
                frame.mesh_vertices
            )
            .unwrap();
            writeln!(output, "instances {:?}", frame.tmesh_instances).unwrap();
            for geometry in &frame.tmesh_geometries {
                writeln!(output, "geometry {}", geometry.cache_key).unwrap();
                for v in geometry.vertices.as_ref() {
                    writeln!(
                        output,
                        "{:?} {:?} {:?} {:?}",
                        v.pos.map(f32::to_bits),
                        v.uv.map(f32::to_bits),
                        v.tex_matrix_scale.map(f32::to_bits),
                        v.color.map(f32::to_bits)
                    )
                    .unwrap();
                }
            }
            scratch.recycle_frame(&mut frame);
        }
    }
    let path = std::env::var_os("DEADSYNC_RENDER_SNAPSHOT").expect("set snapshot output path");
    std::fs::write(path, output).expect("write comparison snapshot");
}
