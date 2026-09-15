// Frozen finish_frame from 4e8446163 for before/after tests and benchmarks.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn finish_frame<const TRACK_SPRITE_RUNS: bool>(
    builder: &mut FrameBuilder,
    mesh_vertices: &mut Vec<renderer::MeshVertex>,
    tmesh_instances: &mut Vec<renderer::TexturedMeshInstanceRaw>,
    tmesh_geometries: &mut Vec<renderer::TexturedMeshGeometry>,
    ops: &mut Vec<renderer::DrawOp>,
    geom_map: &mut HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
) -> SpriteGatherStats {
    if ops.capacity() < builder.items.len() {
        ops.reserve(builder.items.len());
    }
    let mut sprite_stats = SpriteGatherStats::default();
    let mut previous_sprite = None;
    let mut cursor = 0usize;
    while cursor < builder.items.len() {
        let item = builder.items[cursor];
        let texture_handle = item.texture_handle;
        let blend = item.blend;
        let camera = item.camera;
        match item.kind {
            DrawKind::Sprite => {
                let instance_start = item.payload_index;
                if texture_handle == renderer::INVALID_TEXTURE_HANDLE {
                    cursor += 1;
                    continue;
                }
                let mut instance_count = 1u32;
                while let Some(next) = builder.items.get(cursor + instance_count as usize) {
                    if next.z != item.z
                        || next.texture_handle != texture_handle
                        || next.blend != blend
                        || next.camera != camera
                        || next.kind != DrawKind::Sprite
                        || next.payload_index != instance_start.saturating_add(instance_count)
                    {
                        break;
                    }
                    instance_count = instance_count.saturating_add(1);
                }
                let run = renderer::SpriteRun {
                    instance_start,
                    instance_count,
                    blend,
                    texture_handle,
                    camera,
                };
                if TRACK_SPRITE_RUNS {
                    sprite_stats.sprites = sprite_stats.sprites.saturating_add(instance_count);
                    sprite_stats.runs_before = sprite_stats.runs_before.saturating_add(1);
                    sprite_stats.runs_after =
                        sprite_stats
                            .runs_after
                            .saturating_add(u32::from(previous_sprite.is_none_or(|previous| {
                                !sprite_runs_are_compatible(previous, run)
                            })));
                    previous_sprite = Some(run);
                }
                ops.push(renderer::DrawOp::Sprite(run));
                cursor = cursor.saturating_add(instance_count as usize);
            }
            DrawKind::Mesh => {
                let MeshPayload {
                    transform,
                    tint,
                    vertices,
                } = builder.meshes[item.payload_index as usize]
                    .take()
                    .expect("draw item references live mesh payload");
                if vertices.is_empty() {
                    cursor += 1;
                    continue;
                }
                let vertex_start = saturating_u32(mesh_vertices.len());
                append_mesh_vertices(mesh_vertices, &transform, tint, vertices.as_ref());
                let mut object_count = 1usize;
                while let Some(next) = builder.items.get(cursor + object_count).copied() {
                    let compatible = next.z == item.z
                        && next.blend == blend
                        && next.camera == camera
                        && next.kind == DrawKind::Mesh
                        && builder.meshes[next.payload_index as usize]
                            .as_ref()
                            .is_some_and(|payload| !payload.vertices.is_empty());
                    if !compatible {
                        break;
                    }
                    let MeshPayload {
                        transform,
                        tint,
                        vertices,
                    } = builder.meshes[next.payload_index as usize]
                        .take()
                        .expect("draw item references live mesh payload");
                    append_mesh_vertices(mesh_vertices, &transform, tint, vertices.as_ref());
                    object_count += 1;
                }
                ops.push(renderer::DrawOp::Mesh(renderer::MeshRun {
                    vertex_start,
                    vertex_count: saturating_u32(mesh_vertices.len()).saturating_sub(vertex_start),
                    blend,
                    camera,
                }));
                if TRACK_SPRITE_RUNS {
                    previous_sprite = None;
                }
                cursor += object_count;
            }
            DrawKind::TexturedMesh => {
                let TexturedMeshPayload {
                    instance,
                    vertices,
                    geom_cache_key,
                    depth_test,
                } = builder.textured_meshes[item.payload_index as usize]
                    .take()
                    .expect("draw item references live textured-mesh payload");
                if vertices.is_empty() || texture_handle == renderer::INVALID_TEXTURE_HANDLE {
                    cursor += 1;
                    continue;
                }
                let identity = tmesh_identity(&vertices, geom_cache_key);
                let geometry = push_tmesh_geometry(
                    vertices,
                    geom_cache_key,
                    identity,
                    tmesh_geometries,
                    geom_map,
                );
                let instance_start = saturating_u32(tmesh_instances.len());
                tmesh_instances.push(instance);
                let mut object_count = 1usize;
                while identity.is_some()
                    && let Some(next) = builder.items.get(cursor + object_count).copied()
                {
                    let compatible = next.z == item.z
                        && next.texture_handle == texture_handle
                        && next.blend == blend
                        && next.camera == camera
                        && next.kind == DrawKind::TexturedMesh
                        && builder.textured_meshes[next.payload_index as usize]
                            .as_ref()
                            .is_some_and(|payload| {
                                payload.depth_test == depth_test
                                    && tmesh_identity(&payload.vertices, payload.geom_cache_key)
                                        == identity
                            });
                    if !compatible {
                        break;
                    }
                    let payload = builder.textured_meshes[next.payload_index as usize]
                        .take()
                        .expect("draw item references live textured-mesh payload");
                    tmesh_instances.push(payload.instance);
                    object_count += 1;
                }
                ops.push(renderer::DrawOp::TexturedMesh(renderer::TexturedMeshRun {
                    geometry,
                    instance_start,
                    instance_count: saturating_u32(tmesh_instances.len())
                        .saturating_sub(instance_start),
                    blend,
                    texture_handle,
                    camera,
                    depth_test,
                }));
                if TRACK_SPRITE_RUNS {
                    previous_sprite = None;
                }
                cursor += object_count;
            }
        }
    }
    builder.clear();
    sprite_stats
}
