use deadlib_present::actors::Actor;
use deadsync_noteskin::ModelDrawState;
use glam::{Mat4 as Matrix4, Vec3 as Vector3};

#[inline(always)]
pub fn song_lua_note_model_draw(mut draw: ModelDrawState, rotation_y_deg: f32) -> ModelDrawState {
    if rotation_y_deg.abs() > f32::EPSILON {
        draw.rot[1] += rotation_y_deg;
    }
    draw
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaPlayerTransformRequest {
    pub screen_width: f32,
    pub screen_height: f32,
    pub screen_center_y: f32,
    pub playfield_center_x: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub rotation_x_deg: f32,
    pub rotation_z_deg: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
}

pub fn song_lua_player_skew_x_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        amount, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

pub fn song_lua_player_skew_y_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, amount, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

#[inline(always)]
fn song_lua_fold_x_around_pivot(x: f32, pivot_x: f32, cos_y: f32) -> f32 {
    pivot_x + (x - pivot_x) * cos_y
}

pub fn song_lua_player_y_fold_actor(actor: Actor, pivot_x: f32, rotation_y_deg: f32) -> Actor {
    if !pivot_x.is_finite() || !rotation_y_deg.is_finite() || rotation_y_deg.abs() <= f32::EPSILON {
        return actor;
    }
    let cos_y = rotation_y_deg.to_radians().cos();
    match actor {
        Actor::Sprite {
            align,
            mut offset,
            world_z,
            size,
            source,
            tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Sprite {
                align,
                offset,
                world_z,
                size,
                source,
                tint,
                glow,
                z,
                cell,
                grid,
                uv_rect,
                visible,
                flip_x,
                flip_y,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                local_offset,
                local_offset_rot_sin_cos,
                texcoordvelocity,
                animate,
                state_delay,
                scale,
                shadow_len,
                shadow_color,
                effect,
            }
        }
        Actor::Text {
            align,
            mut offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            mut scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            scale[0] *= cos_y;
            Actor::Text {
                align,
                offset,
                local_transform,
                color,
                stroke_color,
                glow,
                font,
                content,
                attributes,
                align_text,
                z,
                scale,
                fit_width,
                fit_height,
                line_spacing,
                wrap_width_pixels,
                max_width,
                max_height,
                max_w_pre_zoom,
                max_h_pre_zoom,
                jitter,
                distortion,
                clip,
                mask_dest,
                blend,
                shadow_len,
                shadow_color,
                effect,
            }
        }
        Actor::Mesh {
            align,
            mut offset,
            size,
            vertices,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Mesh {
                align,
                offset,
                size,
                vertices,
                visible,
                blend,
                z,
            }
        }
        Actor::TexturedMesh {
            align,
            mut offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geometry_id,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::TexturedMesh {
                align,
                offset,
                world_z,
                size,
                local_transform,
                texture,
                tint,
                glow,
                vertices,
                geometry_id,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                depth_test,
                visible,
                blend,
                z,
            }
        }
        Actor::Frame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Frame {
                align,
                offset,
                size,
                children: children
                    .into_iter()
                    .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                    .collect(),
                background,
                z,
            }
        }
        Actor::SharedFrame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
            tint,
            blend,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::SharedFrame {
                align,
                offset,
                size,
                children,
                background,
                z,
                tint,
                blend,
            }
        }
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                .collect(),
        },
        Actor::CameraPush { view_proj } => Actor::CameraPush { view_proj },
        Actor::CameraPop => Actor::CameraPop,
        Actor::Shadow { len, color, child } => Actor::Shadow {
            len,
            color,
            child: Box::new(song_lua_player_y_fold_actor(
                *child,
                pivot_x,
                rotation_y_deg,
            )),
        },
    }
}

