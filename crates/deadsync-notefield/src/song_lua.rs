use deadsync_noteskin::ModelDrawState;
use glam::{Mat4 as Matrix4, Vec3 as Vector3};

#[inline(always)]
#[must_use]
pub fn song_lua_note_model_draw(
    mut draw: ModelDrawState,
    rotation_xy_deg: [f32; 2],
) -> ModelDrawState {
    if rotation_xy_deg[0] != 0.0 && rotation_xy_deg[0].is_finite() {
        draw.rot[0] += rotation_xy_deg[0];
    }
    if rotation_xy_deg[1].abs() > f32::EPSILON {
        draw.rot[1] += rotation_xy_deg[1];
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
    pub rotation_y_deg: f32,
    pub rotation_z_deg: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
    /// Player Z, toward the camera. Native BeginDraw translates by it.
    pub target_z: f32,
    /// The Player's wrapper states (`AddWrapperState`), outermost first, as one
    /// native screen-space matrix (Y down, origin at the top-left).
    pub wrappers: Matrix4,
}

/// Map a native screen-space matrix (Y down, origin at the top-left) into the
/// Y-up world space centered on the screen used by Player transforms.
#[must_use]
pub fn song_lua_screen_matrix_to_world(
    matrix: Matrix4,
    screen_width: f32,
    screen_height: f32,
) -> Matrix4 {
    let flip = Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));
    let half = Vector3::new(0.5 * screen_width, 0.5 * screen_height, 0.0);
    Matrix4::from_translation(Vector3::new(-half.x, half.y, 0.0))
        * flip
        * matrix
        * flip
        * Matrix4::from_translation(Vector3::new(half.x, -half.y, 0.0))
}

#[must_use]
pub const fn song_lua_player_skew_x_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        amount, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

#[must_use]
pub const fn song_lua_player_skew_y_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, amount, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

