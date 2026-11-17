use super::*;
use crate::actors::VertexColorEncoding;

fn native_sprite(color: [f32; 4]) -> Actor {
    let mut actor = test_sprite(SpriteSource::Solid);
    let Actor::Sprite {
        tint,
        vertex_color_encoding,
        ..
    } = &mut actor
    else {
        unreachable!()
    };
    *tint = color;
    *vertex_color_encoding = VertexColorEncoding::RageVColor;
    actor
}

fn tinted(children: Vec<Actor>, tint: [f32; 4]) -> Actor {
    Actor::SharedFrame {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Fill; 2],
        children: children.into(),
        background: None,
        z: 0,
        tint,
        blend: None,
    }
}

fn render(actors: &[Actor]) -> RenderFrame {
    build_screen(
        actors,
        [0.0; 4],
        &Metrics::centered(128.0, 128.0),
        &font::FontMap::default(),
        0.0,
    )
}

fn rgba(bytes: [u8; 4]) -> [f32; 4] {
    bytes.map(|byte| f32::from(byte) / 255.0)
}

#[test]
fn native_vertex_colors_pack_combined_parent_tint_without_changing_actor_state() {
    let source = native_sprite([0.875, 0.5, 0.25, 0.875]);
    let mut float = source.clone();
    let Actor::Sprite {
        vertex_color_encoding,
        ..
    } = &mut float
    else {
        unreachable!()
    };
    *vertex_color_encoding = VertexColorEncoding::Float;
    let actors = [tinted(vec![source.clone(), float], [0.875; 4])];
    let frame = render(&actors);
    assert_eq!(frame.sprite_instances.len(), 2);
    assert_eq!(frame.sprite_instances[0].tint, rgba([196, 112, 56, 196]));
    assert_eq!(
        frame.sprite_instances[1].tint,
        [0.765625, 0.4375, 0.21875, 0.765625]
    );
    let Actor::Sprite { tint, .. } = source else {
        unreachable!()
    };
    assert_eq!(tint, [0.875, 0.5, 0.25, 0.875]);
}

#[test]
fn native_vertex_colors_select_sub_byte_diffuse_and_glow_before_packing() {
    let mut sprite = native_sprite([1.0, 0.5, 0.25, 0.001]);
    let Actor::Sprite { glow, .. } = &mut sprite else {
        unreachable!()
    };
    *glow = [0.875, 0.5, 0.25, 0.001];
    let frame = render(&[sprite]);
    assert_eq!(
        frame.sprite_instances.len(),
        2,
        "both raw float alphas select a pass"
    );
    assert_eq!(frame.sprite_instances[0].tint, rgba([255, 128, 64, 0]));
    assert_eq!(frame.sprite_instances[1].tint, rgba([224, 128, 64, 0]));
    assert!(frame.sprite_instances[1].texture_mask > 0.5);
}

fn native_mesh(vertices: Arc<[TexturedMeshVertex]>) -> Actor {
    Actor::TexturedMesh {
        environment: None,
        align: [0.0; 2],
        offset: [0.0; 2],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0); 2],
        local_transform: Matrix4::IDENTITY,
        texture: "native-colors.png".into(),
        tint: [0.5; 4],
        vertex_color_encoding: VertexColorEncoding::RageVColor,
        glow: [0.0; 4],
        vertices,
        geom_cache_key: 42,
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        uv_tex_shift: [0.0; 2],
        depth_test: false,
        depth_write: false,
        depth_test_on_fail: false,
        clear_depth: false,
        clear_depth_after: false,
        cull_mode: deadlib_render_core::CullMode::None,
        visible: true,
        blend: BlendMode::Alpha,
        z: 0,
    }
}

fn colored_quad() -> Arc<[TexturedMeshVertex]> {
    Arc::from(
        [
            [0.0, 0.0],
            [32.0, 0.0],
            [32.0, 32.0],
            [0.0, 0.0],
            [32.0, 32.0],
            [0.0, 32.0],
        ]
        .map(|pos| TexturedMeshVertex {
            pos: [pos[0], pos[1], 0.0],
            normal: [0.0, 0.0, 1.0, 0.0],
            uv: [pos[0] / 32.0, pos[1] / 32.0],
            tex_matrix_scale: [1.0; 2],
            color: [0.5, 0.25, 0.875, if pos[0] == 0.0 { 0.5 } else { 0.25 }],
        }),
    )
}

#[test]
fn native_vertex_colors_pack_corner_products_and_preserve_shared_geometry() {
    let vertices = colored_quad();
    let frame = render(&[tinted(vec![native_mesh(Arc::clone(&vertices))], [0.5; 4])]);
    let (_, instance, geometry) = tmesh_draw(&frame, 0);
    assert_eq!(instance.tint, [1.0; 4]);
    assert_eq!(geometry.vertices[0].color, rgba([32, 16, 56, 32]));
    assert_eq!(geometry.vertices[1].color, rgba([32, 16, 56, 16]));
    assert_eq!(geometry.cache_key, INVALID_TMESH_CACHE_KEY);
    assert_eq!(vertices[0].color, [0.5, 0.25, 0.875, 0.5]);
    assert_eq!(vertices[1].color[3], 0.25);
}

#[test]
fn native_vertex_colors_shadow_uses_first_corner_alpha_and_glow_uses_its_own_color() {
    let vertices = colored_quad();
    let shadow = Actor::Shadow {
        len: [3.0, 4.0],
        color: [0.875, 0.5, 0.25, 0.5],
        child: Box::new(native_mesh(Arc::clone(&vertices))),
    };
    let mut glow = native_mesh(vertices);
    let Actor::TexturedMesh {
        tint, glow: color, ..
    } = &mut glow
    else {
        unreachable!()
    };
    tint[3] = 0.0;
    *color = [0.875; 4];
    let frame = render(&[tinted(vec![shadow, glow], [0.5; 4])]);
    assert_eq!(frame.ops.len(), 3);
    let (_, shadow, geometry) = tmesh_draw(&frame, 0);
    for vertex in geometry.vertices.iter() {
        assert_eq!(
            mul_rgba(vertex.color, shadow.tint),
            rgba([224, 128, 64, 16])
        );
    }
    let (_, glow, geometry) = tmesh_draw(&frame, 2);
    assert!(glow.texture_mask > 0.5);
    for vertex in geometry.vertices.iter() {
        assert_eq!(mul_rgba(vertex.color, glow.tint), rgba([112; 4]));
    }
}

#[test]
fn native_vertex_colors_retained_frames_pack_once_per_final_draw() {
    let frame = Arc::new(RetainedActorFrame::new(vec![native_sprite([0.875; 4])]));
    let mut actors = [Actor::RetainedFrame {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Fill; 2],
        frame,
        z: 0,
        tint: [0.875; 4],
        blend: None,
        visible: true,
    }];
    let metrics = Metrics::centered(128.0, 128.0);
    let fonts = font::FontMap::default();
    let mut text_cache = TextLayoutCache::default();
    let mut scratch = ComposeScratch::default();
    for expected in [196, 196, 112] {
        let mut render = build_screen_cached_with_scratch(
            &actors,
            [0.0; 4],
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut scratch,
        );
        assert_eq!(render.sprite_instances[0].tint, rgba([expected; 4]));
        scratch.recycle_frame(&mut render);
        if scratch.retained_frame_stats().hits == 1 {
            let Actor::RetainedFrame { tint, .. } = &mut actors[0] else {
                unreachable!()
            };
            *tint = [0.5; 4];
        }
    }
    assert_eq!(scratch.retained_frame_stats().hits, 1);
}
