use crate::{sprite_sheet_dims, texture_dims, texture_handle, texture_registry_generation};
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render::TextureHandle;

pub struct AssetTextureContext;

impl TextureContext for AssetTextureContext {
    #[inline(always)]
    fn texture_registry_generation(&self) -> u64 {
        texture_registry_generation()
    }

    #[inline(always)]
    fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
        texture_dims(key).map(|meta| TextureMeta {
            w: meta.w,
            h: meta.h,
        })
    }

    #[inline(always)]
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        sprite_sheet_dims(key)
    }

    #[inline(always)]
    fn texture_handle(&self, key: &str) -> TextureHandle {
        texture_handle(key)
    }
}

pub const ASSET_TEXTURE_CONTEXT: AssetTextureContext = AssetTextureContext;

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::texture::TextureContext;

    #[test]
    fn asset_texture_context_falls_back_to_registry_defaults() {
        assert_eq!(ASSET_TEXTURE_CONTEXT.texture_handle("__missing"), 0);
        assert_eq!(
            ASSET_TEXTURE_CONTEXT.sprite_sheet_dims("sheet 2x4.png"),
            (2, 4)
        );
    }
}
