//! Bundled transparent texture preparation, shared by both song hosts.
use std::sync::OnceLock;

pub(crate) const NATIVE_BLANK_TEXTURE_PATH: &str = "Themes/_fallback/Graphics/_blank.png";

pub(crate) fn load_native_blank_texture() -> Result<&'static str, String> {
    // Bundled native bitmap, decoded/prepared once at screen construction.
    // The generated registry carries real pixels to ordinary renderer prewarm.
    static PREPARED: OnceLock<Result<(), String>> = OnceLock::new();
    PREPARED
        .get_or_init(|| {
            let decoded = image::load_from_memory_with_format(
                include_bytes!("../assets/itgmania/_blank.png"),
                image::ImageFormat::Png,
            )
            .map_err(|error| error.to_string())?;
            let mut bitmap = decoded.into_rgba8();
            let source_size = [bitmap.width(), bitmap.height()];
            let hints = deadlib_assets::TextureHints {
                min_size: Some(8),
                max_size: Some(2048),
                ..Default::default()
            };
            let upload = deadlib_assets::texture_image_size(source_size, &hints)
                .map_err(|error| error.to_string())?;
            if upload != source_size {
                bitmap = image::imageops::resize(
                    &bitmap,
                    upload[0],
                    upload[1],
                    image::imageops::FilterType::Triangle,
                );
            }
            deadlib_assets::fix_hidden_alpha(&mut bitmap);
            deadlib_assets::register_prepared_generated_texture(
                NATIVE_BLANK_TEXTURE_PATH,
                bitmap,
                source_size,
                hints.sampler_desc(),
            );
            Ok(())
        })
        .as_ref()
        .map(|_| NATIVE_BLANK_TEXTURE_PATH)
        .map_err(Clone::clone)
}
