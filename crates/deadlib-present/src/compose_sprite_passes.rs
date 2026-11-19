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
                    vertex_color_encoding: Default::default(),
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
                    rot_x_deg: 0.0,
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

fn rotated_sprite_frame(rotation: [f32; 3], flat: bool) -> RenderFrame {
    let metrics = Metrics::centered(128.0, 128.0);
    let fonts = font::FontMap::default();
    // A perspective camera whose homogeneous W is 200 - world Z.
    let camera = Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 200.0,
    ]);
    let tint = [0.8, 0.6, 0.4, 0.7];
    let glow = [0.2, 0.8, 0.6, 0.4];
    if flat {
        let draws = [FlatDraw::Sprite(FlatSprite {
            vertex_color_encoding: Default::default(),
            center: [64.0; 2],
            size: [64.0, 48.0],
            world_z: 3.0,
            source: SpriteSource::TextureStatic("rotating.png"),
            tint,
            glow,
            uv_rect: [0.1, 0.2, 0.9, 0.8],
            flip_x: false,
            flip_y: false,
            fade: [0.0; 4],
            blend: BlendMode::Alpha,
            rot_x_deg: rotation[0],
            rot_y_deg: rotation[1],
            rot_z_deg: rotation[2],
            z: 2,
        })];
        return build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &[ActorSegment::new(&[]).with_flat_draws(&draws, Some(&camera))],
            [0.0; 4], &metrics, &fonts, 0.0,
            &mut TextLayoutCache::default(), &mut ComposeScratch::default(),
            &TestDrawTextureContext, &ActorResourceArena::new(0),
        );
    }
    let mut sprite = test_sprite(SpriteSource::TextureStatic("rotating.png"));
    let Actor::Sprite {
        align, offset, size, world_z, tint: actor_tint, glow: actor_glow,
        uv_rect, rot_x_deg, rot_y_deg, rot_z_deg, ..
    } = &mut sprite else { unreachable!() };
    *align = [0.5; 2];
    *offset = [64.0; 2];
    *size = [SizeSpec::Px(64.0), SizeSpec::Px(48.0)];
    *world_z = 3.0;
    *actor_tint = tint;
    *actor_glow = glow;
    *uv_rect = Some([0.1, 0.2, 0.9, 0.8]);
    [*rot_x_deg, *rot_y_deg, *rot_z_deg] = rotation;
    build_screen(
        &[Actor::Camera { view_proj: camera, children: vec![sprite] }],
        [0.0; 4], &metrics, &fonts, 0.0,
    )
}

#[test]
fn sprite_xy_rotations_keep_native_handedness_and_perspective() {
    for flat in [false, true] {
        for rotation in [
            [30.0_f32, 0.0, 0.0], [0.0, 35.0, 0.0], [25.0, -40.0, 15.0],
            [180.0, 0.0, 0.0], [0.0, 180.0, 0.0],
        ] {
            let frame = rotated_sprite_frame(rotation, flat);
            assert!(frame.sprite_instances.is_empty());
            assert_eq!(frame.tmesh_instances.len(), 2, "diffuse and glow");
            assert_eq!(frame.tmesh_instances[0].texture_mask, 0.0);
            assert_eq!(frame.tmesh_instances[1].texture_mask, 1.0);
            let DrawOp::TexturedMesh(run) = frame.ops[0] else {
                panic!("rotated sprite must retain depth in mesh geometry");
            };
            assert!(!run.depth_test && !run.depth_write);
            assert_eq!(run.sampler, Some(deadlib_render_core::MeshSampler::Texture));
            let instance = &frame.tmesh_instances[run.instance_start as usize];
            let transform = instance.transform();
            // Native RageMatrixRotationXYZ applies Z, Y, X to local points.
            // Y-down -> Y-up reverses X's sign relative to the native matrix;
            // the Sprite contract already supplies Z in world coordinates.
            let expected_rotation = Matrix4::from_rotation_x(rotation[0].to_radians())
                * Matrix4::from_rotation_y(-rotation[1].to_radians())
                * Matrix4::from_rotation_z(rotation[2].to_radians());
            let camera = frame.cameras[run.camera as usize];
            let mut clip_w_range = [f32::INFINITY, f32::NEG_INFINITY];
            for vertex in frame.tmesh_geometries[run.geometry as usize].vertices.as_ref() {
                let local = Vector3::new(
                    (vertex.uv[0] - 0.5) * 64.0,
                    (0.5 - vertex.uv[1]) * 48.0,
                    0.0,
                );
                let expected = expected_rotation.transform_point3(local) + Vector3::new(0.0, 0.0, 3.0);
                let actual = transform.transform_point3(Vector3::from(vertex.pos));
                assert!((actual - expected).abs().max_element() < 0.00002,
                    "flat={flat}, rotation={rotation:?}, uv={:?}: {actual:?} vs {expected:?}", vertex.uv);
                let clip = camera * actual.extend(1.0);
                assert!((clip.w - (200.0 - expected.z)).abs() < 0.00002);
                clip_w_range[0] = clip_w_range[0].min(clip.w);
                clip_w_range[1] = clip_w_range[1].max(clip.w);
            }
            if rotation[0].abs() < 90.0 && rotation[1].abs() < 90.0 {
                assert!(clip_w_range[1] - clip_w_range[0] > 20.0,
                    "X/Y tilt must vary perspective W, not just shrink a flat quad");
            }
            if rotation == [30.0, 0.0, 0.0] {
                let top = transform.transform_point3(Vector3::new(0.0, 0.5, 0.0));
                let bottom = transform.transform_point3(Vector3::new(0.0, -0.5, 0.0));
                assert!(top.z > 3.0 && bottom.z < 3.0);
            }
        }
    }
}

