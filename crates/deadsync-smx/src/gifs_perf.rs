use super::*;
use image::Rgba;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "gifs_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn encoded(size: (u32, u32), count: usize, start: usize, end: usize) -> Vec<u8> {
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame};
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        for index in 0..count {
            let mut image = RgbaImage::from_fn(size.0, size.1, |x, y| {
                Rgba([
                    (x * 13 + index as u32) as u8,
                    (y * 17) as u8,
                    (x + y) as u8,
                    255,
                ])
            });
            image.put_pixel(
                0,
                size.1 - 1,
                Rgba([if index >= start { 255 } else { 0 }, 0, 0, 255]),
            );
            image.put_pixel(
                1,
                size.1 - 1,
                Rgba([if index >= end { 255 } else { 0 }, 0, 0, 255]),
            );
            let ms = [0, 10, 30, 40, 50, 100, 250][index % 7];
            encoder
                .encode_frame(Frame::from_parts(
                    image,
                    0,
                    0,
                    Delay::from_numer_denom_ms(ms, 1),
                ))
                .unwrap();
        }
    }
    bytes
}

fn assert_same(data: &[u8]) {
    match (original::decode_full_pad(data), decode_full_pad(data)) {
        (Ok((a, sa)), Ok((b, sb))) => {
            assert_eq!(sa, sb);
            assert_eq!(a.panels, b.panels);
            assert_eq!(a.durations, b.durations);
            assert_eq!(a.loop_frame, b.loop_frame);
            assert_eq!(a.beats_per_loop, b.beats_per_loop);
        }
        (Err(a), Err(b)) => assert_eq!(a, b),
        _ => panic!("full pad result changed"),
    }
    match (original::decode_panel(data), decode_panel(data)) {
        (Ok((a, sa)), Ok((b, sb))) => {
            assert_eq!(sa, sb);
            assert_eq!(a.frames, b.frames);
            assert_eq!(a.durations, b.durations);
            assert_eq!(a.loop_frame, b.loop_frame);
            assert_eq!(a.loop_end, b.loop_end);
            assert_eq!(a.has_outro(), b.has_outro());
        }
        (Err(a), Err(b)) => assert_eq!(a, b),
        _ => panic!("panel result changed"),
    }
}

#[test]
fn streamed_gifs_preserve_pixels_durations_markers_and_errors() {
    for size in [(23, 24), (14, 15), (7, 8), (7, 7), (4, 5), (4, 4), (3, 3)] {
        for (start, end) in [(0, 0), (1, 3), (3, 1), (2, 2), (99, 99), (99, 1)] {
            assert_same(&encoded(size, 6, start, end));
        }
    }
    for data in [&b""[..], &b"not a gif"[..], &b"GIF89a"[..]] {
        assert_same(data);
    }
    let data = encoded((7, 8), 8, 2, 5);
    let mut compared = 0;
    let mut nonterminating = 0;
    for end in (0..data.len()).step_by(23) {
        // Some truncated frames yield errors indefinitely in image. Bound the
        // preflight before invoking the original unbounded filter_map loop.
        let terminates = match GifDecoder::new(Cursor::new(&data[..end])) {
            Err(_) => true,
            Ok(decoder) => {
                let mut frames = decoder.into_frames();
                (0..64).any(|_| frames.next().is_none())
            }
        };
        if terminates {
            assert_same(&data[..end]);
            compared += 1;
        } else {
            nonterminating += 1;
        }
    }
    assert!(compared > 0);
    println!(
        "Compared {compared} truncated GIFs; skipped {nonterminating} nonterminating reference inputs"
    );
}

#[test]
fn shipped_gifs_match_original_decoding() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = vec![
        root.join("assets/smx-pad-lights"),
        root.join("assets/smx-judge-lights"),
    ];
    let mut count = 0;
    while let Some(path) = paths.pop() {
        if path.is_dir() {
            paths.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else if path.extension().is_some_and(|ext| ext == "gif") {
            assert_same(&std::fs::read(&path).unwrap());
            count += 1;
        }
    }
    assert!(count > 0);
    println!("Compared {count} shipped GIF files");
}

#[test]
fn streamed_gifs_reduce_peak_heap_without_retaining_spare_frames() {
    let data = encoded((23, 24), 257, 2, 200);
    let (before, old) = allocations::measure(|| original::decode_full_pad(&data).unwrap());
    let (after, new) = allocations::measure(|| decode_full_pad(&data).unwrap());
    assert_eq!(before.0.panels, after.0.panels);
    assert!(
        new.peak_bytes * 5 < old.peak_bytes * 3,
        "{old:?} -> {new:?}"
    );
    for panel in &after.0.panels {
        assert_eq!(panel.capacity(), panel.len());
    }
    assert!(new.allocated_bytes - new.freed_bytes <= old.allocated_bytes - old.freed_bytes);
}

#[test]
#[ignore = "paired performance benchmark"]
fn benchmark_streamed_gifs() {
    for (size, count) in [((23, 24), 32), ((23, 24), 257), ((7, 8), 257)] {
        let data = encoded(size, count, 2, count - 2);
        let full = size.0 == 23;
        let run = |current| {
            if full {
                drop(black_box(if current {
                    decode_full_pad(black_box(&data))
                } else {
                    original::decode_full_pad(black_box(&data))
                }));
            } else {
                drop(black_box(if current {
                    decode_panel(black_box(&data))
                } else {
                    original::decode_panel(black_box(&data))
                }));
            }
        };
        let (_, old) = allocations::measure(|| run(false));
        let (_, new) = allocations::measure(|| run(true));
        println!("GIF {size:?} frames={count}: original {old:?}; current {new:?}");
        paired::compare(&format!("GIF {size:?} frames={count}"), 256, run);
    }
}
