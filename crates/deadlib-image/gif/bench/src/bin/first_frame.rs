//! First-image RGBA comparison with identical owned output and no timed file I/O.
#[cfg(not(feature = "allocations"))]
use std::time::Instant;
use std::{
    hint::black_box,
    io::Cursor,
    path::{Path, PathBuf},
};

use deadlib_image_gif as native;
use image::{AnimationDecoder, RgbaImage};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CODECS: [&str; 4] = [
    "native-first",
    "gif-asset-first",
    "image-first",
    "native-copy-first",
];

#[cfg(feature = "allocations")]
mod allocations {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    };

    pub static ENABLED: AtomicBool = AtomicBool::new(false);
    pub static CALLS: AtomicUsize = AtomicUsize::new(0);
    pub static BYTES: AtomicUsize = AtomicUsize::new(0);
    pub struct Allocator;
    // Measurement only: all pointers, layouts and sizes pass unchanged to System.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(layout.size(), Relaxed);
            }
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(layout.size(), Relaxed);
            }
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(size, Relaxed);
            }
            unsafe { System.realloc(ptr, layout, size) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }
}
#[cfg(feature = "allocations")]
#[global_allocator]
static ALLOCATOR: allocations::Allocator = allocations::Allocator;

#[cfg(windows)]
mod cycles {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        #[cfg(not(feature = "allocations"))]
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    }
    pub fn pin() {
        // The current-thread pseudo-handle is valid for this thread-affinity call.
        assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1) }, 0);
    }
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        let mut value = 0;
        // A valid thread handle and writable u64 live throughout the FFI call.
        assert_ne!(
            unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
            0
        );
        value
    }
}
#[cfg(not(windows))]
mod cycles {
    pub fn pin() {}
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        0
    }
}

fn native_first<const COPY: bool>(
    bytes: &[u8],
    options: &native::DecodeOptions,
) -> Result<RgbaImage> {
    let mut decoder = native::Decoder::new(options.clone());
    let mut render = None;
    let mut at = 0;
    loop {
        let progress = decoder.update(&bytes[at..])?;
        at += progress.consumed;
        match progress.event {
            Some(native::Event::Header) => {
                render = Some(native::Compositor::with_options(
                    decoder.header().unwrap(),
                    native::CompositeOptions {
                        background: native::Background::Transparent,
                        ..Default::default()
                    },
                )?);
            }
            Some(native::Event::Block(native::Block::Frame(frame))) => {
                let canvas = render.as_mut().unwrap().render(&frame)?;
                if COPY {
                    // Keep the original owned-output path as a same-binary
                    // before/after control for the consuming conversion.
                    return Ok(RgbaImage::from_raw(
                        u32::from(canvas.width),
                        u32::from(canvas.height),
                        canvas.rgba.to_vec(),
                    )
                    .unwrap());
                }
                let image = render.unwrap().into_image();
                let native::Samples::U8(pixels) = image.samples else {
                    unreachable!("RGBA8 compositor returns byte samples");
                };
                return Ok(RgbaImage::from_raw(image.width, image.height, pixels).unwrap());
            }
            Some(native::Event::Block(_)) => {}
            Some(native::Event::End) => return Err("GIF has no image frame".into()),
            None => return Err("truncated GIF before first image frame".into()),
        }
    }
}

#[expect(
    clippy::chunks_exact_to_as_chunks,
    reason = "Measure the asset loader's existing palette loop unchanged."
)]
fn gif_asset_first(bytes: &[u8]) -> Result<RgbaImage> {
    // The core of deadlib-assets/src/decode.rs::decode_gif_from_reader, with
    // in-memory input and color_key=false. No disposal is applied to frame one.
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(image::Limits::default().max_alloc.unwrap_or(u64::MAX)).unwrap(),
    ));
    let mut reader = options.read_info(Cursor::new(bytes))?;
    let canvas = [u32::from(reader.width()), u32::from(reader.height())];
    let frame = reader.next_frame_info()?.ok_or("GIF has no image frame")?;
    let size = [u32::from(frame.width), u32::from(frame.height)];
    let origin = [u32::from(frame.left), u32::from(frame.top)];
    if size.contains(&0)
        || canvas.contains(&0)
        || origin[0] + size[0] > canvas[0]
        || origin[1] + size[1] > canvas[1]
    {
        return Err("first image rectangle is empty or outside the canvas".into());
    }
    let mut colors = [[0; 4]; 256];
    let transparent = frame.transparent;
    let palette = match frame.palette.as_deref() {
        Some(palette) => palette,
        None => reader.global_palette().unwrap_or_default(),
    };
    for (color, rgb) in colors.iter_mut().zip(palette.chunks_exact(3)) {
        *color = [rgb[0], rgb[1], rgb[2], 255];
    }
    if let Some(index) = transparent {
        colors[index as usize] = [0; 4];
    }
    let mut indices = vec![0; size[0] as usize * size[1] as usize];
    reader.read_into_buffer(&mut indices)?;
    let mut image = RgbaImage::new(canvas[0], canvas[1]);
    for (source, row) in indices.chunks_exact(size[0] as usize).zip(
        image
            .as_mut()
            .chunks_exact_mut(canvas[0] as usize * 4)
            .skip(origin[1] as usize),
    ) {
        for (&index, pixel) in source
            .iter()
            .zip(row[origin[0] as usize * 4..].as_chunks_mut::<4>().0)
        {
            *pixel = colors[index as usize];
        }
    }
    Ok(image)
}

fn decode(codec: usize, bytes: &[u8], options: &native::DecodeOptions) -> Result<RgbaImage> {
    match codec {
        0 => native_first::<false>(bytes, options),
        1 => gif_asset_first(bytes),
        2 => Ok(image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?
            .into_frames()
            .next()
            .ok_or("GIF has no image frame")??
            .into_buffer()),
        3 => native_first::<true>(bytes, options),
        _ => unreachable!(),
    }
}

