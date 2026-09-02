//! Read-only access to production actor math for native ITGmania fixtures.
//!
//! ITGmania serializes matrices by row while glam stores columns. The adapter
//! below transposes storage only; actor coordinates and projected positions are
//! compared in ITGmania's logical top-left screen space without exemptions.

use super::*;

#[derive(Clone, Copy, Debug)]
pub struct EffectSample {
    pub tint: [f32; 4],
    pub glow: [f32; 4],
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct SpriteVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[must_use]
pub fn effect_sample(state: SongLuaOverlayState, time: f32, beat: f32) -> EffectSample {
    let mut sample = EffectSample {
        tint: state.diffuse,
        glow: state.glow,
        position: [state.x, state.y, state.z],
        scale: {
            let [x, y] = song_lua_overlay_axis_scale(state);
            [x, y, song_lua_overlay_z_scale(state)]
        },
        rotation: [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg],
    };
    song_lua_apply_overlay_effect(
        song_lua_overlay_effect_state(state),
        state.rainbow,
        song_lua_overlay_vibrate_magnitude(state),
        time,
        beat,
        &mut sample.tint,
        &mut sample.glow,
        &mut sample.position,
        &mut sample.scale,
        &mut sample.rotation,
    );
    sample
}

#[must_use]
pub fn vibration_magnitude(state: SongLuaOverlayState) -> [f32; 3] {
    song_lua_overlay_vibrate_magnitude(state)
}

#[must_use]
pub fn vibration_sample(mut position: [f32; 3], magnitude: [f32; 3], jitter: [f32; 3]) -> [f32; 3] {
    song_lua_apply_vibration(&mut position, magnitude, jitter);
    position
}

#[must_use]
pub fn actor_matrix(
    position: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
    skew: [f32; 2],
) -> [[f32; 4]; 4] {
    matrix_rows(
        Matrix4::from_translation(Vector3::from(position))
            * song_lua_overlay_local_transform(rotation, skew[0], skew[1])
            * Matrix4::from_scale(Vector3::from(scale)),
    )
}

#[must_use]
pub fn sprite_matrix(
    position: [f32; 3],
    rotation: [f32; 3],
    base_rotation: [f32; 3],
    zoom: [f32; 3],
    base_zoom: [f32; 3],
    size: [f32; 2],
    alignment: [f32; 2],
    skew: [f32; 2],
) -> [[f32; 4]; 4] {
    let rotation = std::array::from_fn(|axis| rotation[axis] + base_rotation[axis]);
    let scale = std::array::from_fn(|axis| zoom[axis] * base_zoom[axis]);
    let align = Vector3::new(
        (0.5 - alignment[0]) * size[0],
        (0.5 - alignment[1]) * size[1],
        0.0,
    );
    matrix_rows(
        Matrix4::from_translation(Vector3::from(position))
            * song_lua_overlay_local_transform(rotation, 0.0, 0.0)
            * Matrix4::from_scale(Vector3::from(scale))
            * Matrix4::from_translation(align)
            * song_lua_overlay_local_transform([0.0; 3], skew[0], skew[1]),
    )
}

#[must_use]
pub fn multiply_matrices(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    matrix_rows(matrix_from_rows(left) * matrix_from_rows(right))
}

#[must_use]
pub fn matrix_rows(matrix: Matrix4) -> [[f32; 4]; 4] {
    let columns = matrix.to_cols_array_2d();
    std::array::from_fn(|row| std::array::from_fn(|column| columns[column][row]))
}

fn matrix_from_rows(rows: [[f32; 4]; 4]) -> Matrix4 {
    Matrix4::from_cols_array_2d(&std::array::from_fn(|column| {
        std::array::from_fn(|row| rows[row][column])
    }))
}

#[must_use]
pub fn view_projection(screen: [u32; 2], fov: f32, vanishpoint: [f32; 2]) -> [[f32; 4]; 4] {
    let _ = deadlib_present::space::ortho_for_window(screen[0], screen[1]);
    matrix_rows(
        song_lua_overlay_view_proj(
            SongLuaOverlayState {
                fov: Some(fov),
                vanishpoint: Some(vanishpoint),
                ..SongLuaOverlayState::default()
            },
            screen[0] as f32,
            screen[1] as f32,
        )
        .expect("valid conformance camera"),
    )
}

#[must_use]
pub fn project_world(view_projection: [[f32; 4]; 4], world: [f32; 4]) -> [f32; 4] {
    (matrix_from_rows(view_projection) * Vector4::from(world)).to_array()
}

#[must_use]
pub fn crop_fade_vertices(state: SongLuaOverlayState, size: [f32; 2]) -> Vec<SpriteVertex> {
    let (center, cropped_size) =
        song_lua_overlay_rect(state, size, 1.0, 1.0, 1.0, 1.0).expect("visible sprite");
    let actor = song_lua_flat_skewed_overlay_actor(
        Arc::from("conformance"),
        state.diffuse,
        BlendMode::Alpha,
        0,
        center,
        cropped_size,
        [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg],
        song_lua_overlay_uvs(state, None, &[], false, false, 0.0, 0.0),
        state,
        false,
        false,
        0.0,
        None,
    )
    .expect("visible conformance sprite");
    let Actor::TexturedMesh { vertices, .. } = actor else {
        panic!("crop/fade conformance actor did not produce a textured mesh");
    };
    vertices
        .iter()
        .map(|vertex| SpriteVertex {
            position: vertex.pos,
            uv: vertex.uv,
            color: vertex.color,
        })
        .collect()
}

#[must_use]
pub fn stable_draw_order(input: &[(String, i32)]) -> Vec<String> {
    let overlays = input
        .iter()
        .map(|(name, draw_order)| SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Actor,
            name: Some(name.clone()),
            parent_index: None,
            initial_state: SongLuaOverlayState {
                draw_order: *draw_order,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut indices = (0..overlays.len()).collect::<Vec<_>>();
    song_lua_sort_static_children(&overlays, &mut indices);
    indices
        .into_iter()
        .map(|index| overlays[index].name.clone().expect("test actor name"))
        .collect()
}
