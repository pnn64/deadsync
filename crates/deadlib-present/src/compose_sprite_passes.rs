use super::*;
use deadlib_render_core::frame_compare::compare_render_frames_semantic;

fn split_passes(sprite: &Actor) -> [Actor; 2] {
    let mut diffuse = sprite.clone();
    let mut glow_only = sprite.clone();
    let Actor::Sprite { glow, .. } = &mut diffuse else {
        unreachable!()
    };
    *glow = [0.0; 4];
    let Actor::Sprite { tint, .. } = &mut glow_only else {
        unreachable!()
    };
    tint[3] = 0.0;
    [diffuse, glow_only]
}

#[test]
fn sprite_passes_match_independent_composition() {
    let metrics = Metrics {
        left: 0.0,
        right: 128.0,
        top: 128.0,
        bottom: 0.0,
    };
    let fonts = font::FontMap::default();
    let sources = [
        SpriteSource::Solid,
        SpriteSource::TextureStatic("glow 4x2.png"),
        SpriteSource::RenderTarget {
            handle: render_target_texture_handle(3),
            size: [64.0; 2],
        },
    ];
    for source in sources {
        for shape in 0..5 {
            for mask_count in 0..=2 {
                for (alpha, glow_alpha) in [
                    (0.7, 0.4),
                    (0.0, 0.4),
                    (0.7, 0.0),
                    (0.7, 0.0001),
                    (-0.0, -0.0),
                    (f32::NAN, 0.4),
                ] {
                    let mut sprite = test_sprite(source.clone());
                    let Actor::Sprite {
                        offset,
                        size,
                        world_z,
                        tint,
                        glow,
                        rot_x_deg,
                        rot_y_deg,
                        rot_z_deg,
                        skew,
                        flip_x,
                        flip_y,
                        cropleft,
                        cropright,
                        croptop,
                        fadeleft,
                        faderight,
                        fadetop,
                        fadebottom,
                        cell,
                        animate,
                        state_delay,
                        texcoordvelocity,
                        local_offset,
                        local_offset_rot_sin_cos,
                        mask_dest,
                        shadow_len,
                        shadow_color,
                        ..
                    } = &mut sprite
                    else {
                        unreachable!()
                    };
                    *offset = [28.0, 24.0];
                    *size = [SizeSpec::Px(48.0), SizeSpec::Px(40.0)];
                    *world_z = 3.0;
                    tint[3] = alpha;
                    *glow = [0.2, 0.8, 0.6, glow_alpha];
                    *rot_x_deg = 180.0;
                    *rot_y_deg = 24.0;
                    *rot_z_deg = if shape == 0 { 0.0 } else { 27.0 };
                    *skew = if shape == 2 { [0.3, -0.2] } else { [0.0; 2] };
                    *flip_x = true;
                    *flip_y = true;
                    *cropleft = if shape == 3 { 1.0 } else { -0.1 };
                    *cropright = 0.2;
                    *croptop = 0.1;
                    *fadeleft = 0.3;
                    *faderight = 0.4;
                    *fadetop = 0.2;
                    *fadebottom = 0.1;
                    *cell = Some((1, u32::MAX));
                    *animate = true;
                    *state_delay = 0.125;
                    *texcoordvelocity = Some([0.2, -0.3]);
                    *local_offset = [2.0, -3.0];
                    *local_offset_rot_sin_cos = [0.6, 0.8];
                    *mask_dest = mask_count != 0;
                    *shadow_len = [2.0, -3.0];
                    *shadow_color = [0.1, 0.2, 0.3, 0.5];
                    let mut prefix = Vec::new();
                    for index in 0..mask_count {
                        let mut mask = test_sprite(SpriteSource::Solid);
                        let Actor::Sprite {
                            offset,
                            size,
                            mask_source,
                            ..
                        } = &mut mask
                        else {
                            unreachable!()
                        };
                        *offset = if shape == 4 {
                            [110.0, 110.0]
                        } else {
                            [32.0 + index as f32 * 12.0, 24.0]
                        };
                        *size = [SizeSpec::Px(28.0), SizeSpec::Px(26.0)];
                        *mask_source = true;
                        prefix.push(mask);
                    }
                    let compose = |sprites: &[Actor]| {
                        let mut children = prefix.clone();
                        children.extend_from_slice(sprites);
                        let actors = [Actor::Camera {
                            view_proj: Matrix4::from_rotation_z(0.125),
                            children: vec![Actor::SharedFrame {
                                align: [0.0; 2],
                                offset: [0.0; 2],
                                size: [SizeSpec::Fill; 2],
                                children: Arc::from(children),
                                background: None,
                                z: 7,
                                tint: [0.9, 0.8, 0.7, 0.6],
                                blend: Some(BlendMode::Add),
                            }],
                        }];
                        build_screen(&actors, [0.0; 4], &metrics, &fonts, 0.7)
                    };
                    let expected = compose(&split_passes(&sprite));
                    let actual = compose(&[sprite]);
                    compare_render_frames_semantic(&expected, &actual).unwrap_or_else(|error| {
                        panic!("{source:?}, shape {shape}, masks {mask_count}, alpha {alpha}, glow {glow_alpha}: {error}")
                    });
                }
            }
        }
    }
}

