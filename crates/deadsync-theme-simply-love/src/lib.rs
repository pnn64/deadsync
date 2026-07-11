pub use deadlib_present::{rgba, rgba_const};
pub use deadlib_render as render;

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
    SimplyLoveEffect, SimplyLoveEffectRouteContext, SimplyLoveEffectRoutePlan,
    SimplyLoveRuntimeRequest, resolve_effect_route,
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

pub mod assets {
    pub use crate::fonts::{
        FontRole, current_machine_font_key, current_machine_font_key_for_text, machine_font_key,
        machine_font_key_for_text,
    };
    pub use crate::{i18n, visual_styles};
    pub use deadsync_assets::*;
}
pub use deadsync_profile_gameplay::{
    GameplayPackData, GameplayProfile, SongLuaRuntimeOverlayStateDelta, chart_effects_from_profile,
    gameplay_attack_mode, gameplay_config_from_config, gameplay_fail_type_from_config,
    gameplay_pack_data, gameplay_play_style_from_profile, gameplay_player_side_from_profile,
    gameplay_runtime_profile_data, gameplay_tick_mode_from_profile, profile_side_from_gameplay,
    profile_tick_mode_from_gameplay, score_display_mode_from_profile, scroll_effects_from_option,
    song_lua_compile_context, song_lua_overlay_delta_mask, song_lua_runtime_column_offset_windows,
    song_lua_runtime_ease_windows, song_lua_runtime_mod_windows,
    song_lua_runtime_overlay_ease_window, tap_explosion_options_from_profile,
};

pub mod config {
    pub use deadsync_config::prelude::*;
}

pub type GameplayCoreState = deadsync_gameplay::GameplayRuntimeState<
    GameplayProfile,
    deadsync_assets::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>,
>;

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
