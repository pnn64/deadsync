use chrono::{DateTime, Datelike, Local};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
const SCREENSHOT_FLASH_ATTACK_SECONDS: f32 = 0.02;
const SCREENSHOT_FLASH_DECAY_SECONDS: f32 = 0.18;
const SCREENSHOT_FLASH_MAX_ALPHA: f32 = 0.7;
const SCREENSHOT_PREVIEW_SCALE: f32 = 0.2;
const SCREENSHOT_PREVIEW_HOLD_SECONDS: f32 = 0.4;
const SCREENSHOT_PREVIEW_MACHINE_EXTRA_HOLD_SECONDS: f32 = 0.25;
const SCREENSHOT_PREVIEW_TWEEN_SECONDS: f32 = 0.75;
const SCREENSHOT_PREVIEW_GLOW_PERIOD_SECONDS: f32 = 0.5;
const SCREENSHOT_PREVIEW_GLOW_ALPHA: f32 = 0.2;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotPreviewTarget {
    Player1,
    Player2,
    Machine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenshotPreviewState {
    pub started_at: Instant,
    pub target: ScreenshotPreviewTarget,
}

#[derive(Clone, Copy, Debug)]
pub struct ScreenshotRuntimeState<RequestSide: Copy> {
    pending: bool,
    request_side: Option<RequestSide>,
    flash_started_at: Option<Instant>,
    preview: Option<ScreenshotPreviewState>,
}

impl<RequestSide: Copy> ScreenshotRuntimeState<RequestSide> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            pending: false,
            request_side: None,
            flash_started_at: None,
            preview: None,
        }
    }

    #[inline(always)]
    pub fn request(&mut self, side: Option<RequestSide>) {
        self.pending = true;
        self.request_side = side;
    }

    #[inline(always)]
    pub fn take_pending_request(&mut self) -> Option<Option<RequestSide>> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        Some(self.request_side.take())
    }

    #[inline(always)]
    pub const fn pending(&self) -> bool {
        self.pending
    }

    #[inline(always)]
    pub fn mark_saved(&mut self, now: Instant) {
        self.flash_started_at = Some(now);
    }

    #[inline(always)]
    pub fn clear_preview(&mut self) {
        self.preview = None;
    }

    #[inline(always)]
    pub fn set_preview(&mut self, now: Instant, target: ScreenshotPreviewTarget) {
        self.preview = Some(ScreenshotPreviewState {
            started_at: now,
            target,
        });
    }

    #[inline(always)]
    pub fn flash_alpha(&self, now: Instant) -> f32 {
        screenshot_flash_alpha(self.flash_started_at, now)
    }

    #[inline(always)]
    pub fn preview_pose(
        &self,
        now: Instant,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ScreenshotPreviewPose> {
        let preview = self.preview?;
        screenshot_preview_pose(preview.started_at, preview.target, now, screen_w, screen_h)
    }
}

impl<RequestSide: Copy> Default for ScreenshotRuntimeState<RequestSide> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenshotPreviewPose {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub glow_alpha: f32,
}

pub fn screenshot_flash_alpha(started_at: Option<Instant>, now: Instant) -> f32 {
    let Some(started_at) = started_at else {
        return 0.0;
    };
    let elapsed = now.duration_since(started_at).as_secs_f32();
    let total = SCREENSHOT_FLASH_ATTACK_SECONDS + SCREENSHOT_FLASH_DECAY_SECONDS;
    if elapsed <= 0.0 || elapsed >= total {
        return 0.0;
    }
    if elapsed <= SCREENSHOT_FLASH_ATTACK_SECONDS {
        return (elapsed / SCREENSHOT_FLASH_ATTACK_SECONDS).clamp(0.0, 1.0)
            * SCREENSHOT_FLASH_MAX_ALPHA;
    }
    let fade = 1.0 - ((elapsed - SCREENSHOT_FLASH_ATTACK_SECONDS) / SCREENSHOT_FLASH_DECAY_SECONDS);
    fade.clamp(0.0, 1.0) * SCREENSHOT_FLASH_MAX_ALPHA
}

