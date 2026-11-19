//! Second render pass: source resolution, offscreen passes, and sprite fades.
use super::*;

#[test]
fn target_passes_keep_order_and_reuse_capacity() {
    let metrics = Metrics {
        left: -50.0,
        right: 50.0,
        top: 50.0,
        bottom: -50.0,
    };
    let fonts = font::FontMap::default();
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    let actors = target_scene(16);
    for _ in 0..3 {
        let mut frame = build_passes(
            std::iter::empty(),
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
            None,
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
        let mut frame = build_passes(
            std::iter::empty(),
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
            None,
        );
        scratch.recycle_frame(&mut frame);
    }
    let heap = crate::HEAP
        .with(|c| c.replace(None))
        .expect("heap measurement enabled");
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
}

#[test]
fn cropped_rotated_sprite_keeps_the_native_uncropped_pivot() {
    let metrics = Metrics::centered(854.0, 480.0);
    let mut actor = sprite_actor(10.0, false);
    if let actors::Actor::Sprite { offset, size, source, mask_dest, cropright, .. } = &mut actor {
        *offset = [377.0, 240.0];
        *size = [SizeSpec::Px(854.0), SizeSpec::Px(480.0)];
        *source = actors::SpriteSource::static_texture("fixture");
        *mask_dest = false;
        *cropright = 0.5;
    }
    let frame = build_screen_with_texture_context(
        &[actor], [0.0; 4], &metrics, &font::FontMap::default(), 0.0, &Textures,
    );
    assert_eq!(frame.sprite_instances.len(), 1);
    let sprite = &frame.sprite_instances[0];
    // Native 663525d, 666 chart 2, retained draw def-0013 at frame 1301.
    // These clip vertices rotate the cropped local rectangle around the full
    // sprite origin; rotating about the cropped center shifts every vertex.
    let expected = [
        [-1.199_504_6, 0.675_858_7],
        [-1.004_302_9, -1.293_756_7],
        [-0.019_495_19, -0.984_807_85],
        [-0.214_696_88, 0.984_807_7],
    ];
    for (point, expected) in [[-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5]].into_iter().zip(expected) {
        let world = sprite_world_xy(sprite.center, sprite.size, sprite.rot_sin_cos, [0.0; 2], point);
        let clip = [world[0] / 427.0, world[1] / 240.0];
        for axis in 0..2 {
            assert!((clip[axis] - expected[axis]).abs() < 2.0e-6, "{point:?}: {clip:?} != {expected:?}");
        }
    }
    assert_eq!(sprite.uv_scale, [0.5, 1.0]);
    assert_eq!(sprite.uv_offset, [0.0, 0.0]);
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

fn target_scene(count: usize) -> Vec<actors::RenderTarget> {
    let mut actors = Vec::with_capacity(count);
    for i in 0..count {
        let mut child = sprite_actor(0.0, false);
        if let actors::Actor::Sprite { mask_dest, .. } = &mut child {
            *mask_dest = false;
        }
        let children = vec![child];
        actors.push(actors::RenderTarget {
            texture_handle: renderer::render_target_texture_handle((i + 1) as u64),
            size: [64 + i as u32, 32 + i as u32],
            viewport: [64 + i as u32, 32 + i as u32],
            logical_size: [100.0; 2],
            float_color: false,
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