#[test]
fn sprite_without_xy_rotation_keeps_instanced_fast_path() {
    for flat in [false, true] {
        for rotation in [[0.0; 3], [0.0, 0.0, 27.0]] {
            let frame = rotated_sprite_frame(rotation, flat);
            assert_eq!(frame.sprite_instances.len(), 2);
            assert!(frame.tmesh_instances.is_empty());
            assert!(frame.tmesh_geometries.is_empty());
        }
    }
}

fn interpolated_mesh_alpha(vertices: &[TexturedMeshVertex], uv: [f32; 2]) -> f32 {
    for triangle in vertices.chunks_exact(3) {
        let [a, b, c] = [triangle[0].uv, triangle[1].uv, triangle[2].uv];
        let determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        let wa = ((b[1] - c[1]) * (uv[0] - c[0]) + (c[0] - b[0]) * (uv[1] - c[1])) / determinant;
        let wb = ((c[1] - a[1]) * (uv[0] - c[0]) + (a[0] - c[0]) * (uv[1] - c[1])) / determinant;
        let wc = 1.0 - wa - wb;
        if wa >= -0.00001 && wb >= -0.00001 && wc >= -0.00001 {
            return wa * triangle[0].color[3] + wb * triangle[1].color[3] + wc * triangle[2].color[3];
        }
    }
    panic!("mesh does not cover UV {uv:?}");
}

