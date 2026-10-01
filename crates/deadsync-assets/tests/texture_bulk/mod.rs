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
            "deadsync-texture-bulk-{:010}-{suffix:020}",
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
                .starts_with("deadsync-texture-bulk-")
        );
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

fn grid(count: usize) -> [u32; 2] {
    let columns = (count as f32).sqrt().ceil() as u32;
    [columns, (count as u32).div_ceil(columns.max(1))]
}

#[test]
fn row_copies_preserve_frame_order_resize_alpha_and_unused_atlas_tiles() {
    for size in [[1, 1], [3, 7], [33, 17], [64, 64], [127, 65]] {
        for resized in [false, true] {
            let fixture = Fixture::new(size, 4, resized);
            for indices in [
                vec![0],
                vec![0, 1],
                vec![0, 0, 1, 1, 2],
                vec![0, 1, 0, 2, 0, 3],
                vec![0; 17],
            ] {
                let animation = fixture.animation(&indices);
                let dimensions = grid(indices.len());
                assert_eq!(
                    baseline::model_animation_atlas(&animation, size, dimensions).unwrap(),
                    model_animation_atlas(&animation, size, dimensions).unwrap()
                );
            }
        }
    }
}

#[test]
fn row_copy_matches_replace_at_tile_edges_without_heap_scratch() {
    for size in [[1, 1], [3, 7], [64, 64], [127, 65]] {
        let frame = RgbaImage::from_fn(size[0], size[1], |x, y| {
            Rgba([
                (x * 79) as u8,
                (y * 53) as u8,
                (x + y) as u8,
                [0, 1, 127, 255][(x + y) as usize % 4],
            ])
        });
        let initial =
            RgbaImage::from_pixel(size[0] * 3 + 2, size[1] * 2 + 3, Rgba([17, 29, 83, 101]));
        for (x, y) in [
            (0, 0),
            (1, 2),
            (
                initial.width() - frame.width(),
                initial.height() - frame.height(),
            ),
        ] {
            let mut old = initial.clone();
            let mut new = initial.clone();
            image::imageops::replace(&mut old, &frame, i64::from(x), i64::from(y));
            perf::assert_no_churn(|| copy_model_atlas_frame(&mut new, &frame, x, y));
            assert_eq!(old, new);
        }
    }
}

#[test]
fn bulk_atlas_preserves_errors_and_refreshes_files_between_builds() {
    let fixture = Fixture::new([8, 8], 2, true);
    let mut animation = fixture.animation(&[0, 0, 1, 1, 0]);
    let saved = model_animation_atlas(&animation, [8, 8], grid(5)).unwrap();
    RgbaImage::from_pixel(8, 8, Rgba([17, 29, 83, 255]))
        .save(&fixture.paths[0])
        .unwrap();
    let new = model_animation_atlas(&animation, [8, 8], grid(5)).unwrap();
    assert_ne!(saved, new);
    assert_eq!(
        baseline::model_animation_atlas(&animation, [8, 8], grid(5)).unwrap(),
        new
    );
    for index in [0, 2, 4] {
        let path = animation.frames[index].path.clone();
        animation.frames[index].path = fixture.root.join("missing.png");
        assert_eq!(
            baseline::model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err(),
            model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err()
        );
        animation.frames[index].path = path;
    }
    std::fs::write(&fixture.paths[1], b"corrupt PNG").unwrap();
    assert_eq!(
        baseline::model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err(),
        model_animation_atlas(&animation, [8, 8], grid(5)).unwrap_err()
    );
}

fn paused_fixture(
    key: &str,
    indices: Option<Vec<usize>>,
    src: [i32; 2],
    selected: usize,
) -> SpriteSlot {
    let mut plan = generated_animation_sprite_slot_plan(
        key.to_owned(),
        (512, 256),
        [32, 16],
        12,
        AnimationRate::FramesPerSecond(30.0),
        false,
    );
    plan.def.src = src;
    if let SpriteSourcePlan::Animated {
        grid,
        frame_indices,
        frame_durations,
        ..
    } = &mut plan.source
    {
        *grid = (4, 3);
        *frame_indices = indices;
        *frame_durations = Some(vec![0.125; 12]);
    }
    let mut slot = slot_from_plan(plan);
    slot.animation_start_frame = selected;
    slot.animation_start_time = 0.75;
    slot
}

