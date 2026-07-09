pub mod audio_folder;
pub mod i18n;
pub mod manager;
pub mod media_cache;
pub mod noteskin;
pub mod present_dsl;
pub mod screenshot;
pub mod song_lua;
pub mod textures;
pub mod visual_styles;

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
pub use deadsync_theme::{FontRole, machine_font_key, machine_font_key_for_text};
pub use manager::{AssetManager, current_machine_font_key, current_machine_font_key_for_text};
pub use textures::{
    canonical_texture_key, held_miss_texture_choices, hold_judgment_texture_choices,
    judgment_texture_choices,
};
