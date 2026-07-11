use crate::manager::AssetManager;
use deadlib_assets::{
    GraphicTextureChoiceCache, INITIAL_GRAPHIC_TEXTURES, InitialTextureLoad, TextureChoice,
    TextureKeyStoreLoad, canonical_texture_key_with_asset_roots, graphic_texture_roots,
    initial_texture_decode_jobs,
};
use deadlib_platform::dirs;
use deadlib_render::{SamplerDesc, SamplerWrap};
use deadlib_renderer::Backend;
use log::{debug, warn};
use std::path::{Path, PathBuf};

static GRAPHIC_TEXTURE_CHOICES: GraphicTextureChoiceCache = GraphicTextureChoiceCache::new();

fn graphics_roots(folder: &str) -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    graphic_texture_roots(folder, dirs.portable, &dirs.data_dir, &dirs.exe_dir)
}

pub fn judgment_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.judgment_texture_choices(graphics_roots)
}

pub fn hold_judgment_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.hold_judgment_texture_choices(graphics_roots)
}

pub fn held_miss_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.held_miss_texture_choices(graphics_roots)
}

pub fn canonical_texture_key<P: AsRef<Path>>(p: P) -> String {
    let dirs = dirs::app_dirs();
    canonical_texture_key_with_asset_roots(
        p.as_ref(),
        [dirs.data_dir.join("assets"), dirs.exe_dir.join("assets")],
    )
}

pub fn model_texture_sampler(key: &str) -> SamplerDesc {
    SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..deadlib_assets::parse_texture_hints(key).sampler_desc()
    }
}

impl AssetManager {
    pub fn load_initial_textures<T>(
        &mut self,
        backend: &mut Backend,
        texture_assets: T,
    ) -> Result<(), deadlib_assets::AssetError>
    where
        T: IntoIterator<Item = deadlib_assets::TextureAssetSpec>,
    {
        debug!("Loading initial textures...");

        let texture_jobs = initial_texture_decode_jobs(
            texture_assets,
            &dirs::app_dirs().noteskin_roots(),
            |path| canonical_texture_key(path),
            &INITIAL_GRAPHIC_TEXTURES,
            graphics_roots,
            |path| dirs::app_dirs().resolve_asset_path(path),
        );

        let texture_needs_repeat_sampler = self.texture_needs_repeat_sampler;
        for loaded in self.store.load_initial_textures_with(
            texture_jobs,
            texture_needs_repeat_sampler,
            |image, sampler| backend.create_texture(image, sampler),
        )? {
            let InitialTextureLoad {
                key,
                built_in,
                retired,
            } = loaded;
            if let Some((handle, texture)) = retired {
                self.retire_texture(backend, handle, texture);
            }
            if built_in {
                debug!("Loaded built-in texture: {key}");
            } else {
                debug!("Loaded texture: {key}");
            }
        }

        Ok(())
    }

    pub fn ensure_texture_for_key(&mut self, backend: &mut Backend, texture_key: &str) {
        self.load_texture_key(backend, texture_key, None, false);
    }

    pub fn ensure_texture_for_key_with_sampler(
        &mut self,
        backend: &mut Backend,
        texture_key: &str,
        sampler: SamplerDesc,
    ) {
        self.load_texture_key(backend, texture_key, Some(sampler), true);
    }

    fn load_texture_key(
        &mut self,
        backend: &mut Backend,
        texture_key: &str,
        sampler_override: Option<SamplerDesc>,
        force_reload: bool,
    ) {
        let texture_needs_repeat_sampler = self.texture_needs_repeat_sampler;
        match self.store.load_texture_key_with(
            texture_key,
            sampler_override,
            force_reload,
            |key| canonical_texture_key(key),
            |path| dirs::app_dirs().resolve_asset_path(path),
            texture_needs_repeat_sampler,
            |image, sampler| backend.create_texture(image, sampler),
        ) {
            TextureKeyStoreLoad::Skip => {}
            TextureKeyStoreLoad::Missing { key } => {
                warn!("Failed to resolve texture key '{key}' for preload.");
            }
            TextureKeyStoreLoad::DecodeFailed { key, message } => {
                warn!("Failed to open texture for key '{key}': {message}");
            }
            TextureKeyStoreLoad::Loaded { retired } => {
                if let Some((handle, texture)) = retired {
                    self.retire_texture(backend, handle, texture);
                }
            }
            TextureKeyStoreLoad::CreateFailed { key, error } => {
                warn!("Failed to create GPU texture for key '{key}': {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_render::{SamplerFilter, SamplerWrap};

    #[test]
    fn model_sampler_forces_repeat_for_plain_textures() {
        let key = "noteskins/dance/custom/textures/Tap Note parts.png";
        let sampler = model_texture_sampler(key);

        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(sampler.filter, SamplerFilter::Linear);
    }

    #[test]
    fn model_sampler_preserves_texture_hints() {
        let key = "noteskins/dance/custom/textures/Tap Note parts (nearest mipmaps).png";
        let sampler = model_texture_sampler(key);

        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(sampler.filter, SamplerFilter::Nearest);
        assert!(sampler.mipmaps);
    }
}