fn compare_frozen(old: &SpriteSlot, new: &SpriteSlot) {
    assert_eq!(old.def, new.def);
    assert_eq!(old.source_size, new.source_size);
    assert_eq!(old.animation_start_frame, new.animation_start_frame);
    assert_eq!(
        old.animation_start_time.to_bits(),
        new.animation_start_time.to_bits()
    );
    assert_eq!(old.texture_key(), new.texture_key());
    assert_eq!(
        old.source.texel_scale().map(f32::to_bits),
        new.source.texel_scale().map(f32::to_bits)
    );
    for time in [-1.0, 0.0, 0.125, 1.0, f32::NAN] {
        assert_eq!(old.frame_index(time, time), new.frame_index(time, time));
        for frame in [0, 1, 7] {
            assert_eq!(
                old.uv_for_frame_at(frame, time).map(f32::to_bits),
                new.uv_for_frame_at(frame, time).map(f32::to_bits)
            );
        }
    }
    match (old.source.as_ref(), new.source.as_ref()) {
        (
            SpriteSource::Atlas {
                tex_dims: a,
                cached_handle: ah,
                cached_generation: ag,
                cached_actor_texture: at,
                ..
            },
            SpriteSource::Atlas {
                tex_dims: b,
                cached_handle: bh,
                cached_generation: bg,
                cached_actor_texture: bt,
                ..
            },
        ) => {
            assert_eq!(a, b);
            for (a, b) in [(ah, bh), (ag, bg), (at, bt)] {
                assert_eq!(a.load(Ordering::Relaxed), b.load(Ordering::Relaxed));
            }
        }
        _ => panic!("frozen atlas expected"),
    }
}

#[test]
fn freezing_preserves_index_origins_mirroring_uv_bits_and_cache_resets() {
    for indices in [None, Some(vec![]), Some(vec![7, 2, 11, 0])] {
        for src in [[0, 0], [37, 19], [-3, -7]] {
            for selected in [0, 1, 3, 7, 11, 15] {
                for mirror in [false, true] {
                    let mut initial =
                        paused_fixture("paused/текстура.png", indices.clone(), src, selected);
                    initial.def.mirror_h = mirror;
                    initial.def.mirror_v = !mirror;
                    initial.source =
                        source_from_plan(source_plan_from_slot(&initial), &initial.def);
                    if let SpriteSource::Animated {
                        cached_handle,
                        cached_generation,
                        cached_actor_texture,
                        ..
                    } = initial.source.as_ref()
                    {
                        cached_handle.store(17, Ordering::Relaxed);
                        cached_generation.store(29, Ordering::Relaxed);
                        cached_actor_texture.store(83, Ordering::Relaxed);
                    }
                    let key = initial.texture_key_shared();
                    let mut old = initial.clone();
                    let mut new = initial.clone();
                    baseline::freeze_sprite_animation(&mut old);
                    freeze_sprite_animation(&mut new);
                    compare_frozen(&old, &new);
                    assert!(Arc::ptr_eq(&key, &new.texture_key_shared()));
                    assert!(matches!(
                        initial.source.as_ref(),
                        SpriteSource::Animated { .. }
                    ));
                    let source = Arc::clone(&new.source);
                    freeze_sprite_animation(&mut new);
                    assert!(Arc::ptr_eq(&source, &new.source));
                }
            }
        }
    }
}

