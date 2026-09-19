// Frozen from adfd8870b for output parity and CPU comparisons.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn build_textured_mesh_actor<T: TextureContext + ?Sized>(
    mesh: TexturedMeshActorView<'_>,
    parent: SmRect,
    m: &Metrics,
    base_z: i16,
    camera: u8,
    style: ComposeStyle,
    x_fold: Option<ActorXFold>,
    order_counter: &mut u32,
    out: &mut FrameBuilder,
    texture_cache: &mut TextureLookupCache,
    texture_ctx: &T,
) {
    if !mesh.visible || mesh.vertices.is_empty() {
        return;
    }

    let offset = x_fold.map_or(mesh.offset, |fold| fold.offset(mesh.offset));
    let rect = place_rect(parent, mesh.align, offset, mesh.size);
    let base_x = m.left + rect.x;
    let base_y = m.top - rect.y;
    let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, mesh.world_z))
        * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0))
        * mesh.local_transform;
    let texture_key = mesh.texture.as_ref();
    let texture_key_ptr = str_ptr(texture_key);
    let texture_handle = texture_cache.texture_handle(texture_ctx, texture_key_ptr, texture_key);
    let actor_blend = style.blend.unwrap_or(mesh.blend);
    let layer = base_z.saturating_add(mesh.z);
    let base_order = *order_counter;
    *order_counter = base_order.saturating_add(1);
    // A glow-only mesh has no diffuse pass, just like a glow-only Sprite.
    if mesh.tint[3] > 0.0 {
        out.push_textured_mesh(
            texture_handle,
            base_order,
            layer,
            actor_blend,
            camera,
            TexturedMeshPayload {
                instance: renderer::TexturedMeshInstanceRaw::new(
                    transform,
                    mul_rgba(mesh.tint, style.tint),
                    mesh.uv_scale,
                    mesh.uv_offset,
                    mesh.uv_tex_shift,
                    false,
                ),
                vertices: mesh.vertices.clone_for_render(),
                geom_cache_key: mesh.geom_cache_key,
                depth_test: mesh.depth_test,
            },
        );
    }
    if mesh.glow[3] > 0.0001 {
        let glow_order = *order_counter;
        *order_counter = glow_order.saturating_add(1);
        out.push_textured_mesh(
            texture_handle,
            glow_order,
            layer,
            actor_blend,
            camera,
            TexturedMeshPayload {
                instance: renderer::TexturedMeshInstanceRaw::new(
                    transform,
                    mul_rgba(mesh.glow, style.tint),
                    mesh.uv_scale,
                    mesh.uv_offset,
                    mesh.uv_tex_shift,
                    true,
                ),
                vertices: mesh.vertices.clone_for_render(),
                geom_cache_key: mesh.geom_cache_key,
                depth_test: mesh.depth_test,
            },
        );
    }
}