#[test]
fn rotated_sprite_preserves_cropped_uvs_and_shader_minimum_edge_fades() {
    let metrics = Metrics::centered(128.0, 128.0);
    let fonts = font::FontMap::default();
    for (fade, crop, flips) in [
        ([0.2, 0.35, 0.25, 0.1], [-0.1, 0.2, 0.1, 0.0], [false, false]),
        ([0.8, 0.6, 0.9, 0.7], [0.1, 0.2, 0.3, 0.1], [true, true]),
        ([0.0, 0.4, 0.0, 0.0], [0.0; 4], [true, false]),
    ] {
        let mut sprite = test_sprite(SpriteSource::TextureStatic("faded.png"));
        let Actor::Sprite {
            align, offset, uv_rect, flip_x, flip_y,
            cropleft, cropright, croptop, cropbottom,
            fadeleft, faderight, fadetop, fadebottom, ..
        } = &mut sprite else { unreachable!() };
        *align = [0.5; 2];
        *offset = [64.0; 2];
        *uv_rect = Some([0.1, 0.2, 0.9, 0.8]);
        [*flip_x, *flip_y] = flips;
        [*cropleft, *cropright, *croptop, *cropbottom] = crop;
        [*fadeleft, *faderight, *fadetop, *fadebottom] = fade;
        let flat = build_screen(&[sprite.clone()], [0.0; 4], &metrics, &fonts, 0.0);
        let baseline = &flat.sprite_instances[0];
        let Actor::Sprite { rot_x_deg, rot_y_deg, skew, .. } = &mut sprite else { unreachable!() };
        *rot_x_deg = 25.0;
        *rot_y_deg = -35.0;
        *skew = [0.3, -0.2];
        let tilted = build_screen(&[sprite], [0.0; 4], &metrics, &fonts, 0.0);
        let instance = &tilted.tmesh_instances[0];
        assert_eq!(instance.uv_scale, baseline.uv_scale);
        assert_eq!(instance.uv_offset, baseline.uv_offset);
        let vertices = tilted.tmesh_geometries[0].vertices.as_ref();
        assert!(vertices.len() <= 54);
        let rotation = Matrix4::from_rotation_x(25.0_f32.to_radians())
            * Matrix4::from_rotation_y(35.0_f32.to_radians());
        for vertex in vertices {
            // Crop remains local to the actor: shear and tilt also rotate its offset.
            let x = (vertex.uv[0] - 0.5) * baseline.size[0] + baseline.center[0];
            let y = (0.5 - vertex.uv[1]) * baseline.size[1] + baseline.center[1];
            let skew_y = y - 0.2 * x;
            let expected = rotation.transform_point3(Vector3::new(x + 0.3 * skew_y, skew_y, 0.0));
            let actual = instance.transform().transform_point3(Vector3::from(vertex.pos));
            assert!((actual - expected).abs().max_element() < 0.00002);
        }
        for u in [0.03, 0.12, 0.28, 0.51, 0.77, 0.96] {
            for v in [0.02, 0.15, 0.34, 0.54, 0.81, 0.97] {
                let mut shader_alpha: f32 = 1.0;
                for (distance, width) in [u, 1.0 - u, v, 1.0 - v].into_iter().zip(baseline.edge_fade) {
                    if width > 0.0 { shader_alpha = shader_alpha.min((distance / width).clamp(0.0, 1.0)); }
                }
                let mesh_alpha = interpolated_mesh_alpha(vertices, [u, v]);
                assert!((mesh_alpha - shader_alpha).abs() < 0.00001,
                    "fade={fade:?}, crop={crop:?}, flips={flips:?}, uv={:?}: {mesh_alpha} vs {shader_alpha}", [u, v]);
            }
        }
    }
}

