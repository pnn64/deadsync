use deadlib_render_core::{
    ProjectionMatrix as Mat4, TexturedMeshInstanceRaw, TexturedMeshVertex, sphere_texture_uv,
    textured_mesh_uvs,
};

fn rows(matrix: Mat4) -> [[f32; 4]; 3] {
    [
        matrix.row(0).to_array(),
        matrix.row(1).to_array(),
        matrix.row(2).to_array(),
    ]
}

#[test]
fn sphere_coordinates_follow_eye_space_normals() {
    let mut view = Mat4::IDENTITY;
    view.w_axis.z = -100.0;
    let uv = sphere_texture_uv([0.0; 3], [0.0, 0.0, 1.0], rows(view));
    assert_eq!(uv, [0.5; 2]);
    view *= Mat4::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let uv = sphere_texture_uv([0.0; 3], [0.0, 0.0, 1.0], rows(view));
    assert!((uv[0] - 0.853_553_4).abs() < 1e-6);
    assert!((uv[1] - 0.5).abs() < 1e-6);
    let uv = sphere_texture_uv(
        [0.0; 3],
        [0.0, 0.0, 1.0],
        rows(Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2) * view),
    );
    assert!((uv[0] - 0.5).abs() < 1e-6);
    assert!((uv[1] - 0.853_553_4).abs() < 1e-6);
}

#[test]
fn sphere_normals_use_inverse_transpose_and_survive_mirroring() {
    let mut view = Mat4::from_cols_array(&[
        2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -100.0, 1.0,
    ]);
    let uv = sphere_texture_uv([0.0; 3], [1.0, 0.0, 1.0], rows(view));
    assert!((uv[0] - 0.723_606_8).abs() < 1e-6);
    view.x_axis.x = -2.0;
    let mirrored = sphere_texture_uv([0.0; 3], [1.0, 0.0, 1.0], rows(view));
    assert!((mirrored[0] - (1.0 - uv[0])).abs() < 1e-6);
}

#[test]
fn material_stages_generate_coordinates_independently_and_glow_uses_authored_uvs() {
    let mut instance = TexturedMeshInstanceRaw::new(
        Mat4::IDENTITY,
        [1.0; 4],
        [0.5; 2],
        [0.25; 2],
        [0.1, 0.2],
        false,
    );
    instance.sphere_rows = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, -100.0],
    ];
    instance.additive_uv = [0.25, 0.5, 0.1, 0.2];
    let mut vertex = TexturedMeshVertex {
        normal: [0.0, 0.0, 1.0, 6.0],
        uv: [0.2, 0.4],
        ..Default::default()
    };
    let uv = textured_mesh_uvs(vertex, instance);
    assert!((uv[0][0] - 0.35).abs() < 1e-6);
    assert!((uv[1][0] - 0.225).abs() < 1e-6);
    vertex.normal[3] = 5.0;
    assert_eq!(textured_mesh_uvs(vertex, instance)[0], [0.5; 2]);
    instance.texture_mask = 1.0;
    assert_eq!(textured_mesh_uvs(vertex, instance)[0], uv[0]);
}
