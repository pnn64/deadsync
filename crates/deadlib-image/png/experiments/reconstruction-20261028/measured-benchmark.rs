//! CPU cycles and untimed allocation counts for decoding and RGBA expansion.
use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, EncodeOptions, Filter, Frame, FrameControl, Image,
    Limits, Png, Samples, decode, encode,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};

#[allow(dead_code)]
#[path = "../../../../tests/support/kernel_bench.rs"]
mod cpu;

struct Counting;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every operation forwards the original pointer and layout to System.
// The counters never access the allocation or change the allocator contract.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn fixture(width: u32, height: u32, color: ColorType, depth: u8) -> Png {
    let count = width as usize * height as usize * color.channels();
    let mask = (1u32 << depth) - 1;
    let mut random = 0x4fe3_a279u32;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        (random & mask) as u16
    };
    Png::new(Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16((0..count).map(|_| next()).collect())
        } else {
            Samples::U8((0..count).map(|_| next() as u8).collect())
        },
    })
}

fn fingerprint(samples: &Samples) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut add = |value: u16| {
        hash = (hash ^ u64::from(value)).wrapping_mul(0x100_0000_01b3);
    };
    match samples {
        Samples::U8(v) => v.iter().for_each(|&v| add(u16::from(v))),
        Samples::U16(v) => v.iter().for_each(|&v| add(v)),
    }
    hash
}

fn measure<T>(case: &str, operation: &str, bytes: usize, hash: u64, mut run: impl FnMut() -> T) {
    let iterations: usize = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .unwrap_or_else(|_| "12".into())
        .parse()
        .unwrap();
    assert!(iterations > 0);
    for _ in 0..3 {
        black_box(run());
    }
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    black_box(run());
    TRACK.store(false, Relaxed);
    let counts = (
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed),
    );
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start_cycles = cpu::cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(run());
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        let cycles = (cpu::cycles() - start_cycles) as f64 / iterations as f64;
        samples.push((cycles, ns));
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let (cycles, ns) = samples[3];
    let rate = bytes as f64 * 1e9 / ns / 1048576.0;
    println!(
        "{case}\t{operation}\t{iterations}\t{bytes}\t{cycles:.3}\t{ns:.3}\t{rate:.3}\t{}\t{}\t{}\t{hash:016x}",
        counts.0, counts.1, counts.2,
    );
}

fn main() {
    cpu::pin_thread();
    println!(
        "case\toperation\titerations\tsample_bytes\tcycles_per_image\tns_per_image\tMiB_per_second\tallocations\treallocations\trequested_bytes\tfingerprint"
    );
    for (name, color) in [
        ("gray", ColorType::Grayscale),
        ("ga", ColorType::GrayscaleAlpha),
        ("rgb", ColorType::Rgb),
        ("rgba", ColorType::Rgba),
    ] {
        for depth in [8, 16] {
            let mut png = fixture(513, 257, color, depth);
            let expected = png.to_rgba16(Limits::default()).unwrap();
            let hash = fingerprint(&expected.samples);
            measure(
                &format!("{name}{depth}"),
                "rgba16",
                expected.samples.len() * 2,
                hash,
                || black_box(&png).to_rgba16(Limits::default()).unwrap(),
            );
            png.animation = Some(Animation {
                plays: 0,
                default_is_frame: true,
                frames: vec![Frame {
                    control: FrameControl {
                        width: png.image.width,
                        height: png.image.height,
                        x_offset: 0,
                        y_offset: 0,
                        delay_num: 1,
                        delay_den: 10,
                        dispose: DisposeOp::None,
                        blend: BlendOp::Source,
                    },
                    image: None,
                }],
            });
            let mut compositor = png.compositor(Limits::default()).unwrap();
            assert_eq!(
                compositor.next_frame().unwrap().unwrap().samples,
                match &expected.samples {
                    Samples::U16(v) => v,
                    _ => unreachable!(),
                }
            );
            measure(
                &format!("{name}{depth}"),
                "apng-source",
                expected.samples.len() * 2,
                hash,
                || {
                    compositor.reset();
                    black_box(compositor.next_frame().unwrap().unwrap().samples);
                },
            );
        }
    }
    for depth in [1, 2, 4, 8, 16] {
        let png = fixture(4097, 257, ColorType::Grayscale, depth);
        let bytes = encode(
            &png,
            &EncodeOptions {
                filter: Filter::None,
                ..Default::default()
            },
        )
        .unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.image, png.image);
        measure(
            &format!("gray{depth}-large"),
            "decode",
            png.image.samples.len() * if depth == 16 { 2 } else { 1 },
            fingerprint(&decoded.image.samples),
            || decode(black_box(&bytes)).unwrap(),
        );
    }
    for filter in [
        Filter::None,
        Filter::Sub,
        Filter::Up,
        Filter::Average,
        Filter::Paeth,
    ] {
        let png = fixture(33, 1, ColorType::Rgba, 8);
        let bytes = encode(
            &png,
            &EncodeOptions {
                filter,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(decode(&bytes).unwrap().image, png.image);
        measure(
            &format!("rgba8-short-{}", filter as u8),
            "decode",
            png.image.samples.len(),
            fingerprint(&png.image.samples),
            || decode(black_box(&bytes)).unwrap(),
        );
    }
}
