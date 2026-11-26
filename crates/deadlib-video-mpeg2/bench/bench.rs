#[cfg(not(mpeg4))]
use deadlib_video_mpeg2::decode_elementary_stream;
#[cfg(mpeg4)]
use deadlib_video_mpeg4::decode_elementary_stream;
use std::{env, ffi::c_void, hint::black_box, io::Write, path::Path};

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

#[derive(Clone, Copy)]
enum Engine {
    Rust,
    Ffmpeg,
}

impl Engine {
    fn name(self) -> &'static str {
        match self {
            Self::Rust => "deadlib",
            Self::Ffmpeg => "ffmpeg",
        }
    }

    fn decode(self, bytes: &[u8], length: usize, sha: Option<&Sha>) -> Output {
        let mut out = Output::default();
        match self {
            Self::Rust => decode_elementary_stream(&bytes[..length], |frame| {
                out.frames += 1;
                out.luma_pixels += (frame.width() * frame.height()) as u64;
                if let Some(sha) = sha {
                    for plane in frame.planes() {
                        for row in plane.rows() {
                            unsafe { av_sha_update(sha.0, row.as_ptr(), row.len()) };
                        }
                    }
                } else {
                    black_box(frame.planes()[0].rows().next().unwrap()[0]);
                }
            })
            .unwrap(),
            Self::Ffmpeg => {
                let mode = if sha.is_some() { 2 } else { 0 };
                let sha_ptr = sha.map_or(std::ptr::null_mut(), |sha| sha.0);
                let result = unsafe {
                    ff_decode(
                        bytes.as_ptr(),
                        i32::try_from(length).unwrap(),
                        mode,
                        sha_ptr,
                        &mut out,
                    )
                };
                assert_eq!(result, 0);
            }
        }
        out
    }
}

#[cfg(not(allocations))]
fn read_cycles() -> u64 {
    let mut result = 0;
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut result) },
        0
    );
    result
}

#[cfg(allocations)]
fn count(engine: Engine, bytes: &[u8], length: usize) -> (Output, Counts) {
    match engine {
        Engine::Rust => {
            rust_alloc::start();
            let out = engine.decode(bytes, length, None);
            (out, rust_alloc::stop())
        }
        Engine::Ffmpeg => {
            let mut counts = Counts::default();
            unsafe { ff_alloc_start() };
            let out = engine.decode(bytes, length, None);
            unsafe { ff_alloc_stop(&mut counts) };
            (out, counts)
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let rounds: usize = args.next().unwrap().parse().unwrap();
    let cpu: u32 = args.next().unwrap().parse().unwrap();
    let mask = 1usize.checked_shl(cpu).unwrap();
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) },
        0
    );

    let version = unsafe { std::ffi::CStr::from_ptr(av_version_info()) };
    let codec_version = unsafe { avcodec_version() };
    eprintln!(
        "FFmpeg {} libavcodec {}.{}.{} CPU {cpu}",
        version.to_string_lossy(),
        codec_version >> 16,
        (codec_version >> 8) & 255,
        codec_version & 255
    );
    #[cfg(allocations)]
    assert_eq!(
        unsafe { ff_alloc_install() },
        1,
        "FFmpeg heap counter calibration"
    );
    #[cfg(allocations)]
    println!("clip,round,engine,frames,luma_pixels,input_bytes,allocations,reallocations,requested_bytes");
    #[cfg(not(allocations))]
    println!("clip,round,engine,frames,luma_pixels,input_bytes,cycles");

    for (index, file) in args.enumerate() {
        let clip = Path::new(&file).file_stem().unwrap().to_str().unwrap();
        let mut bytes = std::fs::read(&file).unwrap();
        let length = bytes.len();
        bytes.resize(length + 64, 0); // AV_INPUT_BUFFER_PADDING_SIZE.
        #[cfg(not(allocations))]
        {
            let rust_sha = Sha::new();
            let ffmpeg_sha = Sha::new();
            let left = Engine::Rust.decode(&bytes, length, Some(&rust_sha));
            let right = Engine::Ffmpeg.decode(&bytes, length, Some(&ffmpeg_sha));
            assert_eq!(left, right);
            let digest = rust_sha.finish();
            assert_eq!(
                digest,
                ffmpeg_sha.finish(),
                "{clip} must match before measuring"
            );
            eprintln!(
                "{clip}: {} frames, verified YUV SHA-256 {digest}",
                left.frames
            );
        }

        let expected = Engine::Rust.decode(&bytes, length, None);
        assert_eq!(expected, Engine::Ffmpeg.decode(&bytes, length, None));
        assert!(expected.frames > 0);
        for round in 0..rounds {
            let order = if (index + round) % 2 == 0 {
                [Engine::Rust, Engine::Ffmpeg]
            } else {
                [Engine::Ffmpeg, Engine::Rust]
            };
            for engine in order {
                let label = engine.name();
                #[cfg(not(allocations))]
                {
                    let start = read_cycles();
                    let result = engine.decode(&bytes, length, None);
                    let cycles = read_cycles() - start;
                    assert_eq!(result, expected);
                    println!(
                        "{clip},{round},{label},{},{},{length},{cycles}",
                        result.frames, result.luma_pixels
                    );
                }
                #[cfg(allocations)]
                {
                    let (result, counts) = count(engine, &bytes, length);
                    assert_eq!(result, expected);
                    println!(
                        "{clip},{round},{label},{},{},{length},{},{},{}",
                        result.frames,
                        result.luma_pixels,
                        counts.allocations,
                        counts.reallocations,
                        counts.requested_bytes
                    );
                }
                std::io::stdout().flush().unwrap();
            }
        }
    }
}
