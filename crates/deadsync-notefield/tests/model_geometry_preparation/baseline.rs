// Frozen from 000a47b8c (0.5.1660).
use super::*;

#[inline(always)]
pub(super) fn build_model_geometry<S: NoteskinSlot>(slot: &S) -> Arc<[TexturedMeshVertex]> {
    let model = slot
        .model()
        .expect("model geometry requested for non-model noteskin slot");
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for &vertex in model.vertices.iter() {
        let vertex = model_vertex_for_sprite(slot.sprite_def(), vertex);
        vertices.push(TexturedMeshVertex {
            normal: [
                vertex.normal[0],
                vertex.normal[1],
                vertex.normal[2],
                f32::from(slot.model_texture_mode()),
            ],
            pos: vertex.pos,
            uv: vertex.uv,
            color: [1.0; 4],
            tex_matrix_scale: vertex.tex_matrix_scale,
        });
    }
    Arc::from(vertices)
}
