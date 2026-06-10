use crate::font;
use deadsync_render as renderer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureMeta {
    pub w: u32,
    pub h: u32,
}

pub trait TextureContext {
    fn texture_registry_generation(&self) -> u64;
    fn texture_dims(&self, key: &str) -> Option<TextureMeta>;
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32);
    fn texture_handle(&self, key: &str) -> renderer::TextureHandle;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NullTextureContext;

impl TextureContext for NullTextureContext {
    #[inline(always)]
    fn texture_registry_generation(&self) -> u64 {
        0
    }

    #[inline(always)]
    fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
        None
    }

    #[inline(always)]
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        font::parse_sprite_sheet_dims_from_key(key)
    }

    #[inline(always)]
    fn texture_handle(&self, _key: &str) -> renderer::TextureHandle {
        renderer::INVALID_TEXTURE_HANDLE
    }
}