#[test]
fn freezing_reuses_keys_and_reduces_owning_allocation_churn() {
    for key in ["paused/frame.png".to_owned(), "k".repeat(1024)] {
        let slot = paused_fixture(&key, Some(vec![7, 2, 11, 0]), [37, 19], 2);
        perf::assert_reduced_churn(
            || {
                let mut old = slot.clone();
                baseline::freeze_sprite_animation(&mut old);
                black_box(old);
            },
            || {
                let mut new = slot.clone();
                freeze_sprite_animation(&mut new);
                black_box(new);
            },
        );
        perf::assert_churn_budget(
            1,
            std::mem::size_of::<SpriteSource>() + 2 * std::mem::size_of::<usize>(),
            || {
                let mut new = slot.clone();
                freeze_sprite_animation(&mut new);
                black_box(new);
            },
        );
    }
}

#[test]
fn uniquely_owned_freezing_reuses_source_storage_without_allocating() {
    let mut slot = paused_fixture("paused/frame.png", Some(vec![7, 2, 11, 0]), [37, 19], 2);
    let pointer = Arc::as_ptr(&slot.source);
    // Keep the independent payloads alive to isolate the source/key churn.
    let payloads = match slot.source.as_ref() {
        SpriteSource::Animated {
            frame_indices,
            frame_durations,
            ..
        } => (frame_indices.clone(), frame_durations.clone()),
        _ => unreachable!(),
    };
    let mut old = slot.clone();
    baseline::freeze_sprite_animation(&mut old);
    assert_eq!(Arc::strong_count(&slot.source), 1);
    perf::assert_no_churn(|| freeze_sprite_animation(&mut slot));
    assert_eq!(pointer, Arc::as_ptr(&slot.source));
    compare_frozen(&old, &slot);
    black_box(payloads);
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
fn benchmark_texture_bulk() {
    for size in [[64, 64], [256, 128]] {
        let frame = RgbaImage::from_fn(size[0], size[1], |x, y| {
            Rgba([
                (x * 7) as u8,
                (y * 19) as u8,
                (x + y) as u8,
                (x * 31 + y) as u8,
            ])
        });
        let mut atlas = RgbaImage::new(size[0] * 4, size[1] * 4);
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("bulk_copy_{}x{}_{label}", size[0], size[1]),
                128,
                16 * size[0] as usize * size[1] as usize,
                || {
                    for index in 0..16 {
                        let x = index % 4 * size[0];
                        let y = index / 4 * size[1];
                        if new {
                            copy_model_atlas_frame(black_box(&mut atlas), black_box(&frame), x, y);
                        } else {
                            image::imageops::replace(
                                black_box(&mut atlas),
                                black_box(&frame),
                                i64::from(x),
                                i64::from(y),
                            );
                        }
                    }
                    black_box(&atlas);
                },
            );
        });
    }
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
                &format!("bulk_atlas_{name}_{label}"),
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
    for (name, key, indices, atlas) in [
        ("short", "paused/frame.png".to_owned(), None, false),
        ("long", "k".repeat(1024), None, false),
        (
            "indexed",
            "paused/frame.png".to_owned(),
            Some(vec![7, 2, 11, 0]),
            false,
        ),
        ("atlas_noop", "paused/frame.png".to_owned(), None, true),
    ] {
        let mut initial = paused_fixture(&key, indices, [37, 19], 2);
        if atlas {
            freeze_sprite_animation(&mut initial);
        }
        pairs(|label, new| {
            perf::measure_sampled(&format!("bulk_freeze_{name}_{label}"), 256, 64, || {
                for _ in 0..64 {
                    let mut slot = black_box(&initial).clone();
                    if new {
                        freeze_sprite_animation(&mut slot);
                    } else {
                        baseline::freeze_sprite_animation(&mut slot);
                    }
                    black_box(slot);
                }
            });
        });
    }
    pairs(|label, new| {
        perf::measure_sampled_with_setup(
            &format!("bulk_freeze_unique_{label}"),
            128,
            64,
            || {
                (0..64)
                    .map(|_| paused_fixture("paused/frame.png", None, [37, 19], 2))
                    .collect::<Vec<_>>()
            },
            |slots| {
                for slot in slots.iter_mut() {
                    if new {
                        freeze_sprite_animation(black_box(slot));
                    } else {
                        baseline::freeze_sprite_animation(black_box(slot));
                    }
                }
            },
        );
    });
}
