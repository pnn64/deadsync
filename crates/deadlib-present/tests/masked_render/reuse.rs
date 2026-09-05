//! Second render pass: source resolution, offscreen passes, and sprite fades.
use super::*;

#[test]
fn target_passes_keep_postorder_and_reuse_capacity() {
    let metrics = Metrics {
        left: -50.0,
        right: 50.0,
        top: 50.0,
        bottom: -50.0,
    };
    let fonts = font::FontMap::default();
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    let actors = target_scene(16, true);
    for _ in 0..3 {
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
        assert!(frame.ops.is_empty());
        assert_eq!(frame.render_targets.len(), 16);
        for (index, target) in frame.render_targets.iter().enumerate() {
            assert_eq!(
                target.texture_handle,
                renderer::render_target_texture_handle(index as u64 + 1)
            );
            assert_eq!(
                (target.width, target.height),
                (64 + index as u32, 32 + index as u32)
            );
            assert_eq!(
                (target.alpha, target.depth, target.preserve),
                (index % 2 == 0, index % 3 == 0, index % 4 == 0)
            );
            assert_eq!(target.sprite_instances.len(), 1);
            assert_eq!(target.ops.len(), 1);
        }
        scratch.recycle_frame(&mut frame);
    }
    crate::HEAP.with(|c| c.set(Some(crate::HeapStats::default())));
    for _ in 0..32 {
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
        scratch.recycle_frame(&mut frame);
    }
    let heap = crate::HEAP
        .with(|c| c.replace(None))
        .expect("heap measurement enabled");
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
}

#[test]
fn sprite_fade_handles_crop_cancellation_and_flips() {
    let metrics = Metrics {
        left: 0.0,
        right: 100.0,
        top: 100.0,
        bottom: 0.0,
    };
    for (fade, expected) in [
        ([0.0; 4], [0.0; 4]),
        ([-0.5; 4], [0.0; 4]),
        ([0.5, 0.25, 0.0, 0.0], [0.25, 0.25, 0.0, 0.0]),
    ] {
        let mut actors = fade_scene(fade);
        actors.truncate(1);
        if let actors::Actor::Sprite {
            cropleft,
            cropright,
            flip_x,
            ..
        } = &mut actors[0]
        {
            *cropleft = -0.25;
            *cropright = 0.0;
            *flip_x = true;
        }
        let frame = build_screen_with_texture_context(
            &actors,
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            0.0,
            &Textures,
        );
        assert_eq!(frame.sprite_instances[0].edge_fade, expected);
    }
}

fn geometry_scene(
    stride: usize,
) -> (
    Vec<renderer::TexturedMeshGeometry>,
    renderer::DenseSlotMap<u32>,
) {
    let vertices: Arc<[renderer::TexturedMeshVertex]> = Arc::from(quad());
    let mut cache = renderer::DenseSlotMap::with_capacity(1024);
    let geometries = (0..1024)
        .map(|i| {
            let key = (i + 1) as u64;
            cache.insert(key, vertices.len() as u32);
            renderer::TexturedMeshGeometry {
                vertices: renderer::TexturedMeshVertices::Shared(Arc::clone(&vertices)),
                cache_key: if stride != 0 && i % stride == 0 {
                    0
                } else {
                    key
                },
            }
        })
        .collect();
    (geometries, cache)
}

fn target_scene(count: usize, nested: bool) -> Vec<actors::Actor> {
    let mut actors = Vec::with_capacity(count);
    for i in 0..count {
        let mut child = sprite_actor(0.0, false);
        if let actors::Actor::Sprite { mask_dest, .. } = &mut child {
            *mask_dest = false;
        }
        let mut children = vec![child];
        if nested && let Some(prior) = actors.pop() {
            children.push(prior);
        }
        actors.push(actors::Actor::RenderTarget {
            texture_handle: renderer::render_target_texture_handle((i + 1) as u64),
            size: [64 + i as u32, 32 + i as u32],
            logical_size: [100.0; 2],
            alpha: i % 2 == 0,
            depth: i % 3 == 0,
            preserve: i % 4 == 0,
            children: Arc::from(children),
        });
    }
    actors
}

fn fade_scene(fade: [f32; 4]) -> Vec<actors::Actor> {
    (0..512)
        .map(|i| {
            let mut actor = sprite_actor(if i % 2 == 0 { 0.0 } else { 23.0 }, false);
            if let actors::Actor::Sprite {
                mask_dest,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                offset,
                source,
                cropleft,
                cropright,
                ..
            } = &mut actor
            {
                *mask_dest = false;
                *source = actors::SpriteSource::static_texture("fixture");
                [*fadeleft, *faderight, *fadetop, *fadebottom] = fade;
                *offset = [(i % 32) as f32 * 3.0, (i / 32) as f32 * 4.0];
                *cropleft = if i % 3 == 0 { 0.25 } else { 0.0 };
                *cropright = if i % 7 == 0 { -0.25 } else { 0.0 };
            }
            actor
        })
        .collect()
}

