// Frozen from 12a1090ed (0.5.1228); function bodies unchanged.
use super::*;

pub(super) fn open_image_fallback_mode(
    path: &Path,
    warn_mismatch: bool,
) -> image::ImageResult<image::DynamicImage> {
    let hint = ImageFormat::from_path(path).ok();
    if let Some(fmt) = hint {
        let mut reader = ImageReader::open(path).map_err(image::ImageError::IoError)?;
        reader.set_format(fmt);
        if let Ok(img) = reader.decode() {
            return Ok(img);
        }
    }

    let guessed = ImageReader::open(path)
        .map_err(image::ImageError::IoError)?
        .with_guessed_format()?;
    let guessed_fmt = guessed.format();
    if let (Some(hint_fmt), Some(real_fmt)) = (hint, guessed_fmt)
        && hint_fmt != real_fmt
        && warn_mismatch
    {
        warn!(
            "Graphic file '{}' is really {:?}",
            path.to_string_lossy(),
            real_fmt
        );
    }
    guessed.decode()
}

pub(super) fn open_image_fallback_quiet(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, false)
}

pub(super) fn build_cached_banner_rgba(
    path: &Path,
    _opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    if is_dynamic_video_path(path) {
        return video::load_poster(path)
            .map_err(|e| image::ImageError::IoError(std::io::Error::other(e)));
    }
    Ok(open_image_fallback_quiet(path)?.to_rgba8())
}
