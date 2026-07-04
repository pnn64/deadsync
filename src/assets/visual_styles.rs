use crate::config::{self, SrpgVariant, VisualStyle};

pub use deadsync_theme::{
    ASSETS, Assets, EffectAssets, SRPG10_EVAL_EXPEDITION_FAILED, SRPG10_EVAL_FAILED_SFX,
    SRPG10_EVAL_GOLD_LEAF_BG, SRPG10_EVAL_PAINT, SRPG10_EVAL_PASS_BG, SRPG10_EVAL_PASSED_SFX,
    SRPG10_EVAL_RED_LINES, SRPG10_EVAL_TEXTURES, SRPG10_EVAL_VICTORY, SRPG10_GAMEOVER_MUSIC,
    SRPG10_TITLE_LOGO, all_assets, effect_zoom_scale, for_style, for_style_and_variant,
    is_shared_background_texture, srpg10_faction_name,
};

#[inline(always)]
pub fn current_style() -> VisualStyle {
    std::panic::catch_unwind(|| config::get().visual_style).unwrap_or(VisualStyle::Hearts)
}

#[inline(always)]
pub fn current_srpg_variant() -> SrpgVariant {
    std::panic::catch_unwind(|| config::get().srpg_variant).unwrap_or(SrpgVariant::Srpg9)
}

#[inline(always)]
pub fn current_assets() -> &'static Assets {
    for_style_and_variant(current_style(), current_srpg_variant())
}

#[inline(always)]
pub fn srpg10_active() -> bool {
    current_style().is_srpg() && current_srpg_variant() == SrpgVariant::Srpg10
}

#[inline(always)]
pub fn title_logo_texture_key() -> Option<&'static str> {
    srpg10_active().then_some(SRPG10_TITLE_LOGO)
}

#[inline(always)]
pub fn select_color_texture_key() -> &'static str {
    current_assets().select_color
}

#[inline(always)]
pub fn shared_background_texture_key() -> &'static str {
    current_assets().shared_background
}

#[inline(always)]
pub fn titlemenu_flycenter_texture_key() -> &'static str {
    current_assets().effects.titlemenu_flycenter
}

#[inline(always)]
pub fn titlemenu_flytop_texture_key() -> &'static str {
    current_assets().effects.titlemenu_flytop
}

#[inline(always)]
pub fn titlemenu_flybottom_texture_key() -> &'static str {
    current_assets().effects.titlemenu_flybottom
}

#[inline(always)]
pub fn gameplayin_splode_texture_key() -> &'static str {
    current_assets().effects.gameplayin_splode
}

#[inline(always)]
pub fn gameplayin_minisplode_texture_key() -> &'static str {
    current_assets().effects.gameplayin_minisplode
}

#[inline(always)]
pub fn combo_100milestone_splode_texture_key() -> &'static str {
    current_assets().effects.combo_100milestone_splode
}

#[inline(always)]
pub fn combo_100milestone_minisplode_texture_key() -> &'static str {
    current_assets().effects.combo_100milestone_minisplode
}

#[inline(always)]
pub fn combo_1000milestone_swoosh_texture_key() -> &'static str {
    current_assets().effects.combo_1000milestone_swoosh
}

#[inline(always)]
pub fn shared_background_video_asset_path() -> Option<&'static str> {
    current_assets().shared_background_video
}

#[inline(always)]
pub fn menu_music_asset_path() -> &'static str {
    current_assets().menu_music
}

#[inline(always)]
pub fn srpg10_gameover_music_path() -> std::path::PathBuf {
    deadlib_platform::dirs::app_dirs().resolve_asset_path(SRPG10_GAMEOVER_MUSIC)
}

/// Returns the absolute path to the menu music file that should play for the
/// current visual style. If the user has dropped one or more `.ogg` files
/// into `{data_dir}/assets/music/menu/{style}/` (lowercase style name) a
/// random one of those is returned; otherwise the bundled per-style file
/// from [`menu_music_asset_path`] is used. Folder override + bundled file
/// satisfy issue #375 without requiring users to overwrite anything inside
/// the bundle.
pub fn menu_music_resolved_path() -> std::path::PathBuf {
    let style = current_style();
    let folder = if style.is_srpg() {
        current_srpg_variant().as_str()
    } else {
        style.as_str()
    };
    let folder_rel = format!("assets/music/menu/{}", folder.to_ascii_lowercase());
    if let Some(p) = crate::assets::audio_folder::random_music_path(&folder_rel) {
        return p;
    }
    deadlib_platform::dirs::app_dirs().resolve_asset_path(menu_music_asset_path())
}

/// Background-music tracks bundled with the game: the per-style menu loops plus
/// the course-select and credits loops. Each relative asset key is resolved
/// through the normal overlay so the returned paths match what actually plays.
/// Used to pre-warm the ReplayGain cache at startup so a fresh install (or a
/// cleared cache) doesn't audibly adjust loudness the first time a menu track
/// plays.
pub fn bundled_music_paths() -> Vec<std::path::PathBuf> {
    use std::collections::BTreeSet;
    let rels: BTreeSet<&'static str> = deadsync_theme::bundled_music_asset_paths().collect();
    let dirs = deadlib_platform::dirs::app_dirs();
    rels.into_iter()
        .map(|rel| dirs.resolve_asset_path(rel))
        .collect()
}

#[inline(always)]
pub fn select_color_aspect(style: VisualStyle) -> f32 {
    deadsync_theme::select_color_aspect(style, current_srpg_variant())
}

#[inline(always)]
pub fn select_color_zoom_scale(style: VisualStyle) -> f32 {
    deadsync_theme::select_color_zoom_scale(style, current_srpg_variant())
}
