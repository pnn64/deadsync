use deadlib_video_h264::{nal::parse_annex_b, ChromaFormat, Frame, OrderedDecoder};
use std::{env, ffi::c_void, hint::black_box, io::Write, path::PathBuf};

#[repr(C)]
#[derive(Default, Debug, PartialEq, Eq)]
struct Output {
    frames: u64,
    luma_pixels: u64,
}

#[cfg(allocations)]
#[repr(C)]
#[derive(Default)]
struct Counts {
    allocations: u64,
    reallocations: u64,
    requested_bytes: u64,
}

unsafe extern "C" {
    fn ff_decode(data: *const u8, size: i32, mode: u32, sha: *mut c_void, out: *mut Output) -> i32;
    fn av_version_info() -> *const std::ffi::c_char;
    fn avcodec_version() -> u32;
    #[cfg(not(allocations))]
    fn av_sha_alloc() -> *mut c_void;
    #[cfg(not(allocations))]
    fn av_sha_init(sha: *mut c_void, bits: i32) -> i32;
    fn av_sha_update(sha: *mut c_void, data: *const u8, len: usize);
    #[cfg(not(allocations))]
    fn av_sha_final(sha: *mut c_void, out: *mut u8);
    fn av_free(ptr: *mut c_void);
    #[cfg(allocations)]
    fn ff_alloc_install() -> i32;
    #[cfg(allocations)]
    fn ff_alloc_start();
    #[cfg(allocations)]
    fn ff_alloc_stop(out: *mut Counts);
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    #[cfg(not(allocations))]
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
}

#[cfg(allocations)]
mod rust_alloc {
    use super::Counts;
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    };

    #[global_allocator]
    static ALLOCATOR: Meter = Meter;
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static ALLOCS: AtomicU64 = AtomicU64::new(0);
    static REALLOCS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);

    struct Meter;

    fn count(ptr: *mut u8, size: usize, events: &AtomicU64) {
        if ENABLED.load(Relaxed) && !ptr.is_null() && size != 0 {
            events.fetch_add(1, Relaxed);
            BYTES.fetch_add(size as u64, Relaxed);
        }
    }

    unsafe impl GlobalAlloc for Meter {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { System.alloc(layout) };
            count(ptr, layout.size(), &ALLOCS);
            ptr
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { System.alloc_zeroed(layout) };
            count(ptr, layout.size(), &ALLOCS);
            ptr
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) };
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let new_ptr = unsafe { System.realloc(ptr, layout, size) };
            count(new_ptr, size, &REALLOCS);
            new_ptr
        }
    }

    pub fn start() {
        ALLOCS.store(0, Relaxed);
        REALLOCS.store(0, Relaxed);
        BYTES.store(0, Relaxed);
        ENABLED.store(true, Relaxed);
    }

    pub fn stop() -> Counts {
        ENABLED.store(false, Relaxed);
        Counts {
            allocations: ALLOCS.load(Relaxed),
            reallocations: REALLOCS.load(Relaxed),
            requested_bytes: BYTES.load(Relaxed),
        }
    }
}

struct Sha(*mut c_void);

#[cfg(not(allocations))]
impl Sha {
    fn new() -> Self {
        let state = unsafe { av_sha_alloc() };
        assert!(!state.is_null());
        assert_eq!(unsafe { av_sha_init(state, 256) }, 0);
        Self(state)
    }

