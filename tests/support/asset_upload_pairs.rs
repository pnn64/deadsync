//! Paired original/current insertion bodies share a process and CPU core.
use super::*;
use std::hint::black_box;

#[path = "kernel_bench.rs"]
mod bench;

#[test]
#[ignore = "paired optimized CPU benchmark"]
fn benchmark_paired_uploads() {
    let image = Arc::new(RgbaImage::new(1, 1));
    for count in [1, 64, 1024, 32768] {
        let mut original = OriginalQueue::default();
        let mut current = TextureUploadQueue::default();
        for handle in 0..count {
            original.push(handle, Arc::clone(&image), SamplerDesc::default());
            current.push(handle, Arc::clone(&image), SamplerDesc::default());
        }
        let mut handle = 0;
        bench::compare(&format!("paired/uploads/{count}"), 100_000, |new| {
            if new {
                black_box(&mut current).push_prepared(
                    black_box(handle),
                    Arc::clone(&image),
                    SamplerDesc::default(),
                    [17, 19],
                );
            } else {
                black_box(&mut original).push_prepared(
                    black_box(handle),
                    Arc::clone(&image),
                    SamplerDesc::default(),
                    [17, 19],
                );
            }
            handle = (handle + 1) % count;
        });
    }
}
