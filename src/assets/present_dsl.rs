use super::PRESENT_TEXTURE_CONTEXT;
use deadsync_present::actors::Actor;
pub use deadsync_present::actors::{IntoTextureKey, TextureKeyHandle};
use deadsync_present::dsl as present_dsl;
pub use present_dsl::TextBuilder;
use std::sync::atomic::AtomicU64;

// PARITY COMMENT STANDARD:
// PARITY[<Source>]: <mirrored behavior>. Ref: <file/symbol> when known.

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
                &PRESENT_TEXTURE_CONTEXT,
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
            .zoomto_with_texture_context(w, h, &PRESENT_TEXTURE_CONTEXT);
    }

    #[inline(always)]
    pub fn build(self, site_base: u64) -> Actor {
        self.inner
            .build_with_texture_context(site_base, &PRESENT_TEXTURE_CONTEXT)
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

#[macro_export]
macro_rules! act {
    (sprite($tex:literal): $($tail:tt)+) => {{
        static __TEXTURE_HANDLE: ::std::sync::atomic::AtomicU64 =
            ::std::sync::atomic::AtomicU64::new($crate::render::INVALID_TEXTURE_HANDLE);
        static __TEXTURE_GENERATION: ::std::sync::atomic::AtomicU64 =
            ::std::sync::atomic::AtomicU64::new(::core::u64::MAX);
        ::deadsync_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::static_texture_cached(
                $tex,
                &__TEXTURE_HANDLE,
                &__TEXTURE_GENERATION,
            )
        )
    }};
    (sprite($tex:expr): $($tail:tt)+) => {{
        ::deadsync_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::texture($tex)
        )
    }};
    (quad: $($tail:tt)+) => {{
        ::deadsync_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::solid()
        )
    }};
    (text: $($tail:tt)+) => {{
        ::deadsync_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::TextBuilder::new()
        )
    }};
}