pub fn screenshot_preview_pose(
    started_at: Instant,
    target: ScreenshotPreviewTarget,
    now: Instant,
    screen_w: f32,
    screen_h: f32,
) -> Option<ScreenshotPreviewPose> {
    let elapsed = now.duration_since(started_at).as_secs_f32();
    if !elapsed.is_finite() || elapsed < 0.0 {
        return None;
    }

    let hold_seconds = SCREENSHOT_PREVIEW_HOLD_SECONDS
        + match target {
            ScreenshotPreviewTarget::Machine => SCREENSHOT_PREVIEW_MACHINE_EXTRA_HOLD_SECONDS,
            ScreenshotPreviewTarget::Player1 | ScreenshotPreviewTarget::Player2 => 0.0,
        };
    let total_seconds = hold_seconds + SCREENSHOT_PREVIEW_TWEEN_SECONDS;
    if elapsed >= total_seconds {
        return None;
    }

    let start_x = screen_w * 0.5;
    let start_y = screen_h * 0.5;
    let (target_x, target_y) = match target {
        ScreenshotPreviewTarget::Player1 => (20.0, screen_h + 10.0),
        ScreenshotPreviewTarget::Player2 => (screen_w - 20.0, screen_h + 10.0),
        ScreenshotPreviewTarget::Machine => (screen_w * 0.5, screen_h + 10.0),
    };

    let (x, y, scale) = if elapsed <= hold_seconds {
        (start_x, start_y, SCREENSHOT_PREVIEW_SCALE)
    } else {
        let t = ((elapsed - hold_seconds) / SCREENSHOT_PREVIEW_TWEEN_SECONDS).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        (
            start_x + (target_x - start_x) * smooth,
            start_y + (target_y - start_y) * smooth,
            SCREENSHOT_PREVIEW_SCALE * (1.0 - smooth),
        )
    };

    let blink_phase = elapsed * (std::f32::consts::TAU / SCREENSHOT_PREVIEW_GLOW_PERIOD_SECONDS);
    let glow_alpha = blink_phase.sin().mul_add(0.5, 0.5) * SCREENSHOT_PREVIEW_GLOW_ALPHA;
    Some(ScreenshotPreviewPose {
        x,
        y,
        scale: scale.max(0.0),
        glow_alpha: glow_alpha.clamp(0.0, 1.0),
    })
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

    #[test]
    fn screenshot_flash_alpha_attacks_and_decays() {
        let started = Instant::now();
        assert_eq!(screenshot_flash_alpha(None, started), 0.0);
        assert_eq!(screenshot_flash_alpha(Some(started), started), 0.0);
        let peak = screenshot_flash_alpha(
            Some(started),
            started + std::time::Duration::from_millis(20),
        );
        assert!((peak - SCREENSHOT_FLASH_MAX_ALPHA).abs() < 0.001);
        assert_eq!(
            screenshot_flash_alpha(
                Some(started),
                started + std::time::Duration::from_millis(250),
            ),
            0.0
        );
    }

    #[test]
    fn screenshot_preview_pose_moves_toward_player_slot() {
        let started = Instant::now();
        let center = screenshot_preview_pose(
            started,
            ScreenshotPreviewTarget::Player1,
            started,
            640.0,
            480.0,
        )
        .expect("center pose");
        assert_eq!(center.x, 320.0);
        assert_eq!(center.y, 240.0);

        let moving = screenshot_preview_pose(
            started,
            ScreenshotPreviewTarget::Player1,
            started + std::time::Duration::from_millis(800),
            640.0,
            480.0,
        )
        .expect("moving pose");
        assert!(moving.x < center.x);
        assert!(moving.y > center.y);
        assert!(moving.scale < center.scale);
    }

    #[test]
    fn screenshot_runtime_latches_and_takes_pending_request() {
        let mut state = ScreenshotRuntimeState::<u8>::new();

        assert_eq!(state.take_pending_request(), None);
        state.request(Some(2));
        assert!(state.pending());
        assert_eq!(state.take_pending_request(), Some(Some(2)));
        assert!(!state.pending());
        assert_eq!(state.take_pending_request(), None);
    }

    #[test]
    fn screenshot_runtime_tracks_flash_and_preview() {
        let started = Instant::now();
        let mut state = ScreenshotRuntimeState::<u8>::new();

        state.mark_saved(started);
        assert!(state.flash_alpha(started + std::time::Duration::from_millis(10)) > 0.0);

        state.set_preview(started, ScreenshotPreviewTarget::Machine);
        assert!(
            state
                .preview_pose(
                    started + std::time::Duration::from_millis(100),
                    640.0,
                    480.0
                )
                .is_some()
        );
        state.clear_preview();
        assert!(state.preview_pose(started, 640.0, 480.0).is_none());
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
