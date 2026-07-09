pub mod app;
pub mod config;
pub mod screens;
pub mod test_support;

pub use deadlib_present::{rgba, rgba_const};
pub use deadlib_render as render;
pub use deadsync_assets as assets;
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

pub type GameplayCoreState = deadsync_gameplay::GameplayRuntimeState<
    GameplayProfile,
    deadsync_assets::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>,
>;

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
