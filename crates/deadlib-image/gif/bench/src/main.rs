#[cfg(not(feature = "allocations"))]
use std::time::Instant;
use std::{
    hint::black_box,
    io::Cursor,
    path::{Path, PathBuf},
};

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
    // Measurement only: every operation forwards the unchanged System contract.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(l.size(), Relaxed);
            }
            unsafe { System.alloc(l) }
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(l.size(), Relaxed);
            }
            unsafe { System.alloc_zeroed(l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(n, Relaxed);
            }
            unsafe { System.realloc(p, l, n) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
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
        assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1) }, 0);
    }
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        let mut value = 0;
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

use deadlib_image_gif as native;
use image::AnimationDecoder;
const CODECS: [&str; 7] = [
    "native-retained",
    "gif-retained",
    "native-stream",
    "gif-stream",
    "native-rgba",
    "gif-rgba",
    "image-rgba",
];
struct Stats {
    frames: usize,
    total_pixels: usize,
}
fn indexed(bytes: &[u8]) -> gif::Decoder<Cursor<&[u8]>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.read_info(Cursor::new(bytes)).unwrap()
}
fn compositor(header: &native::Header) -> native::Compositor {
    native::Compositor::with_options(
        header,
        native::CompositeOptions {
            background: native::Background::Transparent,
            ..Default::default()
        },
    )
    .unwrap()
}
fn verify(bytes: &[u8]) -> Result<Stats, String> {
    let actual = native::decode(bytes).map_err(|e| format!("native strict rejection: {e}"))?;
    if actual.frames().next().is_none() {
        return Err("no image frames".into());
    }
    if actual.blocks.iter().any(|b| {
        matches!(
            b,
            native::Block::Extension(
                native::Extension::PlainText(_)
                    | native::Extension::Unknown {
                        label: 0..=0x7f,
                        ..
                    }
            )
        )
    }) {
        return Err("non-image graphics require a caller-supplied renderer".into());
    }
    let mut reader = indexed(bytes);
    assert_eq!(
        (actual.header.width, actual.header.height),
        (reader.width(), reader.height())
    );
    let global: Option<Vec<u8>> = actual
        .header
        .global_palette
        .as_ref()
        .map(|p| p.iter().flatten().copied().collect());
    assert_eq!(global.as_deref(), reader.global_palette());
    let mut screen = gif_dispose::Screen::new_decoder(&reader);
    let mut render = compositor(&actual.header);
    let mut image_frames = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .unwrap()
        .into_frames();
    let mut stats = Stats {
        frames: 0,
        total_pixels: 0,
    };
    for frame in actual.frames() {
        let expected = reader.read_next_frame().unwrap().unwrap();
        assert_eq!(
            (frame.left, frame.top, frame.width, frame.height),
            (expected.left, expected.top, expected.width, expected.height)
        );
        assert_eq!(frame.indices, expected.buffer.as_ref());
        let local: Option<Vec<u8>> = frame
            .local_palette
            .as_ref()
            .map(|p| p.iter().flatten().copied().collect());
        assert_eq!(local.as_deref(), expected.palette.as_deref());
        let c = frame.control.unwrap_or_default();
        // gif defaults an absent graphic control to Keep; native preserves its
        // absence. Explicit control extensions must match their encoded value.
        let disposal = frame.control.map_or(1, |c| c.disposal as u8);
        assert_eq!(
            (c.delay, c.transparent_index, disposal, c.user_input),
            (
                expected.delay,
                expected.transparent,
                expected.dispose as u8,
                expected.needs_user_input
            )
        );
        screen.blit_frame(expected).unwrap();
        let pixels = render.render(frame).unwrap();
        let expected_rgba = screen.pixels_rgba().to_contiguous_buf();
        assert!(
            pixels
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .zip(expected_rgba.0.iter())
                .all(|(a, b)| *a == [b.r, b.g, b.b, b.a])
        );
        assert_eq!(pixels.rgba.len(), expected_rgba.0.len() * 4);
        let image = image_frames.next().unwrap().unwrap();
        assert_eq!(pixels.rgba, image.buffer().as_raw());
        let (num, denom) = image.delay().numer_denom_ms();
        assert_eq!(u64::from(num), u64::from(c.delay) * 10 * u64::from(denom));
        stats.frames += 1;
        stats.total_pixels += frame.indices.len();
    }
    assert!(reader.read_next_frame().unwrap().is_none());
    assert!(image_frames.next().is_none());
    Ok(stats)
}
fn native_stream(bytes: &[u8], rgba: bool) {
    let mut decoder = native::Decoder::default();
    let mut at = 0;
    let mut render = None;
    loop {
        let p = decoder.update(&bytes[at..]).unwrap();
        at += p.consumed;
        match p.event {
            Some(native::Event::Header) => {
                black_box(decoder.header());
                if rgba {
                    render = Some(compositor(decoder.header().unwrap()));
                }
            }
            Some(native::Event::Block(native::Block::Frame(frame))) => {
                if rgba {
                    black_box(render.as_mut().unwrap().render(&frame).unwrap());
                } else {
                    black_box(&frame);
                }
                decoder.recycle_frame(frame);
            }
            Some(native::Event::Block(block)) => {
                black_box(block);
            }
            Some(native::Event::End) => {
                assert_eq!(at, bytes.len());
                decoder.finish().unwrap();
                break;
            }
            None => panic!("incomplete stream"),
        }
    }
}
fn decode(codec: usize, bytes: &[u8]) {
    match codec {
        0 => {
            black_box(native::decode(black_box(bytes)).unwrap());
        }
        1 => {
            let mut reader = indexed(black_box(bytes));
            let global = reader.global_palette().map(<[u8]>::to_vec);
            let mut frames = Vec::new();
            while let Some(frame) = reader.read_next_frame().unwrap() {
                frames.push(frame.clone());
            }
            black_box((global, frames));
        }
        2 => native_stream(black_box(bytes), false),
        3 => {
            let mut reader = indexed(black_box(bytes));
            while let Some(frame) = reader.read_next_frame().unwrap() {
                black_box(frame);
            }
        }
        4 => native_stream(black_box(bytes), true),
        5 => {
            let mut reader = indexed(black_box(bytes));
            let mut screen = gif_dispose::Screen::new_decoder(&reader);
            while let Some(frame) = reader.read_next_frame().unwrap() {
                screen.blit_frame(frame).unwrap();
                black_box(screen.pixels_rgba());
            }
        }
        6 => {
            let decoder =
                image::codecs::gif::GifDecoder::new(Cursor::new(black_box(bytes))).unwrap();
            for frame in decoder.into_frames() {
                black_box(frame.unwrap());
            }
        }
        _ => unreachable!(),
    }
}
fn files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for e in std::fs::read_dir(path).unwrap() {
            files(&e.unwrap().path(), out);
        }
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"))
    {
        out.push(path.to_owned());
    }
}
fn main() {
    let mut paths = Vec::new();
    for arg in std::env::args_os().skip(1) {
        files(Path::new(&arg), &mut paths);
    }
    paths.sort();
    assert!(!paths.is_empty(), "supply GIF files or corpus directories");
    cycles::pin();
    #[cfg(feature = "allocations")]
    {
        use std::{
            alloc::{GlobalAlloc, Layout},
            sync::atomic::Ordering::Relaxed,
        };
        allocations::ENABLED.store(true, Relaxed);
        // Exercise allocation, zeroed allocation, reallocation and freeing.
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
    let sample_ms = std::env::var("GIF_BENCH_MS").map_or(20, |v| v.parse::<u128>().unwrap());
    println!("file\tcodec\tns\tthread_cycles\talloc_calls\trequested_bytes");
    for (file_index, path) in paths.into_iter().enumerate() {
        let bytes = std::fs::read(&path).unwrap();
        let stats = match verify(&bytes) {
            Ok(stats) => stats,
            Err(error) => {
                eprintln!("EXCLUDED\t{}\t{error}", path.display());
                continue;
            }
        };
        eprintln!(
            "VERIFIED\t{}\t{}\t{}",
            path.display(),
            stats.frames,
            stats.total_pixels
        );
        #[cfg(not(feature = "allocations"))]
        let group_size = if stats.total_pixels > 4_000_000 { 1 } else { 8 };
        for offset in 0..CODECS.len() {
            let codec = (offset + file_index) % CODECS.len();
            decode(codec, &bytes);
            #[cfg(feature = "allocations")]
            {
                use std::sync::atomic::Ordering::Relaxed;
                allocations::CALLS.store(0, Relaxed);
                allocations::BYTES.store(0, Relaxed);
                allocations::ENABLED.store(true, Relaxed);
                decode(codec, &bytes);
                allocations::ENABLED.store(false, Relaxed);
                println!(
                    "{}\t{}\t0\t0\t{}\t{}",
                    path.display(),
                    CODECS[codec],
                    allocations::CALLS.load(Relaxed),
                    allocations::BYTES.load(Relaxed)
                );
            }
            #[cfg(not(feature = "allocations"))]
            {
                let mut timings = Vec::new();
                for _ in 0..5 {
                    let start_cycles = cycles::read();
                    let start = Instant::now();
                    let mut iterations = 0;
                    loop {
                        for _ in 0..group_size {
                            decode(codec, &bytes);
                        }
                        iterations += group_size;
                        if start.elapsed().as_millis() >= sample_ms {
                            break;
                        }
                    }
                    let elapsed = start.elapsed().as_nanos() as u64 / iterations;
                    let cycles = (cycles::read() - start_cycles) / iterations;
                    timings.push((elapsed, cycles));
                }
                let mut wall: Vec<_> = timings.iter().map(|&(ns, _)| ns).collect();
                let mut cpu: Vec<_> = timings.iter().map(|&(_, cycles)| cycles).collect();
                wall.sort_unstable();
                cpu.sort_unstable();
                let (ns, cycles) = (wall[2], cpu[2]);
                println!(
                    "{}\t{}\t{ns}\t{cycles}\t0\t0",
                    path.display(),
                    CODECS[codec]
                );
            }
        }
    }
}
