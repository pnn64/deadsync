use crate::ASSET_TEXTURE_CONTEXT;
use deadlib_present::{
    actors::{Actor, IntoTextureKey},
    dsl as present_dsl,
};
use std::sync::atomic::AtomicU64;

#[doc(hidden)]
pub struct SpriteBuilder {
    inner: present_dsl::SpriteBuilder,
}

impl SpriteBuilder {
    #[inline(always)]
    pub fn texture<T: IntoTextureKey>(tex: T) -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::texture(tex),
        }
    }

    #[inline(always)]
    pub fn static_texture(tex: &'static str) -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::static_texture(tex),
        }
    }

    #[inline(always)]
    pub fn static_texture_cached(
        tex: &'static str,
        cached_handle: &'static AtomicU64,
        cached_generation: &'static AtomicU64,
    ) -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::static_texture_cached_with_texture_context(
                tex,
                cached_handle,
                cached_generation,
                &ASSET_TEXTURE_CONTEXT,
            ),
        }
    }

    #[inline(always)]
    pub fn solid() -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::solid(),
        }
    }

    #[inline(always)]
    pub fn zoomto(&mut self, w: f32, h: f32) {
        self.inner
            .zoomto_with_texture_context(w, h, &ASSET_TEXTURE_CONTEXT);
    }

    #[inline(always)]
    pub fn build(self, site_base: u64) -> Actor {
        self.inner
            .build_with_texture_context(site_base, &ASSET_TEXTURE_CONTEXT)
    }
}

impl std::ops::Deref for SpriteBuilder {
    type Target = present_dsl::SpriteBuilder;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for SpriteBuilder {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
