// Frozen from ea7f72dae (0.5.1656); unchanged image loading/math helpers are shared.
use super::*;

pub(super) fn model_animation_atlas(
    animation: &ItgTextureAnimation,
    size: [u32; 2],
    grid: [u32; 2],
) -> Result<image::RgbaImage, String> {
    let [width, height] = size;
    let [columns, rows] = grid;
    let atlas_width = width * columns;
    let atlas_height = height * rows;
    let mut atlas = image::RgbaImage::new(atlas_width, atlas_height);
    let mut previous: Option<(&Path, image::RgbaImage)> = None;
    for (index, frame) in animation.frames.iter().enumerate() {
        if previous
            .as_ref()
            .is_none_or(|(path, _)| path.as_os_str() != frame.path.as_os_str())
        {
            // Release the old prepared frame before decoding another image.
            drop(previous.take());
            let image = assets::open_image_fallback(&frame.path)
                .map_err(|error| error.to_string())?
                .into_rgba8();
            let image = if image.dimensions() == (width, height) {
                image
            } else {
                image::imageops::resize(
                    &image,
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                )
            };
            previous = Some((&frame.path, image));
        }
        let image = &previous.as_ref().expect("a frame image was decoded").1;
        image::imageops::replace(
            &mut atlas,
            image,
            i64::from(index as u32 % columns * width),
            i64::from(index as u32 / columns * height),
        );
    }
    Ok(atlas)
}

pub(super) fn freeze_sprite_animation(slot: &mut SpriteSlot) {
    let SpriteSource::Animated {
        texture_key,
        tex_dims,
        frame_size,
        grid,
        frame_indices,
        ..
    } = slot.source.as_ref()
    else {
        return;
    };
    let frame = slot.animation_start_frame;
    let (frame, origin) = frame_indices
        .as_ref()
        .map_or((frame, slot.def.src), |indices| {
            (indices.get(frame).copied().unwrap_or(frame), [0, 0])
        });
    slot.def.src = [
        origin[0] + (frame % grid.0.max(1)) as i32 * frame_size[0],
        origin[1] + (frame / grid.0.max(1)) as i32 * frame_size[1],
    ];
    slot.animation_start_frame = 0;
    slot.animation_start_time = 0.0;
    slot.source = source_from_plan(
        SpriteSourcePlan::Atlas {
            texture_key: texture_key.to_string(),
            tex_dims: *tex_dims,
        },
        &slot.def,
    );
}

fn source_from_plan(plan: SpriteSourcePlan, def: &SpriteDefinition) -> Arc<SpriteSource> {
    match plan {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        } => {
            let texel_scale = texture_texel_scale(tex_dims);
            Arc::new(SpriteSource::Atlas {
                texture_key: texture_key.into(),
                tex_dims,
                texel_scale,
                uv_cache: SpriteAtlasUvCache::new(texel_scale, def),
                cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
                cached_generation: AtomicU64::new(u64::MAX),
                cached_actor_texture: AtomicU64::new(0),
            })
        }
        SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
        } => {
            let texel_scale = texture_texel_scale(tex_dims);
            let frame_timing = frame_durations
                .as_deref()
                .map(|durations| SpriteFrameTiming::new(frame_count, durations));
            let uv_cache = SpriteAnimatedUvCache::new(
                texel_scale,
                def,
                frame_size,
                [grid.0, grid.1],
                frame_count,
                frame_indices.is_some(),
            );
            Arc::new(SpriteSource::Animated {
                texture_key: texture_key.into(),
                tex_dims,
                texel_scale,
                frame_size,
                grid,
                frame_count,
                frame_indices: frame_indices.map(|indices| {
                    if indices.is_empty() {
                        Arc::clone(&SEQUENTIAL_FRAME_INDICES)
                    } else {
                        Arc::from(indices)
                    }
                }),
                rate,
                frame_durations: frame_durations.map(Arc::<[f32]>::from),
                frame_timing,
                uv_cache,
                cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
                cached_generation: AtomicU64::new(u64::MAX),
                cached_actor_texture: AtomicU64::new(0),
            })
        }
    }
}
