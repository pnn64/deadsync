use crate::{sprite_sheet_dims, texture_dims, texture_registry_generation};
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render_core::{INVALID_TEXTURE_HANDLE, TextureHandle};

/// CPU metadata for asset construction before a texture store is available.
/// Rendering uses a borrowed `TextureStore` instead.
pub struct MetadataTextureContext;

impl TextureContext for MetadataTextureContext {
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
    fn texture_handle(&self, _key: &str) -> TextureHandle {
        INVALID_TEXTURE_HANDLE
    }
}

pub const METADATA_TEXTURE_CONTEXT: MetadataTextureContext = MetadataTextureContext;

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::texture::TextureContext;

    #[test]
    fn asset_texture_context_falls_back_to_registry_defaults() {
        assert_eq!(METADATA_TEXTURE_CONTEXT.texture_handle("__missing"), 0);
        assert_eq!(
            METADATA_TEXTURE_CONTEXT.sprite_sheet_dims("sheet 2x4.png"),
            (2, 4)
        );
    }
}
