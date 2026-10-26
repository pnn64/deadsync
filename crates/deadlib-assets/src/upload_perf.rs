//! Exact insertion state and CPU/allocation checks for prepared upload queues.
use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
include!("upload_original.rs");

fn state(
    order: &VecDeque<TextureHandle>,
    entries: &FxHashMap<TextureHandle, PendingTextureUpload>,
    bytes: usize,
) -> impl PartialEq + std::fmt::Debug {
    let mut images: Vec<_> = entries
        .iter()
        .map(|(&handle, upload)| {
            let image = match upload.image() {
                TextureUploadImage::Rgba(image) => image,
                _ => unreachable!(),
            };
            (
                handle,
                image.as_raw().clone(),
                image.dimensions(),
                upload.source_size,
                upload.bytes,
            )
        })
        .collect();
    images.sort_by_key(|image| image.0);
    (order.clone(), images, bytes)
}

#[test]
fn prepared_and_ordinary_replacements_match_frozen_queue_state() {
    let mut current = TextureUploadQueue::default();
    let mut original = OriginalQueue::default();
    for i in 0..512 {
        let handle = u64::from((i * 71) % 37);
        let image = Arc::new(RgbaImage::from_pixel(
            i % 7 + 1,
            i % 3 + 1,
            image::Rgba([i as u8; 4]),
        ));
        if i % 3 == 0 {
            current.push(handle, Arc::clone(&image), SamplerDesc::default());
            original.push(handle, image, SamplerDesc::default());
        } else {
            current.push_prepared(
                handle,
                Arc::clone(&image),
                SamplerDesc::default(),
                [i + 1, i + 2],
            );
            original.push_prepared(handle, image, SamplerDesc::default(), [i + 1, i + 2]);
        }
        assert_eq!(
            state(&current.order, &current.entries, current.queued_bytes),
            state(&original.order, &original.entries, original.queued_bytes)
        );
    }
}

#[test]
#[ignore = "optimized CPU-cycle and allocation benchmark"]
fn benchmark_upload_insertions() {
    let image = Arc::new(RgbaImage::new(1, 1));
    for count in [1, 64, 1024] {
        for prepared in [false, true] {
            let mut queue = TextureUploadQueue::default();
            for handle in 0..count {
                queue.push(handle, Arc::clone(&image), SamplerDesc::default());
            }
            let mut handle = 0;
            let mut work = || {
                if prepared {
                    black_box(&mut queue).push_prepared(
                        black_box(handle),
                        Arc::clone(&image),
                        SamplerDesc::default(),
                        [17, 19],
                    );
                } else {
                    black_box(&mut queue).push(
                        black_box(handle),
                        Arc::clone(&image),
                        SamplerDesc::default(),
                    );
                }
                handle = (handle + 1) % count;
            };
            let (_, churn) = crate::perf::measure(&mut work);
            assert_eq!(churn, crate::perf::Churn::default());
            let label = format!("uploads/prepared={prepared}/{count}");
            println!("{label}: allocation {churn:?}");
            bench::measure(&label, 100_000, work);
        }
    }
}
