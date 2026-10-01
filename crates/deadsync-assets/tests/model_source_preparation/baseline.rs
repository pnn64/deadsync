// Frozen from 000a47b8c (0.5.1660). The data helper isolates the unchanged planning tail.
use super::*;

#[inline(always)]
#[must_use]
#[allow(clippy::use_self)]
pub fn build_model_geometry(slot: &SpriteSlot) -> Arc<[TexturedMeshVertex]> {
    let model = slot
        .model
        .as_ref()
        .expect("model geometry requested for non-model noteskin slot");
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for vertex in model.vertices.iter().copied() {
        let vertex = model_vertex_for_sprite(&slot.def, vertex);
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

pub(super) fn model_animation_source(
    animation: &ItgTextureAnimation,
) -> Result<Arc<SpriteSource>, String> {
    let count = animation.frames.len();
    let first = animation.frames.first().ok_or("empty frame sequence")?;
    let (width, height) = image_dimensions(&first.path).map_err(|error| error.to_string())?;
    let columns = (count as f32).sqrt().ceil() as u32;
    let rows = (count as u32).div_ceil(columns.max(1));
    let atlas_width = width.checked_mul(columns).ok_or("atlas width overflow")?;
    let atlas_height = height.checked_mul(rows).ok_or("atlas height overflow")?;
    if width == 0
        || height == 0
        || atlas_width > 8192
        || atlas_height > 8192
        || u64::from(atlas_width) * u64::from(atlas_height) > 16 * 1024 * 1024
    {
        return Err("frame atlas exceeds 64 MiB or 8192px".into());
    }
    let key = format!(
        "{}#model-frames",
        crate::textures::canonical_texture_key(&animation.path)
    );
    if assets::texture_dims(&key).is_none() {
        let atlas = model_animation_atlas(animation, [width, height], [columns, rows])?;
        assets::register_generated_texture(
            &key,
            atlas,
            crate::textures::model_texture_sampler(&key),
        );
    }
    let mut plan = generated_animation_sprite_slot_plan(
        key,
        (atlas_width, atlas_height),
        [width as i32, height as i32],
        count,
        AnimationRate::FramesPerSecond(1.0),
        false,
    );
    if let SpriteSourcePlan::Animated {
        grid,
        frame_durations,
        ..
    } = &mut plan.source
    {
        *grid = (columns as usize, rows as usize);
        *frame_durations = Some(animation.frames.iter().map(|frame| frame.delay).collect());
    }
    Ok(source_from_plan(plan.source, &plan.def))
}

pub(super) fn model_animation_source_data(
    key: String,
    tex_dims: (u32, u32),
    frame_size: [i32; 2],
    grid: (usize, usize),
    animation: &ItgTextureAnimation,
) -> Arc<SpriteSource> {
    let count = animation.frames.len();
    let (atlas_width, atlas_height) = tex_dims;
    let [width, height] = frame_size;
    let (columns, rows) = grid;
    let mut plan = generated_animation_sprite_slot_plan(
        key,
        (atlas_width, atlas_height),
        [width as i32, height as i32],
        count,
        AnimationRate::FramesPerSecond(1.0),
        false,
    );
    if let SpriteSourcePlan::Animated {
        grid,
        frame_durations,
        ..
    } = &mut plan.source
    {
        *grid = (columns as usize, rows as usize);
        *frame_durations = Some(animation.frames.iter().map(|frame| frame.delay).collect());
    }
    source_from_plan(plan.source, &plan.def)
}

pub(super) fn apply_all_state_delays(slot: &mut SpriteSlot, delay: f32, beat_based: bool) {
    let SpriteSource::Animated {
        texture_key,
        tex_dims,
        frame_size,
        grid,
        frame_count,
        frame_indices,
        ..
    } = slot.source.as_ref()
    else {
        return;
    };
    let frame_count = (*frame_count).max(1);
    let delay = delay.max(1e-6);
    // Ordinary sprite sheets fit this stack scratch; only the resulting Arc
    // is allocated. Large authored animations keep the growable fallback.
    let durations: Arc<[f32]> = if frame_count <= 64 {
        Arc::from(&[delay; 64][..frame_count])
    } else {
        Arc::from(vec![delay; frame_count])
    };
    let rate = if beat_based {
        AnimationRate::FramesPerBeat(1.0 / delay)
    } else {
        AnimationRate::FramesPerSecond(1.0 / delay)
    };
    let source = animated_source(
        Arc::clone(texture_key),
        *tex_dims,
        *frame_size,
        *grid,
        frame_count,
        frame_indices.clone(),
        rate,
        Some(durations),
        &slot.def,
    );
    replace_sprite_source(&mut slot.source, source);
}
