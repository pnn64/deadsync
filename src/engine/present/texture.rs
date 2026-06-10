use crate::assets;
use deadsync_present::texture as present_texture;
use deadsync_render::TextureHandle;

pub(crate) struct AssetTextureContext;

impl present_texture::TextureContext for AssetTextureContext {
    #[inline(always)]
    fn texture_registry_generation(&self) -> u64 {
        assets::texture_registry_generation()
    }

    #[inline(always)]
    fn texture_dims(&self, key: &str) -> Option<present_texture::TextureMeta> {
        assets::texture_dims(key).map(|meta| present_texture::TextureMeta {
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

pub(crate) const ASSET_TEXTURE_CONTEXT: AssetTextureContext = AssetTextureContext;
