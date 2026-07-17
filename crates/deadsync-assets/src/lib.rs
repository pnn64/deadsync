pub mod audio_folder;
pub mod dynamic_media;
pub mod language;
pub mod manager;
pub mod media_cache;
pub mod noteskin;
pub mod present_dsl;
pub mod screenshot;
pub mod song_lua;
pub mod textures;

pub use deadlib_assets::upload::TextureUploadBudget;
pub use deadlib_assets::{
    ASSET_TEXTURE_CONTEXT as PRESENT_TEXTURE_CONTEXT, AssetError,
    AssetTextureContext as PresentTextureContext, TexMeta, TextureChoice, TextureHints,
    media_path_key, open_image_fallback, parse_sprite_sheet_dims, parse_texture_hints,
    register_generated_texture, register_texture_dims, resolve_texture_choice_entry,
    resolve_texture_choice_key as resolve_texture_choice, sprite_sheet_dims, strip_sprite_hints,
    texture_dims, texture_handle, texture_registry_generation, texture_source_dims_from_real,
    texture_source_frame_dims_from_real,
};
pub use manager::AssetManager;
pub use textures::{
    canonical_texture_key, held_miss_texture_choices, hold_judgment_texture_choices,
    judgment_texture_choices,
};

/// Resolve a bundled or data-overlay asset without exposing platform paths to
/// asset consumers.
pub fn resolve_asset_path(path: &str) -> std::path::PathBuf {
    deadlib_platform::dirs::app_dirs().resolve_asset_path(path)
}

/// Open an image from bundled/data-overlay assets without exposing resolution
/// paths to the presentation consumer.
pub fn open_bundled_image(path: &str) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback(&resolve_asset_path(path))
}
