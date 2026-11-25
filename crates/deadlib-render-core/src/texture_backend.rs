use crate::{SamplerDesc, Yuv420Upload};
use std::error::Error;

/// Texture operations for the render thread's asset owner.
pub trait TextureBackend {
    type Texture;
    type RgbaImage;

    fn create_texture(
        &mut self,
        image: &Self::RgbaImage,
        sampler: SamplerDesc,
    ) -> Result<Self::Texture, Box<dyn Error>>;

    fn update_texture(
        &mut self,
        texture: &mut Self::Texture,
        image: &Self::RgbaImage,
    ) -> Result<(), Box<dyn Error>>;

    fn create_yuv420_texture(
        &mut self,
        image: Yuv420Upload<'_>,
        sampler: SamplerDesc,
    ) -> Result<Self::Texture, Box<dyn Error>>;

    fn update_yuv420_texture(
        &mut self,
        texture: &mut Self::Texture,
        image: Yuv420Upload<'_>,
    ) -> Result<(), Box<dyn Error>>;

    fn texture_is_yuv420(&self, texture: &Self::Texture) -> bool;
    fn retire_texture(&mut self, texture: Self::Texture);
}