#[test]
#[ignore = "release CPU/allocation benchmark"]
fn render_reuse_bench() {
    println!(
        "LAYOUT textured_mesh_uploads={}",
        std::mem::size_of::<renderer::TexturedMeshUploads>()
    );
    for (name, stride) in [
        ("resolve_mixed_sparse", 128),
        ("resolve_mixed_half", 2),
        ("resolve_cached_control", 0),
        ("resolve_transient_control", 1),
    ] {
        let (geometries, cache) = geometry_scene(stride);
        let mut uploads = renderer::TexturedMeshUploads::default();
        measure(name, geometries.len(), || {
            renderer::resolve_textured_mesh_geometries(
                black_box(&geometries),
                &mut uploads,
                |key, vertices| {
                    cache
                        .get(key)
                        .and_then(|(slot, &len)| (vertices.len() == len as usize).then_some(slot))
                },
            );
            black_box(&uploads);
        });
    }
    let metrics = Metrics {
        left: 0.0,
        right: 100.0,
        top: 100.0,
        bottom: 0.0,
    };
    let fonts = font::FontMap::default();
    for (name, units, actors) in [
        ("compose_targets_16", 16, target_scene(16, false)),
        ("compose_targets_nested", 16, target_scene(16, true)),
        ("compose_targets_control", 2, target_scene(2, false)),
        ("compose_no_fade", 512, fade_scene([0.0; 4])),
        (
            "compose_fade_control",
            512,
            fade_scene([0.2, 0.4, 0.1, 0.3]),
        ),
    ] {
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        measure(name, units, || {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                black_box(&actors),
                [0.1, 0.2, 0.3, 1.0],
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
    }
}

fn record_frame(out: &mut String, frame: &RenderFrame) {
    writeln!(
        out,
        "frame {:?} {:?} {:?} {:?} {:?} {:?}",
        frame.clear_color,
        frame.cameras,
        frame.sprite_instances,
        frame.mesh_vertices,
        frame.tmesh_instances,
        frame.ops
    )
    .unwrap();
    for geometry in &frame.tmesh_geometries {
        writeln!(
            out,
            "geom {} {:?}",
            geometry.cache_key,
            geometry.vertices.as_ref()
        )
        .unwrap();
    }
    for target in &frame.render_targets {
        writeln!(
            out,
            "target {} {} {} {} {} {} {:?} {:?} {:?} {:?} {:?}",
            target.texture_handle,
            target.width,
            target.height,
            target.alpha,
            target.depth,
            target.preserve,
            target.cameras,
            target.sprite_instances,
            target.mesh_vertices,
            target.tmesh_instances,
            target.ops
        )
        .unwrap();
        for geometry in &target.tmesh_geometries {
            writeln!(
                out,
                "geom {} {:?}",
                geometry.cache_key,
                geometry.vertices.as_ref()
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore = "writes explicit old/new render outputs to DEADSYNC_RENDER_SNAPSHOT"]
fn render_reuse_snapshot() {
    let mut out = String::new();
    let metrics = Metrics {
        left: -50.0,
        right: 50.0,
        top: 50.0,
        bottom: -50.0,
    };
    let fonts = font::FontMap::default();
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    for count in [0, 1, 4, 5, 16, 2, 0, 16] {
        for nested in [false, true] {
            let actors = target_scene(count, nested);
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                &actors,
                [0.1, 0.2, 0.3, 1.0],
                &metrics,
                &fonts,
                0.25,
                &mut text,
                &mut scratch,
                &Textures,
            );
            record_frame(&mut out, &frame);
            scratch.recycle_frame(&mut frame);
        }
    }
    for fade in [
        [0.0; 4],
        [-0.0; 4],
        [-0.25; 4],
        [f32::NEG_INFINITY; 4],
        [0.1, 0.2, 0.3, 0.4],
        [1.0; 4],
        [f32::NAN, 0.0, 0.1, f32::INFINITY],
    ] {
        for crop in [-0.5, 0.0, 0.1, 0.75, 1.0] {
            for flips in [[false, false], [true, false], [false, true], [true, true]] {
                let mut actors = fade_scene(fade);
                actors.truncate(4);
                for actor in &mut actors {
                    if let actors::Actor::Sprite {
                        cropleft,
                        cropright,
                        croptop,
                        cropbottom,
                        flip_x,
                        flip_y,
                        ..
                    } = actor
                    {
                        *cropleft = crop;
                        *cropright = crop * 0.25;
                        *croptop = crop * 0.5;
                        *cropbottom = crop * 0.125;
                        [*flip_x, *flip_y] = flips;
                    }
                }
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
                record_frame(&mut out, &frame);
                scratch.recycle_frame(&mut frame);
            }
        }
    }
    let mut uploads = renderer::TexturedMeshUploads::default();
    for stride in [0, 128, 2, 1, 0, 2] {
        let (mut geometries, _) = geometry_scene(stride);
        geometries.truncate(8);
        for step in 0..8 {
            match step {
                1 => geometries.reverse(),
                2 => geometries[0].cache_key = 99,
                3 => {
                    geometries[1].vertices =
                        renderer::TexturedMeshVertices::Transient(quad().repeat(2))
                }
                4 => geometries[2].cache_key = 0,
                5 => geometries.truncate(3),
                6 => geometries.clear(),
                _ => {}
            }
            renderer::resolve_textured_mesh_geometries(&geometries, &mut uploads, |key, _| {
                (key != 99 || step >= 4).then_some(key * 10)
            });
            writeln!(
                out,
                "uploads {stride} {step} {:?} {:?}",
                uploads.sources, uploads.vertices
            )
            .unwrap();
        }
    }
    std::fs::write(
        std::env::var_os("DEADSYNC_RENDER_SNAPSHOT").expect("snapshot path"),
        out,
    )
    .expect("write snapshot");
}
