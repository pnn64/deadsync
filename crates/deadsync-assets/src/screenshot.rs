use chrono::{DateTime, Datelike, Local};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const SCREENSHOT_MAX_SUFFIX: u32 = 9_999;

#[derive(Debug)]
pub enum ScreenshotSaveError {
    Io(std::io::Error),
    Image(image::ImageError),
    FilenameExhausted,
}

impl fmt::Display for ScreenshotSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Image(e) => write!(f, "{e}"),
            Self::FilenameExhausted => write!(f, "failed to allocate unique screenshot filename"),
        }
    }
}

impl Error for ScreenshotSaveError {}

impl From<std::io::Error> for ScreenshotSaveError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<image::ImageError> for ScreenshotSaveError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

pub fn sanitize_screenshot_title(title: &str) -> String {
    title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn set_opaque_alpha(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        pixel.0[3] = 255;
    }
}

pub fn save_screenshot_image(
    root: &Path,
    image: &image::RgbaImage,
    song_info: Option<(&str, Option<u32>)>,
) -> Result<PathBuf, ScreenshotSaveError> {
    let now = Local::now();
    let dir = screenshot_month_dir(root, now);
    std::fs::create_dir_all(&dir)?;

    let stamp = now.format("%Y-%m-%d_%H%M%S").to_string();
    let stem = screenshot_file_stem(&stamp, song_info);
    let path = allocate_screenshot_path(&dir, &stem)?;
    image.save_with_format(&path, image::ImageFormat::Png)?;
    Ok(path)
}

fn screenshot_month_dir(root: &Path, now: DateTime<Local>) -> PathBuf {
    let month_idx = now.month0() as usize;
    let month_name = MONTH_NAMES.get(month_idx).copied().unwrap_or("Unknown");
    root.join(format!("{:04}", now.year()))
        .join(format!("{:02}-{}", now.month(), month_name))
}

fn screenshot_file_stem(stamp: &str, song_info: Option<(&str, Option<u32>)>) -> String {
    let title_part = song_info
        .map(|(title, meter)| {
            let title = sanitize_screenshot_title(title);
            match meter {
                Some(m) if m > 0 => format!("__{m}__{title}"),
                _ => format!("__{title}"),
            }
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_default();
    format!("{stamp}{title_part}")
}

fn allocate_screenshot_path(dir: &Path, stem: &str) -> Result<PathBuf, ScreenshotSaveError> {
    let mut path = dir.join(format!("{stem}.png"));
    let mut suffix = 1_u32;
    while path.exists() {
        path = dir.join(format!("{stem}-{suffix:02}.png"));
        suffix = suffix.saturating_add(1);
        if suffix > SCREENSHOT_MAX_SUFFIX {
            return Err(ScreenshotSaveError::FilenameExhausted);
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn title_sanitizer_keeps_alphanumeric_only() {
        assert_eq!(sanitize_screenshot_title("A/B: C! 09"), "A_B__C__09");
    }

    #[test]
    fn file_stem_includes_meter_and_title() {
        assert_eq!(
            screenshot_file_stem("2026-07-09_120000", Some(("Song!", Some(12)))),
            "2026-07-09_120000__12__Song_"
        );
    }

    #[test]
    fn file_stem_omits_zero_meter() {
        assert_eq!(
            screenshot_file_stem("2026-07-09_120000", Some(("Song!", Some(0)))),
            "2026-07-09_120000__Song_"
        );
    }

    #[test]
    fn path_allocation_uses_suffix_for_collision() {
        let dir = unique_temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shot.png"), b"taken").unwrap();

        let path = allocate_screenshot_path(&dir, "shot").unwrap();
        assert_eq!(path.file_name().unwrap(), "shot-01.png");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn alpha_normalization_sets_every_pixel_opaque() {
        let mut image = image::RgbaImage::from_fn(2, 2, |x, y| {
            image::Rgba([x as u8, y as u8, 0, (x + y) as u8])
        });

        set_opaque_alpha(&mut image);

        assert!(image.pixels().all(|pixel| pixel.0[3] == 255));
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "deadsync-assets-screenshot-{}-{nanos}",
            std::process::id()
        ))
    }
}