#[must_use]
pub fn song_lua_player_transform_matrix(request: SongLuaPlayerTransformRequest) -> Option<Matrix4> {
    let SongLuaPlayerTransformRequest {
        screen_width,
        screen_height,
        screen_center_y,
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_y_deg,
        rotation_z_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
        target_z,
        wrappers,
    } = request;
    if !target_z.is_finite() || !wrappers.is_finite() {
        return None;
    }
    if !screen_width.is_finite()
        || !screen_height.is_finite()
        || !screen_center_y.is_finite()
        || !playfield_center_x.is_finite()
        || !target_x.is_finite()
        || !target_y.is_finite()
        || !rotation_x_deg.is_finite()
        || !rotation_y_deg.is_finite()
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
    let rotation_y_deg = if rotation_y_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_y_deg
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
        && rotation_y_deg.abs() <= f32::EPSILON
        && rotation_z_deg.abs() <= f32::EPSILON
        && skew_x.abs() <= f32::EPSILON
        && skew_y.abs() <= f32::EPSILON
        && (zoom_x - 1.0).abs() <= f32::EPSILON
        && (zoom_y - 1.0).abs() <= f32::EPSILON
        && (zoom_z - 1.0).abs() <= f32::EPSILON
        && translate_x.abs() <= f32::EPSILON
        && translate_y.abs() <= f32::EPSILON
        && target_z.abs() <= f32::EPSILON
        && wrappers == Matrix4::IDENTITY
    {
        return None;
    }

    let pivot_x = 0.5f32.mul_add(-screen_width, playfield_center_x);
    let pivot_y = 0.5f32.mul_add(screen_height, -screen_center_y);
    // RageMatrixRotationXYZ is Rx(-X) * Ry(-Y) * Rz(+Z) for column vectors
    // in native screen coordinates. Conjugating by the Y flip into DeadSync
    // world space gives Rx(+X) * Ry(-Y) * Rz(-Z), preserving rotation depth.
    let rotation_y_deg = -rotation_y_deg;
    let rotation_z_deg = -rotation_z_deg;
    let skew_x = -skew_x;
    let skew_y = -skew_y;
    // Actor::Draw begins the highest-index wrapper first, outside the Player.
    let wrappers = if wrappers == Matrix4::IDENTITY {
        Matrix4::IDENTITY
    } else {
        song_lua_screen_matrix_to_world(wrappers, screen_width, screen_height)
    };
    Some(
        wrappers
            * Matrix4::from_translation(Vector3::new(translate_x, translate_y, target_z))
            * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
            * Matrix4::from_rotation_x(rotation_x_deg.to_radians())
            * Matrix4::from_rotation_y(rotation_y_deg.to_radians())
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
    };
    use glam::{Mat4 as Matrix4, Vec3 as Vector3};

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
            rotation_y_deg: 0.0,
            rotation_z_deg: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
            target_z: 0.0,
            wrappers: Matrix4::IDENTITY,
        }
    }

    fn transform_point(matrix: Matrix4, local: [f32; 2]) -> [f32; 2] {
        let point = matrix.transform_point3(Vector3::new(local[0], local[1], 0.0));
        [point.x, point.y]
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
        invalid = request();
        invalid.rotation_y_deg = f32::NAN;
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
    fn player_rotation_y_preserves_depth_and_uses_field_pivot() {
        let mut request = request();
        request.playfield_center_x = 300.0;
        request.target_x = 325.0;
        request.target_y = 220.0;
        request.target_z = 40.0;
        request.rotation_y_deg = 90.0;
        let matrix = song_lua_player_transform_matrix(request)
            .expect("Y rotation must produce a full Player matrix");
        let pivot = Vector3::new(300.0 - SCREEN_WIDTH / 2.0, 0.0, 0.0);
        let destination = pivot + Vector3::new(25.0, 20.0, 40.0);
        // RageMatrixRotationY(+90) sends native +X toward the camera (+Z)
        // and native +Z toward -X. A cosine-only fold loses both depth terms.
        for (point, expected) in [
            (pivot, destination),
            (pivot + Vector3::X * 32.0, destination + Vector3::Z * 32.0),
            (pivot + Vector3::Z * 32.0, destination - Vector3::X * 32.0),
        ] {
            let actual = matrix.transform_point3(point);
            assert!(
                (actual - expected).abs().max_element() < 0.000_1,
                "point {point}: {actual} vs {expected}"
            );
        }
    }

    #[test]
    fn player_xyz_rotation_matches_native_three_dimensional_basis() {
        let mut request = request();
        request.rotation_x_deg = 30.0;
        request.rotation_y_deg = 60.0;
        request.rotation_z_deg = 90.0;
        let matrix = song_lua_player_transform_matrix(request).unwrap();
        // Closed-form columns of ITGmania RageMatrixRotationXYZ(30, 60, 90),
        // conjugated by the screen/world Y flip. Using 32-unit basis vectors
        // catches the missing Y-axis depth and combined XYZ rotation order.
        let root_three = 3.0_f32.sqrt();
        for (axis, expected) in [
            (Vector3::X, Vector3::new(0.0, -16.0 * root_three, -16.0)),
            (Vector3::Y, Vector3::new(16.0, -8.0 * root_three, 24.0)),
            (
                Vector3::Z,
                Vector3::new(-16.0 * root_three, -8.0, 8.0 * root_three),
            ),
        ] {
            let actual = matrix.transform_vector3(axis * 32.0);
            assert!(
                (actual - expected).abs().max_element() < 0.000_1,
                "axis {axis}: {actual} vs {expected}"
            );
        }
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
    fn player_transform_translates_z_and_applies_wrappers_outside() {
        let mut request = request();
        request.target_z = 40.0;
        let matrix = song_lua_player_transform_matrix(request).expect("Z alone moves the Player");
        let origin = matrix.transform_point3(Vector3::ZERO);
        assert!((origin - Vector3::new(0.0, 0.0, 40.0)).length() <= 0.000_1, "{origin}");

        // A native wrapper (Y down, top-left origin) zooms Z outside the
        // Player's own translation, then moves right and down the screen.
        request.wrappers = Matrix4::from_translation(Vector3::new(100.0, 30.0, 0.0))
            * Matrix4::from_scale(Vector3::new(1.0, 1.0, 2.0));
        let matrix = song_lua_player_transform_matrix(request).expect("wrapped Player");
        let origin = matrix.transform_point3(Vector3::ZERO);
        assert!((origin - Vector3::new(100.0, -30.0, 80.0)).length() <= 0.000_1, "{origin}");
    }

    #[test]
    fn player_transform_combined_order_matches_native_xyz() {
        let request = SongLuaPlayerTransformRequest {
            screen_width: 1000.0,
            screen_height: 600.0,
            screen_center_y: 260.0,
            playfield_center_x: 300.0,
            target_x: 325.0,
            target_y: 220.0,
            rotation_x_deg: 30.0,
            rotation_y_deg: 20.0,
            rotation_z_deg: 40.0,
            skew_x: 0.2,
            skew_y: -0.3,
            zoom_x: 1.2,
            zoom_y: 0.8,
            zoom_z: 1.5,
            target_z: 0.0,
            wrappers: Matrix4::IDENTITY,
        };
        let actual = song_lua_player_transform_matrix(request)
            .expect("combined transform should produce a player matrix");
        let expected = Matrix4::from_translation(Vector3::new(25.0, 40.0, 0.0))
            * Matrix4::from_translation(Vector3::new(-200.0, 40.0, 0.0))
            * Matrix4::from_rotation_x(30.0_f32.to_radians())
            * Matrix4::from_rotation_y(-20.0_f32.to_radians())
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

}
