use crate::METADATA_TEXTURE_CONTEXT;
use deadlib_present::{
    actors::{Actor, IntoTextureKey},
    dsl as present_dsl,
};

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
    #[must_use]
    pub fn static_texture(tex: &'static str) -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::static_texture(tex),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn solid() -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::solid(),
        }
    }

    #[inline(always)]
    pub fn zoomto(&mut self, w: f32, h: f32) {
        self.inner
            .zoomto_with_texture_context(w, h, &METADATA_TEXTURE_CONTEXT);
    }

    #[inline(always)]
    #[must_use]
    pub fn build(self, site_base: u64) -> Actor {
        self.inner.build(site_base)
    }

    #[inline(always)]
    pub fn build_tweened(
        self,
        site_base: u64,
        build_steps: impl FnOnce() -> present_dsl::TweenSteps,
    ) -> Actor {
        self.inner.build_tweened_with_texture_context(
            site_base,
            &METADATA_TEXTURE_CONTEXT,
            build_steps,
        )
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
