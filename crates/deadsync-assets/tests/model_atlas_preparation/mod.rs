use super::*;
use crate::perf;
use deadsync_noteskin::model::ItgTextureFrame;
use image::{Rgba, RgbaImage};
use std::hint::black_box;
use std::time::{SystemTime, UNIX_EPOCH};

mod baseline;

struct Fixture {
    root: PathBuf,
    paths: Vec<PathBuf>,
}

impl Fixture {
    fn new(size: [u32; 2], count: usize, resized: bool) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-model-atlas-preparation-{:010}-{suffix:020}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut paths = Vec::new();
        for index in 0..count {
            let size = if resized && index != 0 {
                [size[0] / 2 + 1, size[1] * 2 + 1]
            } else {
                size
            };
            let path = root.join(format!("frame{index:03}.png"));
            RgbaImage::from_fn(size[0], size[1], |x, y| {
                Rgba([
                    (x * 7 + y * 13 + index as u32 * 37) as u8,
                    (x * 19 + y * 3 + index as u32 * 73) as u8,
                    (x * 11 + y * 29 + index as u32 * 17) as u8,
                    [0, 1, 127, 255][(x as usize + y as usize + index) % 4],
                ])
            })
            .save(&path)
            .unwrap();
            paths.push(path);
        }
        Self { root, paths }
    }

    fn animation(&self, indices: &[usize]) -> ItgTextureAnimation {
        ItgTextureAnimation {
            path: self.root.join("animation.ini"),
            frames: indices
                .iter()
                .enumerate()
                .map(|(index, frame)| ItgTextureFrame {
                    path: self.paths[*frame].clone(),
                    delay: index as f32 * 0.125,
                })
                .collect(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("deadsync-model-atlas-preparation-")
        );
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

fn grid(count: usize) -> [u32; 2] {
    let columns = (count as f32).sqrt().ceil() as u32;
    [columns, (count as u32).div_ceil(columns.max(1))]
}

#[test]
fn consecutive_frame_reuse_preserves_atlas_bytes_resizing_transparency_and_padding() {
    for size in [[1, 1], [3, 7], [16, 16], [33, 17]] {
        for resized in [false, true] {
            let fixture = Fixture::new(size, 4, resized);
            for indices in [
                vec![0],
                vec![0, 1],
                vec![0, 0, 0, 1, 1, 1, 2],
                vec![0, 1, 0, 2, 0, 3],
                vec![0; 17],
                (0..4).collect(),
            ] {
                let animation = fixture.animation(&indices);
                let old =
                    baseline::model_animation_atlas(&animation, size, grid(indices.len())).unwrap();
                let new = model_animation_atlas(&animation, size, grid(indices.len())).unwrap();
                assert_eq!(old.dimensions(), new.dimensions());
                assert_eq!(old.as_raw(), new.as_raw());
            }
        }
    }
}

#[test]
fn atlas_decode_errors_and_refreshes_match_parent_across_calls() {
    let fixture = Fixture::new([8, 8], 2, true);
    let mut animation = fixture.animation(&[0, 0, 1, 1, 0]);
    let saved = model_animation_atlas(&animation, [8, 8], grid(5)).unwrap();
    RgbaImage::from_pixel(8, 8, Rgba([17, 29, 83, 255]))
        .save(&fixture.paths[0])
        .unwrap();
    let refreshed = model_animation_atlas(&animation, [8, 8], grid(5)).unwrap();
    assert_ne!(saved, refreshed);
    assert_eq!(
        baseline::model_animation_atlas(&animation, [8, 8], grid(5)).unwrap(),
        refreshed
    );
    for index in [0, 2, 4] {
        animation.frames[index].path = fixture.root.join("missing.png");
        assert_eq!(
            baseline::model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err(),
            model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err()
        );
        animation.frames[index].path = fixture.paths[if index == 2 { 1 } else { 0 }].clone();
    }
    std::fs::write(&fixture.paths[1], b"corrupt PNG").unwrap();
    assert_eq!(
        baseline::model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err(),
        model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err()
    );
}

#[test]
fn repeated_atlas_frames_reduce_decoder_and_resizer_allocation_churn() {
    for resized in [false, true] {
        let fixture = Fixture::new([64, 64], 2, resized);
        let animation = fixture.animation(&[0, 0, 0, 0, 1, 1, 1, 1]);
        perf::assert_reduced_churn(
            || {
                black_box(baseline::model_animation_atlas(&animation, [64, 64], grid(8)).unwrap());
            },
            || {
                black_box(model_animation_atlas(&animation, [64, 64], grid(8)).unwrap());
            },
        );
    }
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_model_atlas_preparation() {
    for (name, indices, size, resized) in [
        ("single", vec![0], [64, 64], false),
        ("unique", (0..8).collect(), [64, 64], false),
        ("repeat", (0..16).map(|i| i / 8).collect(), [64, 64], false),
        (
            "resize_repeat",
            (0..16).map(|i| i / 8).collect(),
            [64, 64],
            true,
        ),
        (
            "alternating",
            (0..16).map(|i| i % 2).collect(),
            [64, 64],
            true,
        ),
        (
            "large_repeat",
            (0..16).map(|i| i / 8).collect(),
            [256, 128],
            true,
        ),
    ] {
        let fixture = Fixture::new(size, 8, resized);
        let animation = fixture.animation(&indices);
        let grid = grid(indices.len());
        assert_eq!(
            baseline::model_animation_atlas(&animation, size, grid).unwrap(),
            model_animation_atlas(&animation, size, grid).unwrap()
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("tex_atlas_{name}_{label}"),
                32,
                indices.len() * size[0] as usize * size[1] as usize,
                || {
                    let animation = black_box(&animation);
                    black_box(if new {
                        model_animation_atlas(animation, size, grid).unwrap()
                    } else {
                        baseline::model_animation_atlas(animation, size, grid).unwrap()
                    });
                },
            );
        });
    }
}
