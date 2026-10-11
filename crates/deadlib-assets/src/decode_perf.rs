include!("decode_original.rs");

use std::hint::black_box;

fn jobs(path: &Path, count: usize) -> Vec<TextureDecodeJob> {
    (0..count)
        .map(|index| TextureDecodeJob {
            key: format!("image-{index}"),
            path: path.to_owned(),
            sampler: SamplerDesc::default(),
            hints: TextureHints {
                non_default: index % 3 != 0,
                grayscale: index % 3 == 1,
                alphamap: index % 3 == 2,
                ..Default::default()
            },
        })
        .collect()
}

#[test]
fn batching_matches_original_pixels_errors_and_cancellation() {
    let dir = crate::perf_fixture::TempDir::new("decode-batches");
    let path = dir.path.join("texture.png");
    RgbaImage::from_fn(32, 24, |x, y| {
        image::Rgba([x as u8, y as u8, 171, if x % 3 == 0 { 0 } else { 255 }])
    })
    .save(&path)
    .unwrap();
    for count in [0, 1, 2, 8, 9, 16, 17, 65] {
        let run = |current, cancel| {
            let mut input = jobs(&path, count);
            for (index, job) in input.iter_mut().enumerate() {
                if index % 4 == 3 {
                    job.path.set_extension("missing");
                }
            }
            let mut output = Vec::new();
            let mut consumed = 0;
            let consume = |result: TextureDecodeResult| {
                consumed += 1;
                if cancel {
                    return Err("cancel");
                }
                output.push((
                    result.key,
                    result.sampler,
                    result.image.map_err(|e| e.to_string()),
                ));
                Ok(())
            };
            let result = if current {
                decode_texture_jobs_with(input, consume)
            } else {
                decode_texture_jobs_with_original(input, consume)
            };
            output.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            (result, consumed, output)
        };
        assert_eq!(run(false, false), run(true, false), "{count} jobs");
        assert_eq!(run(false, true), run(true, true), "cancel {count} jobs");
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_texture_worker_batches() {
    let dir = crate::perf_fixture::TempDir::new("decode-bench");
    let path = dir.path.join("texture.png");
    RgbaImage::from_pixel(64, 64, image::Rgba([13, 97, 43, 255]))
        .save(&path)
        .unwrap();
    println!(
        "available workers: {:?}",
        std::thread::available_parallelism()
    );
    for count in [1, 8, 32, 129] {
        let mut churn = Vec::new();
        for current in [false, true] {
            let input = jobs(&path, count);
            let (_, counts) = crate::perf::measure(|| {
                let consume = |decoded: TextureDecodeResult| {
                    black_box(decoded.image.unwrap());
                    Ok::<_, ()>(())
                };
                if current {
                    decode_texture_jobs_with(input, consume)
                } else {
                    decode_texture_jobs_with_original(input, consume)
                }
                .unwrap();
            });
            churn.push(counts);
        }
        println!(
            "texture decode/{count} caller-thread churn (workers excluded): original {:?}, current {:?}",
            churn[0], churn[1]
        );
        let available = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        if available.min(count) > available.min(count.div_ceil(DECODE_JOB_BATCH_SIZE)) {
            assert!(churn[1].allocs < churn[0].allocs);
            assert!(churn[1].allocated_bytes < churn[0].allocated_bytes);
        }
        crate::paired_bench::compare_prepared(
            &format!("texture decode/{count} PNGs"),
            30,
            || jobs(&path, count),
            |jobs, current| {
                let consume = |decoded: TextureDecodeResult| {
                    black_box(decoded.image.unwrap());
                    Ok::<_, ()>(())
                };
                if current {
                    decode_texture_jobs_with(jobs, consume)
                } else {
                    decode_texture_jobs_with_original(jobs, consume)
                }
                .unwrap();
            },
        );
    }
}
