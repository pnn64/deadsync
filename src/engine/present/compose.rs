use crate::assets;
use deadsync_present::actors;
use deadsync_present::compose as present_compose;
use deadsync_present::font;
use deadsync_present::space::Metrics;
use deadsync_render::{RenderList, TextureHandle};
use std::collections::HashMap;

pub use deadsync_present::compose::{
    ComposeScratch, NullTextureContext, TextLayoutCache, TextLayoutFrameStats, TextureContext,
    TextureMeta,
};

struct AssetTextureContext;

impl present_compose::TextureContext for AssetTextureContext {
    #[inline(always)]
    fn texture_registry_generation(&self) -> u64 {
        assets::texture_registry_generation()
    }

    #[inline(always)]
    fn texture_dims(&self, key: &str) -> Option<present_compose::TextureMeta> {
        assets::texture_dims(key).map(|meta| present_compose::TextureMeta {
            w: meta.w,
            h: meta.h,
        })
    }

    #[inline(always)]
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        assets::sprite_sheet_dims(key)
    }

    #[inline(always)]
    fn texture_handle(&self, key: &str) -> TextureHandle {
        assets::texture_handle(key)
    }
}

const ASSET_TEXTURE_CONTEXT: AssetTextureContext = AssetTextureContext;

#[inline(always)]
pub fn build_screen(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &HashMap<&'static str, font::Font>,
    total_elapsed: f32,
) -> RenderList {
    present_compose::build_screen_with_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        &ASSET_TEXTURE_CONTEXT,
    )
}

#[inline(always)]
pub fn build_screen_cached(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &HashMap<&'static str, font::Font>,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
) -> RenderList {
    present_compose::build_screen_cached_with_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        &ASSET_TEXTURE_CONTEXT,
    )
}

#[inline(always)]
pub fn build_screen_cached_with_scratch(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &HashMap<&'static str, font::Font>,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
) -> RenderList {
    present_compose::build_screen_cached_with_scratch_and_texture_context(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        scratch,
        &ASSET_TEXTURE_CONTEXT,
    )
}
