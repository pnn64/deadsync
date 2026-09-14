//! Deterministic disk fixtures shared by image-loading performance tests.

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Tree {
    pub path: PathBuf,
    root: PathBuf,
}

impl Tree {
    pub fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "image-loading-{:010}-{:010}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path, root }
    }

    pub fn save(&self, name: &str, image: &DynamicImage, format: ImageFormat) -> PathBuf {
        let path = self.path.join(name);
        image.save_with_format(&path, format).unwrap();
        path
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        assert_eq!(self.path.parent(), Some(self.root.as_path()));
        assert!(
            self.path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("image-loading-")
        );
        fs::remove_dir_all(&self.path).unwrap();
    }
}

pub fn pixels(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            (x.wrapping_mul(17) ^ y.wrapping_mul(3)) as u8,
            (x.wrapping_mul(7) ^ y.wrapping_mul(23)) as u8,
            (x.wrapping_mul(31) ^ y.wrapping_mul(11)) as u8,
            match (x + y) % 7 {
                0 => 0,
                1 => 127,
                _ => 255,
            },
        ])
    })
}

pub fn compare<T>(name: &str, old: impl FnMut() -> T, new: impl FnMut() -> T) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        crate::perf::measure_sampled(&format!("{name}/new"), 16, 1, new);
        crate::perf::measure_sampled(&format!("{name}/old"), 16, 1, old);
    } else {
        crate::perf::measure_sampled(&format!("{name}/old"), 16, 1, old);
        crate::perf::measure_sampled(&format!("{name}/new"), 16, 1, new);
    }
}
