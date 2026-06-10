use crate::assets::PRESENT_TEXTURE_CONTEXT;
use deadsync_present::actors;
use deadsync_present::compose as present_compose;
use deadsync_present::font;
use deadsync_present::space::Metrics;
use deadsync_render::RenderList;
use std::collections::HashMap;

pub use deadsync_present::compose::{
    ComposeScratch, NullTextureContext, TextLayoutCache, TextLayoutFrameStats, TextureContext,
    TextureMeta,
};

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
        &PRESENT_TEXTURE_CONTEXT,
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
        &PRESENT_TEXTURE_CONTEXT,
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
        &PRESENT_TEXTURE_CONTEXT,
    )
}