#[test]
fn masked_tilted_sprite_keeps_depth_and_sprite_shadow_color() {
    let metrics = Metrics::centered(128.0, 128.0);
    let fonts = font::FontMap::default();
    for wrapped_shadow in [false, true] {
        let mut mask = test_sprite(SpriteSource::Solid);
        let Actor::Sprite { align, offset, size, mask_source, .. } = &mut mask else { unreachable!() };
        *align = [0.5; 2];
        *offset = [64.0; 2];
        *size = [SizeSpec::Px(32.0), SizeSpec::Px(24.0)];
        *mask_source = true;
        let mut sprite = test_sprite(SpriteSource::TextureStatic("masked tilt.png"));
        let Actor::Sprite {
            align, offset, size, world_z, tint, rot_x_deg, mask_dest,
            shadow_len, shadow_color, ..
        } = &mut sprite else { unreachable!() };
        *align = [0.5; 2];
        *offset = [64.0; 2];
        *size = [SizeSpec::Px(64.0), SizeSpec::Px(48.0)];
        *world_z = 5.0;
        *tint = [0.8, 0.6, 0.4, 0.7];
        *rot_x_deg = 30.0;
        *mask_dest = true;
        if !wrapped_shadow {
            *shadow_len = [2.0, -3.0];
            *shadow_color = [0.25, 0.5, 0.75, 0.6];
        }
        if wrapped_shadow {
            sprite = Actor::Shadow {
                len: [2.0, -3.0], color: [0.25, 0.5, 0.75, 0.6], child: Box::new(sprite),
            };
        }
        let frame = build_screen(&[mask, sprite], [0.0; 4], &metrics, &fonts, 0.0);
        assert!(frame.sprite_instances.is_empty());
        assert_eq!(frame.tmesh_instances.len(), 2, "shadow and diffuse");
        let mut checked_shadow = false;
        for op in &frame.ops {
            let DrawOp::TexturedMesh(run) = op else { panic!("masked sprite uses mesh geometry") };
            assert_eq!(run.sampler, Some(deadlib_render_core::MeshSampler::Texture),
                "masked diffuse and shadow must retain Sprite sampling");
            let instance = &frame.tmesh_instances[run.instance_start as usize];
            let is_shadow = instance.tint[0] == 0.25;
            if is_shadow {
                assert_eq!(&instance.tint[..3], &[0.25, 0.5, 0.75]);
                assert!((instance.tint[3] - 0.42).abs() < 0.00001);
                checked_shadow = true;
            }
            let offset = if is_shadow { [2.0, -3.0] } else { [0.0; 2] };
            let mut z_range = [f32::INFINITY, f32::NEG_INFINITY];
            for vertex in frame.tmesh_geometries[run.geometry as usize].vertices.as_ref() {
                let p = instance.transform().transform_point3(Vector3::from(vertex.pos));
                let x = p.x - offset[0];
                let y = p.y - offset[1];
                assert!(x.abs() <= 16.00001 && y.abs() <= 12.00001);
                assert!((p.z - (5.0 + y * 30.0_f32.to_radians().tan())).abs() < 0.00002,
                    "mask interpolation must preserve the tilted plane: {p:?}");
                z_range[0] = z_range[0].min(p.z);
                z_range[1] = z_range[1].max(p.z);
            }
            assert!(z_range[1] - z_range[0] > 10.0);
        }
        assert!(checked_shadow, "tilting must not multiply the shadow RGB by the sprite tint");
    }
}

#[test]
fn scrolling_sprite_keeps_texture_sampling_when_rotated() {
    use deadlib_render_core::{MeshSampler, SamplerDesc, SamplerFilter, SamplerWrap, texture_sampler_desc};
    let metrics = Metrics::centered(128.0, 128.0);
    let fonts = font::FontMap::default();
    let mut sprite = test_sprite(SpriteSource::TextureStatic("scrolling.png"));
    let Actor::Sprite { uv_rect, texcoordvelocity, flip_x, .. } = &mut sprite else { unreachable!() };
    *uv_rect = Some([0.2, 0.3, 0.8, 0.9]);
    *texcoordvelocity = Some([0.4, -0.3]);
    *flip_x = true;
    let flat = build_screen(&[sprite.clone()], [0.0; 4], &metrics, &fonts, 4.0);
    let DrawOp::Sprite(flat_run) = flat.ops[0] else { panic!("untilted sprite keeps instancing") };
    let baseline = &flat.sprite_instances[flat_run.instance_start as usize];
    assert!(baseline.uv_offset[0] > 1.0 && baseline.uv_offset[1] < 0.0);
    let Actor::Sprite { rot_x_deg, .. } = &mut sprite else { unreachable!() };
    *rot_x_deg = 30.0;
    let tilted = build_screen(&[sprite], [0.0; 4], &metrics, &fonts, 4.0);
    let DrawOp::TexturedMesh(run) = tilted.ops[0] else { panic!("tilted sprite keeps depth") };
    let instance = &tilted.tmesh_instances[run.instance_start as usize];
    assert_eq!(run.sampler, Some(MeshSampler::Texture));
    assert_eq!(run.texture_handle, flat_run.texture_handle);
    assert_eq!(instance.uv_scale, baseline.uv_scale);
    assert_eq!(instance.uv_offset, baseline.uv_offset);
    for filter in [SamplerFilter::Linear, SamplerFilter::Nearest] {
        for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
            for mipmaps in [false, true] {
                let texture = SamplerDesc { filter, wrap, mipmaps };
                assert_eq!(
                    texture_sampler_desc(texture, run.texture_handle, true, run.sampler),
                    texture_sampler_desc(texture, flat_run.texture_handle, false, None),
                );
            }
        }
    }
}
