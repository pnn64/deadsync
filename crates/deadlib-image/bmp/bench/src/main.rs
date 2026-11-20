use deadlib_image_bmp::{Bitmap, Pixels};
#[cfg(not(feature = "allocations"))]
use std::time::Instant;
use std::{hint::black_box, path::Path};

mod fixtures;

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
    // Measurement-only forwarding: pointers, layouts and ownership are passed
    // unchanged to System. Counters are disabled during setup and printing.
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
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(size, Relaxed);
            }
            unsafe { System.realloc(pointer, layout, size) }
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }
    }
    pub fn measure(operation: impl FnOnce()) -> (usize, usize) {
        CALLS.store(0, Relaxed);
        BYTES.store(0, Relaxed);
        ENABLED.store(true, Relaxed);
        operation();
        ENABLED.store(false, Relaxed);
        (CALLS.load(Relaxed), BYTES.load(Relaxed))
    }
    pub fn calibrate() {
        let (calls, bytes) = measure(|| {
            // Layouts match every allocation/reallocation and every pointer is
            // checked before the corresponding System operation.
            unsafe {
                let a = super::ALLOCATOR.alloc(Layout::from_size_align(31, 8).unwrap());
                assert!(!a.is_null());
                let a = super::ALLOCATOR.realloc(a, Layout::from_size_align(31, 8).unwrap(), 96);
                assert!(!a.is_null());
                let b = super::ALLOCATOR.alloc_zeroed(Layout::from_size_align(17, 8).unwrap());
                assert!(!b.is_null());
                std::hint::black_box((a, b));
                super::ALLOCATOR.dealloc(a, Layout::from_size_align(96, 8).unwrap());
                super::ALLOCATOR.dealloc(b, Layout::from_size_align(17, 8).unwrap());
            }
        });
        assert_eq!((calls, bytes), (3, 144));
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
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
        #[cfg(not(feature = "allocations"))]
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }
    pub fn pin() {
        let cpu = std::env::var("BMP_BENCH_CPU").map_or(0, |v| v.parse::<u32>().unwrap());
        let mask = 1usize
            .checked_shl(cpu)
            .expect("CPU must fit the processor group mask");
        // Current-thread pseudo handle is valid; a nonzero affinity mask is used.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) },
            0
        );
        eprintln!("Pinned benchmark thread to logical CPU {cpu}");
    }
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        let mut value = 0;
        // The writable counter outlives this synchronous FFI call.
        assert_ne!(
            unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
            0
        );
        value
    }
}
#[cfg(not(windows))]
mod cycles {
    pub fn pin() {
        eprintln!("Thread cycles unavailable on this target");
    }
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        0
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Decode,
    Rgba,
    Convert,
    Encode,
    ImageRgba,
    ImageEncode,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Decode => "native-decode",
            Self::Rgba => "native-rgba",
            Self::Convert => "native-convert",
            Self::Encode => "native-encode",
            Self::ImageRgba => "image-rgba",
            Self::ImageEncode => "image-encode",
        }
    }
}

fn image_rgba(bytes: &[u8]) -> image::ImageResult<image::RgbaImage> {
    Ok(image::load_from_memory_with_format(bytes, image::ImageFormat::Bmp)?.into_rgba8())
}

fn image_encode(bitmap: &Bitmap) -> Vec<u8> {
    let (samples, color) = match &bitmap.pixels {
        Pixels::Rgb(samples) => (samples, image::ExtendedColorType::Rgb8),
        Pixels::Rgba(samples) => (samples, image::ExtendedColorType::Rgba8),
        Pixels::Indexed { .. } => unreachable!("indexed encoding has a different output contract"),
    };
    let mut output = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut output)
        .encode(samples, bitmap.width, bitmap.height, color)
        .unwrap();
    output
}

fn run(mode: Mode, case: &fixtures::Case) {
    match mode {
        Mode::Decode => {
            black_box(deadlib_image_bmp::decode(black_box(&case.bytes)).unwrap());
        }
        Mode::Rgba => {
            black_box(
                deadlib_image_bmp::decode(black_box(&case.bytes))
                    .unwrap()
                    .to_rgba8()
                    .unwrap(),
            );
        }
        Mode::Convert => {
            black_box(black_box(&case.expected).to_rgba8().unwrap());
        }
        Mode::Encode => {
            black_box(deadlib_image_bmp::encode(black_box(&case.expected)).unwrap());
        }
        Mode::ImageRgba => {
            black_box(image_rgba(black_box(&case.bytes)).unwrap());
        }
        Mode::ImageEncode => {
            black_box(image_encode(black_box(&case.expected)));
        }
    }
}