pub fn song_lua_player_transform_matrix(request: SongLuaPlayerTransformRequest) -> Option<Matrix4> {
    let SongLuaPlayerTransformRequest {
        screen_width,
        screen_height,
        screen_center_y,
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_z_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    } = request;
    if !screen_width.is_finite()
        || !screen_height.is_finite()
        || !screen_center_y.is_finite()
        || !playfield_center_x.is_finite()
        || !target_x.is_finite()
        || !target_y.is_finite()
        || !rotation_x_deg.is_finite()
        || !rotation_z_deg.is_finite()
        || !skew_x.is_finite()
        || !skew_y.is_finite()
        || !zoom_x.is_finite()
        || !zoom_y.is_finite()
        || !zoom_z.is_finite()
    {
        return None;
    }
    let rotation_x_deg = if rotation_x_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_x_deg
    };
    let rotation_z_deg = if rotation_z_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_z_deg
    };
    let skew_x = if skew_x.abs() <= f32::EPSILON {
        0.0
    } else {
        skew_x
    };
    let skew_y = if skew_y.abs() <= f32::EPSILON {
        0.0
    } else {
        skew_y
    };
    let zoom_x = if (zoom_x - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_x
    };
    let zoom_y = if (zoom_y - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_y
    };
    let zoom_z = if (zoom_z - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_z
    };
    let translate_x = target_x - playfield_center_x;
    let translate_y = screen_center_y - target_y;
    if rotation_x_deg.abs() <= f32::EPSILON
        && rotation_z_deg.abs() <= f32::EPSILON
        && skew_x.abs() <= f32::EPSILON
        && skew_y.abs() <= f32::EPSILON
        && (zoom_x - 1.0).abs() <= f32::EPSILON
        && (zoom_y - 1.0).abs() <= f32::EPSILON
        && (zoom_z - 1.0).abs() <= f32::EPSILON
        && translate_x.abs() <= f32::EPSILON
        && translate_y.abs() <= f32::EPSILON
    {
        return None;
    }

    let pivot_x = playfield_center_x - 0.5 * screen_width;
    let pivot_y = 0.5 * screen_height - screen_center_y;
    // ITGmania actor transforms are authored in screen coordinates (Y down).
    // This matrix is applied in DeadSync world space (Y up), so Z rotation and
    // actor skews flip sign across the Y axis.
    let rotation_z_deg = -rotation_z_deg;
    let skew_x = -skew_x;
    let skew_y = -skew_y;
    Some(
        Matrix4::from_translation(Vector3::new(translate_x, translate_y, 0.0))
            * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
            * Matrix4::from_rotation_x(rotation_x_deg.to_radians())
            * Matrix4::from_rotation_z(rotation_z_deg.to_radians())
            * song_lua_player_skew_x_matrix(skew_x)
            * song_lua_player_skew_y_matrix(skew_y)
            * Matrix4::from_scale(Vector3::new(zoom_x, zoom_y, zoom_z))
            * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        SongLuaPlayerTransformRequest, song_lua_player_skew_x_matrix,
        song_lua_player_skew_y_matrix, song_lua_player_transform_matrix,
        song_lua_player_y_fold_actor,
    };
    use deadlib_present::actors::{Actor, SizeSpec, TextContent};
    use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
    use deadlib_render::{BlendMode, MeshVertex, TexturedMeshVertex};
    use glam::{Mat4 as Matrix4, Vec3 as Vector3};
    use std::sync::Arc;

    const SCREEN_WIDTH: f32 = 854.0;
    const SCREEN_HEIGHT: f32 = 480.0;
    const SCREEN_CENTER_Y: f32 = 240.0;
    const PLAYFIELD_CENTER_X: f32 = 427.0;

    fn request() -> SongLuaPlayerTransformRequest {
        SongLuaPlayerTransformRequest {
            screen_width: SCREEN_WIDTH,
            screen_height: SCREEN_HEIGHT,
            screen_center_y: SCREEN_CENTER_Y,
            playfield_center_x: PLAYFIELD_CENTER_X,
            target_x: PLAYFIELD_CENTER_X,
            target_y: SCREEN_CENTER_Y,
            rotation_x_deg: 0.0,
            rotation_z_deg: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
        }
    }

    fn transform_point(matrix: Matrix4, local: [f32; 2]) -> [f32; 2] {
        let point = matrix.transform_point3(Vector3::new(local[0], local[1], 0.0));
        [point.x, point.y]
    }

    fn sprite(x: f32) -> Actor {
        let mut actor = SpriteBuilder::solid();
        actor.xy(x, 12.0);
        actor.zoomx(5.0);
        actor.zoomy(7.0);
        actor.z(11);
        actor.build(0)
    }

    fn text(x: f32) -> Actor {
        let mut actor = TextBuilder::new();
        actor.xy(x, 15.0);
        actor.zoomx(2.0);
        actor.zoomy(3.0);
        actor.settext(TextContent::Static("fold"));
        actor.z(12);
        actor.build(0)
    }

    fn actor_x(actor: &Actor) -> f32 {
        match actor {
            Actor::Sprite { offset, .. }
            | Actor::Text { offset, .. }
            | Actor::Mesh { offset, .. }
            | Actor::TexturedMesh { offset, .. }
            | Actor::Frame { offset, .. }
            | Actor::SharedFrame { offset, .. } => offset[0],
            other => panic!("actor has no direct x offset: {other:?}"),
        }
    }

    #[test]
    fn player_transform_identity_and_nonfinite_inputs_emit_no_matrix() {
        assert!(song_lua_player_transform_matrix(request()).is_none());
        let mut invalid = request();
        invalid.screen_width = f32::NAN;
        assert!(song_lua_player_transform_matrix(invalid).is_none());
        invalid = request();
        invalid.zoom_z = f32::INFINITY;
        assert!(song_lua_player_transform_matrix(invalid).is_none());
    }

    #[test]
    fn player_rotation_z_matches_itg_screen_space() {
        let mut request = request();
        request.rotation_z_deg = 90.0;
        let matrix = song_lua_player_transform_matrix(request)
            .expect("rotation should produce a player transform");
        let point = transform_point(matrix, [10.0, 0.0]);
        assert!(point[0].abs() <= 0.000_1);
        assert!((point[1] + 10.0).abs() <= 0.000_1);
    }

    #[test]
    fn player_skews_match_itg_screen_space() {
        let mut transform = request();
        transform.skew_x = 0.5;
        let skew_x = song_lua_player_transform_matrix(transform)
            .expect("skewx should produce a player transform");
        let point = transform_point(skew_x, [0.0, -20.0]);
        assert!((point[0] - 10.0).abs() <= 0.000_1);
        assert!((point[1] + 20.0).abs() <= 0.000_1);

        transform = request();
        transform.skew_y = 0.5;
        let skew_y = song_lua_player_transform_matrix(transform)
            .expect("skewy should produce a player transform");
        let point = transform_point(skew_y, [20.0, 0.0]);
        assert!((point[0] - 20.0).abs() <= 0.000_1);
        assert!((point[1] + 10.0).abs() <= 0.000_1);
    }

    #[test]
    fn player_transform_uses_explicit_metrics_for_pivot_and_translation() {
        let mut request = request();
        request.screen_width = 1000.0;
        request.screen_height = 600.0;
        request.screen_center_y = 260.0;
        request.playfield_center_x = 300.0;
        request.target_x = 320.0;
        request.target_y = 230.0;
        let matrix = song_lua_player_transform_matrix(request)
            .expect("translation should produce a player transform");
        let point = transform_point(matrix, [0.0, 0.0]);
        assert!((point[0] - 20.0).abs() <= 0.000_1);
        assert!((point[1] - 30.0).abs() <= 0.000_1);
    }

    #[test]
    fn player_transform_combined_order_matches_legacy_matrix() {
        let request = SongLuaPlayerTransformRequest {
            screen_width: 1000.0,
            screen_height: 600.0,
            screen_center_y: 260.0,
            playfield_center_x: 300.0,
            target_x: 325.0,
            target_y: 220.0,
            rotation_x_deg: 30.0,
            rotation_z_deg: 40.0,
            skew_x: 0.2,
            skew_y: -0.3,
            zoom_x: 1.2,
            zoom_y: 0.8,
            zoom_z: 1.5,
        };
        let actual = song_lua_player_transform_matrix(request)
            .expect("combined transform should produce a player matrix");
        let expected = Matrix4::from_translation(Vector3::new(25.0, 40.0, 0.0))
            * Matrix4::from_translation(Vector3::new(-200.0, 40.0, 0.0))
            * Matrix4::from_rotation_x(30.0_f32.to_radians())
            * Matrix4::from_rotation_z(-40.0_f32.to_radians())
            * song_lua_player_skew_x_matrix(-0.2)
            * song_lua_player_skew_y_matrix(0.3)
            * Matrix4::from_scale(Vector3::new(1.2, 0.8, 1.5))
            * Matrix4::from_translation(Vector3::new(200.0, -40.0, 0.0));

        for (actual, expected) in actual
            .to_cols_array()
            .into_iter()
            .zip(expected.to_cols_array())
        {
            assert!((actual - expected).abs() <= 0.000_1);
        }
    }

    #[test]
    fn player_y_fold_preserves_sprite_fields_and_folds_x() {
        let actor = song_lua_player_y_fold_actor(sprite(120.0), 100.0, 60.0);
        let Actor::Sprite {
            offset, scale, z, ..
        } = actor
        else {
            panic!("fold should preserve the sprite variant");
        };
        assert!((offset[0] - 110.0).abs() <= 0.000_1);
        assert_eq!(offset[1], 12.0);
        assert_eq!(scale, [5.0, 7.0]);
        assert_eq!(z, 11);
    }

    #[test]
    fn player_y_fold_scales_text_x_only() {
        let actor = song_lua_player_y_fold_actor(text(120.0), 100.0, 60.0);
        let Actor::Text {
            offset,
            scale,
            content,
            z,
            ..
        } = actor
        else {
            panic!("fold should preserve the text variant");
        };
        assert!((offset[0] - 110.0).abs() <= 0.000_1);
        assert!((scale[0] - 1.0).abs() <= 0.000_1);
        assert_eq!(scale[1], 3.0);
        assert_eq!(content.as_str(), "fold");
        assert_eq!(z, 12);
    }

    #[test]
    fn player_y_fold_preserves_mesh_and_textured_mesh_payloads() {
        let mesh_vertices: Arc<[MeshVertex]> = Arc::from([MeshVertex {
            pos: [1.0, 2.0],
            color: [0.1, 0.2, 0.3, 0.4],
        }]);
        let mesh = Actor::Mesh {
            align: [0.25, 0.75],
            offset: [140.0, 17.0],
            size: [SizeSpec::Px(31.0), SizeSpec::Fill],
            vertices: Arc::clone(&mesh_vertices),
            visible: false,
            blend: BlendMode::Add,
            z: -7,
        };
        let Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            visible,
            blend,
            z,
        } = song_lua_player_y_fold_actor(mesh, 100.0, 60.0)
        else {
            panic!("fold should preserve the mesh variant");
        };
        assert_eq!(align, [0.25, 0.75]);
        assert!((offset[0] - 120.0).abs() <= 0.000_1);
        assert_eq!(offset[1], 17.0);
        assert!(matches!(size[0], SizeSpec::Px(31.0)));
        assert!(matches!(size[1], SizeSpec::Fill));
        assert!(Arc::ptr_eq(&vertices, &mesh_vertices));
        assert!(!visible);
        assert_eq!(blend, BlendMode::Add);
        assert_eq!(z, -7);

        let local_transform = Matrix4::from_translation(Vector3::new(3.0, 5.0, 7.0))
            * Matrix4::from_scale(Vector3::new(2.0, 4.0, 6.0));
        let texture: Arc<str> = Arc::from("fold-texture");
        let textured_vertices: Arc<[TexturedMeshVertex]> = Arc::from([TexturedMeshVertex {
            pos: [8.0, 9.0, 10.0],
            uv: [0.2, 0.6],
            color: [0.9, 0.8, 0.7, 0.6],
            tex_matrix_scale: [1.25, 0.75],
        }]);
        let geometry_id = deadlib_render::TMeshGeometryId::new(73, textured_vertices.as_ref());
        let textured_mesh = Actor::TexturedMesh {
            align: [0.1, 0.9],
            offset: [140.0, 23.0],
            world_z: 3.5,
            size: [SizeSpec::Fill, SizeSpec::Px(47.0)],
            local_transform,
            texture: Arc::clone(&texture),
            tint: [0.11, 0.22, 0.33, 0.44],
            glow: [0.55, 0.66, 0.77, 0.88],
            vertices: Arc::clone(&textured_vertices),
            geometry_id,
            uv_scale: [1.5, 2.5],
            uv_offset: [0.15, 0.25],
            uv_tex_shift: [0.35, 0.45],
            depth_test: true,
            visible: false,
            blend: BlendMode::Multiply,
            z: -12,
        };
        let Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform: folded_transform,
            texture: folded_texture,
            tint,
            glow,
            vertices,
            geometry_id: folded_geometry_id,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } = song_lua_player_y_fold_actor(textured_mesh, 100.0, 60.0)
        else {
            panic!("fold should preserve the textured-mesh variant");
        };
        assert_eq!(align, [0.1, 0.9]);
        assert!((offset[0] - 120.0).abs() <= 0.000_1);
        assert_eq!(offset[1], 23.0);
        assert_eq!(world_z, 3.5);
        assert!(matches!(size[0], SizeSpec::Fill));
        assert!(matches!(size[1], SizeSpec::Px(47.0)));
        assert_eq!(folded_transform, local_transform);
        assert!(Arc::ptr_eq(&folded_texture, &texture));
        assert_eq!(tint, [0.11, 0.22, 0.33, 0.44]);
        assert_eq!(glow, [0.55, 0.66, 0.77, 0.88]);
        assert!(Arc::ptr_eq(&vertices, &textured_vertices));
        assert_eq!(folded_geometry_id, geometry_id);
        assert_eq!(uv_scale, [1.5, 2.5]);
        assert_eq!(uv_offset, [0.15, 0.25]);
        assert_eq!(uv_tex_shift, [0.35, 0.45]);
        assert!(depth_test);
        assert!(!visible);
        assert_eq!(blend, BlendMode::Multiply);
        assert_eq!(z, -12);
    }

    #[test]
    fn player_y_fold_recurses_owned_children_but_not_shared_children() {
        let owned = Actor::Frame {
            align: [0.5; 2],
            offset: [120.0, 0.0],
            size: [SizeSpec::Px(0.0); 2],
            children: vec![sprite(140.0)],
            background: None,
            z: 0,
        };
        let Actor::Frame {
            offset, children, ..
        } = song_lua_player_y_fold_actor(owned, 100.0, 60.0)
        else {
            panic!("fold should preserve the frame variant");
        };
        assert!((offset[0] - 110.0).abs() <= 0.000_1);
        assert!((actor_x(&children[0]) - 120.0).abs() <= 0.000_1);

        let shared_children: Arc<[Actor]> = Arc::from(vec![sprite(140.0)]);
        let shared = Actor::SharedFrame {
            align: [0.5; 2],
            offset: [120.0, 0.0],
            size: [SizeSpec::Px(0.0); 2],
            children: Arc::clone(&shared_children),
            background: None,
            z: 0,
            tint: [1.0; 4],
            blend: None,
        };
        let Actor::SharedFrame {
            offset, children, ..
        } = song_lua_player_y_fold_actor(shared, 100.0, 60.0)
        else {
            panic!("fold should preserve the shared-frame variant");
        };
        assert!((offset[0] - 110.0).abs() <= 0.000_1);
        assert!(Arc::ptr_eq(&children, &shared_children));
        assert_eq!(actor_x(&children[0]), 140.0);
    }

    #[test]
    fn player_y_fold_recurses_camera_and_shadow_but_keeps_camera_scope_markers() {
        let camera = Actor::Camera {
            view_proj: Matrix4::IDENTITY,
            children: vec![Actor::Shadow {
                len: [1.0, -1.0],
                color: [0.0, 0.0, 0.0, 0.5],
                child: Box::new(sprite(140.0)),
            }],
        };
        let Actor::Camera {
            view_proj,
            children,
        } = song_lua_player_y_fold_actor(camera, 100.0, 60.0)
        else {
            panic!("fold should preserve the camera variant");
        };
        assert_eq!(view_proj, Matrix4::IDENTITY);
        let Actor::Shadow { child, .. } = &children[0] else {
            panic!("fold should preserve the shadow wrapper");
        };
        assert!((actor_x(child) - 120.0).abs() <= 0.000_1);

        let push = Actor::CameraPush {
            view_proj: Matrix4::from_scale(Vector3::splat(2.0)),
        };
        let Actor::CameraPush { view_proj } = song_lua_player_y_fold_actor(push, 100.0, 60.0)
        else {
            panic!("fold should preserve the camera-push variant");
        };
        assert_eq!(view_proj, Matrix4::from_scale(Vector3::splat(2.0)));
        assert!(matches!(
            song_lua_player_y_fold_actor(Actor::CameraPop, 100.0, 60.0),
            Actor::CameraPop
        ));
    }

    #[test]
    fn player_y_fold_ignores_zero_and_nonfinite_inputs() {
        assert_eq!(
            actor_x(&song_lua_player_y_fold_actor(sprite(120.0), 100.0, 0.0)),
            120.0
        );
        assert_eq!(
            actor_x(&song_lua_player_y_fold_actor(sprite(120.0), f32::NAN, 60.0)),
            120.0
        );
        assert_eq!(
            actor_x(&song_lua_player_y_fold_actor(
                sprite(120.0),
                100.0,
                f32::INFINITY,
            )),
            120.0
        );
    }
}
