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

fn metrics() -> Metrics {
    Metrics {
        left: 0.0,
        right: 800.0,
        top: 600.0,
        bottom: 0.0,
    }
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