fn verify(case: &fixtures::Case) -> bool {
    let actual = deadlib_image_bmp::decode(&case.bytes).unwrap();
    assert!(
        actual == case.expected,
        "{}: native representation differs from independent expected pixels",
        case.name
    );
    assert!(
        actual.to_rgba8().unwrap() == case.rgba,
        "{}: native RGBA differs from independent expected pixels",
        case.name
    );
    let encoded = deadlib_image_bmp::encode(&case.expected).unwrap();
    let decoded = deadlib_image_bmp::decode(&encoded).unwrap();
    assert!(
        decoded.to_rgba8().unwrap() == case.rgba,
        "{}: native encode pixel mismatch",
        case.name
    );
    match image_rgba(&case.bytes) {
        Ok(reference)
            if reference.dimensions() == (case.expected.width, case.expected.height)
                && reference.as_raw() == &case.rgba =>
        {
            true
        }
        Ok(reference) => {
            assert_eq!(
                reference.dimensions(),
                (case.expected.width, case.expected.height)
            );
            assert_eq!(
                reference.len(),
                case.rgba.len(),
                "{}: reference length",
                case.name
            );
            assert_ne!(
                case.policy, "none",
                "{}: unexpected reference difference",
                case.name
            );
            let differing = reference
                .iter()
                .zip(&case.rgba)
                .filter(|(a, b)| a != b)
                .count();
            let max_difference = reference
                .iter()
                .zip(&case.rgba)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap_or(0);
            eprintln!(
                "NON_EQUIVALENT\t{}\t{}\t{differing} differing samples\tmax {max_difference}",
                case.name, case.policy
            );
            false
        }
        Err(error) => {
            eprintln!("REFERENCE_UNAVAILABLE\t{}\t{error}", case.name);
            false
        }
    }
}

struct Options {
    filter: String,
    mode: String,
    verify_only: bool,
    files: Vec<String>,
}
fn options() -> Options {
    let mut result = Options {
        filter: String::new(),
        mode: "all".to_owned(),
        verify_only: false,
        files: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--filter" => result.filter = args.next().expect("--filter needs a substring"),
            "--mode" => {
                result.mode = args
                    .next()
                    .expect("--mode needs all/rgba/decode/convert/encode")
            }
            "--verify-only" => result.verify_only = true,
            "--help" => {
                println!(
                    "bmp-codec-bench [--filter SUBSTRING] [--mode all|rgba|decode|convert|encode] [--verify-only] [FILE_OR_DIRECTORY ...]\nWithout paths: independent synthetic layout/size/orientation matrix. BMP_BENCH_MS=20 BMP_BENCH_SAMPLES=5 BMP_BENCH_CPU=0. The allocations feature records counts only."
                );
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => panic!("unknown option: {arg}"),
            _ => result.files.push(arg),
        }
    }
    assert!(
        ["all", "rgba", "decode", "convert", "encode"].contains(&result.mode.as_str()),
        "unknown mode: {}",
        result.mode
    );
    result
}

fn modes(options: &Options, case: &fixtures::Case, equivalent: bool) -> Vec<Mode> {
    let mut result = Vec::new();
    for (selected, mode) in [
        ("decode", Mode::Decode),
        ("rgba", Mode::Rgba),
        ("convert", Mode::Convert),
        ("encode", Mode::Encode),
    ] {
        if options.mode == "all" || options.mode == selected {
            result.push(mode);
        }
    }
    if equivalent && (options.mode == "all" || options.mode == "rgba") {
        result.push(Mode::ImageRgba);
    }
    if matches!(case.expected.pixels, Pixels::Rgb(_) | Pixels::Rgba(_))
        && (options.mode == "all" || options.mode == "encode")
    {
        let encoded = image_encode(&case.expected);
        // Different file headers are allowed for encoding comparisons, but every
        // decoded pixel must match, including explicit fully transparent alpha.
        if deadlib_image_bmp::decode(&encoded)
            .is_ok_and(|bmp| bmp.to_rgba8().is_ok_and(|v| v == case.rgba))
        {
            result.push(Mode::ImageEncode);
        } else {
            eprintln!("NON_EQUIVALENT\t{}\timage-encode", case.name);
        }
    }
    result
}

