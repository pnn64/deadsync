//! Frozen sprite and frame preparation from 7753a0a0f.
use super::*;

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) fn prepare_sprite_vertices(
    proj: &Matrix4,
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    width: usize,
    height: usize,
) -> Option<[ScreenVertex; 4]> {
    if width == 0 || height == 0 {
        return None;
    }

    let mut adjusted_center = center;
    if local_offset[0] != 0.0 || local_offset[1] != 0.0 {
        let s = local_offset_rot_sin_cos[0];
        let c = local_offset_rot_sin_cos[1];
        let ox = c.mul_add(local_offset[0], -(s * local_offset[1]));
        let oy = s.mul_add(local_offset[0], c * local_offset[1]);
        adjusted_center[0] += ox;
        adjusted_center[1] += oy;
    }

    const POS: [(f32, f32); 4] = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    const UV_BASE: [(f32, f32); 4] = [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)];

    let mut v = [ScreenVertex {
        x: 0.0,
        y: 0.0,
        u: 0.0,
        v: 0.0,
    }; 4];

    for i in 0..4 {
        let (lx, ly) = POS[i];
        let local_x = lx * size[0];
        let local_y = ly * size[1];
        let world = Vector4::new(
            rot_sin_cos[1].mul_add(local_x, -(rot_sin_cos[0] * local_y) + adjusted_center[0]),
            rot_sin_cos[0].mul_add(local_x, rot_sin_cos[1].mul_add(local_y, adjusted_center[1])),
            adjusted_center[2],
            1.0,
        );
        let clip = *proj * world;
        if clip.w == 0.0 {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;

        let sx = f32::midpoint(ndc_x, 1.0) * (width as f32);
        let sy = ((1.0 - ndc_y) * 0.5) * (height as f32);

        let (u0, v0) = UV_BASE[i];
        let u = u0.mul_add(uv_scale[0], uv_offset[0]);
        let vv = v0.mul_add(uv_scale[1], uv_offset[1]);

        v[i] = ScreenVertex {
            x: sx,
            y: sy,
            u,
            v: vv,
        };
    }

    Some(v)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_objects(
    frame: SoftwarePass<'_>,
    default_proj: Matrix4,
    textures: &(impl TextureLookup + Sync),
    width: usize,
    height: usize,
    prepared: &mut Vec<PreparedObject>,
    mesh_triangles: &mut Vec<PreparedTriangle<ScreenVertexColor>>,
    tmesh_triangles: &mut Vec<PreparedTriangle<ScreenVertexTexColor>>,
    stage_meshes: bool,
) -> u32 {
    prepared.clear();
    prepared.reserve(
        frame
            .sprite_instances
            .len()
            .saturating_add(frame.tmesh_instances.len()),
    );
    mesh_triangles.clear();
    tmesh_triangles.clear();
    let mut fixed_vertices = 0u32;

    for op in frame.ops {
        if frame.depth && matches!(op, DrawOp::TexturedMesh(run) if run.clear_depth) {
            prepared.push(PreparedObject::ClearDepth);
        }
        match *op {
            DrawOp::Sprite(run) => {
                if textures.software_texture(run.texture_handle).is_none() {
                    continue;
                }
                let projection = frame
                    .cameras
                    .get(run.camera as usize)
                    .copied()
                    .unwrap_or(default_proj);
                let end = run.instance_start.saturating_add(run.instance_count);
                for sprite_index in run.instance_start..end {
                    let Some(sprite) = frame.sprite_instances.get(sprite_index as usize) else {
                        continue;
                    };
                    if sprite.tint[3] <= 0.0 {
                        continue;
                    }
                    let Some(vertices) = prepare_sprite_vertices(
                        &projection,
                        sprite.center,
                        sprite.size,
                        sprite.rot_sin_cos,
                        sprite.uv_scale,
                        sprite.uv_offset,
                        sprite.local_offset,
                        sprite.local_offset_rot_sin_cos,
                        width,
                        height,
                    ) else {
                        continue;
                    };
                    fixed_vertices = fixed_vertices.saturating_add(4);
                    prepared.push(PreparedObject::Sprite {
                        rows: sprite_rows(&vertices, height),
                        inv_denom: sprite_inv_denom(&vertices),
                        vertices,
                        tint: sprite.tint,
                        texture_mask: sprite.texture_mask != 0.0,
                        blend: run.blend,
                        texture_handle: run.texture_handle,
                    });
                }
            }
            DrawOp::Mesh(run) => {
                let projection = frame
                    .cameras
                    .get(run.camera as usize)
                    .copied()
                    .unwrap_or(default_proj);
                let start = run.vertex_start as usize;
                let end = start.saturating_add(run.vertex_count as usize);
                let Some(vertices) = frame.mesh_vertices.get(start..end) else {
                    continue;
                };
                if !stage_meshes
                    || vertices.len().div_ceil(3)
                        > mesh_triangles
                            .capacity()
                            .saturating_sub(mesh_triangles.len())
                {
                    prepared.push(PreparedObject::DirectMesh {
                        vertex_start: run.vertex_start,
                        vertex_count: run.vertex_count,
                        projection,
                        blend: run.blend,
                    });
                    continue;
                }
                let Some((triangle_start, triangle_count, projected_count, rows)) =
                    prepare_mesh_triangles(
                        mesh_triangles,
                        prepared.len() as u32,
                        &projection,
                        vertices,
                        width,
                        height,
                    )
                else {
                    continue;
                };
                fixed_vertices = fixed_vertices.saturating_add(projected_count);
                prepared.push(PreparedObject::Mesh {
                    triangle_start,
                    triangle_count,
                    rows,
                    blend: run.blend,
                });
            }
            DrawOp::TexturedMesh(run) => {
                let Some(geometry) = frame.tmesh_geometries.get(run.geometry as usize) else {
                    continue;
                };
                if textures.software_texture(run.texture_handle).is_none() {
                    continue;
                }
                let projection = frame
                    .cameras
                    .get(run.camera as usize)
                    .copied()
                    .unwrap_or(default_proj);
                let end = run.instance_start.saturating_add(run.instance_count);
                for instance_index in run.instance_start..end {
                    let Some(instance) = frame.tmesh_instances.get(instance_index as usize) else {
                        continue;
                    };
                    let mvp = projection * instance.transform();
                    // One source triangle can become two after near-plane clipping.
                    if !stage_meshes
                        || geometry.vertices.len().div_ceil(3).saturating_mul(2)
                            > tmesh_triangles
                                .capacity()
                                .saturating_sub(tmesh_triangles.len())
                        || geometry
                            .vertices
                            .first()
                            .is_some_and(|vertex| vertex.normal[3] != 0.0)
                    {
                        prepared.push(PreparedObject::DirectTexturedMesh {
                            geometry: run.geometry,
                            instance: instance_index,
                            mvp,
                            blend: run.blend,
                            depth_test: frame.depth && run.depth_test,
                            depth_write: frame.depth && run.depth_write,
                            depth_test_on_fail: frame.depth && run.depth_test_on_fail,
                            texture_handle: run.texture_handle,
                            sampler: run.sampler,
                        });
                        continue;
                    }
                    let Some((triangle_start, triangle_count, projected_count, rows)) =
                        prepare_tmesh_triangles(
                            tmesh_triangles,
                            prepared.len() as u32,
                            &mvp,
                            instance.tint,
                            instance.uv_scale,
                            instance.uv_offset,
                            instance.uv_tex_shift,
                            geometry.vertices.as_ref(),
                            width,
                            height,
                            instance.cull_mode,
                        )
                    else {
                        continue;
                    };
                    fixed_vertices = fixed_vertices.saturating_add(projected_count);
                    prepared.push(PreparedObject::TexturedMesh {
                        triangle_start,
                        triangle_count,
                        rows,
                        texture_mask: instance.texture_mask != 0.0,
                        blend: run.blend,
                        depth_test: frame.depth && run.depth_test,
                        depth_write: frame.depth && run.depth_write,
                        depth_test_on_fail: frame.depth && run.depth_test_on_fail,
                        texture_handle: run.texture_handle,
                        sampler: run.sampler,
                    });
                }
            }
        }
    }
    fixed_vertices
}
