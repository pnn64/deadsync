// Starting main 6324bcb62: original CPU UV calculations.
use deadlib_render_core::{TexturedMeshInstanceRaw, TexturedMeshVertex};

pub(super) fn sphere_texture_uv(pos: [f32; 3], normal: [f32; 3], rows: [[f32; 4]; 3]) -> [f32; 2] {
    let matrix = glam::Mat3::from_cols(
        glam::Vec3::new(rows[0][0], rows[1][0], rows[2][0]),
        glam::Vec3::new(rows[0][1], rows[1][1], rows[2][1]),
        glam::Vec3::new(rows[0][2], rows[1][2], rows[2][2]),
    );
    let cofactor = glam::Mat3::from_cols(
        matrix.y_axis.cross(matrix.z_axis),
        matrix.z_axis.cross(matrix.x_axis),
        matrix.x_axis.cross(matrix.y_axis),
    );
    let n =
        (cofactor * glam::Vec3::from(normal) * matrix.determinant().signum()).normalize_or_zero();
    let p = glam::Vec4::new(pos[0], pos[1], pos[2], 1.0);
    let eye = glam::Vec3::new(
        glam::Vec4::from(rows[0]).dot(p),
        glam::Vec4::from(rows[1]).dot(p),
        glam::Vec4::from(rows[2]).dot(p),
    )
    .normalize_or_zero();
    let reflection = eye - 2.0 * n * eye.dot(n);
    let denominator = (2.0 * (reflection + glam::Vec3::Z).length()).max(1e-20);
    [
        reflection.x / denominator + 0.5,
        reflection.y / denominator + 0.5,
    ]
}

pub(super) fn textured_mesh_uvs(
    vertex: TexturedMeshVertex,
    instance: TexturedMeshInstanceRaw,
) -> [[f32; 2]; 2] {
    let mode = if instance.texture_mask > 0.5 {
        0
    } else {
        vertex.normal[3] as u8
    };
    let sphere = if mode & 3 != 0 {
        sphere_texture_uv(
            vertex.pos,
            [vertex.normal[0], vertex.normal[1], vertex.normal[2]],
            instance.sphere_rows,
        )
    } else {
        vertex.uv
    };
    let uv = if mode & 1 != 0 { sphere } else { vertex.uv };
    let additive = if mode & 2 != 0 { sphere } else { vertex.uv };
    [
        std::array::from_fn(|axis| {
            uv[axis] * instance.uv_scale[axis]
                + instance.uv_offset[axis]
                + instance.uv_tex_shift[axis] * (vertex.tex_matrix_scale[axis] - 1.0)
        }),
        std::array::from_fn(|axis| {
            additive[axis] * instance.additive_uv[axis] + instance.additive_uv[axis + 2]
        }),
    ]
}