fn measure(options: &Options, case: &fixtures::Case, case_index: usize) {
    let equivalent = verify(case);
    if options.verify_only {
        return;
    }
    let modes = modes(options, case, equivalent);
    for &mode in &modes {
        for _ in 0..3 {
            run(mode, case);
        }
    }
    let pixels = u64::from(case.expected.width) * u64::from(case.expected.height);
    #[cfg(feature = "allocations")]
    {
        let _ = case_index;
        for mode in modes {
            let (calls, bytes) = allocations::measure(|| run(mode, case));
            println!(
                "{}\t{}\t{pixels}\t{equivalent}\t0\t0\t0\t0\t0\t{calls}\t{bytes}",
                case.name,
                mode.name()
            );
        }
    }
    #[cfg(not(feature = "allocations"))]
    {
        let milliseconds = std::env::var("BMP_BENCH_MS").map_or(20, |v| v.parse::<u128>().unwrap());
        let samples = std::env::var("BMP_BENCH_SAMPLES").map_or(5, |v| v.parse::<usize>().unwrap());
        assert!(milliseconds > 0 && samples > 0);
        let mut readings = vec![Vec::with_capacity(samples); modes.len()];
        for sample in 0..samples {
            for offset in 0..modes.len() {
                let index = (offset + sample + case_index) % modes.len();
                let start_cycles = cycles::read();
                let start = Instant::now();
                let mut iterations = 0u64;
                let group = if pixels < 1_000_000 { 8 } else { 1 };
                loop {
                    for _ in 0..group {
                        run(modes[index], case);
                    }
                    iterations += group;
                    if start.elapsed().as_millis() >= milliseconds {
                        break;
                    }
                }
                let elapsed = start.elapsed().as_nanos() as f64 / iterations as f64;
                let cycles = (cycles::read() - start_cycles) as f64 / iterations as f64;
                readings[index].push((elapsed, cycles));
            }
        }
        for (mode, readings) in modes.into_iter().zip(readings) {
            let mut ns: Vec<_> = readings.iter().map(|x| x.0).collect();
            let mut cycles: Vec<_> = readings.iter().map(|x| x.1).collect();
            ns.sort_unstable_by(f64::total_cmp);
            cycles.sort_unstable_by(f64::total_cmp);
            let median = ns[samples / 2];
            println!(
                "{}\t{}\t{pixels}\t{equivalent}\t{median:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\t0\t0",
                case.name,
                mode.name(),
                cycles[samples / 2],
                cycles[0],
                cycles[samples - 1],
                pixels as f64 * 1000.0 / median
            );
        }
    }
}

fn files(path: &Path, output: &mut Vec<std::path::PathBuf>, explicit: bool) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            files(&entry.unwrap().path(), output, false);
        }
    } else if explicit
        || path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bmp"))
    {
        output.push(path.to_owned());
    }
}

fn main() {
    let options = options();
    if !options.verify_only {
        cycles::pin();
    }
    #[cfg(feature = "allocations")]
    allocations::calibrate();
    println!(
        "case\tmode\tpixels\timage_equivalent\tns\tthread_cycles\tcycles_min\tcycles_max\tmpixels_sec\talloc_calls\trequested_bytes"
    );
    let mut count = 0;
    if options.files.is_empty() {
        for &layout in fixtures::LAYOUTS {
            for (width, height) in [(1, 1), (17, 9), (511, 257), (1920, 1080)] {
                for top_down in [false, true] {
                    if top_down && !layout.supports_top_down() {
                        continue;
                    }
                    let name = format!(
                        "{}-{}x{}-{}",
                        layout.name,
                        width,
                        height,
                        if top_down { "top" } else { "bottom" }
                    );
                    if !name.contains(&options.filter) {
                        continue;
                    }
                    let case = fixtures::build(layout, width, height, top_down);
                    measure(&options, &case, count);
                    count += 1;
                }
            }
        }
    } else {
        let mut paths = Vec::new();
        for path in &options.files {
            files(Path::new(path), &mut paths, true);
        }
        paths.sort();
        paths.dedup();
        for path in paths {
            let name = path.to_string_lossy().into_owned();
            if !name.contains(&options.filter) {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let expected = match deadlib_image_bmp::decode(&bytes) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    eprintln!("NATIVE_UNAVAILABLE\t{name}\t{error}");
                    continue;
                }
            };
            let rgba = expected.to_rgba8().unwrap();
            let case = fixtures::Case {
                name,
                bytes,
                expected,
                rgba,
                policy: "external-corpus-policy",
            };
            measure(&options, &case, count);
            count += 1;
        }
    }
    assert!(count > 0, "no matching BMP cases");
    eprintln!(
        "Verified {count} BMP cases; synthetic expected pixels are generated independently of both codecs."
    );
}
