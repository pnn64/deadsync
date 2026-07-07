pub use deadlib_assets::present_dsl::SpriteBuilder;
pub use deadlib_present::actors::{IntoTextureKey, TextureKeyHandle};
use deadlib_present::dsl as present_dsl;
pub use present_dsl::TextBuilder;

// PARITY COMMENT STANDARD:
// PARITY[<Source>]: <mirrored behavior>. Ref: <file/symbol> when known.

#[macro_export]
macro_rules! act {
    (sprite($tex:literal): $($tail:tt)+) => {{
        static __TEXTURE_HANDLE: ::std::sync::atomic::AtomicU64 =
            ::std::sync::atomic::AtomicU64::new($crate::render::INVALID_TEXTURE_HANDLE);
        static __TEXTURE_GENERATION: ::std::sync::atomic::AtomicU64 =
            ::std::sync::atomic::AtomicU64::new(::core::u64::MAX);
        ::deadlib_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::static_texture_cached(
                $tex,
                &__TEXTURE_HANDLE,
                &__TEXTURE_GENERATION,
            )
        )
    }};
    (sprite($tex:expr): $($tail:tt)+) => {{
        ::deadlib_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::texture($tex)
        )
    }};
    (sprite_static($tex:expr): $($tail:tt)+) => {{
        ::deadlib_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::static_texture($tex)
        )
    }};
    (quad: $($tail:tt)+) => {{
        ::deadlib_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::SpriteBuilder::solid()
        )
    }};
    (text: $($tail:tt)+) => {{
        ::deadlib_present::__act_from_builder!(
            ($($tail)+)
            $crate::assets::present_dsl::TextBuilder::new()
        )
    }};
}