fn files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            files(&entry.unwrap().path(), out);
        }
    } else if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
    {
        out.push(path.to_owned());
    }
}

fn main() {
    let mut paths = Vec::new();
    let mut options = native::DecodeOptions::default();
    let mut verify_only = false;
    for arg in std::env::args_os().skip(1) {
        if arg == "--compatibility" {
            options.strict = false;
        } else if arg == "--verify-only" {
            verify_only = true;
        } else {
            assert!(
                !arg.to_string_lossy().starts_with("--"),
                "unknown option: {}",
                arg.to_string_lossy()
            );
            files(Path::new(&arg), &mut paths);
        }
    }
    paths.sort();
    let mut seen = std::collections::HashSet::new();
    paths.retain(|path| seen.insert(path.canonicalize().unwrap()));
    assert!(!paths.is_empty(), "supply GIF files or corpus directories");
    cycles::pin();
    #[cfg(feature = "allocations")]
    {
        use std::{
            alloc::{GlobalAlloc, Layout},
            sync::atomic::Ordering::Relaxed,
        };
        allocations::ENABLED.store(true, Relaxed);
        // Each allocation is checked before its original layout is used to free
        // or reallocate it. The calibration covers all counted operations.
        unsafe {
            let a = black_box(ALLOCATOR.alloc(Layout::from_size_align(31, 8).unwrap()));
            assert!(!a.is_null());
            let a = black_box(ALLOCATOR.realloc(a, Layout::from_size_align(31, 8).unwrap(), 96));
            assert!(!a.is_null());
            let b = black_box(ALLOCATOR.alloc_zeroed(Layout::from_size_align(17, 8).unwrap()));
            assert!(!b.is_null());
            ALLOCATOR.dealloc(a, Layout::from_size_align(96, 8).unwrap());
            ALLOCATOR.dealloc(b, Layout::from_size_align(17, 8).unwrap());
        }
        allocations::ENABLED.store(false, Relaxed);
        assert_eq!(allocations::CALLS.load(Relaxed), 3);
        assert_eq!(allocations::BYTES.load(Relaxed), 144);
    }
    #[cfg(not(feature = "allocations"))]
    let sample_ms =
        std::env::var("GIF_BENCH_MS").map_or(20, |value| value.parse::<u128>().unwrap());
    if !verify_only {
        println!("file\tcodec\tcanvas_pixels\tns\tthread_cycles\talloc_calls\trequested_bytes");
    }
    let count = paths.len();
    for (file_index, path) in paths.into_iter().enumerate() {
        let bytes = std::fs::read(&path).unwrap();
        let expected = decode(1, &bytes, &options).unwrap_or_else(|error| {
            panic!("{}: gif-asset-first rejected: {error}", path.display())
        });
        for codec in [0, 2, 3] {
            let actual = decode(codec, &bytes, &options).unwrap_or_else(|error| {
                panic!("{}: {} rejected: {error}", path.display(), CODECS[codec])
            });
            assert_eq!(
                actual.dimensions(),
                expected.dimensions(),
                "{}: {} dimensions",
                path.display(),
                CODECS[codec]
            );
            assert_eq!(
                actual.as_raw(),
                expected.as_raw(),
                "{}: {} RGBA bytes",
                path.display(),
                CODECS[codec]
            );
        }
        let pixels = expected.width() as usize * expected.height() as usize;
        drop(expected);
        eprintln!("VERIFIED\t{}\t{pixels}", path.display());
        if verify_only {
            continue;
        }
        #[cfg(not(feature = "allocations"))]
        let group_size = if pixels > 4_000_000 { 1 } else { 8 };
        for offset in 0..CODECS.len() {
            let codec = (offset + file_index) % CODECS.len();
            black_box(decode(codec, black_box(&bytes), &options).unwrap());
            #[cfg(feature = "allocations")]
            {
                use std::sync::atomic::Ordering::Relaxed;
                allocations::CALLS.store(0, Relaxed);
                allocations::BYTES.store(0, Relaxed);
                allocations::ENABLED.store(true, Relaxed);
                black_box(decode(codec, black_box(&bytes), &options).unwrap());
                allocations::ENABLED.store(false, Relaxed);
                println!(
                    "{}\t{}\t{pixels}\t0\t0\t{}\t{}",
                    path.display(),
                    CODECS[codec],
                    allocations::CALLS.load(Relaxed),
                    allocations::BYTES.load(Relaxed)
                );
            }
            #[cfg(not(feature = "allocations"))]
            {
                let mut wall = [0; 5];
                let mut cpu = [0; 5];
                for batch in 0..5 {
                    let start_cycles = cycles::read();
                    let start = Instant::now();
                    let mut iterations = 0;
                    loop {
                        for _ in 0..group_size {
                            black_box(decode(codec, black_box(&bytes), &options).unwrap());
                        }
                        iterations += group_size;
                        if start.elapsed().as_millis() >= sample_ms {
                            break;
                        }
                    }
                    wall[batch] = start.elapsed().as_nanos() as u64 / iterations;
                    cpu[batch] = (cycles::read() - start_cycles) / iterations;
                }
                wall.sort_unstable();
                cpu.sort_unstable();
                println!(
                    "{}\t{}\t{pixels}\t{}\t{}\t0\t0",
                    path.display(),
                    CODECS[codec],
                    wall[2],
                    cpu[2]
                );
            }
        }
    }
    if verify_only {
        println!("Verified {count} first-image owned RGBA canvases across all four paths.");
    }
}