    fn finish(self) -> String {
        let mut digest = [0u8; 32];
        unsafe { av_sha_final(self.0, digest.as_mut_ptr()) };
        digest.into_iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl Drop for Sha {
    fn drop(&mut self) {
        unsafe { av_free(self.0) };
    }
}

fn rust_decode(bytes: &[u8], sha: Option<&mut Sha>, reuse: bool) -> Output {
    let mut decoder = OrderedDecoder::new();
    let mut out = Output::default();
    let mut consume = |frame: &Frame| {
        assert_eq!(frame.chroma_format, ChromaFormat::Yuv420);
        assert_eq!(frame.bit_depth, 8);
        out.frames += 1;
        out.luma_pixels += u64::from(frame.width) * u64::from(frame.height);
        for plane in [&frame.y, &frame.u, &frame.v] {
            if let Some(sha) = sha.as_ref() {
                unsafe { av_sha_update(sha.0, plane.as_ptr(), plane.len()) };
            } else {
                black_box(plane.as_slice());
            }
        }
    };
    for nal in parse_annex_b(bytes) {
        for frame in decoder.decode_nal(&nal).unwrap() {
            consume(&frame);
            if reuse {
                decoder.recycle_frame(frame);
            }
        }
    }
    for frame in decoder.flush() {
        consume(&frame);
        if reuse {
            decoder.recycle_frame(frame);
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Mode {
    Rust,
    RustRecycled,
    FfmpegSurface,
    FfmpegPacked,
}

impl Mode {
    const ALL: [Self; 4] = [
        Self::Rust,
        Self::RustRecycled,
        Self::FfmpegSurface,
        Self::FfmpegPacked,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Rust => "deadlib",
            Self::RustRecycled => "deadlib-recycled",
            Self::FfmpegSurface => "ffmpeg-surface",
            Self::FfmpegPacked => "ffmpeg-packed",
        }
    }

    fn decode(self, padded: &[u8], sha: Option<&mut Sha>) -> Output {
        let size = padded.len() - 64;
        if matches!(self, Self::Rust | Self::RustRecycled) {
            return rust_decode(&padded[..size], sha, matches!(self, Self::RustRecycled));
        }
        let (mode, ptr) = match sha {
            Some(sha) => (2, sha.0),
            None => (
                u32::from(matches!(self, Self::FfmpegPacked)),
                std::ptr::null_mut(),
            ),
        };
        let mut out = Output::default();
        let rc = unsafe {
            ff_decode(
                padded.as_ptr(),
                size.try_into().unwrap(),
                mode,
                ptr,
                &mut out,
            )
        };
        assert_eq!(rc, 0, "libavcodec decode failed");
        out
    }
}

#[cfg(not(allocations))]
fn cycles() -> u64 {
    let mut value = 0;
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
        0
    );
    value
}

fn main() {
    let mut args = env::args_os().skip(1);
    let runs: usize = args.next().unwrap().to_str().unwrap().parse().unwrap();
    let cpu: u32 = args.next().unwrap().to_str().unwrap().parse().unwrap();
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    assert!(!paths.is_empty());
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << cpu) },
        0
    );
    let version = unsafe { std::ffi::CStr::from_ptr(av_version_info()) };
    let codec = unsafe { avcodec_version() };
    eprintln!(
        "FFmpeg {}, libavcodec {}.{}.{}, logical CPU {cpu}",
        version.to_string_lossy(),
        codec >> 16,
        (codec >> 8) & 255,
        codec & 255
    );

    #[cfg(allocations)]
    assert_eq!(
        unsafe { ff_alloc_install() },
        1,
        "FFmpeg allocator hooks failed calibration"
    );

    println!("stream,mode,run,frames,luma_pixels,cycles,allocations,reallocations,requested_bytes");
    for (index, path) in paths.iter().enumerate() {
        let mut bytes = std::fs::read(path).unwrap();
        let size = bytes.len();
        bytes.resize(size + 64, 0);
        let label = path.file_stem().unwrap().to_str().unwrap();

        #[cfg(not(allocations))]
        {
            let mut ff_sha = Sha::new();
            let ff = Mode::FfmpegSurface.decode(&bytes, Some(&mut ff_sha));
            let hash = ff_sha.finish();
            for mode in [Mode::Rust, Mode::RustRecycled] {
                let mut rust_sha = Sha::new();
                let rust = mode.decode(&bytes, Some(&mut rust_sha));
                assert_eq!(rust, ff);
                assert_eq!(rust_sha.finish(), hash, "{} output mismatch", mode.name());
            }
            eprintln!("{label}: {} frames, SHA-256 {hash}", ff.frames);
        }

        if runs == 0 {
            continue;
        }

        let expected = Mode::FfmpegSurface.decode(&bytes, None);
        for mode in Mode::ALL {
            assert_eq!(mode.decode(&bytes, None), expected);
        }

        for run in 0..runs {
            for i in 0..Mode::ALL.len() {
                let mode = Mode::ALL[(i + run + index) % Mode::ALL.len()];
                #[cfg(not(allocations))]
                let (result, cycle_count, counts) = {
                    let start = cycles();
                    let result = mode.decode(black_box(&bytes), None);
                    (result, cycles() - start, (0, 0, 0))
                };
                #[cfg(allocations)]
                let (result, cycle_count, counts) = {
                    rust_alloc::start();
                    unsafe { ff_alloc_start() };
                    let result = mode.decode(black_box(&bytes), None);
                    let mut counts = Counts::default();
                    unsafe { ff_alloc_stop(&mut counts) };
                    let rust = rust_alloc::stop();
                    let counts = (
                        counts.allocations + rust.allocations,
                        counts.reallocations + rust.reallocations,
                        counts.requested_bytes + rust.requested_bytes,
                    );
                    (result, 0, counts)
                };
                assert_eq!(result, expected);
                println!(
                    "{label},{},{},{},{},{cycle_count},{},{},{}",
                    mode.name(),
                    run + 1,
                    result.frames,
                    result.luma_pixels,
                    counts.0,
                    counts.1,
                    counts.2
                );
                std::io::stdout().flush().unwrap();
            }
        }
    }
}
