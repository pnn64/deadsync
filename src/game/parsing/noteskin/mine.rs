use super::{
    AnimationRate, ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelTweenSegment,
    SpriteDefinition, SpriteSlot, SpriteSource,
};
use crate::assets;
use deadlib_platform::dirs;
use deadlib_render::SamplerDesc;
use deadsync_noteskin::mine::{
    MINE_GRADIENT_SAMPLES, MineGradientSampleRegionError, mine_fill_slots as crate_mine_fill_slots,
    mine_gradient_sample_region, mine_gradient_samples, mine_gradient_slot_plan,
    mine_gradient_texture,
};
use log::warn;
use std::path::Path;
use std::sync::{Arc, atomic::AtomicU64};

pub(super) fn mine_fill_slots(mines: &[Option<SpriteSlot>]) -> Vec<Option<SpriteSlot>> {
    crate_mine_fill_slots(mines, |mine| {
        let colors = load_mine_gradient_colors(mine)?;
        Some(build_mine_gradient_slot(&colors))
    })
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let candidate = Path::new("assets").join(texture_key);
    let path = dirs::app_dirs().resolve_asset_path(&candidate.to_string_lossy());
    let image = assets::open_image_fallback(&path).ok()?.to_rgba8();

    let region = match mine_gradient_sample_region(
        [image.width(), image.height()],
        slot.def.src,
        slot.def.size,
        slot.source.frame_size(),
    ) {
        Ok(region) => region,
        Err(MineGradientSampleRegionError::InvalidSlotSize) => {
            warn!("Mine fill slot has invalid size for gradient sampling");
            return None;
        }
        Err(MineGradientSampleRegionError::RegionOutsideTexture) => {
            let src_x = slot.def.src[0].max(0);
            let src_y = slot.def.src[1].max(0);
            warn!("Mine fill region ({src_x}, {src_y}) is outside of texture {texture_key}");
            return None;
        }
        Err(MineGradientSampleRegionError::ZeroSampleSize) => {
            warn!("Mine fill region has zero sample size for texture {texture_key}");
            return None;
        }
    };

    mine_gradient_samples(&image, region.src, region.size, MINE_GRADIENT_SAMPLES)
}

fn build_mine_gradient_slot(colors: &[[f32; 4]]) -> SpriteSlot {
    let plan = mine_gradient_slot_plan(colors);
    if assets::texture_dims(&plan.texture_key).is_none() {
        let texture = mine_gradient_texture(colors);
        assets::register_generated_texture(&plan.texture_key, texture, SamplerDesc::default());
    }

    let source = Arc::new(SpriteSource::Animated {
        texture_key: plan.texture_key.into(),
        tex_dims: plan.tex_dims,
        frame_size: plan.frame_size,
        grid: (plan.frame_count, 1),
        frame_count: plan.frame_count,
        frame_indices: None,
        rate: AnimationRate::FramesPerBeat(1.0),
        frame_durations: None,
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });

    SpriteSlot {
        def: SpriteDefinition {
            src: [0, 0],
            size: plan.frame_size,
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        base_rot_sin_cos: [0.0, 1.0],
        source_size: plan.frame_size,
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        note_color_translate: false,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    }
}
