pub(crate) use deadlib_present::rgba_const;
pub(crate) use deadlib_render as render;

pub mod effects;
pub mod fonts;
pub mod i18n;
mod i18n_runtime;
pub mod notefield_style;
mod resources;
pub mod scorebox;
pub mod step_stats;
pub mod step_stats_gifs;
pub mod views;
pub mod visual_styles;

pub use effects::{
    SimplyLoveConfigRequest, SimplyLoveDebugRequest, SimplyLoveEffect,
    SimplyLoveEffectRouteContext, SimplyLoveEffectRoutePlan, SimplyLoveHardwareRequest,
    SimplyLoveMediaRequest, SimplyLoveOnlineRequest, SimplyLoveProfileRequest,
    SimplyLoveRuntimeRequest, SimplyLoveSyncRequest, SimplyLoveUpdaterRequest,
    resolve_effect_route,
};

pub struct SimplyLoveTheme;

impl deadsync_theme::Theme for SimplyLoveTheme {
    type Screen = screens::SimplyLoveScreen;
    type RuntimeRequest = SimplyLoveRuntimeRequest;

    #[inline(always)]
    fn screen_id(screen: Self::Screen) -> deadsync_theme::ThemeScreenId {
        screen.id()
    }
}

pub(crate) mod assets {
    pub use crate::fonts::{FontRole, current_machine_font_key, current_machine_font_key_for_text};
    pub use crate::{i18n, visual_styles};
    pub use deadsync_assets::*;
}
pub(crate) mod config {
    pub use deadsync_config::prelude::*;
}

mod act_macro {
    macro_rules! act {
        (sprite($tex:literal): $($tail:tt)+) => {{
            static __TEXTURE_HANDLE: ::std::sync::atomic::AtomicU64 =
                ::std::sync::atomic::AtomicU64::new($crate::render::INVALID_TEXTURE_HANDLE);
            static __TEXTURE_GENERATION: ::std::sync::atomic::AtomicU64 =
                ::std::sync::atomic::AtomicU64::new(::core::u64::MAX);
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::static_texture_cached(
                    $tex,
                    &__TEXTURE_HANDLE,
                    &__TEXTURE_GENERATION,
                )
            )
        }};
        (sprite($tex:expr): $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::texture($tex)
            )
        }};
        (sprite_static($tex:expr): $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::static_texture($tex)
            )
        }};
        (quad: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::SpriteBuilder::solid()
            )
        }};
        (text: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+)
                ::deadsync_assets::present_dsl::TextBuilder::new()
            )
        }};
    }
    pub(crate) use act;
}
pub(crate) use act_macro::act;

pub fn asset_manifest()
-> deadsync_theme::ThemeAssetManifest<impl Iterator<Item = deadlib_assets::TextureAssetSpec>> {
    deadsync_theme::ThemeAssetManifest {
        fonts: &resources::FONT_ASSETS,
        textures: resources::initial_texture_assets(),
        texture_needs_repeat_sampler: resources::texture_needs_repeat_sampler,
    }
}

pub mod screens;

#[cfg(test)]
mod tests {
    #[test]
    fn screen_contract_is_reexported() {
        assert_eq!(
            super::screens::Screen::Menu.current_screen_file_name(),
            "ScreenTitleMenu"
        );
    }

    #[test]
    fn asset_manifest_adapts_current_theme_resources() {
        let manifest = super::asset_manifest();

        assert_eq!(manifest.fonts.len(), super::resources::FONT_ASSETS.len());
        assert!(manifest.textures.into_iter().any(|asset| {
            asset.key == "grades/goldstar (stretch).png"
                && asset.path == "grades/goldstar (stretch).png"
        }));
        assert!((manifest.texture_needs_repeat_sampler)(
            "grades/goldstar (stretch).png"
        ));
        assert!(!(manifest.texture_needs_repeat_sampler)("logo.png"));
    }
}
