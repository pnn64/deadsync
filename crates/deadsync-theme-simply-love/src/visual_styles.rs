use deadsync_config::prelude::{SrpgVariant, VisualStyle, get};

pub use crate::resources::{
    ASSETS, Assets, EffectAssets, SRPG10_EVAL_EXPEDITION_FAILED, SRPG10_EVAL_FAILED_SFX,
    SRPG10_EVAL_GOLD_LEAF_BG, SRPG10_EVAL_PAINT, SRPG10_EVAL_PASS_BG, SRPG10_EVAL_PASSED_SFX,
    SRPG10_EVAL_RED_LINES, SRPG10_EVAL_TEXTURES, SRPG10_EVAL_VICTORY, SRPG10_GAMEOVER_MUSIC,
    SRPG10_TITLE_LOGO, all_assets, effect_zoom_scale, for_style, for_style_and_variant,
    is_shared_background_texture, srpg10_faction_name,
};

#[inline(always)]
pub fn current_style() -> VisualStyle {
    std::panic::catch_unwind(|| get().visual_style).unwrap_or(VisualStyle::Hearts)
}

#[inline(always)]
pub fn current_srpg_variant() -> SrpgVariant {
    std::panic::catch_unwind(|| get().srpg_variant).unwrap_or(SrpgVariant::Srpg9)
}

#[inline(always)]
pub fn current_assets() -> &'static Assets {
    for_style_and_variant(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn srpg10_active() -> bool {
    crate::resources::srpg10_active(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn title_logo_texture_key() -> Option<&'static str> {
    crate::resources::title_logo_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn select_color_texture_key() -> &'static str {
    crate::resources::select_color_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn shared_background_texture_key() -> &'static str {
    crate::resources::shared_background_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn titlemenu_flycenter_texture_key() -> &'static str {
    crate::resources::titlemenu_flycenter_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn titlemenu_flytop_texture_key() -> &'static str {
    crate::resources::titlemenu_flytop_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn titlemenu_flybottom_texture_key() -> &'static str {
    crate::resources::titlemenu_flybottom_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn gameplayin_splode_texture_key() -> &'static str {
    crate::resources::gameplayin_splode_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn gameplayin_minisplode_texture_key() -> &'static str {
    crate::resources::gameplayin_minisplode_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn combo_100milestone_splode_texture_key() -> &'static str {
    crate::resources::combo_100milestone_splode_texture_key(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn combo_100milestone_minisplode_texture_key() -> &'static str {
    crate::resources::combo_100milestone_minisplode_texture_key(
        current_style(),
        current_srpg_variant(),
    )
}

#[inline(always)]
pub fn combo_1000milestone_swoosh_texture_key() -> &'static str {
    crate::resources::combo_1000milestone_swoosh_texture_key(
        current_style(),
        current_srpg_variant(),
    )
}

#[inline(always)]
pub fn shared_background_video_asset_path() -> Option<&'static str> {
    crate::resources::shared_background_video_asset_path(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn menu_music_asset_path() -> &'static str {
    crate::resources::menu_music_asset_path(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn srpg10_gameover_music_path() -> std::path::PathBuf {
    deadlib_platform::dirs::app_dirs().resolve_asset_path(SRPG10_GAMEOVER_MUSIC)
}

pub fn menu_music_resolved_path() -> std::path::PathBuf {
    crate::resources::resolve_menu_music_path(
        current_style(),
        current_srpg_variant(),
        deadsync_assets::audio_folder::random_music_path,
        |path| deadlib_platform::dirs::app_dirs().resolve_asset_path(path),
    )
}

pub fn bundled_music_paths() -> Vec<std::path::PathBuf> {
    crate::resources::resolved_bundled_music_paths(|path| {
        deadlib_platform::dirs::app_dirs().resolve_asset_path(path)
    })
}

#[inline(always)]
pub fn select_color_aspect(style: VisualStyle) -> f32 {
    crate::resources::select_color_aspect(style, current_srpg_variant())
}

#[inline(always)]
pub fn select_color_zoom_scale(style: VisualStyle) -> f32 {
    crate::resources::select_color_zoom_scale(style, current_srpg_variant())
}