#[test]
fn flat_sprite_passes_match_independent_composition() {
    let metrics = Metrics {
        left: 0.0,
        right: 128.0,
        top: 128.0,
        bottom: 0.0,
    };
    let fonts = font::FontMap::default();
    let resources = ActorResourceArena::new(0);
    let camera = Matrix4::from_rotation_z(0.2);
    for alpha in [0.0, -0.0, 0.5, f32::NAN] {
        for glow_alpha in [0.0, 0.0001, 0.25, f32::NAN] {
            for parent_alpha in [0.0, 0.7] {
                let sprite = FlatSprite {
                    center: [24.0, 36.0],
                    size: [32.0, 48.0],
                    world_z: 2.0,
                    source: SpriteSource::TextureStatic("flat glow.png"),
                    tint: [0.8, 0.6, 0.4, alpha],
                    glow: [0.2, 0.6, 0.9, glow_alpha],
                    uv_rect: [0.8, 0.7, 0.1, 0.2],
                    flip_x: true,
                    flip_y: false,
                    fade: [0.1, 0.2, 0.3, 0.4],
                    blend: BlendMode::Alpha,
                    rot_y_deg: 160.0,
                    rot_z_deg: 32.0,
                    z: 3,
                };
                let compose = |sprites: &[FlatSprite]| {
                    let draws: Vec<_> = sprites.iter().cloned().map(FlatDraw::Sprite).collect();
                    let tint = [0.7, 0.8, 0.9, parent_alpha];
                    let segment = ActorSegment::transformed(
                        &[],
                        9,
                        &tint,
                        Some(BlendMode::Add),
                        &camera,
                        &Matrix4::IDENTITY,
                        Some(ActorXFold::new(64.0, 0.8)),
                    )
                    .with_flat_draws(&draws, Some(&camera));
                    build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
                        &[segment], [0.0; 4], &metrics, &fonts, 0.7,
                        &mut TextLayoutCache::default(), &mut ComposeScratch::default(),
                        &TestDrawTextureContext, &resources,
                    )
                };
                let mut diffuse = sprite.clone();
                diffuse.glow = [0.0; 4];
                let mut glow_only = sprite.clone();
                glow_only.tint[3] = 0.0;
                let expected = compose(&[diffuse, glow_only]);
                let actual = compose(&[sprite]);
                compare_render_frames_semantic(&expected, &actual).unwrap_or_else(|error| {
                    panic!("alpha {alpha}, glow {glow_alpha}, parent {parent_alpha}: {error}")
                });
            }
        }
    }
}
